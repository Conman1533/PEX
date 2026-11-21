//! Vanilla `VillagerGoalPackages` and the villager's `Brain.Provider`: the core, work, play,
//! meet, rest, idle and panic activities.
//!
//! Not here yet: raids (`ReactToBell`, `SetRaidStatus` and the pre-raid, raid and hide
//! activities), breeding (`VillagerMakeLove`), gifts to the hero of the village, golem summoning
//! (G1-44), and the per-profession work and villager trading (`HarvestFarmland`, `UseBonemeal`,
//! `WorkAtComposter`, `ShowTradesToPlayer`, `TradeWithVillager`, G1-43).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use pumpkin_data::entity::{EntityStatus, EntityType, MobCategory};
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::villager::VillagerProfession;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use rand::RngExt;
use rand::seq::IndexedRandom;

use super::{VillagerData, VillagerEntity};
use crate::entity::EntityBase;
use crate::entity::ageable::AgeableMob;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        Behavior, BehaviorControl, DoNothing, LookAtTargetSink, MoveToTargetSink, OneShot, Swim,
        Timed, acquire_poi, go_to_wanted_item, interact_with, interact_with_door,
        jump_on_bed::JumpOnBed,
        one_shot, run_one, run_one_with_conditions, set_entity_look_target, set_look_and_interact,
        set_walk_target_away_from, set_walk_target_from_block_memory,
        set_walk_target_from_look_target,
        sleep_in_bed::{SleepInBed, wake_up},
        stroll_to_poi::{stroll_around_poi, stroll_to_poi, stroll_to_poi_list},
        utils::{self, is_baby},
        validate_nearby_poi, village_stroll,
    },
    memory::global_pos::GlobalPos,
    memory::position_tracker::{BlockPosTracker, EntityTracker, PositionTracker},
    memory::walk_target::WalkTarget,
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
    sensing::SensorType,
};
use crate::entity::ai::util::land_random_pos;
use crate::entity::mob::Mob;
use crate::world::World;
use crate::world::villager_poi::{PoiKind, VillageSections};

/// Vanilla `Villager.SPEED_MODIFIER`.
pub const SPEED_MODIFIER: f32 = 0.5;
/// Vanilla `VillagerGoalPackages.STROLL_SPEED_MODIFIER`.
const STROLL_SPEED_MODIFIER: f32 = 0.4;
/// Vanilla `VillagerGoalPackages.INTERACT_DIST_SQR`'s walk-up distance.
const INTERACT_WALKUP_DIST: i32 = 2;
/// Vanilla `Brain.updateActivityFromSchedule`'s 20 tick cadence.
const SCHEDULE_UPDATE_INTERVAL: i64 = 20;

/// The memories villager code writes without a behavior that needs them registered.
const MEMORIES: [MemoryModuleId; 20] = [
    types::HOME.id(),
    types::JOB_SITE.id(),
    types::POTENTIAL_JOB_SITE.id(),
    types::MEETING_POINT.id(),
    types::LAST_SLEPT.id(),
    types::LAST_WOKEN.id(),
    types::LAST_WORKED_AT_POI.id(),
    types::GOLEM_DETECTED_RECENTLY.id(),
    types::PATH.id(),
    types::WALK_TARGET.id(),
    types::LOOK_TARGET.id(),
    types::INTERACTION_TARGET.id(),
    types::BREED_TARGET.id(),
    types::CANT_REACH_WALK_TARGET_SINCE.id(),
    types::DOORS_TO_CLOSE.id(),
    types::HURT_BY.id(),
    types::HURT_BY_ENTITY.id(),
    types::NEAREST_HOSTILE.id(),
    types::SECONDARY_JOB_SITE.id(),
    types::NEAREST_BED.id(),
];

/// Vanilla `Villager.BRAIN_PROVIDER`'s sensors.
const SENSORS: [SensorType; 9] = [
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestItems,
    SensorType::NearestBed,
    SensorType::HurtBy,
    SensorType::VillagerHostiles,
    SensorType::VillagerBabies,
    SensorType::SecondaryPois,
    SensorType::GolemDetected,
];

fn villager(mob: &dyn Mob) -> Option<&VillagerEntity> {
    mob.cast_any().downcast_ref::<VillagerEntity>()
}

fn villager_of(entity: &Arc<dyn EntityBase>) -> Option<&VillagerEntity> {
    entity
        .get_mob()
        .and_then(|mob| mob.cast_any().downcast_ref::<VillagerEntity>())
}

/// Vanilla `VillagerProfession.heldJobSite`.
#[must_use]
pub fn held_job_site(profession: VillagerProfession, kind: PoiKind) -> bool {
    !matches!(
        profession,
        VillagerProfession::None | VillagerProfession::Nitwit
    ) && kind == PoiKind::Job(profession)
}

