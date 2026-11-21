//! Bedrock tests: a server with Bedrock on, a [`BedrockBot`] joining it over `NetherNet` the way
//! a 1.26.60 client does, and a Java [`HeadlessBot`] next to it for the cross-play checks.

mod bot;
mod transport;

use std::{
    collections::HashMap, future::Future, net::SocketAddr, path::Path, sync::Arc, time::Duration,
};

use pumpkin_core::{PumpkinServer, net::bedrock::BedrockClient, server::Server};
use pumpkin_data::entity::{EntityPose, EntityType};
use pumpkin_protocol::{
    bedrock::{
        client::set_actor_data::{entity_data_flag, entity_data_key},
        server::player_auth_input::InputData,
    },
    java::server::play::{Action as PlayerCommandAction, SPlayerInput},
};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{bot::HeadlessBot, harness};
pub use bot::BedrockBot;

const TICK: Duration = Duration::from_millis(50);
/// Plenty for a packet to cross a loopback WebRTC connection and be handled in a tick.
const MAX_WAIT_TICKS: usize = 200;
const BEDROCK_NAME: &str = "BedrockBot";
const JAVA_NAME: &str = "JavaBot";
/// `bedrock.max_players` on the test server: room for the Java and Bedrock bots in `mixed_play`,
/// and for the main Bedrock bot plus one more in `login_checks`.
const MAX_PLAYERS: u32 = 2;

type BedrockClients = Arc<Mutex<HashMap<SocketAddr, Arc<BedrockClient>>>>;

/// A test server with Java and Bedrock listeners on loopback, ticked by the tests.
pub struct BedrockTestServer {
    pub server: Arc<Server>,
    pub java_addr: SocketAddr,
    pub signaling_addr: SocketAddr,
    /// The server's open Bedrock clients, as `PumpkinServer` keeps them.
    pub bedrock_clients: BedrockClients,
    cancel: CancellationToken,
    tasks: TaskTracker,
}

impl BedrockTestServer {
    pub async fn start(world_dir: &Path) -> Result<Self, String> {
        let identity_key = world_dir.join("nethernet-key.der");
        let server = harness::create_test_server_with(world_dir, |advanced| {
            let bedrock = &mut advanced.networking.bedrock;
            bedrock.enabled = true;
            bedrock.online_mode = false;
            bedrock.max_players = MAX_PLAYERS;
            bedrock.authentication.enabled = false;
            bedrock.nethernet.enabled = true;
            bedrock.nethernet.address = SocketAddr::from(([127, 0, 0, 1], 0));
            bedrock.nethernet.identity_key = identity_key;
        })
        .await?;

        let ice = PumpkinServer::bind_bedrock_ice(&server).await;
        if ice.is_none() {
            return Err("could not bind the Bedrock ICE socket".to_string());
        }
        let listener = PumpkinServer::bind_nethernet(&server, ice)
            .await
            .ok_or("could not bind the NetherNet signaling listener")?;
        let signaling_addr = listener.local_addr();

        let java_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|err| format!("Failed to bind local TCP listener: {err}"))?;
        let java_addr = java_listener
            .local_addr()
            .map_err(|err| format!("Failed to query local TCP address: {err}"))?;

        let cancel = CancellationToken::new();
        let tasks = TaskTracker::new();
        harness::spawn_java_listener(java_listener, server.clone(), cancel.clone(), &tasks);

        let accept_server = server.clone();
        let accept_cancel = cancel.clone();
        let accept_tasks = tasks.clone();
        let bedrock_clients: BedrockClients = Arc::new(Mutex::new(HashMap::new()));
        let clients = bedrock_clients.clone();
        tasks.spawn(async move {
            loop {
                tokio::select! {
                    () = accept_cancel.cancelled() => break,
                    accepted = listener.accept() => {
                        let Some((session, address)) = accepted else { break };
                        PumpkinServer::accept_nethernet_session(
                            &accept_server,
                            session,
                            address,
                            &clients,
                            &accept_tasks,
                        )
                        .await;
                    }
                }
            }
        });

        Ok(Self {
            server,
            java_addr,
            signaling_addr,
            bedrock_clients,
            cancel,
            tasks,
        })
    }

    /// Ticks the server every 50 ms until `future` finishes, for steps that need the server
    /// running meanwhile, such as joining.
    pub async fn tick_while<F: Future>(&self, future: F) -> F::Output {
        tokio::pin!(future);
        loop {
            tokio::select! {
                output = &mut future => return output,
                () = tokio::time::sleep(TICK) => self.server.tick(),
            }
        }
    }

    pub async fn stop(self) {
        self.cancel.cancel();
        for player in self.server.get_all_players() {
            player.remove().await;
            self.server.remove_player(&player);
        }
        self.tasks.close();
        self.tasks.wait().await;
        self.server.shutdown().await;
    }
}

/// One tick, then both bots read what arrived.
async fn tick(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: Option<&mut HeadlessBot>,
) -> Result<(), String> {
    // A real client sends `PlayerAuthInput` every tick; the server times out a silent one.
    bedrock.send_input(bedrock.position, &[]).await?;
    server.server.tick();
    tokio::time::sleep(TICK).await;
    bedrock.poll_packets().await?;
    if let Some(java) = java {
        java.poll_packets().await?;
    }
    Ok(())
}

