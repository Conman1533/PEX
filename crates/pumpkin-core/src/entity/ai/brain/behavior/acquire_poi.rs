//! Vanilla `AcquirePoi`, plus the point-of-interest helpers the village behaviors share.

use std::sync::Arc;

use pumpkin_data::entity::EntityStatus;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::{Rng, RngExt};
use rustc_hash::FxHashMap;

use super::super::BrainTick;
use super::super::memory::global_pos::GlobalPos;
use super::super::memory::{MemoryModuleType, MemoryStatus};
use super::OneShot;
use super::utils::is_baby;
use crate::entity::EntityBase;
use crate::entity::ai::pathfinder::Navigator;
use crate::world::World;
use crate::world::villager_poi::{self, Occupancy, PoiKind};

/// Vanilla `AcquirePoi.SCAN_RANGE`.
pub const SCAN_RANGE: i32 = 48;
/// Vanilla `AcquirePoi`'s `batchSize`.
const BATCH_SIZE: usize = 5;
/// Vanilla `AcquirePoi`'s `rate`.
const RATE: i64 = 20;
/// Vanilla `Villager`'s `setRequiredPathLength(48.0F)`.
pub const VILLAGER_PATH_LENGTH: f32 = 48.0;

/// Vanilla `Vec3i.closerToCenterThan`.
#[must_use]
pub fn closer_to_center_than(pos: BlockPos, position: Vector3<f64>, distance: f64) -> bool {
    pos.to_centered_f64().squared_distance_to_vec(&position) < distance * distance
}

/// The brain's own entity, for holding a POI ticket.
#[must_use]
pub fn poi_owner(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    tick.world.get_entity_by_id(tick.mob.get_entity().entity_id)
}

/// Vanilla `Villager.releasePoi`: gives back the ticket the POI in `memory` holds for this mob.
pub fn release_poi(tick: &BrainTick<'_>, memory: MemoryModuleType<GlobalPos>) {
    let Some(pos) = tick.state.get(memory).copied() else {
        return;
    };
    if pos.dimension.id != tick.world.dimension.id {
        return;
    }
    tick.world
        .villager_poi
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .release(pos.pos, tick.mob.get_entity().entity_uuid);
}

/// A navigation set up like a villager's (doors, floating, 48 block paths) for reach checks.
#[must_use]
pub fn villager_navigator() -> Navigator {
    let mut navigator = Navigator::default();
    navigator.set_can_open_doors(true);
    navigator.set_can_float(true);
    navigator.set_required_path_length(VILLAGER_PATH_LENGTH);
    navigator
}

/// Vanilla `AcquirePoi.findPathToPois`: the first of `pois` (closest first) a path reaches
/// within the largest valid range among them.
#[must_use]
pub fn find_path_to_pois(tick: &BrainTick<'_>, pois: &[(BlockPos, PoiKind)]) -> Option<BlockPos> {
    if pois.is_empty() {
        return None;
    }
    let max_range = pois
        .iter()
        .map(|(_, kind)| kind.valid_range())
        .fold(1, i32::max);
    let mut navigator = villager_navigator();
    let living = &tick.mob.get_mob_entity().living_entity;
    pois.iter()
        .map(|(pos, _)| *pos)
        .find(|pos| navigator.can_reach_within(living, pos.to_centered_f64(), max_range as f32))
}

/// Vanilla `AcquirePoi.JitteredLinearRetry`.
struct JitteredLinearRetry {
    previous_attempt: i64,
    next_scheduled_attempt: i64,
    current_delay: i64,
}

impl JitteredLinearRetry {
    const MAX_RETRY_PATHFINDING_INTERVAL: i64 = 400;

    fn new(rng: &mut impl Rng, first_attempt: i64) -> Self {
        let mut retry = Self {
            previous_attempt: 0,
            next_scheduled_attempt: 0,
            current_delay: 0,
        };
        retry.mark_attempt(rng, first_attempt);
        retry
    }

    fn mark_attempt(&mut self, rng: &mut impl Rng, timestamp: i64) {
        self.previous_attempt = timestamp;
        let suggested = self.current_delay + i64::from(rng.random_range(0..40)) + 40;
        self.current_delay = suggested.min(Self::MAX_RETRY_PATHFINDING_INTERVAL);
        self.next_scheduled_attempt = timestamp + self.current_delay;
    }

