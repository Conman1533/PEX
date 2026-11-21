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