/// Ticks until `done` holds, or fails naming `what`.
async fn tick_until(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    mut java: Option<&mut HeadlessBot>,
    what: &str,
    done: impl Fn(&BedrockBot, Option<&HeadlessBot>) -> bool,
) -> Result<(), String> {
    for _ in 0..MAX_WAIT_TICKS {
        if done(bedrock, java.as_deref()) {
            return Ok(());
        }
        tick(server, bedrock, java.as_deref_mut()).await?;
    }
    if done(bedrock, java.as_deref()) {
        Ok(())
    } else {
        Err(format!("waited {MAX_WAIT_TICKS} ticks for {what}"))
    }
}

/// The checks of one scenario: each one runs even if an earlier one failed, so a run lists every
/// gap at once.
struct Checks {
    scenario: &'static str,
    results: Vec<(&'static str, Result<(), String>)>,
}

impl Checks {
    const fn new(scenario: &'static str) -> Self {
        Self {
            scenario,
            results: Vec::new(),
        }
    }

    fn record(&mut self, name: &'static str, result: Result<(), String>) {
        self.results.push((name, result));
    }

    fn finish(self, summary_lines: &mut Vec<String>) -> Result<(), String> {
        let failed: Vec<_> = self
            .results
            .iter()
            .filter_map(|(name, result)| result.as_ref().err().map(|err| format!("{name}: {err}")))
            .collect();
        for (name, result) in &self.results {
            let mark = if result.is_ok() { "ok" } else { "FAILED" };
            summary_lines.push(format!("bedrock:{}:{name} {mark}", self.scenario));
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "bedrock:{} failed {} of {} checks:\n  {}",
                self.scenario,
                failed.len(),
                self.results.len(),
                failed.join("\n  ")
            ))
        }
    }
}

pub async fn run_bedrock_tests(summary_lines: &mut Vec<String>) -> Result<(), String> {
    let world = tempfile::tempdir().map_err(|err| format!("temp dir failed: {err}"))?;
    let server = BedrockTestServer::start(world.path()).await?;
    let result = Box::pin(run_scenarios(&server, summary_lines)).await;
    server.stop().await;
    result
}

async fn run_scenarios(
    server: &BedrockTestServer,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    // The Java bot joins first, so the Bedrock join also checks how a new Bedrock player is
    // announced to players already online.
    let mut java = server
        .tick_while(HeadlessBot::connect_and_join(server.java_addr, JAVA_NAME))
        .await?;
    let mut bedrock = server
        .tick_while(Box::pin(BedrockBot::connect_and_join(
            server.signaling_addr,
            BEDROCK_NAME,
        )))
        .await
        .map_err(|err| format!("bedrock:join_and_spawn: {err}"))?;

    let mut join = Checks::new("join_and_spawn");
    join.record(
        "server has the player",
        if server.server.get_player_by_name(BEDROCK_NAME).is_some() {
            Ok(())
        } else {
            Err(format!("no player named {BEDROCK_NAME} on the server"))
        },
    );
    join.record(
        "chunks around spawn",
        tick_until(
            server,
            &mut bedrock,
            Some(&mut java),
            "9 chunks",
            |bot, _| bot.chunks.len() >= 9,
        )
        .await,
    );
    join.finish(summary_lines)?;

    let mut result = Box::pin(run_mixed_play(server, &mut bedrock, java, summary_lines)).await;
    if result.is_ok() {
        result = Box::pin(run_login_checks(server, &mut bedrock, summary_lines)).await;
    }
    bedrock.disconnect().await;
    if result.is_ok() {
        result = Box::pin(run_bad_packet_checks(server, summary_lines)).await;
    }
    result
}

/// A packet id the protocol doesn't have (ids are 10 bits; the highest real one is in the 300s).
const UNKNOWN_PACKET_ID: i32 = 1000;

