use pumpkin_gametest::{GameTestHelper, GameTestResult};
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
