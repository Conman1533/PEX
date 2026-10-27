use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{
        Entity, EntityBase, experience_orb::ExperienceOrbEntity, item::ItemEntity,
        mob::bat::BatEntity, passive::pig::PigEntity, player::Player,
        projectile::arrow::ArrowEntity,
    },
    plugin::{
        BoxFuture, EventHandler,
        api::events::{EventPriority, entity::entity_damage_by_entity::EntityDamageByEntityEvent},
    },
    server::Server,
    world::World,
};
use pumpkin_data::{entity::EntityType, item::Item, item_stack::ItemStack, translation};
use pumpkin_util::{GameMode, Hand, math::vector3::Vector3};

use super::entity_interaction_range::sync;
use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// The other bot `login_and_movement` logs in, which the spectate test watches.
const SECOND_BOT: &str = "TestBot2";

/// Hotbar slot the pick item test keeps a pig spawn egg in.
const EGG_SLOT: u8 = 5;

/// A pig this many blocks east of the bot, in this game mode, is in reach or not. The eyes are
/// 1.62 above the feet and a pig is 0.9 wide and tall. Survival allows 3 + 3 blocks from the eyes
/// to the pig's box, creative 5 + 3. Pumpkin used to allow 8 blocks from eye to eye, so it picked
/// from the survival pig 7 blocks away (box 6.59 away) and refused the creative one 8.3 away
/// (box 7.88 away).
type Case = (GameMode, f64, bool);

const PICK_CASES: [Case; 4] = [
    (GameMode::Survival, 4.5, true),
    (GameMode::Survival, 7.0, false),
    (GameMode::Creative, 8.3, true),
    (GameMode::Creative, 9.0, false),
];

/// Vanilla `ServerGamePacketListenerImpl.handleAttack`, `handlePickItemFromEntity` and
/// `handleSpectatorAction` checks on the entity a packet names.
pub async fn run_entity_target_packet_tests(
    server: &Arc<Server>,
    addr: SocketAddr,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the entity target packet tests")?;
    let original_mode = player.gamemode.load();
    let original_position = player.position();

    let start = Instant::now();
    let mut result = attack_removed_entity(server, bot, &player)
        .await
        .map_err(|err| format!("Attack on a removed entity: {err}"));
    if result.is_ok() {
        result = attack_checks(server, bot, &player)
            .await
            .map_err(|err| format!("Attack checks: {err}"));
    }
    let attack_time = start.elapsed();

    let pick_start = Instant::now();
    if result.is_ok() {
        result = pick_item(server, bot, &player)
            .await
            .map_err(|err| format!("Pick item from entity: {err}"));
    }
    let pick_time = pick_start.elapsed();

    let spectate_start = Instant::now();
    if result.is_ok() {
        result = spectate(server, bot, &player)
            .await
            .map_err(|err| format!("Spectate entity: {err}"));
    }
    let spectate_time = spectate_start.elapsed();

    player.set_gamemode(original_mode);
    player.reset_camera();
    player.request_teleport(original_position, player.get_entity().yaw.load(), 0.0);
    result?;
    sync(server, bot, &player).await?;

    let invalid_start = Instant::now();
    attack_invalid_targets(server, addr)
        .await
        .map_err(|err| format!("Attack on an invalid entity: {err}"))?;

    summary_lines.push(format!(
        "[PASS] bot:attack_target_checks ({}ms)",
        attack_time.as_millis()
    ));
    summary_lines.push(format!(
        "[PASS] bot:attack_invalid_entity_disconnects ({}ms)",
        invalid_start.elapsed().as_millis()
    ));
    summary_lines.push(format!(
        "[PASS] bot:pick_item_from_entity_range ({}ms)",
        pick_time.as_millis()
    ));
    summary_lines.push(format!(
        "[PASS] bot:spectate_entity_checks ({}ms)",
        spectate_time.as_millis()
    ));
    Ok(())
}

/// Counts the `EntityDamageByEntityEvent`s one entity causes, which fire only for hits that land.
struct DamageEvents {
    damager: i32,
    count: AtomicUsize,
}

impl EventHandler<EntityDamageByEntityEvent> for DamageEvents {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut EntityDamageByEntityEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.damager_id == self.damager {
                self.count.fetch_add(1, Ordering::Relaxed);
            }
        })
    }
}

fn spawn_pig(world: &Arc<World>, position: Vector3<f64>) -> Result<Arc<PigEntity>, String> {
    let pig = PigEntity::new(Entity::new(world.clone(), position, &EntityType::PIG));
    pig.mob_entity.set_no_ai(true);
    pig.get_entity().set_has_no_gravity(true);
    if world.spawn_entity(pig.clone()) {
        Ok(pig)
    } else {
        Err("could not spawn a pig".to_string())
    }
}

