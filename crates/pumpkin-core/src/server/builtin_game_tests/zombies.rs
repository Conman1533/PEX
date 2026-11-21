use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

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

/// Vanilla `Zombie.addBehaviourGoals` targets every `AbstractVillager`, so a zombie villager
/// goes after a wandering trader too.
pub(super) async fn test_zombie_villager_chases_wandering_trader(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    build_pen(&helper, 9, 5).await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:zombie_villager ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            "summon minecraft:wandering_trader ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    let pen_min = BlockPos::new(0, 0, 0);
    let pen_max = BlockPos::new(9, 4, 5);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_target(
                "minecraft:zombie_villager",
                pen_min,
                pen_max,
                Some("minecraft:wandering_trader"),
            )
            .await
    })?;
    Ok(())
}

/// Vanilla `ZombifiedPiglin`: `HurtByTargetGoal.setAlertOthers()` hands the attacker to every
/// zombified piglin in follow range, so the whole pack turns on whoever hit one of them.
pub(super) async fn test_zombified_piglin_alerts_pack_when_hit(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    for (x, z) in [(2, 1), (2, 3), (1, 2)] {
        helper
            .execute_command(
                &BlockPos::new(x, 1, z),
                "summon minecraft:zombified_piglin ~ ~ ~ {PersistenceRequired:1b}",
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
    let pen_max = BlockPos::new(9, 6, 5);
    helper
        .start_sequence()
        .then_execute_after(5, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 1),
                    "damage @e[type=minecraft:zombified_piglin,limit=1,sort=nearest] 1 minecraft:mob_attack by @e[type=minecraft:iron_golem,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:zombified_piglin",
                    pen_min,
                    pen_max,
                    Some("minecraft:iron_golem"),
                )
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Drowned.DrownedTridentAttackGoal`: a drowned holding a trident throws it at a
/// player within 10 blocks. The player stands in water, so `Drowned.okTarget` accepts them by
/// day as well. The drowned starts in its own pool too: by day `DrownedGoToWaterGoal`
/// (priority 1) would otherwise walk a dry drowned to the player's pool whenever one of its
/// random probes hits that block, and restart the throw's 40-tick wait on arrival.
pub(super) async fn test_drowned_throws_trident_at_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    for pool_x in [2, 9] {
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            helper
                .set_block_default(&BlockPos::new(pool_x + dx, 1, 2 + dz), &Block::STONE)
                .await?;
        }
        helper
            .set_block_default(&BlockPos::new(pool_x, 1, 2), &Block::WATER)
            .await?;
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(9.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:drowned ~ ~ ~ {PersistenceRequired:1b,equipment:{mainhand:{id:\"minecraft:trident\",count:1}}}",
        )
        .await?;
    let area_min = BlockPos::new(0, 0, 0);
    let area_max = BlockPos::new(11, 6, 5);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box("minecraft:trident", area_min, area_max)
            .await
    })?;
    Ok(())
}

/// Vanilla `Giant` registers no goals, so a giant next to a survival player never targets
/// them.
pub(super) async fn test_giant_ignores_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 7).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(9.5, 1.0, 3.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(3, 1, 3),
            "summon minecraft:giant ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    let area_min = BlockPos::new(0, 0, 0);
    let area_max = BlockPos::new(11, 14, 7);
    helper
        .start_sequence()
        .then_idle(80)
        .then_execute(move |helper| async move {
            helper
                .assert_entity_target("minecraft:giant", area_min, area_max, None)
                .await
        })
        .then_succeed();
    Ok(())
}
