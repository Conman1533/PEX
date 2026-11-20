use pumpkin_util::math::vector3::Vector3;

use super::super::memory::{MemoryStatus, types, walk_target::WalkTarget};
use super::OneShot;
use crate::entity::ai::util::{air_and_water_random_pos, land_random_pos};
use crate::entity::mob::Mob;

const MAX_XZ_DIST: i32 = 10;
const MAX_Y_DIST: i32 = 7;

/// Vanilla `RandomStroll.stroll(speedModifier)`: a random land position up to 10 blocks away.
#[must_use]
pub fn stroll(speed_modifier: f32) -> OneShot {
    stroll_with(speed_modifier, true)
}

/// Vanilla `RandomStroll.stroll(speedModifier, mayStrollFromWater)`.
#[must_use]
pub fn stroll_with(speed_modifier: f32, may_stroll_from_water: bool) -> OneShot {
    OneShot::new(
        "RandomStroll",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        move |tick| {
            if !may_stroll_from_water && tick.mob.get_entity().is_in_water() {
                return false;
            }
            let target = land_random_pos::get_pos(tick.mob, MAX_XZ_DIST, MAX_Y_DIST);
            tick.state.set_optional(
                types::WALK_TARGET,
                target.map(|pos| WalkTarget::from_vec(pos, speed_modifier, 0)),
            );
            true
        },
    )
}

#[must_use]
pub fn fly(speed_modifier: f32) -> OneShot {
    OneShot::new(
        "RandomStroll",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        move |tick| {
            let target = get_target_fly_pos(tick.mob, MAX_XZ_DIST, MAX_Y_DIST);
            tick.state.set_optional(
                types::WALK_TARGET,
                target.map(|pos| WalkTarget::from_vec(pos, speed_modifier, 0)),
            );
            true
        },
    )
}

fn get_target_fly_pos(
    mob: &dyn Mob,
    max_horizontal_distance: i32,
    max_vertical_distance: i32,
) -> Option<Vector3<f64>> {
    let entity = mob.get_entity();
    let wander_direction =
        Vector3::rotation_vector(f64::from(entity.pitch.load()), f64::from(entity.yaw.load()));
    air_and_water_random_pos::get_pos(
        mob,
        max_horizontal_distance,
        max_vertical_distance,
        -2,
        wander_direction.x,
        wander_direction.z,
        f64::from(std::f32::consts::FRAC_PI_2),
    )
}