/// A bot used to be kicked with `invalid_entity_attacked` when it hit a mob the server had just
/// removed. Vanilla `getEntityOrPart` finds nothing and `handleAttack` does nothing.
async fn attack_removed_entity(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    player.set_gamemode(GameMode::Survival);
    let world = player.world();
    // A bat has no loot to leave behind.
    let bat = BatEntity::new(Entity::new(
        world.clone(),
        player.position() + Vector3::new(2.0, 0.0, 0.0),
        &EntityType::BAT,
    ));
    bat.mob_entity.set_no_ai(true);
    bat.get_entity().set_has_no_gravity(true);
    if !world.spawn_entity(bat.clone()) {
        return Err("could not spawn the bat".to_string());
    }
    let bat_id = bat.get_entity().entity_id;
    bat.kill(bat.as_ref());
    let mut removed = false;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        bot.poll_packets().await?;
        if world.get_entity_by_id(bat_id).is_none() {
            removed = true;
            break;
        }
    }
    if !removed {
        return Err("the killed bat was never removed".to_string());
    }
    bot.attack_entity(bat_id).await?;
    sync(server, bot, player).await
}

/// Punches a fresh pig 2 blocks away per case; only an accepted attack damages it. Spectators don't
/// attack, and piercing weapons (spears) attack through the `STAB` player action instead. The
/// damage events tell the bot's hits apart from suffocation, since the world's seed is random.
async fn attack_checks(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    let cases = [
        (GameMode::Spectator, &Item::AIR, false),
        (GameMode::Survival, &Item::IRON_SPEAR, false),
        (GameMode::Survival, &Item::AIR, true),
    ];
    let world = player.world();
    let events = Arc::new(DamageEvents {
        damager: player.entity_id(),
        count: AtomicUsize::new(0),
    });
    server
        .plugin_manager
        .register::<EntityDamageByEntityEvent, _>(events.clone(), EventPriority::Normal, true);
    for (mode, item, accepted) in cases {
        player.set_gamemode(mode);
        player
            .inventory()
            .set_stack_in_hand(Hand::Right, ItemStack::new(1, item));
        // Full attack strength, so an accepted hit always does damage.
        player.last_attacked_ticks.store(100, Ordering::Relaxed);
        let pig = spawn_pig(&world, player.position() + Vector3::new(2.0, 0.0, 0.0))?;
        let hits = events.count.load(Ordering::Relaxed);
        bot.attack_entity(pig.get_entity().entity_id).await?;
        let synced = sync(server, bot, player).await;
        let hurt = events.count.load(Ordering::Relaxed) > hits;
        pig.get_entity().remove();
        synced?;
        if hurt != accepted {
            return Err(format!(
                "{mode:?} attack holding {} hurt the pig: {hurt}, expected {accepted}",
                item.registry_key
            ));
        }
    }
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::EMPTY.clone());
    Ok(())
}

/// Middle-clicks a pig with a pig spawn egg in the hotbar; an accepted pick selects the egg.
async fn pick_item(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    let world = player.world();
    let inventory = player.inventory();
    inventory
        .main_inventory
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)[usize::from(EGG_SLOT)] =
        ItemStack::new(1, &Item::PIG_SPAWN_EGG);
    let mut result = Ok(());
    for (mode, distance, accepted) in PICK_CASES {
        player.set_gamemode(mode);
        inventory.set_selected_slot(0);
        let pig = spawn_pig(&world, player.position() + Vector3::new(distance, 0.0, 0.0))?;
        bot.pick_item_from_entity(pig.get_entity().entity_id)
            .await?;
        let synced = sync(server, bot, player).await;
        pig.get_entity().remove();
        if let Err(err) = synced {
            result = Err(err);
            break;
        }
        let picked = inventory.get_selected_slot() == EGG_SLOT;
        if picked != accepted {
            result = Err(format!(
                "{mode:?} pick on a pig {distance} blocks away selected the egg: {picked}, \
                 expected {accepted}"
            ));
            break;
        }
    }
    inventory
        .main_inventory
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)[usize::from(EGG_SLOT)] =
        ItemStack::EMPTY.clone();
    inventory.set_selected_slot(0);
    result
}

