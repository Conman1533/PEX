use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;

use super::PiglinBruteEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainTick,
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
use crate::entity::mob::abstract_piglin;
use crate::world::World;

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

/// Vanilla `PiglinBruteAi.wasHurtBy`: anything but a piglin that hurts a brute gets it angry,
/// and the piglins around with it (`PiglinAi.maybeRetaliate`).
pub(super) fn was_hurt_by(brute: &PiglinBruteEntity, attacker: &dyn EntityBase) {
    if attacker.get_living_entity().is_none() || abstract_piglin::is_piglin_like(attacker) {
        return;
    }
    let world = brute.mob_entity.living_entity.entity.world.load_full();
    let Some(attacker) = abstract_piglin::resolve_attacker(&world, attacker) else {
        return;
    };
    let broadcast = brute.mob_entity.try_with_brain(brute, |brain| {
        abstract_piglin::maybe_retaliate(brain, &world, brute, &attacker)
    });
    if let Some(broadcast) = broadcast {
        abstract_piglin::run_broadcast(&world, brute, broadcast);
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
