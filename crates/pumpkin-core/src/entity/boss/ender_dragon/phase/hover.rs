use crate::entity::EntityBase;
use crate::entity::boss::ender_dragon::EnderDragonEntity;
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Mutex, PoisonError};

/// Vanilla `DragonHoverPhase`: hold the current position. A dragon starts here, so a summoned one
/// hovers until something else sets its phase.
#[derive(Default)]
pub struct HoverPhase {
    target_location: Mutex<Option<Vector3<f64>>>,
}

impl super::Phase for HoverPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        self.target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_with(|| dragon.get_entity().pos.load());
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        *self
            .target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn get_fly_speed(&self) -> f32 {
        1.0
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        *self
            .target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
