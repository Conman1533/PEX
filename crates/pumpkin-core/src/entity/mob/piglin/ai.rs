//! Vanilla `PiglinAi`: the piglin's brain and the item, barter and anger rules around it.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::data_component_impl::{BlocksAttacksImpl, EquipmentSlot};
use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::PiglinEntity;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick,
    activity::ActivityData,
    behavior::{
        DoNothing, LookAtTargetSink, MoveToTargetSink, OneShot, Timed, back_up_if_too_close,
        copy_memory_with_expiry, crossbow_attack::CrossbowAttack, dismount_or_skip_mounting,
        erase_memory_if, go_to_target_location, go_to_wanted_item, interact_with,
        interact_with_door, melee_attack, mount, one_shot, random_stroll, run_one,
        set_entity_look_target, set_look_and_interact, set_walk_target_away_from,
        set_walk_target_from_attack_target_if_target_out_of_reach,
        set_walk_target_from_look_target, start_attacking, start_celebrating_if_target_dead,
        stop_attacking_if_target_invalid, stop_being_angry_if_target_dead, utils,
    },
    memory::{MemoryModuleId, MemoryStatus, PackedMemories, types},
    sensing::{SensorType, is_entity_attackable, is_entity_attackable_ignoring_line_of_sight},
};
use crate::entity::ai::random_pos::land_random_pos;
use crate::entity::item::ItemEntity;
use crate::entity::living::LivingEntity;
use crate::entity::mob::Mob;
use crate::entity::mob::abstract_piglin::{self, Broadcast};
use crate::entity::player::Player;
use crate::entity::{Entity, EntityBase};
use crate::world::World;
use crate::world::loot::LootContextParameters;

pub const REPELLENT_DETECTION_RANGE_HORIZONTAL: i32 = 8;
pub const REPELLENT_DETECTION_RANGE_VERTICAL: i32 = 4;
pub const BARTERING_ITEM: &Item = &Item::GOLD_INGOT;
const PLAYER_ANGER_RANGE: f64 = 16.0;
const ADMIRE_DURATION: i64 = 119;
const MAX_DISTANCE_TO_WALK_TO_ITEM: i32 = 9;
const MAX_TIME_TO_WALK_TO_ITEM: i32 = 200;
const HOW_LONG_TIME_TO_DISABLE_ADMIRE_WALKING_IF_CANT_REACH_ITEM: i64 = 200;
const CELEBRATION_TIME: i64 = 300;
const BABY_FLEE_DURATION_AFTER_GETTING_HIT: i64 = 100;
const HIT_BY_PLAYER_MEMORY_TIMEOUT: i64 = 400;
const MAX_WALK_DISTANCE_TO_START_RIDING: i32 = 8;
/// Vanilla `RIDE_START_INTERVAL`: 10 to 40 seconds.
const RIDE_START_INTERVAL: (i32, i32) = (200, 800);
/// Vanilla `RIDE_DURATION`: 10 to 30 seconds.
const RIDE_DURATION: (i32, i32) = (200, 600);
/// Vanilla `RETREAT_DURATION`: 5 to 20 seconds.
const RETREAT_DURATION: (i64, i64) = (100, 400);
const MELEE_ATTACK_COOLDOWN: i32 = 20;
const EAT_COOLDOWN: i64 = 200;
const DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING: i32 = 12;
const MAX_LOOK_DIST: f32 = 8.0;
const MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM: f32 = 14.0;
const INTERACTION_RANGE: i32 = 8;
const MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW: i32 = 5;
const SPEED_WHEN_STRAFING_BACK_FROM_TARGET: f32 = 0.75;
const DESIRED_DISTANCE_FROM_ZOMBIFIED: f64 = 6.0;
/// Vanilla `AVOID_ZOMBIFIED_DURATION` and `BABY_AVOID_NEMESIS_DURATION`: 5 to 7 seconds.
const AVOID_DURATION: (i32, i32) = (100, 140);
const PROBABILITY_OF_CELEBRATION_DANCE: f32 = 0.1;
const SPEED_MULTIPLIER_WHEN_AVOIDING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_MOUNTING: f32 = 0.8;
const SPEED_MULTIPLIER_WHEN_GOING_TO_WANTED_ITEM: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_GOING_TO_CELEBRATE_LOCATION: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_DANCING: f32 = 0.6;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.6;
const THROW_SPEED: f64 = 0.3;
const THROW_HAND_Y_DISTANCE_FROM_EYE: f64 = 0.3;

const MEMORIES: [MemoryModuleId; 6] = [
    types::UNIVERSAL_ANGER.id(),
    types::ATE_RECENTLY.id(),
    types::SPEAR_FLEEING_TIME.id(),
    types::SPEAR_FLEEING_POSITION.id(),
    types::SPEAR_CHARGE_POSITION.id(),
    types::SPEAR_ENGAGE_TIME.id(),
];

/// Vanilla `Piglin.BRAIN_PROVIDER`.
pub(super) fn make_brain(piglin: &PiglinEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &MEMORIES,
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::NearestPlayers,
            SensorType::NearestItems,
            SensorType::HurtBy,
            SensorType::PiglinSpecific,
        ],
        activities,
    }
    .make_brain(piglin, packed)
}

/// Vanilla `PiglinAi.initMemories`: the first hunt waits 30 to 120 seconds.
pub(super) fn init_memories(piglin: &PiglinEntity) {
    piglin.mob_entity.try_with_brain(piglin, |brain| {
        abstract_piglin::dont_kill_any_more_hoglins_for_a_while(brain, piglin);
    });
}

fn piglin(mob: &dyn Mob) -> Option<&PiglinEntity> {
    mob.cast_any().downcast_ref::<PiglinEntity>()
}

