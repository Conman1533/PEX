//! Vanilla `VillagerHostilesSensor`.

use pumpkin_data::entity::EntityType;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;

const REQUIRES: [MemoryModuleId; 2] = [
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_HOSTILE.id(),
];

/// Vanilla `VillagerHostilesSensor.ACCEPTABLE_DISTANCE_FROM_HOSTILES`.
#[must_use]
pub fn acceptable_distance_from(entity_type: &EntityType) -> Option<f32> {
    Some(match entity_type {
        t if t == &EntityType::DROWNED => 8.0,
        t if t == &EntityType::EVOKER => 12.0,
        t if t == &EntityType::HUSK => 8.0,
        t if t == &EntityType::ILLUSIONER => 12.0,
        t if t == &EntityType::PILLAGER => 15.0,
        t if t == &EntityType::RAVAGER => 12.0,
        t if t == &EntityType::VEX => 8.0,
        t if t == &EntityType::VINDICATOR => 10.0,
        t if t == &EntityType::ZOGLIN => 10.0,
        t if t == &EntityType::ZOMBIE => 8.0,
        t if t == &EntityType::ZOMBIE_VILLAGER => 8.0,
        _ => return None,
    })
}

pub struct VillagerHostilesSensor;

impl Sensor for VillagerHostilesSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let body = tick.mob.get_entity().pos.load();
        let nearest = {
            let ctx = tick.visibility();
            ctx.state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .and_then(|visible| {
                    visible.find_closest(&ctx, |entity| {
                        acceptable_distance_from(entity.get_entity().entity_type).is_some_and(
                            |distance| {
                                entity
                                    .get_entity()
                                    .pos
                                    .load()
                                    .squared_distance_to_vec(&body)
                                    <= f64::from(distance * distance)
                            },
                        )
                    })
                })
        };
        tick.state.set_optional(types::NEAREST_HOSTILE, nearest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_distances_match_vanilla() {
        assert_eq!(acceptable_distance_from(&EntityType::PILLAGER), Some(15.0));
        assert_eq!(
            acceptable_distance_from(&EntityType::VINDICATOR),
            Some(10.0)
        );
        assert_eq!(acceptable_distance_from(&EntityType::ZOMBIE), Some(8.0));
        assert_eq!(acceptable_distance_from(&EntityType::SKELETON), None);
        assert_eq!(acceptable_distance_from(&EntityType::CREEPER), None);
    }
}
