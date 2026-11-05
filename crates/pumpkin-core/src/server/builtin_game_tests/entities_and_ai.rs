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

/// Vanilla `DragonFireball.onHit`: the fireball leaves a cloud of dragon's breath, moved onto a
/// living entity within 4 blocks of the impact.
pub(super) async fn test_dragon_fireball_leaves_breath_cloud(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 10, 3).await?;
    helper
        .execute_command(
            &BlockPos::new(6, 1, 1),
            "summon minecraft:pig ~ ~ ~ {NoAI:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(3, 1, 1),
            "summon minecraft:dragon_fireball ~ ~2.5 ~ {Motion:[0.0d,-0.5d,0.0d]}",
        )
        .await?;

    helper.succeed_when(|helper| async move {
        helper
            .assert_entity_not_present("minecraft:dragon_fireball")
            .await?;
        // The cloud is 6 blocks wide, so only one centred on the pig reaches x 8..9; one left at
        // the impact (x 3.5) would stop at x 6.5.
        helper
            .assert_entity_present_in_box(
                "minecraft:area_effect_cloud",
                BlockPos::new(8, 1, 1),
                BlockPos::new(9, 2, 2),
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
