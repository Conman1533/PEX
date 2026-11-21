//! Per-dimension saved data: vanilla's `world_border.dat`, the forced chunks in
//! `chunk_tickets.dat` and `raids.dat`, and plugin data in `pumpkin_custom_data.nbt`.

use std::path::Path;

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::level::LevelFolder;
use pumpkin_world::world_info::{
    LevelData, WorldInfoError,
    data_files::{read_data_file, write_data_file},
};
use rustc_hash::FxHashSet;

use super::border::Worldborder;
use super::raid::Raids;
use crate::entity::player::DATA_VERSION;

const WORLD_BORDER: &str = "world_border.dat";
const CHUNK_TICKETS: &str = "chunk_tickets.dat";
const RAIDS: &str = "raids.dat";
const FORCED: &str = "minecraft:forced";
const CUSTOM_DATA: &str = "pumpkin_custom_data.nbt";
/// Vanilla `ChunkMap.FORCED_TICKET_LEVEL`, the entity ticking level.
const FORCED_TICKET_LEVEL: i32 = 31;

/// A dimension's plugin data (roadmap G7-29), from `pumpkin_custom_data.nbt` in its folder.
///
/// Before G7-29 every dimension read and wrote one file in the root folder, so the overworld
/// falls back to that file until it has its own. The file is a named NBT document, as
/// [`save_custom_data`] and alpha wrote it (alpha read it back unnamed and so always got nothing).
#[must_use]
pub fn load_custom_data(folders: &LevelFolder, overworld: bool) -> NbtCompound {
    let own = folders.dim_folder.join(CUSTOM_DATA);
    let path = if overworld && !own.exists() {
        folders.root_folder.join(CUSTOM_DATA)
    } else {
        own
    };
    std::fs::read(path)
        .ok()
        .and_then(|bytes| {
            pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                &mut std::io::Cursor::new(bytes),
            ))
            .ok()
        })
        .map_or_else(NbtCompound::new, |nbt| nbt.root_tag)
}

/// Writes a dimension's plugin data to its own folder.
///
/// Empty data still replaces a file that [`load_custom_data`] would read, so keys a plugin
/// removed don't come back on the next start.
pub fn save_custom_data(
    folders: &LevelFolder,
    overworld: bool,
    data: &NbtCompound,
) -> std::io::Result<()> {
    let own = folders.dim_folder.join(CUSTOM_DATA);
    if data.is_empty()
        && !own.exists()
        && !(overworld && folders.root_folder.join(CUSTOM_DATA).exists())
    {
        return Ok(());
    }
    std::fs::create_dir_all(&folders.dim_folder)?;
    let temp = folders.dim_folder.join(format!("{CUSTOM_DATA}.tmp"));
    std::fs::write(&temp, pumpkin_nbt::Nbt::from(data.clone()).write())?;
    std::fs::rename(temp, own)
}

#[must_use]
pub fn load_raids(folder: &Path) -> Raids {
    read_data_file(folder, RAIDS).map_or_else(Raids::default, |data| Raids::from_nbt(&data))
}

pub fn save_raids(folder: &Path, raids: &Raids) -> Result<(), WorldInfoError> {
    write_data_file(folder, RAIDS, DATA_VERSION, raids.to_nbt())
}

pub fn load_world_border(folder: &Path) -> Option<Worldborder> {
    read_data_file(folder, WORLD_BORDER).map(|data| Worldborder::from_nbt(&data))
}

pub fn save_world_border(folder: &Path, border: &Worldborder) -> Result<(), WorldInfoError> {
    write_data_file(folder, WORLD_BORDER, DATA_VERSION, border.to_nbt())
}

