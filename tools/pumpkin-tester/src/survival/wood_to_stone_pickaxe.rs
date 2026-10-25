//! Stage 1: punch a tree, craft planks, sticks and a crafting table in the inventory grid, place
//! the table, craft a wooden pickaxe, mine stone with it and craft a stone pickaxe.

use std::sync::PoisonError;

use pumpkin_data::{Block, item::Item, screen::WindowType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::StageContext;
use crate::bot::{crafting_menu, inventory_menu};

/// Where the bot stands. High above the terrain so the stage doesn't depend on the world seed.
const ORIGIN_X: i32 = -184;
const ORIGIN_Y: i32 = 200;
const ORIGIN_Z: i32 = -184;
/// The tree trunk and the stone pillar are this many blocks east and west of the bot.
const PILLAR_OFFSET: i32 = 2;
const PILLAR_HEIGHT: i32 = 3;
/// Half the width of the stone floor and of the cleared space above it.
const AREA_RADIUS: i32 = 4;
const AREA_HEIGHT: i32 = 4;

/// One of each of the four slots in the inventory's 2x2 grid makes a crafting table.
const TABLE_PATTERN: [usize; 4] = [
    inventory_menu::CRAFT_SLOT_START,
    inventory_menu::CRAFT_SLOT_START + 1,
    inventory_menu::CRAFT_SLOT_START + 2,
    inventory_menu::CRAFT_SLOT_START + 3,
];
/// Two planks in the left column of the 2x2 grid make sticks.
const STICK_PATTERN: [usize; 2] = [
    inventory_menu::CRAFT_SLOT_START,
    inventory_menu::CRAFT_SLOT_START + 2,
];
/// The top row of the 3x3 grid holds a pickaxe's head.
const PICKAXE_HEAD: [usize; 3] = [
    crafting_menu::CRAFT_SLOT_START,
    crafting_menu::CRAFT_SLOT_START + 1,
    crafting_menu::CRAFT_SLOT_START + 2,
];
/// The middle column below the head holds the handle.
const PICKAXE_HANDLE: [usize; 2] = [
    crafting_menu::CRAFT_SLOT_START + 4,
    crafting_menu::CRAFT_SLOT_START + 7,
];

const fn origin() -> BlockPos {
    BlockPos::new(ORIGIN_X, ORIGIN_Y, ORIGIN_Z)
}

const fn origin_offset(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(ORIGIN_X + x, ORIGIN_Y + y, ORIGIN_Z + z)
}

/// Standing position on top of the block column at the given offset from the origin.
fn stand_at(x: i32, z: i32) -> Vector3<f64> {
    Vector3::new(
        f64::from(ORIGIN_X + x) + 0.5,
        f64::from(ORIGIN_Y),
        f64::from(ORIGIN_Z + z) + 0.5,
    )
}

fn fill(from: BlockPos, to: BlockPos, block: &Block) -> String {
    format!(
        "fill {} {} {} {} {} {} minecraft:{}",
        from.0.x, from.0.y, from.0.z, to.0.x, to.0.y, to.0.z, block.name
    )
}

pub async fn run(stage: &mut StageContext<'_>) -> Result<(), String> {
    set_up(stage).await?;

    // Punch the tree and pick up the logs.
    for y in 0..PILLAR_HEIGHT {
        stage
            .dig(origin_offset(PILLAR_OFFSET, y, 0), &Block::OAK_LOG)
            .await?;
    }
    stage.collect(&Item::OAK_LOG, 3).await?;

    craft_in_inventory(stage).await?;

    // Place the crafting table on the floor and make a wooden pickaxe on it.
    let table = origin_offset(1, 0, 2);
    let table_hotbar_slot = hotbar_slot_of(stage, &Item::CRAFTING_TABLE)?;
    stage.select_hotbar_slot(table_hotbar_slot).await?;
    stage.use_block(table.down()).await?;
    let placed = stage.world().get_block(&table);
    if placed.id != Block::CRAFTING_TABLE.id {
        return Err(format!(
            "Using the crafting table on the floor left {} at {table}",
            placed.name
        ));
    }
    stage.open_menu(table, WindowType::Crafting as i32).await?;
    place_one_each(stage, &Item::OAK_PLANKS, &PICKAXE_HEAD).await?;
    place_one_each(stage, &Item::STICK, &PICKAXE_HANDLE).await?;
    expect_result(stage, crafting_menu::RESULT_SLOT, &Item::WOODEN_PICKAXE, 1)?;
    stage.shift_click(crafting_menu::RESULT_SLOT).await?;
    stage.close_menu().await?;

    // Mine the stone pillar with the wooden pickaxe and pick up the cobblestone.
    let pickaxe_hotbar_slot = hotbar_slot_of(stage, &Item::WOODEN_PICKAXE)?;
    stage.select_hotbar_slot(pickaxe_hotbar_slot).await?;
    stage.walk_to(stand_at(1 - PILLAR_OFFSET, 0)).await?;
    for y in 0..PILLAR_HEIGHT {
        stage
            .dig(origin_offset(-PILLAR_OFFSET, y, 0), &Block::STONE)
            .await?;
    }
    stage.collect(&Item::COBBLESTONE, 3).await?;

    // Back at the table for the stone pickaxe.
    stage.open_menu(table, WindowType::Crafting as i32).await?;
    place_one_each(stage, &Item::COBBLESTONE, &PICKAXE_HEAD).await?;
    place_one_each(stage, &Item::STICK, &PICKAXE_HANDLE).await?;
    expect_result(stage, crafting_menu::RESULT_SLOT, &Item::STONE_PICKAXE, 1)?;
    stage.shift_click(crafting_menu::RESULT_SLOT).await?;
    stage.close_menu().await?;

    check_outcome(stage)
}

/// Builds a stone floor with an oak trunk to its east and a stone pillar to its west, and puts
/// the bot between them in survival.
async fn set_up(stage: &mut StageContext<'_>) -> Result<(), String> {
    let name = stage.bot.name.clone();
    stage.console(&format!("gamemode survival {name}"))?;
    let start = stand_at(0, 0);
    stage.console(&format!("tp {name} {} {} {}", start.x, start.y, start.z))?;
    stage.wait_for_terrain(origin()).await?;
    stage
        .tick_until("the teleport to arrive", super::MAX_SYNC_TICKS, |bot| {
            bot.position == start
        })
        .await?;

    let low_corner = origin_offset(-AREA_RADIUS, -1, -AREA_RADIUS);
    let high_corner = origin_offset(AREA_RADIUS, AREA_HEIGHT, AREA_RADIUS);
    // `fill` fails when it changes nothing, so fill the whole space with stone before clearing it.
    for command in [
        fill(low_corner, high_corner, &Block::STONE),
        fill(low_corner.up(), high_corner, &Block::AIR),
        fill(
            origin_offset(PILLAR_OFFSET, 0, 0),
            origin_offset(PILLAR_OFFSET, PILLAR_HEIGHT - 1, 0),
            &Block::OAK_LOG,
        ),
        fill(
            origin_offset(-PILLAR_OFFSET, 0, 0),
            origin_offset(-PILLAR_OFFSET, PILLAR_HEIGHT - 1, 0),
            &Block::STONE,
        ),
    ] {
        stage.console(&command)?;
    }

    stage.bot.move_to(start, true).await?;
    stage.sync().await?;
    if stage
        .bot
        .inventory
        .items
        .iter()
        .any(|stack| !stack.is_empty())
    {
        return Err("The bot doesn't start with an empty inventory".to_string());
    }
    Ok(())
}

/// Planks, a crafting table and sticks in the player's own 2x2 grid.
async fn craft_in_inventory(stage: &mut StageContext<'_>) -> Result<(), String> {
    // All three logs go into one grid slot; shift-clicking the result crafts until they run out.
    let logs = find(stage, &Item::OAK_LOG)?;
    stage.left_click(logs).await?;
    stage.left_click(inventory_menu::CRAFT_SLOT_START).await?;
    expect_result(stage, inventory_menu::RESULT_SLOT, &Item::OAK_PLANKS, 4)?;
    stage.shift_click(inventory_menu::RESULT_SLOT).await?;
    expect_count(stage, &Item::OAK_PLANKS, 12)?;

    place_one_each(stage, &Item::OAK_PLANKS, &TABLE_PATTERN).await?;
    expect_result(stage, inventory_menu::RESULT_SLOT, &Item::CRAFTING_TABLE, 1)?;
    // Take the table on the cursor and put it in the hotbar by hand.
    stage.left_click(inventory_menu::RESULT_SLOT).await?;
    let free_hotbar_slot = stage
        .bot
        .inventory
        .free_hotbar_slot()
        .ok_or("The hotbar has no room for the crafting table")?;
    let target = stage.bot.inventory.hotbar_slot(free_hotbar_slot);
    stage.left_click(target).await?;
    expect_count(stage, &Item::CRAFTING_TABLE, 1)?;

    place_one_each(stage, &Item::OAK_PLANKS, &STICK_PATTERN).await?;
    expect_result(stage, inventory_menu::RESULT_SLOT, &Item::STICK, 4)?;
    stage.shift_click(inventory_menu::RESULT_SLOT).await?;
    expect_count(stage, &Item::STICK, 4)?;
    expect_count(stage, &Item::OAK_PLANKS, 6)?;

    stage.close_menu().await
}

/// Picks up the stack of `item`, right-clicks one onto each grid slot and puts the rest back.
async fn place_one_each(
    stage: &mut StageContext<'_>,
    item: &'static Item,
    grid_slots: &[usize],
) -> Result<(), String> {
    let from = find(stage, item)?;
    stage.left_click(from).await?;
    for &slot in grid_slots {
        stage.right_click(slot).await?;
    }
    stage.left_click(from).await?;
    let carried = &stage.bot.inventory.carried;
    if carried.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} {} stayed on the cursor after filling the grid",
            carried.item_count, carried.item.registry_key
        ))
    }
}