/// Vanilla `VillagerProfession.acquirableJobSite`: any job site while unemployed.
#[must_use]
pub fn acquirable_job_site(profession: VillagerProfession, kind: PoiKind) -> bool {
    match profession {
        VillagerProfession::None => kind.is_job(),
        VillagerProfession::Nitwit => false,
        profession => kind == PoiKind::Job(profession),
    }
}

/// Vanilla `VillagerGoalPackages.validateBedPoi`.
fn validate_bed_poi(world: &World, pos: pumpkin_util::math::position::BlockPos) -> bool {
    validate_nearby_poi::bed_occupied(world, &pos) == Some(false)
}

/// Vanilla `Villager.BRAIN_PROVIDER` plus `registerBrainGoals`' first schedule update.
pub(super) fn make_brain(villager: &VillagerEntity, packed: &PackedMemories) -> Brain {
    let mut brain = BrainProvider {
        memory_types: &MEMORIES,
        sensor_types: &SENSORS,
        activities,
    }
    .make_brain(villager, packed);
    let entity = villager.get_entity();
    let world = entity.world.load();
    let scheduled = world.villager_activity(&entity.block_pos.load(), villager.is_baby());
    if !brain.is_active(scheduled) {
        brain.set_active_activity_if_possible(scheduled);
    }
    brain
}

fn activities(mob: &dyn Mob) -> Vec<ActivityData> {
    let profession = villager(mob).map_or(VillagerProfession::None, VillagerEntity::profession);
    let baby = is_baby(mob);
    let schedule = Arc::new(AtomicI64::new(
        mob.get_entity().world.load().get_world_age(),
    ));
    let mut activities = Vec::new();
    if baby {
        activities.push(ActivityData::with_pairs(
            Activity::Play,
            play_package(SPEED_MODIFIER, &schedule),
        ));
    } else {
        activities.push(ActivityData::with_pairs_and_conditions(
            Activity::Work,
            work_package(SPEED_MODIFIER, &schedule),
            vec![(types::JOB_SITE.id(), MemoryStatus::ValuePresent)],
        ));
    }
    activities.push(ActivityData::with_pairs(
        Activity::Core,
        core_package(profession, SPEED_MODIFIER),
    ));
    activities.push(ActivityData::with_pairs_and_conditions(
        Activity::Meet,
        meet_package(SPEED_MODIFIER, &schedule),
        vec![(types::MEETING_POINT.id(), MemoryStatus::ValuePresent)],
    ));
    activities.push(ActivityData::with_pairs(
        Activity::Rest,
        rest_package(SPEED_MODIFIER, &schedule),
    ));
    activities.push(ActivityData::with_pairs(
        Activity::Idle,
        idle_package(SPEED_MODIFIER, &schedule),
    ));
    activities.push(ActivityData::with_pairs(
        Activity::Panic,
        panic_package(SPEED_MODIFIER),
    ));
    activities
}

type Package = Vec<(i32, Box<dyn BehaviorControl>)>;

