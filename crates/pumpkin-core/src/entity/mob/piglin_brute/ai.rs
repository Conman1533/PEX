use std::sync::Arc;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;

use super::PiglinBruteEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick, VisibilityContext,
    activity::ActivityData,
    behavior::{
        DoNothing, LookAtTargetSink, MoveToTargetSink, Timed, interact_with, interact_with_door,
        melee_attack, random_stroll, run_one, set_entity_look_target, set_look_and_interact,
        set_walk_target_from_attack_target_if_target_out_of_reach, start_attacking,
        stop_attacking_if_target_invalid, stop_being_angry_if_target_dead,
        stroll_to_poi::{stroll_around_poi, stroll_to_poi},
        utils,
    },
    memory::{MemoryModuleId, PackedMemories, types},
    sensing::{SensorType, is_entity_attackable_ignoring_line_of_sight},
};
use crate::entity::mob::Mob;
use crate::world::World;

/// Vanilla `PiglinBruteAi.ANGER_DURATION` (also `PiglinAi.ANGER_DURATION`).
const ANGER_DURATION: i64 = 600;
const MELEE_ATTACK_COOLDOWN: i32 = 20;
const ACTIVITY_SOUND_LIKELIHOOD_PER_TICK: f32 = 0.0125;
const MAX_LOOK_DIST: f32 = 8.0;
const INTERACTION_RANGE: i32 = 8;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.6;
const HOME_CLOSE_ENOUGH_DISTANCE: i32 = 2;
const HOME_TOO_FAR_DISTANCE: i32 = 100;
const HOME_STROLL_AROUND_DISTANCE: i32 = 5;

const MEMORIES: [MemoryModuleId; 1] = [types::NEAREST_VISIBLE_ADULT_PIGLINS.id()];

/// Vanilla `PiglinBrute.BRAIN_PROVIDER`.
pub(super) fn make_brain(brute: &PiglinBruteEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORIES,
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::NearestPlayers,
            SensorType::NearestItems,
            SensorType::HurtBy,
            SensorType::PiglinBruteSpecific,
        ],
        activities,
    }
    .make_brain(brute, packed)
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(interact_with_door::create()),
                Box::new(stop_being_angry_if_target_dead::create()),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(start_attacking::create(find_nearest_valid_attack_target)),
                Box::new(run_one(vec![
                    (
                        Box::new(set_entity_look_target::create_for_type(
                            &EntityType::PLAYER,
                            MAX_LOOK_DIST,
                        )),
                        1,
                    ),
                    (
                        Box::new(set_entity_look_target::create_for_type(
                            &EntityType::PIGLIN,
                            MAX_LOOK_DIST,
                        )),
                        1,
                    ),
                    (
                        Box::new(set_entity_look_target::create_for_type(
                            &EntityType::PIGLIN_BRUTE,
                            MAX_LOOK_DIST,
                        )),
                        1,
                    ),
                    (Box::new(set_entity_look_target::create(MAX_LOOK_DIST)), 1),
                    (Box::new(DoNothing::new(30, 60)), 1),
                ])),
                Box::new(run_one(vec![
                    (
                        Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (
                        Box::new(interact_with::of(
                            &EntityType::PIGLIN,
                            INTERACTION_RANGE,
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            2,
                        )),
                        2,
                    ),
                    (
                        Box::new(interact_with::of(
                            &EntityType::PIGLIN_BRUTE,
                            INTERACTION_RANGE,
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            2,
                        )),
                        2,
                    ),
                    (
                        Box::new(stroll_to_poi(
                            types::HOME,
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            HOME_CLOSE_ENOUGH_DISTANCE,
                            HOME_TOO_FAR_DISTANCE,
                        )),
                        2,
                    ),
                    (
                        Box::new(stroll_around_poi(
                            types::HOME,
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            HOME_STROLL_AROUND_DISTANCE,
                        )),
                        2,
                    ),
                    (Box::new(DoNothing::new(30, 60)), 1),
                ])),
                Box::new(set_look_and_interact::create(&EntityType::PLAYER, 4)),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Fight,
            10,
            vec![
                Box::new(stop_attacking_if_target_invalid::create_when(
                    |tick, target| !is_nearest_valid_attack_target(tick, target),
                )),
                Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
                Box::new(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
            ],
            types::ATTACK_TARGET.id(),
        ),
    ]
}

