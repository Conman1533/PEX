use rustc_hash::{FxHashMap, FxHashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use pumpkin_config::chunk::AnvilChunkConfig;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::math::vector3::Vector3;
use serde::{Deserialize, Serialize};

use crate::chunk::format::anvil::{AnvilRegion, CHUNK_COUNT, WORLD_DATA_VERSION};
use crate::chunk::io::{RegionFile, chunk_in_region, region_chunk_index};

/// POI type identifier for nether portals
pub const POI_TYPE_NETHER_PORTAL: &str = "minecraft:nether_portal";

/// A single Point of Interest entry (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoiEntry {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    #[serde(rename = "type")]
    pub poi_type: String,
    pub free_tickets: i32,
}

impl PoiEntry {
    #[must_use]
    pub fn new_portal(pos: BlockPos) -> Self {
        Self {
            x: pos.0.x,
            y: pos.0.y,
            z: pos.0.z,
            poi_type: POI_TYPE_NETHER_PORTAL.to_string(),
            free_tickets: 0,
        }
    }

    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        BlockPos(Vector3::new(self.x, self.y, self.z))
    }

    const fn chunk(&self) -> (i32, i32) {
        (self.x >> 4, self.z >> 4)
    }

    const fn section(&self) -> (i32, i32, i32) {
        (self.x >> 4, self.y >> 4, self.z >> 4)
    }

    /// Reads a record like `PoiRecord.Packed.CODEC`; records from before `pos` existed keep
    /// `x`, `y` and `z`.
    fn from_nbt(record: &NbtCompound) -> Option<Self> {
        let [x, y, z] = match record.get_int_array("pos") {
            Some(&[x, y, z]) => [x, y, z],
            Some(_) => return None,
            None => [
                record.get_int("x")?,
                record.get_int("y")?,
                record.get_int("z")?,
            ],
        };
        Some(Self {
            x,
            y,
            z,
            poi_type: record.get_string("type")?.to_string(),
            free_tickets: record.get_int("free_tickets").unwrap_or(0),
        })
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut record = NbtCompound::new();
        record.put("pos", NbtTag::IntArray(vec![self.x, self.y, self.z]));
        record.put_string("type", self.poi_type.clone());
        // `optionalFieldOf("free_tickets", 0)` leaves out the default.
        if self.free_tickets != 0 {
            record.put_int("free_tickets", self.free_tickets);
        }
        record
    }
}

/// The POI of one region file (32x32 chunks), stored like vanilla's `SectionStorage`: one
/// chunk record per chunk with a `Sections` compound keyed by section Y.
#[derive(Debug, Default)]
pub struct PoiRegion {
    /// Entries indexed by position
    entries: FxHashMap<(i32, i32, i32), PoiEntry>,
    /// Each stored section by (chunk x, section y, chunk z), with its `Valid` flag. Sections
    /// without records stay, as vanilla keeps them.
    sections: FxHashMap<(i32, i32, i32), bool>,
    dirty_chunks: FxHashSet<(i32, i32)>,
}

impl PoiRegion {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    const fn pos_key(pos: &BlockPos) -> (i32, i32, i32) {
        (pos.0.x, pos.0.y, pos.0.z)
    }

    pub fn add(&mut self, entry: PoiEntry) {
        // A section made by an add starts valid, like `new PoiSection(setDirty)`.
        self.sections.entry(entry.section()).or_insert(true);
        self.dirty_chunks.insert(entry.chunk());
        self.entries.insert((entry.x, entry.y, entry.z), entry);
    }

    pub fn remove(&mut self, pos: &BlockPos) -> bool {
        let Some(entry) = self.entries.remove(&Self::pos_key(pos)) else {
            return false;
        };
        self.dirty_chunks.insert(entry.chunk());
        true
    }

    #[must_use]
    pub fn get_all(&self) -> Vec<&PoiEntry> {
        self.entries.values().collect()
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.dirty_chunks.is_empty()
    }

    pub fn mark_clean(&mut self) {
        self.dirty_chunks.clear();
    }