fn boxed(behavior: impl BehaviorControl + 'static) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// Vanilla `VillagerGoalPackages.getCorePackage`.
fn core_package(profession: VillagerProfession, speed_modifier: f32) -> Package {
    vec![
        (0, boxed(Timed::new(Swim::new(0.8)))),
        (0, boxed(interact_with_door::create())),
        (0, boxed(Timed::new(LookAtTargetSink::new(45, 90)))),
        (0, boxed(Timed::new(VillagerPanicTrigger))),
        (0, boxed(wake_up())),
        (
            0,
            boxed(validate_nearby_poi::create(
                move |_, kind| held_job_site(profession, kind),
                types::JOB_SITE,
            )),
        ),
        (
            0,
            boxed(validate_nearby_poi::create(
                move |_, kind| acquirable_job_site(profession, kind),
                types::POTENTIAL_JOB_SITE,
            )),
        ),
        (1, boxed(Timed::new(MoveToTargetSink::default()))),
        (2, boxed(poi_competitor_scan())),
        (
            3,
            boxed(Timed::new(LookAndFollowTradingPlayerSink {
                speed_modifier,
            })),
        ),
        (
            5,
            boxed(go_to_wanted_item::create(speed_modifier, false, 4)),
        ),
        (
            6,
            boxed(acquire_poi::create(
                move |_, kind| acquirable_job_site(profession, kind),
                types::JOB_SITE,
                types::POTENTIAL_JOB_SITE,
                true,
                false,
                |_, _| true,
            )),
        ),
        (
            7,
            boxed(Timed::new(GoToPotentialJobSite { speed_modifier })),
        ),
        (8, boxed(yield_job_site(speed_modifier))),
        (
            10,
            boxed(acquire_poi::create(
                |_, kind| kind == PoiKind::Home,
                types::HOME,
                types::HOME,
                false,
                true,
                validate_bed_poi,
            )),
        ),
        (
            10,
            boxed(acquire_poi::create(
                |_, kind| kind == PoiKind::Meeting,
                types::MEETING_POINT,
                types::MEETING_POINT,
                true,
                true,
                |_, _| true,
            )),
        ),
        (10, boxed(assign_profession_from_job_site())),
        (10, boxed(reset_profession())),
    ]
}

/// Vanilla `VillagerGoalPackages.getWorkPackage`, without the farming behaviors.
fn work_package(speed_modifier: f32, schedule: &Arc<AtomicI64>) -> Package {
    vec![
        minimal_look_behavior(),
        (
            5,
            boxed(run_one(vec![
                (boxed(Timed::new(WorkAtPoi::default())), 7),
                (
                    boxed(stroll_around_poi(types::JOB_SITE, STROLL_SPEED_MODIFIER, 4)),
                    2,
                ),
                (
                    boxed(stroll_to_poi(types::JOB_SITE, STROLL_SPEED_MODIFIER, 1, 10)),
                    5,
                ),
                (
                    boxed(stroll_to_poi_list(
                        types::SECONDARY_JOB_SITE,
                        speed_modifier,
                        1,
                        6,
                        types::JOB_SITE,
                    )),
                    5,
                ),
            ])),
        ),
        (
            10,
            boxed(set_look_and_interact::create(&EntityType::PLAYER, 4)),
        ),
        (
            2,
            boxed(set_walk_target_from_block_memory::create(
                types::JOB_SITE,
                speed_modifier,
                9,
                100,
                1200,
            )),
        ),
        (99, boxed(update_activity_from_schedule(schedule.clone()))),
    ]
}

/// Vanilla `VillagerGoalPackages.getPlayPackage`.
fn play_package(speed_modifier: f32, schedule: &Arc<AtomicI64>) -> Package {
    vec![
        (0, boxed(Timed::new(MoveToTargetSink::new(80, 120)))),
        full_look_behavior(),
        (5, boxed(play_tag_with_other_kids())),
        (
            5,
            boxed(run_one_with_conditions(
                vec![(
                    types::VISIBLE_VILLAGER_BABIES.id(),
                    MemoryStatus::ValueAbsent,
                )],
                vec![
                    (
                        boxed(interact_with::of(
                            &EntityType::VILLAGER,
                            8,
                            speed_modifier,
                            INTERACT_WALKUP_DIST,
                        )),
                        2,
                    ),
                    (
                        boxed(interact_with::of(
                            &EntityType::CAT,
                            8,
                            speed_modifier,
                            INTERACT_WALKUP_DIST,
                        )),
                        1,
                    ),
                    (
                        boxed(village_stroll::village_bound_random_stroll(speed_modifier)),
                        1,
                    ),
                    (
                        boxed(set_walk_target_from_look_target::create(speed_modifier, 2)),
                        1,
                    ),
                    (boxed(Timed::new(JumpOnBed::new(speed_modifier))), 2),
                    (boxed(DoNothing::new(20, 40)), 2),
                ],
            )),
        ),
        (99, boxed(update_activity_from_schedule(schedule.clone()))),
    ]
}

/// Vanilla `VillagerGoalPackages.getRestPackage`.
fn rest_package(speed_modifier: f32, schedule: &Arc<AtomicI64>) -> Package {
    vec![
        (
            2,
            boxed(set_walk_target_from_block_memory::create(
                types::HOME,
                speed_modifier,
                1,
                150,
                1200,
            )),
        ),
        (
            3,
            boxed(validate_nearby_poi::create(
                |_, kind| kind == PoiKind::Home,
                types::HOME,
            )),
        ),
        (3, boxed(Timed::new(SleepInBed::default()))),
        (
            5,
            boxed(run_one_with_conditions(
                vec![(types::HOME.id(), MemoryStatus::ValueAbsent)],
                vec![
                    (
                        boxed(village_stroll::set_closest_home_as_walk_target(
                            speed_modifier,
                        )),
                        1,
                    ),
                    (
                        boxed(village_stroll::inside_brownian_walk(speed_modifier)),
                        4,
                    ),
                    (
                        boxed(village_stroll::go_to_closest_village(speed_modifier, 4)),
                        2,
                    ),
                    (boxed(DoNothing::new(20, 40)), 2),
                ],
            )),
        ),
        minimal_look_behavior(),
        (99, boxed(update_activity_from_schedule(schedule.clone()))),
    ]
}

/// Vanilla `VillagerGoalPackages.getMeetPackage`, without trading and gifts.
fn meet_package(speed_modifier: f32, schedule: &Arc<AtomicI64>) -> Package {
    vec![
        (
            2,
            boxed(one_shot::trigger_one_shuffled(
                "TriggerGate",
                vec![
                    (
                        stroll_around_poi(types::MEETING_POINT, STROLL_SPEED_MODIFIER, 40)
                            .into_trigger(),
                        2,
                    ),
                    (socialize_at_bell().into_trigger(), 2),
                ],
            )),
        ),
        (
            10,
            boxed(set_look_and_interact::create(&EntityType::PLAYER, 4)),
        ),
        (
            2,
            boxed(set_walk_target_from_block_memory::create(
                types::MEETING_POINT,
                speed_modifier,
                6,
                100,
                200,
            )),
        ),
        (
            3,
            boxed(validate_nearby_poi::create(
                |_, kind| kind == PoiKind::Meeting,
                types::MEETING_POINT,
            )),
        ),
        full_look_behavior(),
        (99, boxed(update_activity_from_schedule(schedule.clone()))),
    ]
}

/// Vanilla `VillagerGoalPackages.getIdlePackage`, without breeding, trading and gifts.
fn idle_package(speed_modifier: f32, schedule: &Arc<AtomicI64>) -> Package {
    vec![
        (
            2,
            boxed(run_one(vec![
                (
                    boxed(interact_with::of(
                        &EntityType::VILLAGER,
                        8,
                        speed_modifier,
                        INTERACT_WALKUP_DIST,
                    )),
                    2,
                ),
                (
                    boxed(interact_with::of(
                        &EntityType::CAT,
                        8,
                        speed_modifier,
                        INTERACT_WALKUP_DIST,
                    )),
                    1,
                ),
                (
                    boxed(village_stroll::village_bound_random_stroll(speed_modifier)),
                    1,
                ),
                (
                    boxed(set_walk_target_from_look_target::create(speed_modifier, 2)),
                    1,
                ),
                (boxed(Timed::new(JumpOnBed::new(speed_modifier))), 1),
                (boxed(DoNothing::new(30, 60)), 1),
            ])),
        ),
        (
            3,
            boxed(set_look_and_interact::create(&EntityType::PLAYER, 4)),
        ),
        full_look_behavior(),
        (99, boxed(update_activity_from_schedule(schedule.clone()))),
    ]
}

/// Vanilla `VillagerGoalPackages.getPanicPackage`.
fn panic_package(speed_modifier: f32) -> Package {
    let runaway_speed = speed_modifier * 1.5;
    vec![
        (0, boxed(villager_calm_down())),
        (
            1,
            boxed(set_walk_target_away_from::entity(
                types::NEAREST_HOSTILE,
                runaway_speed,
                6,
                false,
            )),
        ),
        (
            1,
            boxed(set_walk_target_away_from::entity(
                types::HURT_BY_ENTITY,
                runaway_speed,
                6,
                false,
            )),
        ),
        (
            3,
            boxed(village_stroll::village_bound_random_stroll_range(
                runaway_speed,
                2,
                2,
            )),
        ),
        minimal_look_behavior(),
    ]
}

fn look_at_category(category: &'static MobCategory) -> Box<dyn BehaviorControl> {
    boxed(set_entity_look_target::create_with(
        move |entity| entity.get_entity().entity_type.category == category,
        8.0,
    ))
}

/// Vanilla `VillagerGoalPackages.getFullLookBehavior`.
fn full_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    (
        5,
        boxed(run_one(vec![
            (
                boxed(set_entity_look_target::create_for_type(
                    &EntityType::CAT,
                    8.0,
                )),
                8,
            ),
            (
                boxed(set_entity_look_target::create_for_type(
                    &EntityType::VILLAGER,
                    8.0,
                )),
                2,
            ),
            (
                boxed(set_entity_look_target::create_for_type(
                    &EntityType::PLAYER,
                    8.0,
                )),
                2,
            ),
            (look_at_category(&MobCategory::CREATURE), 1),
            (look_at_category(&MobCategory::WATER_CREATURE), 1),
            (look_at_category(&MobCategory::AXOLOTLS), 1),
            (
                look_at_category(&MobCategory::UNDERGROUND_WATER_CREATURE),
                1,
            ),
            (look_at_category(&MobCategory::WATER_AMBIENT), 1),
            (look_at_category(&MobCategory::MONSTER), 1),
            (boxed(DoNothing::new(30, 60)), 2),
        ])),
    )
}

