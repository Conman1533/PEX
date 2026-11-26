use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::GameMode;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

/// A glass tank filling the 11x5x5 structure, with water three blocks deep inside.
async fn build_water_tank(helper: &GameTestHelper) -> GameTestResult<()> {
    for x in 0..11 {
        for z in 0..5 {
            helper
                .set_block_default(&BlockPos::new(x, 0, z), &Block::GLASS)
                .await?;
            let wall = x == 0 || x == 10 || z == 0 || z == 4;
            for y in 1..4 {
                let block = if wall { &Block::GLASS } else { &Block::WATER };
                helper
                    .set_block_default(&BlockPos::new(x, y, z), block)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Vanilla `NautilusAi` idle activity: `FollowTemptation` (speed 1.3) swims a nautilus to a
/// player holding `#nautilus_food`, stopping 3.5 blocks short.
pub(super) async fn test_nautilus_follows_tempting_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_water_tank(&helper).await?;
    helper
        .spawn_mock_player_at("minecraft:cod", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:nautilus", Vector3::new(1.5, 1.5, 2.5))
        .await?;
    // Within 3.5 blocks of the player; the nautilus starts 7 blocks away.
    let near_player_min = BlockPos::new(5, 1, 1);
    let near_player_max = BlockPos::new(9, 3, 3);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box("minecraft:nautilus", near_player_min, near_player_max)
            .await
    })?;
    Ok(())
}

/// Vanilla `ZombieNautilusAi`: a survival player who hurts a zombie nautilus in the water makes it
/// angry (`AbstractNautilus.hurtServer`), `StartAttacking` picks that player, and the fight
/// activity's `ChargeAttack` (speed 0.5) dashes into it.
pub(super) async fn test_zombie_nautilus_charges_survival_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_water_tank(&helper).await?;
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(8.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:zombie_nautilus", Vector3::new(2.5, 1.5, 2.5))
        .await?;

    let tank_min = BlockPos::new(1, 1, 1);
    // The water inside the glass: blocks 1 to 9 along x and 1 to 3 up and across.
    let tank_max = BlockPos::new(10, 4, 4);
    helper
        .start_sequence()
        .then_execute(|helper| async move {
            helper
                .execute_command(
                    &BlockPos::new(8, 1, 2),
                    "damage @e[type=minecraft:zombie_nautilus,limit=1,sort=nearest] 1 \
                     minecraft:player_attack by @p",
                )
                .await
                .map(|_| ())
        })
        .then_wait_until(move |helper| async move {
            let health = helper
                .entity_nbt_values("minecraft:player", tank_min, tank_max, "Health")
                .await?;
            let hurt = health
                .iter()
                .flatten()
                .any(|tag| matches!(tag, NbtTag::Float(value) if *value < 20.0));
            helper.assert_true(
                hurt,
                format!("the zombie nautilus never hit the player who hurt it: {health:?}"),
            )
        })
        .then_succeed();
    Ok(())
}

/// Vanilla `Guardian.registerGoals` and `GuardianAttackGoal`: a guardian targets a squid more
/// than 3 blocks away (`GuardianAttackSelector`), locks its beam on for 80 ticks, then hits it
/// with magic damage plus its 6 attack damage, until the squid dies.
pub(super) async fn test_guardian_laser_kills_squid(helper: GameTestHelper) -> GameTestResult<()> {
    build_water_tank(&helper).await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:guardian ~0.5 ~ ~0.5 {PersistenceRequired:1b}",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(8, 1, 2),
            "summon minecraft:squid ~0.5 ~0.5 ~0.5 {PersistenceRequired:1b}",
        )
        .await?;

    let tank_min = BlockPos::new(0, 0, 0);
    let tank_max = BlockPos::new(10, 4, 4);
    helper.succeed_when(move |helper| async move {
        // The squid starts in the tank and can only leave it by dying.
        helper
            .assert_entity_present_in_box("minecraft:guardian", tank_min, tank_max)
            .await?;
        helper
            .assert_entity_not_present_in_box("minecraft:squid", tank_min, tank_max)
            .await
    })?;
    Ok(())
}

/// Vanilla `ElderGuardian.customServerAiStep`: every 1200 ticks, mining fatigue III for players
/// in survival or adventure within 50 blocks (`MobEffectUtil.addEffectToPlayersAround`), and
/// none for a creative player. The elder is walled in, so its beam can't reach the players.
pub(super) async fn test_elder_guardian_gives_mining_fatigue(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    // A stone floor, and a stone box at x 0..4 holding 3x3x3 of water.
    for x in 0..11 {
        for z in 0..5 {
            helper
                .set_block_default(&BlockPos::new(x, 0, z), &Block::STONE)
                .await?;
        }
    }
    for x in 0..5 {
        for y in 1..5 {
            for z in 0..5 {
                let inside = (1..4).contains(&x) && (1..4).contains(&y) && (1..4).contains(&z);
                let block = if inside { &Block::WATER } else { &Block::STONE };
                helper
                    .set_block_default(&BlockPos::new(x, y, z), block)
                    .await?;
            }
        }
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(7.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_named_mock_player_at(
            "test-creative",
            GameMode::Creative,
            "minecraft:stick",
            Vector3::new(9.5, 1.0, 2.5),
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 1, 2),
            "summon minecraft:elder_guardian ~0.5 ~ ~0.5",
        )
        .await?;

    let survival_min = BlockPos::new(7, 1, 2);
    let survival_max = BlockPos::new(8, 3, 3);
    let creative_min = BlockPos::new(9, 1, 2);
    let creative_max = BlockPos::new(10, 3, 3);
    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_nbt(
                "minecraft:player",
                survival_min,
                survival_max,
                "active_effects[].id",
                &NbtTag::String("minecraft:mining_fatigue".into()),
            )
            .await?;
        helper
            .assert_entity_nbt(
                "minecraft:player",
                survival_min,
                survival_max,
                "active_effects[].amplifier",
                &NbtTag::Byte(2),
            )
            .await?;
        // One call curses every player in range, so the creative one would have it by now.
        let creative = helper
            .entity_nbt_values(
                "minecraft:player",
                creative_min,
                creative_max,
                "active_effects",
            )
            .await?;
        helper.assert_true(
            creative.len() == 1 && creative.iter().all(Vec::is_empty),
            format!("the creative player got effects: {creative:?}"),
        )
    })?;
    Ok(())
}

