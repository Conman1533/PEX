use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;

use super::CamelEntity;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainTick,
    activity::ActivityData,
    behavior::{
        AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing, LookAtTargetSink,
        MoveToTargetSink, OneShot, Swim, Timed, animal_make_love::AnimalMakeLove,
        baby_follow_adult, follow_temptation::FollowTemptation, one_shot,
        random_look_around::RandomLookAround, random_stroll, run_one, run_one_with_conditions,
        set_entity_look_target_sometimes, set_walk_target_from_look_target, utils,
    },
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
    sensing::SensorType,
};
use crate::entity::mob::Mob;

const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 4.0;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 2.5;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 2.5;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 1.0;
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
/// Vanilla `CamelAi.RandomSitting(20)`: the least time in one pose before switching, in seconds.
const RANDOM_SITTING_MIN_POSE_SECONDS: i64 = 20;

/// Vanilla `Camel.BRAIN_PROVIDER`.
pub(super) fn make_brain(camel: &CamelEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &[],
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::HurtBy,
            SensorType::FoodTemptations,
            SensorType::NearestAdult,
        ],
        activities,
    }
    .make_brain(camel, packed)
}

/// Vanilla `CamelAi.updateActivity`.
pub(super) fn update_activity(camel: &CamelEntity) {
    camel.mob_entity.with_brain(camel, |brain| {
        brain.set_active_activity_to_first_valid(&[Activity::Idle]);
    });
}

fn camel<'a>(tick: &BrainTick<'a>) -> Option<&'a CamelEntity> {
    tick.mob.cast_any().downcast_ref::<CamelEntity>()
}

/// Vanilla `Predicate.not(Camel::refuseToMove)`.
fn free_to_move(tick: &BrainTick<'_>) -> bool {
    camel(tick).is_some_and(|camel| !camel.refuse_to_move())
}

/// Vanilla `BehaviorBuilder.triggerIf(Predicate.not(Camel::refuseToMove), behavior)`.
fn unless_refusing_to_move(name: &'static str, behavior: OneShot) -> OneShot {
    one_shot::sequence(name, |tick| free_to_move(tick), behavior)
}

fn tempted_speed(_mob: &dyn Mob) -> f32 {
    SPEED_MULTIPLIER_WHEN_TEMPTED
}

/// Vanilla `camel -> camel.isBaby() ? 2.5 : 3.5`.
fn tempted_close_enough(mob: &dyn Mob) -> f64 {
    if utils::is_baby(mob) { 2.5 } else { 3.5 }
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(CamelPanic(AnimalPanic::new(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                )))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
                Box::new(CountDownCooldownTicks::new(types::GAZE_COOLDOWN_TICKS)),
            ],
        ),
        ActivityData::with_pairs(
            Activity::Idle,
            vec![
                (
                    0,
                    Box::new(set_entity_look_target_sometimes::create_for_type(
                        &EntityType::PLAYER,
                        6.0,
                        30,
                        60,
                    )),
                ),
                (
                    1,
                    Box::new(Timed::new(AnimalMakeLove::new(
                        &EntityType::CAMEL,
                        SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                        1,
                    ))),
                ),
                (
                    2,
                    Box::new(run_one(vec![
                        (
                            Box::new(Timed::new(FollowTemptation::with_close_enough(
                                tempted_speed,
                                tempted_close_enough,
                            ))),
                            1,
                        ),
                        (
                            Box::new(unless_refusing_to_move(
                                "BabyFollowAdult",
                                baby_follow_adult::create(
                                    ADULT_FOLLOW_RANGE.0,
                                    ADULT_FOLLOW_RANGE.1,
                                    SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                                ),
                            )),
                            1,
                        ),
                    ])),
                ),
                (
                    3,
                    Box::new(Timed::new(RandomLookAround::new(150, 250, 30.0, 0.0, 0.0))),
                ),
                (
                    4,
                    Box::new(run_one_with_conditions(
                        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
                        vec![
                            (
                                Box::new(unless_refusing_to_move(
                                    "RandomStroll",
                                    random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING),
                                )),
                                1,
                            ),
                            (
                                Box::new(unless_refusing_to_move(
                                    "SetWalkTargetFromLookTarget",
                                    set_walk_target_from_look_target::create(
                                        SPEED_MULTIPLIER_WHEN_IDLING,
                                        3,
                                    ),
                                )),
                                1,
                            ),
                            (Box::new(Timed::new(RandomSitting)), 1),
                            (Box::new(DoNothing::new(30, 60)), 1),
                        ],
                    )),
                ),
            ],
        ),
    ]
}

/// Vanilla `CamelAi.CamelPanic`: no panic while a mob rides and steers it (a camel husk with its
/// husk), and a panicking camel gets up at once.
struct CamelPanic(AnimalPanic);

impl Behavior for CamelPanic {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.0.entry_conditions()
    }

    fn min_duration(&self) -> i32 {
        self.0.min_duration()
    }

    fn max_duration(&self) -> i32 {
        self.0.max_duration()
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        self.0.check_extra_start_conditions(tick)
            && camel(tick).is_some_and(|camel| !camel.is_mob_controlled())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(camel) = camel(tick) {
            camel.stand_up_instantly();
        }
        self.0.start(tick);
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        self.0.tick(tick);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        self.0.stop(tick);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        self.0.can_still_use(tick)
    }

    fn debug_name(&self) -> &'static str {
        "CamelPanic"
    }
}

/// Vanilla `CamelAi.RandomSitting`: a camel that has held its pose for 20 seconds sits down or
/// gets up.
struct RandomSitting;

impl Behavior for RandomSitting {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(camel) = camel(tick) else {
            return false;
        };
        let entity = tick.mob.get_entity();
        !entity.is_in_water()
            && camel.pose_time() >= RANDOM_SITTING_MIN_POSE_SECONDS * 20
            && !entity.is_leashed()
            && entity.on_ground.load(std::sync::atomic::Ordering::Relaxed)
            && camel.mob_controlling_passenger().is_none()
            && camel.can_camel_change_pose()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let panicking = tick.state.has_memory_value(types::IS_PANICKING.id());
        let Some(camel) = camel(tick) else {
            return;
        };
        if camel.is_camel_sitting() {
            camel.stand_up();
        } else if !panicking {
            camel.sit_down();
        }
    }

    fn debug_name(&self) -> &'static str {
        "RandomSitting"
    }
}
