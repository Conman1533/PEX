use pumpkin_data::{
    Block,
    block_properties::{
        AttachFace, DetectorRailProperties, FurnaceLikeProperties, HorizontalFacing,
        LeverLikeProperties, PoweredRailLikeProperties, RailLikeProperties, RailShape,
        RailShapeStraight, RedstoneWireLikeProperties,
    },
};
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

pub(super) async fn test_powered_rail_propagation(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 3, 10).await?;

    let mut unpowered_props = PoweredRailLikeProperties::default(&Block::POWERED_RAIL);
    unpowered_props.shape = RailShapeStraight::NorthSouth;
    unpowered_props.powered = false;
    let unpowered_rail = unpowered_props.to_state_id(&Block::POWERED_RAIL);

    let mut powered_props = unpowered_props;
    powered_props.powered = true;
    let powered_rail = powered_props.to_state_id(&Block::POWERED_RAIL);

    for z in 1..=8 {
        helper
            .set_block(&BlockPos::new(1, 1, z), unpowered_rail)
            .await?;
    }

    let power_source = BlockPos::new(1, 1, 0);
    helper
        .set_block_default(&power_source, &Block::REDSTONE_BLOCK)
        .await?;

    for z in 1..=8 {
        helper
            .assert_block_state(&BlockPos::new(1, 1, z), powered_rail)
            .await?;
    }

    helper.set_block_default(&power_source, &Block::AIR).await?;

    for z in 1..=8 {
        helper
            .assert_block_state(&BlockPos::new(1, 1, z), unpowered_rail)
            .await?;
    }

    helper.succeed();
    Ok(())
}

pub(super) async fn test_activator_rail_propagation(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 3, 10).await?;

    let mut unpowered_props = PoweredRailLikeProperties::default(&Block::ACTIVATOR_RAIL);
    unpowered_props.shape = RailShapeStraight::NorthSouth;
    unpowered_props.powered = false;
    let unpowered_rail = unpowered_props.to_state_id(&Block::ACTIVATOR_RAIL);

    let mut powered_props = unpowered_props;
    powered_props.powered = true;
    let powered_rail = powered_props.to_state_id(&Block::ACTIVATOR_RAIL);

    for z in 1..=8 {
        helper
            .set_block(&BlockPos::new(1, 1, z), unpowered_rail)
            .await?;
    }

    let power_source = BlockPos::new(1, 1, 0);
    helper
        .set_block_default(&power_source, &Block::REDSTONE_BLOCK)
        .await?;

    for z in 1..=8 {
        helper
            .assert_block_state(&BlockPos::new(1, 1, z), powered_rail)
            .await?;
    }

    helper.set_block_default(&power_source, &Block::AIR).await?;

    for z in 1..=8 {
        helper
            .assert_block_state(&BlockPos::new(1, 1, z), unpowered_rail)
            .await?;
    }

    helper.succeed();
    Ok(())
}

pub(super) async fn test_detector_rail_minecart(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;

    let rail_pos = BlockPos::new(2, 1, 2);
    let wire_pos = BlockPos::new(3, 1, 2);

    let mut rail_props = DetectorRailProperties::default(&Block::DETECTOR_RAIL);
    rail_props.shape = RailShapeStraight::NorthSouth;
    rail_props.powered = false;
    helper
        .set_block(&rail_pos, rail_props.to_state_id(&Block::DETECTOR_RAIL))
        .await?;
    helper
        .set_block_default(&wire_pos, &Block::REDSTONE_WIRE)
        .await?;

    helper
        .spawn_entity_at("minecraft:minecart", Vector3::new(2.5, 1.1, 2.5))
        .await?;

    helper.succeed_when(move |helper| async move {
        let state = DetectorRailProperties::from_state_id(helper.block_state_id(&rail_pos).await);
        let wire =
            RedstoneWireLikeProperties::from_state_id(helper.block_state_id(&wire_pos).await);
        helper.assert_true(
            state.powered && wire.power == 15,
            format!(
                "Expected detector rail powered=true and wire power=15, got powered={} power={}",
                state.powered, wire.power
            ),
        )
    })?;
    Ok(())
}

/// Roadmap G5-12: vanilla `RailBlock.updateState`. A T-junction next to a lit torch curves
/// north (`NORTH_EAST`); when the torch goes out its update, with the torch as the source, turns
/// the junction south (`SOUTH_EAST`).
pub(super) async fn test_rail_junction_switches(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let junction = BlockPos::new(2, 1, 2);
    let rail = |shape| {
        let mut props = RailLikeProperties::default(&Block::RAIL);
        props.shape = shape;
        props.to_state_id(&Block::RAIL)
    };
    helper
        .set_block(&BlockPos::new(2, 1, 1), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(2, 1, 3), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(3, 1, 2), rail(RailShape::EastWest))
        .await?;
    helper
        .set_block(&junction, rail(RailShape::NorthEast))
        .await?;

    // A torch on the west side of the junction, on a wall a lever on top can power.
    helper
        .set_block_default(&BlockPos::new(0, 1, 2), &Block::STONE)
        .await?;
    let mut torch = FurnaceLikeProperties::default(&Block::REDSTONE_WALL_TORCH);
    torch.facing = HorizontalFacing::East;
    torch.lit = true;
    helper
        .set_block(
            &BlockPos::new(1, 1, 2),
            torch.to_state_id(&Block::REDSTONE_WALL_TORCH),
        )
        .await?;
    let lever_pos = BlockPos::new(0, 2, 2);
    let mut lever = LeverLikeProperties::default(&Block::LEVER);
    lever.face = AttachFace::Floor;
    lever.facing = HorizontalFacing::North;
    lever.powered = false;
    helper
        .set_block(&lever_pos, lever.to_state_id(&Block::LEVER))
        .await?;
    helper
        .assert_block_state(&junction, rail(RailShape::NorthEast))
        .await?;

    helper.pull_lever(&lever_pos).await?;
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_block_state(&junction, rail(RailShape::SouthEast))
                .await
        })
        .then_succeed();
    Ok(())
}

