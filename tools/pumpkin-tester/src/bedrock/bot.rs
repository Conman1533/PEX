//! A headless Bedrock client: joins over `NetherNet` with a self-signed login, the way a 1.26.60
//! client joins an offline-mode server by IP, and keeps the state the tests look at.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Write as _,
    io::{Cursor, Read, Write as _},
    net::SocketAddr,
    time::Duration,
};

use base64::{Engine, engine::general_purpose};
use pumpkin_auth::p384::{
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::EncodePublicKey,
};
use pumpkin_core::net::bedrock::level_chunk::CLevelChunk;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::{
    RawPacket,
    bedrock::{
        SubClient,
        client::{
            add_actor::CAddActor,
            add_player::CAddPlayer,
            disconnect::CDisconnect,
            move_player::CMovePlayer,
            network_settings::CNetworkSettings,
            play_status::CPlayStatus,
            remove_actor::CRemoveActor,
            resource_pack_stack::CResourcePackStackPacket,
            resource_packs_info::CResourcePacksInfo,
            set_actor_data::{CSetActorData, entity_data_key},
            set_actor_link::CSetActorLink,
            set_health::CSetHealth,
            start_game::CStartGame,
            update_attributes::CUpdateAttributes,
            update_block::CUpdateBlock,
        },
        network_item::{ContainerName, FullContainerName, NetworkItemDescriptor},
        packet_decoder::BedrockBatchDecoder,
        packet_encoder::BedrockBatchEncoder,
        server::{
            command_request::SCommandRequest, item_stack_request::ItemStackRequestSlotInfo,
            login::SLogin, player_auth_input::SPlayerAuthInput,
            request_chunk_radius::SRequestChunkRadius,
            request_network_settings::SRequestNetworkSettings,
            resource_pack_client_response::SResourcePackClientResponse,
            set_local_player_as_initialized::SSetLocalPlayerAsInitialized, text::SText,
        },
    },
    codec::{var_int::VarInt, var_long::VarLong, var_uint::VarUInt, var_ulong::VarULong},
    packet::Packet,
    serial::{PacketRead, PacketWrite},
};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_world::CURRENT_BEDROCK_MC_PROTOCOL;
use serde_json::{Value, json};
use tokio::time::timeout;

use super::transport::{NetherNetClient, http_request};

const PACKET_TIMEOUT: Duration = Duration::from_secs(20);
/// `CPlayStatus::LoginSuccess` and the other play statuses as they are on the wire.
const PLAY_STATUS_LOGIN_SUCCESS: i32 = 0;
const PLAY_STATUS_PLAYER_SPAWN: i32 = 3;
/// `AuthenticationType::SelfSigned` in `net/bedrock/login/mod.rs`.
const AUTHENTICATION_SELF_SIGNED: u8 = 2;
/// `InputData` flags the bot sets in `PlayerAuthInput`.
const INPUT_PERFORM_BLOCK_ACTIONS: i32 = 35;
/// `InputData::ClientPredictedVehicle`: the client moves the vehicle it rides.
const INPUT_CLIENT_PREDICTED_VEHICLE: i32 = 45;
/// `InputData::VerticalCollision`: the player is standing on something.
pub const INPUT_VERTICAL_COLLISION: i32 = 50;
/// The vertical motion a Bedrock client on the ground sends: one tick of gravity, 0.08 * 0.98.
const GROUND_GRAVITY_DELTA: f32 = -0.078_4;
/// `InputMode::Mouse`, `PlayMode::Normal` and `InteractionModel::Crosshair`.
const INPUT_MODE_MOUSE: u32 = 1;
const PLAY_MODE_NORMAL: u32 = 0;
const INTERACTION_MODEL_CROSSHAIR: i32 = 1;
/// `PlayerActionType::StartDestroyBlock`, which breaks the block at once in creative.
pub const ACTION_START_BREAK: i32 = 0;
/// `PlayerActionType::PredictDestroyBlock`: the client thinks its timed break is done.
pub const ACTION_PREDICT_BREAK: i32 = 26;
/// `PlayerAuthInput`, for tests that send it raw.
pub const PLAYER_AUTH_INPUT_ID: i32 = SPlayerAuthInput::PACKET_ID;
/// `ActorLinkType`s.
const LINK_REMOVE: u8 = 0;
pub const LINK_RIDER: u8 = 1;
pub const LINK_PASSENGER: u8 = 2;
/// `Interact`, and its `StopRiding` action.
const INTERACT_ID: i32 = 33;
/// `InventoryTransaction`.
const INVENTORY_TRANSACTION_ID: i32 = 30;
const CONTAINER_OPEN_ID: i32 = 46;
const CONTAINER_CLOSE_ID: i32 = 47;
const ITEM_STACK_REQUEST_ID: i32 = 147;
const ITEM_STACK_RESPONSE_ID: i32 = 148;
const SET_TITLE_ID: i32 = 88;
/// `ItemStackRequestAction::CraftRecipe`.
const STACK_ACTION_CRAFT_RECIPE: u32 = 10;

/// Which `ItemStackRequest` action a `StackMove` is.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum StackMoveKind {
    Take = 0,
    Place = 1,
}

/// One take or place in an `ItemStackRequest`: `count` items from `from` to `to`, each a
/// container and slot.
#[derive(Clone, Copy)]
pub struct StackMove {
    pub kind: StackMoveKind,
    pub count: u8,
    pub from: (ContainerName, u8),
    pub to: (ContainerName, u8),
}
/// `UseItem` transaction actions.
const USE_ITEM_CLICK_BLOCK: i32 = 0;
const USE_ITEM_CLICK_AIR: i32 = 1;
const INTERACT_STOP_RIDING: u8 = 3;
/// `TextPacketType::Chat`.
const TEXT_TYPE_CHAT: u8 = 1;