    /// The chunk record `SectionStorage.writeChunk` would write.
    fn chunk_nbt(&self, chunk_x: i32, chunk_z: i32) -> NbtCompound {
        let mut section_ys: Vec<i32> = self
            .sections
            .keys()
            .filter(|(x, _, z)| (*x, *z) == (chunk_x, chunk_z))
            .map(|(_, y, _)| *y)
            .collect();
        section_ys.sort_unstable();
        let mut sections = NbtCompound::new();
        for section_y in section_ys {
            let mut records: Vec<&PoiEntry> = self
                .entries
                .values()
                .filter(|entry| entry.section() == (chunk_x, section_y, chunk_z))
                .collect();
            records.sort_unstable_by_key(|entry| (entry.y, entry.z, entry.x));
            let mut section = NbtCompound::new();
            // `lenientOptionalFieldOf("Valid", false)` leaves out the default.
            if self.sections[&(chunk_x, section_y, chunk_z)] {
                section.put_bool("Valid", true);
            }
            section.put_list(
                "Records",
                records
                    .into_iter()
                    .map(|entry| NbtTag::Compound(entry.to_nbt()))
                    .collect(),
            );
            sections.put_compound(&section_y.to_string(), section);
        }
        let mut root = NbtCompound::new();
        root.put_compound("Sections", sections);
        root.put_int("DataVersion", WORLD_DATA_VERSION);
        root
    }

    fn read_chunk_nbt(&mut self, chunk_x: i32, chunk_z: i32, root: &NbtCompound) {
        let Some(sections) = root.get_compound("Sections") else {
            return;
        };
        for (key, tag) in &sections.child_tags {
            let (Ok(section_y), NbtTag::Compound(section)) = (key.parse::<i32>(), tag) else {
                continue;
            };
            self.sections.insert(
                (chunk_x, section_y, chunk_z),
                section.get_bool("Valid").unwrap_or(false),
            );
            for record in section.get_list("Records").unwrap_or_default() {
                let NbtTag::Compound(record) = record else {
                    continue;
                };
                let Some(entry) = PoiEntry::from_nbt(record) else {
                    warn!(
                        "Skipping a malformed POI record in chunk {chunk_x},{chunk_z} section {section_y}"
                    );
                    continue;
                };
                self.entries.insert((entry.x, entry.y, entry.z), entry);
            }
        }
    }

    /// Writes the changed chunks into the region file at `path` through the chunk region code,
    /// which handles every compression and `.mcc` files.
    pub fn save(
        &mut self,
        path: &Path,
        region: Vector2<i32>,
        config: &AnvilChunkConfig,
    ) -> std::io::Result<()> {
        if self.dirty_chunks.is_empty() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = AnvilRegion::open(path, region).map_err(std::io::Error::other)?;
        let mut chunks: Vec<_> = self.dirty_chunks.iter().copied().collect();
        chunks.sort_unstable();
        for (chunk_x, chunk_z) in chunks {
            let nbt =
                pumpkin_nbt::Nbt::new(String::new(), self.chunk_nbt(chunk_x, chunk_z)).write();
            let encoded = AnvilRegion::encode_chunk(nbt, config).map_err(std::io::Error::other)?;
            file.stage_chunk(region_chunk_index(Vector2::new(chunk_x, chunk_z)), encoded);
        }
        file.flush(true)?;
        self.dirty_chunks.clear();
        Ok(())
    }

    /// Reads the region file at `path`. A chunk that can't be read is logged and skipped.
    pub fn load(path: &Path, region: Vector2<i32>) -> std::io::Result<Self> {
        let mut poi = Self::new();
        if !path.exists() {
            return Ok(poi);
        }
        let file = AnvilRegion::open(path, region).map_err(std::io::Error::other)?;
        for index in 0..CHUNK_COUNT {
            let chunk = chunk_in_region(region, index);
            let nbt = match file.read_chunk(index) {
                Ok(None) => continue,
                Ok(Some(raw)) => AnvilRegion::decode_chunk(raw),
                Err(err) => Err(err),
            };
            let root = nbt.map_err(std::io::Error::other).and_then(|nbt| {
                let mut cursor = Cursor::new(nbt);
                pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                    pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
                ))
                .map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
                })
            });
            match root {
                Ok(root) => poi.read_chunk_nbt(chunk.x, chunk.y, &root.root_tag),
                Err(err) => warn!(
                    "Failed to read POI chunk {},{} from {}: {err}",
                    chunk.x,
                    chunk.y,
                    path.display()
                ),
            }
        }
        Ok(poi)
    }
}

