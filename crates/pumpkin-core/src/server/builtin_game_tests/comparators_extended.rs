use pumpkin_data::{
    Block,
    block_properties::{
        CakeLikeProperties, ComparatorLikeProperties, ComposterLikeProperties,
        CopperBulbLikeProperties, DispenserLikeProperties, EastRedstone, Facing,
        FurnaceLikeProperties, HopperLikeProperties, HorizontalFacing, ModeComparator,
        NorthRedstone, RedstoneWireLikeProperties, RepeaterLikeProperties,
        RespawnAnchorLikeProperties, SouthRedstone, WaterCauldronLikeProperties, WestRedstone,
    },
};
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::position::BlockPos;

use super::build_stone_floor;

async fn setup_four_comparators(helper: &GameTestHelper) -> GameTestResult<[BlockPos; 4]> {
    build_stone_floor(helper, 8, 5).await?;
    let mut wire_props = RedstoneWireLikeProperties::default(&Block::REDSTONE_WIRE);
    wire_props.north = NorthRedstone::Side;
    wire_props.south = SouthRedstone::Side;
    wire_props.east = EastRedstone::None;
    wire_props.west = WestRedstone::None;
    let wire_state = wire_props.to_state_id(&Block::REDSTONE_WIRE);

    let mut comp_props = ComparatorLikeProperties::default(&Block::COMPARATOR);
    comp_props.facing = HorizontalFacing::North;
    comp_props.mode = ModeComparator::Compare;
    let comp_state = comp_props.to_state_id(&Block::COMPARATOR);

    let cols = [1, 3, 5, 7];
    let mut wires = [BlockPos::new(0, 0, 0); 4];
    for (i, &x) in cols.iter().enumerate() {
        let comp = BlockPos::new(x, 1, 2);
        let wire = BlockPos::new(x, 1, 3);
        helper.set_block(&wire, wire_state).await?;
        helper.set_block(&comp, comp_state).await?;
        wires[i] = wire;
    }
    Ok(wires)
}

async fn read_wire_power(helper: &GameTestHelper, pos: &BlockPos) -> u8 {
    RedstoneWireLikeProperties::from_state_id(helper.block_state_id(pos).await).power
}

