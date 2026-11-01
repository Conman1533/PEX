use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::GameMode;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

pub(super) async fn test_item_entities_merge(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;

    for x in 0..4 {
        for z in 0..4 {
            if x == 0 || x == 3 || z == 0 || z == 3 {
                helper
                    .set_block_default(&BlockPos::new(x, 1, z), &Block::STONE)
                    .await?;
            }
        }
    }

    helper
        .spawn_item_at("minecraft:diamond", 10, Vector3::new(1.5, 1.1, 1.5))
        .await?;
    helper
        .spawn_item_at("minecraft:diamond", 15, Vector3::new(1.5, 1.1, 1.5))
        .await?;

    let center = BlockPos::new(1, 1, 1);
    helper.succeed_when(move |helper| async move {
        helper.assert_entities_present("minecraft:item", 1).await?;
        helper
            .assert_item_entity_count("minecraft:diamond", &center, 2.0, 25)
            .await
    })?;
    Ok(())
}

pub(super) async fn test_zombie_pathfinds_to_villager(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 8).await?;

    for x in 0..5 {
        for z in 0..8 {
            if x == 0 || x == 4 || z == 0 || z == 7 {
                helper
                    .set_block_default(&BlockPos::new(x, 1, z), &Block::STONE)
                    .await?;
                helper
                    .set_block_default(&BlockPos::new(x, 2, z), &Block::STONE)
                    .await?;
            }
        }
    }

    helper
        .spawn_entity_at("minecraft:villager", Vector3::new(2.5, 1.0, 6.0))
        .await?;
    helper
        .spawn_entity_at("minecraft:husk", Vector3::new(2.5, 1.0, 1.5))
        .await?;

    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box(
                "minecraft:husk",
                BlockPos::new(1, 1, 4),
                BlockPos::new(4, 3, 7),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `Zombie.addBehaviourGoals`: `NearestAttackableTargetGoal<Player>` (priority 2) picks a
/// survival player the zombie can see.
pub(super) async fn test_zombie_targets_survival_mock_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 3).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:iron_sword", Vector3::new(7.5, 1.0, 1.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:zombie", Vector3::new(1.5, 1.0, 1.5))
        .await?;

    let floor_min = BlockPos::new(0, 1, 0);
    let floor_max = BlockPos::new(9, 3, 3);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .assert_entity_is_holding(
                    "minecraft:player",
                    floor_min,
                    floor_max,
                    "minecraft:iron_sword",
                )
                .await?;
            helper
                .assert_entity_nbt(
                    "minecraft:player",
                    floor_min,
                    floor_max,
                    "playerGameType",
                    &NbtTag::Int(GameMode::Survival as i32),
                )
                .await
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:zombie",
                    floor_min,
                    floor_max,
                    Some("minecraft:player"),
                )
                .await
        })
        .then_succeed();
    Ok(())
}
