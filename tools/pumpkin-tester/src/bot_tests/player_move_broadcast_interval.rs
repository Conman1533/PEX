use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{command::CommandSender, server::Server};
use pumpkin_protocol::java::client::play::{
    CEntityPositionSync, CHeadRot, CSpawnEntity, CUpdateEntityPos, CUpdateEntityPosRot,
    CUpdateEntityRot,
};

use pumpkin_util::math::vector3::Vector3;

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// Ticks bot A moves for, one position packet per tick.
const MOVE_TICKS: u32 = 40;
/// Blocks bot A moves along x each tick.
const STEP: f64 = 0.1;
/// Bot A's fixed look direction while it moves.
const YAW: f32 = 90.0;
const PITCH: f32 = 0.0;
/// Polls, 20 ms apart and without ticking, for the packets of the last counted tick.
const DRAIN_POLLS: u32 = 10;
/// Ticks [`settle`] waits before its ping: two of the player type's update interval of 2.
const SETTLE_TICKS: u32 = 4;

/// Vanilla `ChunkMap.TrackedEntity` sends other players a player's movement from
/// `ServerEntity.sendChanges` at the player type's update interval of 2, and head rotation only
/// when it changes. Pumpkin used to send a movement packet and a head rotation for every movement
/// packet the client sent.
pub async fn run_player_move_broadcast_interval_test(
    server: &Arc<Server>,
    mover: &mut HeadlessBot,
    watcher: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let mover_id = server
        .get_player_by_name(&mover.name)
        .ok_or("Player move broadcast: could not find the moving bot")?
        .entity_id();

    let mut paired = false;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        mover.poll_packets().await?;
        watcher.poll_packets().await?;
        if watcher.entity_packet_count::<CSpawnEntity>(mover_id) > 0 {
            paired = true;
            break;
        }
    }
    if !paired {
        return Err("Player move broadcast: the watcher never saw the mover spawn".to_string());
    }

    clear_path(server, mover.position)?;

    // Turn to the fixed look first, and let that rotation go out before counting.
    let mut position = mover.position;
    mover.move_and_look(position, YAW, PITCH, true).await?;
    settle(server, mover, watcher).await?;
    watcher.entity_packets.clear();

    for _ in 0..MOVE_TICKS {
        position.x += STEP;
        mover.move_and_look(position, YAW, PITCH, true).await?;
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        mover.poll_packets().await?;
        watcher.poll_packets().await?;
    }
    // Count only what these 40 ticks sent: no more ticks, just let the last packets arrive.
    for _ in 0..DRAIN_POLLS {
        tokio::time::sleep(Duration::from_millis(20)).await;
        watcher.poll_packets().await?;
    }

    let movement = watcher.entity_packet_count::<CUpdateEntityPos>(mover_id)
        + watcher.entity_packet_count::<CUpdateEntityPosRot>(mover_id)
        + watcher.entity_packet_count::<CUpdateEntityRot>(mover_id)
        + watcher.entity_packet_count::<CEntityPositionSync>(mover_id);
    let head = watcher.entity_packet_count::<CHeadRot>(mover_id);
    if !(19..=21).contains(&movement) || head != 0 {
        return Err(format!(
            "Player move broadcast: the watcher got {movement} movement packets and {head} head \
             rotations for {MOVE_TICKS} ticks of movement with a fixed look; expected 19 to 21 \
             and none"
        ));
    }
    summary_lines.push(format!(
        "[PASS] bot:player_move_broadcast_interval ({movement} movement packets, {head} head \
         rotations in {MOVE_TICKS} ticks, {}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Ticks [`SETTLE_TICKS`], then until the server has answered a ping from each bot, so
/// everything the earlier ticks sent has arrived. The tracker sends a player's changes every
/// second tick, so a change made just before can still go out during these ticks.
async fn settle(
    server: &Arc<Server>,
    mover: &mut HeadlessBot,
    watcher: &mut HeadlessBot,
) -> Result<(), String> {
    for _ in 0..SETTLE_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        mover.poll_packets().await?;
        watcher.poll_packets().await?;
    }
    let mover_ping = mover.send_ping().await?;
    let watcher_ping = watcher.send_ping().await?;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        mover.poll_packets().await?;
        watcher.poll_packets().await?;
        if mover.has_pong(mover_ping) && watcher.has_pong(watcher_ping) {
            return Ok(());
        }
    }
    Err("Player move broadcast: the server did not answer a ping".to_string())
}

/// Clears the blocks along the mover's path and puts stone under it. Only the movement itself
/// may change while counting: whatever the tester seed leaves at spawn (leaves, a slope) changes
/// the mover's entity data, and vanilla `ServerEntity.sendChanges` sends a movement packet
/// outside the update interval whenever that data is dirty.
fn clear_path(server: &Arc<Server>, start: Vector3<f64>) -> Result<(), String> {
    let x = start.x.floor() as i32;
    let y = start.y.floor() as i32;
    let z = start.z.floor() as i32;
    let reach = (f64::from(MOVE_TICKS) * STEP).ceil() as i32 + 1;
    for command in [
        format!(
            "fill {} {y} {} {} {} {} air",
            x - 1,
            z - 1,
            x + reach,
            y + 2,
            z + 1
        ),
        format!(
            "fill {} {} {} {} {} {} stone",
            x - 1,
            y - 1,
            z - 1,
            x + reach,
            y - 1,
            z + 1
        ),
    ] {
        server
            .command_dispatcher
            .load()
            .execute_input(&command, &CommandSender::Console.into_source(server))
            .map_err(|err| {
                format!(
                    "Player move broadcast: '{command}' failed: {}",
                    err.message.get_text()
                )
            })?;
    }
    Ok(())
}
