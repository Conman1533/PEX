//! The village points of interest of vanilla `PoiManager`: job sites, beds (`minecraft:home`)
//! and bells (`minecraft:meeting`), with vanilla's ticket counts.
//!
//! Positions are learned from block changes and, the first time a villager looks around, from a
//! scan of the loaded chunks near it (vanilla reads them from the POI region files or rescans
//! the section). Tickets are held by live entities, so a villager that unloads or dies frees its
//! ticket and takes it back when it next validates its memory.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use pumpkin_data::block_properties::{BedPart, WhiteBedLikeProperties as BedProperties};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId, villager::VillagerProfession};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use uuid::Uuid;

use crate::entity::EntityBase;
use crate::world::World;

/// Vanilla `PoiManager.MAX_VILLAGE_DISTANCE`: how many sections away a village is still felt.
pub const MAX_VILLAGE_DISTANCE: i32 = 6;

/// The village point of interest types of vanilla `PoiTypes`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PoiKind {
    Job(VillagerProfession),
    Home,
    Meeting,
}

impl PoiKind {
    /// Vanilla `PoiType.maxTickets`.
    #[must_use]
    pub const fn max_tickets(self) -> usize {
        match self {
            Self::Job(_) | Self::Home => 1,
            Self::Meeting => 32,
        }
    }

    /// Vanilla `PoiType.validRange`.
    #[must_use]
    pub const fn valid_range(self) -> i32 {
        match self {
            Self::Job(_) | Self::Home => 1,
            Self::Meeting => 6,
        }
    }

    /// The id vanilla saves in the POI region files.
    #[must_use]
    pub const fn type_name(self) -> Option<&'static str> {
        match self {
            Self::Job(profession) => job_type_name(profession),
            Self::Home => Some("minecraft:home"),
            Self::Meeting => Some("minecraft:meeting"),
        }
    }

    #[must_use]
    pub const fn is_job(self) -> bool {
        matches!(self, Self::Job(_))
    }
}

struct Poi {
    kind: PoiKind,
    owners: Vec<(Uuid, Weak<dyn EntityBase>)>,
}

impl Poi {
    const fn new(kind: PoiKind) -> Self {
        Self {
            kind,
            owners: Vec::new(),
        }
    }

    fn prune(&mut self) {
        self.owners
            .retain(|(_, owner)| VillagerPoiStorage::live_owner(owner).is_some());
    }

    fn live_owner_count(&self) -> usize {
        self.owners
            .iter()
            .filter(|(_, owner)| VillagerPoiStorage::live_owner(owner).is_some())
            .count()
    }

    fn has_space(&self) -> bool {
        self.live_owner_count() < self.kind.max_tickets()
    }

    fn is_owned_by(&self, owner: Uuid) -> bool {
        self.owners
            .iter()
            .any(|(uuid, weak)| *uuid == owner && VillagerPoiStorage::live_owner(weak).is_some())
    }
}

/// Vanilla `PoiManager.Occupancy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Occupancy {
    HasSpace,
    IsOccupied,
    Any,
}

#[derive(Default)]
pub struct VillagerPoiStorage {
    pois: HashMap<BlockPos, Poi>,
    scanned_chunks: HashSet<(i32, i32)>,
}

impl VillagerPoiStorage {
    fn live_owner(owner: &Weak<dyn EntityBase>) -> Option<Arc<dyn EntityBase>> {
        owner.upgrade().filter(|entity| {
            !entity.get_entity().is_removed()
                && entity
                    .get_living_entity()
                    .is_none_or(|living| living.health.load() > 0.0)
        })
    }

    /// Records the point of interest a block change leaves at `position`, if any.
    pub fn update_block(&mut self, position: BlockPos, block: &Block, state: BlockStateId) {
        match poi_kind_for_state(block, state) {
            None => {
                self.pois.remove(&position);
            }
            Some(kind) => {
                let poi = self.pois.entry(position).or_insert_with(|| Poi::new(kind));
                if poi.kind != kind {
                    *poi = Poi::new(kind);
                }
            }
        }
    }

    /// Adds points of interest found by a chunk scan; known positions keep their tickets.
    pub fn insert_scanned(&mut self, chunk: (i32, i32), found: Vec<(BlockPos, PoiKind)>) {
        self.scanned_chunks.insert(chunk);
        for (position, kind) in found {
            self.pois.entry(position).or_insert_with(|| Poi::new(kind));
        }
    }

