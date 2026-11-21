use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

/// Stone walls one block in from the floor's edge, `height` blocks high, with a stone roof on
/// top. No sky light gets in, so a spider inside targets players whatever the time of day.
async fn build_dark_box(
    helper: &GameTestHelper,
    width: i32,
    depth: i32,
    height: i32,
) -> GameTestResult<()> {
    build_stone_floor(helper, width, depth).await?;
    for x in 0..width {
        for z in 0..depth {
            let edge = x == 0 || x == width - 1 || z == 0 || z == depth - 1;
            for y in 1..=height + 1 {
                if edge || y == height + 1 {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Vanilla `Spider`: `WallClimberNavigation` paths straight at a target it cannot walk to, and
/// `Spider.tick` sets the climbing flag on every horizontal collision, so the spider walks up
/// a two-block ledge, which no jump reaches, to the player on top.
pub(super) async fn test_spider_climbs_wall_to_target(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_dark_box(&helper, 9, 5, 4).await?;
    for x in 5..8 {
        for z in 1..4 {
            for y in 1..3 {
                helper
                    .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                    .await?;
            }
        }
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(7.5, 3.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(1, 1, 2),
            "summon minecraft:spider ~0.5 ~ ~0.5 {PersistenceRequired:1b}",
        )
        .await?;

    helper.succeed_when(|helper| async move {
        helper
            .assert_entity_present_in_box(
                "minecraft:spider",
                BlockPos::new(6, 3, 1),
                BlockPos::new(8, 5, 4),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `SpiderTargetGoal` (no new target at a brightness of 0.5 or more) and
/// `SpiderAttackGoal.canContinueToUse` (a 1 in 100 chance each tick to drop the target there):
/// a cave spider that found a player in the dark gives up once its roof is gone at noon, and
/// does not pick the player again. The player floats out of reach, so it never gets hurt.
pub(super) async fn test_cave_spider_stops_targeting_in_daylight(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_dark_box(&helper, 9, 7, 5).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(4.5, 3.0, 3.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(1, 1, 3),
            "summon minecraft:cave_spider ~0.5 ~ ~0.5 {PersistenceRequired:1b}",
        )
        .await?;

    let box_min = BlockPos::new(0, 1, 0);
    let box_max = BlockPos::new(9, 7, 7);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:cave_spider",
                    box_min,
                    box_max,
                    Some("minecraft:player"),
                )
                .await
        })
        .then_execute(|helper| async move {
            helper
                .execute_command(&BlockPos::new(0, 1, 0), "time set noon")
                .await?;
            helper
                .execute_command(
                    &BlockPos::new(0, 1, 0),
                    "fill ~1 ~6 ~1 ~7 ~6 ~5 minecraft:air",
                )
                .await?;
            Ok(())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target("minecraft:cave_spider", box_min, box_max, None)
                .await
        })
        .then_execute(|helper| async move {
            helper
                .assert_entity_present_in_box(
                    "minecraft:player",
                    BlockPos::new(4, 3, 3),
                    BlockPos::new(5, 5, 4),
                )
                .await
        })
        .then_succeed();
    Ok(())
}
