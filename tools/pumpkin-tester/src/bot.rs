mod inventory;

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::Duration,
};

use bytes::Bytes;
use pumpkin_data::{Block, entity::EntityType, item_stack::ItemStack, packet::CURRENT_MC_VERSION};
use pumpkin_protocol::{
    ClientPacket, ConnectionState, RawPacket, ServerPacket,
    codec::{
        item_stack_seralizer::{ItemStackSerializer, OptionalItemStackHash},
        var_int::VarInt,
    },
    java::{
        client::{
            config::{CFinishConfig, CKnownPacks},
            login::CLoginSuccess,
            play::{
                CBlockUpdate, CChunkBatchEnd, CChunkData, CCloseContainer, CDisguisedChatMessage,
                CEntityPositionSync, CEntityVelocity, CGameEvent, CHeadRot, CKeepAlive, CLogin,
                CMultiBlockUpdate, COpenScreen, CPingResponse, CPlayDisconnect, CPlayerChatMessage,
                CPlayerPosition, CRemoveEntities, CRespawn, CSetContainerContent,
                CSetContainerSlot, CSetCursorItem, CSetEntityMetadata, CSetHealth, CSetPassengers,
                CSetPlayerInventory, CSetSelectedSlot, CSpawnEntity, CSystemChatMessage,
                CUnloadChunk, CUpdateEntityPos, CUpdateEntityPosRot, CUpdateEntityRot,
            },
        },
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::{
            config::{SAcknowledgeFinishConfig, SClientInformationConfig, SKnownPacks},
            handshake::SHandShake,
            login::{SLoginAcknowledged, SLoginStart},
            play::{
                Action, ActionType, SAttack, SChatCommand, SChatMessage, SChunkBatch, SClickSlot,
                SClientCommand, SCloseContainer, SConfirmTeleport, SInteract, SKeepAlive,
                SPickItemFromEntity, SPlayPingRequest, SPlayerAction, SPlayerCommand, SPlayerInput,
                SPlayerLoaded, SPlayerPosition, SPlayerPositionRotation, SPlayerRotation,
                SSetHeldItem, SSpectateEntity, SSwingArm, SUseItem, SUseItemOn, SlotActionType,
                Status,
            },
        },
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkWriteExt, ReadingError},
};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use tokio::{
    io::{BufReader, BufWriter},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc::{self, UnboundedReceiver, error::TryRecvError},
    task::JoinHandle,
    time::timeout,
};
use uuid::Uuid;

pub use inventory::{ClientInventory, crafting_menu, inventory_menu};

pub const PACKET_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_POLL_TICKS: usize = 500;

/// Chunks per tick the bot asks for after each batch. Vanilla's `PlayerChunkSender` clamps the
/// request to 64.
const CHUNKS_PER_TICK: f32 = 64.0;
/// Upper bound on list lengths the bot reads from the server.
const MAX_LIST_LENGTH: i32 = 1024;
/// Vanilla `VecDeltaCodec` scale of relative entity moves.
const ENTITY_DELTA_SCALE: f64 = 4096.0;
/// Vanilla `Player.getEyeHeight` while standing.
pub const EYE_HEIGHT: f64 = 1.62;
/// `ClientboundGameEventPacket` event ids.
const GAME_EVENT_NO_RESPAWN_BLOCK: u8 = 0;
const GAME_EVENT_WIN_GAME: u8 = 4;
/// `Direction.DOWN`, the face the vanilla client sends with actions that aren't about a block.
const FACE_DOWN: u8 = 0;
/// `Direction.UP`, the face the bot digs and clicks.
const FACE_UP: u8 = 1;

/// An entity the server has told the bot about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackedEntity {
    /// The `minecraft:entity_type` registry id.
    pub entity_type: i32,
    pub position: Vector3<f64>,
    /// The shared flags byte (on fire, sneaking, sprinting, swimming and so on), from the last
    /// `SetEntityData` that carried it.
    pub flags: u8,
    /// The `EntityPose` id, from the last `SetEntityData` that carried it.
    pub pose: i32,
}

/// `Entity.DATA_SHARED_FLAGS_ID` and `Entity.DATA_POSE`, and the data types the bot can skip
/// over to reach them.
const DATA_SHARED_FLAGS: u8 = 0;
const DATA_POSE: u8 = 6;
const DATA_TYPE_BYTE: i32 = 0;
const DATA_TYPE_INT: i32 = 1;
const DATA_TYPE_FLOAT: i32 = 3;
const DATA_TYPE_BOOLEAN: i32 = 8;
const DATA_TYPE_POSE: i32 = 20;
const DATA_END: u8 = 0xFF;

type PacketResult = Result<RawPacket, String>;

