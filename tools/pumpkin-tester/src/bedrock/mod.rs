//! Bedrock tests: a server with Bedrock on, a [`BedrockBot`] joining it over `NetherNet` the way
//! a 1.26.60 client does, and a Java [`HeadlessBot`] next to it for the cross-play checks.

mod bot;
mod dimensions;
mod held_items;
mod riding;
mod transport;

use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use pumpkin_core::{
    PumpkinServer,
    entity::{EntityBase, player::Player},
    net::bedrock::BedrockClient,
    server::Server,
};
use pumpkin_data::{
    Block,
    entity::{EntityPose, EntityType},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_protocol::{
    bedrock::{
        client::set_actor_data::{entity_data_flag, entity_data_key},
        server::player_auth_input::InputData,
    },
    java::server::play::{Action as PlayerCommandAction, SPlayerInput},
};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{bot::HeadlessBot, harness};
pub use bot::BedrockBot;
use bot::{StackMove, StackMoveKind};
use pumpkin_inventory::Inventory;
use pumpkin_protocol::bedrock::network_item::ContainerName;

const TICK: Duration = Duration::from_millis(50);
/// Plenty for a packet to cross a loopback WebRTC connection and be handled in a tick.
const MAX_WAIT_TICKS: usize = 200;
const BEDROCK_NAME: &str = "BedrockBot";
const JAVA_NAME: &str = "JavaBot";
/// `bedrock.max_players` on the test server: room for the Java and Bedrock bots in `mixed_play`,
/// and for the main Bedrock bot plus one more in `login_checks`.
const MAX_PLAYERS: u32 = 2;

/// How long the Bedrock tests may go without finishing a tick before the stall watchdog reports
/// them stuck, and (unless `PEX_TESTER_STALL_EXIT_SECS` says otherwise) ends the run.
const STALL_REPORT: Duration = Duration::from_secs(120);

/// What the last tick was doing, for the stall report.
static TICK_PHASE: AtomicU8 = AtomicU8::new(PHASE_IDLE);
const PHASE_IDLE: u8 = 0;
const PHASE_SERVER_TICK: u8 = 1;
const PHASE_BEDROCK_POLL: u8 = 2;
const PHASE_JAVA_POLL: u8 = 3;
const PHASE_SLEEP: u8 = 4;
/// Milliseconds since `WATCHDOG_START` when the last tick finished.
static LAST_TICK_MS: AtomicU64 = AtomicU64::new(0);
static WATCHDOG_START: OnceLock<Instant> = OnceLock::new();
static WATCHDOG_STOP: AtomicBool = AtomicBool::new(false);
/// The scenario and the check recorded last, for the stall report.
static LAST_CHECK: std::sync::Mutex<(&str, &str)> = std::sync::Mutex::new(("", ""));

fn mark_progress() {
    let start = WATCHDOG_START.get_or_init(Instant::now);
    let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    LAST_TICK_MS.store(elapsed, Ordering::Relaxed);
}

/// A thread that ends the run with a clear message when no tick finishes for `STALL_REPORT`:
/// every wait in the Bedrock tests is capped in ticks, so a stall means an `await` (a server
/// tick or a bot read) never returned. Set `PEX_TESTER_STALL_EXIT_SECS` higher to leave the
/// stuck process running after the report, for a debugger or a crash dump.
#[expect(
    clippy::print_stderr,
    reason = "the stall report goes to the runner log, like main's"
)]
fn start_stall_watchdog() {
    mark_progress();
    WATCHDOG_STOP.store(false, Ordering::Relaxed);
    let exit_after = std::env::var("PEX_TESTER_STALL_EXIT_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .map_or(STALL_REPORT, Duration::from_secs);
    std::thread::spawn(move || {
        let start = *WATCHDOG_START.get_or_init(Instant::now);
        let mut reported = false;
        while !WATCHDOG_STOP.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            let last = Duration::from_millis(LAST_TICK_MS.load(Ordering::Relaxed));
            let stalled = start.elapsed().saturating_sub(last);
            if stalled < STALL_REPORT {
                reported = false;
                continue;
            }
            if !reported {
                reported = true;
                let (scenario, check) = *LAST_CHECK
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let phase = match TICK_PHASE.load(Ordering::Relaxed) {
                    PHASE_SERVER_TICK => "inside Server::tick",
                    PHASE_BEDROCK_POLL => "reading the Bedrock bot's packets",
                    PHASE_JAVA_POLL => "reading the Java bot's packets",
                    PHASE_SLEEP => "sleeping between ticks",
                    _ => "outside a tick",
                };
                eprintln!(
                    "bedrock: STUCK: no tick for {}s, {phase}, in bedrock:{scenario} after the \
                     check \"{check}\"",
                    stalled.as_secs()
                );
            }
            if stalled >= exit_after {
                eprintln!("bedrock: ending the stuck run");
                std::process::exit(101);
            }
        }
    });
}

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
                () = tokio::time::sleep(TICK) => {
                    TICK_PHASE.store(PHASE_SERVER_TICK, Ordering::Relaxed);
                    self.server.tick();
                    TICK_PHASE.store(PHASE_IDLE, Ordering::Relaxed);
                    mark_progress();
                }
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
    TICK_PHASE.store(PHASE_SERVER_TICK, Ordering::Relaxed);
    server.server.tick();
    TICK_PHASE.store(PHASE_SLEEP, Ordering::Relaxed);
    tokio::time::sleep(TICK).await;
    TICK_PHASE.store(PHASE_BEDROCK_POLL, Ordering::Relaxed);
    bedrock.poll_packets().await?;
    if let Some(java) = java {
        TICK_PHASE.store(PHASE_JAVA_POLL, Ordering::Relaxed);
        java.poll_packets().await?;
    }
    TICK_PHASE.store(PHASE_IDLE, Ordering::Relaxed);
    mark_progress();
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
        *LAST_CHECK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = (self.scenario, name);
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
    start_stall_watchdog();
    let result = Box::pin(run_scenarios(&server, summary_lines)).await;
    WATCHDOG_STOP.store(true, Ordering::Relaxed);
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
    riding::run_riding_checks(server, bedrock, &mut java, &mut checks).await?;
    // Before `run_block_checks`, which puts both players in creative.
    run_fall_checks(server, bedrock, &mut java, &mut checks).await?;
    run_survival_break_checks(server, bedrock, &mut java, &mut checks).await?;
    run_place_checks(server, bedrock, &mut java, &mut checks).await?;
    run_item_use_checks(server, bedrock, &mut java, &mut checks).await?;
    held_items::run_held_item_checks(server, bedrock, &mut java, &mut checks).await?;
    run_chest_checks(server, bedrock, &mut java, &mut checks).await?;
    run_furnace_checks(server, bedrock, &mut java, &mut checks).await?;
    run_craft_checks(server, bedrock, &mut java, &mut checks).await?;
    run_combat_checks(server, bedrock, &mut java, &mut checks).await?;
    run_block_checks(server, bedrock, &mut java, &mut checks).await?;
    run_command_checks(server, bedrock, &mut java, &mut checks).await?;
    dimensions::run_dimension_checks(server, bedrock, &mut java, &mut checks).await?;

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
    // Chunk generation spawns animals, so a pig or cow of the world's own may already be in
    // view: take the newly tracked one nearest the spot.
    let seen_before: Vec<i32> = java
        .entities_of_type(entity)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    run_console(
        server,
        &format!(
            "summon minecraft:{} {} {} {}",
            entity.resource_name, at.x, at.y, at.z
        ),
    );
    let find = |java: &HeadlessBot| {
        java.entities_of_type(entity)
            .into_iter()
            .filter(|(id, _)| !seen_before.contains(id))
            .min_by(|(_, a), (_, b)| {
                a.squared_distance_to_vec(&at)
                    .total_cmp(&b.squared_distance_to_vec(&at))
            })
            .map(|(id, _)| id)
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

    run_steer_check(server, bedrock, java, checks, &boat, &bedrock_player).await?;
    boat.get_entity().remove();

    Ok(())
}

