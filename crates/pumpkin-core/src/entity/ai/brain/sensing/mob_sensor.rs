use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryModuleType, types};
use super::Sensor;
use crate::entity::EntityBase;
use crate::entity::mob::Mob;

const REQUIRES: [MemoryModuleId; 1] = [types::NEAREST_LIVING_ENTITIES.id()];

/// Vanilla `MobSensor`: while the mob is ready, a nearby living entity passing `mob_test` sets
/// `to_set` for `memory_time_to_live` ticks; while it isn't, `to_set` is erased.
pub struct MobSensor {
    mob_test: fn(&dyn Mob, &dyn EntityBase) -> bool,
    ready_test: fn(&BrainTick<'_>) -> bool,
    to_set: MemoryModuleType<bool>,
    memory_time_to_live: i64,
}

impl MobSensor {
    #[must_use]
    pub const fn new(
        mob_test: fn(&dyn Mob, &dyn EntityBase) -> bool,
        ready_test: fn(&BrainTick<'_>) -> bool,
        to_set: MemoryModuleType<bool>,
        memory_time_to_live: i64,
    ) -> Self {
        Self {
            mob_test,
            ready_test,
            to_set,
            memory_time_to_live,
        }
    }
}

impl Sensor for MobSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        if !(self.ready_test)(tick) {
            tick.state.erase(self.to_set.id());
            return;
        }
        let mob = tick.mob;
        let detected = tick
            .state
            .get(types::NEAREST_LIVING_ENTITIES)
            .is_some_and(|entities| {
                entities
                    .iter()
                    .any(|entity| (self.mob_test)(mob, entity.as_ref()))
            });
        if detected {
            tick.state
                .set_with_expiry(self.to_set, true, self.memory_time_to_live);
        }
    }
}