/// Spectator left-clicks: vanilla `handleSpectatorAction` sets the camera only on a pickable target
/// within entity interaction range whose block is inside the world border. Spectators have the
/// survival range, 3 + 3 blocks.
async fn spectate(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    player.set_gamemode(GameMode::Spectator);
    let world = player.world();

    let pig = spawn_pig(&world, player.position() + Vector3::new(2.0, 0.0, 0.0))?;
    check_spectate(
        server,
        bot,
        player,
        pig.as_ref(),
        true,
        "a pig 2 blocks away",
    )
    .await?;
    pig.get_entity()
        .set_pos(player.position() + Vector3::new(8.0, 0.0, 0.0));
    check_spectate(
        server,
        bot,
        player,
        pig.as_ref(),
        false,
        "a pig 8 blocks away",
    )
    .await?;

    // Vanilla checks the target's block against the border, not the player. Move the border's
    // east edge onto the pig's block, which puts the block just outside.
    pig.get_entity()
        .set_pos(player.position() + Vector3::new(2.0, 0.0, 0.0));
    let pig_block_x = (player.position().x + 2.0).floor();
    let center_x = {
        let mut border = world
            .worldborder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let center_x = border.center_x;
        border.center_x = pig_block_x - border.new_diameter / 2.0;
        center_x
    };
    let result = check_spectate(
        server,
        bot,
        player,
        pig.as_ref(),
        false,
        "a pig outside the world border",
    )
    .await;
    world
        .worldborder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .center_x = center_x;
    pig.get_entity().remove();
    result?;

    // Items aren't pickable (`Entity.isPickable`), players are unless they spectate.
    let item = Arc::new(ItemEntity::new_with_velocity(
        Entity::new(
            world.clone(),
            player.position() + Vector3::new(2.0, 0.0, 0.0),
            &EntityType::ITEM,
        ),
        ItemStack::new(1, &Item::STONE),
        Vector3::new(0.0, 0.0, 0.0),
        u8::MAX,
    ));
    item.get_entity().set_has_no_gravity(true);
    if !world.spawn_entity(item.clone()) {
        return Err("could not spawn the item".to_string());
    }
    let result = check_spectate(server, bot, player, item.as_ref(), false, "an item").await;
    item.get_entity().remove();
    result?;

    let other = server
        .get_player_by_name(SECOND_BOT)
        .ok_or("no second player to spectate")?;
    let other_position = other.position();
    let other_mode = other.gamemode.load();
    other.set_gamemode(GameMode::Survival);
    other
        .get_entity()
        .set_pos(player.position() + Vector3::new(2.0, 0.0, 0.0));
    let result = check_spectate(
        server,
        bot,
        player,
        other.as_ref(),
        true,
        "a survival player 2 blocks away",
    )
    .await;
    other.get_entity().set_pos(other_position);
    other.set_gamemode(other_mode);
    result
}

async fn check_spectate(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    target: &dyn EntityBase,
    accepted: bool,
    what: &str,
) -> Result<(), String> {
    player.reset_camera();
    let target_id = target.get_entity().entity_id;
    bot.spectate_entity(target_id).await?;
    sync(server, bot, player).await?;
    let spectating = player.get_camera_entity_id() == target_id;
    player.reset_camera();
    if spectating != accepted {
        return Err(format!(
            "spectating {what}: {spectating}, expected {accepted}"
        ));
    }
    Ok(())
}

/// Vanilla `handleAttack` disconnects a player who attacks an item, an experience orb or an arrow
/// that isn't attackable. Each case uses its own bot, since the server drops it.
async fn attack_invalid_targets(server: &Arc<Server>, addr: SocketAddr) -> Result<(), String> {
    for (name, entity_type) in [
        ("AttackItemBot", &EntityType::ITEM),
        ("AttackOrbBot", &EntityType::EXPERIENCE_ORB),
        ("AttackArrowBot", &EntityType::ARROW),
    ] {
        let mut bot = HeadlessBot::connect_and_join(addr, name).await?;
        let mut player = None;
        for _ in 0..MAX_POLL_TICKS {
            server.tick();
            tokio::time::sleep(Duration::from_millis(5)).await;
            bot.poll_packets().await?;
            player = server.get_player_by_name(name);
            if player
                .as_ref()
                .is_some_and(|player| player.has_client_loaded())
            {
                break;
            }
        }
        let player = player.ok_or_else(|| format!("{name} did not join"))?;
        player.set_gamemode(GameMode::Survival);
        sync(server, &mut bot, &player).await?;

        let world = player.world();
        let entity = Entity::new(
            world.clone(),
            player.position() + Vector3::new(2.5, 0.0, 0.0),
            entity_type,
        );
        entity.set_has_no_gravity(true);
        let target: Arc<dyn EntityBase> = if entity_type == &EntityType::ITEM {
            Arc::new(ItemEntity::new_with_velocity(
                entity,
                ItemStack::new(1, &Item::STONE),
                Vector3::new(0.0, 0.0, 0.0),
                u8::MAX,
            ))
        } else if entity_type == &EntityType::EXPERIENCE_ORB {
            Arc::new(ExperienceOrbEntity::new(entity, 1))
        } else {
            Arc::new(ArrowEntity::new(entity, None))
        };
        if !world.spawn_entity(target.clone()) {
            return Err(format!("could not spawn the {}", entity_type.resource_name));
        }
        bot.attack_entity(target.get_entity().entity_id).await?;
        // The packet reaches the server before the first tick, so the orb hasn't moved yet.
        tokio::time::sleep(Duration::from_millis(20)).await;
        for _ in 0..MAX_POLL_TICKS {
            server.tick();
            tokio::time::sleep(Duration::from_millis(5)).await;
            if bot.poll_packets().await.is_err() {
                break;
            }
        }
        target.get_entity().remove();
        for _ in 0..MAX_POLL_TICKS {
            if server.get_player_by_name(name).is_none() {
                break;
            }
            server.tick();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let key = translation::java::MULTIPLAYER_DISCONNECT_INVALID_ENTITY_ATTACKED.as_bytes();
        let kicked = bot
            .disconnect_reason
            .as_ref()
            .is_some_and(|reason| reason.windows(key.len()).any(|window| window == key));
        if !kicked {
            return Err(format!(
                "attacking the {} did not disconnect {name} with invalid_entity_attacked",
                entity_type.resource_name
            ));
        }
    }
    Ok(())
}
