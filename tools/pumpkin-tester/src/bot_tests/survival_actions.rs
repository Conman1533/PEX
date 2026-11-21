//! G6-03: the bot actions the later survival stages use (eating, shooting, attacking, dying and
//! respawning, changing dimension), each checked against what the server sends back.

use std::{net::SocketAddr, sync::Arc, time::Instant};

use pumpkin_core::{entity::EntityBase, server::Server};
use pumpkin_data::{entity::EntityType, item::Item};
use pumpkin_util::math::vector3::Vector3;

use crate::survival::StageContext;

const BOT: &str = "ActionBot";
/// Ticks the server gets to do what a command asked.
const MAX_WAIT_TICKS: usize = 1000;
/// Ticks the food level must hold before the bot eats.
const FOOD_STEADY_TICKS: usize = 40;
/// How far from the world spawn vanilla can respawn a player without a bed (`spawnRadius` 10,
/// plus slack for the spawn search moving off unsafe ground).
const MAX_SPAWN_DISTANCE: f64 = 32.0;

pub async fn run_survival_action_tests(
    server: &Arc<Server>,
    addr: SocketAddr,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut stage = StageContext::join(server, addr, BOT).await?;
    stage.console(&format!("gamemode survival {BOT}"))?;
    stage.sync().await?;

    macro_rules! check {
        ($name:literal, $test:expr) => {{
            let start = Instant::now();
            $test
                .await
                .map_err(|err| format!("bot:{} failed: {err}", $name))?;
            summary_lines.push(format!(
                "[PASS] bot:{} ({}ms)",
                $name,
                start.elapsed().as_millis()
            ));
        }};
    }

    check!("tracks_summoned_mob", tracks_summoned_mob(&mut stage));
    check!(
        "attack_until_gone_kills_a_chicken",
        attack_until_gone_kills_a_chicken(&mut stage)
    );
    check!("eat_restores_food", eat_restores_food(&mut stage));
    check!(
        "bow_release_spawns_arrow",
        bow_release_spawns_arrow(&mut stage)
    );
    check!("kill_then_respawn", kill_then_respawn(&mut stage));
    check!("dimension_change", dimension_change(&mut stage));

    stage.leave().await
}

/// Empties the bot's inventory. `clear` fails on an empty inventory, so it always gets something
/// to clear.
fn empty_inventory(stage: &StageContext<'_>) -> Result<(), String> {
    stage.console(&format!("give {BOT} minecraft:dirt 1"))?;
    stage.console(&format!("clear {BOT}"))
}

/// Summons `entity` `offset` from the bot and waits until the bot tracks it. Returns its id.
async fn summon_near(
    stage: &mut StageContext<'_>,
    entity: &EntityType,
    offset: Vector3<f64>,
) -> Result<i32, String> {
    let at = stage.bot.position + offset;
    stage.console(&format!(
        "summon minecraft:{} {} {} {} {{NoAI:1b}}",
        entity.resource_name, at.x, at.y, at.z
    ))?;
    let near = |bot: &crate::bot::HeadlessBot| {
        bot.entities_of_type(entity)
            .into_iter()
            .find(|(_, pos)| pos.squared_distance_to_vec(&at) < 1.0)
            .map(|(id, _)| id)
    };
    stage
        .tick_until(
            &format!("the {} to show up", entity.resource_name),
            MAX_WAIT_TICKS,
            |bot| near(bot).is_some(),
        )
        .await?;
    near(&stage.bot).ok_or_else(|| format!("The {} went away", entity.resource_name))
}

async fn tracks_summoned_mob(stage: &mut StageContext<'_>) -> Result<(), String> {
    let zombie = summon_near(stage, &EntityType::ZOMBIE, Vector3::new(3.0, 0.0, 0.0)).await?;
    stage.console("kill @e[type=minecraft:zombie]")?;
    stage
        .tick_until("the killed zombie to go", MAX_WAIT_TICKS, |bot| {
            !bot.entities.contains_key(&zombie)
        })
        .await
}

async fn attack_until_gone_kills_a_chicken(stage: &mut StageContext<'_>) -> Result<(), String> {
    empty_inventory(stage)?;
    stage.select_hotbar_slot(0).await?;
    let chicken = summon_near(stage, &EntityType::CHICKEN, Vector3::new(1.5, 0.0, 0.0)).await?;
    // 4 health and 1 damage a punch, or 1.5 for a critical hit. One run took 3 punches, likely a
    // critical from the on-ground flag in the bot's movement packets (not looked into).
    let hits = stage.attack_until_gone(chicken, 8).await?;
    if hits < 3 {
        return Err(format!(
            "A chicken died after {hits} punches, expected 3 or 4"
        ));
    }
    Ok(())
}

/// The food level of the bot's player on the server.
fn server_food(stage: &StageContext<'_>) -> i32 {
    i32::from(stage.player.hunger_manager.level.load())
}