/// A player the server announced with `AddPlayer`.
#[derive(Clone, Debug)]
pub struct SeenPlayer {
    pub name: String,
    pub position: Vector3<f32>,
}

/// A chat or system line the server sent with `Text`.
#[derive(Clone, Debug)]
pub struct SeenText {
    pub message: String,
    pub parameters: Vec<String>,
}

pub struct BedrockBot {
    pub name: String,
    /// The Xbox user id the bot claims, which the server turns into its UUID.
    pub xuid: u64,
    transport: NetherNetClient,
    encoder: BedrockBatchEncoder,
    decoder: BedrockBatchDecoder,
    compressed: bool,
    pub runtime_id: u64,
    pub unique_id: i64,
    pub position: Vector3<f32>,
    pub yaw: f32,
    pub pitch: f32,
    tick: u64,
    /// Chunks received with `LevelChunk`, by chunk coordinates.
    pub chunks: HashSet<(i32, i32)>,
    /// Players announced with `AddPlayer` and not removed, by runtime id.
    pub players: HashMap<u64, SeenPlayer>,
    /// Actor identifiers announced with `AddActor` and not removed, by unique id.
    pub actors: HashMap<i64, String>,
    /// Runtime ids of the actors in [`Self::actors`], to map `RemoveActor` back.
    actor_runtime_ids: HashMap<i64, u64>,
    /// The two actor flag words (flags 0..63 and 64..127) from `SetActorData`, by runtime id.
    pub actor_flags: HashMap<u64, [i64; 2]>,
    /// Every 64-bit actor data value from `SetActorData`, by runtime id and key.
    pub actor_longs: HashMap<(u64, u32), i64>,
    /// Every `SetActorLink` as (ridden, rider, link type), oldest first.
    pub links: Vec<(i64, i64, u8)>,
    /// While set, inputs say the client drives this vehicle (unique id) with this rotation
    /// (pitch, yaw), as `ClientPredictedVehicle`.
    pub driving: Option<(i64, Vector2<f32>)>,
    /// Every `Text` the server sent.
    pub texts: Vec<SeenText>,
    /// Every layer-0 `UpdateBlock`, as position and block network id.
    pub block_updates: Vec<(BlockPos, u32)>,
    /// The container the server has open for the bot: window id and container type, from
    /// `ContainerOpen` until a `ContainerClose`.
    pub open_container: Option<(u8, u8)>,
    /// Every `ItemStackResponse` as (request id, result), where result 0 is success.
    pub stack_responses: Vec<(i32, u8)>,
    /// Every `SetTitle` as (title type, text): 0 clear, 2 title, 3 subtitle, 5 times.
    pub titles: Vec<(i32, String)>,
    pub health: i32,
    /// Whether inputs report standing on something, as a real client does when it isn't
    /// falling or jumping.
    pub on_ground: bool,
    pub disconnect: Option<String>,
    /// How many of each packet id arrived.
    pub packet_counts: HashMap<i32, usize>,
    /// Packets of a batch not handled yet.
    pending: VecDeque<RawPacket>,
    /// Where every received packet is written for `tools/bedrock-wirecheck`, when
    /// `PUMPKIN_TESTER_BEDROCK_DUMP` names a directory.
    dump: Option<std::fs::File>,
}

/// The dump file for one connection, when `PUMPKIN_TESTER_BEDROCK_DUMP` is set.
fn open_dump(name: &str, network_id: u64) -> Result<Option<std::fs::File>, String> {
    let Ok(dir) = std::env::var("PUMPKIN_TESTER_BEDROCK_DUMP") else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir).map_err(|err| format!("packet dump dir {dir}: {err}"))?;
    let path = std::path::Path::new(&dir).join(format!("{name}-{network_id:016x}.hex"));
    std::fs::File::create(&path)
        .map(Some)
        .map_err(|err| format!("packet dump {}: {err}", path.display()))
}

/// A stable, made-up Xbox user id for `name`, in the range real ones use.
pub fn xuid_for(name: &str) -> u64 {
    // FNV-1a, so the id is the same on every run.
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    2_535_000_000_000_000 + hash % 1_000_000_000_000
}

fn b64url(data: &[u8]) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn signing_key() -> SigningKey {
    loop {
        let bytes = rand::random::<[u8; 48]>();
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            return key;
        }
    }
}

/// An ES384 JWT whose header carries its own public key in `x5u`, as a self-signed client makes.
fn self_signed_jwt(key: &SigningKey, public_key_b64: &str, payload: &Value) -> String {
    let header = json!({ "alg": "ES384", "x5u": public_key_b64 });
    let input = format!(
        "{}.{}",
        b64url(header.to_string().as_bytes()),
        b64url(payload.to_string().as_bytes())
    );
    let signature: Signature = key.sign(input.as_bytes());
    format!("{input}.{}", b64url(&signature.to_bytes()))
}

fn read_error(what: &str, err: impl std::fmt::Display) -> String {
    format!("failed to read {what}: {err}")
}

impl BedrockBot {
    /// Checks the server's `/v1/join` status, then joins and waits until the bot has spawned.
    pub async fn connect_and_join(signaling: SocketAddr, name: &str) -> Result<Self, String> {
        Self::connect_and_join_as(signaling, name, xuid_for(name)).await
    }