/// Vanilla `VillagerGoalPackages.getMinimalLookBehavior`.
fn minimal_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    (
        5,
        boxed(run_one(vec![
            (
                boxed(set_entity_look_target::create_for_type(
                    &EntityType::VILLAGER,
                    8.0,
                )),
                2,
            ),
            (
                boxed(set_entity_look_target::create_for_type(
                    &EntityType::PLAYER,
                    8.0,
                )),
                2,
            ),
            (boxed(DoNothing::new(30, 60)), 8),
        ])),
    )
}

/// Vanilla `Brain.updateActivityFromSchedule`, at most once a second.
fn update_schedule(tick: &mut BrainTick<'_>, last_update: &AtomicI64) {
    if tick.time - last_update.load(Ordering::Relaxed) <= SCHEDULE_UPDATE_INTERVAL {
        return;
    }
    last_update.store(tick.time, Ordering::Relaxed);
    let scheduled = tick
        .world
        .villager_activity(&tick.mob.get_entity().block_pos.load(), is_baby(tick.mob));
    if !tick.state.is_active(scheduled) {
        tick.state.set_active_activity_if_possible(scheduled);
    }
}

/// Vanilla `UpdateActivityFromSchedule.create`.
fn update_activity_from_schedule(last_update: Arc<AtomicI64>) -> OneShot {
    OneShot::new("UpdateActivityFromSchedule", Vec::new(), move |tick| {
        update_schedule(tick, &last_update);
        true
    })
}

