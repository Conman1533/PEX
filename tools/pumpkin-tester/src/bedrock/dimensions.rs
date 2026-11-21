//! Dimension changes for the Bedrock bot: commands, a nether portal, respawning into another
//! dimension, and the end credits.

use std::sync::Arc;

use pumpkin_core::entity::{EntityBase, player::Player};
use pumpkin_data::{Block, dimension::Dimension};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::{
    BEDROCK_NAME, BedrockBot, BedrockTestServer, Checks, HeadlessBot, run_console, tick, tick_until,
};

/// Bedrock's dimension ids in `ChangeDimension`.
const OVERWORLD: i32 = 0;
const NETHER: i32 = 1;

/// Where the command checks send the bot in the nether.
const NETHER_SPOT: (i32, i32, i32) = (8, 100, 8);

fn dimension_name(player: &Player) -> &'static str {
    let dimension = &player.world().dimension;
    if *dimension == Dimension::THE_NETHER {
        "the nether"
    } else if *dimension == Dimension::THE_END {
        "the end"
    } else {
        "the overworld"
    }
}

/// Waits for a `ChangeDimension` to `dimension` after the first `seen` ones, then for the
/// loading screen to end (a `PlayerSpawn` and chunks of the new dimension), and acknowledges it
/// as a real client does.
async fn arrive(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    seen: usize,
    dimension: i32,
) -> Result<(), String> {
    let spawns = bedrock.spawn_statuses;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a ChangeDimension",
        |bot, _| {
            bot.dimension_changes[seen..]
                .iter()
                .any(|(id, _, _)| *id == dimension)
        },
    )
    .await?;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a PlayerSpawn and chunks of the new dimension",
        |bot, _| bot.spawn_statuses > spawns && !bot.chunks.is_empty(),
    )
    .await?;
    bedrock.ack_dimension_change().await?;
    for _ in 0..5 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(())
}

pub(super) async fn run_dimension_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player: Arc<Player> = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or_else(|| format!("{BEDROCK_NAME} is not on the server"))?;
    run_console(server, &format!("gamemode creative {BEDROCK_NAME}"));
    let home = player.get_entity().pos.load();

    // A command teleport into the nether, and back.
    let seen = bedrock.dimension_changes.len();
    let (x, y, z) = NETHER_SPOT;
    run_console(
        server,
        &format!("execute in minecraft:the_nether run tp {BEDROCK_NAME} {x} {y} {z}"),
    );
    checks.record(
        "bedrock is sent to the nether",
        arrive(server, bedrock, java, seen, NETHER).await,
    );
    let pos = player.get_entity().pos.load();
    checks.record(
        "bedrock stands where it was sent in the nether",
        if dimension_name(&player) == "the nether" && (pos.y - f64::from(y)).abs() < 0.1 {
            Ok(())
        } else {
            Err(format!(
                "server has it at {pos:?} in {}",
                dimension_name(&player)
            ))
        },
    );
    let seen = bedrock.dimension_changes.len();
    run_console(
        server,
        &format!(
            "execute in minecraft:overworld run tp {BEDROCK_NAME} {} {} {}",
            home.x, home.y, home.z
        ),
    );
    checks.record(
        "bedrock comes back to the overworld",
        arrive(server, bedrock, java, seen, OVERWORLD)
            .await
            .and_then(|()| {
                if dimension_name(&player) == "the overworld" {
                    Ok(())
                } else {
                    Err(format!("server has it in {}", dimension_name(&player)))
                }
            }),
    );

    run_portal_checks(server, bedrock, java, checks, &player, home).await?;
    run_respawn_checks(server, bedrock, java, checks, &player, home).await?;
    run_credits_checks(server, bedrock, java, checks, &player).await?;

    run_console(server, &format!("gamemode creative {BEDROCK_NAME}"));
    Ok(())
}