    const fn is_still_valid(&self, timestamp: i64) -> bool {
        timestamp - self.previous_attempt < Self::MAX_RETRY_PATHFINDING_INTERVAL
    }

    const fn should_retry(&self, timestamp: i64) -> bool {
        timestamp >= self.next_scheduled_attempt
    }
}

/// Vanilla `AcquirePoi.create`: every second or two, takes the closest reachable free POI of
/// a kind and remembers it in `memory_to_acquire`.
#[must_use]
pub fn create(
    poi: impl Fn(&BrainTick<'_>, PoiKind) -> bool + Send + Sync + 'static,
    memory_to_validate: MemoryModuleType<GlobalPos>,
    memory_to_acquire: MemoryModuleType<GlobalPos>,
    only_if_adult: bool,
    happy_on_acquisition: bool,
    valid_poi: impl Fn(&World, BlockPos) -> bool + Send + Sync + 'static,
) -> OneShot {
    let mut next_scheduled_start = 0i64;
    let mut batch_cache: FxHashMap<BlockPos, JitteredLinearRetry> = FxHashMap::default();
    let mut conditions = vec![(memory_to_acquire.id(), MemoryStatus::ValueAbsent)];
    if memory_to_validate.id() != memory_to_acquire.id() {
        conditions.push((memory_to_validate.id(), MemoryStatus::ValueAbsent));
    }
    OneShot::new("AcquirePoi", conditions, move |tick| {
        if only_if_adult && is_baby(tick.mob) {
            return false;
        }
        let mut rng = tick.mob.get_random();
        let timestamp = tick.time;
        if next_scheduled_start == 0 {
            next_scheduled_start = timestamp + i64::from(rng.random_range(0..20));
            return false;
        }
        if timestamp < next_scheduled_start {
            return false;
        }
        next_scheduled_start = timestamp + RATE + i64::from(rng.random_range(0..20));
        batch_cache.retain(|_, retry| retry.is_still_valid(timestamp));

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
                |kind| poi(tick, kind),
                Occupancy::HasSpace,
            );
        let batch: Vec<(BlockPos, PoiKind)> = candidates
            .into_iter()
            .filter(|(pos, _)| match batch_cache.get_mut(pos) {
                None => true,
                Some(retry) if retry.should_retry(timestamp) => {
                    retry.mark_attempt(&mut rng, timestamp);
                    true
                }
                Some(_) => false,
            })
            .take(BATCH_SIZE)
            .filter(|(pos, _)| valid_poi(tick.world, *pos))
            .collect();

        if let Some(target) = find_path_to_pois(tick, &batch) {
            let owner = poi_owner(tick);
            let taken = {
                let mut storage = tick
                    .world
                    .villager_poi
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                storage.kind_at(&target).is_some().then(|| {
                    if let Some(owner) = &owner {
                        storage.take(target, owner);
                    }
                })
            };
            if taken.is_some() {
                tick.state
                    .set_optional(memory_to_acquire, GlobalPos::in_world(tick.world, target));
                if happy_on_acquisition {
                    tick.world.send_entity_status(
                        tick.mob.get_entity(),
                        EntityStatus::VillagerHappy,
                        ActorEventID::VillagerHappy,
                    );
                }
                batch_cache.clear();
            }
        } else {
            for (pos, _) in batch {
                batch_cache
                    .entry(pos)
                    .or_insert_with(|| JitteredLinearRetry::new(&mut rng, timestamp));
            }
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_linearly_and_caps_at_twenty_seconds() {
        let mut rng = rand::rng();
        let mut retry = JitteredLinearRetry::new(&mut rng, 0);
        assert!((40..80).contains(&retry.current_delay));
        assert!(!retry.should_retry(retry.current_delay - 1));
        for _ in 0..20 {
            retry.mark_attempt(&mut rng, 100);
        }
        assert_eq!(retry.current_delay, 400);
        assert!(retry.is_still_valid(499));
        assert!(!retry.is_still_valid(500));
    }

    #[test]
    fn closer_to_center_is_measured_from_the_block_center() {
        let pos = BlockPos::new(0, 64, 0);
        assert!(closer_to_center_than(
            pos,
            Vector3::new(1.9, 64.5, 0.5),
            2.0
        ));
        assert!(!closer_to_center_than(
            pos,
            Vector3::new(2.5, 64.5, 0.5),
            2.0
        ));
    }
}