/// Alone in the boat, the Bedrock player drives it and the Java player sees it move.
async fn run_steer_check(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    boat: &Arc<dyn EntityBase>,
    bedrock_player: &Arc<Player>,
) -> Result<(), String> {
    let boat_id = boat.get_entity().entity_id;
    boat.get_entity()
        .add_passenger(boat.clone(), bedrock_player.clone());
    tick(server, bedrock, Some(&mut *java)).await?;
    let boat_start = boat.get_entity().pos.load();
    bedrock.driving = Some((i64::from(boat_id), Vector2::new(0.0, 90.0)));
    let rider_start = bedrock.position;
    for step in 1..=4u8 {
        let position = rider_start + Vector3::new(f32::from(step) * 0.5, 0.0, 0.0);
        bedrock.send_input(position, &[]).await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let boat_target = boat_start.x + 2.0;
    checks.record(
        "java sees bedrock steer the boat",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the boat two blocks on, on the Java bot",
            |_, java| {
                java.and_then(|java| java.entities.get(&boat_id))
                    .is_some_and(|seen| (seen.position.x - boat_target).abs() < 0.3)
            },
        )
        .await,
    );
    bedrock.driving = None;
    bedrock.stop_riding(boat_id as u64).await?;
    tick(server, bedrock, Some(&mut *java)).await?;
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

/// Digging from Bedrock outside creative follows the Java rules: a timed break ends on the
/// server's clock, and adventure and spectator players can't break blocks.
async fn run_survival_break_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    // Spawn protection lets only operators build.
    run_console(server, &format!("op {BEDROCK_NAME}"));
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));

    // A head-high block beside the Bedrock bot, on the side away from the Java bot.
    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java.position.x > f64::from(bedrock.position.x) {
        -1.0
    } else {
        1.0
    };
    let target = BlockPos::floored(
        f64::from(bedrock.position.x) + side,
        feet + 1.0,
        f64::from(bedrock.position.z),
    );
    let setblock = format!("setblock {} {} {} dirt", target.0.x, target.0.y, target.0.z);
    run_console(server, &setblock);
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let dirt = bedrock
        .block_updates
        .iter()
        .rev()
        .find(|(pos, _)| *pos == target)
        .map(|(_, state)| *state)
        .ok_or("the Bedrock bot never saw the dirt placed")?;

    // Survival: dirt by hand takes 15 ticks. The client claims it after 2; the server keeps the
    // block and finishes the break itself once its own progress gets there, as for Java.
    bedrock
        .send_input(bedrock.position, &[(bot::ACTION_START_BREAK, target, 1)])
        .await?;
    for _ in 0..2 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock
        .send_input(bedrock.position, &[(bot::ACTION_PREDICT_BREAK, target, 1)])
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "an early bedrock break waits for the server",
        if world.get_block_state(&target).is_air() {
            Err("the dirt broke 5 ticks into a 15-tick dig".to_string())
        } else {
            Ok(())
        },
    );
    checks.record(
        "java sees bedrock finish a survival break",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "an air block update at the dug block on the Java bot",
            |_, java| {
                java.is_some_and(|java| {
                    java.block_updates
                        .iter()
                        .any(|(pos, state)| *pos == target && *state == 0)
                })
            },
        )
        .await,
    );

    run_no_break_checks(server, bedrock, java, checks, target, dirt).await?;
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));
    run_console(
        server,
        &format!("setblock {} {} {} air", target.0.x, target.0.y, target.0.z),
    );
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// Adventure (without a `can_break` item) and spectator players dig the block for 20 ticks; it
/// must stay, and the Bedrock bot must be sent it back.
async fn run_no_break_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    target: BlockPos,
    dirt: u32,
) -> Result<(), String> {
    let world = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?
        .world();
    let setblock = format!("setblock {} {} {} dirt", target.0.x, target.0.y, target.0.z);
    for (mode, name) in [
        ("adventure", "adventure bedrock can't break a block"),
        ("spectator", "spectator bedrock can't break a block"),
    ] {
        run_console(server, &format!("gamemode {mode} {BEDROCK_NAME}"));
        run_console(server, &setblock);
        for _ in 0..3 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        let seen = bedrock.block_updates.len();
        bedrock
            .send_input(bedrock.position, &[(bot::ACTION_START_BREAK, target, 1)])
            .await?;
        for _ in 0..20 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        bedrock
            .send_input(bedrock.position, &[(bot::ACTION_PREDICT_BREAK, target, 1)])
            .await?;
        for _ in 0..3 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        let resent = bedrock.block_updates[seen..]
            .iter()
            .any(|(pos, state)| *pos == target && *state == dirt);
        checks.record(
            name,
            if world.get_block_state(&target).is_air() {
                Err(format!("the dirt broke in {mode}"))
            } else if !resent {
                Err("the Bedrock bot was not sent the dirt back".to_string())
            } else {
                Ok(())
            },
        );
    }
    Ok(())
}