/// A T-junction on row `z` whose west side is a stone block, with a switch on that block's west
/// face. Returns the junction.
async fn junction_behind_wall(
    helper: &GameTestHelper,
    z: i32,
    switch: &Block,
) -> GameTestResult<BlockPos> {
    let rail = |shape| {
        let mut props = RailLikeProperties::default(&Block::RAIL);
        props.shape = shape;
        props.to_state_id(&Block::RAIL)
    };
    let junction = BlockPos::new(3, 1, z);
    helper
        .set_block(&BlockPos::new(3, 1, z - 1), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(3, 1, z + 1), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(4, 1, z), rail(RailShape::EastWest))
        .await?;
    helper
        .set_block(&junction, rail(RailShape::SouthEast))
        .await?;
    helper
        .set_block_default(&BlockPos::new(2, 1, z), &Block::STONE)
        .await?;
    // Lever and button share their properties' shape.
    let mut props = LeverLikeProperties::default(switch);
    props.face = AttachFace::Wall;
    props.facing = HorizontalFacing::West;
    props.powered = false;
    helper
        .set_block(&BlockPos::new(1, 1, z), props.to_state_id(switch))
        .await?;
    Ok(junction)
}

/// Roadmap 2.0 emitter update source: a lever and a button update the neighbours of the block
/// they hang on as themselves, so the T-junction beside that block, which only hears that second
/// update, sees a signal source and curves towards the power (`NORTH_EAST`).
pub(super) async fn test_lever_and_button_switch_rail_junctions(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 8, 8).await?;
    let powered = {
        let mut props = RailLikeProperties::default(&Block::RAIL);
        props.shape = RailShape::NorthEast;
        props.to_state_id(&Block::RAIL)
    };
    let by_lever = junction_behind_wall(&helper, 1, &Block::LEVER).await?;
    let by_button = junction_behind_wall(&helper, 5, &Block::STONE_BUTTON).await?;

    helper.pull_lever(&BlockPos::new(1, 1, 1)).await?;
    helper.press_button(&BlockPos::new(1, 1, 5)).await?;
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper.assert_block_state(&by_lever, powered).await?;
            helper.assert_block_state(&by_button, powered).await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `ServerLevel.updateNeighboursOnBlockSet`: `/setblock` and `/fill` update the
/// neighbours with the placed block as the source, so a redstone torch set beside a T-junction
/// curves it north. A player placing the torch would not switch it: `Level.setBlock` gives the
/// replaced block (air) as the source.
pub(super) async fn test_setblock_torch_switches_rail_junction(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let rail = |shape| {
        let mut props = RailLikeProperties::default(&Block::RAIL);
        props.shape = shape;
        props.to_state_id(&Block::RAIL)
    };
    let north_east = rail(RailShape::NorthEast);
    let south_east = rail(RailShape::SouthEast);
    let junction = BlockPos::new(2, 1, 1);
    helper
        .set_block(&BlockPos::new(2, 1, 0), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(2, 1, 2), rail(RailShape::NorthSouth))
        .await?;
    helper
        .set_block(&BlockPos::new(3, 1, 1), rail(RailShape::EastWest))
        .await?;
    helper.set_block(&junction, south_east).await?;
    helper.assert_block_state(&junction, south_east).await?;

    let origin = BlockPos::new(0, 1, 0);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(&origin, "setblock ~1 ~ ~1 minecraft:redstone_torch")
                .await?;
            Ok(())
        })
        .then_execute(move |helper| async move {
            helper.assert_block_state(&junction, north_east).await?;
            helper
                .execute_command(&origin, "setblock ~1 ~ ~1 minecraft:air")
                .await?;
            helper.set_block(&junction, south_east).await?;
            Ok(())
        })
        .then_execute(move |helper| async move {
            helper.assert_block_state(&junction, south_east).await?;
            helper
                .execute_command(&origin, "fill ~1 ~ ~1 ~1 ~ ~1 minecraft:redstone_torch")
                .await?;
            Ok(())
        })
        .then_execute(move |helper| async move {
            helper.assert_block_state(&junction, north_east).await
        })
        .then_succeed();
    Ok(())
}
