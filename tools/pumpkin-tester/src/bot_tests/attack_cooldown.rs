use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, Ordering},
    },
    time::{Duration, Instant},
};

use pumpkin_core::{
    command::CommandSender,
    entity::{Entity, EntityBase, passive::pig::PigEntity, player::Player},
    plugin::{
        BoxFuture, EventHandler,
        api::events::{EventPriority, entity::entity_damage_by_entity::EntityDamageByEntityEvent},
    },
    server::Server,
};
use pumpkin_data::{entity::EntityType, item_stack::ItemStack};
use pumpkin_util::{GameMode, Hand, math::vector3::Vector3};

use crate::bot::HeadlessBot;

/// An empty hand hits with the player's base `attack_damage`.
const BASE_DAMAGE: f32 = 1.0;

/// Records the damage the bot's attacks deal to one target.
struct Hits {
    damager: i32,
    target: AtomicI32,
    damage: Mutex<Option<f32>>,
}

impl EventHandler<EntityDamageByEntityEvent> for Hits {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut EntityDamageByEntityEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.damager_id == self.damager
                && event.entity_id == self.target.load(Ordering::Relaxed)
            {
                *self
                    .damage
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(event.damage);
            }
        })
    }
}

/// Tries for one case: an attack the server handled a tick late spoils the gap, so the case runs
/// again.
const ATTEMPTS: usize = 5;

/// Vanilla `Player.getAttackStrengthScale(0.5)` and the damage scale in `Player.attack`.
fn expected_damage(ticks: u32, attack_speed: f64) -> f32 {
    let delay = 20.0 / attack_speed;
    let scale = ((f64::from(ticks) + 0.5) / delay).clamp(0.0, 1.0) as f32;
    BASE_DAMAGE * (0.2 + scale * scale * 0.8)
}

/// Vanilla times the attack cooldown in ticks: `attackStrengthTicker` counts player ticks and
/// `getCurrentItemAttackStrengthDelay` is `20 / attack_speed` ticks, whatever the tick rate.
pub async fn run_attack_cooldown_test(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the attack cooldown test")?;
    let original_mode = player.gamemode.load();
    let original_hand = player.inventory().held_item();
    player.set_gamemode(GameMode::Survival);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::EMPTY.clone());
    // Haste, Mining Fatigue, Strength and Weakness from earlier tests would change the numbers.
    // Fails with "no effects to remove" when there are none, which is fine.
    let _ = console(server, &format!("effect clear {}", bot.name));

    let hits = Arc::new(Hits {
        damager: player.entity_id(),
        target: AtomicI32::new(-1),
        damage: Mutex::new(None),
    });
    server
        .plugin_manager
        .register::<EntityDamageByEntityEvent, _>(hits.clone(), EventPriority::Normal, true);

    let result = cases(server, bot, &player, &hits).await;

    let _ = console(
        server,
        &format!("attribute {} minecraft:attack_speed base set 4", bot.name),
    );
    let _ = console(server, "tick rate 20");
    hits.target.store(-1, Ordering::Relaxed);
    player.set_gamemode(original_mode);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, original_hand);
    result.map_err(|err| format!("Attack cooldown: {err}"))?;

    summary_lines.push(format!(
        "[PASS] bot:attack_cooldown ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

async fn cases(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    hits: &Hits,
) -> Result<(), String> {
    // (attack_speed base, ticks between the two attacks)
    for (attack_speed, gap) in [(4.0, 5), (4.0, 2), (8.0, 2), (8.0, 1)] {
        console(
            server,
            &format!(
                "attribute {} minecraft:attack_speed base set {attack_speed}",
                bot.name
            ),
        )?;
        let damage = punch_after_retrying(server, bot, player, hits, gap).await?;
        let expected = expected_damage(gap, attack_speed);
        if (damage - expected).abs() > 1e-4 {
            return Err(format!(
                "a punch {gap} ticks after the last attack with attack_speed {attack_speed} \
                 dealt {damage}, expected {expected}"
            ));
        }
    }

    // A faster tick rate must not shorten the cooldown: it counts ticks, not time.
    console(server, "tick rate 40")?;
    console(
        server,
        &format!("attribute {} minecraft:attack_speed base set 4", bot.name),
    )?;
    let damage = punch_after_retrying(server, bot, player, hits, 2).await?;
    let expected = expected_damage(2, 4.0);
    if (damage - expected).abs() > 1e-4 {
        return Err(format!(
            "at tick rate 40 a punch 2 ticks after the last attack dealt {damage}, expected {expected}"
        ));
    }
    Ok(())
}

/// [`punch_after`] until both punches were handled in the tick after they arrived.
async fn punch_after_retrying(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    hits: &Hits,
    gap: u32,
) -> Result<f32, String> {
    for _ in 0..ATTEMPTS {
        if let Some(damage) = punch_after(server, bot, player, hits, gap).await? {
            return Ok(damage);
        }
        // Let the late attack land before the next try.
        for _ in 0..40 {
            server.tick();
        }
    }
    Err(format!(
        "in {ATTEMPTS} tries the server never handled both punches {gap} ticks apart in the tick \
         after they arrived"
    ))
}

/// Punches one pig, ticks the server `gap` times counting the tick that handled that punch, then
/// punches a second pig in the next tick and returns the damage the second punch dealt. Returns
/// `None` when a punch wasn't handled in the tick after it arrived, which happens on a slow
/// machine and leaves the gap unknown.
async fn punch_after(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    hits: &Hits,
    gap: u32,
) -> Result<Option<f32>, String> {
    let world = player.world();
    let mut pigs = Vec::new();
    for side in [1.5, -1.5] {
        let pig = PigEntity::new(Entity::new(
            world.clone(),
            player.position() + Vector3::new(side, 0.0, 0.0),
            &EntityType::PIG,
        ));
        pig.mob_entity.set_no_ai(true);
        pig.get_entity().set_has_no_gravity(true);
        if !world.spawn_entity(pig.clone()) {
            return Err("could not spawn a pig".to_string());
        }
        pigs.push(pig);
    }
    let result = async {
        let first = pigs[0].get_entity().entity_id;
        let second = pigs[1].get_entity().entity_id;
        let take = || {
            hits.damage
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
        };
        hits.target.store(first, Ordering::Relaxed);
        take();
        attack_in_next_tick(server, bot, player, first).await?;
        if take().is_none() {
            return Ok(None);
        }
        for _ in 1..gap {
            server.tick();
        }
        hits.target.store(second, Ordering::Relaxed);
        take();
        attack_in_next_tick(server, bot, player, second).await?;
        Ok(take())
    }
    .await;
    for pig in pigs {
        pig.get_entity().remove();
    }
    result
}

/// Sends an attack and waits, without ticking, until the server has queued it, so the next tick
/// handles it. The tester drives the ticks itself, so the gap between two attacks is exact.
async fn attack_in_next_tick(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    player: &Arc<Player>,
    target: i32,
) -> Result<(), String> {
    bot.attack_entity(target).await?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.inbound_packets.is_empty() {
        if Instant::now() > deadline {
            return Err("the server never received the attack".to_string());
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    server.tick();
    Ok(())
}

fn console(server: &Arc<Server>, command: &str) -> Result<(), String> {
    server
        .command_dispatcher
        .load()
        .execute_input(command, &CommandSender::Console.into_source(server))
        .map(|_| ())
        .map_err(|err| format!("'{command}' failed: {}", err.message.get_text()))
}
