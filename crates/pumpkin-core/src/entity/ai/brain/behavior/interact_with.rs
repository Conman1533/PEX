use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;

/// Vanilla `InteractWith.of` with `INTERACTION_TARGET`.
///
/// Walks to the closest visible entity of `entity_type` within range, looking at it. Succeeds
/// whenever any such entity is visible, even out of range, like vanilla.
#[must_use]
pub fn of(
    entity_type: &'static EntityType,
    interaction_range: i32,
    speed_modifier: f32,
    stop_distance: i32,
) -> OneShot {
    let range_sqr = f64::from(interaction_range * interaction_range);
    OneShot::new(
        "InteractWith",
        vec![
            (types::INTERACTION_TARGET.id(), MemoryStatus::Registered),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let position = tick.mob.get_entity().pos.load();
            let ctx = tick.visibility();
            let Some(visible) = tick.state.get(types::NEAREST_VISIBLE_LIVING_ENTITIES) else {
                return false;
            };
            let is_target_valid = |entity: &Arc<dyn crate::entity::EntityBase>| {
                entity.get_entity().entity_type == entity_type
            };
            if !visible.contains_any(&ctx, is_target_valid) {
                return false;
            }
            let closest = visible.find_closest(&ctx, |entity| {
                entity
                    .get_entity()
                    .pos
                    .load()
                    .squared_distance_to_vec(&position)
                    <= range_sqr
                    && is_target_valid(entity)
            });
            if let Some(target) = closest {
                tick.state.set(types::INTERACTION_TARGET, target.clone());
                tick.state.set(
                    types::LOOK_TARGET,
                    Arc::new(EntityTracker::new(target.clone(), true)) as Arc<dyn PositionTracker>,
                );
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::from_entity(target, speed_modifier, stop_distance),
                );
            }
            true
        },
    )
}