/// What the server does with packets it can't use: unknown ids are ignored, a packet that
/// fails to decode ends the connection.
async fn run_bad_packet_checks(
    server: &BedrockTestServer,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    const NAME: &str = "BadPacketBot";
    let mut checks = Checks::new("bad_packets");
    let mut bot = server
        .tick_while(Box::pin(BedrockBot::connect_and_join(
            server.signaling_addr,
            NAME,
        )))
        .await
        .map_err(|err| format!("bedrock:bad_packets: {NAME} could not join: {err}"))?;

    for _ in 0..3 {
        bot.send_raw(UNKNOWN_PACKET_ID, &[1, 2, 3]).await?;
    }
    for _ in 0..10 {
        tick(server, &mut bot, None).await?;
    }
    checks.record(
        "unknown packets don't disconnect",
        if bot.disconnect.is_none() && server.server.get_player_by_name(NAME).is_some() {
            Ok(())
        } else {
            Err(format!("{NAME} was dropped: {:?}", bot.disconnect))
        },
    );

    // Two bytes of a packet that starts with five floats.
    bot.send_raw(bot::PLAYER_AUTH_INPUT_ID, &[0, 0]).await?;
    let mut kicked = Err("no Disconnect after a malformed PlayerAuthInput".to_string());
    for _ in 0..MAX_WAIT_TICKS {
        server.server.tick();
        tokio::time::sleep(TICK).await;
        let _ = bot.poll_packets().await;
        if bot.disconnect.is_some() {
            kicked = Ok(());
            break;
        }
    }
    checks.record("a malformed packet disconnects", kicked);
    let mut dropped = Err(format!("{NAME} stayed on the server"));
    for _ in 0..MAX_WAIT_TICKS {
        if server.server.get_player_by_name(NAME).is_none() {
            dropped = Ok(());
            break;
        }
        server.server.tick();
        tokio::time::sleep(TICK).await;
    }
    checks.record("the server drops the player", dropped);
    bot.disconnect().await;

    // Every bot so far has left: refused, kicked in game, kicked for a bad packet, or closed by
    // the bot. None of them may leave a client behind.
    let mut released = Err("clients are still listed".to_string());
    for _ in 0..MAX_WAIT_TICKS {
        let open = server.bedrock_clients.lock().await.len();
        if open == 0 {
            released = Ok(());
            break;
        }
        released = Err(format!("{open} closed Bedrock clients are still listed"));
        server.server.tick();
        tokio::time::sleep(TICK).await;
    }
    checks.record("closed sessions are released", released);
    checks.finish(summary_lines)
}

/// What a Java login is refused for, on the Bedrock login path. The main Bedrock bot is online
/// alone when this starts.
async fn run_login_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut checks = Checks::new("login_checks");

    checks.record(
        "a second login with an online name is refused",
        expect_refused(server, bedrock, BEDROCK_NAME, bot::xuid_for("SomeoneElse")).await,
    );
    checks.record(
        "a second login with an online UUID is refused",
        expect_refused(server, bedrock, "OtherName", bedrock.xuid).await,
    );
    checks.record(
        "the first player is still online",
        if server.server.get_player_by_name(BEDROCK_NAME).is_some() {
            Ok(())
        } else {
            Err(format!("{BEDROCK_NAME} was dropped by the refused logins"))
        },
    );

    run_console(server, "whitelist on");
    checks.record(
        "an unlisted player is refused with the whitelist on",
        expect_refused(server, bedrock, "UnlistedBot", bot::xuid_for("UnlistedBot")).await,
    );
    run_console(server, "whitelist off");

    // Fills the server: the main bot plus this one.
    let mut banned = server
        .tick_while(Box::pin(BedrockBot::connect_and_join(
            server.signaling_addr,
            "BannedBot",
        )))
        .await
        .map_err(|err| format!("bedrock:login_checks: BannedBot could not join: {err}"))?;
    checks.record(
        "a login past bedrock.max_players is refused",
        expect_refused(server, bedrock, "LateBot", bot::xuid_for("LateBot")).await,
    );
    run_console(server, "ban BannedBot");
    checks.record(
        "a banned player is kicked",
        tick_until(
            server,
            bedrock,
            None,
            "BannedBot to leave the server",
            |_, _| server.server.get_player_by_name("BannedBot").is_none(),
        )
        .await,
    );
    let _ = banned.poll_packets().await;
    checks.record(
        "a kicked player is told why",
        match &banned.disconnect {
            Some(message) if !message.is_empty() => Ok(()),
            Some(_) => Err("the Disconnect had no message".to_string()),
            None => Err("BannedBot got no Disconnect".to_string()),
        },
    );
    banned.disconnect().await;
    checks.record(
        "a banned player is refused",
        expect_refused(server, bedrock, "BannedBot", bot::xuid_for("BannedBot")).await,
    );
    run_console(server, "pardon BannedBot");

    // Last: it kicks the main bot too, since every bot comes from loopback.
    run_console(server, "ban-ip 127.0.0.1");
    checks.record(
        "a player from a banned IP is refused",
        expect_refused(server, bedrock, "IpBot", bot::xuid_for("IpBot")).await,
    );
    run_console(server, "pardon-ip 127.0.0.1");

    checks.finish(summary_lines)
}

/// Joins as `name` with `xuid` and expects the server to disconnect the bot before it spawns.
/// `online` keeps sending input meanwhile, so the server doesn't time it out.
async fn expect_refused(
    server: &BedrockTestServer,
    online: &mut BedrockBot,
    name: &str,
    xuid: u64,
) -> Result<(), String> {
    let mut join = Box::pin(BedrockBot::connect_and_join_as(
        server.signaling_addr,
        name,
        xuid,
    ));
    let result = loop {
        tokio::select! {
            result = &mut join => break result,
            () = tokio::time::sleep(TICK) => {
                // The online bot may be kicked on the way (the IP ban); that is not this check's
                // business.
                if online.disconnect.is_none() {
                    let _ = online.send_input(online.position, &[]).await;
                    let _ = online.poll_packets().await;
                }
                server.server.tick();
            }
        }
    };
    match result {
        Ok(bot) => {
            bot.disconnect().await;
            Err(format!("{name} joined"))
        }
        Err(err) if err.contains("was disconnected") => Ok(()),
        Err(err) => Err(format!(
            "{name} failed to join without being refused: {err}"
        )),
    }
}

