//! Using held items from Bedrock, away from blocks: eating, drinking, throwing, drawing a bow,
//! raising a shield, casting a rod, and the items used on a block that spawn something (spawn
//! eggs and buckets of fish). Each is driven with the transactions a real client sends.

use std::sync::{Arc, atomic::Ordering};

use pumpkin_core::{entity::player::Player, world::World};
use pumpkin_data::{
    block_properties::EndPortalFrameLikeProperties,
    data_component_impl::{ChargedProjectilesImpl, PotionContentsImpl, StatusEffectInstance},
    effect::StatusEffect,
    entity::EntityType,
    item::Item,
    item_stack::ItemStack,
    potion::Potion,
};
use pumpkin_protocol::bedrock::server::player_auth_input::InputData;
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use super::{
    BEDROCK_NAME, BedrockBot, BedrockTestServer, Checks, HeadlessBot,
    bot::{RELEASE_CONSUME, RELEASE_SHOOT},
    run_console, send_bedrock_flags, tick,
};

/// Ticks to wait for something a use should do at once.
const SOON: usize = 10;

pub(super) async fn run_held_item_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player: Arc<Player> = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or_else(|| format!("{BEDROCK_NAME} is not on the server"))?;
    run_console(server, &format!("gamemode survival {BEDROCK_NAME}"));
    run_console(server, &format!("effect clear {BEDROCK_NAME}"));
    let (old_yaw, old_pitch) = (bedrock.yaw, bedrock.pitch);

    run_consume_checks(server, bedrock, java, checks, &player).await?;
    run_throw_checks(server, bedrock, java, checks, &player).await?;
    run_drawn_checks(server, bedrock, java, checks, &player).await?;
    run_spyglass_horn_checks(server, bedrock, java, checks, &player).await?;
    run_shield_rod_map_checks(server, bedrock, java, checks, &player).await?;
    run_on_block_checks(server, bedrock, java, checks, &player).await?;
    run_console(server, &format!("clear {BEDROCK_NAME}"));
    run_console(server, &format!("effect clear {BEDROCK_NAME}"));
    run_console(server, "kill @e[type=minecraft:arrow]");
    run_console(server, "kill @e[type=minecraft:item]");
    (bedrock.yaw, bedrock.pitch) = (old_yaw, old_pitch);
    tick(server, bedrock, Some(&mut *java)).await?;
    Ok(())
}

/// Eating and drinking: food, a potion and milk.
async fn run_consume_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    // Food: one click-air starts eating and the server finishes it.
    let beef = hold(player, ItemStack::new(4, &Item::COOKED_BEEF));
    player.hunger_manager.level.store(10);
    bedrock.use_item_in_air(0, &beef).await?;
    let ate = wait(server, bedrock, java, 60, || {
        player.hunger_manager.level.load() > 10
    })
    .await?;
    checks.record(
        "bedrock eats cooked beef",
        if !ate {
            Err(format!(
                "food stayed at {}",
                player.hunger_manager.level.load()
            ))
        } else if held_count(player) != 3 {
            Err(format!("{} beef left instead of 3", held_count(player)))
        } else {
            Ok(())
        },
    );

    // An older client also sends `ReleaseItem` consume when its own eating animation ends. That
    // can reach the server a tick before the server's timer runs out, and must not stop the meal.
    player.hunger_manager.level.store(10);
    bedrock.use_item_in_air(0, &beef).await?;
    for _ in 0..30 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.release_item(RELEASE_CONSUME, 0, &beef).await?;
    let ate = wait(server, bedrock, java, 20, || {
        player.hunger_manager.level.load() > 10
    })
    .await?;
    checks.record(
        "bedrock consume release doesn't cancel the meal",
        if ate {
            Ok(())
        } else {
            Err(format!(
                "food stayed at {}",
                player.hunger_manager.level.load()
            ))
        },
    );

    // Drinking a potion gives its effect and leaves a glass bottle.
    let mut potion = ItemStack::new(1, &Item::POTION);
    potion.set_data_component(potion_of(&Potion::SWIFTNESS));
    let potion = hold(player, potion);
    bedrock.use_item_in_air(0, &potion).await?;
    let drank = wait(server, bedrock, java, 60, || {
        player.living_entity.has_effect(&StatusEffect::SPEED)
    })
    .await?;
    checks.record(
        "bedrock drinks a potion of swiftness",
        if !drank {
            Err("no speed effect".to_string())
        } else if held_item(player) != Item::GLASS_BOTTLE.id {
            Err("no glass bottle left".to_string())
        } else {
            Ok(())
        },
    );

    // Milk clears the effect and leaves a bucket.
    let milk = hold(player, ItemStack::new(1, &Item::MILK_BUCKET));
    bedrock.use_item_in_air(0, &milk).await?;
    let cleared = wait(server, bedrock, java, 60, || {
        !player.living_entity.has_effect(&StatusEffect::SPEED)
    })
    .await?;
    checks.record(
        "bedrock drinks milk",
        if !cleared {
            Err("still has speed".to_string())
        } else if held_item(player) != Item::BUCKET.id {
            Err("no bucket left".to_string())
        } else {
            Ok(())
        },
    );
    Ok(())
}