/// Walking into a nether portal frame built beside the bot.
async fn run_portal_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
    home: Vector3<f64>,
) -> Result<(), String> {
    let (fx, fy, fz) = (
        home.x.floor() as i32 + 3,
        home.y.floor() as i32,
        home.z.floor() as i32,
    );
    run_console(
        server,
        &format!(
            "fill {fx} {} {fz} {} {} {fz} obsidian",
            fy - 1,
            fx + 3,
            fy + 3
        ),
    );
    run_console(
        server,
        &format!(
            "fill {} {fy} {fz} {} {} {fz} nether_portal",
            fx + 1,
            fx + 2,
            fy + 2
        ),
    );
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let eye = bedrock.position.y - (home.y as f32);
    let seen = bedrock.dimension_changes.len();
    // Step into the portal and stand there, as a player walking in does.
    bedrock.position = Vector3::new(fx as f32 + 1.5, fy as f32 + eye, fz as f32 + 0.5);
    checks.record(
        "bedrock goes through a nether portal",
        arrive(server, bedrock, java, seen, NETHER)
            .await
            .and_then(|()| {
                if dimension_name(player) == "the nether" {
                    Ok(())
                } else {
                    Err(format!("server has it in {}", dimension_name(player)))
                }
            }),
    );
    let feet = player.get_entity().pos.load();
    let world = player.world();
    let at_feet = BlockPos::new(
        feet.x.floor() as i32,
        feet.y.floor() as i32,
        feet.z.floor() as i32,
    );
    checks.record(
        "bedrock arrives in a nether portal",
        if dimension_name(player) == "the nether"
            && Block::from_state_id(world.get_block_state(&at_feet).id).name
                == Block::NETHER_PORTAL.name
        {
            Ok(())
        } else {
            Err(format!(
                "at {at_feet:?} in {} there is {}",
                dimension_name(player),
                Block::from_state_id(world.get_block_state(&at_feet).id).name
            ))
        },
    );
    drop(world);

    let seen = bedrock.dimension_changes.len();
    run_console(
        server,
        &format!(
            "execute in minecraft:overworld run tp {BEDROCK_NAME} {} {} {}",
            home.x, home.y, home.z
        ),
    );
    if dimension_name(player) != "the overworld" {
        arrive(server, bedrock, java, seen, OVERWORLD).await?;
    }
    run_console(
        server,
        &format!("fill {fx} {} {fz} {} {} {fz} air", fy - 1, fx + 3, fy + 3),
    );
    Ok(())
}

/// Dying in the nether with a spawn point in the overworld.
async fn run_respawn_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
    home: Vector3<f64>,
) -> Result<(), String> {
    run_console(
        server,
        &format!(
            "execute in minecraft:overworld run spawnpoint {BEDROCK_NAME} {} {} {}",
            home.x.floor(),
            home.y.floor(),
            home.z.floor()
        ),
    );
    let seen = bedrock.dimension_changes.len();
    let (x, y, z) = NETHER_SPOT;
    run_console(
        server,
        &format!("execute in minecraft:the_nether run tp {BEDROCK_NAME} {x} {y} {z}"),
    );
    arrive(server, bedrock, java, seen, NETHER).await?;
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));
    let states = bedrock.respawn_states.len();
    run_console(server, &format!("kill {BEDROCK_NAME}"));
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a Respawn telling the bot it died",
        |bot, _| bot.respawn_states.len() > states,
    )
    .await?;
    let states = bedrock.respawn_states.len();
    bedrock.ready_to_respawn().await?;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a Respawn saying the bot may spawn",
        |bot, _| bot.respawn_states[states..].contains(&1),
    )
    .await?;
    let seen = bedrock.dimension_changes.len();
    bedrock.respawn().await?;
    let arrived = arrive(server, bedrock, java, seen, OVERWORLD).await;
    let respawn_flag = bedrock.dimension_changes[seen..]
        .iter()
        .any(|(id, _, respawn)| *id == OVERWORLD && *respawn);
    checks.record(
        "bedrock respawns in the overworld from the nether",
        arrived.and_then(|()| {
            if dimension_name(player) == "the overworld"
                && player.living_entity.health.load() > 0.0
                && respawn_flag
            {
                Ok(())
            } else {
                Err(format!(
                    "in {} with {} health, respawn flag {respawn_flag}",
                    dimension_name(player),
                    player.living_entity.health.load()
                ))
            }
        }),
    );
    run_console(server, &format!("gamemode creative {BEDROCK_NAME}"));
    Ok(())
}

/// Leaving the end through an end portal for the first time shows the credits, and the player
/// comes back to the overworld only once they're closed, as on Java.
async fn run_credits_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    const END: i32 = 2;
    let seen = bedrock.dimension_changes.len();
    run_console(
        server,
        &format!("execute in minecraft:the_end run tp {BEDROCK_NAME} 100 50 0"),
    );
    arrive(server, bedrock, java, seen, END).await?;
    run_console(
        server,
        "execute in minecraft:the_end run fill 99 48 -1 101 48 1 obsidian",
    );
    let credits = bedrock.credits.len();
    let seen = bedrock.dimension_changes.len();
    run_console(
        server,
        "execute in minecraft:the_end run setblock 100 50 0 end_portal",
    );
    checks.record(
        "bedrock sees the end credits",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a ShowCredits start",
            |bot, _| bot.credits[credits..].contains(&0),
        )
        .await,
    );
    for _ in 0..20 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    checks.record(
        "bedrock stays put while the credits roll",
        if bedrock.dimension_changes.len() == seen && dimension_name(player) == "the end" {
            Ok(())
        } else {
            Err(format!(
                "moved to {} before the credits closed",
                dimension_name(player)
            ))
        },
    );
    run_console(
        server,
        "execute in minecraft:the_end run setblock 100 50 0 air",
    );
    bedrock.finish_credits().await?;
    checks.record(
        "bedrock returns to the overworld after the credits",
        arrive(server, bedrock, java, seen, OVERWORLD)
            .await
            .and_then(|()| {
                if dimension_name(player) == "the overworld" {
                    Ok(())
                } else {
                    Err(format!("server has it in {}", dimension_name(player)))
                }
            }),
    );
    Ok(())
}
