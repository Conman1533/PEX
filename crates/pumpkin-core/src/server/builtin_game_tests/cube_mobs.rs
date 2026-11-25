use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

/// A stone floor with stone walls `height` blocks high around its edge, open to the sky.
async fn build_pen(
    helper: &GameTestHelper,
    width: i32,
    depth: i32,
    height: i32,
) -> GameTestResult<()> {
    build_stone_floor(helper, width, depth).await?;
    for x in 0..width {
        for z in 0..depth {
            if x == 0 || x == width - 1 || z == 0 || z == depth - 1 {
                for y in 1..=height {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Vanilla `Slime`: `NearestAttackableTargetGoal<Player>` (within 4 blocks of height) picks the
/// player, `CubeMobAttackGoal` turns towards it, and `CubeMobMoveControl` hops that way.
pub(super) async fn test_slime_hops_toward_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:slime ~ ~ ~ {Size:1,PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(10, 4, 4);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_target(
                "minecraft:slime",
                pen_min,
                pen_max,
                Some("minecraft:player"),
            )
            .await?;
        helper
            .assert_entity_present_in_box(
                "minecraft:slime",
                BlockPos::new(6, 1, 1),
                BlockPos::new(10, 4, 4),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `MagmaCube.addTargetingGoals`: with no player around, the magma cube's
/// `NearestAttackableTargetGoal<IronGolem>` picks the golem.
pub(super) async fn test_magma_cube_targets_iron_golem(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 4).await?;
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:magma_cube ~ ~ ~ {Size:1,PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(10, 5, 4);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_target(
                "minecraft:magma_cube",
                pen_min,
                pen_max,
                Some("minecraft:iron_golem"),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `SulfurCube`: `SulfurCubeSearchForItemsGoal` turns it towards a swallowable item
/// within 8 blocks, and `pickUpItem` swallows one into its body slot.
pub(super) async fn test_sulfur_cube_picks_up_dropped_item(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 9, 5, 3).await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:sulfur_cube ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .spawn_item_at("minecraft:oak_planks", 1, Vector3::new(6.5, 1.1, 2.5))
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(8, 4, 4);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_nbt(
                "minecraft:sulfur_cube",
                pen_min,
                pen_max,
                "equipment.body.id",
                &NbtTag::String("minecraft:oak_planks".into()),
            )
            .await?;
        helper
            .assert_item_entity_not_present("minecraft:oak_planks", &BlockPos::new(4, 1, 2), 4.0)
            .await
    })?;
    Ok(())
}

/// Vanilla `AbstractCubeMob.push`: an iron golem that bumps into a slime big enough to deal
/// damage takes the slime's attack damage (4 for size 4). The slime is invulnerable, so the
/// golem's own attacks cannot end the test early.
pub(super) async fn test_slime_hurts_iron_golem_it_bumps_into(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 7, 5, 4).await?;
    helper
        .execute_command(
            &BlockPos::new(4, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:slime ~ ~ ~ {Size:3,Invulnerable:1b,PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(6, 5, 4);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_nbt(
                "minecraft:iron_golem",
                pen_min,
                pen_max,
                "Health",
                &NbtTag::Float(96.0),
            )
            .await
    })?;
    Ok(())
}
