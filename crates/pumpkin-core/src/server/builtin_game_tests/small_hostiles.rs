use pumpkin_data::Block;
use pumpkin_data::block_properties::{SculkSensorLikeProperties, SculkSensorPhase};
use pumpkin_gametest::{GameTestError, GameTestHelper, GameTestResult};
use pumpkin_util::math::position::BlockPos;

/// An oak plank floor with plank walls `wall_height` high around its edge. Planks, not stone,
/// so a silverfish can't merge into them.
async fn build_plank_pen(
    helper: &GameTestHelper,
    width: i32,
    depth: i32,
    wall_height: i32,
) -> GameTestResult<()> {
    for x in 0..width {
        for z in 0..depth {
            helper
                .set_block_default(&BlockPos::new(x, 0, z), &Block::OAK_PLANKS)
                .await?;
            if x == 0 || x == width - 1 || z == 0 || z == depth - 1 {
                for y in 1..=wall_height {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::OAK_PLANKS)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Vanilla `Silverfish.SilverfishWakeUpFriendsGoal`: a silverfish hurt by an entity breaks the
/// infested blocks around it a second later, and each one lets out another silverfish.
pub(super) async fn test_silverfish_wakes_friends_from_infested_stone(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_plank_pen(&helper, 9, 5, 2).await?;
    let infested = BlockPos::new(5, 1, 2);
    helper
        .set_block_default(&infested, &Block::INFESTED_STONE)
        .await?;
    helper
        .execute_command(
            &BlockPos::new(7, 1, 1),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:silverfish ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .start_sequence()
        .then_execute_after(1, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 2),
                    "damage @e[type=minecraft:silverfish,limit=1,sort=nearest] 1 minecraft:mob_attack by @e[type=minecraft:iron_golem,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_block_not_present(&infested, &Block::INFESTED_STONE)
                .await?;
            helper.assert_entities_present("minecraft:silverfish", 2).await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `ClimbOnTopOfPowderSnowGoal`: an endermite (a `powder_snow_walkable_mobs` mob) stuck
/// in powder snow jumps until it stands on top.
pub(super) async fn test_endermite_climbs_out_of_powder_snow(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_plank_pen(&helper, 5, 5, 0).await?;
    let column = (2, 2);
    // A shaft three blocks deep: two of powder snow, then air walled in so the endermite stays
    // on top.
    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        for y in 1..=3 {
            helper
                .set_block_default(
                    &BlockPos::new(column.0 + dx, y, column.1 + dz),
                    &Block::OAK_PLANKS,
                )
                .await?;
        }
    }
    for y in 1..=2 {
        helper
            .set_block_default(&BlockPos::new(column.0, y, column.1), &Block::POWDER_SNOW)
            .await?;
    }
    helper
        .execute_command(
            &BlockPos::new(column.0, 1, column.1),
            "summon minecraft:endermite ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    let top_min = BlockPos::new(column.0, 3, column.1);
    let top_max = BlockPos::new(column.0 + 1, 4, column.1 + 1);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box("minecraft:endermite", top_min, top_max)
            .await
    })?;
    Ok(())
}

/// Vanilla `Endermite.registerGoals`: `HurtByTargetGoal.setAlertOthers()`, so every endermite
/// around turns on whoever hit one of them.
pub(super) async fn test_endermites_alert_each_other_when_hit(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_plank_pen(&helper, 9, 5, 2).await?;
    for (x, z) in [(2, 1), (2, 3), (1, 2)] {
        helper
            .execute_command(
                &BlockPos::new(x, 1, z),
                "summon minecraft:endermite ~ ~ ~ {PersistenceRequired:1b}",
            )
            .await?;
    }
    helper
        .execute_command(
            &BlockPos::new(7, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    let pen_min = BlockPos::new(0, 0, 0);
    let pen_max = BlockPos::new(9, 4, 5);
    helper
        .start_sequence()
        .then_execute_after(5, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 1),
                    "damage @e[type=minecraft:endermite,limit=1,sort=nearest] 1 minecraft:mob_attack by @e[type=minecraft:iron_golem,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:endermite",
                    pen_min,
                    pen_max,
                    Some("minecraft:iron_golem"),
                )
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Endermite.aiStep` only counts the 2400 tick lifetime of an endermite without
/// `PersistenceRequired`.
pub(super) async fn test_persistent_endermite_never_expires(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_plank_pen(&helper, 5, 5, 2).await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:endermite ~ ~ ~ {NoAI:1b,PersistenceRequired:1b,Lifetime:2399}",
        )
        .await?;
    helper
        .start_sequence()
        .then_idle(20)
        .then_execute(|helper| async move {
            helper
                .assert_entities_present("minecraft:endermite", 1)
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Creeper.tick` sends `GameEvent.PRIME_FUSE` when it starts to swell, which a sculk
/// sensor picks up.
/// `forceload add` and `forceload remove` for the chunks around the sensor.
const FORCELOAD_AROUND_SENSOR: (&str, &str) = (
    "forceload add ~-16 ~-16 ~16 ~16",
    "forceload remove ~-16 ~-16 ~16 ~16",
);

pub(super) async fn test_priming_creeper_sets_off_sculk_sensor(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_plank_pen(&helper, 9, 5, 2).await?;
    let sensor = BlockPos::new(6, 1, 2);
    // A sculk sensor only takes a vibration while the chunks around it are loaded
    // (`VibrationSystem.Ticker`), and a test can sit at the edge of the loaded area.
    helper
        .execute_command(&sensor, FORCELOAD_AROUND_SENSOR.0)
        .await?;
    // No AI, so the creeper makes no step vibrations; a long fuse, so the test ends before it
    // blows up.
    helper
        .execute_command(
            &BlockPos::new(3, 1, 2),
            "summon minecraft:creeper ~ ~ ~ {NoAI:1b,PersistenceRequired:1b,Fuse:400s}",
        )
        .await?;
    helper
        .start_sequence()
        .then_execute_after(20, move |helper| async move {
            helper.set_block_default(&sensor, &Block::SCULK_SENSOR).await
        })
        .then_execute_after(10, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(3, 1, 2),
                    "data merge entity @e[type=minecraft:creeper,limit=1,sort=nearest] {ignited:1b}",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            let state = helper.block_state_id(&sensor).await;
            let phase = SculkSensorLikeProperties::from_state_id(state)
                .sculk_sensor_phase;
            if phase == SculkSensorPhase::Active {
                Ok(())
            } else {
                Err(GameTestError::Assertion {
                    tick: helper.tick(),
                    position: None,
                    message: format!("expected an active sculk sensor, found {phase:?}"),
                })
            }
        })
        .then_execute(move |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(3, 1, 2),
                    "kill @e[type=minecraft:creeper]",
                )
                .await?;
            helper
                .execute_command(&sensor, FORCELOAD_AROUND_SENSOR.1)
                .await
                .map(|_| ())
        })
        .then_succeed();
    Ok(())
}
