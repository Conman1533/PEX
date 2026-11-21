use std::{net::SocketAddr, sync::Arc, time::Duration, time::Instant};

use pumpkin_core::server::Server;
use pumpkin_data::translation;

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

const NAME: &str = "DupLoginBot";

/// Vanilla `disconnectAllPlayersWithProfile`: a second login with the same profile kicks the
/// player already online with `multiplayer.disconnect.duplicate_login` and lets the new one in.
pub async fn run_duplicate_login_test(
    server: &Arc<Server>,
    addr: SocketAddr,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    duplicate_login_kicks_the_old_session(server, addr)
        .await
        .map_err(|err| format!("Duplicate login: {err}"))?;
    summary_lines.push(format!(
        "[PASS] bot:duplicate_login_kicks_old_session ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

async fn duplicate_login_kicks_the_old_session(
    server: &Arc<Server>,
    addr: SocketAddr,
) -> Result<(), String> {
    let mut first = HeadlessBot::connect_and_join(addr, NAME).await?;
    let mut old_player = None;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        first.poll_packets().await?;
        old_player = server.get_player_by_name(NAME);
        if old_player
            .as_ref()
            .is_some_and(|player| player.has_client_loaded())
        {
            break;
        }
    }
    let old_id = old_player
        .ok_or_else(|| format!("{NAME} did not join"))?
        .entity_id();

    let second = tokio::spawn(HeadlessBot::connect_and_join(addr, NAME));
    let mut first_open = true;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !second.is_finished() {
        if Instant::now() > deadline {
            second.abort();
            return Err("the second login did not finish within 20 s".to_string());
        }
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        if first_open && first.poll_packets().await.is_err() {
            first_open = false;
        }
    }
    let mut second = second
        .await
        .map_err(|err| format!("the second login task failed: {err}"))?
        .map_err(|err| format!("the second login was refused: {err}"))?;

    let key = translation::java::MULTIPLAYER_DISCONNECT_DUPLICATE_LOGIN.as_bytes();
    let kicked = first
        .disconnect_reason
        .as_ref()
        .is_some_and(|reason| reason.windows(key.len()).any(|window| window == key));
    if !kicked {
        return Err("the first session was not kicked with duplicate_login".to_string());
    }

    let mut new_player = None;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        second.poll_packets().await?;
        new_player = server
            .get_player_by_name(NAME)
            .filter(|player| player.entity_id() != old_id && player.has_client_loaded());
        if new_player.is_some() {
            break;
        }
    }
    if new_player.is_none() {
        return Err("the second session is not the player online".to_string());
    }
    let online = server
        .get_all_players()
        .iter()
        .filter(|player| player.gameprofile.name == NAME)
        .count();
    if online != 1 {
        return Err(format!("{online} players named {NAME} are online, expected 1"));
    }

    drop(second);
    for _ in 0..MAX_POLL_TICKS {
        if server.get_player_by_name(NAME).is_none() {
            break;
        }
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(())
}