    fn chunks_to_scan(&self, center: BlockPos, radius: i32) -> Vec<(i32, i32)> {
        let min_x = (center.0.x - radius) >> 4;
        let max_x = (center.0.x + radius) >> 4;
        let min_z = (center.0.z - radius) >> 4;
        let max_z = (center.0.z + radius) >> 4;
        let mut chunks = Vec::new();
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if !self.scanned_chunks.contains(&(x, z)) {
                    chunks.push((x, z));
                }
            }
        }
        chunks
    }

    #[must_use]
    pub fn kind_at(&self, position: &BlockPos) -> Option<PoiKind> {
        self.pois.get(position).map(|poi| poi.kind)
    }

    /// Vanilla `PoiManager.exists`.
    #[must_use]
    pub fn exists(&self, position: &BlockPos, predicate: impl Fn(PoiKind) -> bool) -> bool {
        self.kind_at(position).is_some_and(predicate)
    }

    #[must_use]
    pub fn is_owned_by(&self, position: &BlockPos, owner: Uuid) -> bool {
        self.pois
            .get(position)
            .is_some_and(|poi| poi.is_owned_by(owner))
    }

    #[must_use]
    pub fn has_space(&self, position: &BlockPos) -> bool {
        self.pois.get(position).is_some_and(Poi::has_space)
    }

    /// Vanilla `PoiManager.take` for one position: false when it is full or not a POI. An owner
    /// that already holds a ticket keeps it.
    pub fn take(&mut self, position: BlockPos, owner: &Arc<dyn EntityBase>) -> bool {
        let Some(poi) = self.pois.get_mut(&position) else {
            return false;
        };
        let uuid = owner.get_entity().entity_uuid;
        poi.prune();
        if poi.owners.iter().any(|(holder, _)| *holder == uuid) {
            return true;
        }
        if poi.owners.len() >= poi.kind.max_tickets() {
            return false;
        }
        poi.owners.push((uuid, Arc::downgrade(owner)));
        true
    }

    /// Vanilla `PoiManager.release` for the ticket `owner` holds.
    pub fn release(&mut self, position: BlockPos, owner: Uuid) {
        if let Some(poi) = self.pois.get_mut(&position) {
            poi.owners.retain(|(holder, _)| *holder != owner);
        }
    }

    /// Vanilla `PoiManager.release` without an owner: frees one ticket.
    pub fn release_any(&mut self, position: BlockPos) {
        if let Some(poi) = self.pois.get_mut(&position) {
            poi.prune();
            poi.owners.pop();
        }
    }

    fn matches_occupancy(poi: &Poi, occupancy: Occupancy) -> bool {
        match occupancy {
            Occupancy::HasSpace => poi.has_space(),
            Occupancy::IsOccupied => poi.live_owner_count() > 0,
            Occupancy::Any => true,
        }
    }

    /// Vanilla `PoiManager.findAllClosestFirstWithType`: every matching POI within `radius`
    /// (squared block distance), closest first.
    #[must_use]
    pub fn find_all_closest_first(
        &self,
        origin: BlockPos,
        radius: i32,
        predicate: impl Fn(PoiKind) -> bool,
        occupancy: Occupancy,
    ) -> Vec<(BlockPos, PoiKind)> {
        let radius_squared = i64::from(radius).pow(2);
        let mut found: Vec<(i64, BlockPos, PoiKind)> = self
            .pois
            .iter()
            .filter(|(_, poi)| predicate(poi.kind) && Self::matches_occupancy(poi, occupancy))
            .filter_map(|(position, poi)| {
                let distance = dist_sqr(*position, origin);
                (distance <= radius_squared).then_some((distance, *position, poi.kind))
            })
            .collect();
        found.sort_unstable_by_key(|(distance, position, _)| {
            (*distance, position.0.x, position.0.y, position.0.z)
        });
        found
            .into_iter()
            .map(|(_, position, kind)| (position, kind))
            .collect()
    }

    /// Vanilla `PoiManager.findClosest`.
    #[must_use]
    pub fn find_closest(
        &self,
        origin: BlockPos,
        radius: i32,
        predicate: impl Fn(PoiKind) -> bool,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        self.find_all_closest_first(origin, radius, predicate, occupancy)
            .first()
            .map(|(position, _)| *position)
    }

    /// The job sites without a holder within `radius` blocks, closest first.
    #[must_use]
    pub fn available_job_sites(
        &self,
        origin: BlockPos,
        radius: i32,
        expected_profession: Option<VillagerProfession>,
    ) -> Vec<BlockPos> {
        self.find_all_closest_first(
            origin,
            radius,
            |kind| match kind {
                PoiKind::Job(profession) => {
                    expected_profession.is_none_or(|expected| expected == profession)
                }
                _ => false,
            },
            Occupancy::HasSpace,
        )
        .into_iter()
        .map(|(position, _)| position)
        .collect()
    }

    /// The closest village POI (job site, bed or bell) within `radius` blocks.
    #[must_use]
    pub fn get_nearest_village_poi(&self, origin: BlockPos, radius: i32) -> Option<BlockPos> {
        self.find_closest(origin, radius, |_| true, Occupancy::Any)
    }

    /// The sections holding village POIs, to measure village distances without the lock.
    #[must_use]
    pub fn village_sections(&self) -> VillageSections {
        let mut sections: Vec<(i32, i32, i32)> =
            self.pois.keys().map(|pos| section_of(*pos)).collect();
        sections.sort_unstable();
        sections.dedup();
        VillageSections(sections)
    }

    /// Vanilla `PoiManager.sectionsToVillage`.
    #[must_use]
    pub fn sections_to_village(&self, position: BlockPos) -> i32 {
        self.village_sections().sections_to_village(position)
    }

    /// Vanilla `ServerLevel.isVillage`.
    #[must_use]
    pub fn is_village(&self, position: BlockPos) -> bool {
        self.sections_to_village(position) <= 1
    }
}

