//! Moves references to migrated players inside the world to their new UUIDs: owners, throwers,
//! anger targets, gossip, trial spawner and vault lists, and anything else that names a player.
//!
//! Vanilla 26.3 writes every UUID in world NBT with `UUIDUtil.CODEC`, an int array of four
//! (`EntityReference.store`, `TamableAnimal`, `NeutralMob`, `GossipContainer`,
//! `TrialSpawnerStateData`, `VaultServerData`, `ResolvableProfile` and the rest), and so does PEX.
//! Vanilla still reads a tamed animal's or horse's `Owner` as a `UUID.toString()` string
//! (`EntityReference.readWithOldOwnerConversion`), so those strings are changed too. Values are
//! only changed when they are the old UUID of a player in the mapping file.
//!
//! [`RECORD_FILE`] lists the players whose references are moved, so players added to the mapping
//! later, such as those [`migrate_offline_player`](super::migrate_offline_player) moves at login,
//! are moved at the next start.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use pumpkin_config::chunk::ChunkConfig;
use pumpkin_config::migration::write_backup;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_world::chunk::io::converter::{ChunkConversionError, dimension_folders, region_files};
use pumpkin_world::chunk::io::editor::edit_region;
use pumpkin_world::chunk::io::write_file_atomically;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{info, warn};
use uuid::Uuid;
use uuid::fmt::Hyphenated;

use super::{
    LEGACY_VERSION, MAPPING_FILE, MappedPlayer, MigrationError, PLAYERS_DIR, at, read_mapping,
    write_mapping,
};

/// The folders in a dimension with region files that can name players. `poi` can't.
const REGION_FOLDERS: [&str; 2] = ["region", "entities"];
/// The folder in the world, and in each dimension, with saved data (vanilla `LevelResource.DATA`).
const DATA_DIR: &str = "data";
/// Player data is `<uuid>.dat`, and vanilla `PlayerDataStorage.save` keeps the previous one as
/// `<uuid>.dat_old`, which `PlayerDataStorage.load` falls back to.
const PLAYER_DATA_EXTENSIONS: [&str; 2] = ["dat", "dat_old"];
/// Saved data is `<name>.dat` (vanilla `SavedDataStorage.getDataFile`).
const SAVED_DATA_EXTENSIONS: [&str; 1] = ["dat"];
const PROGRESS_INTERVAL: Duration = Duration::from_secs(10);

/// The file next to [`MAPPING_FILE`] that lists, in the same format, the players whose references
/// in the world are moved.
const RECORD_FILE: &str = "uuid-migration-world.json";

/// Changes the players in the world's [`MAPPING_FILE`] from their old to their new UUIDs wherever
/// the world names them, and adds them to [`RECORD_FILE`].
///
/// That covers entities and block entities in every dimension's region files, whatever their
/// format, player data and saved data. Whatever it changes is copied to `<name>.v<version>.bak`
/// first. Only the old UUIDs change, so a second run changes nothing.
pub fn migrate_world_uuids(
    server_dir: &Path,
    world_dir: &Path,
    chunk_config: &ChunkConfig,
    version: u32,
) -> Result<(), MigrationError> {
    let world_dir = server_dir.join(world_dir);
    let players = read_mapping(&world_dir.join(PLAYERS_DIR).join(MAPPING_FILE))?
        .map(|mapping| mapping.players)
        .unwrap_or_default();
    move_references(&world_dir, players, chunk_config, version)
}

