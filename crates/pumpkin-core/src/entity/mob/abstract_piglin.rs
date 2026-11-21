//! Vanilla `PiglinAi`'s anger helpers that piglins and piglin brutes share (`AbstractPiglin`).

use std::sync::Arc;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use rand::RngExt;

use crate::entity::EntityBase;
use crate::entity::ai::brain::behavior::utils;
use crate::entity::ai::brain::memory::types;
use crate::entity::ai::brain::sensing::is_entity_attackable_ignoring_line_of_sight;
use crate::entity::ai::brain::{BrainState, VisibilityContext};
use crate::entity::mob::Mob;
use crate::entity::mob::hoglin::HoglinEntity;
use crate::entity::mob::piglin::PiglinEntity;
use crate::world::World;

/// Vanilla `PiglinAi.ANGER_DURATION`.
pub const ANGER_DURATION: i64 = 600;
/// Vanilla `PiglinAi.TIME_BETWEEN_HUNTS`: 30 to 120 seconds, in ticks.
pub const TIME_BETWEEN_HUNTS: (i64, i64) = (600, 2400);

/// A piglin or piglin brute.
#[must_use]
pub fn is_piglin_like(entity: &dyn EntityBase) -> bool {
    let entity_type = entity.get_entity().entity_type;
    entity_type == &EntityType::PIGLIN || entity_type == &EntityType::PIGLIN_BRUTE
}

/// Vanilla `AbstractPiglin.canHunt`: brutes never hunt; a piglin unless `CannotHunt`.
#[must_use]
pub fn can_hunt(mob: &dyn Mob) -> bool {
    mob.cast_any()
        .downcast_ref::<PiglinEntity>()
        .is_some_and(PiglinEntity::can_hunt)
}

/// Vanilla `Hoglin.canBeHunted`.
#[must_use]
pub fn hoglin_can_be_hunted(entity: &dyn EntityBase) -> bool {
    entity
        .cast_any()
        .downcast_ref::<HoglinEntity>()
        .is_some_and(|hoglin| {
            !hoglin
                .cannot_be_hunted
                .load(std::sync::atomic::Ordering::Relaxed)
        })
}

#[must_use]
pub fn visibility<'a>(
    state: &'a BrainState,
    world: &'a Arc<World>,
    mob: &'a dyn Mob,
) -> VisibilityContext<'a> {
    VisibilityContext {
        state,
        world,
        mob,
        follow_range: mob
            .get_mob_entity()
            .living_entity
            .get_attribute_value(&Attributes::FOLLOW_RANGE),
    }
}

/// Vanilla `PiglinAi.dontKillAnyMoreHoglinsForAWhile`.
pub fn dont_kill_any_more_hoglins_for_a_while(brain: &mut BrainState, mob: &dyn Mob) {
    let ticks = mob
        .get_random()
        .random_range(TIME_BETWEEN_HUNTS.0..=TIME_BETWEEN_HUNTS.1);
    brain.set_with_expiry(types::HUNTED_RECENTLY, true, ticks);
}

/// Vanilla `PiglinAi.setAngerTarget`.
pub fn set_anger_target(
    brain: &mut BrainState,
    world: &Arc<World>,
    body: &dyn Mob,
    target: &Arc<dyn EntityBase>,
) {
    if !is_entity_attackable_ignoring_line_of_sight(
        &visibility(brain, world, body),
        target.as_ref(),
    ) {
        return;
    }
    brain.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.set_with_expiry(
        types::ANGRY_AT,
        target.get_entity().entity_uuid,
        ANGER_DURATION,
    );
    let target_type = target.get_entity().entity_type;
    if target_type == &EntityType::HOGLIN && can_hunt(body) {
        dont_kill_any_more_hoglins_for_a_while(brain, body);
    }
    if target_type == &EntityType::PLAYER && world.level_info.load().game_rules.universal_anger {
        brain.set_with_expiry(types::UNIVERSAL_ANGER, true, ANGER_DURATION);
    }
}

/// Vanilla `PiglinAi.setAngerTargetToNearestTargetablePlayerIfFound`.
pub fn set_anger_target_to_nearest_targetable_player_if_found(
    brain: &mut BrainState,
    world: &Arc<World>,
    body: &dyn Mob,
    target_if_no_player_found: &Arc<dyn EntityBase>,
) {
    let target = brain
        .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .map_or_else(
            || target_if_no_player_found.clone(),
            |player| player.clone() as Arc<dyn EntityBase>,
        );
    set_anger_target(brain, world, body, &target);
}

/// Vanilla `PiglinAi.setAngerTargetIfCloserThanCurrent`.
pub fn set_anger_target_if_closer_than_current(
    brain: &mut BrainState,
    world: &Arc<World>,
    body: &dyn Mob,
    new_target: &Arc<dyn EntityBase>,
) {
    let current = utils::get_living_entity_from_uuid_memory(brain, world, types::ANGRY_AT);
    let nearest = utils::get_nearest_target(body, current.clone(), new_target.clone());
    if current
        .is_none_or(|current| current.get_entity().entity_id != nearest.get_entity().entity_id)
    {
        set_anger_target(brain, world, body, &nearest);
    }
}