/// The `world_border.dat` data of a world whose `level.dat` still has the `Border*` keys (a world
/// from before 26.1). It is written at once, as the next `level.dat` save drops those keys.
#[must_use]
#[expect(clippy::cast_possible_truncation)]
pub fn migrate_legacy_world_border(folder: &Path, info: &LevelData) -> Option<Worldborder> {
    if !info.legacy_border {
        return None;
    }
    let mut data = NbtCompound::new();
    data.put_double("center_x", info.border_center_x);
    data.put_double("center_z", info.border_center_z);
    data.put_double("size", info.border_size);
    data.put_double("damage_per_block", info.border_damage_per_block);
    data.put_double("safe_zone", info.border_safe_zone);
    data.put_int("warning_blocks", info.border_warning_blocks as i32);
    data.put_int("warning_time", info.border_warning_time as i32);
    data.put_double("lerp_target", info.border_size_lerp_target);
    data.put_long("lerp_time", info.border_size_lerp_time);
    let border = Worldborder::from_nbt(&data);
    if let Err(e) = save_world_border(folder, &border) {
        tracing::warn!("Failed to save the world border taken from level.dat: {e}");
    }
    Some(border)
}

fn tickets(data: &NbtCompound) -> impl Iterator<Item = &NbtCompound> {
    data.get_list("tickets")
        .unwrap_or_default()
        .iter()
        .filter_map(|tag| match tag {
            NbtTag::Compound(ticket) => Some(ticket),
            _ => None,
        })
}

fn forced_pos(ticket: &NbtCompound) -> Option<Vector2<i32>> {
    if ticket.get_string("type")? != FORCED {
        return None;
    }
    match ticket.get_int_array("chunk_pos")? {
        [x, z] => Some(Vector2::new(*x, *z)),
        _ => None,
    }
}

/// The chunks `/forceload` added, from the forced tickets in `chunk_tickets.dat`.
#[must_use]
pub fn load_forced_chunks(folder: &Path) -> FxHashSet<Vector2<i32>> {
    read_data_file(folder, CHUNK_TICKETS)
        .map(|data| tickets(&data).filter_map(forced_pos).collect())
        .unwrap_or_default()
}

