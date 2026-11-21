use std::sync::Arc;

use super::super::memory::position_tracker::EntityTracker;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryStatus, types};
use super::{OneShot, utils};

/// Vanilla `Mount.CLOSE_ENOUGH_TO_START_RIDING_DIST`.
const CLOSE_ENOUGH_TO_START_RIDING_DIST: f64 = 1.0;

/// Vanilla `Mount`: walks to the `ride_target` and gets on once within a block.
#[must_use]
pub fn create(speed_modifier: f32) -> OneShot {
    OneShot::new(
        "Mount",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::RIDE_TARGET.id(), MemoryStatus::ValuePresent),
        ],
        move |tick| {
            let body = tick.mob.get_entity();
            if body.has_vehicle() {
                return false;
            }
            let Some(vehicle) = tick.state.get(types::RIDE_TARGET).cloned() else {
                return false;
            };
            let distance_sqr = vehicle
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&body.pos.load());
            if distance_sqr < CLOSE_ENOUGH_TO_START_RIDING_DIST * CLOSE_ENOUGH_TO_START_RIDING_DIST
            {
                tick.mob.start_riding(vehicle);
            } else {
                utils::look_at_entity(tick.state, vehicle.clone());
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::new(
                        Arc::new(EntityTracker::new(vehicle, false)),
                        speed_modifier,
                        1,
                    ),
                );
            }
            true
        },
    )
}
