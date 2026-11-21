use std::sync::atomic::Ordering::Relaxed;

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag;
use pumpkin_protocol::bedrock::server::ActorEventID;
use rand::RngExt;

use super::{ArmadilloEntity, ArmadilloState};
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainTick,
    activity::ActivityData,
    behavior::{
        AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing, LookAtTargetSink,
        MoveToTargetSink, OneShot, Swim, Timed, animal_make_love::AnimalMakeLove,
        baby_follow_adult, follow_temptation::FollowTemptation,
        random_look_around::RandomLookAround, random_stroll, run_one, run_one_with_conditions,
        set_entity_look_target_sometimes, set_walk_target_from_look_target, utils,
    },
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
    sensing::SensorType,
};
use crate::entity::mob::Mob;

const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 1.0;
const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.0;
const BABY_CLOSE_ENOUGH_DIST: f64 = 1.0;
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
/// Vanilla `ArmadilloBallUp.BALL_UP_STAY_IN_STATE`: five minutes.
const BALL_UP_STAY_IN_STATE: i32 = 5 * 60 * 20;
const DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD: i64 = 75;

const MEMORIES: [MemoryModuleId; 1] = [types::DANGER_DETECTED_RECENTLY.id()];

pub(super) fn make_brain(armadillo: &ArmadilloEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORIES,
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::HurtBy,
            SensorType::FoodTemptations,
            SensorType::NearestAdult,
            SensorType::ArmadilloScareDetected,
        ],
        activities,
    }
    .make_brain(armadillo, packed)
}

/// Vanilla `ArmadilloAi.updateActivity`.
pub(super) fn update_activity(armadillo: &ArmadilloEntity) {
    armadillo.mob_entity.with_brain(armadillo, |brain| {
        brain.set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
    });
}

fn armadillo<'a>(tick: &BrainTick<'a>) -> Option<&'a ArmadilloEntity> {
    tick.mob.cast_any().downcast_ref::<ArmadilloEntity>()
}

fn tempted_speed(_mob: &dyn Mob) -> f32 {
    SPEED_MULTIPLIER_WHEN_TEMPTED
}

fn tempted_close_enough(mob: &dyn Mob) -> f64 {
    if utils::is_baby(mob) {
        BABY_CLOSE_ENOUGH_DIST
    } else {
        DEFAULT_CLOSE_ENOUGH_DIST
    }
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(ArmadilloPanic::new(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                ))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(ArmadilloMoveToTargetSink(
                    MoveToTargetSink::default(),
                ))),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
                Box::new(CountDownCooldownTicks::new(types::GAZE_COOLDOWN_TICKS)),
                Box::new(rolling_out()),
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
                        &EntityType::ARMADILLO,
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
                            Box::new(baby_follow_adult::create(
                                ADULT_FOLLOW_RANGE.0,
                                ADULT_FOLLOW_RANGE.1,
                                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
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
                                Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                                1,
                            ),
                            (
                                Box::new(set_walk_target_from_look_target::create(
                                    SPEED_MULTIPLIER_WHEN_IDLING,
                                    3,
                                )),
                                1,
                            ),
                            (Box::new(DoNothing::new(30, 60)), 1),
                        ],
                    )),
                ),
            ],
        ),
        ActivityData::with_pairs_and_conditions(
            Activity::Panic,
            vec![(0, Box::new(Timed::new(ArmadilloBallUp::default())))],
            vec![
                (
                    types::DANGER_DETECTED_RECENTLY.id(),
                    MemoryStatus::ValuePresent,
                ),
                (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
            ],
        ),
    ]
}

/// Vanilla `ArmadilloAi.ARMADILLO_ROLLING_OUT`: once no danger was detected recently, a curled
/// up armadillo rolls out.
fn rolling_out() -> OneShot {
    OneShot::new(
        "ArmadilloRollingOut",
        vec![(
            types::DANGER_DETECTED_RECENTLY.id(),
            MemoryStatus::ValueAbsent,
        )],
        |tick| {
            armadillo(tick).is_some_and(|armadillo| {
                if armadillo.is_scared() {
                    armadillo.roll_out();
                    true
                } else {
                    false
                }
            })
        },
    )
}