/// Cross-play: what one edition does, the other sees.
async fn run_mixed_play(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    mut java: HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut checks = Checks::new("mixed_play");
    let player_type = i32::from(EntityType::PLAYER.id);

    checks.record(
        "bedrock sees the java player",
        tick_until(
            server,
            bedrock,
            Some(&mut java),
            "an AddPlayer for the Java bot",
            |bot, _| bot.players.values().any(|player| player.name == JAVA_NAME),
        )
        .await,
    );
    checks.record(
        "java sees the bedrock player",
        tick_until(
            server,
            bedrock,
            Some(&mut java),
            "a player entity spawn on the Java bot",
            |_, java| {
                java.is_some_and(|java| {
                    java.entities
                        .values()
                        .any(|entity| entity.entity_type == player_type)
                })
            },
        )
        .await,
    );

    run_chat_checks(server, bedrock, &mut java, &mut checks).await?;
    run_move_checks(server, bedrock, &mut java, &mut checks).await?;
    run_pose_checks(server, bedrock, &mut java, &mut checks).await?;
    run_mount_checks(server, bedrock, &mut java, &mut checks).await?;
    // Before `run_block_checks`, which puts both players in creative.
    run_fall_checks(server, bedrock, &mut java, &mut checks).await?;
    run_block_checks(server, bedrock, &mut java, &mut checks).await?;
    run_command_checks(server, bedrock, &mut java, &mut checks).await?;

    drop(java);
    checks.record(
        "bedrock sees java leave",
        tick_until(
            server,
            bedrock,
            None,
            "the Java player to be removed on the Bedrock bot",
            |bot, _| !bot.players.values().any(|player| player.name == JAVA_NAME),
        )
        .await,
    );

    checks.finish(summary_lines)
}

/// Chat both ways.
async fn run_chat_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    bedrock.chat("hello from bedrock").await?;
    checks.record(
        "bedrock chat reaches java",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock chat line on the Java bot",
            |_, java| java.is_some_and(|java| java.saw_chat_containing("hello from bedrock")),
        )
        .await,
    );

    java.send_chat("hello from java").await?;
    checks.record(
        "java chat reaches bedrock",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Java chat line on the Bedrock bot",
            |bot, _| {
                bot.texts.iter().any(|text| {
                    text.message.contains("hello from java")
                        || text
                            .parameters
                            .iter()
                            .any(|parameter| parameter.contains("hello from java"))
                })
            },
        )
        .await,
    );
    Ok(())
}

/// Each bot walks two blocks; the other must see it arrive.
async fn run_move_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player_type = i32::from(EntityType::PLAYER.id);
    let start = bedrock.position;
    let target = start + Vector3::new(2.0, 0.0, 0.0);
    for step in 1..=4u8 {
        let position = start + Vector3::new(f32::from(step) * 0.5, 0.0, 0.0);
        bedrock.send_input(position, &[]).await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "java sees bedrock move",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock player's new position on the Java bot",
            |_, java| {
                java.is_some_and(|java| {
                    java.entities.values().any(|entity| {
                        entity.entity_type == player_type
                            && (entity.position.x - f64::from(target.x)).abs() < 0.6
                    })
                })
            },
        )
        .await,
    );

    let java_start = java.position;
    let java_target = java_start + Vector3::new(0.0, 0.0, 2.0);
    for step in 1..=4u8 {
        let position = java_start + Vector3::new(0.0, 0.0, f64::from(step) * 0.5);
        java.move_to(position, true).await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "bedrock sees java move",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Java player's new position on the Bedrock bot",
            |bot, _| {
                bot.players.values().any(|player| {
                    player.name == JAVA_NAME
                        && (f64::from(player.position.z) - java_target.z).abs() < 0.6
                })
            },
        )
        .await,
    );
    Ok(())
}

/// Java's shared entity flag bits for sneaking and sprinting.
const JAVA_FLAG_SNEAKING: u8 = 1 << 1;
const JAVA_FLAG_SPRINTING: u8 = 1 << 3;

