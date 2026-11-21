use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

/// Long enough for a tempted cow to walk to the player, too short for a random stroll.
const COW_TEMPT_TICKS: u32 = 60;

/// Vanilla `AbstractCow.registerGoals`: `TemptGoal` (speed 1.25, `#cow_food`) walks the cow to a
/// player holding wheat, and `PanicGoal` (speed 2.0) runs it off when that player hits it.
pub(super) async fn test_cow_follows_wheat_and_panics_when_hit(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    helper
        .spawn_mock_player_at("minecraft:wheat", Vector3::new(9.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:cow", Vector3::new(1.5, 1.0, 2.5))
        .await?;

    // TemptGoal stops the cow 2.5 blocks short of the player, inside this box.
    let near_player_min = BlockPos::new(6, 1, 2);
    let near_player_max = BlockPos::new(10, 3, 3);
    // The hit's knockback throws the cow about two blocks back along the lane it walked in on,
    // and TemptGoal walks it back. Only a panic run leaves the lane.
    let lane_min = BlockPos::new(3, 1, 2);
    let lane_max = BlockPos::new(10, 3, 3);
    helper
        .start_sequence()
        .then_execute_after(COW_TEMPT_TICKS, move |helper| async move {
            helper
                .assert_entity_present_in_box("minecraft:cow", near_player_min, near_player_max)
                .await
        })
        .then_execute(|helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(7, 1, 2),
                    "damage @e[type=minecraft:cow,limit=1,sort=nearest] 1 minecraft:player_attack by @p",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_not_present_in_box("minecraft:cow", lane_min, lane_max)
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `ArmadilloAi`: the `armadillo_scare_detected` sensor sees an undead mob within 7 blocks
/// across and 2 up, `ArmadilloBallUp` curls the armadillo up, and once the zombie is gone and
/// `danger_detected_recently` runs out, it unrolls.
pub(super) async fn test_armadillo_rolls_up_near_zombie(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    // A wall two blocks high keeps the idle armadillo's random strolls inside the box it is
    // looked for in.
    for x in 0..9 {
        for z in 0..5 {
            if x == 0 || x == 8 || z == 0 || z == 4 {
                for y in 1..3 {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    helper
        .spawn_entity_at("minecraft:armadillo", Vector3::new(2.5, 1.0, 2.5))
        .await?;
    let area_min = BlockPos::new(0, 1, 0);
    let area_max = BlockPos::new(8, 3, 4);
    let state_is = move |helper: GameTestHelper, state: &'static str| async move {
        helper
            .assert_entity_nbt(
                "minecraft:armadillo",
                area_min,
                area_max,
                "state",
                &NbtTag::String(state.into()),
            )
            .await
    };
    helper
        .start_sequence()
        .then_execute_after(10, move |helper| state_is(helper, "idle"))
        .then_execute(|helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(6, 1, 2),
                    "summon minecraft:zombie ~0.5 ~ ~0.5 {NoAI:1b,PersistenceRequired:1b}",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| state_is(helper, "scared"))
        .then_execute(|helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(6, 1, 2),
                    "kill @e[type=minecraft:zombie,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| state_is(helper, "idle"))
        .then_succeed();
    Ok(())
}

/// Vanilla `CamelAi`: the idle activity's `FollowTemptation` (speed 2.5, `#camel_food`) walks a
/// camel to a player holding cactus and stops it 3.5 blocks short.
pub(super) async fn test_camel_follows_cactus(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 13, 5).await?;
    helper
        .spawn_mock_player_at("minecraft:cactus", Vector3::new(11.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:camel", Vector3::new(1.5, 1.0, 2.5))
        .await?;

    let near_player_min = BlockPos::new(6, 1, 0);
    let near_player_max = BlockPos::new(12, 4, 4);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box("minecraft:camel", near_player_min, near_player_max)
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Mob.getNavigation`: a husk riding in front of a camel husk steers it, so the husk's
/// own attack goal walks the camel husk to the survival player it targets. The camel husk's own
/// brain still picks random strolls in between, as in vanilla, so the walk takes a while.
pub(super) async fn test_husk_rider_steers_camel_husk_to_target(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 13, 5).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(11.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:camel_husk", Vector3::new(1.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:husk", Vector3::new(1.5, 1.0, 0.5))
        .await?;

    let near_player_min = BlockPos::new(6, 1, 0);
    let near_player_max = BlockPos::new(12, 4, 4);
    helper
        .start_sequence()
        .then_execute_after(5, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(1, 1, 2),
                    "ride @e[type=minecraft:husk,limit=1,sort=nearest] mount \
                     @e[type=minecraft:camel_husk,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box(
                    "minecraft:camel_husk",
                    near_player_min,
                    near_player_max,
                )
                .await
        })
        .then_succeed();
    Ok(())
}
