//! The dragon fight on servers that stop and start again: an eye of ender thrown right after
//! startup (G6-14), hits on the dragon's parts (G6-16), its death (G6-17) and the fight's saved
//! data across restarts (G6-19). Each server is the tester's usual one on a world folder of its
//! own; a restart is `Server::shutdown`, which `stop` runs, then a new `Server` on that folder.

use std::{
    sync::{Arc, PoisonError},
    time::Instant,
};

use pumpkin_core::{
    entity::{
        EntityBase,
        boss::ender_dragon::{EnderDragonEntity, phase::EnderDragonPhase},
        experience_orb::ExperienceOrbEntity,
    },
    world::World,
};
use pumpkin_data::{
    Block, dimension::Dimension, entity::EntityType, item::Item, item_stack::ItemStack,
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use crate::{harness::RunningServer, survival::StageContext};

const BOT: &str = "EndBot";
/// Where the bot waits in the End: within `ARENA_RADIUS` of the dragon's spawn, so the fight
/// ticks, and 30 blocks out from the podium, beyond the 8 blocks at which experience orbs fly to
/// a player. The bot's view distance (4) keeps the fight's 7 by 7 chunk arena loaded.
const WAIT_POS: [f64; 3] = [30.5, 128.0, 0.5];
/// Ticks for the End to generate around the bot and the fight to find or spawn its dragon.
const MAX_TICKS: usize = 3000;
/// Ticks an eye of ender gets to appear after the throw.
const EYE_TICKS: usize = 100;
/// Vanilla `EnderDragon.tickDeath` removes the dragon when `dragonDeathTime` reaches 200.
const DEATH_TICKS: usize = 200;
/// Past `MAX_TICKS_BEFORE_DRAGON_RESPAWN` (1200), after which `DragonFight.tick` replaces a dragon
/// it hasn't seen.
const RESPAWN_CHECK_TICKS: usize = 1300;
/// A diamond axe: 1 base attack damage plus 8, and no sweep to hit a second part.
const AXE_DAMAGE: f32 = 9.0;
/// Longer than the axe's 25 tick cooldown and the dragon's hurt invulnerability.
const GAP_TICKS: usize = 30;
const MAX_HEALTH: f32 = 200.0;
/// Vanilla `EnderDragon.onCrystalDestroyed` hurts the head for 10 when its healing crystal goes.
const CRYSTAL_DAMAGE: f32 = 10.0;
/// Out along x from the dragon: inside the 32 blocks around its box where it looks for a crystal
/// to heal from, and beyond the 12 blocks the crystal's explosion reaches.
const CRYSTAL_OFFSET: f64 = 30.0;
/// The health the dragon is left with when the server stops mid-fight.
const MID_FIGHT_HEALTH: f32 = 150.0;
/// Vanilla `EnderDragon.tickDeath` drops 12,000 experience on the first kill.
const FIRST_KILL_XP: u32 = 12_000;
/// Vanilla `EndPodiumFeature`: the 21 columns closer than 2.5 to the centre less the bedrock pillar.
const EXIT_PORTAL_BLOCKS: usize = 20;
/// Vanilla `EndDragonFight`: 20 gateway spots on a circle of radius 96 at y 75.
const GATEWAY_COUNT: i32 = 20;
const GATEWAY_DISTANCE: f64 = 96.0;
const GATEWAY_Y: i32 = 75;

pub async fn run_end_fight_tests(summary_lines: &mut Vec<String>) -> Result<(), String> {
    let first_world = tempfile::tempdir().map_err(|err| format!("No world folder: {err}"))?;

    // A fresh world: throw an eye, hit the dragon, then stop with the fight half done.
    let start = Instant::now();
    let running = RunningServer::start(first_world.path()).await?;
    let mut stage = StageContext::join(&running.server, running.addr, BOT).await?;
    eye_flies_right_after_startup(&mut stage)
        .await
        .map_err(|err| format!("Eye of ender after startup: {err}"))?;
    summary_lines.push(format!(
        "[PASS] bot:eye_of_ender_flies_right_after_startup ({}ms)",
        start.elapsed().as_millis()
    ));

    let start = Instant::now();
    enter_end(&mut stage).await?;
    let dragon = wait_for_dragon(&mut stage).await?;
    remove_crystals(&stage);
    dragon_damage(&mut stage, &dragon)
        .await
        .map_err(|err| format!("Dragon damage: {err}"))?;
    summary_lines.push(format!(
        "[PASS] bot:dragon_damage_through_parts ({}ms)",
        start.elapsed().as_millis()
    ));

    let start = Instant::now();
    hold(&dragon)?;
    living(&dragon)?.set_health(MID_FIGHT_HEALTH);
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
    }
    let dragon_uuid = dragon.get_entity().entity_uuid;
    stage.leave().await?;
    running.stop().await;

    let running = RunningServer::start(first_world.path()).await?;
    let mut stage = StageContext::join(&running.server, running.addr, BOT).await?;
    enter_end(&mut stage).await?;
    let dragon = fight_resumes(&mut stage, dragon_uuid)
        .await
        .map_err(|err| format!("Dragon fight after a restart mid-fight: {err}"))?;
    summary_lines.push(format!(
        "[PASS] bot:dragon_fight_resumes_after_restart ({}ms)",
        start.elapsed().as_millis()
    ));

    let start = Instant::now();
    dragon_death(&mut stage, &dragon)
        .await
        .map_err(|err| format!("Dragon death: {err}"))?;
    summary_lines.push(format!(
        "[PASS] bot:dragon_death_sequence ({}ms)",
        start.elapsed().as_millis()
    ));

    let start = Instant::now();
    let fight_before = fight_nbt(&end_world(&stage))?;
    stage.leave().await?;
    running.stop().await;
    let running = RunningServer::start(first_world.path()).await?;
    let mut stage = StageContext::join(&running.server, running.addr, BOT).await?;
    enter_end(&mut stage).await?;
    dragon_stays_dead(&mut stage, &fight_before)
        .await
        .map_err(|err| format!("Killed dragon after a restart: {err}"))?;
    stage.leave().await?;
    running.stop().await;
    summary_lines.push(format!(
        "[PASS] bot:killed_dragon_stays_dead_after_restart ({}ms)",
        start.elapsed().as_millis()
    ));

    // Another fresh world: `kill` skips the death sequence.
    let start = Instant::now();
    let second_world = tempfile::tempdir().map_err(|err| format!("No world folder: {err}"))?;
    let running = RunningServer::start(second_world.path()).await?;
    let mut stage = StageContext::join(&running.server, running.addr, BOT).await?;
    enter_end(&mut stage).await?;
    wait_for_dragon(&mut stage).await?;
    kill_opens_portal_at_once(&stage).map_err(|err| format!("Dragon kill command: {err}"))?;
    stage.leave().await?;
    running.stop().await;
    summary_lines.push(format!(
        "[PASS] bot:dragon_kill_command_opens_portal_at_once ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Vanilla `EnderEyeItem.use` waits for the stronghold ring positions (`getRingPositionsFor`
/// joins their future), so an eye thrown before they are computed still flies.
async fn eye_flies_right_after_startup(stage: &mut StageContext<'_>) -> Result<(), String> {
    hold_item(stage, &Item::ENDER_EYE);
    stage.bot.use_item(1).await?;
    let world = stage.world();
    for _ in 0..EYE_TICKS {
        stage.tick().await?;
        if world
            .entities
            .load()
            .iter()
            .any(|entity| entity.get_entity().entity_type == &EntityType::EYE_OF_ENDER)
        {
            return Ok(());
        }
    }
    Err(format!(
        "no eye of ender flew within {EYE_TICKS} ticks of the throw"
    ))
}

/// Puts one `item` in the bot's main hand and syncs the slot like an inventory change, which
/// applies the item's attribute modifiers (the axe's attack damage).
fn hold_item(stage: &StageContext<'_>, item: &'static Item) {
    let stack = ItemStack::new(1, item);
    let inventory = stage.player.inventory();
    inventory.set_stack_in_hand(Hand::Right, stack.clone());
    stage
        .player
        .sync_hand_slot(usize::from(inventory.get_selected_slot()), stack);
}

fn end_world(stage: &StageContext<'_>) -> Arc<World> {
    stage.server.get_world_from_dimension(&Dimension::THE_END)
}

/// Teleports the bot within the End. The console runs in the overworld, so a plain `tp` would
/// take the bot back there. `{:?}` keeps the decimal point, since `tp` centres a whole number on
/// its block.
async fn end_tp(stage: &mut StageContext<'_>, pos: Vector3<f64>) -> Result<(), String> {
    stage.console(&format!(
        "execute in minecraft:the_end run tp {BOT} {:?} {:?} {:?}",
        pos.x, pos.y, pos.z
    ))?;
    stage
        .tick_until(&format!("the bot to arrive at {pos:?}"), MAX_TICKS, |bot| {
            bot.position == pos
        })
        .await?;
    stage.sync().await
}

async fn enter_end(stage: &mut StageContext<'_>) -> Result<(), String> {
    stage.console(&format!("gamemode creative {BOT}"))?;
    end_tp(stage, Vector3::new(WAIT_POS[0], WAIT_POS[1], WAIT_POS[2])).await?;
    if !Arc::ptr_eq(&stage.world(), &end_world(stage)) {
        return Err("the bot didn't reach the End".to_string());
    }
    Ok(())
}

fn dragons(world: &World) -> Vec<Arc<dyn EntityBase>> {
    world
        .entities
        .load()
        .iter()
        .filter(|entity| {
            !entity.get_entity().is_removed()
                && entity
                    .cast_any()
                    .downcast_ref::<EnderDragonEntity>()
                    .is_some()
        })
        .cloned()
        .collect()
}

fn as_dragon(entity: &Arc<dyn EntityBase>) -> Result<&EnderDragonEntity, String> {
    entity
        .cast_any()
        .downcast_ref::<EnderDragonEntity>()
        .ok_or_else(|| "not an ender dragon".to_string())
}

fn living(
    entity: &Arc<dyn EntityBase>,
) -> Result<&pumpkin_core::entity::living::LivingEntity, String> {
    Ok(&as_dragon(entity)?.mob_entity.living_entity)
}

fn health(entity: &Arc<dyn EntityBase>) -> Result<f32, String> {
    Ok(living(entity)?.health.load())
}

/// Keeps the dragon where it is (vanilla `DragonHoverPhase`), so the bot can reach its parts.
fn hold(entity: &Arc<dyn EntityBase>) -> Result<(), String> {
    as_dragon(entity)?.set_phase(EnderDragonPhase::Hovering);
    Ok(())
}

/// Turns the dragon's AI off or back on. A hovering dragon circles the spot it holds, like
/// vanilla's, so the part tests stop it with `NoAI`, which vanilla `EnderDragon.aiStep` honours.
fn freeze(entity: &Arc<dyn EntityBase>, frozen: bool) -> Result<(), String> {
    hold(entity)?;
    as_dragon(entity)?.mob_entity.set_no_ai(frozen);
    entity
        .get_entity()
        .velocity
        .store(Vector3::new(0.0, 0.0, 0.0));
    Ok(())
}

async fn wait_for_dragon(stage: &mut StageContext<'_>) -> Result<Arc<dyn EntityBase>, String> {
    let world = end_world(stage);
    for _ in 0..MAX_TICKS {
        if let Some(dragon) = dragons(&world).into_iter().next() {
            return Ok(dragon);
        }
        stage.tick().await?;
    }
    Err(format!(
        "no ender dragon within {MAX_TICKS} ticks in the End"
    ))
}

/// The spike crystals would heal the dragon between hits.
fn remove_crystals(stage: &StageContext<'_>) {
    for entity in end_world(stage).entities.load().iter() {
        if entity.get_entity().entity_type == &EntityType::END_CRYSTAL {
            entity.get_entity().remove();
        }
    }
}

fn fight_nbt(world: &World) -> Result<NbtCompound, String> {
    let fight = world
        .dragon_fight
        .as_ref()
        .ok_or("the End has no dragon fight")?;
    Ok(fight
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .to_nbt())
}

fn gateways(fight: &NbtCompound) -> Vec<i32> {
    fight
        .get_list("gateways")
        .map(|list| {
            list.iter()
                .filter_map(|tag| match tag {
                    NbtTag::Int(gateway) => Some(*gateway),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A part's id, the centre of its box and half its width.
fn part_target(
    entity: &Arc<dyn EntityBase>,
    part: usize,
) -> Result<(i32, Vector3<f64>, f64), String> {
    let dragon = as_dragon(entity)?;
    let part = dragon
        .parts
        .get(part)
        .ok_or_else(|| format!("the dragon has no part {part}"))?;
    let bounds = part.entity.bounding_box.load();
    let center = Vector3::new(
        f64::midpoint(bounds.min.x, bounds.max.x),
        f64::midpoint(bounds.min.y, bounds.max.y),
        f64::midpoint(bounds.min.z, bounds.max.z),
    );
    Ok((
        part.entity.entity_id,
        center,
        (bounds.max.x - bounds.min.x) / 2.0,
    ))
}

/// Puts the bot one block beside the part with its eyes level with the part's centre, then
/// attacks the part's id like a vanilla client. Returns the health the dragon lost.
async fn hit_part(
    stage: &mut StageContext<'_>,
    dragon: &Arc<dyn EntityBase>,
    part: usize,
) -> Result<f32, String> {
    living(dragon)?.set_health(MAX_HEALTH);
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
    }
    let (part_id, center, half_width) = part_target(dragon, part)?;
    end_tp(
        stage,
        Vector3::new(center.x, center.y - 1.62, center.z + half_width + 1.0),
    )
    .await?;
    let before = health(dragon)?;
    stage.bot.attack_entity(part_id).await?;
    stage.sync().await?;
    Ok(before - health(dragon)?)
}

/// Vanilla `EnderDragon.hurt(part, ...)`: the head and neck take the full damage, every other part
/// `damage / 4 + min(damage, 1)`, and only a player or `always_hurts_ender_dragons` damage hurts
/// the dragon at all.
async fn dragon_damage(
    stage: &mut StageContext<'_>,
    dragon: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    freeze(dragon, true)?;
    let result = frozen_dragon_damage(stage, dragon).await;
    freeze(dragon, false)?;
    result
}

async fn frozen_dragon_damage(
    stage: &mut StageContext<'_>,
    dragon: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    hold_item(stage, &Item::DIAMOND_AXE);

    let head = hit_part(stage, dragon, 0).await?;
    if (head - AXE_DAMAGE).abs() > 1e-3 {
        return Err(format!(
            "a diamond axe hit on the head took {head} health, expected {AXE_DAMAGE}"
        ));
    }
    let body = hit_part(stage, dragon, 2).await?;
    let expected_body = AXE_DAMAGE / 4.0 + AXE_DAMAGE.min(1.0);
    if (body - expected_body).abs() > 1e-3 {
        return Err(format!(
            "a diamond axe hit on the body took {body} health, expected {expected_body}"
        ));
    }
    if stage.bot.disconnect_reason.is_some() {
        return Err("attacking the dragon's parts kicked the bot".to_string());
    }

    // An arrow from the bot's bow hits whichever part it meets first.
    living(dragon)?.set_health(MAX_HEALTH);
    stage.console(&format!("give {BOT} minecraft:arrow 16"))?;
    hold_item(stage, &Item::BOW);
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
    }
    // From 6 blocks south of the body with eyes level with its centre, then draw the bow.
    let (_, center, half_width) = part_target(dragon, 2)?;
    let from = Vector3::new(center.x, center.y - 1.62, center.z + half_width + 6.0);
    end_tp(stage, from).await?;
    // Yaw 180 faces north, toward the body.
    stage.bot.move_and_look(from, 180.0, 0.0, false).await?;
    stage.sync().await?;
    stage.bot.use_item(2).await?;
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
    }
    let before = health(dragon)?;
    stage.bot.release_use_item().await?;
    let mut shot = 0.0;
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
        shot = before - health(dragon)?;
        if shot > 0.0 {
            break;
        }
    }
    if shot <= 0.0 {
        let (_, now, _) = part_target(dragon, 2)?;
        return Err(format!(
            "the bot's arrow, shot north from {from:?} at the body around {center:?}, never hurt \
             the dragon (body now around {now:?})"
        ));
    }

    // Damage from no player, outside `always_hurts_ender_dragons`, leaves it alone.
    living(dragon)?.set_health(MAX_HEALTH);
    for _ in 0..GAP_TICKS {
        stage.tick().await?;
    }
    // Vanilla's command fails here, as the dragon refuses the damage.
    let _ = stage.console(
        "execute in minecraft:the_end run damage @e[type=minecraft:ender_dragon,limit=1] 20 \
         minecraft:generic",
    );
    stage.sync().await?;
    let untouched = health(dragon)?;
    if (untouched - MAX_HEALTH).abs() > 1e-3 {
        return Err(format!(
            "generic damage without a player took the dragon to {untouched} health"
        ));
    }

    healing_crystal_costs_ten(stage, dragon).await
}

async fn healing_crystal_costs_ten(
    stage: &mut StageContext<'_>,
    dragon: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    let pos = dragon.get_entity().pos.load();
    let crystal_pos = Vector3::new(pos.x + CRYSTAL_OFFSET, pos.y, pos.z);
    stage.console(&format!(
        "execute in minecraft:the_end run summon minecraft:end_crystal {:?} {:?} {:?}",
        crystal_pos.x, crystal_pos.y, crystal_pos.z
    ))?;
    // Healing shows the dragon has picked the crystal (vanilla `checkCrystals`).
    let hurt = MAX_HEALTH - 20.0;
    living(dragon)?.set_health(hurt);
    let mut healing = false;
    for _ in 0..MAX_TICKS {
        stage.tick().await?;
        if health(dragon)? > hurt {
            healing = true;
            break;
        }
    }
    if !healing {
        return Err(format!(
            "the dragon never healed from a crystal {CRYSTAL_OFFSET} blocks away"
        ));
    }
    living(dragon)?.set_health(MAX_HEALTH);
    let world = end_world(stage);
    let crystal = world
        .entities
        .load()
        .iter()
        .find(|entity| entity.get_entity().entity_type == &EntityType::END_CRYSTAL)
        .cloned()
        .ok_or("the summoned crystal is gone")?;
    let crystal_at = crystal.get_entity().pos.load();
    hold_item(stage, &Item::DIAMOND_AXE);
    end_tp(
        stage,
        Vector3::new(crystal_at.x, crystal_at.y, crystal_at.z + 2.0),
    )
    .await?;
    stage
        .bot
        .attack_entity(crystal.get_entity().entity_id)
        .await?;
    stage.sync().await?;
    let lost = MAX_HEALTH - health(dragon)?;
    if (lost - CRYSTAL_DAMAGE).abs() > 1e-3 {
        return Err(format!(
            "destroying the healing crystal took {lost} health, expected {CRYSTAL_DAMAGE}"
        ));
    }
    Ok(())
}

/// After a restart mid-fight the saved dragon comes back with its health, and the fight keeps
/// it instead of spawning another.
async fn fight_resumes(
    stage: &mut StageContext<'_>,
    dragon_uuid: uuid::Uuid,
) -> Result<Arc<dyn EntityBase>, String> {
    let dragon = wait_for_dragon(stage).await?;
    if dragon.get_entity().entity_uuid != dragon_uuid {
        return Err(format!(
            "a new dragon {} came instead of {dragon_uuid}",
            dragon.get_entity().entity_uuid
        ));
    }
    let loaded = health(&dragon)?;
    if (loaded - MID_FIGHT_HEALTH).abs() > 1e-3 {
        return Err(format!(
            "the dragon came back with {loaded} health, expected {MID_FIGHT_HEALTH}"
        ));
    }
    for _ in 0..RESPAWN_CHECK_TICKS {
        stage.tick().await?;
    }
    let world = end_world(stage);
    let all = dragons(&world);
    if all.len() != 1 || all[0].get_entity().entity_uuid != dragon_uuid {
        return Err(format!(
            "{RESPAWN_CHECK_TICKS} ticks after the restart the End holds {} dragons",
            all.len()
        ));
    }
    let fight = fight_nbt(&world)?;
    if fight.get_uuid("dragon_uuid") != Some(dragon_uuid) {
        return Err("the fight lost track of its dragon".to_string());
    }
    Ok(dragon)
}

/// Blocks of `block` in the box around the exit portal.
fn count_around_podium(world: &World, block: &Block) -> usize {
    let mut count = 0;
    for x in -4..=4 {
        for z in -4..=4 {
            for y in 0..=127 {
                if world.get_block(&BlockPos::new(x, y, z)) == block {
                    count += 1;
                }
            }
        }
    }
    count
}

fn experience_in(world: &World) -> u32 {
    world
        .entities
        .load()
        .iter()
        .filter(|entity| !entity.get_entity().is_removed())
        .filter_map(|entity| {
            entity
                .cast_any()
                .downcast_ref::<ExperienceOrbEntity>()
                .map(ExperienceOrbEntity::amount)
        })
        .sum()
}

/// Vanilla `EnderDragon.handleKillingBlow` and `tickDeath`: a lethal hit in flight leaves the
/// dragon dying for 200 ticks, then the exit portal opens, the egg sits on the podium, one
/// gateway appears and the orbs add up to 12,000 on the first kill.
async fn dragon_death(
    stage: &mut StageContext<'_>,
    dragon: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    let world = end_world(stage);
    end_tp(stage, Vector3::new(WAIT_POS[0], WAIT_POS[1], WAIT_POS[2])).await?;
    let gateways_before = gateways(&fight_nbt(&world)?);
    stage.console(&format!(
        "execute in minecraft:the_end run damage @e[type=minecraft:ender_dragon,limit=1] 1000 \
         minecraft:player_attack by {BOT}"
    ))?;
    let mut ticks = 0;
    let mut portal_too_early = false;
    while !dragon.get_entity().is_removed() {
        if ticks > DEATH_TICKS + 20 {
            return Err(format!(
                "the dragon was still there {ticks} ticks after the killing blow"
            ));
        }
        if ticks == DEATH_TICKS / 2 && count_around_podium(&world, &Block::END_PORTAL) > 0 {
            portal_too_early = true;
        }
        stage.tick().await?;
        ticks += 1;
    }
    // Counted at once: orbs that fell onto the portal leave for the overworld on their next tick.
    let xp = experience_in(&world);
    let portal = count_around_podium(&world, &Block::END_PORTAL);
    let eggs = count_around_podium(&world, &Block::DRAGON_EGG);
    if !(DEATH_TICKS - 1..=DEATH_TICKS + 1).contains(&ticks) {
        return Err(format!(
            "the dragon was removed {ticks} ticks after the killing blow, expected {DEATH_TICKS}"
        ));
    }
    if portal_too_early {
        return Err("the exit portal opened while the dragon was dying".to_string());
    }
    if portal != EXIT_PORTAL_BLOCKS {
        return Err(format!(
            "the exit portal holds {portal} end_portal blocks, expected {EXIT_PORTAL_BLOCKS}"
        ));
    }
    if eggs != 1 {
        return Err(format!("{eggs} dragon eggs around the podium, expected 1"));
    }
    if xp != FIRST_KILL_XP {
        return Err(format!(
            "the death dropped {xp} experience, expected {FIRST_KILL_XP}"
        ));
    }

    let gateways_after = gateways(&fight_nbt(&world)?);
    // The fight shuffles all 20 gateways on the first kill and pops one.
    let spawned: Vec<i32> = (0..GATEWAY_COUNT)
        .filter(|gateway| !gateways_after.contains(gateway))
        .collect();
    let expected_left = if gateways_before.is_empty() {
        GATEWAY_COUNT as usize - 1
    } else {
        gateways_before.len() - 1
    };
    let [gateway] = spawned[..] else {
        return Err(format!(
            "the fight's gateway list {gateways_after:?} doesn't miss exactly one gateway"
        ));
    };
    if gateways_after.len() != expected_left {
        return Err(format!(
            "the fight's gateway list has {} entries after the kill, expected {expected_left}",
            gateways_after.len()
        ));
    }
    let angle = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * f64::from(gateway));
    let gateway_pos = BlockPos::new(
        (GATEWAY_DISTANCE * angle.cos()).floor() as i32,
        GATEWAY_Y,
        (GATEWAY_DISTANCE * angle.sin()).floor() as i32,
    );
    let p = gateway_pos.0;
    end_tp(
        stage,
        Vector3::new(
            f64::from(p.x) + 0.5,
            f64::from(p.y + 4),
            f64::from(p.z) + 0.5,
        ),
    )
    .await?;
    stage.wait_for_terrain(gateway_pos).await?;
    let found = world.get_block(&gateway_pos);
    if found != &Block::END_GATEWAY {
        return Err(format!(
            "gateway {gateway} at {gateway_pos:?} is {}, expected end_gateway",
            found.name
        ));
    }
    end_tp(stage, Vector3::new(WAIT_POS[0], WAIT_POS[1], WAIT_POS[2])).await
}

/// After a restart the killed dragon stays dead: no new dragon, and the same gateways left.
async fn dragon_stays_dead(
    stage: &mut StageContext<'_>,
    fight_before: &NbtCompound,
) -> Result<(), String> {
    for _ in 0..RESPAWN_CHECK_TICKS {
        stage.tick().await?;
    }
    let world = end_world(stage);
    let found = dragons(&world).len();
    if found != 0 {
        return Err(format!("{found} dragons spawned after the restart"));
    }
    let fight = fight_nbt(&world)?;
    if fight.get_bool("dragon_killed") != Some(true)
        || fight.get_bool("previously_killed") != Some(true)
    {
        return Err(format!("the fight forgot the kill: {fight:?}"));
    }
    let (before, after) = (gateways(fight_before), gateways(&fight));
    if before != after {
        return Err(format!(
            "the gateways left were {before:?} before the restart and {after:?} after"
        ));
    }
    Ok(())
}

/// Vanilla `EnderDragon.kill` skips the death sequence and `setDragonKilled` opens the portal.
fn kill_opens_portal_at_once(stage: &StageContext<'_>) -> Result<(), String> {
    stage.console("execute in minecraft:the_end run kill @e[type=minecraft:ender_dragon]")?;
    let world = end_world(stage);
    let left = dragons(&world).len();
    if left != 0 {
        return Err(format!("{left} dragons left after kill"));
    }
    let portal = count_around_podium(&world, &Block::END_PORTAL);
    if portal != EXIT_PORTAL_BLOCKS {
        return Err(format!(
            "right after kill the exit portal holds {portal} end_portal blocks, expected \
             {EXIT_PORTAL_BLOCKS}"
        ));
    }
    if count_around_podium(&world, &Block::DRAGON_EGG) != 1 {
        return Err("no dragon egg on the podium right after kill".to_string());
    }
    Ok(())
}