pub struct HeadlessBot {
    pub name: String,
    packets: UnboundedReceiver<PacketResult>,
    reader_task: JoinHandle<()>,
    pub writer: TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    pub position: Vector3<f64>,
    pub yaw: f32,
    pub pitch: f32,
    pub inventory: ClientInventory,
    /// Dropped item entities the bot has been told about, by entity id.
    pub item_entities: HashMap<i32, Vector3<f64>>,
    /// Dropped item entities the bot has been told about so far, removed or not.
    pub spawned_items: usize,
    /// Chunks the server has sent and not unloaded.
    pub chunks: HashSet<(i32, i32)>,
    /// While set, teleports are kept here instead of being confirmed, like a client that lags.
    pub held_teleports: Option<Vec<CPlayerPosition>>,
    /// Payload of the play disconnect packet, once the server has sent one.
    pub disconnect_reason: Option<Bytes>,
    /// How many of each [`is_counted_entity_packet`] packet arrived, by entity id and packet id.
    pub entity_packets: HashMap<(i32, i32), usize>,
    /// Every entity the server has spawned for the bot and not removed, by entity id.
    pub entities: HashMap<i32, TrackedEntity>,
    /// The passenger ids of each vehicle, from the last `SetPassengers` for it.
    pub passengers: HashMap<i32, Vec<i32>>,
    /// The dimension the bot is in, from the last `CRespawn`.
    pub dimension: String,
    /// From the last `CSetHealth`.
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    /// The server sent the win game event (the End poem and credits).
    pub won_game: bool,
    /// The server said the bot's bed or respawn anchor was missing or blocked.
    pub no_respawn_block: bool,
    /// Payloads of every system, player and disguised chat message.
    pub chat: Vec<Bytes>,
    /// Every single block update, as position and state id.
    pub block_updates: Vec<(BlockPos, i32)>,
    teleports_confirmed: u32,
    last_pong: i64,
    next_ping: i64,
    sequence: i32,
}

impl Drop for HeadlessBot {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

/// Reads packets on its own task: `TCPNetworkDecoder::get_raw_packet` loses data when a read is
/// cancelled, so the bot can't poll the socket with a timeout.
fn spawn_reader(
    read_half: OwnedReadHalf,
    name: String,
) -> (UnboundedReceiver<PacketResult>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut decoder = TCPNetworkDecoder::new(BufReader::new(read_half));
        loop {
            let packet = decoder
                .get_raw_packet()
                .await
                .map_err(|err| format!("Bot '{name}' packet decode error: {err}"));
            let failed = packet.is_err();
            if sender.send(packet).is_err() || failed {
                break;
            }
        }
    });
    (receiver, task)
}

fn read_error(packet_id: i32, err: &ReadingError) -> String {
    format!("Failed to read packet {packet_id} from the server: {err}")
}

fn read_list_length(payload: &mut &[u8]) -> Result<usize, ReadingError> {
    let length = payload.get_var_int()?.0;
    if (0..=MAX_LIST_LENGTH).contains(&length) {
        Ok(length as usize)
    } else {
        Err(ReadingError::Message(format!(
            "list length {length} out of bounds"
        )))
    }
}

fn read_stack(payload: &mut &[u8]) -> Result<ItemStack, ReadingError> {
    ItemStackSerializer::read_with_version(payload, &CURRENT_MC_VERSION)
        .map(ItemStackSerializer::to_stack)
}

/// The position change in a 26.3 `ClientboundMoveEntityPacket`: a var int packing the on-ground
/// flag and a step count, then the delta (`VecDelta.read`). `CUpdateEntityPos::read` still reads
/// the layout from before 26.3. Pumpkin only sends `VecDelta.Linear`, which has no steps.
fn read_entity_delta(payload: &mut &[u8]) -> Result<Vector3<f64>, ReadingError> {
    let steps = payload.get_var_int()?.0 >> 1;
    if steps > 0 {
        return Err(ReadingError::Message(format!(
            "stepped entity move with {steps} steps"
        )));
    }
    Ok(Vector3::new(
        f64::from(payload.get_i16_be()?),
        f64::from(payload.get_i16_be()?),
        f64::from(payload.get_i16_be()?),
    ) * (1.0 / ENTITY_DELTA_SCALE))
}

/// Spawn, movement, rotation, head rotation, metadata and velocity: the packets the entity
/// tracker sends. Each starts with the entity id.
fn is_counted_entity_packet(id: i32) -> bool {
    [
        CSpawnEntity::to_id(CURRENT_MC_VERSION),
        CUpdateEntityPos::to_id(CURRENT_MC_VERSION),
        CUpdateEntityPosRot::to_id(CURRENT_MC_VERSION),
        CUpdateEntityRot::to_id(CURRENT_MC_VERSION),
        CEntityPositionSync::to_id(CURRENT_MC_VERSION),
        CHeadRot::to_id(CURRENT_MC_VERSION),
        CSetEntityMetadata::to_id(CURRENT_MC_VERSION),
        CEntityVelocity::to_id(CURRENT_MC_VERSION),
    ]
    .contains(&id)
}