/// Thrown items: each one is in the world and uses up one item.
async fn run_throw_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    // Throwables, aimed up so they don't land on the bots.
    bedrock.pitch = -60.0;
    tick(server, bedrock, Some(&mut *java)).await?;
    for (name, item, entity) in [
        (
            "bedrock throws a snowball",
            &Item::SNOWBALL,
            &EntityType::SNOWBALL,
        ),
        ("bedrock throws an egg", &Item::EGG, &EntityType::EGG),
        (
            "bedrock throws a splash potion",
            &Item::SPLASH_POTION,
            &EntityType::SPLASH_POTION,
        ),
        (
            "bedrock throws a lingering potion",
            &Item::LINGERING_POTION,
            &EntityType::LINGERING_POTION,
        ),
        (
            "bedrock throws an ender pearl",
            &Item::ENDER_PEARL,
            &EntityType::ENDER_PEARL,
        ),
    ] {
        let mut stack = ItemStack::new(2, item);
        if item.id == Item::SPLASH_POTION.id || item.id == Item::LINGERING_POTION.id {
            stack.set_data_component(potion_of(&Potion::WATER));
        }
        let stack = hold(player, stack);
        let world = player.world();
        let before = count(&world, entity);
        bedrock.use_item_in_air(0, &stack).await?;
        let thrown = wait(server, bedrock, java, SOON, || {
            count(&world, entity) > before
        })
        .await?;
        checks.record(
            name,
            if !thrown {
                Err(format!("no {} in the world", entity.resource_name))
            } else if held_count(player) != 1 {
                Err(format!("{} left instead of 1", held_count(player)))
            } else {
                Ok(())
            },
        );
        // A pearl left flying would teleport the bot in the middle of a later check.
        remove_all(&world, entity);
    }
    checks.record(
        "an ender pearl throw starts its cooldown",
        if player.is_on_cooldown(Item::ENDER_PEARL.registry_key) {
            Ok(())
        } else {
            Err("no ender pearl cooldown".to_string())
        },
    );
    Ok(())
}

/// Items drawn and let go: bow, crossbow and trident.
async fn run_drawn_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    // A bow: hold use, then let go; the server shoots on the release.
    run_console(server, &format!("give {BEDROCK_NAME} arrow 4"));
    let bow = hold(player, ItemStack::new(1, &Item::BOW));
    let world = player.world();
    let arrows = count(&world, &EntityType::ARROW);
    bedrock.use_item_in_air(0, &bow).await?;
    for _ in 0..20 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.release_item(RELEASE_SHOOT, 0, &bow).await?;
    let shot = wait(server, bedrock, java, SOON, || {
        count(&world, &EntityType::ARROW) > arrows
    })
    .await?;
    checks.record(
        "bedrock draws and shoots a bow",
        if shot {
            Ok(())
        } else {
            Err("no arrow after the release".to_string())
        },
    );

    // A crossbow loads on the release after a full draw, and the next use fires it.
    run_console(server, &format!("give {BEDROCK_NAME} arrow 4"));
    let crossbow = hold(player, ItemStack::new(1, &Item::CROSSBOW));
    bedrock.use_item_in_air(0, &crossbow).await?;
    for _ in 0..30 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.release_item(RELEASE_SHOOT, 0, &crossbow).await?;
    let loaded = wait(server, bedrock, java, SOON, || {
        player
            .inventory()
            .held_item()
            .get_data_component::<ChargedProjectilesImpl>()
            .is_some_and(|charged| !charged.projectiles.is_empty())
    })
    .await?;
    let arrows = count(&world, &EntityType::ARROW);
    let loaded_crossbow = player.inventory().held_item();
    bedrock.use_item_in_air(0, &loaded_crossbow).await?;
    let fired = wait(server, bedrock, java, SOON, || {
        count(&world, &EntityType::ARROW) > arrows
    })
    .await?;
    checks.record(
        "bedrock loads and fires a crossbow",
        if !loaded {
            Err("not loaded after the draw".to_string())
        } else if !fired {
            Err("no arrow after firing".to_string())
        } else {
            Ok(())
        },
    );

    // A trident is thrown on the release after a draw.
    let trident = hold(player, ItemStack::new(1, &Item::TRIDENT));
    bedrock.use_item_in_air(0, &trident).await?;
    for _ in 0..15 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.release_item(RELEASE_SHOOT, 0, &trident).await?;
    let thrown = wait(server, bedrock, java, SOON, || {
        count(&world, &EntityType::TRIDENT) > 0
    })
    .await?;
    checks.record(
        "bedrock throws a trident",
        if thrown {
            Ok(())
        } else {
            Err("no trident in the world".to_string())
        },
    );
    run_console(server, "kill @e[type=minecraft:trident]");

    Ok(())
}