/// A snapshot of the sections that hold village POIs.
pub struct VillageSections(Vec<(i32, i32, i32)>);

impl VillageSections {
    /// Vanilla `PoiManager.sectionsToVillage` for a section: the chebyshev section distance to
    /// the closest section holding a village POI, at most [`MAX_VILLAGE_DISTANCE`].
    #[must_use]
    pub fn sections_to_village_from(&self, section: (i32, i32, i32)) -> i32 {
        self.0
            .iter()
            .map(|other| {
                (other.0 - section.0)
                    .abs()
                    .max((other.1 - section.1).abs())
                    .max((other.2 - section.2).abs())
            })
            .min()
            .unwrap_or(MAX_VILLAGE_DISTANCE)
            .min(MAX_VILLAGE_DISTANCE)
    }

    #[must_use]
    pub fn sections_to_village(&self, position: BlockPos) -> i32 {
        self.sections_to_village_from(section_of(position))
    }

    /// Vanilla `ServerLevel.isVillage`.
    #[must_use]
    pub fn is_village(&self, position: BlockPos) -> bool {
        self.sections_to_village(position) <= 1
    }

    /// Vanilla `BehaviorUtils.findSectionClosestToVillage`.
    #[must_use]
    pub fn find_section_closest_to_village(
        &self,
        center: (i32, i32, i32),
        radius: i32,
    ) -> (i32, i32, i32) {
        let here = self.sections_to_village_from(center);
        let mut best: Option<((i32, i32, i32), i32)> = None;
        for x in -radius..=radius {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    let section = (center.0 + x, center.1 + y, center.2 + z);
                    let distance = self.sections_to_village_from(section);
                    if distance < here && best.is_none_or(|(_, d)| distance < d) {
                        best = Some((section, distance));
                    }
                }
            }
        }
        best.map_or(center, |(section, _)| section)
    }
}

#[must_use]
pub const fn section_of(position: BlockPos) -> (i32, i32, i32) {
    (position.0.x >> 4, position.0.y >> 4, position.0.z >> 4)
}

/// Vanilla `Vec3i.distSqr` between two block positions.
#[must_use]
pub fn dist_sqr(a: BlockPos, b: BlockPos) -> i64 {
    let delta = a.0 - b.0;
    i64::from(delta.x).pow(2) + i64::from(delta.y).pow(2) + i64::from(delta.z).pow(2)
}

/// Scans the loaded, not yet scanned chunks within `radius` blocks of `center` for village POIs.
/// Never call it with the world's `villager_poi` lock held: it reads chunks.
pub fn ensure_scanned(world: &World, center: BlockPos, radius: i32) {
    let chunks = world
        .villager_poi
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .chunks_to_scan(center, radius);
    for chunk in chunks {
        let found = world
            .level
            .read_chunk_sync(&Vector2::new(chunk.0, chunk.1), |sync| {
                let sections = sync
                    .section
                    .block_sections
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut found = Vec::new();
                for (index, section) in sections.iter().enumerate() {
                    if !section.palette_contains(is_poi_state) {
                        continue;
                    }
                    let base_y = sync.section.min_y + index as i32 * 16;
                    for y in 0..16 {
                        for z in 0..16 {
                            for x in 0..16 {
                                let state = section.get(x, y, z);
                                let block = Block::from_state_id(state);
                                if let Some(kind) = poi_kind_for_state(block, state) {
                                    found.push((
                                        BlockPos::new(
                                            (chunk.0 << 4) + x as i32,
                                            base_y + y as i32,
                                            (chunk.1 << 4) + z as i32,
                                        ),
                                        kind,
                                    ));
                                }
                            }
                        }
                    }
                }
                found
            });
        if let Some(found) = found {
            world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert_scanned(chunk, found);
        }
    }
}