fn is_hurt(state: &BrainState) -> bool {
    state.has_memory_value(types::HURT_BY.id())
}

fn has_hostile(state: &BrainState) -> bool {
    state.has_memory_value(types::NEAREST_HOSTILE.id())
}

/// Vanilla `VillagerPanicTrigger`. Golem summoning while panicking is G1-44.
struct VillagerPanicTrigger;

impl Behavior for VillagerPanicTrigger {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        is_hurt(tick.state) || has_hostile(tick.state)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if !(is_hurt(tick.state) || has_hostile(tick.state)) {
            return;
        }
        if !tick.state.is_active(Activity::Panic) {
            for memory in [
                types::PATH.id(),
                types::WALK_TARGET.id(),
                types::LOOK_TARGET.id(),
                types::BREED_TARGET.id(),
                types::INTERACTION_TARGET.id(),
            ] {
                tick.state.erase(memory);
            }
        }
        tick.state.set_active_activity_if_possible(Activity::Panic);
    }

    fn debug_name(&self) -> &'static str {
        "VillagerPanicTrigger"
    }
}

/// Vanilla `VillagerCalmDown.SAFE_DISTANCE_FROM_DANGER`.
const SAFE_DISTANCE_FROM_DANGER: f64 = 36.0;

/// Vanilla `VillagerCalmDown.create`.
fn villager_calm_down() -> OneShot {
    OneShot::new(
        "VillagerCalmDown",
        vec![
            (types::HURT_BY.id(), MemoryStatus::Registered),
            (types::HURT_BY_ENTITY.id(), MemoryStatus::Registered),
            (types::NEAREST_HOSTILE.id(), MemoryStatus::Registered),
        ],
        |tick| {
            let body = tick.mob.get_entity().pos.load();
            let feel_scared = is_hurt(tick.state)
                || has_hostile(tick.state)
                || tick.state.get(types::HURT_BY_ENTITY).is_some_and(|entity| {
                    entity
                        .get_entity()
                        .pos
                        .load()
                        .squared_distance_to_vec(&body)
                        <= SAFE_DISTANCE_FROM_DANGER
                });
            if !feel_scared {
                tick.state.erase(types::HURT_BY.id());
                tick.state.erase(types::HURT_BY_ENTITY.id());
                // Vanilla calls the brain's schedule update directly, outside its cadence.
                update_schedule(tick, &AtomicI64::new(i64::MIN / 2));
            }
            true
        },
    )
}