/// Vanilla `PiglinBruteAi.initMemories`: the spawn spot is home.
pub(super) fn init_memories(brute: &PiglinBruteEntity, world: &World) {
    let Some(dimension) =
        pumpkin_data::dimension::Dimension::from_name(world.dimension.minecraft_name)
    else {
        return;
    };
    let home = crate::entity::ai::brain::memory::global_pos::GlobalPos::new(
        dimension,
        brute.mob_entity.living_entity.entity.block_pos.load(),
    );
    brute.mob_entity.try_with_brain(brute, |brain| {
        brain.set(types::HOME, home);
    });
}

/// Vanilla `PiglinBruteAi.updateActivity`, plus `maybePlayActivitySound` and the mirror of
/// `attack_target` into the mob's target (vanilla `getTarget` reads the brain).
pub(super) fn update_activity(brute: &PiglinBruteEntity) {
    let (changed, fighting, target) = brute.mob_entity.with_brain(brute, |brain| {
        let old_activity = brain.get_active_non_core_activity();
        brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
        let new_activity = brain.get_active_non_core_activity();
        (
            old_activity != new_activity,
            new_activity == Some(Activity::Fight),
            brain.get(types::ATTACK_TARGET).cloned(),
        )
    });
    // Vanilla plays the activity sound on a change and, separately, now and then.
    let sometimes = rand::random::<f32>() < ACTIVITY_SOUND_LIKELIHOOD_PER_TICK;
    if fighting && changed {
        brute.play_angry_sound();
    }
    if fighting && sometimes {
        brute.play_angry_sound();
    }
    brute.mob_entity.set_attacking(target.is_some());
    brute.mob_entity.set_target(target);
}

fn visibility<'a>(
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

/// Vanilla `PiglinBruteAi.findNearestValidAttackTarget`: whoever it is angry at, if still
/// attackable, else the nearest visible attackable player, else the nemesis.
fn find_nearest_valid_attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    if let Some(angry_at) =
        utils::get_living_entity_from_uuid_memory(tick.state, tick.world, types::ANGRY_AT)
        && is_entity_attackable_ignoring_line_of_sight(&tick.visibility(), angry_at.as_ref())
    {
        return Some(angry_at);
    }
    tick.state
        .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .map(|player| player.clone() as Arc<dyn EntityBase>)
        .or_else(|| tick.state.get(types::NEAREST_VISIBLE_NEMESIS).cloned())
}

fn is_nearest_valid_attack_target(tick: &BrainTick<'_>, target: &Arc<dyn EntityBase>) -> bool {
    find_nearest_valid_attack_target(tick)
        .is_some_and(|nearest| nearest.get_entity().entity_id == target.get_entity().entity_id)
}

fn is_piglin(entity: &dyn EntityBase) -> bool {
    let entity_type = entity.get_entity().entity_type;
    entity_type == &EntityType::PIGLIN || entity_type == &EntityType::PIGLIN_BRUTE
}