#[expect(
    clippy::too_many_lines,
    reason = "vanilla PiglinAi's seven activities, kept in one table"
)]
fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(interact_with_door::create()),
                Box::new(copy_memory_with_expiry::create(
                    |tick| utils::is_baby(tick.mob),
                    types::NEAREST_VISIBLE_NEMESIS,
                    types::AVOID_TARGET,
                    AVOID_DURATION.0,
                    AVOID_DURATION.1,
                )),
                Box::new(copy_memory_with_expiry::create(
                    |tick| is_near_zombified(tick.state, tick.mob),
                    types::NEAREST_VISIBLE_ZOMBIFIED,
                    types::AVOID_TARGET,
                    AVOID_DURATION.0,
                    AVOID_DURATION.1,
                )),
                Box::new(stop_holding_item_if_no_longer_admiring()),
                Box::new(start_admiring_item_if_seen(ADMIRE_DURATION)),
                Box::new(start_celebrating_if_target_dead::create(
                    CELEBRATION_TIME,
                    wants_to_dance,
                )),
                Box::new(stop_being_angry_if_target_dead::create()),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(look_at_player_holding_loved_item(
                    MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM,
                )),
                Box::new(start_attacking_if_adult()),
                Box::new(one_shot::sequence(
                    "StartHuntingHoglin",
                    |tick| abstract_piglin::can_hunt(tick.mob),
                    start_hunting_hoglin(),
                )),
                Box::new(avoid_repellent()),
                Box::new(baby_sometimes_ride_baby_hoglin()),
                Box::new(idle_look_behaviors()),
                Box::new(idle_movement_behaviors()),
                Box::new(set_look_and_interact::create(&EntityType::PLAYER, 4)),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::AdmireItem,
            10,
            vec![
                Box::new(go_to_wanted_item::create_if(
                    |tick| is_not_holding_loved_item_in_off_hand(tick.mob),
                    SPEED_MULTIPLIER_WHEN_GOING_TO_WANTED_ITEM,
                    true,
                    MAX_DISTANCE_TO_WALK_TO_ITEM,
                )),
                Box::new(stop_admiring_if_item_too_far_away(
                    MAX_DISTANCE_TO_WALK_TO_ITEM,
                )),
                Box::new(stop_admiring_if_tired_of_trying_to_reach_item(
                    MAX_TIME_TO_WALK_TO_ITEM,
                    HOW_LONG_TIME_TO_DISABLE_ADMIRE_WALKING_IF_CANT_REACH_ITEM,
                )),
            ],
            types::ADMIRING_ITEM.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Fight,
            10,
            vec![
                Box::new(stop_attacking_if_target_invalid::create_when(
                    |tick, target| !is_nearest_valid_attack_target(tick, target),
                )),
                Box::new(one_shot::sequence(
                    "BackUpIfTooClose",
                    |tick| has_crossbow(tick.mob),
                    back_up_if_too_close::create(
                        MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW,
                        SPEED_WHEN_STRAFING_BACK_FROM_TARGET,
                    ),
                )),
                Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
                // Vanilla's SpearApproach, SpearAttack and SpearRetreat wait for a mob kinetic
                // weapon attack (G1-06); until then a spear piglin fights in melee.
                Box::new(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
                Box::new(Timed::new(CrossbowAttack::default())),
                Box::new(remember_if_hoglin_was_killed()),
                Box::new(erase_memory_if::create(
                    |tick| is_near_zombified(tick.state, tick.mob),
                    types::ATTACK_TARGET.id(),
                )),
            ],
            types::ATTACK_TARGET.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Celebrate,
            10,
            vec![
                Box::new(avoid_repellent()),
                Box::new(look_at_player_holding_loved_item(
                    MAX_LOOK_DIST_FOR_PLAYER_HOLDING_LOVED_ITEM,
                )),
                Box::new(start_attacking_if_adult()),
                Box::new(one_shot::sequence(
                    "GoToTargetLocation",
                    |tick| !is_dancing(tick.mob),
                    go_to_target_location::create(
                        types::CELEBRATE_LOCATION,
                        2,
                        SPEED_MULTIPLIER_WHEN_GOING_TO_CELEBRATE_LOCATION,
                    ),
                )),
                Box::new(one_shot::sequence(
                    "GoToTargetLocation",
                    |tick| is_dancing(tick.mob),
                    go_to_target_location::create(
                        types::CELEBRATE_LOCATION,
                        4,
                        SPEED_MULTIPLIER_WHEN_DANCING,
                    ),
                )),
                Box::new(run_one(vec![
                    (
                        Box::new(set_entity_look_target::create_for_type(
                            &EntityType::PIGLIN,
                            MAX_LOOK_DIST,
                        )),
                        1,
                    ),
                    (
                        Box::new(random_stroll::stroll_range(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            2,
                            1,
                        )),
                        1,
                    ),
                    (Box::new(DoNothing::new(10, 20)), 1),
                ])),
            ],
            types::CELEBRATE_LOCATION.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Avoid,
            10,
            vec![
                Box::new(set_walk_target_away_from::entity(
                    types::AVOID_TARGET,
                    SPEED_MULTIPLIER_WHEN_AVOIDING,
                    DESIRED_DISTANCE_FROM_ENTITY_WHEN_AVOIDING,
                    true,
                )),
                Box::new(idle_look_behaviors()),
                Box::new(idle_movement_behaviors()),
                Box::new(erase_memory_if::create(
                    wants_to_stop_fleeing,
                    types::AVOID_TARGET.id(),
                )),
            ],
            types::AVOID_TARGET.id(),
        ),
        ActivityData::with_gate_memory(
            Activity::Ride,
            10,
            vec![
                Box::new(mount::create(SPEED_MULTIPLIER_WHEN_MOUNTING)),
                Box::new(look_at_player_holding_loved_item(MAX_LOOK_DIST)),
                Box::new(one_shot::sequence(
                    "RideLookAround",
                    |tick| tick.mob.get_entity().has_vehicle(),
                    one_shot::trigger_one_shuffled(
                        "TriggerGate",
                        vec![
                            (
                                set_entity_look_target::create_for_type(
                                    &EntityType::PLAYER,
                                    MAX_LOOK_DIST,
                                )
                                .into_trigger(),
                                1,
                            ),
                            (
                                set_entity_look_target::create_for_type(
                                    &EntityType::PIGLIN,
                                    MAX_LOOK_DIST,
                                )
                                .into_trigger(),
                                1,
                            ),
                            (
                                set_entity_look_target::create(MAX_LOOK_DIST).into_trigger(),
                                1,
                            ),
                            (Box::new(|_: &mut BrainTick<'_>| true), 1),
                        ],
                    ),
                )),
                Box::new(dismount_or_skip_mounting::create(
                    MAX_WALK_DISTANCE_TO_START_RIDING,
                    wants_to_stop_riding,
                )),
            ],
            types::RIDE_TARGET.id(),
        ),
    ]
}

fn look_at_player_holding_loved_item(max_dist: f32) -> OneShot {
    set_entity_look_target::create_with(
        |entity| {
            entity
                .cast_any()
                .downcast_ref::<Player>()
                .is_some_and(is_player_holding_loved_item)
        },
        max_dist,
    )
}

