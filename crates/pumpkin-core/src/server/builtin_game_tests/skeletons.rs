use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

/// A helmet keeps the burning skeletons out of the sun goals whatever the time of day.
const HELMET: &str = "head:{id:\"minecraft:diamond_helmet\",count:1}";

/// Stone walls two blocks high around the edge of a `width` by `depth` floor.
async fn build_pen(helper: &GameTestHelper, width: i32, depth: i32) -> GameTestResult<()> {
    for x in 0..width {
        for z in 0..depth {
            if x == 0 || x == width - 1 || z == 0 || z == depth - 1 {
                for y in 1..=2 {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Vanilla `AbstractSkeleton.reassessWeaponGoal`: a skeleton whose bow is swapped for a sword
/// drops the bow goal (which keeps it at range) for the melee goal and walks up to its target.
pub(super) async fn test_skeleton_switches_to_melee_without_bow(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    build_pen(&helper, 11, 5).await?;
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            &format!(
                "summon minecraft:skeleton ~ ~ ~ {{PersistenceRequired:1b,equipment:{{mainhand:{{id:\"minecraft:bow\",count:1}},{HELMET}}}}}"
            ),
        )
        .await?;
    let near_golem_min = BlockPos::new(6, 0, 0);
    let near_golem_max = BlockPos::new(11, 4, 5);
    helper
        .start_sequence()
        .then_idle(40)
        .then_execute(move |helper| async move {
            // With the bow it shoots from where it stands.
            helper
                .assert_entity_not_present_in_box(
                    "minecraft:skeleton",
                    near_golem_min,
                    near_golem_max,
                )
                .await
        })
        .then_execute(|helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 2),
                    "item replace entity @e[type=minecraft:skeleton,limit=1,sort=nearest] weapon.mainhand with minecraft:stone_sword",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box("minecraft:skeleton", near_golem_min, near_golem_max)
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `RangedBowAttackGoal.isHoldingBow` (`Mob.isHolding`) and
/// `ProjectileUtil.getWeaponHoldingHand` accept a bow in the off hand, so a skeleton holding
/// only that still shoots.
pub(super) async fn test_skeleton_shoots_with_bow_in_off_hand(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    build_pen(&helper, 11, 5).await?;
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            &format!(
                "summon minecraft:skeleton ~ ~ ~ {{PersistenceRequired:1b,equipment:{{mainhand:{{id:\"minecraft:stick\",count:1}},offhand:{{id:\"minecraft:bow\",count:1}},{HELMET}}}}}"
            ),
        )
        .await?;
    let area_min = BlockPos::new(0, 0, 0);
    let area_max = BlockPos::new(11, 6, 5);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box("minecraft:arrow", area_min, area_max)
            .await
    })?;
    Ok(())
}

/// A skeleton variant with a bow shoots a survival player standing in a stone pocket across the
/// pen until one arrow lands, and the player then has `effect`.
async fn variant_arrow_applies_effect(
    helper: GameTestHelper,
    variant: &str,
    effect: &str,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    build_pen(&helper, 11, 5).await?;
    // The pocket stops the arrows' knockback pushing the player out of reach.
    for (x, z) in [(8, 1), (9, 1), (8, 3), (9, 3)] {
        for y in 1..=2 {
            helper
                .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                .await?;
        }
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(9.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            &format!(
                "summon {variant} ~ ~ ~ {{PersistenceRequired:1b,equipment:{{mainhand:{{id:\"minecraft:bow\",count:1}},{HELMET}}}}}"
            ),
        )
        .await?;
    let area_min = BlockPos::new(0, 0, 0);
    let area_max = BlockPos::new(11, 6, 5);
    let effect = NbtTag::String(effect.into());
    helper.succeed_when(move |helper| {
        let effect = effect.clone();
        async move {
            helper
                .assert_entity_nbt(
                    "minecraft:player",
                    area_min,
                    area_max,
                    "active_effects[].id",
                    &effect,
                )
                .await
        }
    })?;
    Ok(())
}

/// Vanilla `Stray.getArrow`: 30 seconds of slowness on every arrow.
pub(super) async fn test_stray_arrow_applies_slowness(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    variant_arrow_applies_effect(helper, "minecraft:stray", "minecraft:slowness").await
}

/// Vanilla `Bogged.getArrow`: 5 seconds of poison on every arrow.
pub(super) async fn test_bogged_arrow_applies_poison(helper: GameTestHelper) -> GameTestResult<()> {
    variant_arrow_applies_effect(helper, "minecraft:bogged", "minecraft:poison").await
}

/// Vanilla `Parched.getArrow`: 30 seconds of weakness on every arrow.
pub(super) async fn test_parched_arrow_applies_weakness(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    variant_arrow_applies_effect(helper, "minecraft:parched", "minecraft:weakness").await
}

/// Vanilla `WitherSkeleton.registerGoals` adds a `NearestAttackableTargetGoal` for every
/// `AbstractPiglin`.
pub(super) async fn test_wither_skeleton_targets_piglin(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    build_pen(&helper, 9, 5).await?;
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            "summon minecraft:piglin ~ ~ ~ {NoAI:1b,PersistenceRequired:1b,IsImmuneToZombification:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:wither_skeleton ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    let pen_min = BlockPos::new(0, 0, 0);
    let pen_max = BlockPos::new(9, 4, 5);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_target(
                "minecraft:wither_skeleton",
                pen_min,
                pen_max,
                Some("minecraft:piglin"),
            )
            .await
    })?;
    Ok(())
}