/// Survival placing from Bedrock, with the packets a real client sends: the client's own guess
/// at its inventory comes along, and the server must go by its inventory instead.
async fn run_place_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    run_console(server, &format!("op {BEDROCK_NAME}"));
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} dirt 10"));

    // A stone two blocks beside the Bedrock bot, away from the Java bot, to place on top of, and
    // one out of reach.
    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java.position.x > f64::from(bedrock.position.x) {
        -1.0
    } else {
        1.0
    };
    let beside = |blocks: f64| {
        BlockPos::floored(
            f64::from(bedrock.position.x) + side * blocks,
            feet,
            f64::from(bedrock.position.z),
        )
    };
    let (support, far_support) = (beside(2.0), beside(9.0));
    let (placed, far_placed) = (support.up(), far_support.up());
    let setblock = |pos: BlockPos, block: &str| {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    };
    for pos in [support, far_support] {
        setblock(pos, "stone");
    }
    for pos in [placed, far_placed] {
        setblock(pos, "air");
    }
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let held = || bedrock_player.inventory().held_item();
    if held().item.id != Item::DIRT.id || held().item_count != 10 {
        return Err("the Bedrock bot is not holding the 10 dirt it was given".to_string());
    }

    // The client predicts 10 dirt going to 9; the server takes its own one, not a second.
    let dirt = |count| ItemStack::new(count, &Item::DIRT);
    bedrock
        .use_item_on_block(support, 1, 0, &dirt(10), &[(0, dirt(10), dirt(9))])
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "bedrock places a block and loses one item",
        if world.get_block(&placed).id != Block::DIRT.id {
            Err("no dirt was placed".to_string())
        } else if held().item_count == 9 {
            Ok(())
        } else {
            Err(format!("{} dirt left of 10", held().item_count))
        },
    );
    checks.record(
        "java sees bedrock place a block",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a block update for the placed dirt on the Java bot",
            |_, java| {
                java.is_some_and(|java| {
                    java.block_updates
                        .iter()
                        .any(|(pos, state)| *pos == placed && *state != 0)
                })
            },
        )
        .await,
    );

    let spots = PlaceSpots {
        support,
        placed,
        far_support,
        far_placed,
    };
    run_refused_place_checks(server, bedrock, java, checks, &spots).await
}

/// Where `run_place_checks` places: on top of `support` within reach, and on top of
/// `far_support` out of reach.
#[derive(Clone, Copy)]
struct PlaceSpots {
    support: BlockPos,
    placed: BlockPos,
    far_support: BlockPos,
    far_placed: BlockPos,
}