/// Vanilla's `MoveToTargetSink` override in `ArmadilloAi`: no walking while curled up.
struct ArmadilloMoveToTargetSink(MoveToTargetSink);

impl Behavior for ArmadilloMoveToTargetSink {
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
        if armadillo(tick).is_some_and(ArmadilloEntity::is_scared) {
            return false;
        }
        self.0.check_extra_start_conditions(tick)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
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
        self.0.debug_name()
    }
}

/// Vanilla `ArmadilloAi.ArmadilloPanic`: only environmental damage makes it panic, and it rolls
/// out first.
struct ArmadilloPanic(AnimalPanic);

impl ArmadilloPanic {
    const fn new(speed_multiplier: f32) -> Self {
        Self(
            AnimalPanic::new(speed_multiplier)
                .with_panic_causes(&tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES),
        )
    }
}

impl Behavior for ArmadilloPanic {
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
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(armadillo) = armadillo(tick) {
            armadillo.roll_out();
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
        "ArmadilloPanic"
    }
}

/// Vanilla `ArmadilloAi.ArmadilloBallUp`: curls up, peeks while the danger is still around and
/// unrolls as the danger memory runs out.
#[derive(Default)]
struct ArmadilloBallUp {
    next_peek_timer: i32,
    danger_was_around: bool,
}

impl ArmadilloBallUp {
    fn pick_next_peek_timer(tick: &BrainTick<'_>) -> i32 {
        ArmadilloState::Scared.animation_duration() as i32
            + tick.mob.get_random().random_range(100..=400)
    }
}

impl Behavior for ArmadilloBallUp {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn min_duration(&self) -> i32 {
        BALL_UP_STAY_IN_STATE
    }

    fn max_duration(&self) -> i32 {
        BALL_UP_STAY_IN_STATE
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        entity.on_ground.load(Relaxed)
            && !entity.is_in_water()
            && !entity.touching_lava.load(Relaxed)
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        armadillo(tick).is_some_and(|armadillo| armadillo.get_state().is_threatened())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(armadillo) = armadillo(tick) {
            armadillo.roll_up();
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        let panicking = tick.state.has_memory_value(types::IS_PANICKING.id());
        if let Some(armadillo) = armadillo(tick)
            && !armadillo.can_stay_rolled_up_with(panicking)
        {
            armadillo.roll_out();
        }
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(armadillo) = armadillo(tick) else {
            return;
        };
        if self.next_peek_timer > 0 {
            self.next_peek_timer -= 1;
        }
        let entity = tick.mob.get_entity();
        let on_ground = entity.on_ground.load(Relaxed);
        if armadillo.should_switch_to_scared_state() {
            armadillo.switch_to_state(ArmadilloState::Scared);
            if on_ground {
                armadillo.play_sound(Sound::EntityArmadilloLand);
            }
            return;
        }
        let state = armadillo.get_state();
        let danger_tick_counter = tick
            .state
            .time_until_expiry(types::DANGER_DETECTED_RECENTLY.id())
            .unwrap_or(0);
        let danger_is_around = danger_tick_counter > DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD;
        if danger_is_around != self.danger_was_around {
            self.next_peek_timer = Self::pick_next_peek_timer(tick);
        }
        self.danger_was_around = danger_is_around;
        let unrolling = ArmadilloState::Unrolling.animation_duration() as i64;
        if state == ArmadilloState::Scared {
            if self.next_peek_timer == 0 && on_ground && danger_is_around {
                tick.world.send_entity_status(
                    entity,
                    EntityStatus::ArmadilloPeek,
                    ActorEventID::None,
                );
                self.next_peek_timer = Self::pick_next_peek_timer(tick);
            }
            if danger_tick_counter < unrolling {
                armadillo.play_sound(Sound::EntityArmadilloUnrollStart);
                armadillo.switch_to_state(ArmadilloState::Unrolling);
            }
        } else if state == ArmadilloState::Unrolling && danger_tick_counter > unrolling {
            armadillo.switch_to_state(ArmadilloState::Scared);
        }
    }

    fn debug_name(&self) -> &'static str {
        "ArmadilloBallUp"
    }
}