/// Rewrites the forced tickets in `chunk_tickets.dat`. The other persisting tickets vanilla
/// wrote there are kept as they were.
pub fn save_forced_chunks(
    folder: &Path,
    forced: &FxHashSet<Vector2<i32>>,
) -> Result<(), WorldInfoError> {
    let existing = read_data_file(folder, CHUNK_TICKETS);
    if existing.is_none() && forced.is_empty() {
        return Ok(());
    }
    let mut kept: Vec<NbtTag> = existing
        .as_ref()
        .map(|data| {
            tickets(data)
                .filter(|ticket| ticket.get_string("type") != Some(FORCED))
                .cloned()
                .map(NbtTag::Compound)
                .collect()
        })
        .unwrap_or_default();
    let mut positions: Vec<_> = forced.iter().copied().collect();
    positions.sort_unstable_by_key(|pos| (pos.x, pos.y));
    for pos in positions {
        let mut ticket = NbtCompound::new();
        ticket.put("chunk_pos", NbtTag::IntArray(vec![pos.x, pos.y]));
        ticket.put_string("type", FORCED.to_string());
        ticket.put_int("level", FORCED_TICKET_LEVEL);
        kept.push(NbtTag::Compound(ticket));
    }
    let mut data = existing.unwrap_or_default();
    data.put_list("tickets", kept);
    write_data_file(folder, CHUNK_TICKETS, DATA_VERSION, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders(root: &Path, dimension: &str) -> LevelFolder {
        let dim_folder = root.join("dimensions").join("minecraft").join(dimension);
        LevelFolder {
            root_folder: root.to_path_buf(),
            region_folder: dim_folder.join("region"),
            entities_folder: dim_folder.join("entities"),
            poi_folder: dim_folder.join("poi"),
            dim_folder,
        }
    }

    fn plugin_data(value: &str) -> NbtCompound {
        let mut data = NbtCompound::new();
        let mut plugin = NbtCompound::new();
        plugin.put_string("home", value.to_string());
        data.put_compound("myplugin", plugin);
        data
    }

    /// G7-29: the overworld and the Nether keep their own plugin data through a restart
    /// (alpha wrote both to one root file, so the last world to save won).
    #[test]
    fn each_dimension_keeps_its_own_plugin_data() {
        let root = temp_folder("custom-data");
        let overworld = folders(&root, "overworld");
        let nether = folders(&root, "the_nether");
        save_custom_data(&overworld, true, &plugin_data("overworld")).unwrap();
        save_custom_data(&nether, false, &plugin_data("nether")).unwrap();

        assert_eq!(load_custom_data(&overworld, true), plugin_data("overworld"));
        assert_eq!(load_custom_data(&nether, false), plugin_data("nether"));
        assert!(!root.join(CUSTOM_DATA).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// G7-29: the shared root file from before is the overworld's until it saves its own, and
    /// emptying it then doesn't bring the old file back.
    #[test]
    fn the_overworld_takes_over_the_old_root_file() {
        let root = temp_folder("custom-data-root");
        let overworld = folders(&root, "overworld");
        let nether = folders(&root, "the_nether");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(CUSTOM_DATA),
            pumpkin_nbt::Nbt::from(plugin_data("old")).write(),
        )
        .unwrap();

        assert_eq!(load_custom_data(&overworld, true), plugin_data("old"));
        assert!(load_custom_data(&nether, false).is_empty());

        save_custom_data(&overworld, true, &NbtCompound::new()).unwrap();
        assert!(load_custom_data(&overworld, true).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pex-saved-data-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn forced_chunks_round_trip_and_keep_other_tickets() {
        let folder = temp_folder("tickets");
        let mut pearl = NbtCompound::new();
        pearl.put("chunk_pos", NbtTag::IntArray(vec![9, 9]));
        pearl.put_string("type", "minecraft:ender_pearl".to_string());
        pearl.put_int("level", 31);
        pearl.put_long("ticks_left", 40);
        let mut data = NbtCompound::new();
        data.put_list("tickets", vec![NbtTag::Compound(pearl)]);
        write_data_file(&folder, CHUNK_TICKETS, DATA_VERSION, data).unwrap();

        let forced: FxHashSet<_> = [Vector2::new(1, -2), Vector2::new(-30, 4)]
            .into_iter()
            .collect();
        save_forced_chunks(&folder, &forced).unwrap();

        assert_eq!(load_forced_chunks(&folder), forced);
        let saved = read_data_file(&folder, CHUNK_TICKETS).unwrap();
        let types: Vec<_> = tickets(&saved)
            .filter_map(|t| t.get_string("type"))
            .collect();
        assert_eq!(types, ["minecraft:ender_pearl", FORCED, FORCED]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_border_in_an_old_level_dat_becomes_world_border_dat() {
        let folder = temp_folder("legacy-border");
        let mut info = LevelData::default(pumpkin_util::world_seed::Seed(1));
        assert!(migrate_legacy_world_border(&folder, &info).is_none());

        info.legacy_border = true;
        info.border_size = 1024.0;
        info.border_center_x = 8.0;
        info.border_damage_per_block = 0.5;
        let border = migrate_legacy_world_border(&folder, &info).unwrap();
        assert_eq!((border.center_x, border.new_diameter), (8.0, 1024.0));
        let saved = load_world_border(&folder).expect("world_border.dat written");
        assert_eq!(saved.center_x, 8.0);
        assert_eq!(saved.new_diameter, 1024.0);
        assert_eq!(saved.damage_per_block, 0.5);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn no_file_is_written_without_forced_chunks() {
        let folder = temp_folder("none");
        save_forced_chunks(&folder, &FxHashSet::default()).unwrap();
        assert!(!folder.join("data/minecraft").join(CHUNK_TICKETS).exists());
    }
}