/// Places the server must refuse: out of reach, and with a stack the client only claims.
async fn run_refused_place_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    spots: &PlaceSpots,
) -> Result<(), String> {
    let PlaceSpots {
        support,
        placed,
        far_support,
        far_placed,
    } = *spots;
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    let held = || bedrock_player.inventory().held_item();
    let dirt = |count| ItemStack::new(count, &Item::DIRT);
    let setblock = |pos: BlockPos, block: &str| {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    };

    // Out of reach: nothing is placed, and the client's predicted block is taken back.
    let seen = bedrock.block_updates.len();
    bedrock
        .use_item_on_block(far_support, 1, 0, &dirt(9), &[(0, dirt(9), dirt(8))])
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let taken_back = bedrock.block_updates[seen..]
        .iter()
        .any(|(pos, _)| *pos == far_placed);
    checks.record(
        "a bedrock place out of reach is undone",
        if !world.get_block_state(&far_placed).is_air() {
            Err("a block was placed out of reach".to_string())
        } else if !taken_back {
            Err("the Bedrock bot was not sent the block back".to_string())
        } else if held().item_count != 9 {
            Err(format!("{} dirt left of 9", held().item_count))
        } else {
            Ok(())
        },
    );

    // A client claiming a stack it doesn't have gets nothing.
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    setblock(placed, "air");
    tick(server, bedrock, Some(&mut *java)).await?;
    let diamonds = ItemStack::new(64, &Item::DIAMOND_BLOCK);
    bedrock
        .use_item_on_block(
            support,
            1,
            0,
            &diamonds,
            &[(0, ItemStack::EMPTY.clone(), diamonds.clone())],
        )
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "bedrock can't place an item it doesn't hold",
        if !world.get_block_state(&placed).is_air() {
            Err("the claimed block was placed".to_string())
        } else if !held().is_empty() {
            Err("the claimed stack ended up in the inventory".to_string())
        } else {
            Ok(())
        },
    );

    run_console(server, &format!("clear {BEDROCK_NAME}"));
    for pos in [support, far_support, placed, far_placed] {
        setblock(pos, "air");
    }
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// One tool or bucket use in `run_item_use_checks`.
#[derive(Clone, Copy)]
struct ItemUseCase {
    name: &'static str,
    /// The block clicked on its top face.
    under: &'static str,
    item: &'static Item,
    /// Whether the result lands on top of the clicked block rather than replacing it.
    on_top: bool,
    block: &'static Block,
    /// What the hand holds afterwards.
    after: &'static Item,
}

const ITEM_USE_CASES: [ItemUseCase; 5] = [
    ItemUseCase {
        name: "bedrock tills dirt with a hoe",
        under: "dirt",
        item: &Item::WOODEN_HOE,
        on_top: false,
        block: &Block::FARMLAND,
        after: &Item::WOODEN_HOE,
    },
    ItemUseCase {
        name: "bedrock strips a log with an axe",
        under: "oak_log",
        item: &Item::WOODEN_AXE,
        on_top: false,
        block: &Block::STRIPPED_OAK_LOG,
        after: &Item::WOODEN_AXE,
    },
    ItemUseCase {
        name: "bedrock makes a path with a shovel",
        under: "grass_block",
        item: &Item::WOODEN_SHOVEL,
        on_top: false,
        block: &Block::DIRT_PATH,
        after: &Item::WOODEN_SHOVEL,
    },
    ItemUseCase {
        name: "bedrock empties a water bucket",
        under: "stone",
        item: &Item::WATER_BUCKET,
        on_top: true,
        block: &Block::WATER,
        after: &Item::BUCKET,
    },
    ItemUseCase {
        name: "bedrock lights a fire with flint and steel",
        under: "stone",
        item: &Item::FLINT_AND_STEEL,
        on_top: true,
        block: &Block::FIRE,
        after: &Item::FLINT_AND_STEEL,
    },
];

/// Tools and buckets used on a block from Bedrock, in survival: each must change the block (and
/// the held item) as it does for Java.
async fn run_item_use_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    run_console(server, &format!("op {BEDROCK_NAME}"));
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));

    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java.position.x > f64::from(bedrock.position.x) {
        -1.0
    } else {
        1.0
    };
    let target = BlockPos::floored(
        f64::from(bedrock.position.x) + side * 2.0,
        feet,
        f64::from(bedrock.position.z),
    );
    let above = target.up();
    let setblock = |pos: BlockPos, block: &str| {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    };

    // Look at the top of the clicked block: buckets aim with the player's view, as in vanilla.
    let (old_yaw, old_pitch) = (bedrock.yaw, bedrock.pitch);
    let aim = Vector3::new(
        f64::from(target.0.x) + 0.5,
        f64::from(target.0.y) + 1.0,
        f64::from(target.0.z) + 0.5,
    ) - bedrock.position.to_f64();
    bedrock.yaw = (-aim.x.atan2(aim.z)).to_degrees() as f32;
    bedrock.pitch = (-aim.y.atan2(aim.horizontal_length())).to_degrees() as f32;
    tick(server, bedrock, Some(&mut *java)).await?;
    for case in &ITEM_USE_CASES {
        let ItemUseCase {
            name,
            under,
            item,
            on_top,
            block: expect_block,
            after: expect_item,
        } = *case;
        let expect_at = if on_top { above } else { target };
        run_console(server, &format!("clear {BEDROCK_NAME}"));
        run_console(
            server,
            &format!("give {BEDROCK_NAME} {}", item.registry_key),
        );
        setblock(above, "air");
        setblock(target, under);
        for _ in 0..3 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        let stack = ItemStack::new(1, item);
        bedrock.use_item_on_block(target, 1, 0, &stack, &[]).await?;
        for _ in 0..3 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        let found = world.get_block(&expect_at);
        let held = bedrock_player.inventory().held_item();
        checks.record(
            name,
            if found.id != expect_block.id {
                Err(format!(
                    "found {} instead of {}",
                    found.name, expect_block.name
                ))
            } else if held.item.id != expect_item.id {
                Err(format!(
                    "holding {} instead of {}",
                    held.item.registry_key, expect_item.registry_key
                ))
            } else {
                Ok(())
            },
        );
    }
    run_bucket_press_check(server, bedrock, java, checks, target).await?;
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    setblock(above, "air");
    setblock(target, "air");
    (bedrock.yaw, bedrock.pitch) = (old_yaw, old_pitch);
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// A client that follows its click on a block with a click-air for the same press must not get
/// the item's use twice: a second bucket use would pick the water straight back up.
async fn run_bucket_press_check(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    target: BlockPos,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} water_bucket"));
    for (pos, block) in [(target.up(), "air"), (target, "stone")] {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    }
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let bucket = ItemStack::new(1, &Item::WATER_BUCKET);
    bedrock
        .use_item_on_block(target, 1, 0, &bucket, &[])
        .await?;
    bedrock.use_item_in_air(0, &bucket).await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let held = bedrock_player.inventory().held_item();
    checks.record(
        "one bedrock bucket press is one use",
        if world.get_block(&target.up()).id != Block::WATER.id {
            Err("the water is gone".to_string())
        } else if held.item.id != Item::BUCKET.id {
            Err(format!("holding {}", held.item.registry_key))
        } else {
            Ok(())
        },
    );
    Ok(())
}

