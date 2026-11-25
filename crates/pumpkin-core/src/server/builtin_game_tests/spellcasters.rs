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

/// Removes this test's vexes, which fly through walls and could reach the next test. Only
/// within 16 blocks: the tester puts mob tests 56 blocks apart, and the next test has its own.
async fn kill_vexes(helper: &GameTestHelper) -> GameTestResult<()> {
    helper
        .execute_command(
            &BlockPos::new(5, 1, 2),
            "kill @e[type=minecraft:vex,distance=..16]",
        )
        .await
        .map(|_| ())
}

/// Vanilla `Evoker.EvokerSummonSpellGoal`: an evoker that targets a survival player summons
/// three vexes it owns, one block above spots around it.
pub(super) async fn test_evoker_summons_vexes_at_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:evoker ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;

    // Wider than the pen: the evoker backs away from the player, and vexes appear up to 2
    // blocks from it.
    let area_min = BlockPos::new(-4, -2, -4);
    let area_max = BlockPos::new(14, 8, 8);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            let owners = helper
                .entity_nbt_values("minecraft:vex", area_min, area_max, "owner")
                .await?;
            let owned = owners.iter().filter(|tags| !tags.is_empty()).count();
            helper.assert_true(
                owned >= 3,
                format!("expected 3 vexes with an owner, found {owned}"),
            )
        })
        .then_execute(move |helper| async move { kill_vexes(&helper).await })
        .then_succeed();
    Ok(())
}

/// Vanilla `Illusioner.registerGoals`: `RangedBowAttackGoal` at 6. The illusioner draws its bow
/// and shoots a survival player in a stone pocket across the pen until an arrow lands.
pub(super) async fn test_illusioner_shoots_bow_at_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_pen(&helper, 11, 5, 3).await?;
    // The pocket stops the arrows' knockback pushing the player out of range.
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
            "summon minecraft:illusioner ~ ~ ~ {PersistenceRequired:1b,equipment:{mainhand:{id:\"minecraft:bow\",count:1}}}",
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
            helper.assert_true(hurt, "the illusioner never hit the player with an arrow")
        })
        .then_succeed();
    Ok(())
}

/// Horizontal speed above anything a vex reaches on a random move (speed 0.25, about 0.14
/// blocks a tick with the air drag of 0.91); a charge (speed 1.0) passes it within a few ticks.
/// Vertical speed tells nothing: with the vertical drag of 0.98, a random move up or down gets
/// past 0.2 too.
const VEX_CHARGE_SPEED: f64 = 0.2;

/// Keeps a wandering vex ticking: it stops in a chunk that isn't loaded.
const FORCELOAD_AROUND_TEST: (&str, &str) = (
    "forceload add ~-16 ~-16 ~16 ~16",
    "forceload remove ~-16 ~-16 ~16 ~16",
);

/// Vanilla `Vex.tick` and `VexMoveControl`: a vex has no physics, so it charges straight
/// through stone. Once the vex has a survival player as its target and is charging from outside
/// the space around the player, a stone shell closes around the player, and the vex still gets
/// into it. The test doesn't wait for a hit, which is up to chance in vanilla too: the vex aims
/// its feet at the player's eyes and stops within about half a block of them, sometimes just
/// above the player's head.
pub(super) async fn test_vex_flies_through_wall_to_target(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    let center = BlockPos::new(5, 1, 2);
    helper
        .execute_command(&center, FORCELOAD_AROUND_TEST.0)
        .await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    // Resistance V: the vex hits the player before the shell is up, and mustn't kill them.
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "effect give @a[distance=..2] minecraft:resistance 1000 4 true",
        )
        .await?;
    // Its random moves stay around (1, 2, 2), mostly outside the space the shell fills.
    let bound = helper.absolute_pos(&BlockPos::new(1, 2, 2));
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            &format!(
                "summon minecraft:vex ~ ~ ~ {{PersistenceRequired:1b,bound_pos:[I;{},{},{}]}}",
                bound.0.x, bound.0.y, bound.0.z
            ),
        )
        .await?;

    // The space the shell fills. Touching its faces doesn't count as being in it.
    let shell_min = BlockPos::new(7, 0, 1);
    let shell_max = BlockPos::new(10, 4, 4);
    // Everywhere its random moves can take it: vanilla `VexRandomMoveGoal` picks spots up to 7
    // blocks across and 5 up or down from (1, 2, 2). A narrower box misses most charges, which
    // start from wherever the last random move ended.
    let area_min = BlockPos::new(-7, -4, -6);
    let area_max = BlockPos::new(10, 8, 10);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:vex",
                    area_min,
                    area_max,
                    Some("minecraft:player"),
                )
                .await?;
            // Outside the space the shell fills, so the vex can only get in through stone.
            helper
                .assert_entity_not_present_in_box("minecraft:vex", shell_min, shell_max)
                .await?;
            let motion = helper
                .entity_nbt_values("minecraft:vex", area_min, area_max, "Motion")
                .await?;
            let charging = motion.iter().flatten().any(|tag| match tag {
                NbtTag::List(values) => match values.as_slice() {
                    [NbtTag::Double(x), _, NbtTag::Double(z)] => {
                        x.mul_add(*x, z * z) > VEX_CHARGE_SPEED * VEX_CHARGE_SPEED
                    }
                    _ => false,
                },
                _ => false,
            });
            helper.assert_true(charging, "the vex isn't charging")
        })
        .then_execute(move |helper| async move {
            // A stone shell around the player at (8, 1..2, 2).
            for x in 7..=9 {
                for y in 1..=3 {
                    for z in 1..=3 {
                        if x != 8 || z != 2 || y == 3 {
                            helper
                                .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                                .await?;
                        }
                    }
                }
            }
            Ok(())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box("minecraft:vex", shell_min, shell_max)
                .await
        })
        .then_execute(move |helper| async move {
            kill_vexes(&helper).await?;
            helper
                .execute_command(&center, FORCELOAD_AROUND_TEST.1)
                .await
                .map(|_| ())
        })
        .then_succeed();
    Ok(())
}
