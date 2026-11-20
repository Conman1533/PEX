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

/// Vanilla `Zoglin.findNearestValidAttackTarget`: a zoglin attacks any mob it can see, not only
/// players. The pen's walls are three blocks high so the zoglin's throw keeps the cow inside.
pub(super) async fn test_zoglin_attacks_any_nearby_mob(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    for x in 0..9 {
        for z in 0..5 {
            if x == 0 || x == 8 || z == 0 || z == 4 {
                for y in 1..4 {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            "summon minecraft:cow ~0.5 ~ ~0.5 {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:zoglin ~0.5 ~ ~0.5 {IsBaby:0b,PersistenceRequired:1b}",
        )
        .await?;
    helper.succeed_when(|helper| async move {
        helper.assert_entity_not_present("minecraft:cow").await
    })?;
    Ok(())
}

/// Its own name, so the `tp` below moves no other test's mock player.
const ENDERMAN_STARER: &str = "ender-starer";

/// Vanilla `EndermanLookForPlayerGoal`: a survival player looking at an enderman's eyes with
/// nothing solid in between becomes its target.
pub(super) async fn test_enderman_angers_when_stared_at(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
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
        .spawn_named_mock_player_at(
            ENDERMAN_STARER,
            GameMode::Survival,
            "minecraft:stick",
            Vector3::new(1.5, 1.0, 2.5),
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            "summon minecraft:enderman ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(0, 1, 0);
    let pen_max = BlockPos::new(9, 4, 5);
    helper.succeed_when(move |helper| async move {
        // Aim from the player's eyes at the enderman's, wherever it has wandered.
        helper
            .execute_command(
                &BlockPos::new(1, 1, 2),
                "execute anchored eyes run tp @a[name=ender-starer] ~ ~ ~ facing entity @e[type=minecraft:enderman,limit=1,sort=nearest] eyes",
            )
            .await?;
        helper
            .assert_entity_target("minecraft:enderman", pen_min, pen_max, Some("minecraft:player"))
            .await
    })?;
    Ok(())
}

/// Vanilla `ShulkerNearestAttackGoal` + `ShulkerAttackGoal`: a shulker targets a survival player
/// within its follow range and fires a bullet at them.
pub(super) async fn test_shulker_shoots_bullet_at_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(1.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(7, 1, 2),
            "summon minecraft:shulker ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper.succeed_when(|helper| async move {
        helper
            .assert_entity_present("minecraft:shulker_bullet")
            .await
    })?;
    Ok(())
}

/// Vanilla `Blaze.registerGoals`: `HurtByTargetGoal(this).setAlertOthers()`. One blaze hurt by an
/// iron golem sends the other blaze after the golem too; blazes never pick golems themselves.
pub(super) async fn test_blaze_alerts_nearby_blazes_when_hit(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    for (x, z) in [(2, 1), (2, 3)] {
        helper
            .execute_command(
                &BlockPos::new(x, 1, z),
                "summon minecraft:blaze ~ ~ ~ {PersistenceRequired:1b}",
            )
            .await?;
    }
    helper
        .execute_command(
            &BlockPos::new(6, 1, 2),
            "summon minecraft:iron_golem ~ ~ ~ {NoAI:1b,PersistenceRequired:1b}",
        )
        .await?;

    let pen_min = BlockPos::new(0, 0, 0);
    let pen_max = BlockPos::new(9, 6, 5);
    // Summoned mobs join the world on the next tick, so the hit waits a moment.
    helper
        .start_sequence()
        .then_execute_after(5, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 1),
                    "damage @e[type=minecraft:blaze,limit=1,sort=nearest] 1 minecraft:mob_attack by @e[type=minecraft:iron_golem,limit=1,sort=nearest]",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_target(
                    "minecraft:blaze",
                    pen_min,
                    pen_max,
                    Some("minecraft:iron_golem"),
                )
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `GhastShootFireballGoal`: a ghast that sees a survival player level with it charges
/// for 20 ticks and fires a large fireball at them.
pub(super) async fn test_ghast_shoots_fireball_at_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(1.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(9, 1, 2),
            "summon minecraft:ghast ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper.succeed_when(|helper| async move {
        helper.assert_entity_present("minecraft:fireball").await
    })?;
    Ok(())
}

/// Vanilla `StriderGoToLavaGoal`: a strider out of lava walks back to lava within 8 blocks.
/// The ground is two layers of stone with a lava pool sunk into the top one.
pub(super) async fn test_strider_walks_back_to_lava(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    for x in 0..9 {
        for z in 0..5 {
            let pool = (6..8).contains(&x) && (1..4).contains(&z);
            let block = if pool { &Block::LAVA } else { &Block::STONE };
            helper
                .set_block_default(&BlockPos::new(x, 1, z), block)
                .await?;
        }
    }
    helper
        .execute_command(
            &BlockPos::new(1, 2, 2),
            "summon minecraft:strider ~ ~ ~ {PersistenceRequired:1b}",
        )
        .await?;
    helper.succeed_when(|helper| async move {
        helper
            .assert_entity_present_in_box(
                "minecraft:strider",
                BlockPos::new(6, 1, 1),
                BlockPos::new(8, 3, 4),
            )
            .await
    })?;
    Ok(())
}