/// A survival Bedrock player opens a chest, puts items in, takes some back and closes it, with
/// the `ItemStackRequest`s a real client's screen sends: each move goes through the cursor.
async fn run_chest_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    for case in &CONTAINER_CASES {
        run_container_case(server, bedrock, java, checks, case).await?;
    }
    Ok(())
}

/// A survival Bedrock player smelts raw iron in a furnace: ingredient and fuel in through the
/// furnace's own slots, the ingot out of the result slot.
async fn run_furnace_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} raw_iron 1"));
    run_console(server, &format!("give {BEDROCK_NAME} coal 1"));
    let furnace = spot_beside(bedrock, java, 2.0);
    let setblock = |block: &str| set_block(server, furnace, block);
    setblock("furnace");
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock
        .use_item_on_block(furnace, 1, 0, &ItemStack::new(1, &Item::RAW_IRON), &[])
        .await?;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a ContainerOpen for the furnace",
        |bot, _| bot.open_container.is_some(),
    )
    .await?;
    move_through_cursor(
        bedrock,
        10,
        None,
        1,
        (ContainerName::HotBar, 0),
        (ContainerName::FurnaceIngredient, 0),
    )
    .await?;
    move_through_cursor(
        bedrock,
        11,
        None,
        1,
        (ContainerName::HotBar, 1),
        (ContainerName::FurnaceFuel, 1),
    )
    .await?;
    let world = bedrock_player.world();
    let furnace_stack = |slot| {
        world
            .get_block_entity(&furnace)
            .and_then(pumpkin_core::block::entities::BlockEntity::get_inventory)
            .map(|inventory| inventory.get_stack(slot))
    };
    checks.record(
        "bedrock smelts raw iron in a furnace",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "an iron ingot in the furnace's result slot",
            |_, _| furnace_stack(2).is_some_and(|stack| stack.item.id == Item::IRON_INGOT.id),
        )
        .await,
    );
    move_through_cursor(
        bedrock,
        12,
        None,
        1,
        (ContainerName::FurnaceResult, 2),
        (ContainerName::HotBar, 2),
    )
    .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let taken = bedrock_player.inventory().get_stack(2);
    checks.record(
        "bedrock takes the ingot out of a furnace",
        if !bedrock.stack_responses.contains(&(12, 0)) {
            Err(format!("responses: {:?}", bedrock.stack_responses))
        } else if taken.item.id != Item::IRON_INGOT.id {
            Err(format!("hotbar slot 2 holds {}", taken.item.registry_key))
        } else if furnace_stack(2).is_some_and(|stack| !stack.is_empty()) {
            Err("the ingot is still in the furnace".to_string())
        } else {
            Ok(())
        },
    );
    bedrock.close_container().await?;
    setblock("air");
    tick(server, bedrock, Some(&mut *java)).await?;
    run_console(server, "kill @e[type=item]");
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// Crafting planks in the 2x2 grid of the Bedrock player's own inventory, as the client does:
/// the log into the grid, `CraftRecipe`, then the result out of the created-output slot. A
/// request for more crafts than the grid holds must not make more than the grid allows.
async fn run_craft_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} oak_log 1"));
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    move_through_cursor(
        bedrock,
        20,
        None,
        1,
        (ContainerName::HotBar, 0),
        (ContainerName::CraftingInput, 28),
    )
    .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    // One log makes 4 planks. Ask for 5 crafts, and take all 20 the client would predict.
    move_through_cursor(
        bedrock,
        21,
        Some((1, 5)),
        20,
        (ContainerName::CreatedOutput, 50),
        (ContainerName::HotBar, 1),
    )
    .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let inventory = bedrock_player.inventory();
    let planks: u32 = (0..36)
        .map(|slot| inventory.get_stack(slot))
        .filter(|stack| stack.item.id == Item::OAK_PLANKS.id)
        .map(|stack| u32::from(stack.item_count))
        .sum();
    checks.record(
        "bedrock can't craft more than the grid holds",
        if planks <= 4 {
            Ok(())
        } else {
            Err(format!("1 log made {planks} planks"))
        },
    );

    // A plain single craft makes exactly 4.
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} oak_log 1"));
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    move_through_cursor(
        bedrock,
        22,
        None,
        1,
        (ContainerName::HotBar, 0),
        (ContainerName::CraftingInput, 28),
    )
    .await?;
    move_through_cursor(
        bedrock,
        23,
        Some((1, 1)),
        4,
        (ContainerName::CreatedOutput, 50),
        (ContainerName::HotBar, 1),
    )
    .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let crafted = inventory.get_stack(1);
    checks.record(
        "bedrock crafts planks from a log",
        if crafted.item.id == Item::OAK_PLANKS.id && crafted.item_count == 4 {
            Ok(())
        } else {
            Err(format!(
                "hotbar slot 1 holds {} {}; responses {:?}",
                crafted.item_count, crafted.item.registry_key, bedrock.stack_responses
            ))
        },
    );
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// Moves `count` items from one slot to another through the cursor, with an optional
/// `CraftRecipe` first, in one `ItemStackRequest`, as a real client's screen does.
async fn move_through_cursor(
    bedrock: &BedrockBot,
    request_id: i32,
    craft: Option<(u32, u8)>,
    count: u8,
    from: (ContainerName, u8),
    to: (ContainerName, u8),
) -> Result<(), String> {
    bedrock
        .craft_request(
            request_id,
            craft,
            &[
                StackMove {
                    kind: StackMoveKind::Take,
                    count,
                    from,
                    to: (ContainerName::Cursor, 0),
                },
                StackMove {
                    kind: StackMoveKind::Place,
                    count,
                    from: (ContainerName::Cursor, 0),
                    to,
                },
            ],
        )
        .await
}

