use pumpkin_util::math::position::BlockPos;
use rand::RngExt;

use super::super::memory::{MemoryModuleType, MemoryStatus, types};
use super::{OneShot, utils};

/// Vanilla `GoToTargetLocation`: walks to a block next to (or on) the remembered location until
/// within `close_enough_dist` of it.
#[must_use]
pub fn create(
    location: MemoryModuleType<BlockPos>,
    close_enough_dist: i32,
    speed_modifier: f32,
) -> OneShot {
    OneShot::new(
        "GoToTargetLocation",
        vec![
            (location.id(), MemoryStatus::ValuePresent),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
        ],
        move |tick| {
            let Some(target) = tick.state.get(location).copied() else {
                return false;
            };
            let body = tick.mob.get_entity().block_pos.load();
            let (dx, dy, dz) = (
                i64::from(target.0.x - body.0.x),
                i64::from(target.0.y - body.0.y),
                i64::from(target.0.z - body.0.z),
            );
            let close = i64::from(close_enough_dist);
            if dx * dx + dy * dy + dz * dz >= close * close {
                let mut rng = tick.mob.get_random();
                let nearby = BlockPos::new(
                    target.0.x + rng.random_range(-1..=1),
                    target.0.y,
                    target.0.z + rng.random_range(-1..=1),
                );
                utils::set_walk_and_look_target_memories_to_block(
                    tick.state,
                    nearby,
                    speed_modifier,
                    close_enough_dist,
                );
            }
            true
        },
    )
}