/// Sneaking, sprinting and crawling show up for players of the other edition.
async fn run_pose_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let entity_id_of = |name: &str| -> Result<i32, String> {
        server
            .server
            .get_player_by_name(name)
            .map(|player| player.entity_id())
            .ok_or_else(|| format!("{name} is not on the server"))
    };
    let bedrock_id = entity_id_of(BEDROCK_NAME)?;
    let java_id = entity_id_of(JAVA_NAME)?;
    let java_runtime_id = bedrock
        .player_runtime_id(JAVA_NAME)
        .ok_or("the Bedrock bot has no AddPlayer for the Java bot")?;
    let ids = PoseIds {
        bedrock_entity: bedrock_id,
        java_entity: java_id,
        java_runtime: java_runtime_id,
    };
    let bedrock_feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let bedrock_block = BlockPos::floored(
        f64::from(bedrock.position.x),
        bedrock_feet + 0.5,
        f64::from(bedrock.position.z),
    );
    let java_block = BlockPos::floored(java.position.x, java.position.y + 0.5, java.position.z);

    // The bots spawn in a tree on the test seed. Clear the leaves around them so that they
    // have room to stand.
    for pos in [bedrock_block, java_block] {
        let (x, y, z) = (pos.0.x, pos.0.y, pos.0.z);
        run_console(
            server,
            &format!(
                "fill {} {y} {} {} {} {} air",
                x - 1,
                z - 1,
                x + 1,
                y + 2,
                z + 1
            ),
        );
    }
    tick(server, bedrock, Some(&mut *java)).await?;

    run_bedrock_pose_checks(server, bedrock, java, checks, &ids).await?;
    run_java_pose_checks(server, bedrock, java, checks, &ids).await?;
    let blocks = [bedrock_block, java_block];
    run_crawl_checks(server, bedrock, java, checks, &ids, blocks).await?;
    run_swim_checks(server, bedrock, java, checks, &ids, blocks).await
}

/// The ids the pose checks look the players up by.
struct PoseIds {
    /// The Bedrock player's entity id on the server and the Java bot.
    bedrock_entity: i32,
    java_entity: i32,
    /// The Java player's runtime id on the Bedrock bot.
    java_runtime: u64,
}

/// The Bedrock player's shared flags and pose, as the Java bot last saw them.
fn seen_by_java(java: Option<&HeadlessBot>, ids: &PoseIds) -> Option<(u8, i32)> {
    java.and_then(|java| java.entities.get(&ids.bedrock_entity))
        .map(|entity| (entity.flags, entity.pose))
}

/// Sends one `PlayerAuthInput` standing where the bot is, with these `InputData` flags.
async fn send_bedrock_flags(bedrock: &mut BedrockBot, flags: &[InputData]) -> Result<(), String> {
    let mut wire = vec![bot::INPUT_VERTICAL_COLLISION];
    wire.extend(flags.iter().map(|flag| *flag as i32));
    bedrock
        .send_input_with_flags(bedrock.position, &wire, &[])
        .await
}

/// A Bedrock player's sneaking and sprinting, as a Java player sees them.
async fn run_bedrock_pose_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    ids: &PoseIds,
) -> Result<(), String> {
    let crouching = EntityPose::Crouching as i32;
    let standing = EntityPose::Standing as i32;
    send_bedrock_flags(bedrock, &[InputData::StartSneaking, InputData::Sneaking]).await?;
    checks.record(
        "java sees bedrock sneak",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the sneaking flag and crouching pose on the Bedrock player",
            |_, java| {
                seen_by_java(java, ids).is_some_and(|(flags, pose)| {
                    flags & JAVA_FLAG_SNEAKING != 0 && pose == crouching
                })
            },
        )
        .await,
    );
    send_bedrock_flags(bedrock, &[InputData::StopSneaking]).await?;
    checks.record(
        "java sees bedrock stop sneaking",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock player standing again",
            |_, java| {
                seen_by_java(java, ids).is_some_and(|(flags, pose)| {
                    flags & JAVA_FLAG_SNEAKING == 0 && pose == standing
                })
            },
        )
        .await,
    );
    send_bedrock_flags(bedrock, &[InputData::StartSprinting, InputData::Sprinting]).await?;
    checks.record(
        "java sees bedrock sprint",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the sprinting flag on the Bedrock player",
            |_, java| {
                seen_by_java(java, ids).is_some_and(|(flags, _)| flags & JAVA_FLAG_SPRINTING != 0)
            },
        )
        .await,
    );
    send_bedrock_flags(bedrock, &[InputData::StopSprinting]).await?;
    tick(server, bedrock, Some(java)).await
}

/// A Java player's sneaking and sprinting, as a Bedrock player sees them.
async fn run_java_pose_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    ids: &PoseIds,
) -> Result<(), String> {
    let java_runtime_id = ids.java_runtime;
    java.send_input(SPlayerInput::SNEAK).await?;
    checks.record(
        "bedrock sees java sneak",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the SNEAKING actor flag on the Java player",
            |bot, _| bot.actor_flag(java_runtime_id, entity_data_flag::SNEAKING),
        )
        .await,
    );
    java.send_input(0).await?;
    checks.record(
        "bedrock sees java stop sneaking",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the SNEAKING actor flag cleared on the Java player",
            |bot, _| !bot.actor_flag(java_runtime_id, entity_data_flag::SNEAKING),
        )
        .await,
    );
    java.send_player_command(ids.java_entity, PlayerCommandAction::StartSprinting)
        .await?;
    checks.record(
        "bedrock sees java sprint",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the SPRINTING actor flag on the Java player",
            |bot, _| bot.actor_flag(java_runtime_id, entity_data_flag::SPRINTING),
        )
        .await,
    );
    java.send_player_command(ids.java_entity, PlayerCommandAction::StopSprinting)
        .await?;
    tick(server, bedrock, Some(java)).await
}