fn start_attacking_if_adult() -> OneShot {
    start_attacking::create_with(
        |tick| !utils::is_baby(tick.mob),
        find_nearest_valid_attack_target,
    )
}

/// Vanilla `PiglinAi.createIdleLookBehaviors`.
fn idle_look_behaviors() -> crate::entity::ai::brain::behavior::GateBehavior {
    run_one(vec![
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
        (Box::new(set_entity_look_target::create(MAX_LOOK_DIST)), 1),
        (Box::new(DoNothing::new(30, 60)), 1),
    ])
}

/// Vanilla `PiglinAi.createIdleMovementBehaviors`.
fn idle_movement_behaviors() -> crate::entity::ai::brain::behavior::GateBehavior {
    run_one(vec![
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
            Box::new(one_shot::sequence(
                "SetWalkTargetFromLookTarget",
                |tick| !sees_player_holding_loved_item(tick.state),
                set_walk_target_from_look_target::create(SPEED_MULTIPLIER_WHEN_IDLING, 3),
            )),
            2,
        ),
        (Box::new(DoNothing::new(30, 60)), 1),
    ])
}

/// Vanilla `PiglinAi.avoidRepellent`.
fn avoid_repellent() -> OneShot {
    set_walk_target_away_from::pos(
        types::NEAREST_REPELLENT,
        1.0,
        REPELLENT_DETECTION_RANGE_HORIZONTAL,
        false,
    )
}

/// Vanilla `PiglinAi.babySometimesRideBabyHoglin`, with `SetEntityLookTargetSometimes.Ticker`.
fn baby_sometimes_ride_baby_hoglin() -> OneShot {
    let ticks_until_next_start = AtomicI32::new(0);
    copy_memory_with_expiry::create(
        move |tick| {
            if !utils::is_baby(tick.mob) {
                return false;
            }
            let left = ticks_until_next_start.load(Ordering::Relaxed);
            if left == 0 {
                let next = tick
                    .mob
                    .get_random()
                    .random_range(RIDE_START_INTERVAL.0..=RIDE_START_INTERVAL.1);
                ticks_until_next_start.store(next - 1, Ordering::Relaxed);
                false
            } else {
                ticks_until_next_start.store(left - 1, Ordering::Relaxed);
                left - 1 == 0
            }
        },
        types::NEAREST_VISIBLE_BABY_HOGLIN,
        types::RIDE_TARGET,
        RIDE_DURATION.0,
        RIDE_DURATION.1,
    )
}

/// Vanilla `PiglinAi.updateActivity`, plus the mirror of `attack_target` into the mob's target
/// (vanilla `getTarget` reads the brain).
pub(super) fn update_activity(piglin: &PiglinEntity) {
    let (changed, sound, dancing, target) = piglin.mob_entity.with_brain(piglin, |brain| {
        let old_activity = brain.get_active_non_core_activity();
        brain.set_active_activity_to_first_valid(&[
            Activity::AdmireItem,
            Activity::Fight,
            Activity::Avoid,
            Activity::Celebrate,
            Activity::Ride,
            Activity::Idle,
        ]);
        let new_activity = brain.get_active_non_core_activity();
        if !brain.has_memory_value(types::CELEBRATE_LOCATION.id()) {
            brain.erase(types::DANCING.id());
        }
        (
            old_activity != new_activity,
            sound_for_current_activity(brain, piglin),
            brain.has_memory_value(types::DANCING.id()),
            (
                brain.get(types::ATTACK_TARGET).cloned(),
                brain.has_memory_value(types::RIDE_TARGET.id()),
            ),
        )
    });
    if changed && let Some(sound) = sound {
        piglin.make_sound(sound);
    }
    let (target, has_ride_target) = target;
    piglin.mob_entity.set_attacking(target.is_some());
    piglin.mob_entity.set_target(target);
    if !has_ride_target && is_baby_riding_baby(piglin) {
        piglin.stop_riding();
    }
    if piglin.is_dancing() != dancing {
        piglin.set_dancing(dancing);
    }
}

fn is_baby_riding_baby(piglin: &PiglinEntity) -> bool {
    if !piglin.is_baby() {
        return false;
    }
    piglin
        .mob_entity
        .living_entity
        .entity
        .get_vehicle()
        .is_some_and(|vehicle| {
            let vehicle_type = vehicle.get_entity().entity_type;
            (vehicle_type == &EntityType::PIGLIN || vehicle_type == &EntityType::HOGLIN)
                && utils::is_baby(vehicle.as_ref())
        })
}

/// Vanilla `PiglinAi.pickUpItem`.
pub(super) fn pick_up_item(brain: &mut BrainState, piglin: &PiglinEntity, item: &ItemEntity) {
    stop_walking(brain, piglin);
    let count = {
        let stack = item
            .get_item_stack()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Nuggets are taken whole, everything else one at a time.
        if stack.item.id == Item::GOLD_NUGGET.id {
            stack.item_count
        } else {
            stack.item_count.min(1)
        }
    };
    if count == 0
        || !piglin
            .mob_entity
            .living_entity
            .pickup(item.get_entity(), u32::from(count))
    {
        return;
    }
    let taken = item
        .get_item_stack()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .split(count);
    let emptied = item
        .get_item_stack()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty();
    if emptied {
        item.get_entity().remove();
    } else {
        item.init_data_tracker();
    }

    if is_loved_item(&taken) {
        brain.erase(types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id());
        piglin.hold_in_off_hand(taken);
        admire_gold_item(brain);
    } else if is_food(&taken) && !brain.has_memory_value(types::ATE_RECENTLY.id()) {
        brain.set_with_expiry(types::ATE_RECENTLY, true, EAT_COOLDOWN);
    } else if crate::entity::mob::equipment::equip_item_if_possible(piglin, taken.clone())
        .is_empty()
    {
        put_in_inventory(brain, piglin, taken);
    }
}