impl HeadlessBot {
    /// How many `P` packets about `entity_id` arrived since [`Self::entity_packets`] was cleared.
    pub fn entity_packet_count<P: MultiVersionJavaPacket>(&self, entity_id: i32) -> usize {
        self.entity_packets
            .get(&(entity_id, P::to_id(CURRENT_MC_VERSION)))
            .copied()
            .unwrap_or(0)
    }

    pub async fn send_packet<P: ClientPacket + MultiVersionJavaPacket>(
        &mut self,
        packet: &P,
    ) -> Result<(), String> {
        send_framed_packet(&mut self.writer, packet).await
    }

    pub async fn next_packet(&mut self) -> Result<RawPacket, String> {
        match timeout(PACKET_TIMEOUT, self.packets.recv()).await {
            Ok(Some(packet)) => packet,
            Ok(None) => Err(format!("Bot '{}' connection closed", self.name)),
            Err(_) => Err(format!(
                "Bot '{}' timed out waiting for packet from server",
                self.name
            )),
        }
    }

    pub async fn connect_and_join(addr: SocketAddr, name: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|err| format!("Bot '{name}' failed to connect to {addr}: {err}"))?;
        let _ = stream.set_nodelay(true);
        let (read_half, write_half) = stream.into_split();
        let (packets, reader_task) = spawn_reader(read_half, name.to_string());
        let mut bot = Self {
            name: name.to_string(),
            packets,
            reader_task,
            writer: TCPNetworkEncoder::new(BufWriter::new(write_half)),
            position: Vector3::new(0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            inventory: ClientInventory::new(),
            item_entities: HashMap::new(),
            spawned_items: 0,
            chunks: HashSet::new(),
            held_teleports: None,
            disconnect_reason: None,
            entity_packets: HashMap::new(),
            entities: HashMap::new(),
            passengers: HashMap::new(),
            dimension: "minecraft:overworld".to_string(),
            health: 20.0,
            food: 20,
            saturation: 5.0,
            won_game: false,
            no_respawn_block: false,
            chat: Vec::new(),
            block_updates: Vec::new(),
            teleports_confirmed: 0,
            last_pong: 0,
            next_ping: 0,
            sequence: 0,
        };

        // 1. Handshake -> Login
        bot.send_packet(&SHandShake {
            protocol_version: VarInt(CURRENT_MC_VERSION.protocol_version()),
            server_address: "127.0.0.1".into(),
            server_port: addr.port(),
            next_state: ConnectionState::Login,
        })
        .await?;

        bot.send_packet(&SLoginStart {
            name: name.into(),
            uuid: Uuid::new_v4(),
        })
        .await?;

        let login_success_id = CLoginSuccess::to_id(CURRENT_MC_VERSION);
        loop {
            let pkt = bot.next_packet().await?;
            if pkt.id == login_success_id {
                break;
            }
        }

        bot.send_packet(&SLoginAcknowledged).await?;

        // 2. Configuration state
        bot.send_packet(&SClientInformationConfig {
            locale: "en_us",
            view_distance: 4,
            chat_mode: VarInt(0),
            chat_colors: true,
            skin_parts: 0x7F,
            main_hand: VarInt(1),
            text_filtering: false,
            server_listing: true,
        })
        .await?;

        let known_packs_id = CKnownPacks::to_id(CURRENT_MC_VERSION);
        let finish_config_id = CFinishConfig::to_id(CURRENT_MC_VERSION);
        loop {
            let pkt = bot.next_packet().await?;
            if pkt.id == known_packs_id {
                bot.send_packet(&SKnownPacks {
                    known_packs: Vec::new(),
                })
                .await?;
            } else if pkt.id == finish_config_id {
                bot.send_packet(&SAcknowledgeFinishConfig).await?;
                break;
            }
        }

        // 3. Play state: wait for CLogin and the initial CPlayerPosition. Everything else the
        // server sends meanwhile (inventory, chunks) goes through the normal play handler.
        let play_login_id = CLogin::to_id(CURRENT_MC_VERSION);
        let mut saw_play_login = false;
        while !(saw_play_login && bot.teleports_confirmed > 0) {
            let pkt = bot.next_packet().await?;
            if pkt.id == play_login_id {
                saw_play_login = true;
            } else {
                bot.handle_play_packet(&pkt).await?;
            }
        }
        bot.send_packet(&SPlayerLoaded).await?;

        Ok(bot)
    }

