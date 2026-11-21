//! Vanilla `SetWalkTargetFromBlockMemory`.

use std::f64::consts::FRAC_PI_2;

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use super::super::memory::global_pos::GlobalPos;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleType, MemoryStatus, types};
use super::OneShot;
use super::acquire_poi::release_poi;
use crate::entity::ai::util::default_random_pos;

/// Vanilla `SetWalkTargetFromBlockMemory`'s `MAX_TRIES`.
const MAX_TRIES: i32 = 1000;

#[must_use]
pub const fn dist_manhattan(a: BlockPos, b: BlockPos) -> i32 {
    (a.0.x - b.0.x).abs() + (a.0.y - b.0.y).abs() + (a.0.z - b.0.z).abs()
}

/// `Vec3.atBottomCenterOf`.
#[must_use]
pub fn bottom_center(pos: BlockPos) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    )
}

/// Vanilla `SetWalkTargetFromBlockMemory.create`: walks to the block in `memory`, a step at a
/// time when it is far, and gives it up when it stays out of reach for too long.
#[must_use]
pub fn create(
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f32,
    close_enough_dist: i32,
    too_far_distance: i32,
    too_long_unreachable_duration: i64,
) -> OneShot {
    OneShot::new(
        "SetWalkTargetFromBlockMemory",
        vec![
            (
                types::CANT_REACH_WALK_TARGET_SINCE.id(),
                MemoryStatus::Registered,
            ),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (memory.id(), MemoryStatus::ValuePresent),
        ],
        move |tick| {
            let Some(target) = tick.state.get(memory).copied() else {
                return false;
            };
            let cant_reach_since = tick.state.get(types::CANT_REACH_WALK_TARGET_SINCE).copied();
            let body = tick.mob.get_entity().block_pos.load();
            let give_up = |tick: &mut super::super::BrainTick<'_>| {
                release_poi(tick, memory);
                tick.state.erase(memory.id());
                tick.state
                    .set(types::CANT_REACH_WALK_TARGET_SINCE, tick.time);
            };
            if target.dimension.id != tick.world.dimension.id
                || cant_reach_since
                    .is_some_and(|since| tick.time - since > too_long_unreachable_duration)
            {
                give_up(tick);
                return true;
            }
            let distance = dist_manhattan(target.pos, body);
            if distance > too_far_distance {
                let mut tries = 0;
                loop {
                    let towards = default_random_pos::get_pos_towards(
                        tick.mob,
                        15,
                        7,
                        bottom_center(target.pos),
                        FRAC_PI_2,
                    );
                    if let Some(towards) = towards
                        && dist_manhattan(BlockPos::floored_v(towards), body) <= too_far_distance
                    {
                        tick.state.set(
                            types::WALK_TARGET,
                            WalkTarget::from_vec(towards, speed_modifier, close_enough_dist),
                        );
                        break;
                    }
                    tries += 1;
                    if tries == MAX_TRIES {
                        give_up(tick);
                        return true;
                    }
                }
            } else if distance > close_enough_dist {
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::from_block_pos(target.pos, speed_modifier, close_enough_dist),
                );
            }
            true
        },
    )
}
