use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::{MemoryStatus, types};
use super::OneShot;

/// Vanilla `SetLookAndInteract.create(type, interactionRange)`: the closest visible entity of
/// `entity_type` within range becomes the interaction target and the look target.
#[must_use]
pub fn create(entity_type: &'static EntityType, interaction_range: i32) -> OneShot {
    let range_sqr = f64::from(interaction_range * interaction_range);
    OneShot::new(
        "SetLookAndInteract",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::INTERACTION_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let position = tick.mob.get_entity().pos.load();
            let closest = tick
                .state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .and_then(|visible| {
                    visible.find_closest(&tick.visibility(), |entity| {
                        let entity = entity.get_entity();
                        entity.pos.load().squared_distance_to_vec(&position) <= range_sqr
                            && entity.entity_type == entity_type
                    })
                });
            let Some(closest) = closest else {
                return false;
            };
            tick.state.set(types::INTERACTION_TARGET, closest.clone());
            tick.state.set(
                types::LOOK_TARGET,
                Arc::new(EntityTracker::new(closest, true)) as Arc<dyn PositionTracker>,
            );
            true
        },
    )
}