    /// Handles every packet received so far, answering the ones a client has to answer.
    pub async fn poll_packets(&mut self) -> Result<(), String> {
        loop {
            match self.packets.try_recv() {
                Ok(packet) => self.handle_play_packet(&packet?).await?,
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    return Err(format!("Bot '{}' lost its connection", self.name));
                }
            }
        }
    }

    async fn handle_play_packet(&mut self, packet: &RawPacket) -> Result<(), String> {
        let mut payload = &packet.payload[..];
        match packet.id {
            id if id == CKeepAlive::to_id(CURRENT_MC_VERSION) => {
                let keep_alive_id = payload.get_i64_be().map_err(|err| read_error(id, &err))?;
                self.send_packet(&SKeepAlive { keep_alive_id }).await?;
            }
            id if id == CPlayerPosition::to_id(CURRENT_MC_VERSION) => {
                let teleport = CPlayerPosition::read(&mut payload, &CURRENT_MC_VERSION)
                    .map_err(|err| read_error(id, &err))?;
                if let Some(held) = &mut self.held_teleports {
                    held.push(teleport);
                } else {
                    self.confirm_teleport(&teleport).await?;
                }
            }
            id if id == CChunkData::to_id(CURRENT_MC_VERSION) => {
                let x = payload.get_i32_be().map_err(|err| read_error(id, &err))?;
                let z = payload.get_i32_be().map_err(|err| read_error(id, &err))?;
                self.chunks.insert((x, z));
            }
            id if id == CUnloadChunk::to_id(CURRENT_MC_VERSION) => {
                let unload = CUnloadChunk::read(&mut payload, &CURRENT_MC_VERSION)
                    .map_err(|err| read_error(id, &err))?;
                self.chunks.remove(&(unload.x, unload.z));
            }
            id if id == CChunkBatchEnd::to_id(CURRENT_MC_VERSION) => {
                self.send_packet(&SChunkBatch {
                    chunks_per_tick: CHUNKS_PER_TICK,
                })
                .await?;
            }
            id if id == CPingResponse::to_id(CURRENT_MC_VERSION) => {
                self.last_pong = payload.get_i64_be().map_err(|err| read_error(id, &err))?;
            }
            // A dimension change: the vanilla client drops its chunks and says it has loaded
            // the new level, which the server waits for before taking actions again.
            id if id == CRespawn::to_id(CURRENT_MC_VERSION) => {
                let respawn = CRespawn::read(&mut payload, &CURRENT_MC_VERSION)
                    .map_err(|err| read_error(id, &err))?;
                self.dimension = respawn
                    .player_spawn_info
                    .dimension
                    .minecraft_name
                    .to_string();
                self.chunks.clear();
                self.item_entities.clear();
                self.entities.clear();
                self.send_packet(&SPlayerLoaded).await?;
            }
            id if id == CSetHealth::to_id(CURRENT_MC_VERSION) => {
                let health = CSetHealth::read(&mut payload, &CURRENT_MC_VERSION)
                    .map_err(|err| read_error(id, &err))?;
                self.health = health.health;
                self.food = health.food.0;
                self.saturation = health.food_saturation;
            }
            id if id == CGameEvent::to_id(CURRENT_MC_VERSION) => {
                let event = payload.get_u8().map_err(|err| read_error(id, &err))?;
                match event {
                    GAME_EVENT_WIN_GAME => self.won_game = true,
                    GAME_EVENT_NO_RESPAWN_BLOCK => self.no_respawn_block = true,
                    _ => {}
                }
            }
            id if id == CPlayDisconnect::to_id(CURRENT_MC_VERSION) => {
                self.disconnect_reason = Some(packet.payload.clone());
            }
            id if [
                CSystemChatMessage::to_id(CURRENT_MC_VERSION),
                CPlayerChatMessage::to_id(CURRENT_MC_VERSION),
                CDisguisedChatMessage::to_id(CURRENT_MC_VERSION),
            ]
            .contains(&id) =>
            {
                self.chat.push(packet.payload.clone());
            }
            id if id == CBlockUpdate::to_id(CURRENT_MC_VERSION) => {
                let position =
                    BlockPos::from_i64(payload.get_i64_be().map_err(|err| read_error(id, &err))?);
                let state = payload.get_var_int().map_err(|err| read_error(id, &err))?.0;
                self.block_updates.push((position, state));
            }
            id if id == CMultiBlockUpdate::to_id(CURRENT_MC_VERSION) => {
                self.read_multi_block_update(id, &mut payload)?;
            }
            id => {
                self.handle_inventory_packet(id, &mut payload)
                    .map_err(|err| read_error(id, &err))?;
                self.handle_entity_packet(id, &mut payload)
                    .map_err(|err| read_error(id, &err))?;
            }
        }
        Ok(())
    }

    /// `SectionBlocksUpdate`: several changes in one section in one tick come as one packet.
    fn read_multi_block_update(&mut self, id: i32, payload: &mut &[u8]) -> Result<(), String> {
        // Several changes in one section in one tick come as one packet.
        let section = payload.get_i64_be().map_err(|err| read_error(id, &err))?;
        let (section_x, section_y, section_z) = (
            (section >> 42) as i32,
            ((section << 44) >> 44) as i32,
            ((section << 22) >> 42) as i32,
        );
        let count = payload.get_var_int().map_err(|err| read_error(id, &err))?.0;
        for _ in 0..count {
            let packed = payload
                .get_var_long()
                .map_err(|err| read_error(id, &err))?
                .0;
            let local = |shift: i64| ((packed >> shift) & 0xF) as i32;
            let position = BlockPos::new(
                section_x * 16 + local(8),
                section_y * 16 + local(0),
                section_z * 16 + local(4),
            );
            self.block_updates.push((position, (packed >> 12) as i32));
        }
        Ok(())
    }

    fn handle_inventory_packet(
        &mut self,
        id: i32,
        payload: &mut &[u8],
    ) -> Result<(), ReadingError> {
        if id == COpenScreen::to_id(CURRENT_MC_VERSION) {
            let container_id = payload.get_var_int()?.0;
            let menu_type = payload.get_var_int()?.0;
            self.inventory.open(container_id, menu_type);
        } else if id == CCloseContainer::to_id(CURRENT_MC_VERSION) {
            self.inventory.open_menu = None;
        } else if id == CSetContainerContent::to_id(CURRENT_MC_VERSION) {
            let container_id = payload.get_container_id(&CURRENT_MC_VERSION)?.0;
            let state_id = payload.get_var_int()?.0;
            let count = read_list_length(payload)?;
            let slots = (0..count)
                .map(|_| read_stack(payload))
                .collect::<Result<Vec<_>, _>>()?;
            let carried = read_stack(payload)?;
            self.inventory
                .set_contents(container_id, state_id, slots, carried);
        } else if id == CSetContainerSlot::to_id(CURRENT_MC_VERSION) {
            let container_id = payload.get_container_id(&CURRENT_MC_VERSION)?.0;
            let state_id = payload.get_var_int()?.0;
            let slot = payload.get_i16_be()?;
            let stack = read_stack(payload)?;
            if let Ok(slot) = usize::try_from(slot) {
                self.inventory.set_slot(container_id, state_id, slot, stack);
            }
        } else if id == CSetCursorItem::to_id(CURRENT_MC_VERSION) {
            self.inventory.carried = read_stack(payload)?;
        } else if id == CSetPlayerInventory::to_id(CURRENT_MC_VERSION) {
            let index = payload.get_var_int()?.0;
            let stack = read_stack(payload)?;
            if let Ok(index) = usize::try_from(index) {
                self.inventory.set_inventory_item(index, stack);
            }
        } else if id == CSetSelectedSlot::to_id(CURRENT_MC_VERSION)
            && let Ok(slot) = usize::try_from(payload.get_var_int()?.0)
        {
            self.inventory.select(slot);
        }
        Ok(())
    }

    /// Takes the shared flags and the pose from a `SetEntityData`. Stops at the first value of a
    /// type it can't skip; the entity tracker sends the flags and pose before any of those.
    fn read_entity_data(
        &mut self,
        entity_id: i32,
        payload: &mut &[u8],
    ) -> Result<(), ReadingError> {
        loop {
            let index = payload.get_u8()?;
            if index == DATA_END {
                return Ok(());
            }
            let value = match payload.get_var_int()?.0 {
                DATA_TYPE_BYTE => i32::from(payload.get_u8()?),
                DATA_TYPE_INT | DATA_TYPE_POSE => payload.get_var_int()?.0,
                DATA_TYPE_FLOAT => {
                    payload.get_f32_be()?;
                    continue;
                }
                DATA_TYPE_BOOLEAN => {
                    payload.get_bool()?;
                    continue;
                }
                _ => return Ok(()),
            };
            if let Some(entity) = self.entities.get_mut(&entity_id) {
                match index {
                    DATA_SHARED_FLAGS => entity.flags = value as u8,
                    DATA_POSE => entity.pose = value,
                    _ => {}
                }
            }
        }
    }

    fn handle_entity_packet(&mut self, id: i32, payload: &mut &[u8]) -> Result<(), ReadingError> {
        if is_counted_entity_packet(id) {
            let mut peek = *payload;
            let entity_id = peek.get_var_int()?.0;
            *self.entity_packets.entry((entity_id, id)).or_default() += 1;
        }
        if id == CSpawnEntity::to_id(CURRENT_MC_VERSION) {
            let spawn = CSpawnEntity::read_packet_data(&mut *payload, &CURRENT_MC_VERSION)?;
            if !payload.is_empty() {
                return Err(ReadingError::Message(format!(
                    "{} bytes left after the spawn entity packet",
                    payload.len()
                )));
            }
            if spawn.r#type.0 == i32::from(EntityType::ITEM.id) {
                self.item_entities.insert(spawn.entity_id.0, spawn.position);
                self.spawned_items += 1;
            }
            self.entities.insert(
                spawn.entity_id.0,
                TrackedEntity {
                    entity_type: spawn.r#type.0,
                    position: spawn.position,
                    flags: 0,
                    pose: 0,
                },
            );
        } else if id == CUpdateEntityPos::to_id(CURRENT_MC_VERSION)
            || id == CUpdateEntityPosRot::to_id(CURRENT_MC_VERSION)
        {
            let entity_id = payload.get_var_int()?.0;
            let delta = read_entity_delta(payload)?;
            if let Some(position) = self.item_entities.get_mut(&entity_id) {
                *position += delta;
            }
            if let Some(entity) = self.entities.get_mut(&entity_id) {
                entity.position += delta;
            }
        } else if id == CSetPassengers::to_id(CURRENT_MC_VERSION) {
            let vehicle = payload.get_var_int()?.0;
            let count = read_list_length(payload)?;
            let mut ids = Vec::with_capacity(count);
            for _ in 0..count {
                ids.push(payload.get_var_int()?.0);
            }
            self.passengers.insert(vehicle, ids);
        } else if id == CSetEntityMetadata::to_id(CURRENT_MC_VERSION) {
            let entity_id = payload.get_var_int()?.0;
            self.read_entity_data(entity_id, payload)?;
        } else if id == CEntityPositionSync::to_id(CURRENT_MC_VERSION) {
            let entity_id = payload.get_var_int()?.0;
            // Since 26.3 the position is a `PositionPath`; type 0 is `Linear`, a single position.
            // Pumpkin never sends `Stepped`.
            if payload.get_var_int()?.0 == 0 {
                let position = Vector3::new(
                    payload.get_f64_be()?,
                    payload.get_f64_be()?,
                    payload.get_f64_be()?,
                );
                if let Some(item) = self.item_entities.get_mut(&entity_id) {
                    *item = position;
                }
                if let Some(entity) = self.entities.get_mut(&entity_id) {
                    entity.position = position;
                }
            }
        } else if id == CRemoveEntities::to_id(CURRENT_MC_VERSION) {
            for _ in 0..read_list_length(payload)? {
                let entity_id = payload.get_var_int()?.0;
                self.item_entities.remove(&entity_id);
                self.entities.remove(&entity_id);
            }
        }
        Ok(())
    }

    /// Moves to where `teleport` puts the bot and confirms it.
    pub async fn confirm_teleport(&mut self, teleport: &CPlayerPosition) -> Result<(), String> {
        self.position = teleport.position;
        self.yaw = teleport.yaw;
        self.pitch = teleport.pitch;
        self.send_packet(&SConfirmTeleport {
            teleport_id: teleport.teleport_id,
            position: teleport.position,
            yaw: teleport.yaw,
            pitch: teleport.pitch,
        })
        .await?;
        // The vanilla client follows the confirmation with its new position, not on the ground.
        // Pumpkin moves the player's chunk view on that packet.
        self.send_packet(&SPlayerPositionRotation {
            position: teleport.position,
            yaw: teleport.yaw,
            pitch: teleport.pitch,
            collision: 0,
        })
        .await?;
        self.teleports_confirmed += 1;
        Ok(())
    }

    /// Sends a ping request. The server answers it in order with the bot's other packets, so the
    /// pong means everything sent before it has been handled.
    pub async fn send_ping(&mut self) -> Result<i64, String> {
        self.next_ping += 1;
        let payload = self.next_ping;
        self.send_packet(&SPlayPingRequest { payload }).await?;
        Ok(payload)
    }

    pub const fn has_pong(&self, ping: i64) -> bool {
        self.last_pong >= ping
    }

    /// The next block prediction sequence number, like vanilla's `BlockStatePredictionHandler`.
    pub const fn next_sequence(&mut self) -> i32 {
        self.sequence += 1;
        self.sequence
    }

    pub async fn move_to(&mut self, position: Vector3<f64>, on_ground: bool) -> Result<(), String> {
        self.position = position;
        self.send_packet(&SPlayerPosition {
            position,
            collision: u8::from(on_ground),
        })
        .await
    }

    pub async fn move_and_look(
        &mut self,
        position: Vector3<f64>,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> Result<(), String> {
        self.position = position;
        self.yaw = yaw;
        self.pitch = pitch;
        self.send_packet(&SPlayerPositionRotation {
            position,
            yaw,
            pitch,
            collision: u8::from(on_ground),
        })
        .await
    }

    /// Turns the bot's head so its eyes look at `target`, like `Entity.lookAt`.
    pub async fn look_at(&mut self, target: Vector3<f64>) -> Result<(), String> {
        let (yaw, pitch) = look_angles(self.position, target);
        self.yaw = yaw;
        self.pitch = pitch;
        self.send_packet(&SPlayerRotation {
            yaw,
            pitch,
            ground: true,
        })
        .await
    }

    /// Presses the drop key: one item, or the whole stack with the modifier held.
    pub async fn drop_item(&mut self, whole_stack: bool) -> Result<(), String> {
        let status = if whole_stack {
            Status::DropItemStack
        } else {
            Status::DropItem
        };
        self.send_player_action(status, BlockPos::new(0, 0, 0), FACE_DOWN, 0)
            .await?;
        // The client takes the item out of its own inventory (`LocalPlayer.drop` calls
        // `Inventory.removeFromSelected`) and the server sends no slot update for it.
        let held = &mut self.inventory.items[self.inventory.selected];
        held.item_count = if whole_stack {
            0
        } else {
            held.item_count.saturating_sub(1)
        };
        Ok(())
    }

    /// Swings the main hand, which the vanilla client sends with every attack.
    pub async fn swing(&mut self) -> Result<(), String> {
        self.send_packet(&SSwingArm { hand: VarInt(0) }).await
    }

    /// Clicks Respawn on the death screen.
    pub async fn respawn(&mut self) -> Result<(), String> {
        self.send_packet(&SClientCommand::new(VarInt(
            SClientCommand::PERFORM_RESPAWN,
        )))
        .await
    }

    /// The tracked entities of `entity_type`, by entity id.
    pub fn entities_of_type(&self, entity_type: &EntityType) -> Vec<(i32, Vector3<f64>)> {
        self.entities
            .iter()
            .filter(|(_, entity)| entity.entity_type == i32::from(entity_type.id))
            .map(|(id, entity)| (*id, entity.position))
            .collect()
    }

    /// Sends an unsigned chat message, as an offline-mode client does.
    pub async fn send_chat(&mut self, message: &str) -> Result<(), String> {
        self.send_packet(&SChatMessage {
            message,
            timestamp: 0,
            salt: 0,
            signature: None,
            message_count: VarInt(0),
            acknowledged: &[0; 3],
            checksum: 0,
        })
        .await
    }

    /// Whether a chat message whose text contains `text` arrived.
    pub fn saw_chat_containing(&self, text: &str) -> bool {
        self.chat
            .iter()
            .any(|payload| String::from_utf8_lossy(payload).contains(text))
    }

    pub async fn send_command(&mut self, command: &str) -> Result<(), String> {
        self.send_packet(&SChatCommand { command }).await
    }

    pub async fn set_held_item(&mut self, slot: i16) -> Result<(), String> {
        if let Ok(selected) = usize::try_from(slot) {
            self.inventory.select(selected);
        }
        self.send_packet(&SSetHeldItem { slot }).await
    }

    /// Sends the movement keys held this tick, as `SPlayerInput` bits.
    pub async fn send_input(&mut self, input: i8) -> Result<(), String> {
        self.send_packet(&SPlayerInput { input }).await
    }

    pub async fn send_player_command(
        &mut self,
        entity_id: i32,
        action: Action,
    ) -> Result<(), String> {
        self.send_packet(&SPlayerCommand {
            entity_id: VarInt(entity_id),
            action,
            jump_boost: VarInt(0),
        })
        .await
    }

    pub async fn use_block(&mut self, position: BlockPos, sequence: i32) -> Result<(), String> {
        self.send_packet(&SUseItemOn {
            hand: VarInt(0),
            position,
            face: VarInt(1),
            cursor_pos: Vector3::new(0.5, 0.5, 0.5),
            inside_block: false,
            is_against_world_border: false,
            sequence: VarInt(sequence),
        })
        .await
    }

    /// Right-clicks an entity with the main hand. `location` is where the click hit it, relative
    /// to its position, as the vanilla client sends it.
    pub async fn interact_entity(
        &mut self,
        entity_id: i32,
        location: Vector3<f64>,
    ) -> Result<(), String> {
        self.send_packet(&SInteract {
            entity_id: VarInt(entity_id),
            r#type: VarInt(ActionType::InteractAt as i32),
            target_position: Some(location),
            hand: Some(VarInt(0)),
            sneaking: false,
        })
        .await
    }

    /// Right-clicks an entity with the main hand without a hit position, the packet that boards
    /// boats and mounts.
    pub async fn use_entity(&mut self, entity_id: i32) -> Result<(), String> {
        self.send_packet(&SInteract {
            entity_id: VarInt(entity_id),
            r#type: VarInt(ActionType::Interact as i32),
            target_position: None,
            hand: Some(VarInt(0)),
            sneaking: false,
        })
        .await
    }

    /// Right-clicks with the main hand without aiming at a block, like throwing an eye of ender
    /// or drawing a bow.
    pub async fn use_item(&mut self, sequence: i32) -> Result<(), String> {
        self.send_packet(&SUseItem {
            hand: VarInt(0),
            sequence: VarInt(sequence),
            yaw: self.yaw,
            pitch: self.pitch,
        })
        .await
    }

    /// Lets go of the use key, like loosing a drawn bow.
    pub async fn release_use_item(&mut self) -> Result<(), String> {
        self.send_player_action(
            Status::ReleaseItemInUse,
            BlockPos::new(0, 0, 0),
            FACE_DOWN,
            0,
        )
        .await
    }

    pub async fn attack_entity(&mut self, entity_id: i32) -> Result<(), String> {
        self.send_packet(&SAttack {
            entity_id: VarInt(entity_id),
        })
        .await
    }

    /// Middle-clicks an entity, like the vanilla client's pick block on an entity.
    pub async fn pick_item_from_entity(&mut self, entity_id: i32) -> Result<(), String> {
        self.send_packet(&SPickItemFromEntity {
            id: VarInt(entity_id),
            include_data: false,
        })
        .await
    }

    /// Left-clicks an entity in spectator mode.
    pub async fn spectate_entity(&mut self, entity_id: i32) -> Result<(), String> {
        // Vanilla `OPTIONAL_VAR_INT` sends the id plus one; 0 means no target.
        self.send_packet(&SSpectateEntity {
            target: VarInt(entity_id + 1),
        })
        .await
    }

    pub async fn start_digging(&mut self, position: BlockPos, sequence: i32) -> Result<(), String> {
        self.send_player_action(Status::StartedDigging, position, FACE_UP, sequence)
            .await
    }

    pub async fn finish_digging(
        &mut self,
        position: BlockPos,
        sequence: i32,
    ) -> Result<(), String> {
        self.send_player_action(Status::FinishedDigging, position, FACE_UP, sequence)
            .await
    }

    async fn send_player_action(
        &mut self,
        status: Status,
        position: BlockPos,
        face: u8,
        sequence: i32,
    ) -> Result<(), String> {
        self.send_packet(&SPlayerAction {
            status: VarInt(status as i32),
            position,
            face,
            sequence: VarInt(sequence),
        })
        .await
    }

    /// Ticks the client spends digging `block` with the held item before it sends
    /// `STOP_DESTROY_BLOCK`: it adds `BlockBehaviour.getDestroyProgress` every tick until the sum
    /// reaches 1. 0 when the first tick's progress already breaks it, so the client sends only
    /// `START_DESTROY_BLOCK`. Assumes the bot stands on the ground, dry, with no effects or
    /// Efficiency. `None` when the bot can never break it.
    pub fn destroy_ticks(&self, block: &'static Block) -> Option<u32> {
        let state = block.default_state;
        if state.hardness < 0.0 {
            return None;
        }
        let held = self.inventory.held_item();
        let modifier = if !state.tool_required() || held.is_correct_for_drops(block) {
            30.0
        } else {
            100.0
        };
        let progress_per_tick = held.get_speed(block) / state.hardness / modifier;
        if progress_per_tick <= 0.0 {
            return None;
        }
        if progress_per_tick >= 1.0 {
            return Some(0);
        }
        let mut progress = 0.0;
        let mut ticks = 0;
        while progress < 1.0 {
            progress += progress_per_tick;
            ticks += 1;
        }
        Some(ticks)
    }

    /// Clicks `slot` of the menu on screen without predicting the result. The packet lists no
    /// changed slots and an empty cursor; `ServerGamePacketListenerImpl.handleContainerClick`
    /// takes those as the client's state and sends every slot and the cursor where its own state
    /// differs. So until the server answers, the bot's cursor is the empty one it claimed.
    pub async fn click_slot(
        &mut self,
        slot: usize,
        button: i8,
        mode: SlotActionType,
    ) -> Result<(), String> {
        let menu = self.inventory.menu();
        let packet = SClickSlot {
            sync_id: VarInt(menu.container_id),
            revision: VarInt(menu.state_id),
            slot: slot as i16,
            button,
            mode,
            length_of_array: VarInt(0),
            array_of_changed_slots: Vec::new(),
            carried_item: OptionalItemStackHash(None),
        };
        self.inventory.carried = ItemStack::EMPTY.clone();
        self.send_packet(&packet).await
    }

    /// Closes the menu on screen, like pressing Escape.
    pub async fn close_container(&mut self) -> Result<(), String> {
        let window_id = VarInt(self.inventory.menu().container_id);
        self.inventory.open_menu = None;
        self.send_packet(&SCloseContainer { window_id }).await
    }
}

