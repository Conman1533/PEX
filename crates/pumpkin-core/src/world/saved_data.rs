//! Per-dimension vanilla saved data: `world_border.dat` and the forced chunks in
//! `chunk_tickets.dat`.

use std::path::Path;

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::world_info::{
    WorldInfoError,
    data_files::{read_data_file, write_data_file},
};
use rustc_hash::FxHashSet;

use super::border::Worldborder;
use crate::entity::player::DATA_VERSION;

const WORLD_BORDER: &str = "world_border.dat";
const CHUNK_TICKETS: &str = "chunk_tickets.dat";
const FORCED: &str = "minecraft:forced";
/// Vanilla `ChunkMap.FORCED_TICKET_LEVEL`, the entity ticking level.
const FORCED_TICKET_LEVEL: i32 = 31;

#[must_use]
pub fn load_world_border(folder: &Path) -> Option<Worldborder> {
    read_data_file(folder, WORLD_BORDER).map(|data| Worldborder::from_nbt(&data))
}

pub fn save_world_border(folder: &Path, border: &Worldborder) -> Result<(), WorldInfoError> {
    write_data_file(folder, WORLD_BORDER, DATA_VERSION, border.to_nbt())
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
    fn no_file_is_written_without_forced_chunks() {
        let folder = temp_folder("none");
        save_forced_chunks(&folder, &FxHashSet::default()).unwrap();
        assert!(!folder.join("data/minecraft").join(CHUNK_TICKETS).exists());
    }
}