    /// Like [`Self::connect_and_join`], claiming `xuid` instead of the one derived from `name`.
    pub async fn connect_and_join_as(
        signaling: SocketAddr,
        name: &str,
        xuid: u64,
    ) -> Result<Self, String> {
        let status = http_request(signaling, "GET", "/v1/join", &[]).await?;
        if status.status != 200 {
            return Err(format!("/v1/join answered HTTP {}", status.status));
        }
        let status: Value = serde_json::from_slice(&status.body)
            .map_err(|err| format!("/v1/join body is not JSON: {err}"))?;
        if status["protocol"].as_u64() != Some(u64::from(CURRENT_BEDROCK_MC_PROTOCOL)) {
            return Err(format!("/v1/join reports {status}"));
        }

        let network_id = rand::random::<u64>();
        let transport = NetherNetClient::connect(signaling, network_id).await?;
        let mut bot = Self {
            name: name.to_string(),
            xuid,
            transport,
            encoder: BedrockBatchEncoder::new(),
            decoder: BedrockBatchDecoder::new(),
            compressed: false,
            runtime_id: 0,
            unique_id: 0,
            position: Vector3::new(0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            tick: 0,
            chunks: HashSet::new(),
            players: HashMap::new(),
            actors: HashMap::new(),
            actor_runtime_ids: HashMap::new(),
            actor_flags: HashMap::new(),
            actor_longs: HashMap::new(),
            links: Vec::new(),
            driving: None,
            texts: Vec::new(),
            block_updates: Vec::new(),
            open_container: None,
            stack_responses: Vec::new(),
            titles: Vec::new(),
            health: 20,
            on_ground: true,
            disconnect: None,
            packet_counts: HashMap::new(),
            pending: VecDeque::new(),
            dump: open_dump(name, network_id)?,
        };
        bot.login().await?;
        Ok(bot)
    }

    async fn login(&mut self) -> Result<(), String> {
        let mut payload = Vec::new();
        i32::try_from(CURRENT_BEDROCK_MC_PROTOCOL)
            .map_err(|_| "protocol does not fit an i32")?
            .write_be(&mut payload)
            .map_err(|err| err.to_string())?;
        self.send_raw(SRequestNetworkSettings::PACKET_ID, &payload)
            .await?;
        let settings = self.wait_for(CNetworkSettings::PACKET_ID).await?;
        let threshold = u16::read(&mut &settings.payload[..])
            .map_err(|err| read_error("NetworkSettings", err))?;
        self.encoder.set_compression((usize::from(threshold), 6));
        self.decoder.set_compression(usize::from(threshold));
        self.compressed = true;

        self.send_raw(SLogin::PACKET_ID, &self.login_payload()?)
            .await?;
        let status = self.wait_for(CPlayStatus::PACKET_ID).await?;
        let status =
            i32::read_be(&mut &status.payload[..]).map_err(|err| read_error("PlayStatus", err))?;
        if status != PLAY_STATUS_LOGIN_SUCCESS {
            return Err(format!("login refused with play status {status}"));
        }

        self.wait_for(CResourcePacksInfo::PACKET_ID).await?;
        self.send_pack_response(SResourcePackClientResponse::STATUS_HAVE_ALL_PACKS)
            .await?;
        self.wait_for(CResourcePackStackPacket::PACKET_ID).await?;
        self.send_pack_response(SResourcePackClientResponse::STATUS_COMPLETED)
            .await?;

        let start = self.wait_for(CStartGame::PACKET_ID).await?;
        self.read_start_game(&start.payload)?;

        let mut radius = Vec::new();
        VarInt(4)
            .write(&mut radius)
            .map_err(|err| err.to_string())?;
        4u8.write(&mut radius).map_err(|err| err.to_string())?;
        self.send_raw(SRequestChunkRadius::PACKET_ID, &radius)
            .await?;

        loop {
            let packet = self.next_packet().await?;
            let id = packet.id;
            let payload = packet.payload.clone();
            self.handle_packet(&packet)?;
            self.check_disconnected()?;
            if id == CPlayStatus::PACKET_ID
                && i32::read_be(&mut &payload[..]).ok() == Some(PLAY_STATUS_PLAYER_SPAWN)
            {
                break;
            }
        }

        let mut initialized = Vec::new();
        VarULong(self.runtime_id)
            .write(&mut initialized)
            .map_err(|err| err.to_string())?;
        self.send_raw(SSetLocalPlayerAsInitialized::PACKET_ID, &initialized)
            .await
    }

    fn login_payload(&self) -> Result<Vec<u8>, String> {
        let key = signing_key();
        let public_key = key
            .verifying_key()
            .to_public_key_der()
            .map_err(|err| format!("public key encoding failed: {err}"))?;
        let public_key_b64 = general_purpose::STANDARD.encode(public_key.as_bytes());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        let identity = self_signed_jwt(
            &key,
            &public_key_b64,
            &json!({
                "xname": self.name,
                "xid": self.xuid.to_string(),
                "cpk": public_key_b64,
                "iat": now,
                "exp": now + 3600,
            }),
        );
        let auth = json!({
            "AuthenticationType": AUTHENTICATION_SELF_SIGNED,
            "Token": identity,
        })
        .to_string();
        let client_data = self_signed_jwt(
            &key,
            &public_key_b64,
            &json!({
                "ServerAddress": "127.0.0.1",
                "ThirdPartyName": self.name,
                "LanguageCode": "en_US",
                "GameVersion": pumpkin_world::CURRENT_BEDROCK_MC_VERSION,
                "DeviceOS": 7,
                "DeviceId": format!("pumpkin-tester-{}", self.name),
                "DeviceModel": "pumpkin-tester",
                "MaxViewDistance": 4,
                "CurrentInputMode": INPUT_MODE_MOUSE,
                "DefaultInputMode": INPUT_MODE_MOUSE,
                "UIProfile": 0,
                "GuiScale": 0,
                "ClientRandomId": rand::random::<i64>(),
                "SelfSignedId": uuid::Uuid::new_v4().to_string(),
                "SkinId": "pumpkin-tester-skin",
                "SkinData": general_purpose::STANDARD.encode(vec![0u8; 64 * 64 * 4]),
                "SkinImageWidth": 64,
                "SkinImageHeight": 64,
                "SkinResourcePatch": general_purpose::STANDARD
                    .encode(br#"{"geometry":{"default":"geometry.humanoid.custom"}}"#),
                "SkinGeometryData": "",
                "ArmSize": "wide",
                "SkinColor": "#0",
                "PersonaSkin": false,
                "PremiumSkin": false,
                "TrustedSkin": true,
                "CapeData": "",
                "CapeId": "",
                "CapeImageWidth": 0,
                "CapeImageHeight": 0,
                "AnimatedImageData": [],
                "PersonaPieces": [],
                "PieceTintColors": [],
                "PlatformOfflineId": "",
                "PlatformOnlineId": "",
                "PlayFabId": "",
            }),
        );

        let mut payload = Vec::new();
        i32::try_from(CURRENT_BEDROCK_MC_PROTOCOL)
            .map_err(|_| "protocol does not fit an i32")?
            .write_be(&mut payload)
            .map_err(|err| err.to_string())?;
        let request_len = 4 + auth.len() + 4 + client_data.len();
        VarUInt(u32::try_from(request_len).map_err(|_| "login too large")?)
            .write(&mut payload)
            .map_err(|err| err.to_string())?;
        for part in [auth.as_bytes(), client_data.as_bytes()] {
            u32::try_from(part.len())
                .map_err(|_| "login part too large")?
                .write(&mut payload)
                .map_err(|err| err.to_string())?;
            payload.extend_from_slice(part);
        }
        Ok(payload)
    }

    async fn send_pack_response(&self, status: u8) -> Result<(), String> {
        let mut payload = Vec::new();
        // The server reads the status as a var uint one below the response codes.
        VarUInt(u32::from(status) - 1)
            .write(&mut payload)
            .map_err(|err| err.to_string())?;
        String::new()
            .write(&mut payload)
            .map_err(|err| err.to_string())?;
        self.send_raw(SResourcePackClientResponse::PACKET_ID, &payload)
            .await
    }

    fn read_start_game(&mut self, payload: &[u8]) -> Result<(), String> {
        let reader = &mut &payload[..];
        self.unique_id = VarLong::read(reader)
            .map_err(|err| read_error("StartGame", err))?
            .0;
        self.runtime_id = VarULong::read(reader)
            .map_err(|err| read_error("StartGame", err))?
            .0;
        let _gamemode = VarInt::read(reader).map_err(|err| read_error("StartGame", err))?;
        self.position = Vector3::<f32>::read(reader).map_err(|err| read_error("StartGame", err))?;
        self.pitch = f32::read(reader).map_err(|err| read_error("StartGame", err))?;
        self.yaw = f32::read(reader).map_err(|err| read_error("StartGame", err))?;
        Ok(())
    }

    /// Encodes one serverbound packet into its own batch and sends it.
    pub async fn send_raw(&self, packet_id: i32, payload: &[u8]) -> Result<(), String> {
        let packet_id = u16::try_from(packet_id).map_err(|_| "bad packet id")?;
        let mut batch = Vec::new();
        self.encoder
            .write_game_packet(
                packet_id,
                SubClient::Main,
                SubClient::Main,
                payload,
                &mut batch,
            )
            .map_err(|err| format!("failed to encode packet {packet_id}: {err}"))?;
        // NetherNet carries the batch without the 0xfe marker.
        self.transport.send(&batch[1..]).await
    }

    async fn next_batch(&mut self) -> Result<Vec<RawPacket>, String> {
        let batch = match timeout(PACKET_TIMEOUT, self.transport.recv()).await {
            Ok(Some(batch)) => batch?,
            Ok(None) => return Err(format!("Bedrock bot '{}' connection closed", self.name)),
            Err(_) => {
                return Err(format!(
                    "Bedrock bot '{}' timed out waiting for a packet",
                    self.name
                ));
            }
        };
        self.decode_batch(&batch).await
    }

    async fn decode_batch(&mut self, batch: &[u8]) -> Result<Vec<RawPacket>, String> {
        let mut framed = Vec::with_capacity(batch.len() + 1);
        framed.push(pumpkin_protocol::bedrock::BEDROCK_GAME_PACKET);
        framed.extend_from_slice(batch);
        let payload = self
            .decoder
            .get_packet_payload(framed)
            .await
            .map_err(|err| format!("bad batch from the server: {err}"))?;
        let length = payload.len() as u64;
        let mut cursor = Cursor::new(payload);
        let mut packets = Vec::new();
        while cursor.position() < length {
            packets.push(
                self.decoder
                    .get_game_packet(&mut cursor)
                    .map_err(|err| format!("bad packet from the server: {err}"))?,
            );
        }
        if let Some(dump) = &mut self.dump {
            for packet in &packets {
                let mut line = String::with_capacity(packet.payload.len() * 2 + 8);
                let _ = write!(line, "{} ", packet.id);
                for byte in &packet.payload {
                    let _ = write!(line, "{byte:02x}");
                }
                line.push('\n');
                dump.write_all(line.as_bytes())
                    .map_err(|err| format!("packet dump write failed: {err}"))?;
            }
        }
        Ok(packets)
    }

    /// The next packet from the server, waiting for one if none is queued.
    pub async fn next_packet(&mut self) -> Result<RawPacket, String> {
        loop {
            if let Some(packet) = self.pending.pop_front() {
                *self.packet_counts.entry(packet.id).or_default() += 1;
                return Ok(packet);
            }
            let packets = self.next_batch().await?;
            self.pending.extend(packets);
        }
    }

    /// Handles packets until one with `packet_id` arrives, and returns that one unhandled.
    async fn wait_for(&mut self, packet_id: i32) -> Result<RawPacket, String> {
        loop {
            let packet = self.next_packet().await?;
            if packet.id == packet_id {
                return Ok(packet);
            }
            self.handle_packet(&packet)?;
            self.check_disconnected()?;
        }
    }

    /// An error once the server has sent `Disconnect`, so a refused join fails at once.
    fn check_disconnected(&self) -> Result<(), String> {
        self.disconnect.as_ref().map_or(Ok(()), |message| {
            Err(format!(
                "Bedrock bot '{}' was disconnected: {message}",
                self.name
            ))
        })
    }

    /// Handles every packet that has already arrived, without waiting.
    pub async fn poll_packets(&mut self) -> Result<(), String> {
        while let Some(batch) = self.transport.try_recv() {
            let packets = self.decode_batch(&batch?).await?;
            self.pending.extend(packets);
        }
        while let Some(packet) = self.pending.pop_front() {
            *self.packet_counts.entry(packet.id).or_default() += 1;
            self.handle_packet(&packet)?;
        }
        Ok(())
    }

    fn handle_packet(&mut self, packet: &RawPacket) -> Result<(), String> {
        let reader = &mut &packet.payload[..];
        match packet.id {
            CLevelChunk::PACKET_ID => {
                let x = VarInt::read(reader)
                    .map_err(|err| read_error("LevelChunk", err))?
                    .0;
                let z = VarInt::read(reader)
                    .map_err(|err| read_error("LevelChunk", err))?
                    .0;
                self.chunks.insert((x, z));
            }
            CUpdateBlock::PACKET_ID => {
                let position =
                    BlockPos::read(reader).map_err(|err| read_error("UpdateBlock", err))?;
                let mut field = || {
                    VarUInt::read(&mut *reader)
                        .map(|value| value.0)
                        .map_err(|err| read_error("UpdateBlock", err))
                };
                let block = field()?;
                let _flags = field()?;
                // Layer 1 holds waterlogging; the block itself is on layer 0.
                if field()? == 0 {
                    self.block_updates.push((position, block));
                }
            }
            CONTAINER_OPEN_ID => {
                let err = |err| read_error("ContainerOpen", err);
                let window = u8::read(reader).map_err(err)?;
                let container_type = u8::read(reader).map_err(err)?;
                self.open_container = Some((window, container_type));
            }
            CONTAINER_CLOSE_ID => self.open_container = None,
            ITEM_STACK_RESPONSE_ID => {
                let err = |err| read_error("ItemStackResponse", err);
                let count = VarUInt::read(reader).map_err(err)?.0;
                // Only the first response is read; the bot sends one request at a time.
                if count > 0 {
                    let result = u8::read(reader).map_err(err)?;
                    let request = VarInt::read(reader).map_err(err)?.0;
                    self.stack_responses.push((request, result));
                }
            }
            id if id == SText::PACKET_ID => {
                let text = SText::read(reader).map_err(|err| read_error("Text", err))?;
                self.texts.push(SeenText {
                    message: text.message.into_owned(),
                    parameters: text
                        .parameters
                        .into_iter()
                        .map(std::borrow::Cow::into_owned)
                        .collect(),
                });
            }
            SET_TITLE_ID => {
                let err = |err| read_error("SetTitle", err);
                let kind = VarInt::read(reader).map_err(err)?.0;
                let text = String::read(reader).map_err(err)?;
                self.titles.push((kind, text));
            }
            CSetHealth::PACKET_ID => {
                self.health = VarInt::read(reader)
                    .map_err(|err| read_error("SetHealth", err))?
                    .0;
            }
            CUpdateAttributes::PACKET_ID => self.read_attributes(reader)?,
            CSetActorData::PACKET_ID => self.read_actor_data(reader)?,
            CSetActorLink::PACKET_ID => {
                let err = |err| read_error("SetActorLink", err);
                let ridden = VarLong::read(reader).map_err(err)?.0;
                let rider = VarLong::read(reader).map_err(err)?.0;
                let link_type = u8::read(reader).map_err(err)?;
                self.links.push((ridden, rider, link_type));
            }
            CDisconnect::PACKET_ID => {
                let _reason = VarInt::read(reader).map_err(|err| read_error("Disconnect", err))?;
                let skip_message =
                    bool::read(reader).map_err(|err| read_error("Disconnect", err))?;
                self.disconnect = Some(if skip_message {
                    String::new()
                } else {
                    String::read(reader).map_err(|err| read_error("Disconnect", err))?
                });
            }
            _ => self.handle_entity_packet(packet.id, reader)?,
        }
        Ok(())
    }

    /// Players and other entities coming, going and moving.
    /// Takes the bot's health from an `UpdateAttributes` for its own runtime id, which is how
    /// PEX tells Bedrock players their health.
    fn read_attributes(&mut self, reader: &mut &[u8]) -> Result<(), String> {
        let err = |err| read_error("UpdateAttributes", err);
        let runtime_id = VarULong::read(reader).map_err(err)?.0;
        let count = VarUInt::read(reader).map_err(err)?.0;
        for _ in 0..count {
            let mut floats = [0.0f32; 6];
            for value in &mut floats {
                *value = f32::read(reader).map_err(err)?;
            }
            let name = String::read(reader).map_err(err)?;
            for _ in 0..VarUInt::read(reader).map_err(err)?.0 {
                String::read(reader).map_err(err)?;
                String::read(reader).map_err(err)?;
                f32::read(reader).map_err(err)?;
                i32::read(reader).map_err(err)?;
                i32::read(reader).map_err(err)?;
                bool::read(reader).map_err(err)?;
            }
            if runtime_id == self.runtime_id && name == "minecraft:health" {
                // Min, max, then the current value.
                self.health = floats[2].ceil() as i32;
            }
        }
        Ok(())
    }

    /// Keeps the actor flag words from a `SetActorData`. Stops at a compound tag, the one type
    /// it can't skip.
    fn read_actor_data(&mut self, reader: &mut &[u8]) -> Result<(), String> {
        let err = |err| read_error("SetActorData", err);
        let runtime_id = VarULong::read(reader).map_err(err)?.0;
        for _ in 0..VarUInt::read(reader).map_err(err)?.0 {
            let key = VarUInt::read(reader).map_err(err)?.0;
            let data_type = VarUInt::read(reader).map_err(err)?.0;
            // The type again, as a one-byte selector.
            u8::read(reader).map_err(err)?;
            match data_type {
                0 => drop(u8::read(reader).map_err(err)?),
                1 => drop(i16::read(reader).map_err(err)?),
                2 => drop(VarInt::read(reader).map_err(err)?),
                3 => drop(f32::read(reader).map_err(err)?),
                4 => drop(String::read(reader).map_err(err)?),
                6 => {
                    for _ in 0..3 {
                        VarInt::read(reader).map_err(err)?;
                    }
                }
                7 => {
                    let value = VarLong::read(reader).map_err(err)?.0;
                    self.actor_longs.insert((runtime_id, key), value);
                    let words = self.actor_flags.entry(runtime_id).or_default();
                    if key == entity_data_key::FLAGS {
                        words[0] = value;
                    } else if key == entity_data_key::FLAGS_TWO {
                        words[1] = value;
                    }
                }
                8 => drop(Vector3::<f32>::read(reader).map_err(err)?),
                _ => return Ok(()),
            }
        }
        Ok(())
    }

    /// Whether the last `SetActorData` for `runtime_id` had actor flag `flag` set.
    pub fn actor_flag(&self, runtime_id: u64, flag: u32) -> bool {
        self.actor_flags.get(&runtime_id).is_some_and(|words| {
            let (word, index) = if flag >= 64 {
                (words[1], flag - 64)
            } else {
                (words[0], flag)
            };
            word & (1 << index) != 0
        })
    }

    /// What `rider` rides according to the last `SetActorLink` naming it: the vehicle's unique
    /// id and the link type. `None` once a remove link came last.
    pub fn ride_link(&self, rider: i64) -> Option<(i64, u8)> {
        self.links
            .iter()
            .rev()
            .find(|(_, link_rider, _)| *link_rider == rider)
            .filter(|(_, _, link_type)| *link_type != LINK_REMOVE)
            .map(|(ridden, _, link_type)| (*ridden, *link_type))
    }

    /// Clicks `face` of `block` with hotbar slot `slot`, as a real client's `UseItem` inventory
    /// transaction: `in_hand` is the item the client says it holds, and `actions` are its
    /// predicted inventory changes as (slot, old, new).
    pub async fn use_item_on_block(
        &self,
        block: BlockPos,
        face: u8,
        slot: i32,
        in_hand: &ItemStack,
        actions: &[(u32, ItemStack, ItemStack)],
    ) -> Result<(), String> {
        self.send_use_item(USE_ITEM_CLICK_BLOCK, block, face, slot, in_hand, actions)
            .await
    }

    /// Hits the entity with this runtime id, as a real client's `UseItemOnEntity` attack
    /// transaction with an empty hand.
    pub async fn attack(&self, runtime_id: u64) -> Result<(), String> {
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            // No legacy request, no legacy slots, transaction type `UseItemOnEntity`, no actions.
            VarInt(0).write(payload)?;
            false.write(payload)?;
            VarUInt(3).write(payload)?;
            VarUInt(0).write(payload)?;
            VarULong(runtime_id).write(payload)?;
            // Attack, with hotbar slot 0 in the main hand.
            VarInt(1).write(payload)?;
            VarInt(0).write(payload)?;
            0u8.write(payload)?;
            NetworkItemDescriptor::from(&ItemStack::EMPTY.clone()).write(payload)?;
            self.position.write(payload)?;
            Vector3::new(0.0f32, 0.0, 0.0).write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(INVENTORY_TRANSACTION_ID, &payload).await
    }

    /// Uses the item in hotbar slot `slot` without a block, as a real client's click-air
    /// `UseItem` transaction.
    pub async fn use_item_in_air(&self, slot: i32, in_hand: &ItemStack) -> Result<(), String> {
        self.send_use_item(
            USE_ITEM_CLICK_AIR,
            BlockPos::new(0, 0, 0),
            u8::MAX,
            slot,
            in_hand,
            &[],
        )
        .await
    }

    async fn send_use_item(
        &self,
        action: i32,
        block: BlockPos,
        face: u8,
        slot: i32,
        in_hand: &ItemStack,
        actions: &[(u32, ItemStack, ItemStack)],
    ) -> Result<(), String> {
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            // No legacy request, no legacy slots, transaction type `UseItem`.
            VarInt(0).write(payload)?;
            false.write(payload)?;
            VarUInt(2).write(payload)?;
            VarUInt(actions.len() as u32).write(payload)?;
            for (slot, old, new) in actions {
                // From a container: the player inventory, window 0, no source flags.
                VarUInt(0).write(payload)?;
                true.write(payload)?;
                0i8.write(payload)?;
                false.write(payload)?;
                VarUInt(*slot).write(payload)?;
                NetworkItemDescriptor::from(old).write(payload)?;
                NetworkItemDescriptor::from(new).write(payload)?;
            }
            // The action, triggered by player input.
            VarInt(action).write(payload)?;
            1u8.write(payload)?;
            block.write(payload)?;
            face.write(payload)?;
            VarInt(slot).write(payload)?;
            // Main hand.
            0u8.write(payload)?;
            NetworkItemDescriptor::from(in_hand).write(payload)?;
            self.position.write(payload)?;
            // Where on the face it clicked, then the clicked block's runtime id (unused), a
            // successful client prediction and no cooldown.
            Vector3::new(0.5f32, 1.0, 0.5).write(payload)?;
            VarUInt(0).write(payload)?;
            1u8.write(payload)?;
            0u8.write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(INVENTORY_TRANSACTION_ID, &payload).await
    }

    /// Sends one `ItemStackRequest` with `actions`, each a take or place of `count` items from
    /// one slot to another, as a real client's inventory screen does.
    pub async fn item_stack_request(
        &self,
        request_id: i32,
        actions: &[StackMove],
    ) -> Result<(), String> {
        self.craft_request(request_id, None, actions).await
    }

    /// An `ItemStackRequest` that starts with `CraftRecipe` (recipe network id, times) when
    /// `craft` is set, followed by `actions`, as a real client's crafting screen sends.
    pub async fn craft_request(
        &self,
        request_id: i32,
        craft: Option<(u32, u8)>,
        actions: &[StackMove],
    ) -> Result<(), String> {
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            VarUInt(1).write(payload)?;
            VarInt(request_id).write(payload)?;
            VarUInt((actions.len() + usize::from(craft.is_some())) as u32).write(payload)?;
            if let Some((recipe, times)) = craft {
                VarUInt(STACK_ACTION_CRAFT_RECIPE).write(payload)?;
                0u8.write(payload)?;
                VarUInt(recipe).write(payload)?;
                times.write(payload)?;
            }
            for action in actions {
                VarUInt(u32::from(action.kind as u8)).write(payload)?;
                // The legacy action type, unused.
                0u8.write(payload)?;
                action.count.write(payload)?;
                for (container, slot) in [action.from, action.to] {
                    ItemStackRequestSlotInfo {
                        container_name: FullContainerName {
                            container_name: container,
                            dynamic_id: None,
                        },
                        slot_id: slot,
                        stack_id: 0,
                    }
                    .write(payload)?;
                }
            }
            // No filter strings, no filter cause.
            VarUInt(0).write(payload)?;
            0i32.write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(ITEM_STACK_REQUEST_ID, &payload).await
    }

    /// Closes the open container, as the client does when its screen shuts.
    pub async fn close_container(&self) -> Result<(), String> {
        let (window, container_type) = self.open_container.ok_or("no container is open")?;
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            window.write(payload)?;
            container_type.write(payload)?;
            false.write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(CONTAINER_CLOSE_ID, &payload).await
    }

    /// Asks to get off the vehicle, as the dismount button does.
    pub async fn stop_riding(&self, vehicle_runtime_id: u64) -> Result<(), String> {
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            INTERACT_STOP_RIDING.write(payload)?;
            VarULong(vehicle_runtime_id).write(payload)?;
            // Where the client would land.
            true.write(payload)?;
            self.position.write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(INTERACT_ID, &payload).await
    }

    /// The runtime id of the player named `name`, once `AddPlayer` announced it.
    pub fn player_runtime_id(&self, name: &str) -> Option<u64> {
        self.players
            .iter()
            .find(|(_, player)| player.name == name)
            .map(|(runtime_id, _)| *runtime_id)
    }

    fn handle_entity_packet(&mut self, id: i32, reader: &mut &[u8]) -> Result<(), String> {
        match id {
            CAddPlayer::PACKET_ID => {
                let mut uuid = [0; 16];
                reader
                    .read_exact(&mut uuid)
                    .map_err(|err| read_error("AddPlayer", err))?;
                let name = String::read(reader).map_err(|err| read_error("AddPlayer", err))?;
                let runtime_id = VarULong::read(reader)
                    .map_err(|err| read_error("AddPlayer", err))?
                    .0;
                let _platform_chat_id =
                    String::read(reader).map_err(|err| read_error("AddPlayer", err))?;
                let position =
                    Vector3::<f32>::read(reader).map_err(|err| read_error("AddPlayer", err))?;
                self.players
                    .insert(runtime_id, SeenPlayer { name, position });
            }
            CAddActor::PACKET_ID => {
                let unique_id = VarLong::read(reader)
                    .map_err(|err| read_error("AddActor", err))?
                    .0;
                let runtime_id = VarULong::read(reader)
                    .map_err(|err| read_error("AddActor", err))?
                    .0;
                let identifier = String::read(reader).map_err(|err| read_error("AddActor", err))?;
                self.actors.insert(unique_id, identifier);
                self.actor_runtime_ids.insert(unique_id, runtime_id);
            }
            CRemoveActor::PACKET_ID => {
                let unique_id = VarLong::read(reader)
                    .map_err(|err| read_error("RemoveActor", err))?
                    .0;
                self.actors.remove(&unique_id);
                if let Some(runtime_id) = self.actor_runtime_ids.remove(&unique_id) {
                    self.players.remove(&runtime_id);
                }
                // A player's unique id is its runtime id on Pumpkin.
                self.players.remove(&(unique_id as u64));
            }
            CMovePlayer::PACKET_ID => {
                let runtime_id = VarULong::read(reader)
                    .map_err(|err| read_error("MovePlayer", err))?
                    .0;
                let position =
                    Vector3::<f32>::read(reader).map_err(|err| read_error("MovePlayer", err))?;
                if runtime_id == self.runtime_id {
                    self.position = position;
                } else if let Some(player) = self.players.get_mut(&runtime_id) {
                    player.position = position;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Sends a chat message, as the chat box does.
    pub async fn chat(&self, message: &str) -> Result<(), String> {
        let packet =
            SText::chat_with_xuid(message.to_string(), self.name.clone(), String::new(), None);
        let mut payload = Vec::new();
        packet.write(&mut payload).map_err(|err| err.to_string())?;
        debug_assert_eq!(payload.get(1), Some(&TEXT_TYPE_CHAT));
        self.send_raw(SText::PACKET_ID, &payload).await
    }

    /// Runs a command, with the leading slash, as the chat box does.
    pub async fn command(&self, command: &str) -> Result<(), String> {
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            command.write(payload)?;
            "player".write(payload)?;
            payload.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
            "".write(payload)?;
            0i64.write(payload)?;
            false.write(payload)?;
            "latest".write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(SCommandRequest::PACKET_ID, &payload).await
    }

    /// Sends one `PlayerAuthInput` at `position`, standing on the ground if [`Self::on_ground`]
    /// says so, with block actions if there are any.
    pub async fn send_input(
        &mut self,
        position: Vector3<f32>,
        block_actions: &[(i32, BlockPos, i32)],
    ) -> Result<(), String> {
        let flags: &[i32] = if self.on_ground {
            &[INPUT_VERTICAL_COLLISION]
        } else {
            &[]
        };
        self.send_input_with_flags(position, flags, block_actions)
            .await
    }

    /// Sends one `PlayerAuthInput` at `position` with the given `InputData` flags, plus
    /// `PerformBlockActions` when there are block actions.
    pub async fn send_input_with_flags(
        &mut self,
        position: Vector3<f32>,
        flags: &[i32],
        block_actions: &[(i32, BlockPos, i32)],
    ) -> Result<(), String> {
        self.tick += 1;
        let mut delta = position - self.position;
        // A client standing on the ground reports one tick of gravity as its vertical motion.
        if flags.contains(&INPUT_VERTICAL_COLLISION) && delta.y >= 0.0 {
            delta.y = GROUND_GRAVITY_DELTA;
        }
        let mut flags = flags.to_vec();
        if !block_actions.is_empty() {
            flags.push(INPUT_PERFORM_BLOCK_ACTIONS);
        }
        if self.driving.is_some() {
            flags.push(INPUT_CLIENT_PREDICTED_VEHICLE);
        }
        let driving = self.driving;
        let mut payload = Vec::new();
        let write = |payload: &mut Vec<u8>| -> std::io::Result<()> {
            self.pitch.write(payload)?;
            self.yaw.write(payload)?;
            position.write(payload)?;
            0.0f32.write(payload)?;
            0.0f32.write(payload)?;
            self.yaw.write(payload)?;
            VarUInt(flags.len() as u32).write(payload)?;
            for flag in &flags {
                VarInt(*flag).write(payload)?;
            }
            VarUInt(INPUT_MODE_MOUSE).write(payload)?;
            VarUInt(PLAY_MODE_NORMAL).write(payload)?;
            VarInt(INTERACTION_MODEL_CROSSHAIR).write(payload)?;
            self.pitch.write(payload)?;
            self.yaw.write(payload)?;
            VarULong(self.tick).write(payload)?;
            delta.write(payload)?;
            // No item interaction, no item stack request.
            false.write(payload)?;
            false.write(payload)?;
            if block_actions.is_empty() {
                false.write(payload)?;
            } else {
                true.write(payload)?;
                VarUInt(block_actions.len() as u32).write(payload)?;
                for (action, block, face) in block_actions {
                    VarInt(*action).write(payload)?;
                    block.write(payload)?;
                    VarInt(*face).write(payload)?;
                }
            }
            if let Some((vehicle, rotation)) = driving {
                true.write(payload)?;
                rotation.write(payload)?;
                true.write(payload)?;
                VarLong(vehicle).write(payload)?;
            } else {
                false.write(payload)?;
                false.write(payload)?;
            }
            0.0f32.write(payload)?;
            0.0f32.write(payload)?;
            Vector3::new(0.0f32, 0.0, 0.0).write(payload)?;
            0.0f32.write(payload)?;
            0.0f32.write(payload)
        };
        write(&mut payload).map_err(|err| err.to_string())?;
        self.send_raw(SPlayerAuthInput::PACKET_ID, &payload).await?;
        self.position = position;
        Ok(())
    }

    pub async fn disconnect(self) {
        self.transport.close().await;
    }
}