/// Vanilla `PiglinAi.stopHoldingOffHandItem`: an adult barters a gold ingot (when
/// `bartering_enabled`) or keeps the item; a baby keeps it in its main hand.
pub(super) fn stop_holding_off_hand_item(
    brain: &BrainState,
    piglin: &PiglinEntity,
    bartering_enabled: bool,
) {
    let item = piglin
        .mob_entity
        .set_item_slot(&EquipmentSlot::OFF_HAND, ItemStack::EMPTY.clone());
    if piglin.is_adult() {
        let is_barter = is_barter_currency(&item);
        if bartering_enabled && is_barter {
            let outcomes = get_barter_response_items(piglin);
            let entity = &piglin.mob_entity.living_entity.entity;
            let mut event =
                crate::plugin::api::events::entity::piglin_barter::PiglinBarterEvent::new(
                    entity.entity_id,
                    item,
                    outcomes,
                );
            if let Some(server) = entity.world.load().server.upgrade() {
                server.plugin_manager.fire_blocking(&server, &mut event);
            }
            if !event.cancelled {
                throw_items(brain, piglin, event.outcome);
            }
        } else if !is_barter
            && crate::entity::mob::equipment::equip_item_if_possible(piglin, item.clone())
                .is_empty()
        {
            put_in_inventory(brain, piglin, item);
        }
    } else if crate::entity::mob::equipment::equip_item_if_possible(piglin, item.clone()).is_empty()
    {
        let main_hand = piglin.main_hand_item();
        if is_loved_item(&main_hand) {
            put_in_inventory(brain, piglin, main_hand);
        } else {
            throw_items(brain, piglin, vec![main_hand]);
        }
        piglin.hold_in_main_hand(item);
    }
}

/// Vanilla `PiglinAi.cancelAdmiring`: a converting piglin drops what it was admiring.
pub(super) fn cancel_admiring(brain: &BrainState, piglin: &PiglinEntity) {
    if brain.has_memory_value(types::ADMIRING_ITEM.id()) && piglin.is_holding_item_in_off_hand() {
        piglin
            .mob_entity
            .living_entity
            .entity
            .spawn_at_location(piglin.off_hand_item());
        piglin
            .mob_entity
            .set_item_slot(&EquipmentSlot::OFF_HAND, ItemStack::EMPTY.clone());
    }
}

fn put_in_inventory(brain: &BrainState, piglin: &PiglinEntity, item: ItemStack) {
    let remainder = piglin
        .add_to_inventory(item)
        .unwrap_or_else(|| ItemStack::EMPTY.clone());
    // Vanilla swings the off hand here even when nothing was left over.
    let _ = brain;
    throw_items_toward_random_pos(piglin, vec![remainder]);
}

fn throw_items(brain: &BrainState, piglin: &PiglinEntity, items: Vec<ItemStack>) {
    match brain.get(types::NEAREST_VISIBLE_PLAYER) {
        Some(player) => {
            let target = player.get_entity().pos.load();
            throw_items_toward_pos(piglin, items, target);
        }
        None => throw_items_toward_random_pos(piglin, items),
    }
}

fn throw_items_toward_random_pos(piglin: &PiglinEntity, items: Vec<ItemStack>) {
    let target = land_random_pos(piglin, 4, 2)
        .unwrap_or_else(|| piglin.mob_entity.living_entity.entity.pos.load());
    throw_items_toward_pos(piglin, items, target);
}

/// Vanilla `PiglinAi.throwItemsTowardPos` with `BehaviorUtils.throwItem`.
fn throw_items_toward_pos(piglin: &PiglinEntity, items: Vec<ItemStack>, target: Vector3<f64>) {
    if items.is_empty() {
        return;
    }
    let living = &piglin.mob_entity.living_entity;
    living.swing_off_hand();

    let entity = &living.entity;
    let world = entity.world.load();
    let pos = entity.pos.load();
    let hand_pos = Vector3::new(
        pos.x,
        entity.get_eye_y() - THROW_HAND_Y_DISTANCE_FROM_EYE,
        pos.z,
    );
    let direction =
        Vector3::new(target.x - pos.x, target.y + 1.0 - pos.y, target.z - pos.z).normalize();
    let velocity = Vector3::new(
        direction.x * THROW_SPEED,
        direction.y * THROW_SPEED,
        direction.z * THROW_SPEED,
    );
    for item in items {
        if item.is_empty() {
            continue;
        }
        let item_entity = ItemEntity::new_with_velocity(
            Entity::new(world.clone(), hand_pos, &EntityType::ITEM),
            item,
            velocity,
            ItemEntity::DEFAULT_PICKUP_DELAY,
        );
        world.spawn_entity(Arc::new(item_entity));
    }
}

/// Vanilla `PiglinAi.getBarterResponseItems`.
///
/// A roll of the `gameplay/piglin_bartering` loot table with the piglin as `this_entity`, so a datapack's table applies and the books, boots
/// and potions carry their enchantments and effects.
#[must_use]
pub fn get_barter_response_items(piglin: &PiglinEntity) -> Vec<ItemStack> {
    let entity = &piglin.mob_entity.living_entity.entity;
    let world = entity.world.load_full();
    let Some(table) = world.get_loot_table("minecraft:gameplay/piglin_bartering") else {
        return Vec::new();
    };
    table.generate_loot_with_context(
        rand::random::<i64>(),
        &LootContextParameters {
            world: Some(world.as_ref()),
            this_entity: Some(piglin as &dyn EntityBase),
            position: Some(entity.pos.load()),
            ..Default::default()
        },
    )
}

/// Vanilla `PiglinAi.wantsToDance`: one in ten piglins dance after a hoglin kill.
fn wants_to_dance(tick: &BrainTick<'_>, killed_target: &Arc<dyn EntityBase>) -> bool {
    killed_target.get_entity().entity_type == &EntityType::HOGLIN
        && tick.mob.get_random().random::<f32>() < PROBABILITY_OF_CELEBRATION_DANCE
}

/// Vanilla `PiglinAi.wantsToPickup`.
#[must_use]
pub(super) fn wants_to_pickup(
    brain: &BrainState,
    piglin: &PiglinEntity,
    stack: &ItemStack,
) -> bool {
    if piglin.is_baby()
        && stack
            .item
            .has_tag(&tag::Item::MINECRAFT_IGNORED_BY_PIGLIN_BABIES)
    {
        return false;
    }
    if stack.item.has_tag(&tag::Item::MINECRAFT_PIGLIN_REPELLENTS) {
        return false;
    }
    if brain.has_memory_value(types::ADMIRING_DISABLED.id())
        && brain.has_memory_value(types::ATTACK_TARGET.id())
    {
        return false;
    }
    if is_barter_currency(stack) {
        return is_not_holding_loved_item_in_off_hand(piglin);
    }
    let has_space = piglin.can_add_to_inventory(stack);
    if stack.item.id == Item::GOLD_NUGGET.id {
        return has_space;
    }
    if is_food(stack) {
        return !brain.has_memory_value(types::ATE_RECENTLY.id()) && has_space;
    }
    if !is_loved_item(stack) {
        return piglin.can_replace_current_item_for(stack);
    }
    is_not_holding_loved_item_in_off_hand(piglin) && has_space
}

