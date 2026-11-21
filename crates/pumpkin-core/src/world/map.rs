use crate::entity::player::{DATA_VERSION, Player};
use dashmap::{DashMap, DashSet};
use pumpkin_data::dimension::Dimension;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use pumpkin_world::world_info::data_files::{read_data_file, write_data_file};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, warn};

/// Vanilla's `MapIndex`, `data/minecraft/maps/last_id.dat`.
const LAST_ID: &str = "maps/last_id.dat";
const MAP_SIZE: usize = 128 * 128;

/// The saved-data name of map `id`, vanilla's `MapId.key()`.
fn map_file(id: i32) -> String {
    format!("maps/{id}.dat")
}

/// The server's maps, loaded from and saved to `data/minecraft/maps/` like vanilla's saved data.
pub struct MapManager {
    pub maps: DashMap<i32, Arc<Mutex<MapData>>>,
    /// Ids looked up that have no file, so a held map without data doesn't read the disk each tick.
    missing: DashSet<i32>,
    folder: PathBuf,
    /// The last id handed out, vanilla's `MapIndex.lastMapId` (-1 before the first map).
    last_id: AtomicI32,
}

impl MapManager {
    /// Opens the maps of the world in `folder`. `legacy_next_id` is the next map id an older PEX
    /// kept in `level.dat`, used when the world has no `last_id.dat` yet.
    #[must_use]
    pub fn new(folder: PathBuf, legacy_next_id: i32) -> Self {
        let last_id = read_data_file(&folder, LAST_ID)
            .map_or(legacy_next_id - 1, |data| data.get_int("map").unwrap_or(-1));
        Self {
            maps: DashMap::new(),
            missing: DashSet::new(),
            folder,
            last_id: AtomicI32::new(last_id),
        }
    }

    /// Hands out a new map id, vanilla's `MapIndex.getNextMapId`.
    pub fn next_id(&self) -> i32 {
        self.last_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// The map with this id, read from its file the first time it is asked for.
    #[must_use]
    pub fn get_map(&self, id: i32) -> Option<Arc<Mutex<MapData>>> {
        if let Some(map) = self.maps.get(&id) {
            return Some(map.clone());
        }
        if self.missing.contains(&id) {
            return None;
        }
        let Some(map) = read_data_file(&self.folder, &map_file(id)).and_then(MapData::from_nbt)
        else {
            self.missing.insert(id);
            return None;
        };
        Some(
            self.maps
                .entry(id)
                .or_insert_with(|| Arc::new(Mutex::new(map)))
                .clone(),
        )
    }

    /// Writes every map changed since the last save, and `last_id.dat`.
    pub fn save(&self) {
        if let Err(err) = std::fs::create_dir_all(self.folder.join("data/minecraft/maps")) {
            error!("Failed to create the maps folder: {err}");
            return;
        }
        for entry in &self.maps {
            let mut map = entry
                .value()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !map.unsaved {
                continue;
            }
            match write_data_file(
                &self.folder,
                &map_file(*entry.key()),
                DATA_VERSION,
                map.to_nbt(),
            ) {
                Ok(()) => map.unsaved = false,
                Err(err) => error!("Failed to save map {}: {err}", entry.key()),
            }
        }
        let last_id = self.last_id.load(Ordering::SeqCst);
        let mut index = NbtCompound::new();
        // `optionalFieldOf("map", -1)` leaves out the default.
        if last_id != -1 {
            index.put_int("map", last_id);
        }
        if let Err(err) = write_data_file(&self.folder, LAST_ID, DATA_VERSION, index) {
            error!("Failed to save {LAST_ID}: {err}");
        }
    }

    #[must_use]
    pub fn create_map(
        &self,
        id: i32,
        dimension: Dimension,
        x: i32,
        z: i32,
        scale: i8,
    ) -> Arc<Mutex<MapData>> {
        let map = Arc::new(Mutex::new(MapData::new(dimension, x, z, scale)));
        self.maps.insert(id, map.clone());
        self.missing.remove(&id);
        map
    }
}

pub struct MapData {
    pub scale: i8,
    pub locked: bool,
    pub dimension: Dimension,
    pub center_x: i32,
    pub center_z: i32,
    pub colors: Box<[u8; 128 * 128]>,
    pub decorations: Vec<MapDecoration>,
    pub dirty: bool,
    pub fully_updated: bool,
    /// Changed since it was last written to disk.
    pub unsaved: bool,
    /// Saved keys PEX doesn't model (`trackingPosition`, `unlimitedTracking`, `banners`,
    /// `frames`), written back as read.
    pub extra: NbtCompound,
}

impl MapData {
    #[must_use]
    pub fn new(dimension: Dimension, x: i32, z: i32, scale: i8) -> Self {
        Self {
            scale,
            locked: false,
            dimension,
            center_x: x,
            center_z: z,
            colors: Box::new([0; 128 * 128]),
            decorations: Vec::new(),
            dirty: true,
            fully_updated: false,
            unsaved: true,
            extra: NbtCompound::new(),
        }
    }

