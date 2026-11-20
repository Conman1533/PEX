use pumpkin_data::tag::{self, Taggable};

use super::super::BrainTick;
use super::super::behavior::utils;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;

const REQUIRES: [MemoryModuleId; 2] = [
    types::NEAREST_VISIBLE_ADULT.id(),
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
];

/// Vanilla `AdultSensor` (`nearest_adult`): the closest visible adult of the mob's own type.
///
/// With `any_type` it is vanilla `AdultSensorAnyType` (`nearest_adult_any_type`): the closest
/// visible adult in `#followable_friendly_mobs`.
pub struct AdultSensor {
    any_type: bool,
}

impl AdultSensor {
    #[must_use]
    pub const fn same_type() -> Self {
        Self { any_type: false }
    }

    #[must_use]
    pub const fn any_type() -> Self {
        Self { any_type: true }
    }
}

impl Sensor for AdultSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let body_type = tick.mob.get_entity().entity_type;
        let adult = {
            let Some(visible) = tick.state.get(types::NEAREST_VISIBLE_LIVING_ENTITIES) else {
                return;
            };
            visible.find_closest(&tick.visibility(), |entity| {
                let entity_type = entity.get_entity().entity_type;
                let matches = if self.any_type {
                    entity_type.has_tag(&tag::EntityType::MINECRAFT_FOLLOWABLE_FRIENDLY_MOBS)
                } else {
                    entity_type == body_type
                };
                matches && !utils::is_baby(entity.as_ref())
            })
        };
        tick.state.set_optional(types::NEAREST_VISIBLE_ADULT, adult);
    }
}
