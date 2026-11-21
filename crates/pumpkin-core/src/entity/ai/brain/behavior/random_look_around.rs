use std::sync::Arc;

use pumpkin_util::math::{self, vector3::Vector3};
use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::position_tracker::{BlockPosTracker, PositionTracker};
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::timed::Behavior;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::LOOK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::GAZE_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `RandomLookAround`: looks in a random direction near the current one, then waits
/// `interval` ticks before looking around again.
pub struct RandomLookAround {
    interval_min: i32,
    interval_max: i32,
    max_yaw: f32,
    min_pitch: f32,
    pitch_range: f32,
}

impl RandomLookAround {
    /// # Panics
    /// Like vanilla, when `min_pitch` is larger than `max_pitch`.
    #[must_use]
    pub fn new(
        interval_min: i32,
        interval_max: i32,
        max_yaw: f32,
        min_pitch: f32,
        max_pitch: f32,
    ) -> Self {
        assert!(
            min_pitch <= max_pitch,
            "Minimum pitch is larger than maximum pitch! {min_pitch} > {max_pitch}"
        );
        Self {
            interval_min,
            interval_max,
            max_yaw,
            min_pitch,
            pitch_range: max_pitch - min_pitch,
        }
    }
}

/// Vanilla `Vec3.directionFromRotation`, with vanilla's sine table.
#[must_use]
pub fn direction_from_rotation(pitch: f32, yaw: f32) -> Vector3<f64> {
    let to_radians = (std::f64::consts::PI / 180.0) as f32;
    let pi = std::f64::consts::PI as f32;
    let y_cos = math::cos(-yaw * to_radians - pi);
    let y_sin = math::sin(-yaw * to_radians - pi);
    let x_cos = -math::cos(-pitch * to_radians);
    let x_sin = math::sin(-pitch * to_radians);
    Vector3::new(
        f64::from(y_sin * x_cos),
        f64::from(x_sin),
        f64::from(y_cos * x_cos),
    )
}

impl Behavior for RandomLookAround {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        let mut random = tick.mob.get_random();
        // Vanilla rounds each step, so no fused multiply-adds here.
        let pitch = (random.random::<f32>() * self.pitch_range + self.min_pitch).clamp(-90.0, 90.0);
        let rotation = math::wrap_degrees(
            entity.yaw.load() + 2.0 * random.random::<f32>() * self.max_yaw - self.max_yaw,
        );
        let target = entity.get_eye_pos() + direction_from_rotation(pitch, rotation);
        tick.state.set(
            types::LOOK_TARGET,
            Arc::new(BlockPosTracker::from_vec(target)) as Arc<dyn PositionTracker>,
        );
        tick.state.set(
            types::GAZE_COOLDOWN_TICKS,
            random.random_range(self.interval_min..=self.interval_max),
        );
    }

    fn debug_name(&self) -> &'static str {
        "RandomLookAround"
    }
}