/// Region-based POI storage using MCA format
pub struct PoiStorage {
    /// Path to the poi folder
    folder: PathBuf,
    /// Loaded regions, keyed by (`region_x`, `region_z`)
    regions: FxHashMap<(i32, i32), PoiRegion>,
    /// Compression for written chunks; reading takes any.
    config: AnvilChunkConfig,
}

impl PoiStorage {
    #[must_use]
    pub fn new(poi_folder: PathBuf) -> Self {
        Self {
            folder: poi_folder,
            regions: FxHashMap::default(),
            config: AnvilChunkConfig::default(),
        }
    }

    const fn region_coords(pos: &BlockPos) -> (i32, i32) {
        let chunk_x = pos.0.x >> 4;
        let chunk_z = pos.0.z >> 4;
        (chunk_x >> 5, chunk_z >> 5)
    }

    fn region_path(&self, rx: i32, rz: i32) -> PathBuf {
        self.folder.join(format!("r.{rx}.{rz}.mca"))
    }

    fn get_or_load_region(&mut self, rx: i32, rz: i32) -> &mut PoiRegion {
        let path = self.region_path(rx, rz);
        self.regions.entry((rx, rz)).or_insert_with(|| {
            PoiRegion::load(&path, Vector2::new(rx, rz)).unwrap_or_else(|e| {
                if path.exists() {
                    warn!("Failed to load POI region {}: {}", path.display(), e);
                }
                PoiRegion::new()
            })
        })
    }

    pub fn add(&mut self, pos: BlockPos, poi_type: &str) {
        self.add_with_free_tickets(pos, poi_type, 0);
    }

    pub fn add_with_free_tickets(&mut self, pos: BlockPos, poi_type: &str, free_tickets: i32) {
        let (rx, rz) = Self::region_coords(&pos);
        let region = self.get_or_load_region(rx, rz);
        region.add(PoiEntry {
            x: pos.0.x,
            y: pos.0.y,
            z: pos.0.z,
            poi_type: poi_type.to_string(),
            free_tickets,
        });
    }

    pub fn add_portal(&mut self, pos: BlockPos) {
        self.add(pos, POI_TYPE_NETHER_PORTAL);
    }

    pub fn remove(&mut self, pos: &BlockPos) -> bool {
        let (rx, rz) = Self::region_coords(pos);
        let region = self.get_or_load_region(rx, rz);
        region.remove(pos)
    }

    /// Get all POI positions within a square radius (for portal search)
    #[expect(clippy::similar_names)]
    pub fn get_in_square(
        &mut self,
        center: BlockPos,
        radius: i32,
        poi_type: Option<&str>,
    ) -> Vec<BlockPos> {
        let min_x = center.0.x - radius;
        let max_x = center.0.x + radius;
        let min_z = center.0.z - radius;
        let max_z = center.0.z + radius;

        // Calculate which regions we need to check
        let min_rx = (min_x >> 4) >> 5;
        let max_rx = (max_x >> 4) >> 5;
        let min_rz = (min_z >> 4) >> 5;
        let max_rz = (max_z >> 4) >> 5;

        let mut results = Vec::new();

        for rx in min_rx..=max_rx {
            for rz in min_rz..=max_rz {
                let region = self.get_or_load_region(rx, rz);
                for entry in region.get_all() {
                    if let Some(filter_type) = poi_type
                        && entry.poi_type != filter_type
                    {
                        continue;
                    }

                    let dx = (entry.x - center.0.x).abs();
                    let dz = (entry.z - center.0.z).abs();
                    if dx <= radius && dz <= radius {
                        results.push(entry.pos());
                    }
                }
            }
        }

        results
    }