/// A spyglass held up and lowered, and a goat horn blown.
async fn run_spyglass_horn_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    // A spyglass is in use while held up; a goat horn sounds and starts its cooldown.
    let spyglass = hold(player, ItemStack::new(1, &Item::SPYGLASS));
    bedrock.use_item_in_air(0, &spyglass).await?;
    let looking = wait(server, bedrock, java, SOON, || {
        using(player, &Item::SPYGLASS)
    })
    .await?;
    bedrock.release_item(RELEASE_SHOOT, 0, &spyglass).await?;
    let lowered = wait(server, bedrock, java, SOON, || {
        !using(player, &Item::SPYGLASS)
    })
    .await?;
    checks.record(
        "bedrock looks through a spyglass",
        if !looking {
            Err("the spyglass was never in use".to_string())
        } else if !lowered {
            Err("still in use after the release".to_string())
        } else {
            Ok(())
        },
    );
    let horn = hold(player, ItemStack::new(1, &Item::GOAT_HORN));
    bedrock.use_item_in_air(0, &horn).await?;
    let sounded = wait(server, bedrock, java, SOON, || {
        player.get_cooldown(Item::GOAT_HORN.registry_key) > 0.0
    })
    .await?;
    checks.record(
        "bedrock blows a goat horn",
        if sounded {
            Ok(())
        } else {
            Err("no goat horn cooldown".to_string())
        },
    );
    player.living_entity.clear_active_hand();
    Ok(())
}

/// The shield, the fishing rod and an empty map.
async fn run_shield_rod_map_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    let world = player.world();
    // A Bedrock player blocks with a shield by sneaking with it in hand, and stops on standing up.
    bedrock.pitch = 0.0;
    hold(player, ItemStack::new(1, &Item::SHIELD));
    send_bedrock_flags(bedrock, &[InputData::StartSneaking, InputData::Sneaking]).await?;
    let blocking = wait(server, bedrock, java, SOON, || {
        player.living_entity.is_blocking()
    })
    .await?;
    send_bedrock_flags(bedrock, &[InputData::StopSneaking]).await?;
    let lowered = wait(server, bedrock, java, SOON, || {
        !player.living_entity.is_blocking()
    })
    .await?;
    checks.record(
        "bedrock blocks with a shield while sneaking",
        if !blocking {
            Err("never blocking".to_string())
        } else if !lowered {
            Err("still blocking after standing up".to_string())
        } else {
            Ok(())
        },
    );

    // A fishing rod casts a bobber, and a second use reels it in.
    let rod = hold(player, ItemStack::new(1, &Item::FISHING_ROD));
    bedrock.use_item_in_air(0, &rod).await?;
    let cast = wait(server, bedrock, java, SOON, || {
        count(&world, &EntityType::FISHING_BOBBER) > 0
    })
    .await?;
    for _ in 0..5 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.use_item_in_air(0, &rod).await?;
    let reeled = wait(server, bedrock, java, SOON, || {
        count(&world, &EntityType::FISHING_BOBBER) == 0
    })
    .await?;
    checks.record(
        "bedrock casts and reels in a fishing rod",
        if !cast {
            Err("no bobber".to_string())
        } else if !reeled {
            Err("the bobber is still out".to_string())
        } else {
            Ok(())
        },
    );

    // An empty map used in the air becomes a filled map of the area.
    let map = hold(player, ItemStack::new(1, &Item::MAP));
    bedrock.use_item_in_air(0, &map).await?;
    let filled = wait(server, bedrock, java, SOON, || {
        held_item(player) == Item::FILLED_MAP.id
    })
    .await?;
    checks.record(
        "bedrock fills an empty map",
        if filled {
            Ok(())
        } else {
            Err("still holding an empty map".to_string())
        },
    );
    Ok(())
}