/// Does what [`migrate_world_uuids`] does for the players in [`MAPPING_FILE`] that [`RECORD_FILE`]
/// doesn't list yet. Without any, it reads no region file.
///
/// Backups are named after the `config_version` of files with PEX 1.x UUIDs, like those of
/// [`migrate_offline_player`](super::migrate_offline_player).
pub fn migrate_world_uuids_of_new_players(
    server_dir: &Path,
    world_dir: &Path,
    chunk_config: &ChunkConfig,
) -> Result<(), MigrationError> {
    let world_dir = server_dir.join(world_dir);
    let players_dir = world_dir.join(PLAYERS_DIR);
    let Some(mapping) = read_mapping(&players_dir.join(MAPPING_FILE))? else {
        return Ok(());
    };
    let recorded: FxHashSet<Uuid> = read_mapping(&players_dir.join(RECORD_FILE))?
        .iter()
        .flat_map(|record| &record.players)
        .map(|player| player.old_uuid)
        .collect();
    let new_players = mapping
        .players
        .into_iter()
        .filter(|player| !recorded.contains(&player.old_uuid))
        .collect();
    move_references(&world_dir, new_players, chunk_config, LEGACY_VERSION)
}

/// Moves the references to `players`, then adds them to [`RECORD_FILE`]. A failure leaves the
/// record as it was, so the next start goes through the world for them again.
fn move_references(
    world_dir: &Path,
    players: Vec<MappedPlayer>,
    chunk_config: &ChunkConfig,
    version: u32,
) -> Result<(), MigrationError> {
    let uuids: FxHashMap<Uuid, Uuid> = players
        .iter()
        .map(|player| (player.old_uuid, player.new_uuid))
        .collect();
    if uuids.is_empty() {
        return Ok(());
    }
    let players_dir = world_dir.join(PLAYERS_DIR);

    let start = Instant::now();
    let dimensions = dimension_folders(world_dir);
    let mut regions = Vec::new();
    for dimension in &dimensions {
        for folder in REGION_FOLDERS {
            let folder = dimension.join(folder);
            regions.extend(region_files(&folder).map_err(at(&folder))?);
        }
    }
    info!(
        "Moving references to {} migrated player(s) in {} region file(s), player data and saved data to their new UUIDs",
        uuids.len(),
        regions.len()
    );

    let replaced = AtomicUsize::new(0);
    let edit = |nbt: &mut NbtCompound| {
        let count = replace_in_compound(nbt, &uuids);
        replaced.fetch_add(count, Ordering::Relaxed);
        count > 0
    };

    let mut files_changed = 0;
    let mut nbt_files = list_nbt_files(&players_dir.join("data"), &PLAYER_DATA_EXTENSIONS)?;
    for dimension in &dimensions {
        nbt_files.extend(list_nbt_files(
            &dimension.join(DATA_DIR),
            &SAVED_DATA_EXTENSIONS,
        )?);
    }
    for path in nbt_files {
        files_changed += usize::from(edit_nbt_file(&path, edit, version)?);
    }

    let chunks = AtomicUsize::new(0);
    let chunks_changed = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let last_progress = Mutex::new(Instant::now());
    regions
        .par_iter()
        .try_for_each(|(path, region, format)| -> Result<(), MigrationError> {
            let result = edit_region(path, *region, *format, chunk_config, edit, |files| {
                backup_files(files, version)
            })
            .map_err(|err| match err {
                ChunkConversionError::Io(err) => at(path)(err),
                err => at(path)(io::Error::other(err)),
            })?;
            chunks.fetch_add(result.chunks, Ordering::Relaxed);
            chunks_changed.fetch_add(result.changed, Ordering::Relaxed);
            let done = done.fetch_add(1, Ordering::Relaxed) + 1;
            let mut last_progress = last_progress.lock().unwrap_or_else(PoisonError::into_inner);
            if last_progress.elapsed() >= PROGRESS_INTERVAL {
                *last_progress = Instant::now();
                drop(last_progress);
                info!("Went through {done} of {} region files", regions.len());
            }
            Ok(())
        })?;

    let (replaced, chunks, seconds) = (
        replaced.into_inner(),
        chunks.into_inner(),
        start.elapsed().as_secs_f64(),
    );
    if replaced == 0 {
        info!(
            "Found no references to migrated players in {chunks} chunks, player data and saved data ({seconds:.1} s)"
        );
    } else {
        info!(
            "Changed {replaced} reference(s) to migrated players in {} chunk(s) and {files_changed} other file(s), after going through {chunks} chunks in {seconds:.1} s. Each changed file was copied to <name>.v{version}.bak first",
            chunks_changed.into_inner(),
        );
    }
    write_mapping(
        &players_dir.join(RECORD_FILE),
        players.into_iter(),
        &BTreeSet::new(),
    )
}