    /// Finds the closest POI whose type matches `matches`, considering
    /// entries within `radius` blocks of `center` on the x/z axes (like
    /// vanilla's `PoiManager.findClosestWithType`: a chebyshev square gather
    /// followed by picking the smallest 3D squared distance).
    ///
    /// Returns the entry's position together with its type.
    pub fn find_closest_matching(
        &mut self,
        center: BlockPos,
        radius: i32,
        matches: impl Fn(&str) -> bool,
    ) -> Option<(BlockPos, String)> {
        let min_rx = ((center.0.x - radius) >> 4) >> 5;
        let max_rx = ((center.0.x + radius) >> 4) >> 5;
        let min_rz = ((center.0.z - radius) >> 4) >> 5;
        let max_rz = ((center.0.z + radius) >> 4) >> 5;

        let mut best: Option<(BlockPos, String, i64)> = None;

        for rx in min_rx..=max_rx {
            for rz in min_rz..=max_rz {
                let region = self.get_or_load_region(rx, rz);
                for entry in region.get_all() {
                    if (entry.x - center.0.x).abs() > radius
                        || (entry.z - center.0.z).abs() > radius
                        || !matches(&entry.poi_type)
                    {
                        continue;
                    }

                    let dx = i64::from(entry.x - center.0.x);
                    let dy = i64::from(entry.y - center.0.y);
                    let dz = i64::from(entry.z - center.0.z);
                    let distance_sq = dx * dx + dy * dy + dz * dz;

                    if best.as_ref().is_none_or(|(_, _, d)| distance_sq < *d) {
                        best = Some((entry.pos(), entry.poi_type.clone(), distance_sq));
                    }
                }
            }
        }

        best.map(|(pos, poi_type, _)| (pos, poi_type))
    }

    pub fn save_all(&mut self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.folder)?;

        let mut saved = 0;
        for ((rx, rz), region) in &mut self.regions {
            if region.is_dirty() {
                let path = self.folder.join(format!("r.{rx}.{rz}.mca"));
                region.save(&path, Vector2::new(*rx, *rz), &self.config)?;
                saved += 1;
            }
        }

