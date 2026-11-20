use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;

use super::ZoglinEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainTick,
    activity::ActivityData,
    behavior::{
        DoNothing, LookAtTargetSink, MoveToTargetSink, Timed, melee_attack, random_stroll, run_one,
        set_entity_look_target_sometimes,
        set_walk_target_from_attack_target_if_target_out_of_reach,
        set_walk_target_from_look_target, start_attacking, stop_attacking_if_target_invalid, utils,
    },
    memory::{PackedMemories, types},
    sensing::{SensorType, is_entity_attackable},
};
use crate::entity::mob::Mob;

const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.4;
const ATTACK_INTERVAL: i32 = 40;
const BABY_ATTACK_INTERVAL: i32 = 15;
/// Vanilla `ATTACK_DURATION`: how long a hit keeps the attacker as the target.
const ATTACK_DURATION: i64 = 200;

/// Vanilla `Zoglin.BRAIN_PROVIDER`.
pub(super) fn make_brain(zoglin: &ZoglinEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &[],
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::NearestPlayers,
        ],
        activities,
    }
    .make_brain(zoglin, packed)
}

/// Vanilla `Zoglin.updateActivity`. Also mirrors `attack_target` into the mob's target, which is
/// vanilla's `getTarget` from the brain.
pub(super) fn update_activity(zoglin: &ZoglinEntity) {
    let (entered_fight, target) = zoglin.mob_entity.with_brain(zoglin, |brain| {
        let old_activity = brain.get_active_non_core_activity();
        brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
        let new_activity = brain.get_active_non_core_activity();
        (
            new_activity == Some(Activity::Fight) && old_activity != Some(Activity::Fight),
            brain.get(types::ATTACK_TARGET).cloned(),
        )
    });
    if entered_fight {
        zoglin
            .mob_entity
            .living_entity
            .make_sound(Sound::EntityZoglinAngry);
    }
    zoglin.mob_entity.set_attacking(target.is_some());
    zoglin.mob_entity.set_target(target);
}

/// Vanilla `Zoglin.hurtServer`: a living attacker becomes the target for 10 seconds, unless it is
/// much further away than the current one.
pub(super) fn react_to_hurt(zoglin: &ZoglinEntity, attacker: &dyn EntityBase) {
    if attacker.get_living_entity().is_none() || !zoglin.can_attack(attacker) {
        return;
    }
    let world = zoglin.mob_entity.living_entity.entity.world.load();
    let Some(attacker) = world.get_entity_by_id(attacker.get_entity().entity_id) else {
        return;
    };
    zoglin.mob_entity.try_with_brain(zoglin, |brain| {
        if utils::is_other_target_much_further_away_than_current_attack_target(
            brain,
            zoglin,
            attacker.as_ref(),
            4.0,
        ) {
            return;
        }
        brain.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
        brain.set_with_expiry(types::ATTACK_TARGET, attacker, ATTACK_DURATION);
    });
}

fn zoglin<'a>(tick: &BrainTick<'a>) -> Option<&'a ZoglinEntity> {
    tick.mob.cast_any().downcast_ref::<ZoglinEntity>()
}

fn is_adult(tick: &BrainTick<'_>) -> bool {
    zoglin(tick).is_some_and(|zoglin| !zoglin.is_baby())
}

fn is_baby(tick: &BrainTick<'_>) -> bool {
    zoglin(tick).is_some_and(ZoglinEntity::is_baby)
}

/// Vanilla `Zoglin.findNearestValidAttackTarget`: anything visible and attackable but zoglins and
/// creepers.
fn find_nearest_valid_attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    let ctx = tick.visibility();
    tick.state
        .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)?
        .find_closest(&ctx, |target| {
            let entity_type = target.get_entity().entity_type;
            entity_type != &EntityType::ZOGLIN
                && entity_type != &EntityType::CREEPER
                && is_entity_attackable(&ctx, target.as_ref())
        })
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
            ],
        ),
        ActivityData::with_priority_start(
            Activity::Idle,
            10,
            vec![
                Box::new(start_attacking::create(find_nearest_valid_attack_target)),
                Box::new(set_entity_look_target_sometimes::create(8.0, 30, 60)),
                Box::new(run_one(vec![
                    (
                        Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (
                        Box::new(set_walk_target_from_look_target::create(
                            SPEED_MULTIPLIER_WHEN_IDLING,
                            3,
                        )),
                        2,
                    ),
                    (Box::new(DoNothing::new(30, 60)), 1),
                ])),
            ],
        ),
        ActivityData::with_gate_memory(
            Activity::Fight,
            10,
            vec![
                Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
                Box::new(melee_attack::create_with(is_adult, ATTACK_INTERVAL)),
                Box::new(melee_attack::create_with(is_baby, BABY_ATTACK_INTERVAL)),
                Box::new(stop_attacking_if_target_invalid::create()),
            ],
            types::ATTACK_TARGET.id(),
        ),
    ]
}
