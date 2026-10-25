use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{command::CommandSender, entity::player::Player, server::Server};
use pumpkin_data::dimension::Dimension;
use pumpkin_protocol::java::client::play::CPlayerPosition;
use pumpkin_util::math::vector3::Vector3;

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// Ticks the bot leaves a teleport unconfirmed before moving again. Vanilla
/// `updateAwaitingTeleport` sends it again on a movement packet more than 20 ticks after it.
const RESEND_WAIT_TICKS: usize = 25;
/// The targets are block centres, so their coordinates are exact; this only absorbs rounding.
const POSITION_TOLERANCE: f64 = 1.0e-6;

/// A lagging client answers a teleport after the server already sent the next one. Vanilla
/// `handleAcceptTeleportPacket` ignores confirmations of anything but the last teleport, and
/// `updateAwaitingTeleport` sends an unconfirmed teleport again when the client keeps moving.
pub async fn run_teleport_confirm_tests(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let Some(player) = world.get_player_by_name("TestBot1") else {
        return Err("Could not find 'TestBot1' in Overworld".to_string());
    };

    // Earlier tests may have left teleports to confirm; let the bot answer them first.
    sync(server, bot1, &player).await?;
    let start_position = bot1.position;
    bot1.held_teleports = Some(Vec::new());
    let mut result = late_confirmation_is_ignored(server, bot1, &player)
        .await
        .map_err(|err| format!("Late teleport confirmation: {err}"));
    if result.is_ok() {
        result = unconfirmed_teleport_is_resent(server, bot1, &player)
            .await
            .map_err(|err| format!("Unconfirmed teleport: {err}"));
    }
    bot1.held_teleports = None;
    result?;
    // The tests that follow build around the bot's position.
    walk(server, bot1, &player, start_position).await?;

    summary_lines.push(format!(
        "[PASS] bot:teleport_confirm_late_and_resend ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Two `/tp`s in a row; the bot confirms the first only after the second was sent, then sends
/// the position the first one gave it, like the vanilla client does after a confirmation.
async fn late_confirmation_is_ignored(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
) -> Result<(), String> {
    let origin = block_centre(bot.position);
    let first_target = origin + Vector3::new(4.0, 0.0, 0.0);
    let second_target = origin + Vector3::new(8.0, 0.0, 0.0);
    console_tp(server, first_target)?;
    console_tp(server, second_target)?;
    let [first, second] = held_teleports(server, bot, player).await?;
    expect_near(first.position, first_target, "first teleport")?;
    expect_near(second.position, second_target, "second teleport")?;

    bot.confirm_teleport(&first).await?;
    sync(server, bot, player).await?;
    expect_near(
        player.living_entity.entity.pos.load(),
        second_target,
        "server position after the late confirmation",
    )?;

    confirm_with_resends(server, bot, player, second).await?;
    walk(
        server,
        bot,
        player,
        second_target + Vector3::new(1.0, 0.0, 0.0),
    )
    .await
}

/// A `/tp` the bot doesn't confirm while it keeps sending its old position: the server ignores
/// the position and sends the teleport again with a new id, after which only that id counts.
async fn unconfirmed_teleport_is_resent(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
) -> Result<(), String> {
    let old_position = bot.position;
    let target = block_centre(old_position) + Vector3::new(4.0, 0.0, 0.0);
    console_tp(server, target)?;
    let [teleport] = held_teleports(server, bot, player).await?;

    tick(server, bot, player, RESEND_WAIT_TICKS).await?;
    bot.move_to(old_position, true).await?;
    let [resent] = held_teleports(server, bot, player).await?;
    expect_near(
        player.living_entity.entity.pos.load(),
        target,
        "server position after moving without confirming",
    )?;
    if resent.teleport_id == teleport.teleport_id {
        return Err(format!(
            "resent teleport reused id {}",
            teleport.teleport_id.0
        ));
    }
    expect_near(resent.position, target, "resent teleport")?;

    bot.confirm_teleport(&teleport).await?;
    sync(server, bot, player).await?;
    confirm_with_resends(server, bot, player, resent).await?;
    walk(server, bot, player, target + Vector3::new(1.0, 0.0, 0.0)).await
}

fn block_centre(position: Vector3<f64>) -> Vector3<f64> {
    Vector3::new(
        position.x.floor() + 0.5,
        position.y.floor(),
        position.z.floor() + 0.5,
    )
}

fn console_tp(server: &Arc<Server>, target: Vector3<f64>) -> Result<(), String> {
    let command = format!(
        "tp TestBot1 {:.1} {:.1} {:.1}",
        target.x, target.y, target.z
    );
    server
        .command_dispatcher
        .load()
        .execute_input(&command, &CommandSender::Console.into_source(server))
        .map(|_| ())
        .map_err(|err| format!("'{command}' failed: {}", err.message.get_text()))
}

/// Server ticks, each followed by whatever the bot received.
async fn tick(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
    ticks: usize,
) -> Result<(), String> {
    for _ in 0..ticks {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        if player.client.closed() {
            return Err("TestBot1 was disconnected".to_string());
        }
        bot.poll_packets().await?;
    }
    Ok(())
}

/// Ticks until the server has handled everything the bot sent so far and the bot has read the
/// answers.
async fn sync(server: &Arc<Server>, bot: &mut HeadlessBot, player: &Player) -> Result<(), String> {
    let ping = bot.send_ping().await?;
    for _ in 0..MAX_POLL_TICKS {
        tick(server, bot, player, 1).await?;
        if bot.has_pong(ping) {
            return Ok(());
        }
    }
    Err("the server did not answer a ping".to_string())
}

fn take_held_teleports(bot: &mut HeadlessBot) -> Vec<CPlayerPosition> {
    bot.held_teleports.replace(Vec::new()).unwrap_or_default()
}

/// Syncs, then takes the teleports the bot held back, which must be exactly `N`.
async fn held_teleports<const N: usize>(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
) -> Result<[CPlayerPosition; N], String> {
    sync(server, bot, player).await?;
    let held = take_held_teleports(bot);
    let count = held.len();
    held.try_into()
        .map_err(|_| format!("expected {N} teleports, got {count}"))
}

/// Confirms `teleport`, then every copy of it the server sent again meanwhile. On a busy machine
/// the server can take more than 20 ticks to read the position that follows a confirmation, and
/// then sends the teleport again like vanilla.
async fn confirm_with_resends(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
    teleport: CPlayerPosition,
) -> Result<(), String> {
    let target = teleport.position;
    let mut pending = vec![teleport];
    pending.extend(take_held_teleports(bot));
    while !pending.is_empty() {
        for teleport in &pending {
            expect_near(teleport.position, target, "resent teleport")?;
            bot.confirm_teleport(teleport).await?;
        }
        sync(server, bot, player).await?;
        pending = take_held_teleports(bot);
    }
    Ok(())
}

/// Moves the bot and checks the server took the position.
async fn walk(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
    target: Vector3<f64>,
) -> Result<(), String> {
    bot.move_to(target, true).await?;
    sync(server, bot, player).await?;
    expect_near(
        player.living_entity.entity.pos.load(),
        target,
        "server position after moving",
    )
}

fn expect_near(actual: Vector3<f64>, expected: Vector3<f64>, what: &str) -> Result<(), String> {
    if (actual - expected).length() > POSITION_TOLERANCE {
        return Err(format!("expected {what} at {expected:?}, got {actual:?}"));
    }
    Ok(())
}