/// The yaw and pitch, in degrees, that point the eyes of a player standing at `feet` at `target`
/// (vanilla `Entity.lookAt`).
#[expect(clippy::cast_possible_truncation, reason = "angles fit an f32")]
pub fn look_angles(feet: Vector3<f64>, target: Vector3<f64>) -> (f32, f32) {
    let dx = target.x - feet.x;
    let dy = target.y - (feet.y + EYE_HEIGHT);
    let dz = target.z - feet.z;
    let horizontal = dx.hypot(dz);
    let yaw = (dz.atan2(dx).to_degrees() - 90.0) as f32;
    let pitch = (-dy.atan2(horizontal).to_degrees()) as f32;
    (yaw, pitch)
}

pub async fn send_framed_packet<P: ClientPacket + MultiVersionJavaPacket>(
    writer: &mut TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    packet: &P,
) -> Result<(), String> {
    let packet_id = P::to_id(CURRENT_MC_VERSION);
    let mut buf = Vec::new();
    buf.write_var_int(&VarInt(packet_id))
        .map_err(|err| format!("Failed to write packet id {packet_id}: {err:?}"))?;
    packet
        .write_packet_data(&mut buf, &CURRENT_MC_VERSION)
        .map_err(|err| format!("Failed to serialize packet {packet_id}: {err:?}"))?;
    writer
        .write_packet(Bytes::from(buf))
        .await
        .map_err(|err| format!("Failed to write packet to socket: {err}"))?;
    writer
        .flush()
        .await
        .map_err(|err| format!("Failed to flush socket: {err}"))
}
