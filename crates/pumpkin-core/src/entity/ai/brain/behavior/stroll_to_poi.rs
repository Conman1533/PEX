use pumpkin_util::math::vector3::Vector3;

use super::super::BrainTick;
use super::super::memory::global_pos::GlobalPos;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleType, MemoryStatus, types};
use super::OneShot;
use crate::entity::ai::util::land_random_pos;

/// Vanilla `StrollAroundPoi.MIN_TIME_BETWEEN_STROLLS`.
const MIN_TIME_BETWEEN_STROLLS_AROUND: i64 = 180;
/// Vanilla `StrollToPoi`'s wait before it sets a new walk target.
const MIN_TIME_BETWEEN_STROLLS_TO: i64 = 80;

/// The point of interest in `memory`, if it is in the mob's world and closer than `max_distance`
/// to the mob (vanilla `closerToCenterThan`).
fn poi_in_reach(
    tick: &BrainTick<'_>,
    memory: MemoryModuleType<GlobalPos>,
    max_distance: i32,
) -> Option<GlobalPos> {
    let pos = *tick.state.get(memory)?;
    let body = tick.mob.get_entity().pos.load();
    let center = pos.pos.to_centered_f64();
    let max = f64::from(max_distance);
    (pos.dimension.minecraft_name == tick.world.dimension.minecraft_name
        && center.squared_distance_to_vec(&body) < max * max)
        .then_some(pos)
}

/// Vanilla `StrollToPoi.create(memoryType, speedModifier, closeEnoughDist, maxDistanceFromPoi)`:
/// walks back to the point of interest every 4 seconds while within `max_distance_from_poi`.
#[must_use]
pub fn stroll_to_poi(
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f32,
    close_enough_dist: i32,
    max_distance_from_poi: i32,
) -> OneShot {
    let mut next_ok_start_time = 0i64;
    OneShot::new(
        "StrollToPoi",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (memory.id(), MemoryStatus::ValuePresent),
        ],
        move |tick| {
            let Some(pos) = poi_in_reach(tick, memory, max_distance_from_poi) else {
                return false;
            };
            if tick.time <= next_ok_start_time {
                return true;
            }
            tick.state.set(
                types::WALK_TARGET,
                WalkTarget::from_block_pos(pos.pos, speed_modifier, close_enough_dist),
            );
            next_ok_start_time = tick.time + MIN_TIME_BETWEEN_STROLLS_TO;
            true
        },
    )
}

/// Vanilla `StrollAroundPoi.create(memoryType, speedModifier, maxDistanceFromPoi)`: every 9
/// seconds, a random land spot up to 8 blocks away while within `max_distance_from_poi`.
#[must_use]
pub fn stroll_around_poi(
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f32,
    max_distance_from_poi: i32,
) -> OneShot {
    let mut next_ok_start_time = 0i64;
    OneShot::new(
        "StrollAroundPoi",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (memory.id(), MemoryStatus::ValuePresent),
        ],
        move |tick| {
            if poi_in_reach(tick, memory, max_distance_from_poi).is_none() {
                return false;
            }
            if tick.time <= next_ok_start_time {
                return true;
            }
            let land_pos: Option<Vector3<f64>> = land_random_pos::get_pos(tick.mob, 8, 6);
            tick.state.set_optional(
                types::WALK_TARGET,
                land_pos.map(|pos| WalkTarget::from_vec(pos, speed_modifier, 1)),
            );
            next_ok_start_time = tick.time + MIN_TIME_BETWEEN_STROLLS_AROUND;
            true
        },
    )
}