    /// Reads vanilla's map saved data (`MapItemSavedData.CODEC`).
    #[must_use]
    pub fn from_nbt(mut data: NbtCompound) -> Option<Self> {
        let dimension_name = data.get_string("dimension")?;
        let Some(dimension) = Dimension::from_name(dimension_name) else {
            warn!("Skipping a map in unknown dimension {dimension_name}");
            return None;
        };
        let mut map = Self::new(
            dimension.clone(),
            data.get_int("xCenter")?,
            data.get_int("zCenter")?,
            data.get_byte("scale").unwrap_or(0),
        );
        // Vanilla keeps the blank map when `colors` has the wrong length.
        if let Some(colors) = data.get_byte_array("colors")
            && colors.len() == MAP_SIZE
        {
            for (to, from) in map.colors.iter_mut().zip(colors) {
                *to = *from as u8;
            }
        }
        map.locked = data.get_bool("locked").unwrap_or(false);
        map.unsaved = false;
        // The saved picture is the map so far: only the area around a holder is drawn again.
        map.fully_updated = true;
        for key in [
            "dimension",
            "xCenter",
            "zCenter",
            "scale",
            "colors",
            "locked",
        ] {
            data.child_tags.remove(key);
        }
        map.extra = data;
        Some(map)
    }

    /// Vanilla's map saved data, leaving out fields at their defaults like its codec.
    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut data = self.extra.clone();
        data.put_string("dimension", self.dimension.minecraft_name.to_string());
        data.put_int("xCenter", self.center_x);
        data.put_int("zCenter", self.center_z);
        if self.scale != 0 {
            data.put_byte("scale", self.scale);
        }
        let colors: Box<[i8]> = self.colors.iter().map(|&color| color as i8).collect();
        data.put("colors", NbtTag::ByteArray(colors));
        if self.locked {
            data.put_bool("locked", true);
        }
        data
    }

    pub fn set_color(&mut self, x: usize, z: usize, color: u8) {
        if x < 128 && z < 128 {
            let idx = z * 128 + x;
            if self.colors[idx] != color {
                self.colors[idx] = color;
                self.dirty = true;
                self.unsaved = true;
            }
        }
    }

