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
    // and TemptGoal walks it back. Only a panic run leaves the lane. A panic lasts 100 to 120
    // ticks and runs to random spots, some inside the lane, so the test allows the whole panic.
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
    build_pen_wall(&helper, 9, 5).await?;
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
    build_pen_wall(&helper, 13, 5).await?;
    helper
        .spawn_mock_player_at("minecraft:cactus", Vector3::new(11.5, 1.0, 2.5))
        .await?;
    helper
        // Clear of the wall: a camel is 1.7 blocks wide.
        .spawn_entity_at("minecraft:camel", Vector3::new(2.5, 1.0, 2.5))
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

/// A stone wall two blocks high around the edge of a `width` by `depth` floor, which keeps a
/// mob's random strolls inside the box a test looks for it in.
async fn build_pen_wall(helper: &GameTestHelper, width: i32, depth: i32) -> GameTestResult<()> {
    for x in 0..width {
        for z in 0..depth {
            if x == 0 || x == width - 1 || z == 0 || z == depth - 1 {
                for y in 1..3 {
                    helper
                        .set_block_default(&BlockPos::new(x, y, z), &Block::STONE)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// A creature walking toward a player on an `11x5` stone floor: tempted by `held_item`, it stops
/// 2.5 blocks short of the player, inside the box near the player.
async fn follows_tempting_item(
    helper: GameTestHelper,
    entity_name: &'static str,
    held_item: &str,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 11, 5).await?;
    helper
        .spawn_mock_player_at(held_item, Vector3::new(9.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at(entity_name, Vector3::new(1.5, 1.0, 2.5))
        .await?;
    let near_player_min = BlockPos::new(6, 1, 2);
    let near_player_max = BlockPos::new(10, 3, 3);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_present_in_box(entity_name, near_player_min, near_player_max)
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Chicken.registerGoals`: `TemptGoal` (speed 1.0, `#chicken_food`) walks a chicken to a
/// player holding seeds.
pub(super) async fn test_chicken_follows_seeds(helper: GameTestHelper) -> GameTestResult<()> {
    follows_tempting_item(helper, "minecraft:chicken", "minecraft:wheat_seeds").await
}

/// Vanilla `Pig.registerGoals`: a `TemptGoal` (speed 1.2) for a carrot on a stick, next to the one
/// for `#pig_food`, walks a pig to a player holding one.
pub(super) async fn test_pig_follows_carrot_on_a_stick(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    follows_tempting_item(helper, "minecraft:pig", "minecraft:carrot_on_a_stick").await
}

/// Vanilla `Animal.mobInteract`: wheat puts two adult mooshrooms in love, and `BreedGoal` walks
/// them together until they have a calf.
pub(super) async fn test_mooshrooms_breed_when_fed_wheat(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 9, 5).await?;
    build_pen_wall(&helper, 9, 5).await?;
    helper
        .spawn_mock_player_at("minecraft:wheat", Vector3::new(4.5, 1.0, 2.5))
        .await?;
    for x in [2.0, 7.0] {
        helper
            .spawn_entity_at("minecraft:mooshroom", Vector3::new(x, 1.0, 2.5))
            .await?;
    }
    let area_min = BlockPos::new(0, 1, 0);
    let area_max = BlockPos::new(8, 3, 4);
    helper
        .start_sequence()
        .then_execute_after(1, move |helper| async move {
            let fed = helper
                .mock_player_interact_with_entities("minecraft:mooshroom", area_min, area_max)
                .await?;
            helper.assert_true(fed == 2, format!("{fed} of 2 mooshrooms took the wheat"))
        })
        .then_wait_until(move |helper| async move {
            let ages = helper
                .entity_nbt_values("minecraft:mooshroom", area_min, area_max, "Age")
                .await?;
            let calves = ages
                .iter()
                .flatten()
                .filter(|age| matches!(age, NbtTag::Int(age) if *age < 0))
                .count();
            helper.assert_true(
                ages.len() == 3 && calves == 1,
                format!("no calf yet, mooshroom ages {ages:?}"),
            )
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `EatBlockGoal` and `Sheep.ate`: a sheared sheep on grass eats it, which turns the grass
/// to dirt and grows the wool back. Four sheep make the wait for the 1 in 1000 roll short.
pub(super) async fn test_sheep_eats_grass_and_regrows_wool(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    for x in 0..7 {
        for z in 0..7 {
            helper
                .set_block_default(&BlockPos::new(x, 0, z), &Block::GRASS_BLOCK)
                .await?;
        }
    }
    build_pen_wall(&helper, 7, 7).await?;
    for (x, z) in [(2, 2), (4, 2), (2, 4), (4, 4)] {
        helper
            .execute_command(
                &BlockPos::new(x, 1, z),
                "summon minecraft:sheep ~0.5 ~ ~0.5 {Sheared:1b,Color:0b}",
            )
            .await?;
    }
    let area_min = BlockPos::new(0, 1, 0);
    let area_max = BlockPos::new(6, 3, 6);
    helper
        .start_sequence()
        .then_wait_until(move |helper| async move {
            let sheared = helper
                .entity_nbt_values("minecraft:sheep", area_min, area_max, "Sheared")
                .await?;
            helper.assert_true(
                sheared.len() == 4 && sheared.iter().flatten().any(|tag| *tag == NbtTag::Byte(0)),
                format!("no sheep has its wool back, Sheared {sheared:?}"),
            )?;
            let mut eaten = 0;
            for x in 1..6 {
                for z in 1..6 {
                    if helper.block_state_id(&BlockPos::new(x, 0, z)).await
                        == Block::DIRT.default_state.id
                    {
                        eaten += 1;
                    }
                }
            }
            helper.assert_true(eaten > 0, "a sheep grew wool without eating grass")
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Pig.thunderHit`: outside peaceful, lightning turns a pig into a persistent zombified
/// piglin.
pub(super) async fn test_pig_struck_by_lightning_becomes_zombified_piglin(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    helper
        .spawn_entity_at("minecraft:pig", Vector3::new(2.5, 1.0, 2.5))
        .await?;
    let area_min = BlockPos::new(0, 1, 0);
    let area_max = BlockPos::new(4, 4, 4);
    helper
        .start_sequence()
        .then_execute_after(1, |helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 2),
                    "summon minecraft:lightning_bolt ~0.5 ~ ~0.5",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            helper
                .assert_entity_not_present_in_box("minecraft:pig", area_min, area_max)
                .await?;
            helper
                .assert_entity_nbt(
                    "minecraft:zombified_piglin",
                    area_min,
                    area_max,
                    "PersistenceRequired",
                    &NbtTag::Byte(1),
                )
                .await
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `MushroomCow.thunderHit`: a bolt turns a red mooshroom brown, once.
pub(super) async fn test_mooshroom_struck_by_lightning_turns_brown(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:mooshroom ~0.5 ~ ~0.5 {Type:\"red\",NoAI:1b}",
        )
        .await?;
    let area_min = BlockPos::new(0, 1, 0);
    let area_max = BlockPos::new(4, 4, 4);
    let type_is = move |helper: GameTestHelper, variant: &'static str| async move {
        helper
            .assert_entity_nbt(
                "minecraft:mooshroom",
                area_min,
                area_max,
                "Type",
                &NbtTag::String(variant.into()),
            )
            .await
    };
    helper
        .start_sequence()
        .then_execute_after(1, move |helper| async move {
            type_is(helper.clone(), "red").await?;
            helper
                .execute_command(
                    &BlockPos::new(2, 1, 2),
                    "summon minecraft:lightning_bolt ~0.5 ~ ~0.5",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| type_is(helper, "brown"))
        // The bolt keeps striking for a few ticks; it must not swap the colour back.
        .then_execute_after(20, move |helper| type_is(helper, "brown"))
        .then_succeed();
    Ok(())
}
