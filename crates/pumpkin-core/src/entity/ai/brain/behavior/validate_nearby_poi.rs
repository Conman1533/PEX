//! Vanilla `ValidateNearbyPoi`.

use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;

use super::super::BrainTick;
use super::super::memory::global_pos::GlobalPos;
use super::super::memory::{MemoryModuleType, MemoryStatus};
use super::OneShot;
use super::acquire_poi::{closer_to_center_than, poi_owner};
use crate::world::World;
use crate::world::villager_poi::{self, PoiKind};

/// Vanilla `ValidateNearbyPoi.MAX_DISTANCE`.
const MAX_DISTANCE: f64 = 16.0;

/// Whether the bed at `pos` (if it is one villagers sleep in) is occupied.
#[must_use]
pub fn bed_occupied(world: &World, pos: &BlockPos) -> Option<bool> {
    let (block, state) = world.get_block_and_state_id(pos);
    block
        .has_tag(&tag::Block::MINECRAFT_VILLAGERS_CAN_SLEEP_ON_BED)
        .then(|| {
            pumpkin_data::block_properties::WhiteBedLikeProperties::from_state_id(state).occupied
        })
}

fn bed_is_occupied_by_villager(world: &World, pos: &BlockPos) -> bool {
    let block_box = BoundingBox::from_block(pos);
    world.get_all_at_box(&block_box).iter().any(|entity| {
        entity.get_entity().entity_type == &EntityType::VILLAGER
            && entity
                .get_mob()
                .is_some_and(crate::entity::mob::Mob::is_sleeping)
    })
}

/// Vanilla `ValidateNearbyPoi.create`: within 16 blocks, forgets the POI in `memory` once it is
/// gone or its bed is taken by someone else.
///
/// PEX keeps tickets in memory only, so a villager that finds itself without a ticket for a POI
/// it remembers (after a restart or reload) takes it back, and forgets it if someone else has it.
#[must_use]
pub fn create(
    poi: impl Fn(&BrainTick<'_>, PoiKind) -> bool + Send + Sync + 'static,
    memory: MemoryModuleType<GlobalPos>,
) -> OneShot {
    OneShot::new(
        "ValidateNearbyPoi",
        vec![(memory.id(), MemoryStatus::ValuePresent)],
        move |tick| {
            let Some(global_pos) = tick.state.get(memory).copied() else {
                return false;
            };
            let pos = global_pos.pos;
            let body = tick.mob.get_entity().pos.load();
            if global_pos.dimension.id != tick.world.dimension.id
                || !closer_to_center_than(pos, body, MAX_DISTANCE)
            {
                return false;
            }
            villager_poi::ensure_scanned(tick.world, pos, 0);
            let kind = tick
                .world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kind_at(&pos);
            if !kind.is_some_and(|kind| poi(tick, kind)) {
                tick.state.erase(memory.id());
            } else if bed_occupied(tick.world, &pos) == Some(true) && !tick.mob.is_sleeping() {
                tick.state.erase(memory.id());
                if !bed_is_occupied_by_villager(tick.world, &pos) {
                    tick.world
                        .villager_poi
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .release_any(pos);
                }
            } else if let Some(owner) = poi_owner(tick) {
                let mut storage = tick
                    .world
                    .villager_poi
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let kept = storage.is_owned_by(&pos, tick.mob.get_entity().entity_uuid)
                    || storage.take(pos, &owner);
                drop(storage);
                if !kept {
                    tick.state.erase(memory.id());
                }
            }
            true
        },
    )
}