fn find(stage: &StageContext<'_>, item: &Item) -> Result<usize, String> {
    stage
        .bot
        .inventory
        .find(item)
        .ok_or_else(|| format!("The bot has no {}", item.registry_key))
}

fn hotbar_slot_of(stage: &StageContext<'_>, item: &Item) -> Result<usize, String> {
    stage
        .bot
        .inventory
        .find_in_hotbar(item)
        .ok_or_else(|| format!("The bot's hotbar has no {}", item.registry_key))
}

/// Checks the crafting result the bot sees.
fn expect_result(
    stage: &StageContext<'_>,
    slot: usize,
    item: &Item,
    count: u8,
) -> Result<(), String> {
    let result = stage.bot.inventory.slot(slot);
    if result.item == item && result.item_count == count {
        Ok(())
    } else {
        Err(format!(
            "The crafting result shows {} {} instead of {count} {}",
            result.item_count, result.item.registry_key, item.registry_key
        ))
    }
}

/// Checks how many of `item` the bot sees in its inventory.
fn expect_count(stage: &StageContext<'_>, item: &Item, count: u32) -> Result<(), String> {
    let held = stage.bot.inventory.count(item);
    if held == count {
        Ok(())
    } else {
        Err(format!(
            "The bot has {held} {} instead of {count}",
            item.registry_key
        ))
    }
}