#[must_use]
pub fn is_loved_item(stack: &ItemStack) -> bool {
    stack.item.has_tag(&tag::Item::MINECRAFT_PIGLIN_LOVED)
}

#[must_use]
pub const fn is_barter_currency(stack: &ItemStack) -> bool {
    stack.item.id == BARTERING_ITEM.id
}

#[must_use]
pub fn is_food(stack: &ItemStack) -> bool {
    stack.item.has_tag(&tag::Item::MINECRAFT_PIGLIN_FOOD)
}

/// Vanilla `PiglinAi.isZombified`.
#[must_use]
pub fn is_zombified(entity_type: &EntityType) -> bool {
    entity_type == &EntityType::ZOMBIFIED_PIGLIN || entity_type == &EntityType::ZOGLIN
}

/// Vanilla `PiglinAi.wantsToStopRiding`.
fn wants_to_stop_riding(tick: &BrainTick<'_>, vehicle: &Arc<dyn EntityBase>) -> bool {
    let Some(vehicle_mob) = vehicle.get_mob() else {
        return false;
    };
    let vehicle_hurt_recently = vehicle_mob
        .get_mob_entity()
        .try_with_brain(vehicle_mob, |brain| {
            brain.has_memory_value(types::HURT_BY.id())
        })
        .unwrap_or(false);
    !utils::is_baby(vehicle.as_ref())
        || !vehicle.get_entity().is_alive()
        || tick.state.has_memory_value(types::HURT_BY.id())
        || vehicle_hurt_recently
        || (vehicle.get_entity().entity_type == &EntityType::PIGLIN
            && !vehicle.get_entity().has_vehicle())
}

fn is_nearest_valid_attack_target(tick: &BrainTick<'_>, target: &Arc<dyn EntityBase>) -> bool {
    find_nearest_valid_attack_target(tick)
        .is_some_and(|nearest| nearest.get_entity().entity_id == target.get_entity().entity_id)
}

/// Vanilla `PiglinAi.isNearZombified`.
fn is_near_zombified(brain: &BrainState, body: &dyn Mob) -> bool {
    brain
        .get(types::NEAREST_VISIBLE_ZOMBIFIED)
        .is_some_and(|zombified| {
            zombified
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&body.get_entity().pos.load())
                < DESIRED_DISTANCE_FROM_ZOMBIFIED * DESIRED_DISTANCE_FROM_ZOMBIFIED
        })
}

/// Vanilla `PiglinAi.findNearestValidAttackTarget`.
fn find_nearest_valid_attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    if is_near_zombified(tick.state, tick.mob) {
        return None;
    }
    let ctx = tick.visibility();
    if let Some(angry_at) =
        utils::get_living_entity_from_uuid_memory(tick.state, tick.world, types::ANGRY_AT)
        && is_entity_attackable_ignoring_line_of_sight(&ctx, angry_at.as_ref())
    {
        return Some(angry_at);
    }
    if tick.state.has_memory_value(types::UNIVERSAL_ANGER.id())
        && let Some(player) = tick.state.get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
    {
        return Some(player.clone() as Arc<dyn EntityBase>);
    }
    if let Some(nemesis) = tick.state.get(types::NEAREST_VISIBLE_NEMESIS) {
        return Some(nemesis.clone());
    }
    tick.state
        .get(types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD)
        .filter(|player| is_entity_attackable(&ctx, player.as_ref() as &dyn EntityBase))
        .map(|player| player.clone() as Arc<dyn EntityBase>)
}

/// Vanilla `PiglinAi.angerNearbyPiglins`: idle piglins within 16 blocks of `player` (that see the
/// player, when `only_if_they_see_the_player`) get angry at the player.
pub fn anger_nearby_piglins(
    world: &Arc<World>,
    player: &Arc<Player>,
    only_if_they_see_the_player: bool,
) {
    let player_entity = player.get_entity();
    let search = player_entity.bounding_box.load().expand(
        PLAYER_ANGER_RANGE,
        PLAYER_ANGER_RANGE,
        PLAYER_ANGER_RANGE,
    );
    let universal_anger = world.level_info.load().game_rules.universal_anger;
    let target: Arc<dyn EntityBase> = player.clone();
    for entity in world.get_entities_at_box(&search) {
        let Some(piglin) = entity.cast_any().downcast_ref::<PiglinEntity>() else {
            continue;
        };
        piglin.mob_entity.try_with_brain(piglin, |brain| {
            if brain.get_active_non_core_activity() != Some(Activity::Idle) {
                return;
            }
            if only_if_they_see_the_player
                && !utils::can_see(
                    &abstract_piglin::visibility(brain, world, piglin),
                    target.as_ref(),
                )
            {
                return;
            }
            if universal_anger {
                abstract_piglin::set_anger_target_to_nearest_targetable_player_if_found(
                    brain, world, piglin, &target,
                );
            } else {
                abstract_piglin::set_anger_target(brain, world, piglin, &target);
            }
        });
    }
}

/// Vanilla `PiglinAi.mobInteract`: an adult that may admire takes one gold ingot from the
/// player's hand and starts admiring it.
pub(super) fn mob_interact(piglin: &PiglinEntity, player: &Player, stack: &mut ItemStack) -> bool {
    piglin.mob_entity.with_brain(piglin, |brain| {
        if !can_admire(brain, piglin, stack) {
            return false;
        }
        let taken = stack.split_unless_creative(player.gamemode.load(), 1);
        piglin.hold_in_off_hand(taken);
        admire_gold_item(brain);
        stop_walking(brain, piglin);
        true
    })
}

/// Vanilla `PiglinAi.canAdmire`.
fn can_admire(brain: &BrainState, piglin: &PiglinEntity, stack: &ItemStack) -> bool {
    !brain.has_memory_value(types::ADMIRING_DISABLED.id())
        && !brain.has_memory_value(types::ADMIRING_ITEM.id())
        && piglin.is_adult()
        && is_barter_currency(stack)
}