/// What `maybe_retaliate` asks the nearby adult piglins to do, once the hurt piglin's own brain
/// is released.
pub enum Broadcast {
    None,
    /// Vanilla `broadcastAngerTarget`.
    Anger(Arc<dyn EntityBase>, Vec<Arc<dyn EntityBase>>),
    /// Vanilla `broadcastUniversalAnger`.
    Universal(Vec<Arc<dyn EntityBase>>),
}

/// Vanilla `PiglinAi.maybeRetaliate`: a piglin that isn't fleeing gets angry at an attacker it
/// can attack (unless its current target is much closer), and tells the adult piglins near it.
pub fn maybe_retaliate(
    brain: &mut BrainState,
    world: &Arc<World>,
    body: &dyn Mob,
    attacker: &Arc<dyn EntityBase>,
) -> Broadcast {
    if brain.get_active_non_core_activity() == Some(Activity::Avoid)
        || !is_entity_attackable_ignoring_line_of_sight(
            &visibility(brain, world, body),
            attacker.as_ref(),
        )
        || utils::is_other_target_much_further_away_than_current_attack_target(
            brain,
            body,
            attacker.as_ref(),
            4.0,
        )
    {
        return Broadcast::None;
    }
    let adults = brain
        .get(types::NEARBY_ADULT_PIGLINS)
        .cloned()
        .unwrap_or_default();
    if attacker.get_entity().entity_type == &EntityType::PLAYER
        && world.level_info.load().game_rules.universal_anger
    {
        set_anger_target_to_nearest_targetable_player_if_found(brain, world, body, attacker);
        Broadcast::Universal(adults)
    } else {
        set_anger_target(brain, world, body, attacker);
        Broadcast::Anger(attacker.clone(), adults)
    }
}

/// Runs a broadcast from `body` on the other piglins' brains. A brain that is busy this instant
/// (its own tick) misses it.
pub fn run_broadcast(world: &Arc<World>, body: &dyn Mob, broadcast: Broadcast) {
    match broadcast {
        Broadcast::None => {}
        Broadcast::Anger(target, piglins) => broadcast_anger_target(world, body, &piglins, &target),
        Broadcast::Universal(piglins) => broadcast_universal_anger(world, body, &piglins),
    }
}

/// Vanilla `PiglinAi.broadcastAngerTarget`: each adult piglin takes `target` as its anger target
/// if it is closer than its own; a hoglin only for piglins that hunt and hoglins that can be
/// hunted.
pub fn broadcast_anger_target(
    world: &Arc<World>,
    body: &dyn Mob,
    piglins: &[Arc<dyn EntityBase>],
    target: &Arc<dyn EntityBase>,
) {
    let hoglin = target.get_entity().entity_type == &EntityType::HOGLIN;
    for piglin in others(body, piglins) {
        let Some(mob) = piglin.get_mob() else {
            continue;
        };
        if hoglin && (!can_hunt(mob) || !hoglin_can_be_hunted(target.as_ref())) {
            continue;
        }
        mob.get_mob_entity().try_with_brain(mob, |brain| {
            set_anger_target_if_closer_than_current(brain, world, mob, target);
        });
    }
}

/// Vanilla `PiglinAi.broadcastUniversalAnger`: each adult piglin gets angry at the nearest player
/// it can attack.
pub fn broadcast_universal_anger(
    world: &Arc<World>,
    body: &dyn Mob,
    piglins: &[Arc<dyn EntityBase>],
) {
    for piglin in others(body, piglins) {
        let Some(mob) = piglin.get_mob() else {
            continue;
        };
        mob.get_mob_entity().try_with_brain(mob, |brain| {
            if let Some(player) = brain.get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER).cloned() {
                set_anger_target(brain, world, mob, &(player as Arc<dyn EntityBase>));
            }
        });
    }
}

fn others<'a>(
    body: &dyn Mob,
    piglins: &'a [Arc<dyn EntityBase>],
) -> impl Iterator<Item = &'a Arc<dyn EntityBase>> {
    let id = body.get_entity().entity_id;
    piglins
        .iter()
        .filter(move |piglin| piglin.get_entity().entity_id != id)
}

/// The live entity behind `attacker`: players are looked up apart from other entities.
#[must_use]
pub fn resolve_attacker(world: &World, attacker: &dyn EntityBase) -> Option<Arc<dyn EntityBase>> {
    let id = attacker.get_entity().entity_id;
    world.get_entity_by_id(id).or_else(|| {
        world
            .get_player_by_id(id)
            .map(|player| player as Arc<dyn EntityBase>)
    })
}
