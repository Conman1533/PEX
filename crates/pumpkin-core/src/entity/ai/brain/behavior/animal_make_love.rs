use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::timed::Behavior;
use super::utils;
use crate::entity::EntityBase;

/// Vanilla `AnimalMakeLove.BREED_RANGE`.
const BREED_RANGE: f64 = 3.0;
const MIN_DURATION: i32 = 60;
const MAX_DURATION: i32 = 110;
const DEFAULT_CLOSE_ENOUGH_DISTANCE: i32 = 2;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 5] = [
    (
        types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        MemoryStatus::ValuePresent,
    ),
    (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::Registered),
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `AnimalMakeLove`: two animals in love walk to each other and breed.
pub struct AnimalMakeLove {
    partner_type: &'static EntityType,
    speed_modifier: f32,
    close_enough_distance: i32,
    spawn_child_at_time: i64,
}

impl AnimalMakeLove {
    #[must_use]
    pub const fn new(
        partner_type: &'static EntityType,
        speed_modifier: f32,
        close_enough_distance: i32,
    ) -> Self {
        Self {
            partner_type,
            speed_modifier,
            close_enough_distance,
            spawn_child_at_time: 0,
        }
    }

    #[must_use]
    pub const fn for_type(partner_type: &'static EntityType) -> Self {
        Self::new(partner_type, 1.0, DEFAULT_CLOSE_ENOUGH_DISTANCE)
    }

    fn find_valid_breed_partner(&self, tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
        let animal = tick.mob.as_animal()?;
        let visible = tick.state.get(types::NEAREST_VISIBLE_LIVING_ENTITIES)?;
        visible.find_closest(&tick.visibility(), |entity| {
            entity.get_entity().entity_type == self.partner_type
                && entity
                    .get_mob()
                    .is_some_and(|mob| mob.as_animal().is_some() && animal.can_mate(mob))
                && !entity.is_panicking()
        })
    }

    /// Vanilla `BehaviorUtils.lockGazeAndWalkToEachOther`; the partner's side goes through its
    /// own brain.
    fn lock_gaze_and_walk_to_each_other(
        &self,
        tick: &mut BrainTick<'_>,
        partner: &Arc<dyn EntityBase>,
        set_partner_breed_target: bool,
    ) {
        utils::set_walk_and_look_target_memories_to_entity(
            tick.state,
            partner.clone(),
            self.speed_modifier,
            self.close_enough_distance,
        );
        let (Some(body), Some(partner_mob)) = (
            tick.world.get_entity_by_id(tick.mob.get_entity().entity_id),
            partner.get_mob(),
        ) else {
            return;
        };
        partner_mob
            .get_mob_entity()
            .with_brain(partner_mob, |brain| {
                if set_partner_breed_target {
                    brain.set(types::BREED_TARGET, body.clone());
                }
                utils::set_walk_and_look_target_memories_to_entity(
                    brain,
                    body,
                    self.speed_modifier,
                    self.close_enough_distance,
                );
            });
    }
}

fn erase_partner_breed_target(partner: &Arc<dyn EntityBase>) {
    if let Some(partner_mob) = partner.get_mob() {
        partner_mob
            .get_mob_entity()
            .with_brain(partner_mob, |brain| brain.erase(types::BREED_TARGET.id()));
    }
}

impl Behavior for AnimalMakeLove {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        MAX_DURATION
    }

    fn max_duration(&self) -> i32 {
        MAX_DURATION
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.mob.get_mob_entity().is_in_love() && self.find_valid_breed_partner(tick).is_some()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let Some(partner) = self.find_valid_breed_partner(tick) else {
            return;
        };
        tick.state.set(types::BREED_TARGET, partner.clone());
        self.lock_gaze_and_walk_to_each_other(tick, &partner, true);
        let duration = MIN_DURATION + tick.mob.get_random().random_range(0..50);
        self.spawn_child_at_time = tick.time + i64::from(duration);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(partner) = tick.state.get(types::BREED_TARGET).cloned() else {
            return false;
        };
        if partner.get_entity().entity_type != self.partner_type {
            return false;
        }
        let Some(animal) = tick.mob.as_animal() else {
            return false;
        };
        partner.get_entity().is_alive()
            && partner.get_mob().is_some_and(|mob| animal.can_mate(mob))
            && utils::entity_is_visible(&tick.visibility(), partner.as_ref())
            && tick.time <= self.spawn_child_at_time
            && !tick.state.has_memory_value(types::IS_PANICKING.id())
            && !partner.is_panicking()
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(partner) = tick.state.get(types::BREED_TARGET).cloned() else {
            return;
        };
        self.lock_gaze_and_walk_to_each_other(tick, &partner, false);
        let distance_sq = tick
            .mob
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&partner.get_entity().pos.load());
        if distance_sq < BREED_RANGE * BREED_RANGE && tick.time >= self.spawn_child_at_time {
            if let (Some(animal), Some(partner_mob)) = (tick.mob.as_animal(), partner.get_mob()) {
                animal.spawn_child_from_breeding(tick.world, partner_mob);
            }
            tick.state.erase(types::BREED_TARGET.id());
            erase_partner_breed_target(&partner);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        tick.state.erase(types::BREED_TARGET.id());
        tick.state.erase(types::WALK_TARGET.id());
        tick.state.erase(types::LOOK_TARGET.id());
        self.spawn_child_at_time = 0;
    }

    fn debug_name(&self) -> &'static str {
        "AnimalMakeLove"
    }
}