/// Vanilla `PiglinAi.wasHurtBy`.
pub(super) fn was_hurt_by(piglin: &PiglinEntity, attacker: &dyn EntityBase) {
    if attacker.get_living_entity().is_none()
        || attacker.get_entity().entity_type == &EntityType::PIGLIN
    {
        return;
    }
    let world = piglin.mob_entity.living_entity.entity.world.load_full();
    let Some(attacker) = abstract_piglin::resolve_attacker(&world, attacker) else {
        return;
    };
    let attacker_type = attacker.get_entity().entity_type;
    let reaction = piglin.mob_entity.try_with_brain(piglin, |brain| {
        if piglin.is_holding_item_in_off_hand() {
            stop_holding_off_hand_item(brain, piglin, false);
        }
        brain.erase(types::CELEBRATE_LOCATION.id());
        brain.erase(types::DANCING.id());
        brain.erase(types::ADMIRING_ITEM.id());
        if attacker_type == &EntityType::PLAYER {
            brain.set_with_expiry(types::ADMIRING_DISABLED, true, HIT_BY_PLAYER_MEMORY_TIMEOUT);
        }
        if brain
            .get(types::AVOID_TARGET)
            .is_some_and(|avoid| avoid.get_entity().entity_type != attacker_type)
        {
            brain.erase(types::AVOID_TARGET.id());
        }
        if piglin.is_baby() {
            brain.set_with_expiry(
                types::AVOID_TARGET,
                attacker.clone(),
                BABY_FLEE_DURATION_AFTER_GETTING_HIT,
            );
            if is_entity_attackable_ignoring_line_of_sight(
                &abstract_piglin::visibility(brain, &world, piglin),
                attacker.as_ref(),
            ) {
                let adults = brain
                    .get(types::NEARBY_ADULT_PIGLINS)
                    .cloned()
                    .unwrap_or_default();
                return Reaction::Broadcast(Broadcast::Anger(attacker.clone(), adults));
            }
            Reaction::Broadcast(Broadcast::None)
        } else if attacker_type == &EntityType::HOGLIN && hoglins_outnumber_piglins(brain) {
            set_avoid_target_and_dont_hunt_for_a_while(brain, piglin, attacker.clone());
            let visible = brain
                .get(types::NEAREST_VISIBLE_ADULT_PIGLINS)
                .cloned()
                .unwrap_or_default();
            Reaction::Retreat(visible)
        } else {
            Reaction::Broadcast(abstract_piglin::maybe_retaliate(
                brain, &world, piglin, &attacker,
            ))
        }
    });
    match reaction {
        Some(Reaction::Broadcast(broadcast)) => {
            abstract_piglin::run_broadcast(&world, piglin, broadcast);
        }
        Some(Reaction::Retreat(piglins)) => broadcast_retreat(piglin, &piglins, &attacker),
        None => {}
    }
}

/// What a hurt piglin asks of the piglins around it, once its own brain is released.
enum Reaction {
    Broadcast(Broadcast),
    Retreat(Vec<Arc<dyn EntityBase>>),
}

/// Vanilla `PiglinAi.broadcastRetreat`: the adult piglins it sees flee the nearest of the hoglin
/// and their own threats.
fn broadcast_retreat(
    body: &PiglinEntity,
    piglins: &[Arc<dyn EntityBase>],
    target: &Arc<dyn EntityBase>,
) {
    let id = body.mob_entity.living_entity.entity.entity_id;
    for other in piglins {
        if other.get_entity().entity_id == id {
            continue;
        }
        let Some(other) = other.cast_any().downcast_ref::<PiglinEntity>() else {
            continue;
        };
        other.mob_entity.try_with_brain(other, |brain| {
            let nearest = utils::get_nearest_target(
                other,
                brain.get(types::AVOID_TARGET).cloned(),
                target.clone(),
            );
            let nearest =
                utils::get_nearest_target(other, brain.get(types::ATTACK_TARGET).cloned(), nearest);
            set_avoid_target_and_dont_hunt_for_a_while(brain, other, nearest);
        });
    }
}

/// Vanilla `PiglinAi.getSoundForActivity`.
fn sound_for_current_activity(brain: &BrainState, piglin: &PiglinEntity) -> Option<Sound> {
    let activity = brain.get_active_non_core_activity()?;
    let world = piglin.mob_entity.living_entity.entity.world.load();
    Some(if activity == Activity::Fight {
        Sound::EntityPiglinAngry
    } else if piglin.is_converting(&world)
        || (activity == Activity::Avoid && is_near_avoid_target(brain, piglin))
    {
        Sound::EntityPiglinRetreat
    } else if activity == Activity::AdmireItem {
        Sound::EntityPiglinAdmiringItem
    } else if activity == Activity::Celebrate {
        Sound::EntityPiglinCelebrate
    } else if sees_player_holding_loved_item(brain) {
        Sound::EntityPiglinJealous
    } else if brain.has_memory_value(types::NEAREST_REPELLENT.id()) {
        Sound::EntityPiglinRetreat
    } else {
        Sound::EntityPiglinAmbient
    })
}

/// Vanilla `Piglin.getAmbientSound`, played only while idle (`AbstractPiglin.playAmbientSound`).
pub(super) fn ambient_sound(piglin: &PiglinEntity) -> Option<Sound> {
    piglin
        .mob_entity
        .try_with_brain(piglin, |brain| {
            if brain.get_active_non_core_activity() == Some(Activity::Idle) {
                sound_for_current_activity(brain, piglin)
            } else {
                None
            }
        })
        .flatten()
}

fn is_near_avoid_target(brain: &BrainState, piglin: &PiglinEntity) -> bool {
    brain.get(types::AVOID_TARGET).is_some_and(|avoid| {
        avoid
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&piglin.mob_entity.living_entity.entity.pos.load())
            < 12.0 * 12.0
    })
}

/// Vanilla `PiglinAi.isWearingSafeArmor`: any armour piece in `#piglin_safe_armor`.
#[must_use]
pub fn is_wearing_safe_armor(entity: &LivingEntity) -> bool {
    let Ok(equipment) = entity.entity_equipment.try_lock() else {
        return false;
    };
    [
        EquipmentSlot::HEAD,
        EquipmentSlot::CHEST,
        EquipmentSlot::LEGS,
        EquipmentSlot::FEET,
        EquipmentSlot::BODY,
    ]
    .iter()
    .any(|slot| {
        equipment.equipment.get(slot).is_some_and(|stack| {
            !stack.is_empty() && stack.item.has_tag(&tag::Item::MINECRAFT_PIGLIN_SAFE_ARMOR)
        })
    })
}

fn stop_walking(brain: &mut BrainState, piglin: &PiglinEntity) {
    brain.erase(types::WALK_TARGET.id());
    piglin.mob_entity.steered().navigator().stop();
}