/// A block just above each player's head leaves room only to crawl. The server picks the pose
/// from the space around the player, for both editions.
async fn run_crawl_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    ids: &PoseIds,
    [bedrock_block, java_block]: [BlockPos; 2],
) -> Result<(), String> {
    let swimming = EntityPose::Swimming as i32;
    let java_runtime_id = ids.java_runtime;
    let above_bedrock = bedrock_block.up();
    let above_java = java_block.up();
    let set_blocks = |block: &str| {
        for pos in [above_bedrock, above_java] {
            run_console(
                server,
                &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
            );
        }
    };
    set_blocks("stone");
    checks.record(
        "java sees bedrock crawl",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the swimming pose on the Bedrock player under a block",
            |_, java| seen_by_java(java, ids).is_some_and(|(_, pose)| pose == swimming),
        )
        .await,
    );
    checks.record(
        "bedrock sees java crawl",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the CRAWLING actor flag on the Java player under a block",
            |bot, _| bot.actor_flag(java_runtime_id, entity_data_flag::CRAWLING),
        )
        .await,
    );
    set_blocks("air");
    checks.record(
        "bedrock sees java stop crawling",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the CRAWLING actor flag cleared once the block is gone",
            |bot, _| !bot.actor_flag(java_runtime_id, entity_data_flag::CRAWLING),
        )
        .await,
    );
    Ok(())
}

/// Java's shared entity flag bit for swimming.
const JAVA_FLAG_SWIMMING: u8 = 1 << 4;

/// Each player sprints under water and starts swimming; the other edition sees it.
async fn run_swim_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    ids: &PoseIds,
    blocks: [BlockPos; 2],
) -> Result<(), String> {
    let swimming = EntityPose::Swimming as i32;
    let java_runtime_id = ids.java_runtime;
    let fill = |block: &str| {
        for pos in blocks {
            let (x, y, z) = (pos.0.x, pos.0.y, pos.0.z);
            run_console(
                server,
                &format!(
                    "fill {} {y} {} {} {} {} {block}",
                    x - 1,
                    z - 1,
                    x + 1,
                    y + 2,
                    z + 1
                ),
            );
        }
    };
    fill("water");
    tick(server, bedrock, Some(&mut *java)).await?;

    send_bedrock_flags(
        bedrock,
        &[
            InputData::StartSprinting,
            InputData::Sprinting,
            InputData::StartSwimming,
        ],
    )
    .await?;
    checks.record(
        "java sees bedrock swim",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the swimming flag and pose on the Bedrock player in water",
            |_, java| {
                seen_by_java(java, ids).is_some_and(|(flags, pose)| {
                    flags & JAVA_FLAG_SWIMMING != 0 && pose == swimming
                })
            },
        )
        .await,
    );

    java.send_player_command(ids.java_entity, PlayerCommandAction::StartSprinting)
        .await?;
    checks.record(
        "bedrock sees java swim",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the SWIMMING actor flag on the Java player in water",
            |bot, _| {
                bot.actor_flag(java_runtime_id, entity_data_flag::SWIMMING)
                    && !bot.actor_flag(java_runtime_id, entity_data_flag::CRAWLING)
            },
        )
        .await,
    );

    send_bedrock_flags(
        bedrock,
        &[InputData::StopSwimming, InputData::StopSprinting],
    )
    .await?;
    java.send_player_command(ids.java_entity, PlayerCommandAction::StopSprinting)
        .await?;
    fill("air");
    checks.record(
        "java sees bedrock stop swimming",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock player standing again out of the water",
            |_, java| {
                seen_by_java(java, ids).is_some_and(|(flags, pose)| {
                    flags & JAVA_FLAG_SWIMMING == 0 && pose == EntityPose::Standing as i32
                })
            },
        )
        .await,
    );
    Ok(())
}

/// Summons `entity` next to the Java bot and waits until both bots know it. Returns its
/// entity id.
async fn summon_next_to_java(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    entity: &EntityType,
) -> Result<i32, String> {
    let at = java.position + Vector3::new(1.5, 0.0, 0.0);
    run_console(
        server,
        &format!(
            "summon minecraft:{} {} {} {}",
            entity.resource_name, at.x, at.y, at.z
        ),
    );
    let type_id = i32::from(entity.id);
    let find = |java: &HeadlessBot| {
        java.entities
            .iter()
            .find(|(_, seen)| seen.entity_type == type_id)
            .map(|(id, _)| *id)
    };
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "the summoned entity on both bots",
        |bot, java| {
            java.and_then(find)
                .is_some_and(|id| bot.actors.contains_key(&i64::from(id)))
        },
    )
    .await?;
    find(java).ok_or_else(|| "the summoned entity is gone".to_string())
}

/// Riding and leads across editions: each edition sees the other board and leave a boat, and
/// a lead shows as a lead rather than a rider.
async fn run_mount_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let (bedrock_start, java_start) = (bedrock.position, java.position);
    run_boat_checks(server, bedrock, java, checks).await?;
    run_leash_checks(server, bedrock, java, checks).await?;

    // Dismounting put both players next to the boat; walk back so the later checks have each
    // player on its own block again.
    bedrock.send_input(bedrock_start, &[]).await?;
    java.move_to(java_start, true).await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(())
}

