use std::sync::Arc;

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleType, MemoryStatus, MemoryValue, types};
use super::OneShot;
use crate::entity::EntityBase;
use crate::entity::ai::util::land_random_pos;

/// Vanilla `SetWalkTargetAwayFrom.pos`: flee from a remembered block, measured from its bottom
/// centre.
#[must_use]
pub fn pos(
    memory: MemoryModuleType<BlockPos>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
) -> OneShot {
    create(
        memory,
        speed_modifier,
        desired_distance,
        interrupt_current_walk,
        |pos| {
            Vector3::new(
                f64::from(pos.0.x) + 0.5,
                f64::from(pos.0.y),
                f64::from(pos.0.z) + 0.5,
            )
        },
    )
}

/// Vanilla `SetWalkTargetAwayFrom.entity`: flee from a remembered entity's position.
#[must_use]
pub fn entity(
    memory: MemoryModuleType<Arc<dyn EntityBase>>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
) -> OneShot {
    create(
        memory,
        speed_modifier,
        desired_distance,
        interrupt_current_walk,
        |entity| entity.get_entity().pos.load(),
    )
}

fn create<T: MemoryValue>(
    walk_away_from: MemoryModuleType<T>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
    to_position: fn(&T) -> Vector3<f64>,
) -> OneShot {
    OneShot::new(
        "SetWalkTargetAwayFrom",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (walk_away_from.id(), MemoryStatus::ValuePresent),
        ],
        move |tick| {
            let current = tick
                .state
                .get(types::WALK_TARGET)
                .map(|walk| (walk.speed_modifier(), walk.target().current_position()));
            if current.is_some() && !interrupt_current_walk {
                return false;
            }
            let Some(avoid_position) = tick.state.get(walk_away_from).map(to_position) else {
                return false;
            };
            let body_position = tick.mob.get_entity().pos.load();
            let desired = f64::from(desired_distance);
            if body_position.squared_distance_to_vec(&avoid_position) >= desired * desired {
                return false;
            }
            #[expect(
                clippy::float_cmp,
                reason = "vanilla compares the walk target's speed exactly"
            )]
            if let Some((current_speed, current_position)) = current
                && current_speed == speed_modifier
            {
                let current_direction = current_position - body_position;
                let avoid_direction = avoid_position - body_position;
                if current_direction.dot(&avoid_direction) < 0.0 {
                    return false;
                }
            }
            for _ in 0..10 {
                if let Some(flee_to) =
                    land_random_pos::get_pos_away(tick.mob, 16, 7, avoid_position)
                {
                    tick.state.set(
                        types::WALK_TARGET,
                        WalkTarget::from_vec(flee_to, speed_modifier, 0),
                    );
                    break;
                }
            }
            true
        },
    )
}