/// The block at the Bedrock bot's feet level, `blocks` along x on the side away from the Java
/// bot.
fn spot_beside(bedrock: &BedrockBot, java: &HeadlessBot, blocks: f64) -> BlockPos {
    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java.position.x > f64::from(bedrock.position.x) {
        -1.0
    } else {
        1.0
    };
    BlockPos::floored(
        f64::from(bedrock.position.x) + side * blocks,
        feet,
        f64::from(bedrock.position.z),
    )
}

fn set_block(server: &BedrockTestServer, pos: BlockPos, block: &str) {
    run_console(
        server,
        &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
    );
}

/// Hits both ways between the editions, with Java's player-versus-player rules for Bedrock attackers: no hits
/// from spectators, none on a creative player (`protect_creative`), none on oneself.
async fn run_combat_checks(
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
    let java_runtime = bedrock
        .player_runtime_id(JAVA_NAME)
        .ok_or("the Bedrock bot has no runtime id for the Java player")?;
    for name in [BEDROCK_NAME, JAVA_NAME] {
        run_console(server, &format!("gamemode survival {name}"));
        run_console(server, &format!("clear {name}"));
    }
    // Stand the Java bot within reach, beside the Bedrock bot.
    let java_start = java.position;
    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java_start.x > f64::from(bedrock.position.x) {
        1.5
    } else {
        -1.5
    };
    java.move_to(
        Vector3::new(
            f64::from(bedrock.position.x) + side,
            feet,
            f64::from(bedrock.position.z),
        ),
        true,
    )
    .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let java_health = || java_player.living_entity.health.load();
    // Each hit waits out the victim's hurt cooldown (10 ticks) first.
    let settle = async |bedrock: &mut BedrockBot, java: &mut HeadlessBot| {
        for _ in 0..12 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        Ok::<(), String>(())
    };

    let before = java_health();
    bedrock.attack(java_runtime).await?;
    settle(bedrock, java).await?;
    checks.record(
        "bedrock hits a java player",
        if java_health() < before {
            Ok(())
        } else {
            Err(format!("health stayed at {before}"))
        },
    );

    let before = bedrock.health;
    java.attack_entity(bedrock_player.entity_id()).await?;
    checks.record(
        "bedrock sees java hit it",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a lower SetHealth on the Bedrock bot",
            |bot, _| bot.health < before,
        )
        .await,
    );
    settle(bedrock, java).await?;

    run_attack_cooldown_checks(server, bedrock, java, checks, java_runtime).await?;
    run_refused_hit_checks(server, bedrock, java, checks, java_runtime).await?;

    for name in [BEDROCK_NAME, JAVA_NAME] {
        run_console(server, &format!("effect give {name} instant_health 1 5"));
    }
    java.move_to(java_start, true).await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(())
}