/// Vanilla `Guardian`: water-bound navigation (`WaterBoundPathNavigation`), `GuardianMoveControl`
/// and no float goal, so a guardian swims anywhere in the water, down as well as up. A glass
/// tank 9 blocks long holds 10 blocks of water, split by two layers of stone with a gap one block
/// wide across the far end. The guardian starts above the stone at the near end, and a survival player stands
/// on the bottom below it, out of its sight from anywhere above the stone. The guardian has to
/// swim along and down through the gap, where it sees the player 8 blocks away and hits it with
/// its beam. A guardian that floats to the surface never gets there.
pub(super) async fn test_guardian_swims_down_to_hidden_player(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    for x in 0..11 {
        for z in 0..5 {
            helper
                .set_block_default(&BlockPos::new(x, 0, z), &Block::STONE)
                .await?;
            let wall = x == 0 || x == 10 || z == 0 || z == 4;
            for y in 1..11 {
                let block = if wall {
                    &Block::GLASS
                } else if (y == 5 || y == 6) && x != 9 {
                    &Block::STONE
                } else {
                    &Block::WATER
                };
                helper
                    .set_block_default(&BlockPos::new(x, y, z), block)
                    .await?;
            }
        }
    }
    helper
        .spawn_survival_mock_player_at("minecraft:stick", Vector3::new(1.5, 1.0, 2.5))
        .await?;
    helper
        .execute_command(
            &BlockPos::new(1, 1, 2),
            "effect give @a[distance=..2] minecraft:water_breathing 1000 0 true",
        )
        .await?;
    helper
        .execute_command(
            &BlockPos::new(2, 8, 2),
            "summon minecraft:guardian ~0.5 ~ ~0.5 {PersistenceRequired:1b}",
        )
        .await?;

    let player_min = BlockPos::new(1, 1, 2);
    let player_max = BlockPos::new(2, 3, 3);
    let tank_min = BlockPos::new(0, 0, 0);
    let tank_max = BlockPos::new(11, 12, 5);
    helper.succeed_when(move |helper| async move {
        let health = helper
            .entity_nbt_values("minecraft:player", player_min, player_max, "Health")
            .await?;
        let hurt = health
            .iter()
            .flatten()
            .any(|tag| matches!(tag, NbtTag::Float(value) if *value < 20.0));
        if hurt {
            return Ok(());
        }
        // Where the guardian is, for a failure: under the stone (y below 5) or not.
        let guardian = helper
            .entity_nbt_values("minecraft:guardian", tank_min, tank_max, "Pos")
            .await?;
        let player = helper
            .entity_nbt_values("minecraft:player", player_min, player_max, "Pos")
            .await?;
        helper.assert_true(
            false,
            format!(
                "the guardian never hit the player; guardian at {guardian:?}, player at {player:?}"
            ),
        )
    })?;
    Ok(())
}