/// Each edition sees the other board and leave a boat.
async fn run_boat_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player = |name: &str| {
        server
            .server
            .get_player_by_name(name)
            .ok_or_else(|| format!("{name} is not on the server"))
    };
    let (bedrock_player, java_player) = (player(BEDROCK_NAME)?, player(JAVA_NAME)?);
    let bedrock_id = bedrock_player.entity_id();
    let java_id = java_player.entity_id();
    let boat_id = summon_next_to_java(server, bedrock, java, &EntityType::OAK_BOAT).await?;
    let boat = bedrock_player
        .world()
        .get_entity_by_id(boat_id)
        .ok_or("the boat is not in the world")?;

    java.use_entity(boat_id).await?;
    checks.record(
        "bedrock sees java board a boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a rider link from the Java player to the boat",
            |bot, _| {
                bot.ride_link(i64::from(java_id)) == Some((i64::from(boat_id), bot::LINK_RIDER))
            },
        )
        .await,
    );

    // The same call as `/ride` and the boat's own interaction.
    boat.get_entity()
        .add_passenger(boat.clone(), bedrock_player.clone());
    checks.record(
        "bedrock told it rides the boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a passenger link from the Bedrock bot to the boat",
            |bot, _| {
                bot.ride_link(bot.unique_id) == Some((i64::from(boat_id), bot::LINK_PASSENGER))
            },
        )
        .await,
    );
    checks.record(
        "java sees bedrock board the boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock player in the boat's passengers",
            |_, java| {
                java.and_then(|java| java.passengers.get(&boat_id))
                    .is_some_and(|ids| ids.contains(&bedrock_id))
            },
        )
        .await,
    );

    bedrock.stop_riding(boat_id as u64).await?;
    checks.record(
        "bedrock leaves the boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Bedrock player off the boat for the server and both bots",
            |bot, java| {
                !bedrock_player.living_entity.entity.has_vehicle()
                    && bot.ride_link(bot.unique_id).is_none()
                    && java
                        .and_then(|java| java.passengers.get(&boat_id))
                        .is_some_and(|ids| !ids.contains(&bedrock_id))
            },
        )
        .await,
    );

    java.send_input(SPlayerInput::SNEAK).await?;
    checks.record(
        "bedrock sees java leave the boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the Java player's rider link removed",
            |bot, _| bot.ride_link(i64::from(java_id)).is_none(),
        )
        .await,
    );
    java.send_input(0).await?;
    boat.get_entity().remove();

    Ok(())
}

/// A lead shows on Bedrock as a lead, not as the holder riding the mob.
async fn run_leash_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player = |name: &str| {
        server
            .server
            .get_player_by_name(name)
            .ok_or_else(|| format!("{name} is not on the server"))
    };
    let (bedrock_player, java_player) = (player(BEDROCK_NAME)?, player(JAVA_NAME)?);
    let java_id = java_player.entity_id();
    let cow_id = summon_next_to_java(server, bedrock, java, &EntityType::COW).await?;
    let cow = bedrock_player
        .world()
        .get_entity_by_id(cow_id)
        .ok_or("the cow is not in the world")?;
    cow.get_entity().leash_to(java_player.clone());
    let cow_runtime_id = cow_id as u64;
    checks.record(
        "bedrock sees a lead as a lead",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "LEASH_HOLDER on the cow naming the Java player, and no link",
            |bot, _| {
                bot.actor_longs
                    .get(&(cow_runtime_id, entity_data_key::LEASH_HOLDER))
                    == Some(&i64::from(java_id))
                    && bot.actor_flag(cow_runtime_id, entity_data_flag::LEASHED)
                    && !bot
                        .links
                        .iter()
                        .any(|(ridden, _, _)| *ridden == i64::from(cow_id))
            },
        )
        .await,
    );
    cow.get_entity().unleash();
    checks.record(
        "bedrock sees the lead removed",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "LEASH_HOLDER -1 on the cow",
            |bot, _| {
                bot.actor_longs
                    .get(&(cow_runtime_id, entity_data_key::LEASH_HOLDER))
                    == Some(&-1)
                    && !bot.actor_flag(cow_runtime_id, entity_data_flag::LEASHED)
            },
        )
        .await,
    );
    cow.get_entity().remove();

    Ok(())
}

/// How far the fall checks drop each player. Vanilla's fall damage is the floor of the distance
/// less the safe fall distance of 3, so 7 health; the half block keeps float error away from the
/// floor.
const FALL_HEIGHT: f32 = 10.5;
const FALL_STEP: f32 = 0.5;

