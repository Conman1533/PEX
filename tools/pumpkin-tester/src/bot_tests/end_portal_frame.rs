//! Vanilla `EnderEyeItem.useOn` on a ring of end portal frames: the eye goes in, comparators
//! reading the frame see it at once, a player standing on the frame is lifted onto the eye
//! (`Block.pushEntitiesUp`), and the last eye opens the portal, breaking whatever stood inside
//! the ring with drops (`destroyBlock(pos, true)`).

use std::{sync::Arc, time::Instant};

use pumpkin_core::{
    block::entities::comparator::ComparatorBlockEntity,
    command::CommandSender,
    entity::{EntityBase, player::Player},
    server::Server,
    world::{BlockFlags, World},
};
use pumpkin_data::{
    Block, BlockStateId,
    block_properties::{ComparatorLikeProperties, EndPortalFrameLikeProperties, HorizontalFacing},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    GameMode, Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// High above the terrain, so nothing needs clearing.
const RING_Y: i32 = 200;
/// The end portal frame's collision box is 13 pixels tall without an eye and a full block with one.
const FRAME_TOP: f64 = 13.0 / 16.0;
/// Ticks for the comparator, which updates 2 ticks after its input changes.
const SETTLE_TICKS: usize = 10;

pub async fn run_end_portal_frame_test(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the end portal frame test")?;
    let world = player.world();
    let home = player.get_entity().pos.load();
    let feet = player.get_entity().block_pos.load().0;
    let center = BlockPos::new(feet.x + 8, RING_Y, feet.z);
    let original_mode = player.gamemode.load();
    let original_hand = player.inventory().held_item();
    player.set_gamemode(GameMode::Creative);

    let result = run(server, bot, &player, &world, center).await;

    let c = center.0;
    let _ = console(
        server,
        &format!(
            "fill {} {} {} {} {} {} air",
            c.x - 4,
            c.y - 1,
            c.z - 4,
            c.x + 4,
            c.y + 3,
            c.z + 4
        ),
    );
    player.set_gamemode(original_mode);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, original_hand);
    let back = teleport(server, bot, home).await;
    result.map_err(|err| format!("End portal frame: {err}"))?;
    back?;

    summary_lines.push(format!(
        "[PASS] bot:end_portal_frame ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// The twelve frames, three on each side two blocks out from `center`, each facing it.
fn ring(center: BlockPos) -> Vec<(BlockPos, HorizontalFacing)> {
    let c = center.0;
    let mut frames = Vec::new();
    for offset in -1..=1 {
        frames.push((
            BlockPos::new(c.x + offset, c.y, c.z - 2),
            HorizontalFacing::South,
        ));
        frames.push((
            BlockPos::new(c.x + offset, c.y, c.z + 2),
            HorizontalFacing::North,
        ));
        frames.push((
            BlockPos::new(c.x - 2, c.y, c.z + offset),
            HorizontalFacing::East,
        ));
        frames.push((
            BlockPos::new(c.x + 2, c.y, c.z + offset),
            HorizontalFacing::West,
        ));
    }
    frames
}

fn frame_state(facing: HorizontalFacing, eye: bool) -> BlockStateId {
    let mut props = EndPortalFrameLikeProperties::default(&Block::END_PORTAL_FRAME);
    props.facing = facing;
    props.eye = eye;
    props.to_state_id(&Block::END_PORTAL_FRAME)
}

fn has_eye(world: &World, pos: &BlockPos) -> bool {
    world.get_block(pos) == &Block::END_PORTAL_FRAME
        && EndPortalFrameLikeProperties::from_state_id(world.get_block_state_id(pos)).eye
}

/// Clears the space, lays the ring with ten eyes in, puts a comparator on the south middle
/// frame and dirt in the middle. Returns the frame the comparator reads, the frame the bot
/// fills last and the comparator.
fn build(
    server: &Arc<Server>,
    world: &Arc<World>,
    center: BlockPos,
) -> Result<(BlockPos, BlockPos, BlockPos), String> {
    let c = center.0;
    console(
        server,
        &format!(
            "fill {} {} {} {} {} {} air",
            c.x - 4,
            c.y - 1,
            c.z - 4,
            c.x + 4,
            c.y + 3,
            c.z + 4
        ),
    )?;
    // The bot fills the middle frame of the south side, which a comparator reads, and last the
    // middle frame of the west side, which it stands on. The other ten already hold eyes.
    let watched = BlockPos::new(c.x, c.y, c.z + 2);
    let last = BlockPos::new(c.x - 2, c.y, c.z);
    for (pos, facing) in ring(center) {
        let eye = pos != watched && pos != last;
        world.set_block_state(&pos, frame_state(facing, eye), BlockFlags::NOTIFY_ALL);
    }
    // A comparator reads the block its `facing` points at, and pops off without a block under it.
    let comparator = BlockPos::new(c.x, c.y, c.z + 3);
    world.set_block_state(
        &comparator.down(),
        Block::STONE.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    let mut comparator_props = ComparatorLikeProperties::default(&Block::COMPARATOR);
    comparator_props.facing = HorizontalFacing::North;
    world.set_block_state(
        &comparator,
        comparator_props.to_state_id(&Block::COMPARATOR),
        BlockFlags::NOTIFY_ALL,
    );
    if world.get_block_entity(&comparator).is_none() {
        world.add_block_entity(Arc::new(ComparatorBlockEntity::new(comparator)));
    }
    // Whatever stands inside the ring breaks with drops when the portal opens.
    world.set_block_state(
        &center,
        Block::DIRT.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    Ok((watched, last, comparator))
}

async fn run(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    world: &Arc<World>,
    center: BlockPos,
) -> Result<(), String> {
    let c = center.0;
    let (watched, last, comparator) = build(server, world, center)?;

    let on_frame = Vector3::new(
        f64::from(last.0.x) + 0.5,
        f64::from(last.0.y) + FRAME_TOP,
        f64::from(last.0.z) + 0.5,
    );
    teleport(server, bot, on_frame).await?;
    for _ in 0..SETTLE_TICKS {
        tick(server, bot).await?;
    }
    let before = comparator_output(world, &comparator)?;
    if before != 0 {
        return Err(format!("the comparator read {before} from an empty frame"));
    }

    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(2, &Item::ENDER_EYE));
    bot.use_block(watched, 1).await?;
    sync(server, bot).await?;
    for _ in 0..SETTLE_TICKS {
        tick(server, bot).await?;
    }
    if !has_eye(world, &watched) {
        return Err("the eye didn't go into the frame".to_string());
    }
    let after = comparator_output(world, &comparator)?;
    if after != 15 {
        return Err(format!(
            "the comparator read {after} from a frame holding an eye, expected 15"
        ));
    }

    let items_before = bot.spawned_items;
    bot.use_block(last, 2).await?;
    sync(server, bot).await?;
    if !has_eye(world, &last) {
        return Err("the last eye didn't go into the frame".to_string());
    }
    let lifted = f64::from(last.0.y) + 1.0;
    let server_y = player.get_entity().pos.load().y;
    if (bot.position.y - lifted).abs() > 1e-9 || (server_y - lifted).abs() > 1e-9 {
        return Err(format!(
            "the bot standing on the frame is at y {} (server y {server_y}), expected a teleport \
             up onto the eye at y {lifted}",
            bot.position.y
        ));
    }
    let mut missing = Vec::new();
    for dx in -1..=1 {
        for dz in -1..=1 {
            let pos = BlockPos::new(c.x + dx, c.y, c.z + dz);
            if world.get_block(&pos) != &Block::END_PORTAL {
                missing.push(pos);
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!("the last eye left no end_portal at {missing:?}"));
    }
    if bot.spawned_items == items_before {
        return Err("the dirt inside the ring broke without dropping an item".to_string());
    }
    Ok(())
}

fn comparator_output(world: &World, pos: &BlockPos) -> Result<i32, String> {
    let entity = world
        .get_block_entity(pos)
        .ok_or("the comparator has no block entity")?;
    let mut nbt = NbtCompound::new();
    entity.write_nbt(&mut nbt);
    nbt.get_int("OutputSignal")
        .ok_or_else(|| "the comparator has no OutputSignal".to_string())
}

/// `{:?}` keeps the decimal point, since `tp` centres a whole number on its block.
async fn teleport(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    pos: Vector3<f64>,
) -> Result<(), String> {
    console(
        server,
        &format!("tp {} {:?} {:?} {:?}", bot.name, pos.x, pos.y, pos.z),
    )?;
    for _ in 0..MAX_POLL_TICKS {
        tick(server, bot).await?;
        if bot.position == pos {
            return sync(server, bot).await;
        }
    }
    Err(format!("the bot never arrived at {pos:?}"))
}

async fn tick(server: &Arc<Server>, bot: &mut HeadlessBot) -> Result<(), String> {
    server.tick();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    bot.poll_packets().await
}

async fn sync(server: &Arc<Server>, bot: &mut HeadlessBot) -> Result<(), String> {
    let ping = bot.send_ping().await?;
    for _ in 0..MAX_POLL_TICKS {
        tick(server, bot).await?;
        if bot.has_pong(ping) {
            return Ok(());
        }
    }
    Err("the server never answered a ping".to_string())
}

fn console(server: &Arc<Server>, command: &str) -> Result<(), String> {
    server
        .command_dispatcher
        .load()
        .execute_input(command, &CommandSender::Console.into_source(server))
        .map(|_| ())
        .map_err(|err| format!("'{command}' failed: {}", err.message.get_text()))
}
