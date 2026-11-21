use std::sync::Arc;

use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleType, MemoryStatus, types};
use super::{OneShot, utils};
use crate::entity::EntityBase;
use crate::entity::mob::Mob;

/// Vanilla `BabyFollowAdult.create(followRange, speedModifier)`: a baby walks to the nearest
/// visible adult when it is between `follow_min` and `follow_max + 1` blocks away.
#[must_use]
pub fn create(follow_min: i32, follow_max: i32, speed_modifier: f32) -> OneShot {
    create_with(
        follow_min,
        follow_max,
        move |_| speed_modifier,
        types::NEAREST_VISIBLE_ADULT,
        false,
    )
}

#[must_use]
pub fn create_with(
    follow_min: i32,
    follow_max: i32,
    speed_modifier: impl Fn(&dyn Mob) -> f32 + Send + Sync + 'static,
    nearest_visible: MemoryModuleType<Arc<dyn EntityBase>>,
    target_eye: bool,
) -> OneShot {
    let max = f64::from(follow_max + 1);
    let min = f64::from(follow_min);
    OneShot::new(
        "BabyFollowAdult",
        vec![
            (nearest_visible.id(), MemoryStatus::ValuePresent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
        ],
        move |tick| {
            if !utils::is_baby(tick.mob) {
                return false;
            }
            let Some(adult) = tick.state.get(nearest_visible).cloned() else {
                return false;
            };
            let distance_sq = tick
                .mob
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&adult.get_entity().pos.load());
            if distance_sq >= max * max || distance_sq < min * min {
                return false;
            }
            let walk = WalkTarget::new(
                Arc::new(EntityTracker::with_target_eye_height(
                    adult.clone(),
                    target_eye,
                    target_eye,
                )),
                speed_modifier(tick.mob),
                follow_min - 1,
            );
            tick.state.set(
                types::LOOK_TARGET,
                Arc::new(EntityTracker::with_target_eye_height(
                    adult, true, target_eye,
                )) as Arc<dyn PositionTracker>,
            );
            tick.state.set(types::WALK_TARGET, walk);
            true
        },
    )
}