/// Vanilla `PiglinAi.wantsToStopFleeing`.
fn wants_to_stop_fleeing(tick: &BrainTick<'_>) -> bool {
    let Some(avoided) = tick.state.get(types::AVOID_TARGET) else {
        return true;
    };
    let avoided_type = avoided.get_entity().entity_type;
    if avoided_type == &EntityType::HOGLIN {
        return !hoglins_outnumber_piglins(tick.state);
    }
    if is_zombified(avoided_type) {
        return !tick
            .state
            .is_entity_value(types::NEAREST_VISIBLE_ZOMBIFIED, avoided.as_ref());
    }
    false
}

fn hoglins_outnumber_piglins(brain: &BrainState) -> bool {
    let piglins = brain
        .get(types::VISIBLE_ADULT_PIGLIN_COUNT)
        .copied()
        .unwrap_or(0)
        + 1;
    let hoglins = brain
        .get(types::VISIBLE_ADULT_HOGLIN_COUNT)
        .copied()
        .unwrap_or(0);
    hoglins > piglins
}

fn set_avoid_target_and_dont_hunt_for_a_while(
    brain: &mut BrainState,
    piglin: &PiglinEntity,
    target: Arc<dyn EntityBase>,
) {
    brain.erase(types::ANGRY_AT.id());
    brain.erase(types::ATTACK_TARGET.id());
    brain.erase(types::WALK_TARGET.id());
    let duration = piglin
        .get_random()
        .random_range(RETREAT_DURATION.0..=RETREAT_DURATION.1);
    brain.set_with_expiry(types::AVOID_TARGET, target, duration);
    abstract_piglin::dont_kill_any_more_hoglins_for_a_while(brain, piglin);
}

fn admire_gold_item(brain: &mut BrainState) {
    brain.set_with_expiry(types::ADMIRING_ITEM, true, ADMIRE_DURATION);
}

fn has_crossbow(mob: &dyn Mob) -> bool {
    let equipment = mob
        .get_mob_entity()
        .living_entity
        .entity_equipment
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    equipment.get(&EquipmentSlot::MAIN_HAND).item.id == Item::CROSSBOW.id
        || equipment.get(&EquipmentSlot::OFF_HAND).item.id == Item::CROSSBOW.id
}

fn is_dancing(mob: &dyn Mob) -> bool {
    piglin(mob).is_some_and(PiglinEntity::is_dancing)
}

fn sees_player_holding_loved_item(brain: &BrainState) -> bool {
    brain.has_memory_value(types::NEAREST_PLAYER_HOLDING_WANTED_ITEM.id())
}

/// Vanilla `PiglinAi.isPlayerHoldingLovedItem`.
#[must_use]
pub fn is_player_holding_loved_item(player: &Player) -> bool {
    let inventory = player.inventory();
    is_loved_item(&inventory.held_item()) || is_loved_item(&inventory.off_hand_item())
}

fn is_not_holding_loved_item_in_off_hand(mob: &dyn Mob) -> bool {
    piglin(mob).is_none_or(|piglin| {
        let off_hand = piglin.off_hand_item();
        off_hand.is_empty() || !is_loved_item(&off_hand)
    })
}

/// Vanilla `StartAdmiringItemIfSeen`.
fn start_admiring_item_if_seen(admire_duration: i64) -> OneShot {
    OneShot::new(
        "StartAdmiringItemIfSeen",
        vec![
            (
                types::NEAREST_VISIBLE_WANTED_ITEM.id(),
                MemoryStatus::ValuePresent,
            ),
            (types::ADMIRING_ITEM.id(), MemoryStatus::ValueAbsent),
            (types::ADMIRING_DISABLED.id(), MemoryStatus::ValueAbsent),
            (
                types::DISABLE_WALK_TO_ADMIRE_ITEM.id(),
                MemoryStatus::ValueAbsent,
            ),
        ],
        move |tick| {
            let loved = tick
                .state
                .get(types::NEAREST_VISIBLE_WANTED_ITEM)
                .and_then(|item| {
                    item.get_item_entity().map(|item| {
                        is_loved_item(
                            &item
                                .get_item_stack()
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        )
                    })
                })
                .unwrap_or(false);
            if !loved {
                return false;
            }
            tick.state
                .set_with_expiry(types::ADMIRING_ITEM, true, admire_duration);
            true
        },
    )
}

/// Vanilla `StopHoldingItemIfNoLongerAdmiring`.
fn stop_holding_item_if_no_longer_admiring() -> OneShot {
    OneShot::new(
        "StopHoldingItemIfNoLongerAdmiring",
        vec![(types::ADMIRING_ITEM.id(), MemoryStatus::ValueAbsent)],
        |tick| {
            let Some(piglin) = piglin(tick.mob) else {
                return false;
            };
            let off_hand = piglin.off_hand_item();
            if off_hand.is_empty() || off_hand.get_data_component::<BlocksAttacksImpl>().is_some() {
                return false;
            }
            stop_holding_off_hand_item(tick.state, piglin, true);
            true
        },
    )
}

/// Vanilla `StartHuntingHoglin`.
fn start_hunting_hoglin() -> OneShot {
    OneShot::new(
        "StartHuntingHoglin",
        vec![
            (
                types::NEAREST_VISIBLE_HUNTABLE_HOGLIN.id(),
                MemoryStatus::ValuePresent,
            ),
            (types::ANGRY_AT.id(), MemoryStatus::ValueAbsent),
            (types::HUNTED_RECENTLY.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_ADULT_PIGLINS.id(),
                MemoryStatus::Registered,
            ),
        ],
        |tick| {
            if utils::is_baby(tick.mob) {
                return false;
            }
            let piglins = tick
                .state
                .get(types::NEAREST_VISIBLE_ADULT_PIGLINS)
                .cloned();
            let someone_hunted_recently = piglins.as_ref().is_some_and(|piglins| {
                piglins.iter().any(|other| {
                    other.get_mob().is_some_and(|mob| {
                        mob.get_mob_entity()
                            .try_with_brain(mob, |brain| {
                                brain.has_memory_value(types::HUNTED_RECENTLY.id())
                            })
                            .unwrap_or(false)
                    })
                })
            });
            if someone_hunted_recently {
                return false;
            }
            let Some(hoglin) = tick
                .state
                .get(types::NEAREST_VISIBLE_HUNTABLE_HOGLIN)
                .cloned()
            else {
                return false;
            };
            abstract_piglin::set_anger_target(tick.state, tick.world, tick.mob, &hoglin);
            abstract_piglin::dont_kill_any_more_hoglins_for_a_while(tick.state, tick.mob);
            let adults = tick
                .state
                .get(types::NEARBY_ADULT_PIGLINS)
                .cloned()
                .unwrap_or_default();
            abstract_piglin::broadcast_anger_target(tick.world, tick.mob, &adults, &hoglin);
            for other in piglins.unwrap_or_default() {
                if let Some(mob) = other.get_mob() {
                    mob.get_mob_entity().try_with_brain(mob, |brain| {
                        abstract_piglin::dont_kill_any_more_hoglins_for_a_while(brain, mob);
                    });
                }
            }
            true
        },
    )
}

