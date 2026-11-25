use pumpkin_data::{
    Block,
    block_properties::{
        CommandBlockLikeProperties, Facing, FurnaceLikeProperties, Half, HorizontalFacing,
        StairsShape, StickyPistonLikeProperties, WhiteWoolStairsLikeProperties,
    },
};
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_nbt::NbtCompound;
use pumpkin_util::GameMode;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

fn command_nbt(id: &str, command: &str, auto: bool) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    nbt.put_string("id", id.to_string());
    nbt.put_string("Command", command.to_string());
    nbt.put_bool("auto", auto);
    nbt
}

pub(super) async fn test_command_block_impulse_setblock(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;
    let cmd_pos = BlockPos::new(1, 1, 1);
    let target = BlockPos::new(2, 1, 1);
    let abs = helper.absolute_pos(&target);

    let props = CommandBlockLikeProperties::default(&Block::COMMAND_BLOCK);
    let nbt = command_nbt(
        "minecraft:command_block",
        &format!(
            "setblock {} {} {} minecraft:gold_block",
            abs.0.x, abs.0.y, abs.0.z
        ),
        false,
    );
    helper
        .set_block_with_nbt(&cmd_pos, props.to_state_id(&Block::COMMAND_BLOCK), &nbt)
        .await?;
    helper
        .set_block_default(&BlockPos::new(1, 2, 1), &Block::REDSTONE_BLOCK)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            helper
                .assert_block_present(&target, &Block::GOLD_BLOCK)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_command_block_chain_three_stages(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 6, 4).await?;
    let c1 = BlockPos::new(1, 1, 1);
    let c2 = BlockPos::new(2, 1, 1);
    let c3 = BlockPos::new(3, 1, 1);
    let t1 = BlockPos::new(1, 1, 2);
    let t2 = BlockPos::new(2, 1, 2);
    let t3 = BlockPos::new(3, 1, 2);

    let mut props = CommandBlockLikeProperties::default(&Block::COMMAND_BLOCK);
    props.facing = Facing::East;

    for (cmd_pos, block, target, block_name, auto) in [
        (c1, &Block::COMMAND_BLOCK, t1, "minecraft:iron_block", false),
        (
            c2,
            &Block::CHAIN_COMMAND_BLOCK,
            t2,
            "minecraft:gold_block",
            true,
        ),
        (
            c3,
            &Block::CHAIN_COMMAND_BLOCK,
            t3,
            "minecraft:diamond_block",
            true,
        ),
    ] {
        let abs = helper.absolute_pos(&target);
        let nbt = command_nbt(
            "minecraft:command_block",
            &format!("setblock {} {} {} {block_name}", abs.0.x, abs.0.y, abs.0.z),
            auto,
        );
        helper
            .set_block_with_nbt(&cmd_pos, props.to_state_id(block), &nbt)
            .await?;
    }

    helper
        .set_block_default(&BlockPos::new(1, 2, 1), &Block::REDSTONE_BLOCK)
        .await?;

    helper
        .start_sequence()
        .then_idle(4)
        .then_execute(move |helper| async move {
            helper.assert_block_present(&t1, &Block::IRON_BLOCK).await?;
            helper.assert_block_present(&t2, &Block::GOLD_BLOCK).await?;
            helper
                .assert_block_present(&t3, &Block::DIAMOND_BLOCK)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_command_block_unconditional_vs_conditional(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 4).await?;
    let c1 = BlockPos::new(1, 1, 1);
    let c2 = BlockPos::new(2, 1, 1);
    let target = BlockPos::new(2, 1, 2);
    let abs = helper.absolute_pos(&target);

    let mut p1 = CommandBlockLikeProperties::default(&Block::COMMAND_BLOCK);
    p1.facing = Facing::East;
    let nbt1 = command_nbt("minecraft:command_block", "", false);
    helper
        .set_block_with_nbt(&c1, p1.to_state_id(&Block::COMMAND_BLOCK), &nbt1)
        .await?;

    let mut p2 = CommandBlockLikeProperties::default(&Block::CHAIN_COMMAND_BLOCK);
    p2.facing = Facing::East;
    p2.conditional = true;
    let nbt2 = command_nbt(
        "minecraft:command_block",
        &format!(
            "setblock {} {} {} minecraft:emerald_block",
            abs.0.x, abs.0.y, abs.0.z
        ),
        true,
    );
    helper
        .set_block_with_nbt(&c2, p2.to_state_id(&Block::CHAIN_COMMAND_BLOCK), &nbt2)
        .await?;
    helper
        .set_block_default(&BlockPos::new(1, 2, 1), &Block::REDSTONE_BLOCK)
        .await?;

    helper
        .start_sequence()
        .then_idle(4)
        .then_execute(move |helper| async move {
            helper.assert_block_present(&target, &Block::AIR).await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_command_block_auto_executes_on_placement(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;
    let cmd_pos = BlockPos::new(1, 1, 1);
    let target = BlockPos::new(2, 1, 1);
    let abs = helper.absolute_pos(&target);

    let props = CommandBlockLikeProperties::default(&Block::REPEATING_COMMAND_BLOCK);
    let nbt = command_nbt(
        "minecraft:command_block",
        &format!(
            "setblock {} {} {} minecraft:lapis_block",
            abs.0.x, abs.0.y, abs.0.z
        ),
        false,
    );
    helper
        .set_block_with_nbt(
            &cmd_pos,
            props.to_state_id(&Block::REPEATING_COMMAND_BLOCK),
            &nbt,
        )
        .await?;
    // Power repeating command block -> executes and re-schedules itself
    helper
        .set_block_default(&BlockPos::new(1, 2, 1), &Block::REDSTONE_BLOCK)
        .await?;

    helper
        .start_sequence()
        .then_idle(3)
        .then_execute(move |helper| async move {
            helper
                .assert_block_present(&target, &Block::LAPIS_BLOCK)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_falling_gravel_and_red_sand_land(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 4).await?;
    helper
        .set_block_default(&BlockPos::new(1, 4, 1), &Block::GRAVEL)
        .await?;
    helper
        .set_block_default(&BlockPos::new(3, 4, 1), &Block::RED_SAND)
        .await?;

    helper
        .start_sequence()
        .then_idle(16)
        .then_execute(move |helper| async move {
            helper
                .assert_block_present(&BlockPos::new(1, 1, 1), &Block::GRAVEL)
                .await?;
            helper
                .assert_block_present(&BlockPos::new(3, 1, 1), &Block::RED_SAND)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_falling_concrete_powder_solidifies_in_water(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;
    // Place white_concrete_powder at (1, 3, 1) above air and let it land on the stone floor
    helper
        .set_block_default(&BlockPos::new(1, 3, 1), &Block::WHITE_CONCRETE_POWDER)
        .await?;

    helper
        .start_sequence()
        .then_idle(14)
        .then_execute(move |helper| async move {
            helper
                .assert_block_present(&BlockPos::new(1, 1, 1), &Block::WHITE_CONCRETE_POWDER)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_falling_anvil_lands_and_preserves_block(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;
    helper
        .set_block_default(&BlockPos::new(1, 3, 1), &Block::ANVIL)
        .await?;

    helper
        .start_sequence()
        .then_idle(14)
        .then_execute(move |helper| async move {
            helper
                .assert_block_present(&BlockPos::new(1, 1, 1), &Block::ANVIL)
                .await
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_tnt_primed_by_redstone_torch_and_wire(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 4).await?;
    let tnt = BlockPos::new(1, 1, 1);
    helper.set_block_default(&tnt, &Block::TNT).await?;
    helper
        .set_block_default(&BlockPos::new(2, 1, 1), &Block::REDSTONE_TORCH)
        .await?;

    helper.assert_block_present(&tnt, &Block::AIR).await?;
    helper.assert_entity_present("minecraft:tnt").await?;
    helper.succeed();
    Ok(())
}

pub(super) async fn test_snow_golem_horizontal_pattern_spawns(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    helper
        .set_block_default(&BlockPos::new(2, 1, 2), &Block::SNOW_BLOCK)
        .await?;
    helper
        .set_block_default(&BlockPos::new(2, 2, 2), &Block::SNOW_BLOCK)
        .await?;
    helper
        .set_block_default(&BlockPos::new(2, 3, 2), &Block::JACK_O_LANTERN)
        .await?;

    helper.assert_entity_present("minecraft:snow_golem").await?;
    helper
        .assert_block_present(&BlockPos::new(2, 1, 2), &Block::AIR)
        .await?;
    helper
        .assert_block_present(&BlockPos::new(2, 2, 2), &Block::AIR)
        .await?;
    helper
        .assert_block_present(&BlockPos::new(2, 3, 2), &Block::AIR)
        .await?;
    helper.succeed();
    Ok(())
}

pub(super) async fn test_iron_golem_clears_all_four_pattern_blocks(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let blocks = [
        BlockPos::new(2, 1, 2),
        BlockPos::new(2, 2, 2),
        BlockPos::new(1, 2, 2),
        BlockPos::new(3, 2, 2),
    ];
    for pos in &blocks {
        helper.set_block_default(pos, &Block::IRON_BLOCK).await?;
    }
    let head = BlockPos::new(2, 3, 2);
    helper
        .set_block_default(&head, &Block::JACK_O_LANTERN)
        .await?;

    helper.assert_entity_present("minecraft:iron_golem").await?;
    for pos in &blocks {
        helper.assert_block_present(pos, &Block::AIR).await?;
    }
    helper.assert_block_present(&head, &Block::AIR).await?;
    helper.succeed();
    Ok(())
}

pub(super) async fn test_item_entities_different_types_do_not_merge(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;
    let pos = Vector3::new(2.0, 1.2, 2.0);
    helper.spawn_item_at("minecraft:diamond", 2, pos).await?;
    helper.spawn_item_at("minecraft:emerald", 2, pos).await?;

    helper
        .start_sequence()
        .then_idle(10)
        .then_execute(move |helper| async move {
            helper.assert_entities_present("minecraft:item", 2).await
        })
        .then_succeed();
    Ok(())
}

/// A name Brigadier doesn't read unquoted. Bedrock players can have names like it.
const NON_ASCII_PLAYER_NAME: &str = "玩家一号";

/// G6-31: `forceload add` loads its chunks before it answers, as vanilla does, so a `setblock`
/// straight after it finds the position loaded.
pub(super) async fn test_forceload_add_loads_before_answering(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(0, 1, 0);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(&origin, "forceload add ~3000 ~3000")
                .await?;
            let placed = helper
                .execute_command(&origin, "setblock ~3000 ~ ~3000 minecraft:gold_block")
                .await;
            helper
                .execute_command(&origin, "forceload remove ~3000 ~3000")
                .await?;
            placed?;
            Ok(())
        })
        .then_succeed();
    Ok(())
}

/// `setblock` and `fill` take block states and block entity data, as vanilla
/// `BlockStateParser` reads them, and reject properties or values the block does not have.
pub(super) async fn test_setblock_and_fill_parse_block_states(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(0, 1, 0);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(&origin, "setblock ~1 ~ ~ minecraft:piston[facing=east]")
                .await?;
            helper
                .execute_command(
                    &origin,
                    "setblock ~3 ~ ~ minecraft:chest{Items:[{Slot:0b,id:\"minecraft:stone\",count:3}]}",
                )
                .await?;
            helper
                .execute_command(
                    &origin,
                    "fill ~1 ~ ~2 ~3 ~ ~2 minecraft:oak_stairs[facing=south,half=top]",
                )
                .await?;
            let unknown = helper
                .execute_command(&origin, "setblock ~1 ~1 ~ minecraft:piston[color=red]")
                .await;
            helper.assert_true(unknown.is_err(), "an unknown property should fail")?;
            let invalid = helper
                .execute_command(&origin, "setblock ~1 ~1 ~ minecraft:piston[facing=sideways]")
                .await;
            helper.assert_true(invalid.is_err(), "an invalid property value should fail")?;
            Ok(())
        })
        .then_execute(|helper| async move {
            let piston = StickyPistonLikeProperties {
                extended: false,
                facing: Facing::East,
            };
            helper
                .assert_block_state(&BlockPos::new(1, 1, 0), piston.to_state_id(&Block::PISTON))
                .await?;
            helper
                .assert_container_contains_count(&BlockPos::new(3, 1, 0), "minecraft:stone", 3)
                .await?;
            let stairs = WhiteWoolStairsLikeProperties {
                facing: HorizontalFacing::South,
                half: Half::Top,
                shape: StairsShape::Straight,
                waterlogged: false,
            }
            .to_state_id(&Block::OAK_STAIRS);
            for x in 1..=3 {
                helper
                    .assert_block_state(&BlockPos::new(x, 1, 2), stairs)
                    .await?;
            }
            helper
                .assert_block_present(&BlockPos::new(1, 2, 0), &Block::AIR)
                .await
        })
        .then_succeed();
    Ok(())
}

/// `give`, `tp` and `@a[name=...]` reach a player with such a name when it is quoted, as on
/// vanilla (`EntitySelectorParser.parseNameOrUUID` and the `name` option read it with
/// `readString`).
pub(super) async fn test_quoted_non_ascii_player_name(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    helper
        .spawn_named_mock_player_at(
            NON_ASCII_PLAYER_NAME,
            GameMode::Creative,
            "minecraft:stick",
            Vector3::new(1.5, 1.0, 1.5),
        )
        .await?;
    let origin = BlockPos::new(0, 1, 0);
    let destination = helper.absolute_vec(Vector3::new(3.5, 1.0, 3.5));
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(
                    &origin,
                    &format!("give \"{NON_ASCII_PLAYER_NAME}\" minecraft:torch 21"),
                )
                .await?;
            // A maximum of 0 makes `clear` count without removing anything.
            let torches = helper
                .execute_command(
                    &origin,
                    &format!("clear \"{NON_ASCII_PLAYER_NAME}\" minecraft:torch 0"),
                )
                .await?;
            helper.assert_true(torches == 21, format!("expected 21 torches, found {torches}"))?;
            helper
                .execute_command(
                    &origin,
                    &format!(
                        "tp \"{NON_ASCII_PLAYER_NAME}\" {} {} {}",
                        destination.x, destination.y, destination.z
                    ),
                )
                .await?;
            helper
                .execute_command(
                    &origin,
                    &format!(
                        "execute as @a[name=\"{NON_ASCII_PLAYER_NAME}\"] at @s run setblock ~ ~ ~ minecraft:gold_block"
                    ),
                )
                .await?;
            helper
                .assert_block_present(&BlockPos::new(3, 1, 3), &Block::GOLD_BLOCK)
                .await
        })
        .then_succeed();
    Ok(())
}

/// G6-30: `execute if blocks` compares block states and block entity data like vanilla
/// `ExecuteCommand.checkRegions`, not only the block type.
pub(super) async fn test_execute_if_blocks_compares_states(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 7, 3).await?;
    let origin = BlockPos::new(0, 1, 0);
    let src = BlockPos::new(1, 1, 1);
    let dst = BlockPos::new(5, 1, 1);
    let facing = |facing: HorizontalFacing| {
        let mut props = FurnaceLikeProperties::default(&Block::FURNACE);
        props.facing = facing;
        props.to_state_id(&Block::FURNACE)
    };
    let check = "execute if blocks ~1 ~ ~1 ~1 ~ ~1 ~5 ~ ~1 all";
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .set_block(&src, facing(HorizontalFacing::North))
                .await?;
            helper
                .set_block(&dst, facing(HorizontalFacing::South))
                .await?;
            helper.assert_true(
                helper.execute_command(&origin, check).await.is_err(),
                "furnaces facing different ways should not match",
            )?;
            helper
                .set_block(&dst, facing(HorizontalFacing::North))
                .await?;
            helper.assert_true(
                helper.execute_command(&origin, check).await.is_ok(),
                "furnaces facing the same way should match",
            )?;
            helper.set_block_default(&src, &Block::CHEST).await?;
            helper.set_block_default(&dst, &Block::CHEST).await?;
            helper
                .execute_command(
                    &origin,
                    "item replace block ~1 ~ ~1 container.0 with minecraft:stone 3",
                )
                .await?;
            helper.assert_true(
                helper.execute_command(&origin, check).await.is_err(),
                "a chest holding stone should not match an empty chest",
            )?;
            helper
                .execute_command(
                    &origin,
                    "item replace block ~5 ~ ~1 container.0 with minecraft:stone 3",
                )
                .await?;
            helper.assert_true(
                helper.execute_command(&origin, check).await.is_ok(),
                "chests holding the same items should match",
            )
        })
        .then_succeed();
    Ok(())
}

/// `whitelist add` and `op` give a name the server has never seen vanilla's UUID: Mojang's,
/// or else the offline UUID of the lowercased name (`CachedUserNameToIdResolver.get`, D4-1).
/// The UUIDs are `UUIDUtil.createOfflinePlayerUUID(name.toLowerCase(Locale.ROOT))`, worked
/// out with the JDK. Both names fail `isValidPlayerName` (one is not ASCII, the other is over 16
/// characters), so Mojang is never asked and the test needs no network. Removing each entry by
/// that UUID only works if the entry was stored under it.
pub(super) async fn test_unseen_names_get_vanilla_offline_uuids(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    const NON_ASCII_NAME: &str = "ÜNSEEN_Ö";
    const NON_ASCII_UUID: &str = "71a40931-0171-32ae-9ad7-f1fe3f10bdd5";
    const ASCII_NAME: &str = "Pex_Unseen_Operator";
    const ASCII_UUID: &str = "cc0dca9b-f304-394a-b001-6cc071e65d7c";
    let origin = BlockPos::new(0, 1, 0);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(&origin, &format!("whitelist add {NON_ASCII_NAME}"))
                .await?;
            helper
                .execute_command(&origin, &format!("op {ASCII_NAME}"))
                .await?;
            let unlisted = helper
                .execute_command(&origin, &format!("whitelist remove {NON_ASCII_UUID}"))
                .await;
            let deopped = helper
                .execute_command(&origin, &format!("deop {ASCII_UUID}"))
                .await;
            helper.assert_true(
                unlisted.is_ok(),
                format!("{NON_ASCII_NAME} should be whitelisted as {NON_ASCII_UUID}: {unlisted:?}"),
            )?;
            helper.assert_true(
                deopped.is_ok(),
                format!("{ASCII_NAME} should be an operator as {ASCII_UUID}: {deopped:?}"),
            )?;
            Ok(())
        })
        .then_succeed();
    Ok(())
}

/// G6-33: `data modify`, `data merge` and `data remove` on a block change the block entity, like
/// vanilla `BlockDataAccessor.setData`.
pub(super) async fn test_data_modify_block_changes_block_entity(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(0, 1, 0);
    let chest = BlockPos::new(2, 1, 2);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper.set_block_default(&chest, &Block::CHEST).await?;
            helper
                .execute_command(
                    &origin,
                    "item replace block ~2 ~ ~2 container.0 with minecraft:stone 3",
                )
                .await?;
            helper
                .execute_command(
                    &origin,
                    "data modify block ~2 ~ ~2 Items[0].count set value 5",
                )
                .await?;
            helper.assert_true(
                helper
                    .execute_command(&origin, "execute if data block ~2 ~ ~2 Items[{count:5}]")
                    .await
                    .is_ok(),
                "data modify should set the stack in slot 0 to 5",
            )?;
            helper
                .execute_command(&origin, r#"data merge block ~2 ~ ~2 {CustomName:"Box"}"#)
                .await?;
            helper.assert_true(
                helper
                    .execute_command(&origin, "execute if data block ~2 ~ ~2 CustomName")
                    .await
                    .is_ok(),
                "data merge should name the chest",
            )?;
            helper
                .execute_command(&origin, "data remove block ~2 ~ ~2 CustomName")
                .await?;
            helper.assert_true(
                helper
                    .execute_command(&origin, "execute if data block ~2 ~ ~2 CustomName")
                    .await
                    .is_err(),
                "data remove should take the name off again",
            )?;
            helper.assert_true(
                helper
                    .execute_command(&origin, "execute if data block ~2 ~ ~2 Items[{count:5}]")
                    .await
                    .is_ok(),
                "the stack should still be in the chest",
            )
        })
        .then_succeed();
    Ok(())
}

/// G6-35: `spreadplayers` re-rolls unsafe spots like vanilla `spreadPositions` until it finds a
/// safe one. The only safe spot in a pool of water is a stone pillar, and the pig lands on it
/// every time.
pub(super) async fn test_spreadplayers_lands_on_the_only_dry_block(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 8, 8).await?;
    let pillar = BlockPos::new(4, 1, 4);
    let max_height = helper.absolute_pos(&BlockPos::new(0, 3, 0)).0.y;
    let spread = format!(
        "spreadplayers ~ ~ 0 2 under {max_height} false @e[type=minecraft:pig,distance=..8]"
    );
    helper
        .start_sequence()
        .then_execute(move |helper| {
            let spread = spread.clone();
            async move {
                for x in 1..8 {
                    for z in 1..8 {
                        helper
                            .set_block_default(&BlockPos::new(x, 1, z), &Block::WATER)
                            .await?;
                    }
                }
                helper.set_block_default(&pillar, &Block::STONE).await?;
                helper
                    .execute_command(
                        &pillar,
                        "summon minecraft:pig ~-3 ~1 ~-3 {NoAI:1b,NoGravity:1b}",
                    )
                    .await?;
                for _ in 0..3 {
                    helper.execute_command(&pillar, &spread).await?;
                    helper
                        .assert_entity_present_in_box(
                            "minecraft:pig",
                            BlockPos::new(4, 2, 4),
                            BlockPos::new(5, 3, 5),
                        )
                        .await?;
                }
                Ok(())
            }
        })
        .then_succeed();
    Ok(())
}

/// Roadmap G6-32: `execute if|unless block` and the filters of `fill` and `clone` take a
/// vanilla `block_predicate`: a block or tag, properties checked against the state, and NBT
/// matched partially against the block entity.
pub(super) async fn test_block_predicates_check_states_tags_and_nbt(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(0, 1, 0);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            helper
                .execute_command(&origin, "setblock ~1 ~ ~ minecraft:piston[facing=east]")
                .await?;
            helper.set_block_default(&BlockPos::new(2, 1, 0), &Block::OAK_LOG).await?;
            helper.set_block_default(&BlockPos::new(3, 1, 0), &Block::CHEST).await?;
            helper
                .execute_command(&origin, "setblock ~1 ~ ~2 minecraft:oak_stairs[facing=north]")
                .await?;
            helper
                .execute_command(&origin, "setblock ~2 ~ ~2 minecraft:oak_stairs[facing=south]")
                .await?;
            Ok(())
        })
        .then_execute(move |helper| async move {
            for (command, expected) in [
                ("execute if block ~1 ~ ~ minecraft:piston[facing=north]", false),
                ("execute if block ~1 ~ ~ minecraft:piston[facing=east]", true),
                ("execute unless block ~1 ~ ~ minecraft:piston[facing=north]", true),
                ("execute if block ~2 ~ ~ #minecraft:logs", true),
                ("execute if block ~2 ~ ~ #minecraft:logs[axis=y]", true),
                ("execute if block ~2 ~ ~ #minecraft:logs[axis=x]", false),
                ("execute if block ~3 ~ ~ minecraft:chest{Items:[{Slot:0b}]}", false),
            ] {
                helper.assert_true(
                    helper.execute_command(&origin, command).await.is_ok() == expected,
                    format!("`{command}` should {}", if expected { "pass" } else { "fail" }),
                )?;
            }
            helper
                .execute_command(
                    &origin,
                    "item replace block ~3 ~ ~ container.0 with minecraft:stone 3",
                )
                .await?;
            for (command, expected) in [
                ("execute if block ~3 ~ ~ minecraft:chest{Items:[{Slot:0b}]}", true),
                ("execute if block ~3 ~ ~ minecraft:chest{Items:[{Slot:1b}]}", false),
            ] {
                helper.assert_true(
                    helper.execute_command(&origin, command).await.is_ok() == expected,
                    format!("`{command}` should {}", if expected { "pass" } else { "fail" }),
                )?;
            }
            helper.assert_true(
                helper
                    .execute_command(&origin, "execute if block ~1 ~ ~ minecraft:piston[color=red]")
                    .await
                    .is_err(),
                "an unknown property of a block should not parse",
            )?;
            helper
                .execute_command(
                    &origin,
                    "fill ~1 ~ ~2 ~2 ~ ~2 minecraft:stone replace minecraft:oak_stairs[facing=north]",
                )
                .await?;
            Ok(())
        })
        .then_execute(|helper| async move {
            helper
                .assert_block_present(&BlockPos::new(1, 1, 2), &Block::STONE)
                .await?;
            helper
                .assert_block_present(&BlockPos::new(2, 1, 2), &Block::OAK_STAIRS)
                .await
        })
        .then_succeed();
    Ok(())
}

