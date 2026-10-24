use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{server::Server, world::BlockFlags};
use pumpkin_data::{Block, dimension::Dimension, packet::CURRENT_MC_VERSION};
use pumpkin_protocol::{
    java::client::play::{CAcknowledgeBlockChange, CBlockUpdate},
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{GameMode, math::position::BlockPos};

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

// Above the sequences the other bot tests use, so an older acknowledgement can't match.
const EARLY_START_SEQUENCE: i32 = 100;
const EARLY_FINISH_SEQUENCE: i32 = 101;

/// Survival digging is timed by the server: a finish sent right after the start must not break
/// the block, a finish after the full digging time must.
pub async fn run_block_breaking_tests(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let Some(player) = world.get_player_by_name("TestBot1") else {
        return Err("Could not find 'TestBot1' in Overworld".to_string());
    };
    let original_mode = player.gamemode.load();
    player.set_gamemode(GameMode::Survival);

    let base_x = bot1.position.x.floor() as i32;
    let base_y = bot1.position.y.floor() as i32;
    let base_z = bot1.position.z.floor() as i32;
    // Above the bot's head, in the chunk it stands in, which is loaded.
    let early_pos = BlockPos::new(base_x, base_y + 2, base_z);
    let full_pos = BlockPos::new(base_x, base_y + 3, base_z);

    let mut result = early_finish_keeps_block(server, bot1, early_pos).await;
    if result.is_ok() {
        result = full_dig_breaks_block(server, bot1, full_pos).await;
    }

    for pos in [early_pos, full_pos] {
        world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    }
    player.set_gamemode(original_mode);
    result?;

    summary_lines.push(format!(
        "[PASS] bot:survival_dig_timing ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Obsidian by hand takes 5000 ticks. Vanilla answers a finish sent at once with only the block
/// change acknowledgement, which makes the client put back the block it predicted broken.
async fn early_finish_keeps_block(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    pos: BlockPos,
) -> Result<(), String> {
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let obsidian = Block::OBSIDIAN.default_state.id;
    world.set_block_state(&pos, obsidian, BlockFlags::NOTIFY_ALL);
    if world.get_block_state_id(&pos) != obsidian {
        return Err(format!("Could not place obsidian at {pos}"));
    }

    bot1.start_digging(pos, EARLY_START_SEQUENCE).await?;
    bot1.finish_digging(pos, EARLY_FINISH_SEQUENCE).await?;
    tick(server, 40).await;

    if world.get_block_state_id(&pos) != obsidian {
        return Err(format!(
            "A dig finished right after it started broke the obsidian at {pos}"
        ));
    }

    let ack_id = CAcknowledgeBlockChange::to_id(CURRENT_MC_VERSION);
    let block_update_id = CBlockUpdate::to_id(CURRENT_MC_VERSION);
    loop {
        let packet = bot1.next_packet().await?;
        let mut payload = &packet.payload[..];
        if packet.id == block_update_id {
            let updated = payload
                .get_block_pos(&CURRENT_MC_VERSION)
                .map_err(|err| format!("Failed to read block update: {err}"))?;
            let state = payload
                .get_var_int()
                .map_err(|err| format!("Failed to read block update: {err}"))?;
            if updated == pos && state.0 != i32::from(obsidian.as_u16()) {
                return Err(format!(
                    "Server sent state {} for the obsidian at {pos} after an early dig finish",
                    state.0
                ));
            }
        } else if packet.id == ack_id {
            let sequence = payload
                .get_var_int()
                .map_err(|err| format!("Failed to read block change ack: {err}"))?;
            if sequence.0 >= EARLY_FINISH_SEQUENCE {
                break;
            }
        }
    }

    // Removing the block ends the delayed destroy the early finish left behind.
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    tick(server, 2).await;
    Ok(())
}

/// Dirt by hand takes 15 ticks on the ground and 75 in the air.
async fn full_dig_breaks_block(
    server: &Arc<Server>,
    bot1: &mut HeadlessBot,
    pos: BlockPos,
) -> Result<(), String> {
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    world.set_block_state(&pos, Block::DIRT.default_state.id, BlockFlags::NOTIFY_ALL);
    if world.get_block(&pos).id != Block::DIRT.id {
        return Err(format!("Could not place dirt at {pos}"));
    }

    bot1.start_digging(pos, 102).await?;
    tick(server, 80).await;
    bot1.finish_digging(pos, 103).await?;
    for _ in 0..MAX_POLL_TICKS {
        tick(server, 1).await;
        if world.get_block(&pos).id == Block::AIR.id {
            return Ok(());
        }
    }
    Err(format!(
        "A dig finished after the full digging time did not break the dirt at {pos}"
    ))
}

async fn tick(server: &Arc<Server>, ticks: usize) {
    for _ in 0..ticks {
        tokio::time::sleep(Duration::from_millis(5)).await;
        server.tick();
    }
}