/// A Bedrock player takes the same fall damage as a Java player falling the same distance, and
/// is told its new health.
async fn run_fall_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let health_of = |name: &str| -> Result<f32, String> {
        server
            .server
            .get_player_by_name(name)
            .map(|player| player.living_entity.health.load())
            .ok_or_else(|| format!("{name} is not on the server"))
    };
    // Natural regeneration would heal the first player while the second one falls.
    run_console(server, "gamerule natural_health_regeneration false");
    let (bedrock_before, java_before) = (health_of(BEDROCK_NAME)?, health_of(JAVA_NAME)?);
    let steps = (FALL_HEIGHT / FALL_STEP) as u8;

    // Up in one move, then down half a block per tick, landing on the last one.
    let start = bedrock.position;
    bedrock.on_ground = false;
    bedrock
        .send_input(start + Vector3::new(0.0, FALL_HEIGHT, 0.0), &[])
        .await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    for step in 1..=steps {
        bedrock.on_ground = step == steps;
        let height = FALL_HEIGHT - f32::from(step) * FALL_STEP;
        bedrock
            .send_input(start + Vector3::new(0.0, height, 0.0), &[])
            .await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }

    let java_start = java.position;
    java.move_to(
        java_start + Vector3::new(0.0, f64::from(FALL_HEIGHT), 0.0),
        false,
    )
    .await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    for step in 1..=steps {
        let height = f64::from(FALL_HEIGHT - f32::from(step) * FALL_STEP);
        java.move_to(java_start + Vector3::new(0.0, height, 0.0), step == steps)
            .await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    tick(server, bedrock, Some(&mut *java)).await?;

    let bedrock_lost = bedrock_before - health_of(BEDROCK_NAME)?;
    let java_lost = java_before - health_of(JAVA_NAME)?;
    checks.record(
        "bedrock fall damage matches java",
        if java_lost >= 7.0 && (bedrock_lost - java_lost).abs() < 0.01 {
            Ok(())
        } else {
            Err(format!(
                "a {FALL_HEIGHT}-block fall cost the Bedrock player {bedrock_lost} health and the \
                 Java player {java_lost}"
            ))
        },
    );
    run_console(server, "gamerule natural_health_regeneration true");
    let expected = health_of(BEDROCK_NAME)?.ceil() as i32;
    checks.record(
        "bedrock told its health after the fall",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a SetHealth with the health left after the fall",
            |bot, _| bot.health == expected,
        )
        .await,
    );
    Ok(())
}

/// A Bedrock player's command runs and its output reaches Java players. Needs the operator
/// status `run_block_checks` gives.
async fn run_command_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    bedrock.command("/say said from bedrock").await?;
    checks.record(
        "bedrock command reaches java",
        tick_until(
            server,
            bedrock,
            Some(java),
            "the Bedrock /say line on the Java bot",
            |_, java| java.is_some_and(|java| java.saw_chat_containing("said from bedrock")),
        )
        .await,
    );
    Ok(())
}

/// Each bot breaks a block in creative next to it; the other bot must see it go.
async fn run_block_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    // Both bots stand inside spawn protection, which only operators may build in.
    for name in [BEDROCK_NAME, JAVA_NAME] {
        run_console(server, &format!("op {name}"));
        run_console(server, &format!("gamemode creative {name}"));
    }
    // Bedrock positions are at eye height.
    let bedrock_feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let below_bedrock = BlockPos::floored(
        f64::from(bedrock.position.x),
        bedrock_feet - 0.5,
        f64::from(bedrock.position.z),
    );
    let below_java = BlockPos::floored(java.position.x, java.position.y - 1.0, java.position.z);
    for pos in [below_bedrock, below_java] {
        run_console(
            server,
            &format!("setblock {} {} {} stone", pos.0.x, pos.0.y, pos.0.z),
        );
    }
    for _ in 0..5 {
        tick(server, bedrock, Some(java)).await?;
    }
    // The Bedrock bot's runtime id for stone, from the setblock above, so the break check below
    // can tell the break from the placement.
    let bedrock_stone = bedrock
        .block_updates
        .iter()
        .rev()
        .find(|(pos, _)| *pos == below_java)
        .map(|(_, state)| *state)
        .ok_or("the Bedrock bot never saw the setblock under the Java bot")?;

    bedrock
        .send_input(
            bedrock.position,
            &[(bot::ACTION_START_BREAK, below_bedrock, 1)],
        )
        .await?;
    checks.record(
        "java sees bedrock break a block",
        tick_until(
            server,
            bedrock,
            Some(java),
            "an air block update on the Java bot",
            |_, java| {
                java.is_some_and(|java| {
                    java.block_updates
                        .iter()
                        .any(|(pos, state)| *pos == below_bedrock && *state == 0)
                })
            },
        )
        .await,
    );

    let sequence = 1;
    java.start_digging(below_java, sequence).await?;
    checks.record(
        "bedrock sees java break a block",
        tick_until(
            server,
            bedrock,
            Some(java),
            "a non-stone block update under the Java bot on the Bedrock bot",
            |bot, _| {
                bot.block_updates
                    .iter()
                    .rev()
                    .find(|(pos, _)| *pos == below_java)
                    .is_some_and(|(_, state)| *state != bedrock_stone)
            },
        )
        .await,
    );
    Ok(())
}

fn run_console(server: &BedrockTestServer, command: &str) {
    let dispatcher = server.server.command_dispatcher.load();
    dispatcher.handle_command(
        &pumpkin_core::command::CommandSender::Console.into_source(&server.server),
        command,
    );
}