/// Vanilla `PiglinBruteAi.wasHurtBy`: anything but a piglin that hurts a brute gets it angry,
/// and the piglins around with it (`PiglinAi.maybeRetaliate`).
pub(super) fn was_hurt_by(brute: &PiglinBruteEntity, attacker: &dyn EntityBase) {
    if attacker.get_living_entity().is_none() || is_piglin(attacker) {
        return;
    }
    let world = brute.mob_entity.living_entity.entity.world.load_full();
    let attacker_id = attacker.get_entity().entity_id;
    let Some(attacker) = world.get_entity_by_id(attacker_id).or_else(|| {
        world
            .get_player_by_id(attacker_id)
            .map(|player| player as Arc<dyn EntityBase>)
    }) else {
        return;
    };
    let universal_anger = attacker.get_entity().entity_type == &EntityType::PLAYER
        && world.level_info.load().game_rules.universal_anger;
    let nearby = brute.mob_entity.try_with_brain(brute, |brain| {
        let ctx = visibility(brain, &world, brute);
        if !is_entity_attackable_ignoring_line_of_sight(&ctx, attacker.as_ref())
            || utils::is_other_target_much_further_away_than_current_attack_target(
                brain,
                brute,
                attacker.as_ref(),
                4.0,
            )
        {
            return None;
        }
        let target = if universal_anger {
            brain
                .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
                .map_or_else(
                    || attacker.clone(),
                    |player| player.clone() as Arc<dyn EntityBase>,
                )
        } else {
            attacker.clone()
        };
        set_anger_target(brain, &world, brute, &target);
        Some(
            brain
                .get(types::NEARBY_ADULT_PIGLINS)
                .cloned()
                .unwrap_or_default(),
        )
    });
    let Some(Some(nearby)) = nearby else {
        return;
    };
    for piglin in nearby {
        if piglin.get_entity().entity_id == brute.mob_entity.living_entity.entity.entity_id {
            continue;
        }
        anger_nearby_piglin(&world, piglin.as_ref(), &attacker, universal_anger);
    }
}

/// Vanilla `PiglinAi.setAngerTarget` on a brain.
fn set_anger_target(
    brain: &mut Brain,
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
    if target.get_entity().entity_type == &EntityType::PLAYER
        && world.level_info.load().game_rules.universal_anger
    {
        brain.set_with_expiry(types::UNIVERSAL_ANGER, true, ANGER_DURATION);
    }
}

/// Vanilla `PiglinAi.broadcastAngerTarget` (`setAngerTargetIfCloserThanCurrent`) and
/// `broadcastUniversalAnger` for one nearby adult piglin. A brute takes it on its brain; a
/// piglin is still on goals until G1-30, so it takes the target directly when it has none.
fn anger_nearby_piglin(
    world: &Arc<World>,
    piglin: &dyn EntityBase,
    attacker: &Arc<dyn EntityBase>,
    universal_anger: bool,
) {
    let Some(mob) = piglin.get_mob() else {
        return;
    };
    // Vanilla skips a hoglin target for piglins that cannot hunt; brutes never hunt.
    let hoglin = attacker.get_entity().entity_type == &EntityType::HOGLIN;
    if let Some(other) = mob.cast_any().downcast_ref::<PiglinBruteEntity>() {
        if hoglin && !universal_anger {
            return;
        }
        // A brain that is busy this instant (its own tick) misses the broadcast.
        other.mob_entity.try_with_brain(other, |brain| {
            if universal_anger {
                if let Some(player) = brain.get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER).cloned() {
                    set_anger_target(brain, world, other, &(player as Arc<dyn EntityBase>));
                }
                return;
            }
            let current = utils::get_living_entity_from_uuid_memory(brain, world, types::ANGRY_AT);
            let nearest = utils::get_nearest_target(other, current.clone(), attacker.clone());
            if current.is_none_or(|current| {
                current.get_entity().entity_id != nearest.get_entity().entity_id
            }) {
                set_anger_target(brain, world, other, &nearest);
            }
        });
    } else if let Some(other) = mob
        .cast_any()
        .downcast_ref::<crate::entity::mob::piglin::PiglinEntity>()
        && (!hoglin || other.can_hunt())
        && other.is_adult()
        && mob.get_mob_entity().get_target().is_none()
        && mob.can_attack(attacker.as_ref())
    {
        mob.get_mob_entity().set_target(Some(attacker.clone()));
    }
}

impl PiglinBruteEntity {
    /// Vanilla `PiglinBrute.playAngrySound`.
    pub(super) fn play_angry_sound(&self) {
        self.mob_entity
            .living_entity
            .make_sound(Sound::EntityPiglinBruteAngry);
    }
}