        if saved > 0 {
            info!("Saved {saved} POI region(s)");
        }
        Ok(())
    }

    /// Get count of loaded regions
    #[must_use]
    pub fn loaded_region_count(&self) -> usize {
        self.regions.len()
    }

    /// Get total POI count across all loaded regions
    #[must_use]
    pub fn total_poi_count(&self) -> usize {
        self.regions.values().map(|r| r.get_all().len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poi_entry() {
        let entry = PoiEntry::new_portal(BlockPos(Vector3::new(100, 64, 200)));
        assert_eq!(entry.x, 100);
        assert_eq!(entry.y, 64);
        assert_eq!(entry.z, 200);
        assert_eq!(entry.poi_type, POI_TYPE_NETHER_PORTAL);
    }

    #[test]
    fn poi_region() {
        let mut region = PoiRegion::new();
        region.add(PoiEntry::new_portal(BlockPos(Vector3::new(100, 64, 200))));
        region.add(PoiEntry::new_portal(BlockPos(Vector3::new(101, 64, 200))));

        assert_eq!(region.get_all().len(), 2);
        assert!(region.is_dirty());

        region.remove(&BlockPos(Vector3::new(100, 64, 200)));
        assert_eq!(region.get_all().len(), 1);
    }

    #[test]
    fn poi_find_closest_matching() {
        let mut storage = PoiStorage::new(std::env::temp_dir().join("pumpkin_poi_closest_test"));

        storage.add_portal(BlockPos(Vector3::new(100, 64, 100)));
        storage.add_portal(BlockPos(Vector3::new(120, 64, 100)));
        storage.add(BlockPos(Vector3::new(101, 64, 100)), "minecraft:home");

        let center = BlockPos(Vector3::new(105, 64, 100));
        let (pos, poi_type) = storage
            .find_closest_matching(center, 256, |t| t == POI_TYPE_NETHER_PORTAL)
            .unwrap();
        assert_eq!(pos, BlockPos(Vector3::new(100, 64, 100)));
        assert_eq!(poi_type, POI_TYPE_NETHER_PORTAL);

        // The overall closest one ignores the type filter mismatch above.
        let (pos, poi_type) = storage
            .find_closest_matching(center, 256, |_| true)
            .unwrap();
        assert_eq!(pos, BlockPos(Vector3::new(101, 64, 100)));
        assert_eq!(poi_type, "minecraft:home");

        assert!(
            storage
                .find_closest_matching(center, 256, |t| t == "minecraft:lodestone")
                .is_none()
        );
        // Out of horizontal range.
        assert!(
            storage
                .find_closest_matching(BlockPos(Vector3::new(1000, 64, 100)), 16, |_| true)
                .is_none()
        );
    }

    fn decoded_chunk(path: &Path, region: Vector2<i32>, chunk: Vector2<i32>) -> NbtCompound {
        let file = AnvilRegion::open(path, region).unwrap();
        let raw = file.read_chunk(region_chunk_index(chunk)).unwrap().unwrap();
        let mut cursor = Cursor::new(AnvilRegion::decode_chunk(raw).unwrap());
        pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        ))
        .unwrap()
        .root_tag
    }

    /// `testdata/vanilla-26.3-r.-1.-1.mca` is the POI region vanilla 26.3 saved after
    /// `setblock` placed a lodestone at -5 70 -7, a bell at -20 -60 -3 and a nether portal at
    /// -40 80 -40 in a flat world.
    #[test]
    fn vanilla_poi_region_reads_and_writes_back_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r.-1.-1.mca");
        std::fs::write(&path, include_bytes!("testdata/vanilla-26.3-r.-1.-1.mca")).unwrap();
        let region = Vector2::new(-1, -1);
        let mut poi = PoiRegion::load(&path, region).unwrap();
        let mut records: Vec<_> = poi
            .get_all()
            .into_iter()
            .map(|entry| (entry.pos(), entry.poi_type.as_str(), entry.free_tickets))
            .collect();
        records.sort_unstable_by_key(|(pos, ..)| pos.0.x);
        assert_eq!(
            records,
            [
                (
                    BlockPos(Vector3::new(-40, 80, -40)),
                    "minecraft:nether_portal",
                    0
                ),
                (
                    BlockPos(Vector3::new(-20, -60, -3)),
                    "minecraft:meeting",
                    32
                ),
                (BlockPos(Vector3::new(-5, 70, -7)), "minecraft:lodestone", 0),
            ]
        );

        let lodestone_chunk = Vector2::new(-1, -1);
        let bell_chunk = Vector2::new(-2, -1);
        let vanilla_lodestone = decoded_chunk(&path, region, lodestone_chunk);
        let vanilla_bell = decoded_chunk(&path, region, bell_chunk);
        // Rewrite every chunk: what PEX writes must equal what vanilla wrote.
        poi.dirty_chunks.extend([(-1, -1), (-2, -1), (-3, -3)]);
        let config = AnvilChunkConfig::default();
        poi.save(&path, region, &config).unwrap();
        assert_eq!(
            decoded_chunk(&path, region, lodestone_chunk),
            vanilla_lodestone
        );
        assert_eq!(decoded_chunk(&path, region, bell_chunk), vanilla_bell);

        // A removed record leaves its section, still valid, like vanilla.
        poi.remove(&BlockPos(Vector3::new(-5, 70, -7)));
        poi.save(&path, region, &config).unwrap();
        let section = decoded_chunk(&path, region, lodestone_chunk)
            .get_compound("Sections")
            .and_then(|sections| sections.get_compound("4"))
            .cloned()
            .unwrap();
        assert_eq!(section.get_bool("Valid"), Some(true));
        assert_eq!(section.get_list("Records").map(<[NbtTag]>::len), Some(0));
        let reloaded = PoiRegion::load(&path, region).unwrap();
        assert_eq!(reloaded.get_all().len(), 2);
    }

    #[test]
    fn poi_storage_mca() {
        let dir = std::env::temp_dir().join("pumpkin_poi_mca_test");
        let _ = std::fs::remove_dir_all(&dir);

        let mut storage = PoiStorage::new(dir.join("poi"));

        storage.add_portal(BlockPos(Vector3::new(100, 64, 100)));
        storage.add_portal(BlockPos(Vector3::new(110, 64, 100)));
        storage.add_portal(BlockPos(Vector3::new(1000, 64, 1000))); // Different region

        let results = storage.get_in_square(
            BlockPos(Vector3::new(105, 64, 100)),
            16,
            Some(POI_TYPE_NETHER_PORTAL),
        );
        assert_eq!(results.len(), 2);

        storage.save_all().unwrap();

        // Verify .mca file was created
        let mca_path = dir.join("poi").join("r.0.0.mca");
        assert!(mca_path.exists());

        // Reload and verify
        let mut storage2 = PoiStorage::new(dir.join("poi"));
        let results2 = storage2.get_in_square(
            BlockPos(Vector3::new(105, 64, 100)),
            16,
            Some(POI_TYPE_NETHER_PORTAL),
        );
        assert_eq!(results2.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
