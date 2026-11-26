use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

/// A stone floor with stone walls `height` blocks high around its edge.
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

/// Whether some `entity_name` in the box has a float `Health` that passes `check`.
async fn any_health(
    helper: &GameTestHelper,
    entity_name: &str,
    min: BlockPos,
    max: BlockPos,
    check: impl Fn(f32) -> bool,
) -> GameTestResult<bool> {
    let health = helper
        .entity_nbt_values(entity_name, min, max, "Health")
        .await?;
    Ok(health
        .iter()
        .flatten()
        .any(|tag| matches!(tag, NbtTag::Float(value) if check(*value))))
}

/// Vanilla `Pillager.registerGoals`: the player target at 2 and `RangedCrossbowAttackGoal` at 3.
/// The pillager charges its crossbow and shoots a survival player in a stone pocket across the
/// pen until a bolt lands.
pub(super) async fn test_pillager_shoots_crossbow_at_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    // The pocket stops the bolts' knockback pushing the player out of range.
    for (x, z) in [(8, 1), (9, 1), (8, 3), (9, 3)] {
        for y in 1..=2 {
            helper
                .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                .await?;
        }
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:pillager ~ ~ ~ {PersistenceRequired:1b,equipment:{mainhand:{id:\"minecraft:crossbow\",count:1}}}",
        )
        .await?;

    let area_min = BlockPos::new(1, 1, 1);
    let area_max = BlockPos::new(10, 3, 4);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            let hurt = any_health(&helper, "minecraft:player", area_min, area_max, |health| {
                health < 20.0
            })
            .await?;
            helper.assert_true(hurt, "the pillager never hit the player with a bolt")
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Vindicator.VindicatorJohnnyAttackGoal`: a vindicator named Johnny targets any
/// attackable mob, here a pig no other vindicator would care about.
pub(super) async fn test_vindicator_named_johnny_attacks_any_mob(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:pig ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:vindicator ~ ~ ~ {PersistenceRequired:1b,CustomName:\"Johnny\"}",
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(10, 4, 4);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_target(
                "minecraft:vindicator",
                pen_min,
                pen_max,
                Some("minecraft:pig"),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `Ravager.aiStep`: a ravager that runs into leaves while `mobGriefing` is on breaks
/// them, and its pathing treats leaves as free (`PathType.LEAVES` malus 0). A wall of leaves two
/// blocks high, too high to step over, stands between the ravager and an iron golem it targets.
pub(super) async fn test_ravager_breaks_leaves_in_its_path(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 4).await?;
    for z in 1..=3 {
        for y in 1..=2 {
            helper
                .execute_command(
                    &BlockPos::new(6, y, z),
                    "setblock ~ ~ ~ minecraft:oak_leaves[persistent=true]",
                )
                .await?;
        }
    }
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(3, 1, 2),
            "summon minecraft:ravager ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;

    helper.succeed_when(|helper| async move {
        helper
            .assert_block_present(&BlockPos::new(6, 1, 2), &Block::AIR)
            .await?;
        helper
            .assert_block_present(&BlockPos::new(6, 2, 2), &Block::AIR)
            .await
    })?;
    Ok(())
}

/// G1-48, vanilla `Ravager.registerGoals`: its `AbstractVillager` target goal skips babies
/// (`!target.isBaby()`). A ravager penned with a baby villager leaves it alone for 200 ticks, then
/// targets the same villager once it has grown up.
pub(super) async fn test_ravager_ignores_baby_villager(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    let villager = BlockPos::new(8, 1, 2);
    helper
        .execute_command(
            &villager,
            "summon minecraft:villager ~ ~ ~ {NoAI:1b,PersistenceRequired:1b,Age:-24000}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:ravager ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(10, 4, 4);
    helper
        .start_sequence()
        .then_execute_for(200, move |helper| async move {
            helper
                .assert_entity_target("minecraft:ravager", pen_min, pen_max, None)
                .await
        })
        .then_execute(move |helper| async move {
            helper
                .execute_command(
                    &villager,
                    "data merge entity @e[type=minecraft:villager,distance=..2,limit=1] {Age:0}",
                )
                .await?;
            Ok(())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:ravager",
                    pen_min,
                    pen_max,
                    Some("minecraft:villager"),
                )
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Witch.registerGoals`: `NearestHealableRaiderTargetGoal` picks a raider of the
/// witch's raid, and `performRangedAttack` throws healing at one with 4 health or less.
pub(super) async fn test_witch_heals_hurt_raider(helper: GameTestHelper) -> GameTestResult<()> {
    build_pen(&helper, 9, 5, 3).await?;
    let raid_id = helper
        .start_raid_without_waves(&BlockPos::new(4, 1, 2))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            &format!(
                "summon minecraft:pillager ~ ~ ~ {{NoAI:1b,PersistenceRequired:1b,Health:3f,RaidId:{raid_id}}}"
            ),
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            &format!("summon minecraft:witch ~ ~ ~ {{PersistenceRequired:1b,RaidId:{raid_id}}}"),
        )
        .await?;

    let pen_min = BlockPos::new(1, 1, 1);
    let pen_max = BlockPos::new(8, 3, 4);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            let healed = any_health(&helper, "minecraft:pillager", pen_min, pen_max, |health| {
                health > 3.0
            })
            .await?;
            helper.assert_true(healed, "the witch never healed the hurt pillager")
        })
        .then_execute(move |helper| async move { helper.stop_raid(raid_id).await })
        .then_succeed();
    Ok(())
}
