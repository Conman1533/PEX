//! Block entity changes reach the disk with their chunk, as vanilla saves a chunk's block entities
//! when it unloads the chunk (`ChunkMap.scheduleUnload`). Hopper lines carry eggs into chests by a
//! bot, the bot leaves, the chunks unload, and once the server has stopped the chests in the saved
//! region files must hold the eggs.

use std::{
    cell::Cell,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_config::chunk::ChunkConfig;
use pumpkin_core::{
    block::entities::BlockEntity,
    server::Server,
    world::{BlockFlags, World},
};
use pumpkin_data::{
    Block,
    block_properties::{FacingHopper, HopperLikeProperties},
    item::Item,
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_world::chunk::io::{converter::parse_region_file_name, editor::edit_region};

use crate::survival::StageContext;

const BOT_NAME: &str = "UnloadBot";
/// The bot's chunk: the survival stage's, so the chunks around it are generated already.
const BOT_CHUNK_X: i32 = -12;
const BOT_CHUNK_Z: i32 = -12;
/// High above the terrain, so nothing needs clearing.
const LINE_Y: i32 = 240;
/// The chunks of the hopper lines, as x offsets from the bot's chunk. One line is in the bot's own
/// chunk, which the bot watches. The other is 7 chunks west: inside the simulation distance (10),
/// so its hoppers tick, but outside the bot's view distance (4), so the bot never watches it.
const LINE_CHUNK_OFFSETS: [i32; 2] = [0, -7];
const HOPPERS_PER_LINE: i32 = 3;
const EGGS: u8 = 5;
/// Ticks for the eggs to reach the chests (a hopper moves one item every 8 ticks), and for the
/// chunks to load or unload.
const MAX_TICKS: usize = 2000;
const TICK_PAUSE: Duration = Duration::from_millis(5);

/// The chests whose eggs the saved world has to keep, for [`check_saved_chests`].
pub struct UnloadedChests {
    chests: Vec<BlockPos>,
    region_folder: PathBuf,
    chunk_config: ChunkConfig,
    start: Instant,
}

/// Fills a chest through each hopper line, then lets the bot leave and waits until the chunks of
/// the lines are unloaded. Runs with no other player online.
pub async fn run_block_entity_unload_test(
    server: &Arc<Server>,
    addr: SocketAddr,
) -> Result<UnloadedChests, String> {
    let start = Instant::now();
    let mut stage = StageContext::join(server, addr, BOT_NAME).await?;
    let world = stage.world();
    // A block's centre, where `tp` puts whole coordinates too.
    let stand_at = Vector3::new(
        f64::from(BOT_CHUNK_X * 16 + 8) + 0.5,
        f64::from(LINE_Y + 2),
        f64::from(BOT_CHUNK_Z * 16 + 8) + 0.5,
    );
    stage.console(&format!(
        "tp {BOT_NAME} {} {} {}",
        stand_at.x, stand_at.y, stand_at.z
    ))?;
    stage
        .tick_until("the teleport to arrive", MAX_TICKS, |bot| {
            bot.position == stand_at
        })
        .await?;

    let mut hopper = HopperLikeProperties::default(&Block::HOPPER);
    hopper.facing = FacingHopper::East;
    let hopper = hopper.to_state_id(&Block::HOPPER);
    let mut chests = Vec::new();
    for chunk_offset in LINE_CHUNK_OFFSETS {
        let first_hopper = BlockPos::new(
            (BOT_CHUNK_X + chunk_offset) * 16 + 4,
            LINE_Y,
            BOT_CHUNK_Z * 16 + 4,
        );
        let chest = first_hopper.offset(Vector3::new(HOPPERS_PER_LINE, 0, 0));
        let chunk = chest.chunk_position();
        // The bot's tickets load the chunk; the far one is never sent to the bot.
        tick_until(&mut stage, &format!("chunk {chunk:?} to load"), || {
            world.level.is_chunk_loaded(&chunk)
        })
        .await?;
        for x in 0..HOPPERS_PER_LINE {
            world.set_block_state(
                &first_hopper.offset(Vector3::new(x, 0, 0)),
                hopper,
                BlockFlags::NOTIFY_ALL,
            );
        }
        world.set_block_state(
            &chest,
            Block::CHEST.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        let Vector3 { x, y, z } = first_hopper.0;
        stage.console(&format!(
            "item replace block {x} {y} {z} container.0 with minecraft:egg {EGGS}"
        ))?;
        chests.push(chest);
    }
    for chest in &chests {
        tick_until(
            &mut stage,
            &format!("the eggs to reach the chest at {chest}"),
            || live_chest_eggs(&world, chest) == EGGS,
        )
        .await?;
    }

    stage.leave().await?;
    // No ticket holds the chunks now, so the chunk system unloads and saves them.
    for chest in &chests {
        let chunk = chest.chunk_position();
        let mut unloaded = false;
        for _ in 0..MAX_TICKS {
            server.tick();
            tokio::time::sleep(TICK_PAUSE).await;
            if !world.level.is_chunk_loaded(&chunk) {
                unloaded = true;
                break;
            }
        }
        if !unloaded {
            return Err(format!(
                "Chunk {chunk:?} stayed loaded {MAX_TICKS} ticks after {BOT_NAME} left"
            ));
        }
    }

    Ok(UnloadedChests {
        chests,
        region_folder: world.level.level_folder.region_folder.clone(),
        chunk_config: server.advanced_config.world.chunk.clone(),
        start,
    })
}

/// Once the server has stopped, which saves every world as `stop` does, reads each chest back from
/// the region files.
pub fn check_saved_chests(unloaded: &UnloadedChests) -> Result<String, String> {
    for chest in &unloaded.chests {
        let eggs = saved_chest_eggs(unloaded, chest)?;
        if eggs != i32::from(EGGS) {
            return Err(format!(
                "The chest at {chest} was saved with {eggs} of the {EGGS} eggs it held when its chunk unloaded"
            ));
        }
    }
    Ok(format!(
        "[PASS] bot:block_entities_saved_with_unloaded_chunk ({}ms)",
        unloaded.start.elapsed().as_millis()
    ))
}

/// Ticks until `done` holds.
async fn tick_until(
    stage: &mut StageContext<'_>,
    what: &str,
    done: impl Fn() -> bool,
) -> Result<(), String> {
    for _ in 0..MAX_TICKS {
        if done() {
            return Ok(());
        }
        stage.tick().await?;
    }
    if done() {
        Ok(())
    } else {
        Err(format!("Waited {MAX_TICKS} ticks for {what}"))
    }
}

fn live_chest_eggs(world: &World, chest: &BlockPos) -> u8 {
    world
        .get_block_entity(chest)
        .and_then(BlockEntity::get_inventory)
        .map_or(0, |inventory| inventory.count(&Item::EGG))
}

fn saved_chest_eggs(unloaded: &UnloadedChests, chest: &BlockPos) -> Result<i32, String> {
    let chunk = chest.chunk_position();
    let region = Vector2::new(chunk.x >> 5, chunk.y >> 5);
    let (path, format) = fs::read_dir(&unloaded.region_folder)
        .map_err(|err| format!("Can't list {}: {err}", unloaded.region_folder.display()))?
        .filter_map(Result::ok)
        .find_map(|entry| {
            let (file_region, format) = parse_region_file_name(entry.file_name().to_str()?)?;
            (file_region == region).then(|| (entry.path(), format))
        })
        .ok_or_else(|| format!("No region file holds chunk {chunk:?}"))?;
    let eggs = Cell::new(None);
    edit_region(
        &path,
        region,
        format,
        &unloaded.chunk_config,
        |nbt| {
            // NBT formats hand over a whole chunk, Pump's own format each block entity.
            let block_entities: Vec<&NbtCompound> = nbt.get_list("block_entities").map_or_else(
                || vec![&*nbt],
                |list| list.iter().filter_map(NbtTag::extract_compound).collect(),
            );
            for block_entity in block_entities {
                if [("x", chest.0.x), ("y", chest.0.y), ("z", chest.0.z)]
                    .iter()
                    .all(|(key, value)| block_entity.get_int(key) == Some(*value))
                {
                    eggs.set(Some(item_count(block_entity, &Item::EGG)));
                }
            }
            false
        },
        |_| Ok(()),
    )
    .map_err(|err| format!("Can't read {}: {err}", path.display()))?;
    eggs.get().ok_or_else(|| {
        format!(
            "The chest at {chest} was saved without its block entity in {}",
            path.display()
        )
    })
}

fn item_count(container: &NbtCompound, item: &Item) -> i32 {
    let id = format!("minecraft:{}", item.registry_key);
    container
        .get_list("Items")
        .unwrap_or_default()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .filter(|stack| stack.get_string("id") == Some(id.as_str()))
        .filter_map(|stack| stack.get_int("count"))
        .sum()
}