pub(super) async fn test_comparator_composter_levels_1_to_4(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    for (i, (&x, level)) in [1, 3, 5, 7].iter().zip([1u8, 2, 3, 4]).enumerate() {
        let mut props = ComposterLikeProperties::default(&Block::COMPOSTER);
        props.level = level;
        helper
            .set_block(
                &BlockPos::new(x, 1, 1),
                props.to_state_id(&Block::COMPOSTER),
            )
            .await?;
        let _ = i;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [1u8, 2, 3, 4].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected composter level {expected} power {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_composter_levels_5_to_8(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    for (&x, level) in [1, 3, 5, 7].iter().zip([5u8, 6, 7, 8]) {
        let mut props = ComposterLikeProperties::default(&Block::COMPOSTER);
        props.level = level;
        helper
            .set_block(
                &BlockPos::new(x, 1, 1),
                props.to_state_id(&Block::COMPOSTER),
            )
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [5u8, 6, 7, 8].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected composter level {expected} power {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_water_cauldron_levels_1_2_3(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    helper
        .set_block_default(&BlockPos::new(1, 1, 1), &Block::CAULDRON)
        .await?;
    for (&x, level) in [3, 5, 7].iter().zip([1u8, 2, 3]) {
        let mut props = WaterCauldronLikeProperties::default(&Block::WATER_CAULDRON);
        props.level = level;
        helper
            .set_block(
                &BlockPos::new(x, 1, 1),
                props.to_state_id(&Block::WATER_CAULDRON),
            )
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [0u8, 1, 2, 3].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected water cauldron comparator power {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_powder_snow_cauldron_levels_1_2_3(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    helper
        .set_block_default(&BlockPos::new(1, 1, 1), &Block::CAULDRON)
        .await?;
    for (&x, level) in [3, 5, 7].iter().zip([1u8, 2, 3]) {
        let mut props = WaterCauldronLikeProperties::default(&Block::POWDER_SNOW_CAULDRON);
        props.level = level;
        helper
            .set_block(
                &BlockPos::new(x, 1, 1),
                props.to_state_id(&Block::POWDER_SNOW_CAULDRON),
            )
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [0u8, 1, 2, 3].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!(
                        "Expected powder snow cauldron comparator power {expected}, got {actual}"
                    ),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_respawn_anchor_charges_0_1_3_4(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    for (&x, charges) in [1, 3, 5, 7].iter().zip([0u8, 1, 3, 4]) {
        let mut props = RespawnAnchorLikeProperties::default(&Block::RESPAWN_ANCHOR);
        props.charges = charges;
        helper
            .set_block(
                &BlockPos::new(x, 1, 1),
                props.to_state_id(&Block::RESPAWN_ANCHOR),
            )
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [0u8, 3, 11, 15].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected respawn anchor comparator power {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_cake_bites_0_2_4_6(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    for (&x, bites) in [1, 3, 5, 7].iter().zip([0u8, 2, 4, 6]) {
        let mut props = CakeLikeProperties::default(&Block::CAKE);
        props.bites = bites;
        helper
            .set_block(&BlockPos::new(x, 1, 1), props.to_state_id(&Block::CAKE))
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [14u8, 10, 6, 2].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected cake comparator power {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_candle_cake_full_signal(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    for (&x, block) in [1, 3, 5, 7].iter().zip([
        &Block::CANDLE_CAKE,
        &Block::WHITE_CANDLE_CAKE,
        &Block::RED_CANDLE_CAKE,
        &Block::BLUE_CANDLE_CAKE,
    ]) {
        helper
            .set_block_default(&BlockPos::new(x, 1, 1), block)
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for wire in &wires {
                let actual = read_wire_power(&helper, wire).await;
                helper.assert_true(
                    actual == 14,
                    format!("Expected candle cake comparator power 14, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_chest_single_item_signal_1(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let chest = BlockPos::new(1, 1, 1);
    helper.set_block_default(&chest, &Block::CHEST).await?;
    helper
        .set_container_item(&chest, 0, "minecraft:cobblestone", 1)
        .await?;
    // Trigger neighbor update on comparator after filling chest
    helper
        .set_block_default(&BlockPos::new(1, 2, 2), &Block::STONE)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let actual = read_wire_power(&helper, &wires[0]).await;
            helper.assert_true(
                actual == 1,
                format!("Expected chest with 1 item to output comparator power 1, got {actual}"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_chest_nine_swords_signal_5(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let chest = BlockPos::new(1, 1, 1);
    helper.set_block_default(&chest, &Block::CHEST).await?;
    for slot in 0..9 {
        helper
            .set_container_item(&chest, slot, "minecraft:diamond_sword", 1)
            .await?;
    }
    helper
        .set_block_default(&BlockPos::new(1, 2, 2), &Block::STONE)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let actual = read_wire_power(&helper, &wires[0]).await;
            helper.assert_true(
                actual == 5,
                format!("Expected chest 9/27 full to output comparator power 5, got {actual}"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_hopper_fill_levels_1_and_5(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let h1 = BlockPos::new(1, 1, 1);
    let h2 = BlockPos::new(3, 1, 1);
    let mut h_props = HopperLikeProperties::default(&Block::HOPPER);
    h_props.enabled = false;
    let h_state = h_props.to_state_id(&Block::HOPPER);
    helper.set_block(&h1, h_state).await?;
    helper.set_block(&h2, h_state).await?;

    helper
        .set_container_item(&h1, 0, "minecraft:diamond_sword", 1)
        .await?;
    for slot in 0..5 {
        helper
            .set_container_item(&h2, slot, "minecraft:diamond_sword", 1)
            .await?;
    }
    helper
        .set_block_default(&BlockPos::new(1, 2, 2), &Block::STONE)
        .await?;
    helper
        .set_block_default(&BlockPos::new(3, 2, 2), &Block::STONE)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let p1 = read_wire_power(&helper, &wires[0]).await;
            let p2 = read_wire_power(&helper, &wires[1]).await;
            helper.assert_true(
                p1 == 3 && p2 == 15,
                format!("Expected hopper 1/5=3 and 5/5=15, got {p1} and {p2}"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_dispenser_and_dropper_three_swords(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let disp = BlockPos::new(1, 1, 1);
    let drop = BlockPos::new(3, 1, 1);
    let mut d_props = DispenserLikeProperties::default(&Block::DISPENSER);
    d_props.facing = Facing::Up;
    helper
        .set_block(&disp, d_props.to_state_id(&Block::DISPENSER))
        .await?;
    helper
        .set_block(&drop, d_props.to_state_id(&Block::DROPPER))
        .await?;

    for slot in 0..3 {
        helper
            .set_container_item(&disp, slot, "minecraft:diamond_sword", 1)
            .await?;
        helper
            .set_container_item(&drop, slot, "minecraft:diamond_sword", 1)
            .await?;
    }
    helper
        .set_block_default(&BlockPos::new(1, 2, 2), &Block::STONE)
        .await?;
    helper
        .set_block_default(&BlockPos::new(3, 2, 2), &Block::STONE)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let p1 = read_wire_power(&helper, &wires[0]).await;
            let p2 = read_wire_power(&helper, &wires[1]).await;
            helper.assert_true(
                p1 == 5 && p2 == 5,
                format!("Expected 3/9 full dispenser & dropper to output 5, got {p1} and {p2}"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_furnace_slot_signal(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let furnace = BlockPos::new(1, 1, 1);
    let props = FurnaceLikeProperties::default(&Block::FURNACE);
    helper
        .set_block(&furnace, props.to_state_id(&Block::FURNACE))
        .await?;
    helper
        .set_container_item(&furnace, 0, "minecraft:diamond_sword", 1)
        .await?;
    helper
        .set_block_default(&BlockPos::new(1, 2, 2), &Block::STONE)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let actual = read_wire_power(&helper, &wires[0]).await;
            helper.assert_true(
                actual == 5,
                format!("Expected 1/3 full furnace to output comparator power 5, got {actual}"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_copper_bulbs_oxidation_variants(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    let wires = setup_four_comparators(&helper).await?;
    let bulbs = [
        (&Block::EXPOSED_COPPER_BULB, true),
        (&Block::WEATHERED_COPPER_BULB, false),
        (&Block::OXIDIZED_COPPER_BULB, true),
        (&Block::WAXED_COPPER_BULB, true),
    ];
    for (&x, (block, lit)) in [1, 3, 5, 7].iter().zip(bulbs) {
        let mut props = CopperBulbLikeProperties::default(block);
        props.lit = lit;
        helper
            .set_block(&BlockPos::new(x, 1, 1), props.to_state_id(block))
            .await?;
    }

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            for (i, expected) in [15u8, 0, 15, 15].into_iter().enumerate() {
                let actual = read_wire_power(&helper, &wires[i]).await;
                helper.assert_true(
                    actual == expected,
                    format!("Expected bulb variant {i} comparator output {expected}, got {actual}"),
                )?;
            }
            Ok(())
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_comparator_subtract_mode_clamps_to_zero(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let mut wire_props = RedstoneWireLikeProperties::default(&Block::REDSTONE_WIRE);
    wire_props.north = NorthRedstone::Side;
    wire_props.south = SouthRedstone::Side;
    let out_wire = BlockPos::new(2, 1, 3);
    helper
        .set_block(&out_wire, wire_props.to_state_id(&Block::REDSTONE_WIRE))
        .await?;

    let mut comp = ComparatorLikeProperties::default(&Block::COMPARATOR);
    comp.facing = HorizontalFacing::North;
    comp.mode = ModeComparator::Subtract;
    helper
        .set_block(
            &BlockPos::new(2, 1, 2),
            comp.to_state_id(&Block::COMPARATOR),
        )
        .await?;

    // Side input = 15 (REDSTONE_BLOCK at (1,1,2)), rear input = 6 (composter level 6 at (2,1,1)) -> max(6 - 15, 0) = 0
    helper
        .set_block_default(&BlockPos::new(1, 1, 2), &Block::REDSTONE_BLOCK)
        .await?;
    let mut composter = ComposterLikeProperties::default(&Block::COMPOSTER);
    composter.level = 6;
    helper
        .set_block(
            &BlockPos::new(2, 1, 1),
            composter.to_state_id(&Block::COMPOSTER),
        )
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            let p = read_wire_power(&helper, &out_wire).await;
            helper.assert_true(
                p == 0,
                format!("Expected subtract comparator with side>=rear to clamp to 0, got {p}"),
            )
        })
        .then_succeed();
    Ok(())
}

/// A dropper fires into a chest in the same game tick as a comparator chain powers repeater R,
/// and the chest's comparator K locks R from the side. The dropper's tick comes first, so
/// vanilla `BlockEntity.setChanged` schedules K's tick before R's (both `HIGH`, two ticks out);
/// K turns on first and R stays locked off. When the chest's change only reaches K after the
/// block entities tick, R's tick runs first and R turns on.
pub(super) async fn test_comparator_container_race(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 8, 6).await?;
    let dropper = BlockPos::new(1, 1, 3);
    let chest = BlockPos::new(2, 1, 3);
    let comparator_k = BlockPos::new(3, 1, 3);
    let repeater_r = BlockPos::new(4, 1, 3);

    let comparator = |facing| {
        let mut props = ComparatorLikeProperties::default(&Block::COMPARATOR);
        props.facing = facing;
        props.mode = ModeComparator::Compare;
        props.to_state_id(&Block::COMPARATOR)
    };
    let wire = |north, east, south| {
        let mut props = RedstoneWireLikeProperties::default(&Block::REDSTONE_WIRE);
        props.north = north;
        props.east = east;
        props.south = south;
        props.west = WestRedstone::None;
        props.to_state_id(&Block::REDSTONE_WIRE)
    };

    let mut dropper_props = DispenserLikeProperties::default(&Block::DROPPER);
    dropper_props.facing = Facing::East;
    helper
        .set_block(&dropper, dropper_props.to_state_id(&Block::DROPPER))
        .await?;
    helper.set_block_default(&chest, &Block::CHEST).await?;
    helper
        .set_container_item(&dropper, 0, "minecraft:cobblestone", 1)
        .await?;
    // K reads the chest and outputs east into R's side.
    helper
        .set_block(&comparator_k, comparator(HorizontalFacing::West))
        .await?;
    let mut repeater_props = RepeaterLikeProperties::default(&Block::REPEATER);
    repeater_props.facing = HorizontalFacing::North;
    repeater_props.delay = 1;
    helper
        .set_block(&repeater_r, repeater_props.to_state_id(&Block::REPEATER))
        .await?;
    // R's input: C0 (reading the redstone block placed later) -> dust -> C1 -> dust -> R, two
    // comparator ticks so C1 fires in the dropper's game tick, after it. Dust in front keeps
    // both comparators at `NORMAL` priority.
    helper
        .set_block(
            &BlockPos::new(4, 1, 2),
            wire(NorthRedstone::Side, EastRedstone::None, SouthRedstone::Side),
        )
        .await?;
    helper
        .set_block(&BlockPos::new(4, 1, 1), comparator(HorizontalFacing::North))
        .await?;
    helper
        .set_block(
            &BlockPos::new(4, 1, 0),
            wire(NorthRedstone::None, EastRedstone::Side, SouthRedstone::Side),
        )
        .await?;
    helper
        .set_block(&BlockPos::new(5, 1, 0), comparator(HorizontalFacing::East))
        .await?;

    helper
        .start_sequence()
        .then_idle(5)
        .then_execute(|helper| async move {
            // The dropper's tick is scheduled first, then C0's.
            helper
                .set_block_default(&BlockPos::new(1, 1, 4), &Block::REDSTONE_BLOCK)
                .await?;
            helper
                .set_block_default(&BlockPos::new(6, 1, 0), &Block::REDSTONE_BLOCK)
                .await?;
            Ok(())
        })
        .then_wait_until(move |helper| async move {
            let k =
                ComparatorLikeProperties::from_state_id(helper.block_state_id(&comparator_k).await);
            helper.assert_true(k.powered, "the chest's comparator should turn on")
        })
        .then_execute(move |helper| async move {
            let r = RepeaterLikeProperties::from_state_id(helper.block_state_id(&repeater_r).await);
            helper.assert_true(
                !r.powered,
                "the chest's comparator should lock the repeater before the repeater's tick",
            )
        })
        .then_idle(4)
        .then_execute(move |helper| async move {
            let r = RepeaterLikeProperties::from_state_id(helper.block_state_id(&repeater_r).await);
            helper.assert_true(
                !r.powered && r.locked,
                "the repeater should stay locked off",
            )
        })
        .then_succeed();
    Ok(())
}