/// The stage ends with a used wooden pickaxe, a new stone pickaxe and the three planks left
/// over, on the server and in the bot's view of its inventory.
fn check_outcome(stage: &StageContext<'_>) -> Result<(), String> {
    // Each stone block costs the wooden pickaxe one point of durability (vanilla `Item.mineBlock`).
    let expected: [(&Item, u8, i32); 3] = [
        (&Item::WOODEN_PICKAXE, 1, PILLAR_HEIGHT),
        (&Item::STONE_PICKAXE, 1, 0),
        (&Item::OAK_PLANKS, 3, 0),
    ];
    let inventory = stage.player.inventory();
    let mut server_stacks: Vec<_> = inventory
        .main_inventory
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|stack| !stack.is_empty())
        .map(|stack| (stack.item, stack.item_count, stack.get_damage()))
        .collect();
    for (item, count, damage) in expected {
        let Some(index) = server_stacks
            .iter()
            .position(|stack| *stack == (item, count, damage))
        else {
            return Err(format!(
                "The server inventory has no stack of {count} {} with damage {damage}: {}",
                item.registry_key,
                describe(&server_stacks)
            ));
        };
        server_stacks.swap_remove(index);
        let seen = stage.bot.inventory.count(item);
        if seen != u32::from(count) {
            return Err(format!(
                "The bot sees {seen} {} but the server has {count}",
                item.registry_key
            ));
        }
    }
    if !server_stacks.is_empty() {
        return Err(format!(
            "The server inventory has extra stacks: {}",
            describe(&server_stacks)
        ));
    }
    Ok(())
}

fn describe(stacks: &[(&Item, u8, i32)]) -> String {
    stacks
        .iter()
        .map(|(item, count, damage)| format!("{count} {} (damage {damage})", item.registry_key))
        .collect::<Vec<_>>()
        .join(", ")
}
