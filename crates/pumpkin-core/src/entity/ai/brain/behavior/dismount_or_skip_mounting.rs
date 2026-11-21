use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use crate::entity::EntityBase;

/// Vanilla `DismountOrSkipMounting`: gets off (or gives up on) a vehicle that is dead, more
/// than `max_walk_dist_to_ride_target` away, in another world, or one `dont_ride_if` rejects.
#[must_use]
pub fn create(
    max_walk_dist_to_ride_target: i32,
    dont_ride_if: impl Fn(&BrainTick<'_>, &Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
) -> OneShot {
    let max_dist_sqr = f64::from(max_walk_dist_to_ride_target * max_walk_dist_to_ride_target);
    OneShot::new(
        "DismountOrSkipMounting",
        vec![(types::RIDE_TARGET.id(), MemoryStatus::Registered)],
        move |tick| {
            let body = tick.mob.get_entity();
            let current = body.get_vehicle();
            let Some(vehicle) = current
                .clone()
                .or_else(|| tick.state.get(types::RIDE_TARGET).cloned())
            else {
                return false;
            };
            let vehicle_entity = vehicle.get_entity();
            let valid = vehicle_entity.is_alive()
                && vehicle_entity
                    .pos
                    .load()
                    .squared_distance_to_vec(&body.pos.load())
                    < max_dist_sqr
                && Arc::ptr_eq(&vehicle_entity.world.load_full(), tick.world);
            if valid && !dont_ride_if(tick, &vehicle) {
                return false;
            }
            if let Some(current) = current {
                current.get_entity().remove_passenger_sync(body.entity_id);
            }
            tick.state.erase(types::RIDE_TARGET.id());
            true
        },
    )
}