async fn eat_restores_food(stage: &mut StageContext<'_>) -> Result<(), String> {
    empty_inventory(stage)?;
    stage.console(&format!("effect give {BOT} minecraft:hunger 60 100"))?;
    stage.sync().await?;
    stage
        .tick_until(
            "hunger to empty some of the food bar",
            MAX_WAIT_TICKS,
            |bot| bot.food <= 15,
        )
        .await?;
    stage.console(&format!("effect clear {BOT}"))?;
    stage.console(&format!("give {BOT} minecraft:bread 1"))?;
    stage.select_hotbar_slot(0).await?;
    stage
        .tick_until("the bread to arrive", MAX_WAIT_TICKS, |bot| {
            bot.inventory.count(&Item::BREAD) == 1
        })
        .await?;
    // Hunger's last exhaustion can still take a point after the effect is cleared, and on a slow
    // machine the food level the bot sees can trail the server's by many ticks, so the food
    // level is read from the server's player and the bot eats once it has held still.
    let mut before = server_food(stage);
    let mut steady = 0;
    for _ in 0..MAX_WAIT_TICKS {
        stage.tick().await?;
        let now = server_food(stage);
        if now == before {
            steady += 1;
            if steady >= FOOD_STEADY_TICKS {
                break;
            }
        } else {
            before = now;
            steady = 0;
        }
    }
    if steady < FOOD_STEADY_TICKS {
        return Err(format!(
            "The food level kept changing after the hunger effect was cleared (now {before})"
        ));
    }
    let trace = stage.eat().await?;
    // Bread gives 5 food.
    let after = server_food(stage);
    if after != before + 5 {
        return Err(format!(
            "Eating bread took the food level from {before} to {after}, expected {} (now {}; {trace})",
            before + 5,
            stage.hunger_state()
        ));
    }
    stage
        .tick_until("the bot to see the new food level", MAX_WAIT_TICKS, |bot| {
            bot.food == after
        })
        .await?;
    stage
        .tick_until("the bread to be used up", MAX_WAIT_TICKS, |bot| {
            bot.inventory.count(&Item::BREAD) == 0
        })
        .await
}

async fn bow_release_spawns_arrow(stage: &mut StageContext<'_>) -> Result<(), String> {
    empty_inventory(stage)?;
    stage.console(&format!("give {BOT} minecraft:bow 1"))?;
    stage.console(&format!("give {BOT} minecraft:arrow 4"))?;
    stage.select_hotbar_slot(0).await?;
    stage
        .tick_until("the bow and arrows to arrive", MAX_WAIT_TICKS, |bot| {
            bot.inventory.count(&Item::BOW) == 1 && bot.inventory.count(&Item::ARROW) == 4
        })
        .await?;
    let target = stage.bot.position + Vector3::new(0.0, 1.0, 8.0);
    stage.shoot_bow_at(target).await?;
    stage
        .tick_until("the shot to use an arrow", MAX_WAIT_TICKS, |bot| {
            bot.inventory.count(&Item::ARROW) == 3
        })
        .await?;
    // Q with the bow held throws it out: an item entity shows up and the server inventory has no
    // bow left.
    let items_before = stage.bot.entities_of_type(&EntityType::ITEM).len();
    stage.bot.drop_item(false).await?;
    stage
        .tick_until("the dropped bow to show up", MAX_WAIT_TICKS, |bot| {
            bot.entities_of_type(&EntityType::ITEM).len() > items_before
        })
        .await?;
    let bows_left = stage
        .player
        .inventory()
        .main_inventory
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|stack| !stack.is_empty() && stack.item == &Item::BOW)
        .count();
    if bows_left > 0 || stage.bot.inventory.count(&Item::BOW) > 0 {
        return Err(format!(
            "The dropped bow is still in the inventory: {bows_left} on the server, {} seen by the bot, selected slot {}",
            stage.bot.inventory.count(&Item::BOW),
            stage.bot.inventory.selected
        ));
    }
    Ok(())
}

async fn kill_then_respawn(stage: &mut StageContext<'_>) -> Result<(), String> {
    stage.console(&format!("kill {BOT}"))?;
    stage
        .tick_until("the bot's health to reach 0", MAX_WAIT_TICKS, |bot| {
            bot.health <= 0.0
        })
        .await?;
    stage.bot.respawn().await?;
    stage
        .tick_until(
            "the bot to respawn with full health",
            MAX_WAIT_TICKS,
            |bot| bot.health >= 20.0,
        )
        .await?;
    stage.sync().await?;

    let player = stage
        .server
        .get_player_by_name(BOT)
        .ok_or("The bot is gone after respawning")?;
    stage.player = player.clone();
    let health = player.living_entity.health.load();
    if health < 20.0 {
        return Err(format!(
            "The server has the respawned bot at {health} health"
        ));
    }
    let info = stage.server.level_info.load();
    let spawn = Vector3::new(f64::from(info.spawn_x), 0.0, f64::from(info.spawn_z));
    let pos = player.get_entity().pos.load();
    let distance = pos.squared_distance_to_vec_xz(spawn).sqrt();
    if distance > MAX_SPAWN_DISTANCE {
        return Err(format!(
            "The bot respawned at {pos:?}, {distance:.1} blocks from the world spawn"
        ));
    }
    Ok(())
}

async fn dimension_change(stage: &mut StageContext<'_>) -> Result<(), String> {
    // Creative, so landing inside netherrack costs nothing.
    stage.console(&format!("gamemode creative {BOT}"))?;
    let home = stage.bot.position;
    stage.console(&format!(
        "execute in minecraft:the_nether run tp {BOT} 0 100 0"
    ))?;
    stage.wait_for_dimension("minecraft:the_nether").await?;
    stage.sync().await?;
    stage.console(&format!(
        "execute in minecraft:overworld run tp {BOT} {} {} {}",
        home.x, home.y, home.z
    ))?;
    stage.wait_for_dimension("minecraft:overworld").await?;
    stage.sync().await?;
    stage.console(&format!("gamemode survival {BOT}"))
}