/// Vanilla `PoiCompetitorScan.create`: of the villagers holding the same job site, the one with
/// the most experience keeps it.
fn poi_competitor_scan() -> OneShot {
    OneShot::new(
        "PoiCompetitorScan",
        vec![
            (types::JOB_SITE.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        |tick| {
            let Some(me) = villager(tick.mob) else {
                return true;
            };
            let Some(pos) = tick.state.get(types::JOB_SITE).copied() else {
                return true;
            };
            let Some(kind) = tick
                .world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kind_at(&pos.pos)
            else {
                return true;
            };
            let my_id = tick.mob.get_entity().entity_id;
            let competitors: Vec<Arc<dyn EntityBase>> = tick
                .state
                .get(types::NEAREST_LIVING_ENTITIES)
                .map(|entities| {
                    entities
                        .iter()
                        .filter(|entity| {
                            entity.get_entity().entity_id != my_id
                                && entity.get_entity().is_alive()
                                && villager_of(entity).is_some_and(|other| {
                                    held_job_site(other.profession(), kind)
                                        && other.job_site_memory() == Some(pos)
                                })
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            // `None` is this villager, which may only be told to give up through its own state.
            let mut winner: Option<Arc<dyn EntityBase>> = None;
            for competitor in competitors {
                let Some(other) = villager_of(&competitor) else {
                    continue;
                };
                let winner_xp = winner.as_ref().and_then(villager_of).map_or_else(
                    || me.xp.load(Ordering::Relaxed),
                    |v| v.xp.load(Ordering::Relaxed),
                );
                if winner_xp > other.xp.load(Ordering::Relaxed) {
                    other.forget_job_site();
                } else {
                    match winner.as_ref().and_then(villager_of) {
                        None => tick.state.erase(types::JOB_SITE.id()),
                        Some(loser) => loser.forget_job_site(),
                    }
                    winner = Some(competitor.clone());
                }
            }
            true
        },
    )
}

/// Vanilla `YieldJobSite.create`: an unemployed villager hands the job site it is walking to over
/// to a villager of that profession that needs it.
fn yield_job_site(speed_modifier: f32) -> OneShot {
    OneShot::new(
        "YieldJobSite",
        vec![
            (types::POTENTIAL_JOB_SITE.id(), MemoryStatus::ValuePresent),
            (types::JOB_SITE.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
        ],
        move |tick| {
            if is_baby(tick.mob)
                || villager(tick.mob).map(VillagerEntity::profession)
                    != Some(VillagerProfession::None)
            {
                return false;
            }
            let Some(poi_pos) = tick.state.get(types::POTENTIAL_JOB_SITE).map(|pos| pos.pos) else {
                return false;
            };
            let Some(kind) = tick
                .world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kind_at(&poi_pos)
            else {
                return true;
            };
            let my_id = tick.mob.get_entity().entity_id;
            let taker = tick
                .state
                .get(types::NEAREST_LIVING_ENTITIES)
                .and_then(|entities| {
                    entities
                        .iter()
                        .find(|entity| {
                            entity.get_entity().entity_id != my_id
                                && entity.get_entity().is_alive()
                                && villager_of(entity)
                                    .is_some_and(|other| other.wants_job_site(kind, poi_pos))
                        })
                        .cloned()
                });
            if let Some(taker) = taker {
                tick.state.erase(types::WALK_TARGET.id());
                tick.state.erase(types::LOOK_TARGET.id());
                tick.state.erase(types::POTENTIAL_JOB_SITE.id());
                if let Some(other) = villager_of(&taker)
                    && other.job_site_memory().is_none()
                {
                    let mut storage = tick
                        .world
                        .villager_poi
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    storage.release(poi_pos, tick.mob.get_entity().entity_uuid);
                    storage.take(poi_pos, &taker);
                    drop(storage);
                    if let Some(pos) = GlobalPos::in_world(tick.world, poi_pos) {
                        other.take_potential_job_site(pos, speed_modifier);
                    }
                }
            }
            true
        },
    )
}

/// Vanilla `GoToPotentialJobSite`.
struct GoToPotentialJobSite {
    speed_modifier: f32,
}

const POTENTIAL_JOB_SITE_PRESENT: [(MemoryModuleId, MemoryStatus); 1] =
    [(types::POTENTIAL_JOB_SITE.id(), MemoryStatus::ValuePresent)];

impl Behavior for GoToPotentialJobSite {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &POTENTIAL_JOB_SITE_PRESENT
    }

    /// Vanilla `GoToPotentialJobSite.TICKS_UNTIL_TIMEOUT`.
    fn min_duration(&self) -> i32 {
        1200
    }

    fn max_duration(&self) -> i32 {
        1200
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state
            .get_active_non_core_activity()
            .is_none_or(|activity| {
                matches!(activity, Activity::Idle | Activity::Work | Activity::Play)
            })
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::POTENTIAL_JOB_SITE.id())
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(pos) = tick.state.get(types::POTENTIAL_JOB_SITE).map(|pos| pos.pos) {
            utils::set_walk_and_look_target_memories_to_block(
                tick.state,
                pos,
                self.speed_modifier,
                1,
            );
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        acquire_poi::release_poi(tick, types::POTENTIAL_JOB_SITE);
        tick.state.erase(types::POTENTIAL_JOB_SITE.id());
    }

    fn debug_name(&self) -> &'static str {
        "GoToPotentialJobSite"
    }
}

/// Vanilla `AssignProfessionFromJobSite.create`: once close, the potential job site becomes the
/// job site and an unemployed villager takes its profession.
fn assign_profession_from_job_site() -> OneShot {
    OneShot::new(
        "AssignProfessionFromJobSite",
        vec![
            (types::POTENTIAL_JOB_SITE.id(), MemoryStatus::ValuePresent),
            (types::JOB_SITE.id(), MemoryStatus::Registered),
        ],
        |tick| {
            let Some(pos) = tick.state.get(types::POTENTIAL_JOB_SITE).copied() else {
                return false;
            };
            if !acquire_poi::closer_to_center_than(pos.pos, tick.mob.get_entity().pos.load(), 2.0) {
                return false;
            }
            tick.state.erase(types::POTENTIAL_JOB_SITE.id());
            tick.state.set(types::JOB_SITE, pos);
            tick.world.send_entity_status(
                tick.mob.get_entity(),
                EntityStatus::VillagerHappy,
                ActorEventID::VillagerHappy,
            );
            let Some(me) = villager(tick.mob) else {
                return true;
            };
            if me.profession() != VillagerProfession::None {
                return true;
            }
            let kind = tick
                .world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kind_at(&pos.pos);
            if let Some(PoiKind::Job(profession)) = kind {
                let data = me.villager_data();
                me.set_villager_data(VillagerData::new(
                    data.type_enum(),
                    profession,
                    data.level.0,
                ));
                me.request_brain_refresh();
            }
            true
        },
    )
}

/// Vanilla `ResetProfession.create`: a villager that never traded loses its profession with its
/// job site.
fn reset_profession() -> OneShot {
    OneShot::new(
        "ResetProfession",
        vec![(types::JOB_SITE.id(), MemoryStatus::ValueAbsent)],
        |tick| {
            let Some(me) = villager(tick.mob) else {
                return false;
            };
            let data = me.villager_data();
            let profession = data.profession_enum();
            let can_be_fired =
                profession != VillagerProfession::None && profession != VillagerProfession::Nitwit;
            if can_be_fired && me.xp.load(Ordering::Relaxed) == 0 && data.level.0 <= 1 {
                me.set_villager_data(VillagerData::new(
                    data.type_enum(),
                    VillagerProfession::None,
                    data.level.0,
                ));
                me.request_brain_refresh();
                true
            } else {
                false
            }
        },
    )
}

/// Vanilla `LookAndFollowTradingPlayerSink`.
struct LookAndFollowTradingPlayerSink {
    speed_modifier: f32,
}

const WALK_AND_LOOK_REGISTERED: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::WALK_TARGET.id(), MemoryStatus::Registered),
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
];

impl LookAndFollowTradingPlayerSink {
    fn trading_player(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
        let living = &tick.mob.get_mob_entity().living_entity;
        let body = &living.entity;
        let player = tick.mob.get_trading_player()?;
        (body.is_alive()
            && !body.touching_water.load(Ordering::Relaxed)
            && living.hurt_cooldown.load(Ordering::Relaxed) <= 0
            && body
                .pos
                .load()
                .squared_distance_to_vec(&player.living_entity.entity.pos.load())
                <= 16.0)
            .then_some(player as Arc<dyn EntityBase>)
    }

    fn follow(&self, tick: &mut BrainTick<'_>) {
        if let Some(player) = Self::trading_player(tick) {
            tick.state.set(
                types::WALK_TARGET,
                WalkTarget::from_entity(player.clone(), self.speed_modifier, 2),
            );
            utils::look_at_entity(tick.state, player);
        }
    }
}

impl Behavior for LookAndFollowTradingPlayerSink {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &WALK_AND_LOOK_REGISTERED
    }

    // Vanilla `timedOut` is always false.
    fn min_duration(&self) -> i32 {
        i32::MAX
    }

    fn max_duration(&self) -> i32 {
        i32::MAX
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        Self::trading_player(tick).is_some()
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        Self::trading_player(tick).is_some()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        self.follow(tick);
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        self.follow(tick);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        tick.state.erase(types::WALK_TARGET.id());
        tick.state.erase(types::LOOK_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "LookAndFollowTradingPlayerSink"
    }
}

/// Vanilla `SocializeAtBell.SPEED_MODIFIER`.
const SOCIALIZE_SPEED_MODIFIER: f32 = 0.3;

/// Vanilla `SocializeAtBell.create`: now and then, walks up to a villager near the bell.
fn socialize_at_bell() -> OneShot {
    OneShot::new(
        "SocializeAtBell",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::MEETING_POINT.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
            (types::INTERACTION_TARGET.id(), MemoryStatus::ValueAbsent),
        ],
        |tick| {
            let Some(meeting) = tick.state.get(types::MEETING_POINT).copied() else {
                return false;
            };
            let body = tick.mob.get_entity().pos.load();
            if tick.mob.get_random().random_range(0..100) != 0
                || meeting.dimension.id != tick.world.dimension.id
                || !acquire_poi::closer_to_center_than(meeting.pos, body, 4.0)
            {
                return false;
            }
            let is_villager = |entity: &Arc<dyn EntityBase>| {
                entity.get_entity().entity_type == &EntityType::VILLAGER
            };
            let target = {
                let ctx = tick.visibility();
                let Some(visible) = ctx.state.get(types::NEAREST_VISIBLE_LIVING_ENTITIES) else {
                    return false;
                };
                if !visible.contains_any(&ctx, is_villager) {
                    return false;
                }
                visible.find_closest(&ctx, |entity| {
                    is_villager(entity)
                        && entity
                            .get_entity()
                            .pos
                            .load()
                            .squared_distance_to_vec(&body)
                            <= 32.0
                })
            };
            if let Some(target) = target {
                tick.state.set(types::INTERACTION_TARGET, target.clone());
                utils::look_at_entity(tick.state, target.clone());
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::new(
                        Arc::new(EntityTracker::new(target, false)),
                        SOCIALIZE_SPEED_MODIFIER,
                        1,
                    ),
                );
            }
            true
        },
    )
}

/// Vanilla `PlayTagWithOtherKids`' speeds and chaser limit.
const PLAY_SPEED_MODIFIER: f32 = 0.6;
const MAX_CHASERS_PER_TARGET: usize = 5;

/// Vanilla `PlayTagWithOtherKids.create`: baby villagers chase each other around the village.
fn play_tag_with_other_kids() -> OneShot {
    OneShot::new(
        "PlayTagWithOtherKids",
        vec![
            (
                types::VISIBLE_VILLAGER_BABIES.id(),
                MemoryStatus::ValuePresent,
            ),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::INTERACTION_TARGET.id(), MemoryStatus::Registered),
        ],
        |tick| {
            if tick.mob.get_random().random_range(0..10) != 0 {
                return false;
            }
            let friends: Vec<Arc<dyn EntityBase>> = tick
                .state
                .get(types::VISIBLE_VILLAGER_BABIES)
                .cloned()
                .unwrap_or_default();
            let my_id = tick.mob.get_entity().entity_id;
            let chasing = |friend: &Arc<dyn EntityBase>| {
                villager_of(friend).and_then(VillagerEntity::interaction_target_id)
            };
            if friends.iter().any(|friend| chasing(friend) == Some(my_id)) {
                let sections: VillageSections = village_stroll::village_sections(tick);
                for _ in 0..10 {
                    if let Some(pos) = land_random_pos::get_pos(tick.mob, 20, 8)
                        && sections
                            .is_village(pumpkin_util::math::position::BlockPos::floored_v(pos))
                    {
                        tick.state.set(
                            types::WALK_TARGET,
                            WalkTarget::from_vec(pos, PLAY_SPEED_MODIFIER, 0),
                        );
                        break;
                    }
                }
                return true;
            }
            let mut chased: Vec<(i32, usize)> = Vec::new();
            for target in friends.iter().filter_map(chasing) {
                match chased.iter_mut().find(|(id, _)| *id == target) {
                    Some((_, count)) => *count += 1,
                    None => chased.push((target, 1)),
                }
            }
            chased.sort_by_key(|(_, count)| *count);
            let being_chased = chased
                .iter()
                .find(|(_, count)| (1..=MAX_CHASERS_PER_TARGET).contains(count))
                .and_then(|(id, _)| tick.world.get_entity_by_id(*id));
            let kid = being_chased.or_else(|| friends.choose(&mut tick.mob.get_random()).cloned());
            if let Some(kid) = kid {
                tick.state.set(types::INTERACTION_TARGET, kid.clone());
                utils::look_at_entity(tick.state, kid.clone());
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::new(
                        Arc::new(EntityTracker::new(kid, false)),
                        PLAY_SPEED_MODIFIER,
                        1,
                    ),
                );
            }
            true
        },
    )
}

