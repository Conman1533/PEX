//! Bedrock tests: a server with Bedrock on, a [`BedrockBot`] joining it over `NetherNet` the way
//! a 1.26.60 client does, and a Java [`HeadlessBot`] next to it for the cross-play checks.

mod bot;
mod transport;

use std::{
    collections::HashMap, future::Future, net::SocketAddr, path::Path, sync::Arc, time::Duration,
};

use pumpkin_core::{PumpkinServer, server::Server};
use pumpkin_data::entity::EntityType;
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

/// A test server with Java and Bedrock listeners on loopback, ticked by the tests.
pub struct BedrockTestServer {
    pub server: Arc<Server>,
    pub java_addr: SocketAddr,
    pub signaling_addr: SocketAddr,
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
        tasks.spawn(async move {
            let clients = Arc::new(Mutex::new(HashMap::new()));
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
    result
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