/// Vanilla `RememberIfHoglinWasKilled`.
fn remember_if_hoglin_was_killed() -> OneShot {
    OneShot::new(
        "RememberIfHoglinWasKilled",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::HUNTED_RECENTLY.id(), MemoryStatus::Registered),
        ],
        |tick| {
            let killed_hoglin = tick.state.get(types::ATTACK_TARGET).is_some_and(|target| {
                target.get_entity().entity_type == &EntityType::HOGLIN
                    && target
                        .get_living_entity()
                        .is_some_and(LivingEntity::is_dead_or_dying)
            });
            if killed_hoglin {
                abstract_piglin::dont_kill_any_more_hoglins_for_a_while(tick.state, tick.mob);
            }
            true
        },
    )
}

/// Vanilla `StopAdmiringIfItemTooFarAway`.
fn stop_admiring_if_item_too_far_away(max_distance_to_item: i32) -> OneShot {
    let max_sqr = f64::from(max_distance_to_item * max_distance_to_item);
    OneShot::new(
        "StopAdmiringIfItemTooFarAway",
        vec![
            (types::ADMIRING_ITEM.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_WANTED_ITEM.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            if piglin(tick.mob).is_some_and(PiglinEntity::is_holding_item_in_off_hand) {
                return false;
            }
            let position = tick.mob.get_entity().pos.load();
            let close = tick
                .state
                .get(types::NEAREST_VISIBLE_WANTED_ITEM)
                .is_some_and(|item| {
                    item.get_entity()
                        .pos
                        .load()
                        .squared_distance_to_vec(&position)
                        < max_sqr
                });
            if close {
                return false;
            }
            tick.state.erase(types::ADMIRING_ITEM.id());
            true
        },
    )
}

/// Vanilla `StopAdmiringIfTiredOfTryingToReachItem`.
fn stop_admiring_if_tired_of_trying_to_reach_item(
    max_time_to_reach_item: i32,
    disable_time: i64,
) -> OneShot {
    OneShot::new(
        "StopAdmiringIfTiredOfTryingToReachItem",
        vec![
            (types::ADMIRING_ITEM.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_WANTED_ITEM.id(),
                MemoryStatus::ValuePresent,
            ),
            (
                types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id(),
                MemoryStatus::Registered,
            ),
            (
                types::DISABLE_WALK_TO_ADMIRE_ITEM.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            if piglin(tick.mob).is_some_and(PiglinEntity::is_holding_item_in_off_hand) {
                return false;
            }
            match tick
                .state
                .get(types::TIME_TRYING_TO_REACH_ADMIRE_ITEM)
                .copied()
            {
                None => tick.state.set(types::TIME_TRYING_TO_REACH_ADMIRE_ITEM, 0),
                Some(time) if time > max_time_to_reach_item => {
                    tick.state.erase(types::ADMIRING_ITEM.id());
                    tick.state
                        .erase(types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id());
                    tick.state.set_with_expiry(
                        types::DISABLE_WALK_TO_ADMIRE_ITEM,
                        true,
                        disable_time,
                    );
                }
                Some(time) => tick
                    .state
                    .set(types::TIME_TRYING_TO_REACH_ADMIRE_ITEM, time + 1),
            }
            true
        },
    )
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item::Item;
    use pumpkin_util::math::vector3::Vector3;

    use super::get_barter_response_items;
    use crate::entity::mob::piglin::PiglinEntity;
    use crate::world::test_world;

    #[tokio::test]
    async fn bartering_rolls_the_loot_table() {
        let world = test_world();
        let piglin = crate::entity::r#type::from_type(
            &EntityType::PIGLIN,
            Vector3::new(0.5, 64.0, 0.5),
            &world,
            uuid::Uuid::new_v4(),
        );
        let piglin = piglin
            .cast_any()
            .downcast_ref::<PiglinEntity>()
            .expect("a piglin");
        let soul_speed = &pumpkin_data::Enchantment::SOUL_SPEED;
        let (mut books, mut boots, mut potions) = (0, 0, 0);
        for _ in 0..2000 {
            let items = get_barter_response_items(piglin);
            assert_eq!(items.len(), 1, "one stack per barter");
            let item = &items[0];
            assert!(!item.is_empty());
            assert_ne!(item.item, &Item::BOOK, "the book is always enchanted");
            if item.item == &Item::ENCHANTED_BOOK {
                let stored = item
                    .get_data_component::<pumpkin_data::data_component_impl::StoredEnchantmentsImpl>()
                    .expect("stored enchantments");
                assert_eq!(stored.enchantment[0].0.name, soul_speed.name);
                books += 1;
            } else if item.item == &Item::IRON_BOOTS {
                assert!(item.get_enchantment_level(soul_speed) > 0);
                boots += 1;
            } else if item.item == &Item::POTION || item.item == &Item::SPLASH_POTION {
                let potion = item
                    .get_data_component::<pumpkin_data::data_component_impl::PotionContentsImpl>()
                    .and_then(|p| p.potion_id)
                    .expect("a potion id");
                // The table's splash potion is always Fire Resistance; its plain potion is Fire
                // Resistance or a water bottle.
                let fire_resistance = i32::from(pumpkin_data::potion::Potion::FIRE_RESISTANCE.id);
                if potion == fire_resistance {
                    potions += 1;
                } else {
                    assert_eq!(item.item, &Item::POTION, "splash potion {potion}");
                    assert_eq!(potion, i32::from(pumpkin_data::potion::Potion::WATER.id));
                }
            }
        }
        // 5, 8 and 8 + 8 of 459 weight: about 22, 35 and 70 in 2000.
        assert!(
            books > 0 && boots > 0 && potions > 0,
            "{books} {boots} {potions}"
        );
    }
}