/// Vanilla `WorkAtPoi.CHECK_COOLDOWN`.
const WORK_CHECK_COOLDOWN: i64 = 300;
/// Vanilla `WorkAtPoi.DISTANCE`.
const WORK_DISTANCE: f64 = 1.73;

const WORK_AT_POI_ENTRY: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::JOB_SITE.id(), MemoryStatus::ValuePresent),
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
];

/// Vanilla `WorkAtPoi`: at the job site, plays the work sound and restocks.
#[derive(Default)]
struct WorkAtPoi {
    last_check: i64,
}

fn near_job_site(tick: &BrainTick<'_>) -> bool {
    tick.state.get(types::JOB_SITE).is_some_and(|target| {
        target.dimension.id == tick.world.dimension.id
            && acquire_poi::closer_to_center_than(
                target.pos,
                tick.mob.get_entity().pos.load(),
                WORK_DISTANCE,
            )
    })
}

impl Behavior for WorkAtPoi {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &WORK_AT_POI_ENTRY
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        if tick.time - self.last_check < WORK_CHECK_COOLDOWN
            || tick.mob.get_random().random_range(0..2) != 0
        {
            return false;
        }
        self.last_check = tick.time;
        near_job_site(tick)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.state.set(types::LAST_WORKED_AT_POI, tick.time);
        if let Some(pos) = tick.state.get(types::JOB_SITE).map(|pos| pos.pos) {
            tick.state.set(
                types::LOOK_TARGET,
                Arc::new(BlockPosTracker::new(pos)) as Arc<dyn PositionTracker>,
            );
        }
        if let Some(me) = villager(tick.mob) {
            me.work_at_poi(tick.world);
        }
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        near_job_site(tick)
    }

    fn debug_name(&self) -> &'static str {
        "WorkAtPoi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unemployed_villagers_may_take_any_job_site_and_nitwits_none() {
        let lectern = PoiKind::Job(VillagerProfession::Librarian);
        assert!(acquirable_job_site(VillagerProfession::None, lectern));
        assert!(!held_job_site(VillagerProfession::None, lectern));
        assert!(!acquirable_job_site(VillagerProfession::Nitwit, lectern));
        assert!(acquirable_job_site(VillagerProfession::Librarian, lectern));
        assert!(held_job_site(VillagerProfession::Librarian, lectern));
        assert!(!acquirable_job_site(VillagerProfession::Farmer, lectern));
        assert!(!acquirable_job_site(
            VillagerProfession::None,
            PoiKind::Home
        ));
    }
}