/// Replaces every old UUID in `compound` with its new one, and returns how many it replaced.
fn replace_in_compound(compound: &mut NbtCompound, uuids: &FxHashMap<Uuid, Uuid>) -> usize {
    compound
        .child_tags
        .values_mut()
        .map(|tag| replace_in_tag(tag, uuids))
        .sum()
}

fn replace_in_tag(tag: &mut NbtTag, uuids: &FxHashMap<Uuid, Uuid>) -> usize {
    match tag {
        NbtTag::IntArray(_) => {
            let Some(&new) = tag.extract_uuid().and_then(|old| uuids.get(&old)) else {
                return 0;
            };
            *tag = NbtTag::from_uuid(new);
            1
        }
        NbtTag::String(string) => {
            let Some(&new) = (string.len() == Hyphenated::LENGTH)
                .then(|| Uuid::try_parse(string).ok())
                .flatten()
                .and_then(|old| uuids.get(&old))
            else {
                return 0;
            };
            *string = new.hyphenated().to_string().into();
            1
        }
        NbtTag::List(list) => list.iter_mut().map(|tag| replace_in_tag(tag, uuids)).sum(),
        NbtTag::Compound(compound) => replace_in_compound(compound, uuids),
        _ => 0,
    }
}

/// The files with one of `extensions` in `dir` and the folders in it. A missing folder has none.
fn list_nbt_files(dir: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>, MigrationError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(at(dir)(err)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(at(dir))?.path();
        if path.is_dir() {
            files.extend(list_nbt_files(&path, extensions)?);
        } else if path
            .extension()
            .is_some_and(|extension| extensions.iter().any(|wanted| extension == *wanted))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Runs `edit` on a gzipped NBT file and writes it back if it changed. Returns whether it did.
fn edit_nbt_file(
    path: &Path,
    edit: impl Fn(&mut NbtCompound) -> bool,
    version: u32,
) -> Result<bool, MigrationError> {
    let bytes = fs::read(path).map_err(at(path))?;
    let Ok(mut nbt) = read_gzip_compound_tag(Cursor::new(&bytes)) else {
        warn!(
            "Couldn't read {} as NBT, so references to migrated players in it keep their old UUIDs",
            path.display()
        );
        return Ok(false);
    };
    if !edit(&mut nbt) {
        return Ok(false);
    }
    write_backup(path, &bytes, version).map_err(at(path))?;
    let bytes =
        write_gzip_compound_tag_to_bytes(nbt).map_err(|err| at(path)(io::Error::other(err)))?;
    write_file_atomically(path, &bytes, true).map_err(at(path))?;
    Ok(true)
}

fn backup_files(files: &[PathBuf], version: u32) -> io::Result<()> {
    for file in files {
        match fs::read(file) {
            Ok(contents) => {
                write_backup(file, contents, version)?;
            }
            // An Anvil stub can point at an external chunk file that is gone.
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};

    use bytes::Bytes;
    use pumpkin_config::chunk::{AnvilChunkConfig, ChunkConfig};
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::deserializer::NbtReadHelperJava;
    use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
    use pumpkin_nbt::tag::NbtTag;
    use pumpkin_nbt::{Nbt, nbt_compound_tag};
    use pumpkin_util::math::vector2::Vector2;
    use pumpkin_world::chunk::format::anvil::AnvilRegion;
    use pumpkin_world::chunk::format::linear::LinearV2File;
    use pumpkin_world::chunk::format::pump::PumpFile;
    use pumpkin_world::chunk::io::RegionFile;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::{RECORD_FILE, migrate_world_uuids, migrate_world_uuids_of_new_players};
    use crate::data::offline_uuid_migration::migrate_offline_player;

    // Notch's PEX 1.2.0 UUID and the one a vanilla 26.3 server gives the name (from 4.2's tests).
    const NOTCH_OLD: &str = "ff4af744-2839-cc96-4605-0dd2214a30c1";
    const NOTCH_NEW: &str = "b50ad385-829d-3141-a216-7e7d7539ba7f";
    /// Ghost's PEX 1.2.0 UUID, which no name matched.
    const GHOST: &str = "df1bc498-4a05-5e10-b286-3eb03abf1170";
    /// The UUID a vanilla 26.3 server gave Ghost at login (from 4.7's tests).
    const GHOST_NEW: &str = "48f83307-567a-33d7-8604-3b22ee4c1b84";
    /// Notch's Mojang UUID.
    const MOJANG: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
    const WOLF: &str = "3c6c1d4e-8f6a-4d7b-9b1e-2a4f6c8e0d12";

    fn uuid(uuid: &str) -> NbtTag {
        NbtTag::from_uuid(uuid.parse().unwrap())
    }

    /// The compound `nbt_compound_tag!` built.
    fn compound(tag: NbtTag) -> NbtCompound {
        let NbtTag::Compound(compound) = tag else {
            return NbtCompound::new();
        };
        compound
    }

    /// The forms vanilla 26.3 writes, each naming Notch by `notch`, and UUIDs that must stay.
    fn entities(notch: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "DataVersion": NbtTag::Int(4790),
            "Position": NbtTag::IntArray(vec![0, 0]),
            "Entities": NbtTag::List(vec![
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:wolf"),
                    "UUID": uuid(WOLF),
                    "Owner": uuid(notch),
                    "angry_at": uuid(MOJANG),
                    "Passengers": NbtTag::List(vec![nbt_compound_tag! {
                        "id": NbtTag::from("minecraft:item"),
                        "Thrower": uuid(notch),
                        "Owner": uuid(notch),
                        "Item": nbt_compound_tag! {
                            "id": NbtTag::from("minecraft:player_head"),
                            "components": nbt_compound_tag! {
                                "minecraft:profile": nbt_compound_tag! {
                                    "id": uuid(notch),
                                    "name": NbtTag::from("Notch")
                                }
                            }
                        }
                    }])
                },
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:villager"),
                    "Gossips": NbtTag::List(vec![
                        nbt_compound_tag! {
                            "Target": uuid(notch),
                            "Type": NbtTag::from("minor_positive"),
                            "Value": NbtTag::Int(5)
                        },
                        nbt_compound_tag! {
                            "Target": uuid(GHOST),
                            "Type": NbtTag::from("minor_negative"),
                            "Value": NbtTag::Int(3)
                        }
                    ])
                },
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:zombified_piglin"),
                    "angry_at": uuid(notch),
                    "Brain": nbt_compound_tag! {
                        "memories": nbt_compound_tag! {
                            "minecraft:angry_at": nbt_compound_tag! { "value": uuid(notch) }
                        }
                    }
                },
                // Vanilla still reads a horse's or tamed animal's Owner as a UUID string.
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:horse"),
                    "Owner": NbtTag::from(notch),
                    "CustomName": NbtTag::String(NOTCH_OLD.replace('-', "").into())
                }
            ])
        })
    }

    fn chunk_with_block_entities(notch: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "xPos": NbtTag::Int(0),
            "zPos": NbtTag::Int(0),
            "Status": NbtTag::from("minecraft:full"),
            "block_entities": NbtTag::List(vec![
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:trial_spawner"),
                    "registered_players": NbtTag::List(vec![uuid(notch), uuid(MOJANG)]),
                    "current_mobs": NbtTag::List(vec![uuid(WOLF)])
                },
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:vault"),
                    "server_data": nbt_compound_tag! {
                        "rewarded_players": NbtTag::List(vec![uuid(notch)])
                    },
                    "shared_data": nbt_compound_tag! {
                        "connected_players": NbtTag::List(vec![uuid(notch)])
                    }
                }
            ])
        })
    }

    fn untouched_chunk() -> NbtCompound {
        compound(nbt_compound_tag! {
            "Entities": NbtTag::List(vec![nbt_compound_tag! {
                "id": NbtTag::from("minecraft:arrow"),
                "Owner": uuid(MOJANG)
            }])
        })
    }

    fn write_region<R: RegionFile>(path: &Path, config: &R::Config, chunks: &[NbtCompound]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = R::open(path, Vector2::new(0, 0)).unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            let bytes = Nbt::from(chunk.clone()).write();
            file.stage_chunk(index, R::encode_chunk(bytes, config).unwrap());
        }
        file.flush(true).unwrap();
    }

    fn read_region<R: RegionFile>(path: &Path) -> Vec<NbtCompound> {
        let file = R::open(path, Vector2::new(0, 0)).unwrap();
        (0..)
            .map_while(|index| file.read_chunk(index).unwrap())
            .map(|raw| {
                let bytes: Bytes = R::decode_chunk(raw).unwrap();
                let mut cursor = Cursor::new(&bytes[..]);
                Nbt::read(&mut NbtReadHelperJava::new(&mut cursor))
                    .unwrap()
                    .root_tag
            })
            .collect()
    }

    fn write_gzip(path: &Path, nbt: &NbtCompound) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, write_gzip_compound_tag_to_bytes(nbt.clone()).unwrap()).unwrap();
    }

    fn read_gzip(path: &Path) -> NbtCompound {
        read_gzip_compound_tag(Cursor::new(fs::read(path).unwrap())).unwrap()
    }

    fn player_data(notch: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "UUID": uuid(NOTCH_NEW),
            "ShoulderEntityLeft": nbt_compound_tag! {
                "id": NbtTag::from("minecraft:parrot"),
                "Owner": uuid(notch)
            },
            "ender_pearls": NbtTag::List(vec![nbt_compound_tag! {
                "id": NbtTag::from("minecraft:ender_pearl"),
                "Owner": uuid(notch)
            }])
        })
    }

    fn boss_events(notch: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "data": nbt_compound_tag! {
                "minecraft:quest": nbt_compound_tag! { "Players": NbtTag::List(vec![uuid(notch)]) }
            }
        })
    }

    fn raids_data(notch: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "data": nbt_compound_tag! { "raids": NbtTag::List(vec![nbt_compound_tag! {
                "heroes_of_the_village": NbtTag::List(vec![uuid(notch), uuid(GHOST)])
            }]) }
        })
    }

    fn read_record(world: &Path) -> Value {
        serde_json::from_slice(&fs::read(world.join("players").join(RECORD_FILE)).unwrap()).unwrap()
    }

    fn record_of(players: &[(&str, &str, &str)]) -> Value {
        let players: Vec<Value> = players
            .iter()
            .map(|(name, old, new)| json!({ "name": name, "old_uuid": old, "new_uuid": new }))
            .collect();
        json!({ "players": players, "unmatched": [] })
    }

    /// Every file under `dir` with its contents.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut dirs = vec![dir.to_owned()];
        while let Some(next) = dirs.pop() {
            for entry in fs::read_dir(next).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.insert(
                        path.strip_prefix(dir).unwrap().to_owned(),
                        fs::read(&path).unwrap(),
                    );
                }
            }
        }
        files
    }

    #[test]
    fn moves_references_in_every_format_and_file_once() {
        let dir = TempDir::new().unwrap();
        let server = dir.path();
        let world = server.join("world");
        let mapping = json!({
            "players": [{ "name": "Notch", "old_uuid": NOTCH_OLD, "new_uuid": NOTCH_NEW }],
            "unmatched": [GHOST],
        });
        fs::create_dir_all(world.join("players/data")).unwrap();
        fs::write(
            world.join("players/uuid-migration.json"),
            mapping.to_string(),
        )
        .unwrap();

        let overworld = world.join("dimensions/minecraft/overworld");
        let anvil = AnvilChunkConfig::default();
        let entities_mca = overworld.join("entities/r.0.0.mca");
        write_region::<AnvilRegion>(
            &entities_mca,
            &anvil,
            &[entities(NOTCH_OLD), untouched_chunk()],
        );
        let region_linear = overworld.join("region/r.0.0.linear");
        write_region::<LinearV2File>(&region_linear, &(), &[chunk_with_block_entities(NOTCH_OLD)]);
        let nether_pump = world.join("DIM-1/entities/r.0.0.pump");
        write_region::<PumpFile>(&nether_pump, &(), &[entities(NOTCH_OLD)]);
        let untouched_mca = world.join("DIM1/region/r.0.0.mca");
        write_region::<AnvilRegion>(&untouched_mca, &anvil, &[untouched_chunk()]);

        let notch = world.join(format!("players/data/{NOTCH_NEW}.dat"));
        write_gzip(&notch, &player_data(NOTCH_OLD));
        let notch_dat_old = world.join(format!("players/data/{NOTCH_NEW}.dat_old"));
        write_gzip(&notch_dat_old, &player_data(NOTCH_OLD));
        let boss_events_dat = world.join("data/minecraft/custom_boss_events.dat");
        write_gzip(&boss_events_dat, &boss_events(NOTCH_OLD));
        let raids = overworld.join("data/minecraft/raids.dat");
        write_gzip(&raids, &raids_data(NOTCH_OLD));
        let weather = world.join("data/minecraft/weather.dat");
        write_gzip(
            &weather,
            &compound(nbt_compound_tag! { "clear_weather_time": NbtTag::Int(7) }),
        );
        fs::write(world.join("data/not_nbt.dat"), b"not gzip").unwrap();
        let before = snapshot(server);

        let config = ChunkConfig::Anvil(anvil);
        migrate_world_uuids(server, Path::new("world"), &config, 1).unwrap();

        assert_eq!(
            read_region::<AnvilRegion>(&entities_mca),
            [entities(NOTCH_NEW), untouched_chunk()]
        );
        assert_eq!(
            read_region::<LinearV2File>(&region_linear),
            [chunk_with_block_entities(NOTCH_NEW)]
        );
        assert_eq!(read_region::<PumpFile>(&nether_pump), [entities(NOTCH_NEW)]);
        assert_eq!(read_gzip(&notch), player_data(NOTCH_NEW));
        assert_eq!(read_gzip(&notch_dat_old), player_data(NOTCH_NEW));
        assert_eq!(read_gzip(&boss_events_dat), boss_events(NOTCH_NEW));
        assert_eq!(read_gzip(&raids), raids_data(NOTCH_NEW));

        let after = snapshot(server);
        let changed = [
            &entities_mca,
            &region_linear,
            &nether_pump,
            &notch,
            &notch_dat_old,
            &boss_events_dat,
            &raids,
        ];
        for path in changed {
            let path = path.strip_prefix(server).unwrap();
            let backup = PathBuf::from(format!("{}.v1.bak", path.display()));
            assert_ne!(after[path], before[path], "{}", path.display());
            assert_eq!(after[&backup], before[path], "{}", backup.display());
        }
        let backups = after
            .keys()
            .filter(|path| path.extension().is_some_and(|ext| ext == "bak"));
        assert_eq!(backups.count(), changed.len());
        for path in [&untouched_mca, &weather] {
            let path = path.strip_prefix(server).unwrap();
            assert_eq!(after[path], before[path], "{}", path.display());
        }
        assert_eq!(
            read_record(&world),
            record_of(&[("Notch", NOTCH_OLD, NOTCH_NEW)])
        );

        migrate_world_uuids(server, Path::new("world"), &config, 1).unwrap();
        assert_eq!(snapshot(server), after);
    }

    fn wolves(owners: &[&str]) -> NbtCompound {
        let wolves = owners
            .iter()
            .map(|owner| {
                nbt_compound_tag! {
                    "id": NbtTag::from("minecraft:wolf"),
                    "Owner": uuid(owner)
                }
            })
            .collect();
        compound(nbt_compound_tag! { "Entities": NbtTag::List(wolves) })
    }

    /// Ghost's player data, with their own UUID and their shoulder parrot's owner both `ghost`.
    fn ghost_data(ghost: &str) -> NbtCompound {
        compound(nbt_compound_tag! {
            "UUID": uuid(ghost),
            "ShoulderEntityLeft": nbt_compound_tag! {
                "id": NbtTag::from("minecraft:parrot"),
                "Owner": uuid(ghost)
            }
        })
    }

    /// Starts after the `config_version` 1 to 2 step of a build without [`RECORD_FILE`]: Notch's
    /// references are moved, Ghost is unmatched. Ghost is then moved at login.
    #[test]
    fn moves_references_to_players_added_to_the_mapping_at_the_next_start() {
        let dir = TempDir::new().unwrap();
        let server = dir.path();
        let world = server.join("world");
        let mapping = json!({
            "players": [{ "name": "Notch", "old_uuid": NOTCH_OLD, "new_uuid": NOTCH_NEW }],
            "unmatched": [GHOST],
        });
        fs::create_dir_all(world.join("players")).unwrap();
        fs::write(
            world.join("players/uuid-migration.json"),
            mapping.to_string(),
        )
        .unwrap();
        let overworld = world.join("dimensions/minecraft/overworld");
        let anvil = AnvilChunkConfig::default();
        let entities_mca = overworld.join("entities/r.0.0.mca");
        write_region::<AnvilRegion>(&entities_mca, &anvil, &[wolves(&[NOTCH_NEW, GHOST])]);
        let region_mca = overworld.join("region/r.0.0.mca");
        write_region::<AnvilRegion>(&region_mca, &anvil, &[chunk_with_block_entities(NOTCH_NEW)]);
        write_gzip(
            &world.join(format!("players/data/{GHOST}.dat")),
            &ghost_data(GHOST),
        );
        let config = ChunkConfig::Anvil(anvil);
        let run = || migrate_world_uuids_of_new_players(server, Path::new("world"), &config);

        // Without a record every player counts as new, but Notch's references are already moved.
        let before = snapshot(server);
        run().unwrap();
        let after = snapshot(server);
        let record_path = Path::new("world/players").join(RECORD_FILE);
        assert!(!before.contains_key(&record_path));
        assert_eq!(after.len(), before.len() + 1);
        assert!(
            before
                .iter()
                .all(|(path, contents)| after[path] == *contents)
        );
        let notch = ("Notch", NOTCH_OLD, NOTCH_NEW);
        assert_eq!(read_record(&world), record_of(&[notch]));

        // With no new players, no region file is read: this one would fail.
        let not_a_region = overworld.join("region/r.1.0.mca");
        fs::create_dir(&not_a_region).unwrap();
        run().unwrap();
        assert_eq!(snapshot(server), after);

        // Ghost logs in, and their files move to the new UUID with their parrot's old owner.
        assert!(migrate_offline_player(&world, "Ghost").unwrap());
        let error = run().unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with(&not_a_region.display().to_string())
        );
        assert_eq!(read_record(&world), record_of(&[notch]));

        // The next start tries again.
        fs::remove_dir(&not_a_region).unwrap();
        run().unwrap();
        assert_eq!(
            read_region::<AnvilRegion>(&entities_mca),
            [wolves(&[NOTCH_NEW, GHOST_NEW])]
        );
        let ghost = world.join(format!("players/data/{GHOST_NEW}.dat"));
        assert_eq!(read_gzip(&ghost), ghost_data(GHOST_NEW));
        let region_mca = region_mca.strip_prefix(server).unwrap();
        assert_eq!(snapshot(server)[region_mca], after[region_mca]);
        assert_eq!(
            read_record(&world),
            record_of(&[("Ghost", GHOST, GHOST_NEW), notch])
        );

        let after = snapshot(server);
        run().unwrap();
        assert_eq!(snapshot(server), after);
    }
}