fn is_poi_state(state: BlockStateId) -> bool {
    poi_kind_for_state(Block::from_state_id(state), state).is_some()
}

/// The village point of interest a block state is, as vanilla `PoiTypes.forState`.
#[must_use]
pub fn poi_kind_for_state(block: &Block, state: BlockStateId) -> Option<PoiKind> {
    if let Some(profession) = profession_for_block(block) {
        return Some(PoiKind::Job(profession));
    }
    if block == &Block::BELL {
        return Some(PoiKind::Meeting);
    }
    if block.has_tag(&tag::Block::MINECRAFT_BEDS)
        && BedProperties::from_state_id(state).part == BedPart::Head
    {
        return Some(PoiKind::Home);
    }
    None
}

#[must_use]
pub fn profession_for_block(block: &Block) -> Option<VillagerProfession> {
    match block {
        block if block == &Block::COMPOSTER => Some(VillagerProfession::Farmer),
        block if block == &Block::LECTERN => Some(VillagerProfession::Librarian),
        block if block == &Block::BLAST_FURNACE => Some(VillagerProfession::Armorer),
        block if block == &Block::SMOKER => Some(VillagerProfession::Butcher),
        block if block == &Block::CARTOGRAPHY_TABLE => Some(VillagerProfession::Cartographer),
        block if block == &Block::BREWING_STAND => Some(VillagerProfession::Cleric),
        block if block == &Block::BARREL => Some(VillagerProfession::Fisherman),
        block if block == &Block::FLETCHING_TABLE => Some(VillagerProfession::Fletcher),
        block
            if [
                &Block::CAULDRON,
                &Block::WATER_CAULDRON,
                &Block::LAVA_CAULDRON,
                &Block::POWDER_SNOW_CAULDRON,
            ]
            .contains(&block) =>
        {
            Some(VillagerProfession::Leatherworker)
        }
        block if block == &Block::STONECUTTER => Some(VillagerProfession::Mason),
        block if block == &Block::LOOM => Some(VillagerProfession::Shepherd),
        block if block == &Block::SMITHING_TABLE => Some(VillagerProfession::Toolsmith),
        block if block == &Block::GRINDSTONE => Some(VillagerProfession::Weaponsmith),
        _ => None,
    }
}

const fn job_type_name(profession: VillagerProfession) -> Option<&'static str> {
    Some(match profession {
        VillagerProfession::Armorer => "minecraft:armorer",
        VillagerProfession::Butcher => "minecraft:butcher",
        VillagerProfession::Cartographer => "minecraft:cartographer",
        VillagerProfession::Cleric => "minecraft:cleric",
        VillagerProfession::Farmer => "minecraft:farmer",
        VillagerProfession::Fisherman => "minecraft:fisherman",
        VillagerProfession::Fletcher => "minecraft:fletcher",
        VillagerProfession::Leatherworker => "minecraft:leatherworker",
        VillagerProfession::Librarian => "minecraft:librarian",
        VillagerProfession::Mason => "minecraft:mason",
        VillagerProfession::Shepherd => "minecraft:shepherd",
        VillagerProfession::Toolsmith => "minecraft:toolsmith",
        VillagerProfession::Weaponsmith => "minecraft:weaponsmith",
        VillagerProfession::None | VillagerProfession::Nitwit => return None,
    })
}

#[must_use]
pub fn poi_type_for_block(block: &Block) -> Option<&'static str> {
    job_type_name(profession_for_block(block)?)
}

/// The id vanilla saves for the POI a block state is, if any.
#[must_use]
pub fn poi_type_for_state(block: &Block, state: BlockStateId) -> Option<&'static str> {
    poi_kind_for_state(block, state)?.type_name()
}

#[cfg(test)]
mod tests {
    use pumpkin_data::block_properties::HorizontalFacing;

    use super::*;