    pub fn update(&mut self, player: &Player) {
        let world = player.world();
        let scale = 1 << self.scale;
        let center_x = self.center_x;
        let center_z = self.center_z;

        let player_pos = player.position();
        let player_x = player_pos.x as i32;
        let player_z = player_pos.z as i32;

        let start_img_x = ((player_x - center_x) / scale + 64).clamp(0, 127) as usize;
        let start_img_z = ((player_z - center_z) / scale + 64).clamp(0, 127) as usize;

        let radius = 16;
        let (range_x, range_z) = if self.fully_updated {
            (
                (start_img_x.saturating_sub(radius))..(start_img_x + radius).min(128),
                (start_img_z.saturating_sub(radius))..(start_img_z + radius).min(128),
            )
        } else {
            self.fully_updated = true;
            (0..128, 0..128)
        };

        for img_x in range_x {
            let mut prev_y = -1;
            for img_z in range_z.clone() {
                let world_x = (img_x as i32 - 64) * scale + center_x;
                let world_z = (img_z as i32 - 64) * scale + center_z;

                let top_y = world.get_top_block(Vector2::new(world_x, world_z));
                let block = world.get_block(&BlockPos::new(world_x, top_y, world_z));

                let color_base = block.map_color;

                let mut brightness = 2; // Normal
                if prev_y != -1 {
                    if top_y > prev_y {
                        brightness = 3; // High
                    } else if top_y < prev_y {
                        brightness = 1; // Low
                    }
                }
                prev_y = top_y;

                let color = color_base * 4 + brightness;
                self.set_color(img_x, img_z, color);
            }
        }
    }
}

pub struct MapDecoration {
    pub icon_type: i32,
    pub x: i8,
    pub z: i8,
    pub direction: i8,
    pub display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA_MAP: &[u8] = include_bytes!("testdata/map_vanilla_26_3.dat");

    /// `testdata/map_vanilla_26_3.dat` is a buried treasure map vanilla 26.3 made from
    /// `/loot spawn .. loot minecraft:chests/shipwreck_map`.
    #[test]
    fn a_vanilla_map_loads_its_picture_and_saves_unchanged() {
        let root =
            pumpkin_nbt::nbt_compress::read_gzip_compound_tag(std::io::Cursor::new(VANILLA_MAP))
                .unwrap();
        let data = root.get_compound("data").unwrap().clone();
        let map = MapData::from_nbt(data.clone()).unwrap();
        assert_eq!((map.center_x, map.center_z, map.scale), (576, -448, 1));
        assert_eq!(map.colors.iter().filter(|&&color| color != 0).count(), 3155);
        assert!(!map.unsaved);
        assert!(map.fully_updated && map.dirty);
        assert_eq!(map.to_nbt(), data);
    }

    /// Vanilla saved `last_id.dat` as `{map: 1}` after making maps 0 and 1.
    #[test]
    fn new_ids_follow_last_id_and_are_saved_back() {
        let world = tempfile::tempdir().unwrap();
        let maps = world.path().join("data/minecraft/maps");
        std::fs::create_dir_all(&maps).unwrap();
        std::fs::write(maps.join("0.dat"), VANILLA_MAP).unwrap();
        let mut index = NbtCompound::new();
        index.put_int("map", 1);
        write_data_file(world.path(), LAST_ID, DATA_VERSION, index).unwrap();

        let manager = MapManager::new(world.path().to_path_buf(), 0);
        assert!(manager.get_map(0).is_some());
        assert!(manager.get_map(5).is_none());
        let id = manager.next_id();
        assert_eq!(id, 2);
        let _ = manager.create_map(id, Dimension::OVERWORLD.clone(), 0, 0, 0);
        manager.save();

        let reopened = MapManager::new(world.path().to_path_buf(), 0);
        assert_eq!(reopened.next_id(), 3);
        assert!(reopened.get_map(2).is_some());
        assert_eq!(
            read_data_file(world.path(), LAST_ID)
                .unwrap()
                .get_int("map"),
            Some(2)
        );
    }

    #[test]
    fn a_world_without_last_id_continues_from_the_old_level_dat_counter() {
        let world = tempfile::tempdir().unwrap();
        assert_eq!(MapManager::new(world.path().to_path_buf(), 4).next_id(), 4);
        assert_eq!(MapManager::new(world.path().to_path_buf(), 0).next_id(), 0);
    }
}