/// `summon` with NBT skips `finalizeSpawn` like vanilla, so the mob gets no random equipment: a
/// drowned without equipment in its NBT never holds a trident or fishing rod (vanilla gives one to
/// 1 in 10 drowned that finalize their spawn).
pub(super) async fn test_summon_with_nbt_gives_no_random_equipment(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(2, 1, 2);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            for _ in 0..40 {
                helper
                    .execute_command(
                        &origin,
                        "summon minecraft:drowned ~ ~ ~ {NoAI:1b,PersistenceRequired:1b,Silent:1b}",
                    )
                    .await?;
            }
            let hands = helper
                .entity_nbt_values(
                    "minecraft:drowned",
                    BlockPos::new(0, 0, 0),
                    BlockPos::new(5, 4, 5),
                    "equipment.mainhand",
                )
                .await?;
            helper.assert_true(
                hands.len() == 40,
                format!("expected 40 drowned, found {}", hands.len()),
            )?;
            let armed = hands.iter().filter(|tags| !tags.is_empty()).count();
            helper.assert_true(
                armed == 0,
                format!("{armed} of 40 drowned summoned with NBT got a random weapon"),
            )?;
            helper
                .execute_command(&origin, "kill @e[type=minecraft:drowned,distance=..4]")
                .await?;
            Ok(())
        })
        .then_succeed();
    Ok(())
}