/// The Java attack cooldown for Bedrock attackers: a hit before the weapon has charged does
/// less damage, and the Bedrock bot is drawn a charge bar that clears once charged.
async fn run_attack_cooldown_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    java_runtime: u64,
) -> Result<(), String> {
    /// A wooden axe charges in 25 ticks (attack speed 0.8) and hits for 7.
    const CHARGE_TICKS: usize = 30;
    /// Past the victim's 10-tick hurt cooldown, but well short of a full charge.
    const QUICK_HIT_TICKS: usize = 11;
    const BAR_DOT: char = '\u{2d9}';
    let java_player = server
        .server
        .get_player_by_name(JAVA_NAME)
        .ok_or_else(|| format!("{JAVA_NAME} is not on the server"))?;
    let java_health = || java_player.living_entity.health.load();
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    // `item replace` into the held slot applies the axe's attack modifiers straight away;
    // `give` leaves them off until the next equipment sync, on Java too.
    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with wooden_axe"),
    );
    run_console(
        server,
        &format!("effect give {JAVA_NAME} instant_health 1 5"),
    );
    for _ in 0..CHARGE_TICKS {
        tick(server, bedrock, Some(&mut *java)).await?;
    }

    // A charged hit, then one 11 ticks later.
    let hit = async |bedrock: &mut BedrockBot, java: &mut HeadlessBot| {
        let before = java_health();
        bedrock.attack(java_runtime).await?;
        for _ in 0..2 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        Ok::<f32, String>(before - java_health())
    };
    bedrock.titles.clear();
    let charged = hit(bedrock, java).await?;
    for _ in 0..QUICK_HIT_TICKS - 2 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let quick = hit(bedrock, java).await?;
    checks.record(
        "a quick bedrock hit does less damage",
        if charged > 0.0 && quick < charged * 0.6 {
            Ok(())
        } else {
            Err(format!(
                "a charged hit did {charged}, one 11 ticks later did {quick}"
            ))
        },
    );
    checks.record(
        "bedrock is shown the attack charge",
        if bedrock
            .titles
            .iter()
            .any(|(kind, text)| *kind == 3 && text.contains(BAR_DOT))
        {
            Ok(())
        } else {
            Err(format!("no charge bar subtitle in {:?}", bedrock.titles))
        },
    );
    for _ in 0..CHARGE_TICKS {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let last_bar = bedrock.titles.iter().rposition(|(kind, _)| *kind == 3);
    let last_clear = bedrock.titles.iter().rposition(|(kind, _)| *kind == 0);
    checks.record(
        "the charge bar clears once charged",
        match (last_bar, last_clear) {
            (Some(bar), Some(clear)) if clear > bar => Ok(()),
            _ => Err(format!("titles after charging: {:?}", bedrock.titles)),
        },
    );
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(
        server,
        &format!("effect give {JAVA_NAME} instant_health 1 5"),
    );
    Ok(())
}

/// Hits the server must refuse from Bedrock: as a spectator, on a creative player, on oneself.
async fn run_refused_hit_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    java_runtime: u64,
) -> Result<(), String> {
    let player = |name: &str| {
        server
            .server
            .get_player_by_name(name)
            .ok_or_else(|| format!("{name} is not on the server"))
    };
    let (bedrock_player, java_player) = (player(BEDROCK_NAME)?, player(JAVA_NAME)?);
    let bedrock_runtime = u64::try_from(bedrock_player.entity_id()).map_err(|e| e.to_string())?;
    let java_health = || java_player.living_entity.health.load();
    let bedrock_health = || bedrock_player.living_entity.health.load();
    let settle = async |bedrock: &mut BedrockBot, java: &mut HeadlessBot| {
        for _ in 0..12 {
            tick(server, bedrock, Some(&mut *java)).await?;
        }
        Ok::<(), String>(())
    };
    for (mode, name) in [
        ("spectator", "a spectator on bedrock can't hit"),
        ("creative", "bedrock can't hit a creative player"),
    ] {
        let target = if mode == "spectator" {
            BEDROCK_NAME
        } else {
            JAVA_NAME
        };
        run_console(server, &format!("gamemode {mode} {target}"));
        settle(bedrock, java).await?;
        let before = java_health();
        bedrock.attack(java_runtime).await?;
        settle(bedrock, java).await?;
        checks.record(
            name,
            if java_health() < before {
                Err(format!("health went from {before} to {}", java_health()))
            } else {
                Ok(())
            },
        );
        run_console(server, &format!("gamemode survival {target}"));
    }

    let before = bedrock_health();
    bedrock.attack(bedrock_runtime).await?;
    settle(bedrock, java).await?;
    checks.record(
        "bedrock can't hit itself",
        if bedrock_health() < before {
            Err(format!("health went from {before} to {}", bedrock_health()))
        } else {
            Ok(())
        },
    );

    Ok(())
}

