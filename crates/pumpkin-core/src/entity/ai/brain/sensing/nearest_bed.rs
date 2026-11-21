//! Vanilla `NearestBedSensor`.

use rand::RngExt;
use rustc_hash::FxHashMap;

use super::super::BrainTick;
use super::super::behavior::acquire_poi::find_path_to_pois;
use super::super::behavior::utils::is_baby;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use crate::world::villager_poi::{self, Occupancy, PoiKind};

/// Vanilla `NearestBedSensor.CACHE_TIMEOUT`.
const CACHE_TIMEOUT: i64 = 40;
/// Vanilla `NearestBedSensor.BATCH_SIZE`.
const BATCH_SIZE: usize = 5;
/// Vanilla `AcquirePoi.SCAN_RANGE`.
const SCAN_RANGE: i32 = 48;

const REQUIRES: [MemoryModuleId; 1] = [types::NEAREST_BED.id()];

#[derive(Default)]
pub struct NearestBedSensor {
    batch_cache: FxHashMap<pumpkin_util::math::position::BlockPos, i64>,
    last_update: i64,
}

impl Sensor for NearestBedSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        if !is_baby(tick.mob) {
            return;
        }
        self.last_update = tick.time + i64::from(tick.mob.get_random().random_range(0..20));
        let origin = tick.mob.get_entity().block_pos.load();
        villager_poi::ensure_scanned(tick.world, origin, SCAN_RANGE);
        let candidates = tick
            .world
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .find_all_closest_first(
                origin,
                SCAN_RANGE,
                |kind| kind == PoiKind::Home,
                Occupancy::Any,
            );
        let mut tried = 0;
        let mut batch = Vec::new();
        for (pos, kind) in candidates {
            if self.batch_cache.contains_key(&pos) {
                continue;
            }
            tried += 1;
            if tried >= BATCH_SIZE {
                break;
            }
            self.batch_cache
                .insert(pos, self.last_update + CACHE_TIMEOUT);
            batch.push((pos, kind));
        }
        if let Some(target) = find_path_to_pois(tick, &batch) {
            tick.state.set(types::NEAREST_BED, target);
        } else if tried < BATCH_SIZE {
            let last_update = self.last_update;
            self.batch_cache.retain(|_, expiry| *expiry >= last_update);
        }
    }
}