    #[test]
    fn every_villager_workstation_maps_to_its_profession() {
        let workstations = [
            (&Block::COMPOSTER, VillagerProfession::Farmer),
            (&Block::LECTERN, VillagerProfession::Librarian),
            (&Block::BLAST_FURNACE, VillagerProfession::Armorer),
            (&Block::SMOKER, VillagerProfession::Butcher),
            (&Block::CARTOGRAPHY_TABLE, VillagerProfession::Cartographer),
            (&Block::BREWING_STAND, VillagerProfession::Cleric),
            (&Block::BARREL, VillagerProfession::Fisherman),
            (&Block::FLETCHING_TABLE, VillagerProfession::Fletcher),
            (&Block::CAULDRON, VillagerProfession::Leatherworker),
            (&Block::STONECUTTER, VillagerProfession::Mason),
            (&Block::LOOM, VillagerProfession::Shepherd),
            (&Block::SMITHING_TABLE, VillagerProfession::Toolsmith),
            (&Block::GRINDSTONE, VillagerProfession::Weaponsmith),
        ];

        for (block, profession) in workstations {
            assert_eq!(profession_for_block(block), Some(profession));
            assert_eq!(
                poi_type_for_block(block).unwrap(),
                format!(
                    "minecraft:{}",
                    profession.translation_key().rsplit('.').next().unwrap()
                )
            );
        }
        assert_eq!(profession_for_block(&Block::DIRT), None);
        assert_eq!(poi_type_for_block(&Block::DIRT), None);
    }

    #[test]
    fn only_the_head_of_a_bed_and_any_bell_are_village_pois() {
        let red_bed = &Block::RED_BED;
        let state = |part| {
            BedProperties {
                facing: HorizontalFacing::North,
                occupied: false,
                part,
            }
            .to_state_id(red_bed)
        };
        assert_eq!(
            poi_kind_for_state(red_bed, state(BedPart::Head)),
            Some(PoiKind::Home)
        );
        assert_eq!(poi_kind_for_state(red_bed, state(BedPart::Foot)), None);
        assert_eq!(
            poi_kind_for_state(&Block::BELL, Block::BELL.default_state.id),
            Some(PoiKind::Meeting)
        );
        assert_eq!(PoiKind::Meeting.max_tickets(), 32);
        assert_eq!(PoiKind::Meeting.valid_range(), 6);
        assert_eq!(PoiKind::Home.type_name(), Some("minecraft:home"));
    }

    #[test]
    fn available_job_sites_respect_profession_and_vanilla_search_radius() {
        let origin = BlockPos::new(0, 64, 0);
        let lectern = BlockPos::new(48, 64, 0);
        let too_far = BlockPos::new(49, 64, 0);
        let composter = BlockPos::new(1, 64, 0);
        let mut storage = VillagerPoiStorage::default();

        storage.update_block(lectern, &Block::LECTERN, Block::LECTERN.default_state.id);
        storage.update_block(too_far, &Block::LECTERN, Block::LECTERN.default_state.id);
        storage.update_block(
            composter,
            &Block::COMPOSTER,
            Block::COMPOSTER.default_state.id,
        );

        assert_eq!(
            storage.available_job_sites(origin, 48, Some(VillagerProfession::Librarian)),
            vec![lectern]
        );
        assert_eq!(
            storage.available_job_sites(origin, 48, Some(VillagerProfession::Farmer)),
            vec![composter]
        );
    }

    #[test]
    fn sections_to_village_counts_chebyshev_sections_up_to_six() {
        let mut storage = VillagerPoiStorage::default();
        let origin = BlockPos::new(8, 64, 8);
        assert_eq!(storage.sections_to_village(origin), MAX_VILLAGE_DISTANCE);
        storage.update_block(
            BlockPos::new(40, 64, 8),
            &Block::BELL,
            Block::BELL.default_state.id,
        );
        assert_eq!(storage.sections_to_village(origin), 2);
        assert!(!storage.is_village(origin));
        assert!(storage.is_village(BlockPos::new(24, 64, 8)));
        assert_eq!(
            storage.sections_to_village(BlockPos::new(8, 64, 500)),
            MAX_VILLAGE_DISTANCE
        );
    }

    #[test]
    fn removing_the_block_forgets_the_poi() {
        let mut storage = VillagerPoiStorage::default();
        let bell = BlockPos::new(0, 64, 0);
        storage.update_block(bell, &Block::BELL, Block::BELL.default_state.id);
        assert!(storage.exists(&bell, |kind| kind == PoiKind::Meeting));
        storage.update_block(bell, &Block::AIR, Block::AIR.default_state.id);
        assert!(!storage.exists(&bell, |_| true));
    }
}