/// One container block for `run_container_case`, and the names of its four checks.
struct ContainerCase {
    block: &'static str,
    /// The container slot the items go to.
    slot: u8,
    names: [&'static str; 4],
}

const CONTAINER_CASES: [ContainerCase; 6] = [
    ContainerCase {
        block: "chest",
        slot: 0,
        names: [
            "bedrock opens a chest",
            "bedrock puts items in a chest",
            "bedrock takes items from a chest",
            "bedrock closes a chest",
        ],
    },
    ContainerCase {
        block: "barrel",
        slot: 26,
        names: [
            "bedrock opens a barrel",
            "bedrock puts items in a barrel",
            "bedrock takes items from a barrel",
            "bedrock closes a barrel",
        ],
    },
    ContainerCase {
        block: "shulker_box",
        slot: 13,
        names: [
            "bedrock opens a shulker box",
            "bedrock puts items in a shulker box",
            "bedrock takes items from a shulker box",
            "bedrock closes a shulker box",
        ],
    },
    ContainerCase {
        block: "hopper",
        slot: 4,
        names: [
            "bedrock opens a hopper",
            "bedrock puts items in a hopper",
            "bedrock takes items from a hopper",
            "bedrock closes a hopper",
        ],
    },
    ContainerCase {
        block: "dispenser",
        slot: 8,
        names: [
            "bedrock opens a dispenser",
            "bedrock puts items in a dispenser",
            "bedrock takes items from a dispenser",
            "bedrock closes a dispenser",
        ],
    },
    ContainerCase {
        block: "trapped_chest",
        slot: 5,
        names: [
            "bedrock opens a trapped chest",
            "bedrock puts items in a trapped chest",
            "bedrock takes items from a trapped chest",
            "bedrock closes a trapped chest",
        ],
    },
];

async fn run_container_case(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    case: &ContainerCase,
) -> Result<(), String> {
    let [open_name, _, _, close_name] = case.names;
    run_console(server, &format!("op {BEDROCK_NAME}"));
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("give {BEDROCK_NAME} diamond 5"));
    let feet = f64::from(bedrock.position.y) - f64::from(EntityType::PLAYER.eye_height);
    let side = if java.position.x > f64::from(bedrock.position.x) {
        -1.0
    } else {
        1.0
    };
    let chest = BlockPos::floored(
        f64::from(bedrock.position.x) + side * 2.0,
        feet,
        f64::from(bedrock.position.z),
    );
    let setblock = |pos: BlockPos, block: &str| {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    };
    setblock(chest.up(), "air");
    setblock(chest, case.block);
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }

    let diamonds = ItemStack::new(5, &Item::DIAMOND);
    bedrock
        .use_item_on_block(chest, 1, 0, &diamonds, &[])
        .await?;
    checks.record(
        open_name,
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a ContainerOpen on the Bedrock bot",
            |bot, _| bot.open_container.is_some(),
        )
        .await,
    );

    run_container_moves(server, bedrock, java, checks, case, chest).await?;

    bedrock.close_container().await?;
    checks.record(
        close_name,
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the close echoed back to the Bedrock bot",
            |bot, _| bot.open_container.is_none(),
        )
        .await,
    );

    // Breaking the container drops what is left in it; nobody should pick that up later.
    setblock(chest, "air");
    tick(server, bedrock, Some(&mut *java)).await?;
    run_console(server, "kill @e[type=item]");
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// The put and take of `run_container_case`, with the container open.
async fn run_container_moves(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    case: &ContainerCase,
    chest: BlockPos,
) -> Result<(), String> {
    let [_, put_name, take_name, _] = case.names;
    let bedrock_player = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or("the Bedrock bot is not on the server")?;
    let world = bedrock_player.world();
    let chest_stack = |slot| {
        world
            .get_block_entity(&chest)
            .and_then(pumpkin_core::block::entities::BlockEntity::get_inventory)
            .map(|inventory| inventory.get_stack(slot))
    };
    let player_stack = |slot| bedrock_player.inventory().get_stack(slot);
    let moves = |count, from, to| {
        [
            StackMove {
                kind: StackMoveKind::Take,
                count,
                from,
                to: (ContainerName::Cursor, 0),
            },
            StackMove {
                kind: StackMoveKind::Place,
                count,
                from: (ContainerName::Cursor, 0),
                to,
            },
        ]
    };

    // All 5 diamonds from hotbar slot 0 into the chest's first slot.
    bedrock
        .item_stack_request(
            1,
            &moves(
                5,
                (ContainerName::HotBar, 0),
                (ContainerName::LevelEntity, case.slot),
            ),
        )
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let in_chest = chest_stack(usize::from(case.slot)).ok_or("the chest has no inventory")?;
    checks.record(
        put_name,
        if !bedrock.stack_responses.contains(&(1, 0)) {
            Err(format!("responses: {:?}", bedrock.stack_responses))
        } else if in_chest.item.id != Item::DIAMOND.id || in_chest.item_count != 5 {
            Err(format!(
                "the chest holds {} {}",
                in_chest.item_count, in_chest.item.registry_key
            ))
        } else if !player_stack(0).is_empty() {
            Err("the diamonds are still in the hotbar".to_string())
        } else {
            Ok(())
        },
    );

    // 2 of them back, into hotbar slot 1.
    bedrock
        .item_stack_request(
            2,
            &moves(
                2,
                (ContainerName::LevelEntity, case.slot),
                (ContainerName::HotBar, 1),
            ),
        )
        .await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let in_chest = chest_stack(usize::from(case.slot)).ok_or("the chest has no inventory")?;
    let taken = player_stack(1);
    checks.record(
        take_name,
        if !bedrock.stack_responses.contains(&(2, 0)) {
            Err(format!("responses: {:?}", bedrock.stack_responses))
        } else if in_chest.item_count != 3 {
            Err(format!("the chest holds {} diamonds", in_chest.item_count))
        } else if taken.item.id != Item::DIAMOND.id || taken.item_count != 2 {
            Err(format!(
                "hotbar slot 1 holds {} {}",
                taken.item_count, taken.item.registry_key
            ))
        } else {
            Ok(())
        },
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
