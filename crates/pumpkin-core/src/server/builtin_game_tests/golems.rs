use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

pub(super) async fn test_snow_golem_pattern_spawns(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;

    let base_pos = BlockPos::new(2, 1, 2);
    let mid_pos = BlockPos::new(2, 2, 2);
    let head_pos = BlockPos::new(2, 3, 2);

    helper
        .set_block_default(&base_pos, &Block::SNOW_BLOCK)
        .await?;
    helper
        .set_block_default(&mid_pos, &Block::SNOW_BLOCK)
        .await?;
    helper
        .set_block_default(&head_pos, &Block::CARVED_PUMPKIN)
        .await?;

    helper.assert_block_present(&base_pos, &Block::AIR).await?;
    helper.assert_block_present(&mid_pos, &Block::AIR).await?;
    helper.assert_block_present(&head_pos, &Block::AIR).await?;
    helper.assert_entity_present("minecraft:snow_golem").await?;

    helper.succeed();
    Ok(())
}

pub(super) async fn test_iron_golem_pattern_spawns(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;

    let base_pos = BlockPos::new(2, 1, 2);
    let mid_pos = BlockPos::new(2, 2, 2);
    let left_arm = BlockPos::new(1, 2, 2);
    let right_arm = BlockPos::new(3, 2, 2);
    let head_pos = BlockPos::new(2, 3, 2);

    helper
        .set_block_default(&base_pos, &Block::IRON_BLOCK)
        .await?;
    helper
        .set_block_default(&mid_pos, &Block::IRON_BLOCK)
        .await?;
    helper
        .set_block_default(&left_arm, &Block::IRON_BLOCK)
        .await?;
    helper
        .set_block_default(&right_arm, &Block::IRON_BLOCK)
        .await?;
    helper
        .set_block_default(&head_pos, &Block::CARVED_PUMPKIN)
        .await?;

    for pos in [base_pos, mid_pos, left_arm, right_arm, head_pos] {
        helper.assert_block_present(&pos, &Block::AIR).await?;
    }
    helper.assert_entity_present("minecraft:iron_golem").await?;

    helper.succeed();
    Ok(())
}

pub(super) async fn test_wither_pattern_spawns(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;

    let base_pos = BlockPos::new(2, 1, 2);
    let mid_pos = BlockPos::new(2, 2, 2);
    let left_arm = BlockPos::new(1, 2, 2);
    let right_arm = BlockPos::new(3, 2, 2);
    let left_skull = BlockPos::new(1, 3, 2);
    let right_skull = BlockPos::new(3, 3, 2);
    let center_skull = BlockPos::new(2, 3, 2);

    for pos in [base_pos, mid_pos, left_arm, right_arm] {
        helper.set_block_default(&pos, &Block::SOUL_SAND).await?;
    }
    for pos in [left_skull, right_skull, center_skull] {
        helper
            .set_block_default(&pos, &Block::WITHER_SKELETON_SKULL)
            .await?;
    }

    for pos in [
        base_pos,
        mid_pos,
        left_arm,
        right_arm,
        left_skull,
        right_skull,
        center_skull,
    ] {
        helper.assert_block_present(&pos, &Block::AIR).await?;
    }
    helper.assert_entity_present("minecraft:wither").await?;

    helper.succeed();
    Ok(())
}

/// Vanilla `WitherBoss.registerGoals`: its `NearestAttackableTargetGoal` over `LivingEntity`, with
/// `LIVING_ENTITY_SELECTOR`, makes a survival player its target, and its `RangedAttackGoal` then
/// shoots a skull at it. The heads pick their own targets apart from this, so the test checks the
/// wither's target itself before it waits for the skull.
pub(super) async fn test_wither_targets_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 11).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 5.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:wither", Vector3::new(2.5, 1.0, 5.5))
        .await?;
    // The wither flies about while it attacks, so the box is taller than the test.
    let area_min = BlockPos::new(-10, -5, -10);
    let area_max = BlockPos::new(20, 20, 20);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:wither",
                    area_min,
                    area_max,
                    Some("minecraft:player"),
                )
                .await
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box("minecraft:wither_skull", area_min, area_max)
                .await
        })
        .then_succeed();
    Ok(())
}
