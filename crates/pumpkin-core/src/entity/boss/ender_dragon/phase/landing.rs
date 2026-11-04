use super::EnderDragonPhase;
use crate::entity::EntityBase;
use crate::entity::boss::ender_dragon::EnderDragonEntity;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use std::sync::{Mutex, PoisonError};

/// Vanilla `DragonLandingPhase`: drop onto the portal, then start scanning for players.
#[derive(Default)]
pub struct LandingPhase {
    target_location: Mutex<Option<Vector3<f64>>>,
}

impl super::Phase for LandingPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut target_location = self
            .target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let target = *target_location.get_or_insert_with(|| {
            let egg = dragon.podium_pos(ChunkHeightmapType::MotionBlockingNoLeaves);
            Vector3::new(
                f64::from(egg.0.x) + 0.5,
                f64::from(egg.0.y),
                f64::from(egg.0.z) + 0.5,
            )
        });
        drop(target_location);

        if target.squared_distance_to_vec(&dragon.get_entity().pos.load()) < 1.0 {
            dragon.phase_manager.sitting_flaming.reset_flame_count();
            dragon.set_phase(EnderDragonPhase::SittingScanning);
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        *self
            .target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn get_fly_speed(&self) -> f32 {
        1.5
    }

    fn get_turn_speed(&self, dragon: &EnderDragonEntity) -> f32 {
        let rot_speed = dragon.horizontal_speed() + 1.0;
        let dist = rot_speed.min(40.0);
        dist / rot_speed
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        *self
            .target_location
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
