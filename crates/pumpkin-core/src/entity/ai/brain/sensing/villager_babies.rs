//! Vanilla `VillagerBabiesSensor`.

use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::BrainTick;
use super::super::behavior::utils::is_baby;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use crate::entity::EntityBase;

const REQUIRES: [MemoryModuleId; 1] = [types::VISIBLE_VILLAGER_BABIES.id()];

pub struct VillagerBabiesSensor;

impl Sensor for VillagerBabiesSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let babies: Vec<Arc<dyn EntityBase>> = {
            let ctx = tick.visibility();
            ctx.state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .map(|visible| {
                    visible
                        .find_all(&ctx, |entity| {
                            entity.get_entity().entity_type == &EntityType::VILLAGER
                                && is_baby(entity.as_ref())
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        tick.state.set(types::VISIBLE_VILLAGER_BABIES, babies);
    }
}
