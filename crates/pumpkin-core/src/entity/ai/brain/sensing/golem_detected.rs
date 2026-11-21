//! Vanilla `GolemSensor`.

use pumpkin_data::entity::EntityType;

use super::super::BrainState;
use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;

/// Vanilla `GolemSensor.MEMORY_TIME_TO_LIVE`.
const MEMORY_TIME_TO_LIVE: i64 = 599;

const REQUIRES: [MemoryModuleId; 2] = [
    types::NEAREST_LIVING_ENTITIES.id(),
    types::GOLEM_DETECTED_RECENTLY.id(),
];

pub struct GolemSensor;

impl Sensor for GolemSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        check_for_nearby_golem(tick.state);
    }
}

/// Vanilla `GolemSensor.checkForNearbyGolem`.
pub fn check_for_nearby_golem(state: &mut BrainState) {
    let golem_present = state
        .get(types::NEAREST_LIVING_ENTITIES)
        .is_some_and(|entities| {
            entities
                .iter()
                .any(|entity| entity.get_entity().entity_type == &EntityType::IRON_GOLEM)
        });
    if golem_present {
        golem_detected(state);
    }
}

/// Vanilla `GolemSensor.golemDetected`.
pub fn golem_detected(state: &mut BrainState) {
    state.set_with_expiry(types::GOLEM_DETECTED_RECENTLY, true, MEMORY_TIME_TO_LIVE);
}
