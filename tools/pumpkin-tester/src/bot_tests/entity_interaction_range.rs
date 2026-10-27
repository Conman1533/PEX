use std::{
    sync::{
        Arc,
        atomic::{AtomicI32, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{
        Entity, EntityBase,
        mob::Mob,
        passive::{pig::PigEntity, tamable::TamableAnimal, wolf::WolfEntity},
        player::Player,
    },
    plugin::{
        BoxFuture, EventHandler,
        api::events::{
            EventPriority, player::player_interact_entity_event::PlayerInteractEntityEvent,
        },
    },
    server::Server,
};
use pumpkin_data::{entity::EntityType, item_stack::ItemStack};
use pumpkin_util::{GameMode, Hand, math::vector3::Vector3};

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// Where the bot's right-click hits the target, relative to the target's position.
const CLICK_LOCATION: Vector3<f64> = Vector3::new(0.0, 0.5, 0.0);

/// A target this many blocks east of the bot, in this game mode, is in reach or not. The eyes are
/// 1.62 above the feet, so a wolf 4.5 blocks away is 4.27 from them: past the 3 block
/// `entity_interaction_range` but inside vanilla's 3 block packet buffer. Creative adds 2.
type Case = (GameMode, f64, bool);

const INTERACT_CASES: [Case; 4] = [
    (GameMode::Survival, 8.0, false),
    (GameMode::Survival, 4.5, true),
    (GameMode::Survival, 2.0, true),
    (GameMode::Creative, 7.0, true),
];

const ATTACK_CASES: [Case; 3] = [
    (GameMode::Survival, 8.0, false),
    (GameMode::Survival, 4.5, true),
    (GameMode::Creative, 7.0, true),
];

/// Counts the `PlayerInteractEntityEvent`s aimed at one entity.
struct InteractEvents {
    target: AtomicI32,
    count: AtomicUsize,
}

impl EventHandler<PlayerInteractEntityEvent> for InteractEvents {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PlayerInteractEntityEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.target.get_entity().entity_id == self.target.load(Ordering::Relaxed) {
                self.count.fetch_add(1, Ordering::Relaxed);
            }
        })
    }
}

/// Vanilla `handleInteract` and `handleAttack` do nothing when the target is out of reach or its
/// block is outside the world border. A bot used to toggle a wolf from 20 blocks away.
pub async fn run_entity_interaction_range_tests(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the entity interaction range test")?;
    let original_mode = player.gamemode.load();
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::EMPTY.clone());

    let mut result = interactions(server, bot, &player)
        .await
        .map_err(|err| format!("Entity interaction range: {err}"));
    if result.is_ok() {
        result = attacks(server, bot, &player)
            .await
            .map_err(|err| format!("Attack range: {err}"));
    }
    player.set_gamemode(original_mode);
    result?;

    summary_lines.push(format!(
        "[PASS] bot:entity_interaction_range ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Right-clicks the bot's own sitting wolf, which stands up or sits down on every accepted click.
async fn interactions(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    let world = player.world();
    let wolf = WolfEntity::new(Entity::new(
        world.clone(),
        player.position(),
        &EntityType::WOLF,
    ));
    wolf.mob_entity.set_no_ai(true);
    wolf.get_entity().set_has_no_gravity(true);
    TamableAnimal::tame(wolf.as_ref(), player.gameprofile.id);
    wolf.set_ordered_to_sit(true);
    if !world.spawn_entity(wolf.clone()) {
        return Err("could not spawn the wolf".to_string());
    }
    let events = Arc::new(InteractEvents {
        target: AtomicI32::new(wolf.get_entity().entity_id),
        count: AtomicUsize::new(0),
    });
    server
        .plugin_manager
        .register::<PlayerInteractEntityEvent, _>(events.clone(), EventPriority::Normal, true);

    let mut result = Ok(());
    for case in INTERACT_CASES {
        result = check_interact(server, bot, player, &wolf, &events, case).await;
        if result.is_err() {
            break;
        }
    }
    if result.is_ok() {
        // Vanilla checks the target's block against the border, not the player. Move the border's
        // east edge onto the wolf's block, which puts the block just outside.
        let wolf_block_x = (player.position().x + 2.0).floor();
        let center_x = {
            let mut border = world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let center_x = border.center_x;
            border.center_x = wolf_block_x - border.new_diameter / 2.0;
            center_x
        };
        result = check_interact(
            server,
            bot,
            player,
            &wolf,
            &events,
            (GameMode::Survival, 2.0, false),
        )
        .await
        .map_err(|err| format!("outside the world border: {err}"));
        world
            .worldborder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .center_x = center_x;
    }
    wolf.get_entity().remove();
    result
}

async fn check_interact(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    wolf: &WolfEntity,
    events: &InteractEvents,
    (mode, distance, accepted): Case,
) -> Result<(), String> {
    player.set_gamemode(mode);
    wolf.get_entity()
        .set_pos(player.position() + Vector3::new(distance, 0.0, 0.0));
    let sitting = wolf.is_ordered_to_sit();
    let events_before = events.count.load(Ordering::Relaxed);
    bot.interact_entity(wolf.get_entity().entity_id, CLICK_LOCATION)
        .await?;
    sync(server, bot, player).await?;
    let toggled = wolf.is_ordered_to_sit() != sitting;
    let fired = events.count.load(Ordering::Relaxed) - events_before;
    if toggled != accepted || fired != usize::from(accepted) {
        return Err(format!(
            "{mode:?} right-click on a wolf {distance} blocks away toggled sitting: {toggled}, \
             interact events: {fired}; expected {accepted} and {}",
            usize::from(accepted)
        ));
    }
    Ok(())
}

/// Punches a fresh pig per case; only an accepted attack hurts it.
async fn attacks(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
) -> Result<(), String> {
    let world = player.world();
    for (mode, distance, accepted) in ATTACK_CASES {
        player.set_gamemode(mode);
        let pig = PigEntity::new(Entity::new(
            world.clone(),
            player.position() + Vector3::new(distance, 0.0, 0.0),
            &EntityType::PIG,
        ));
        pig.mob_entity.set_no_ai(true);
        pig.get_entity().set_has_no_gravity(true);
        if !world.spawn_entity(pig.clone()) {
            return Err("could not spawn the pig".to_string());
        }
        let living = &pig.get_mob_entity().living_entity;
        let health = living.health.load();
        bot.attack_entity(pig.get_entity().entity_id).await?;
        let synced = sync(server, bot, player).await;
        let hurt = living.health.load() < health;
        pig.get_entity().remove();
        synced?;
        if hurt != accepted {
            return Err(format!(
                "{mode:?} attack on a pig {distance} blocks away hurt it: {hurt}, expected {accepted}"
            ));
        }
    }
    Ok(())
}

/// Ticks until the server has handled everything the bot sent so far.
pub(super) async fn sync(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Player,
) -> Result<(), String> {
    let ping = bot.send_ping().await?;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        if player.client.closed() {
            return Err(format!("{} was disconnected", bot.name));
        }
        bot.poll_packets().await?;
        if bot.has_pong(ping) {
            return Ok(());
        }
    }
    Err("the server did not answer a ping".to_string())
}