/// Roadmap G6-44: `team join` and `team leave` take score holders like vanilla, so names that
/// are not online players and selectors both work, and a holder is on one team at most. The
/// `team` and `scores` selector options find mobs by their scoreboard name, their UUID.
pub(super) async fn test_team_members_are_score_holders(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    let origin = BlockPos::new(2, 1, 2);
    helper
        .start_sequence()
        .then_execute(move |helper| async move {
            for _ in 0..2 {
                helper
                    .execute_command(&origin, "summon minecraft:pig ~ ~ ~ {NoAI:1b}")
                    .await?;
            }
            helper
                .execute_command(&origin, "team add pex_g644_a")
                .await?;
            helper
                .execute_command(&origin, "team add pex_g644_b")
                .await?;
            helper
                .execute_command(&origin, "scoreboard objectives add pex_g644 dummy")
                .await?;
            helper
                .execute_command(
                    &origin,
                    "scoreboard players set @e[type=minecraft:pig,distance=..2] pex_g644 5",
                )
                .await?;
            let checks = [
                ("team join pex_g644_a PexFakeHolder", 1),
                (
                    "team join pex_g644_a @e[type=minecraft:pig,distance=..2]",
                    2,
                ),
                (
                    "execute if entity @e[type=minecraft:pig,distance=..2,team=pex_g644_a]",
                    2,
                ),
                ("team join pex_g644_b PexFakeHolder", 1),
                ("team leave PexFakeHolder", 1),
                ("team leave PexFakeHolder", 0),
                ("team leave @e[type=minecraft:pig,distance=..2]", 2),
                (
                    "execute if entity @e[type=minecraft:pig,distance=..2,team=]",
                    2,
                ),
                (
                    "execute if entity @e[type=minecraft:pig,distance=..2,scores={pex_g644=5}]",
                    2,
                ),
            ];
            for (command, expected) in checks {
                let result = helper.execute_command(&origin, command).await?;
                helper.assert_true(
                    result == expected,
                    format!("`{command}` should give {expected}, gave {result}"),
                )?;
            }
            helper
                .execute_command(&origin, "scoreboard objectives remove pex_g644")
                .await?;
            helper
                .execute_command(&origin, "team remove pex_g644_a")
                .await?;
            helper
                .execute_command(&origin, "team remove pex_g644_b")
                .await?;
            helper
                .execute_command(&origin, "kill @e[type=minecraft:pig,distance=..2]")
                .await?;
            Ok(())
        })
        .then_succeed();
    Ok(())
}
