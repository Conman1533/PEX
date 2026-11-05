use super::{EnderDragonPhase, distance_to_target, needs_new_target};
use crate::entity::boss::ender_dragon::EnderDragonEntity;
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Mutex, PoisonError};
use tracing::warn;

const CHARGE_RECOVERY_TIME: i32 = 10;

#[derive(Default)]
struct State {
    target_location: Option<Vector3<f64>>,
    time_since_charge: i32,
}

/// Vanilla `DragonChargePlayerPhase`: dive at a spot fast, then go back to circling.
#[derive(Default)]
pub struct ChargingPlayerPhase {
    state: Mutex<State>,
}

impl ChargingPlayerPhase {
    pub fn set_target(&self, target: Vector3<f64>) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .target_location = Some(target);
    }
}

impl super::Phase for ChargingPlayerPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(target) = state.target_location else {
            drop(state);
            warn!("Aborting charge player as no target was set.");
            dragon.set_phase(EnderDragonPhase::HoldingPattern);
            return;
        };

        // Vanilla's `timeSinceCharge > 0 && timeSinceCharge++ >= 10` counts up even when it
        // falls through to the distance check.
        if state.time_since_charge > 0 {
            state.time_since_charge += 1;
            if state.time_since_charge > CHARGE_RECOVERY_TIME {
                drop(state);
                dragon.set_phase(EnderDragonPhase::HoldingPattern);
                return;
            }
        }
        if needs_new_target(dragon, distance_to_target(dragon, Some(target))) {
            state.time_since_charge += 1;
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.target_location = None;
        state.time_since_charge = 0;
    }

    fn get_fly_speed(&self) -> f32 {
        3.0
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .target_location
    }
}
