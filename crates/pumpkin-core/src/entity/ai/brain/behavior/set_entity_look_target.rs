use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::memory::{MemoryStatus, types};
use super::{OneShot, utils};
use crate::entity::EntityBase;

/// Vanilla `SetEntityLookTarget.create(maxDist)`: looks at the closest visible living entity.
#[must_use]
pub fn create(max_dist: f32) -> OneShot {
    create_with(|_| true, max_dist)
}

/// Vanilla `SetEntityLookTarget.create(type, maxDist)`.
#[must_use]
pub fn create_for_type(entity_type: &'static EntityType, max_dist: f32) -> OneShot {
    create_with(
        move |entity| entity.get_entity().entity_type == entity_type,
        max_dist,
    )
}

/// Vanilla `SetEntityLookTarget.create(predicate, maxDist)`: the closest visible living entity
/// matching `predicate` within `max_dist` that isn't riding this mob.
#[must_use]
pub fn create_with(
    predicate: impl Fn(&Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
    max_dist: f32,
) -> OneShot {
    let max_dist_sqr = f64::from(max_dist * max_dist);
    OneShot::new(
        "SetEntityLookTarget",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let body = tick.mob.get_entity();
            let position = body.pos.load();
            let target = tick
                .state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .and_then(|visible| {
                    visible.find_closest(&tick.visibility(), |entity| {
                        predicate(entity)
                            && entity
                                .get_entity()
                                .pos
                                .load()
                                .squared_distance_to_vec(&position)
                                <= max_dist_sqr
                            && !body.has_passenger(entity.get_entity().entity_id)
                    })
                });
            let Some(target) = target else {
                return false;
            };
            utils::look_at_entity(tick.state, target);
            true
        },
    )
}