/// Items used on a block: a spawn egg, a bucket of cod and an eye of ender.
async fn run_on_block_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
) -> Result<(), String> {
    let world = player.world();
    // Items used on a block that spawn a mob there.
    let feet = bedrock.position.y - EntityType::PLAYER.eye_height;
    let ground = BlockPos::floored(
        f64::from(bedrock.position.x) + 2.0,
        f64::from(feet) - 1.0,
        f64::from(bedrock.position.z),
    );
    let setblock = |pos: BlockPos, block: &str| {
        run_console(
            server,
            &format!("setblock {} {} {} {block}", pos.0.x, pos.0.y, pos.0.z),
        );
    };
    setblock(ground, "stone");
    setblock(ground.up(), "air");
    // Buckets aim with the player's view, as in vanilla: look at the top of the block.
    let aim = Vector3::new(
        f64::from(ground.0.x) + 0.5,
        f64::from(ground.0.y) + 1.0,
        f64::from(ground.0.z) + 0.5,
    ) - bedrock.position.to_f64();
    bedrock.yaw = (-aim.x.atan2(aim.z)).to_degrees() as f32;
    bedrock.pitch = (-aim.y.atan2(aim.horizontal_length())).to_degrees() as f32;
    tick(server, bedrock, Some(&mut *java)).await?;
    for (name, item, entity) in [
        (
            "bedrock spawn egg spawns a cow",
            &Item::COW_SPAWN_EGG,
            &EntityType::COW,
        ),
        (
            "bedrock bucket of cod lets out a cod",
            &Item::COD_BUCKET,
            &EntityType::COD,
        ),
    ] {
        let stack = hold(player, ItemStack::new(1, item));
        let before = count(&world, entity);
        bedrock.use_item_on_block(ground, 1, 0, &stack, &[]).await?;
        let spawned = wait(server, bedrock, java, SOON, || {
            count(&world, entity) > before
        })
        .await?;
        checks.record(
            name,
            if spawned {
                Ok(())
            } else {
                Err(format!("no {} spawned", entity.resource_name))
            },
        );
        // Removed rather than killed: a killed cow or cod drops loot the bot would pick up.
        remove_all(&world, entity);
        setblock(ground.up(), "air");
    }

    // An eye of ender goes into an end portal frame.
    setblock(ground.up(), "air");
    setblock(ground, "end_portal_frame");
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let eye = hold(player, ItemStack::new(1, &Item::ENDER_EYE));
    bedrock.use_item_on_block(ground, 1, 0, &eye, &[]).await?;
    let world_for_frame = player.world();
    let set = wait(server, bedrock, java, SOON, || {
        let state = world_for_frame.get_block_state_id(&ground);
        EndPortalFrameLikeProperties::from_state_id(state).eye
    })
    .await?;
    checks.record(
        "bedrock puts an eye of ender in an end portal frame",
        if set {
            Ok(())
        } else {
            Err("the eye wasn't used".to_string())
        },
    );
    setblock(ground, "stone");
    Ok(())
}

/// Puts `stack` in the bot's selected hotbar slot on the server and returns it, for the
/// transaction's held item.
fn hold(player: &Player, stack: ItemStack) -> ItemStack {
    player.inventory().set_selected_slot(0);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, stack.clone());
    stack
}

fn held_count(player: &Player) -> u8 {
    player.inventory().held_item().item_count
}

fn held_item(player: &Player) -> u16 {
    player.inventory().held_item().item.id
}

fn using(player: &Player, item: &Item) -> bool {
    player.living_entity.item_use_time.load(Ordering::Relaxed) > 0
        && player
            .living_entity
            .item_in_use
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|stack| stack.item.id == item.id)
}

fn potion_of(potion: &Potion) -> PotionContentsImpl {
    PotionContentsImpl {
        potion_id: Some(i32::from(potion.id)),
        custom_color: None,
        custom_effects: Vec::<StatusEffectInstance>::new(),
        custom_name: None,
    }
}

/// Removes every entity of this type at once, with no death or loot.
fn remove_all(world: &World, entity: &EntityType) {
    for found in world.entities.load().iter() {
        if found.get_entity().entity_type.id == entity.id {
            found.get_entity().remove();
        }
    }
}

fn count(world: &World, entity: &EntityType) -> usize {
    world
        .entities
        .load()
        .iter()
        .filter(|seen| seen.get_entity().entity_type.id == entity.id)
        .count()
}

/// Ticks until `done`, at most `ticks` times.
async fn wait(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    ticks: usize,
    done: impl Fn() -> bool,
) -> Result<bool, String> {
    for _ in 0..ticks {
        if done() {
            return Ok(true);
        }
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(done())
}
