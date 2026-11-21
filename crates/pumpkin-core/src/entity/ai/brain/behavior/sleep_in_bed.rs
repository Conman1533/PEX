//! Vanilla `SleepInBed` and `WakeUp`.

use pumpkin_data::environment_attribute::Activity;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::acquire_poi::closer_to_center_than;
use super::interact_with_door::close_doors_that_i_have_opened_or_passed_through;
use super::validate_nearby_poi::bed_occupied;
use super::{Behavior, OneShot};

/// Vanilla `SleepInBed.COOLDOWN_AFTER_BEING_WOKEN`.
pub const COOLDOWN_AFTER_BEING_WOKEN: i64 = 100;

/// Vanilla `Villager.stopSleeping`: wakes the body and remembers when.
pub fn stop_sleeping(tick: &mut BrainTick<'_>) {
    if tick.mob.stop_sleeping() {
        tick.state.set(types::LAST_WOKEN, tick.time);
    }
}

/// Vanilla `WakeUp.create`.
#[must_use]
pub fn wake_up() -> OneShot {
    OneShot::new("WakeUp", Vec::new(), |tick| {
        if !tick.state.is_active(Activity::Rest) && tick.mob.is_sleeping() {
            stop_sleeping(tick);
            true
        } else {
            false
        }
    })
}

const ENTRY: [(MemoryModuleId, MemoryStatus); 5] = [
    (types::HOME.id(), MemoryStatus::ValuePresent),
    (types::LAST_WOKEN.id(), MemoryStatus::Registered),
    (types::LAST_SLEPT.id(), MemoryStatus::Registered),
    (types::WALK_TARGET.id(), MemoryStatus::Registered),
    (
        types::CANT_REACH_WALK_TARGET_SINCE.id(),
        MemoryStatus::Registered,
    ),
];

/// Vanilla `SleepInBed`.
#[derive(Default)]
pub struct SleepInBed {
    next_ok_start_time: i64,
}

impl Behavior for SleepInBed {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY
    }

    // Vanilla `timedOut` is always false.
    fn min_duration(&self) -> i32 {
        i32::MAX
    }

    fn max_duration(&self) -> i32 {
        i32::MAX
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        if tick.mob.get_entity().has_vehicle() {
            return false;
        }
        let Some(target) = tick.state.get(types::HOME).copied() else {
            return false;
        };
        if target.dimension.id != tick.world.dimension.id {
            return false;
        }
        if let Some(last_woken) = tick.state.get(types::LAST_WOKEN).copied() {
            let since = tick.time - last_woken;
            if since > 0 && since < COOLDOWN_AFTER_BEING_WOKEN {
                return false;
            }
        }
        closer_to_center_than(target.pos, tick.mob.get_entity().pos.load(), 2.0)
            && bed_occupied(tick.world, &target.pos) == Some(false)
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(home) = tick.state.get(types::HOME).copied() else {
            return false;
        };
        let body = tick.mob.get_entity().pos.load();
        tick.state.is_active(Activity::Rest)
            && body.y > f64::from(home.pos.0.y) + 0.4
            && closer_to_center_than(home.pos, body, 1.14)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if tick.time <= self.next_ok_start_time {
            return;
        }
        if let Some(mut doors) = tick.state.get(types::DOORS_TO_CLOSE).cloned() {
            close_doors_that_i_have_opened_or_passed_through(tick, None, None, &mut doors);
            tick.state.set(types::DOORS_TO_CLOSE, doors);
        }
        if let Some(home) = tick.state.get(types::HOME).copied()
            && tick.mob.start_sleeping(home.pos)
        {
            tick.state.set(types::LAST_SLEPT, tick.time);
        }
        tick.state.erase(types::WALK_TARGET.id());
        tick.state.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        if tick.mob.is_sleeping() {
            stop_sleeping(tick);
            self.next_ok_start_time = tick.time + 40;
        }
    }

    fn debug_name(&self) -> &'static str {
        "SleepInBed"
    }
}
