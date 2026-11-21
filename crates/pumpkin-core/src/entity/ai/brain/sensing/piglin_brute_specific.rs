use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::BrainTick;
use super::super::behavior::utils;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use crate::entity::EntityBase;

const REQUIRES: [MemoryModuleId; 3] = [
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_VISIBLE_NEMESIS.id(),
    types::NEARBY_ADULT_PIGLINS.id(),
];

/// Vanilla `PiglinAi.findNearbyAdultPiglins`: the adult piglins and piglin brutes among the
/// nearest living entities, seen or not.
#[must_use]
pub fn find_nearby_adult_piglins(tick: &BrainTick<'_>) -> Vec<Arc<dyn EntityBase>> {
    tick.state
        .get(types::NEAREST_LIVING_ENTITIES)
        .map(|entities| {
            entities
                .iter()
                .filter(|entity| {
                    let entity_type = entity.get_entity().entity_type;
                    (entity_type == &EntityType::PIGLIN || entity_type == &EntityType::PIGLIN_BRUTE)
                        && !utils::is_baby(entity.as_ref())
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Vanilla `PiglinBruteSpecificSensor` (`piglin_brute_specific_sensor`): the closest visible
/// wither skeleton or wither is the nemesis, and the nearby adult piglins are remembered.
pub struct PiglinBruteSpecificSensor;

impl Sensor for PiglinBruteSpecificSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let nemesis = tick
            .state
            .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .and_then(|visible| {
                visible.find_closest(&tick.visibility(), |entity| {
                    let entity_type = entity.get_entity().entity_type;
                    entity_type == &EntityType::WITHER_SKELETON
                        || entity_type == &EntityType::WITHER
                })
            });
        let adult_piglins = find_nearby_adult_piglins(tick);
        tick.state
            .set_optional(types::NEAREST_VISIBLE_NEMESIS, nemesis);
        tick.state.set(types::NEARBY_ADULT_PIGLINS, adult_piglins);
    }
}
