//! Moves PEX 1.x's advancement and statistics files to vanilla's form (G7-22).
//!
//! PEX 1.x wrote `players/advancements/<uuid>.json` in a form of its own and kept statistics in
//! the player file under `Statistics`. Vanilla reads neither: it reads its advancement form and
//! `players/stats/<uuid>.json`. The server reads both forms, so this only matters for a world
//! that goes to vanilla, but it runs once for every player so none has to log in first.

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use pumpkin_config::migration::write_backup;
use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
use pumpkin_world::chunk::io::write_file_atomically;
use serde_json::Value;
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::player::advancement::legacy_advancements_to_vanilla;
use crate::entity::player::statistics::{LEGACY_NBT_KEY, Statistics};

/// A file the move couldn't read or write.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub struct PlayerFilesError {
    path: PathBuf,
    source: io::Error,
}

fn at(path: &Path) -> impl FnOnce(io::Error) -> PlayerFilesError + '_ {
    move |source| PlayerFilesError {
        path: path.to_owned(),
        source,
    }
}

/// Moves the world's PEX 1.x advancement and statistics files to vanilla's form.
///
/// Rewrites PEX 1.x advancement files in vanilla's form and moves the statistics in player files
/// to `players/stats/<uuid>.json`. Each file it changes is first copied to
/// `<name>.v<version>.bak`. A second run changes nothing, and a run that stopped halfway is
/// finished by the next one.
pub fn migrate_player_files(
    server_dir: &Path,
    world_dir: &Path,
    version: u32,
) -> Result<(), PlayerFilesError> {
    let players_dir = server_dir.join(world_dir).join("players");
    let advancements = migrate_advancements(&players_dir.join("advancements"), version)?;
    let statistics = migrate_statistics(&players_dir, version)?;
    if advancements > 0 || statistics > 0 {
        info!(
            "Moved player files to vanilla's form: {advancements} advancement file(s) and the statistics of {statistics} player(s)"
        );
    }
    Ok(())
}

/// The files in `dir` named `<uuid><suffix>`.
fn player_files(dir: &Path, suffix: &str) -> Result<Vec<(Uuid, PathBuf)>, PlayerFilesError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(at(dir)(err)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(at(dir))?.path();
        let uuid = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(suffix))
            .and_then(|stem| Uuid::parse_str(stem).ok());
        if let Some(uuid) = uuid
            && path.is_file()
        {
            files.push((uuid, path));
        }
    }
    files.sort();
    Ok(files)
}

fn log_backup(path: &Path, backup: &Path) {
    info!(
        "Saved a copy of {} as {} before moving it to vanilla's form",
        path.display(),
        backup.file_name().unwrap_or_default().to_string_lossy()
    );
}

fn write_json(path: &Path, json: &Value) -> Result<(), PlayerFilesError> {
    let bytes = serde_json::to_vec_pretty(json).map_err(|err| at(path)(io::Error::other(err)))?;
    write_file_atomically(path, &bytes, true).map_err(at(path))
}

fn migrate_advancements(dir: &Path, version: u32) -> Result<usize, PlayerFilesError> {
    let mut moved = 0;
    for (_, path) in player_files(dir, ".json")? {
        let bytes = fs::read(&path).map_err(at(&path))?;
        let Ok(file) = serde_json::from_slice::<Value>(&bytes) else {
            warn!("Left {} as it is: it isn't valid JSON", path.display());
            continue;
        };
        let Some(vanilla) = legacy_advancements_to_vanilla(&file) else {
            continue;
        };
        let backup = write_backup(&path, &bytes, version).map_err(at(&path))?;
        log_backup(&path, &backup);
        write_json(&path, &vanilla)?;
        moved += 1;
    }
    Ok(moved)
}

/// Moves `Statistics` out of each player file. A statistics file the world already has (from a
/// time it ran on vanilla) keeps the statistics PEX didn't count; those PEX counted win.
fn migrate_statistics(players_dir: &Path, version: u32) -> Result<usize, PlayerFilesError> {
    let mut moved = 0;
    for (uuid, path) in player_files(&players_dir.join("data"), ".dat")? {
        let bytes = fs::read(&path).map_err(at(&path))?;
        let Ok(mut nbt) = read_gzip_compound_tag(Cursor::new(&bytes)) else {
            warn!("Left {} as it is: it isn't a player file", path.display());
            continue;
        };
        if !nbt.has(LEGACY_NBT_KEY) {
            continue;
        }

        let stats_path = players_dir.join("stats").join(format!("{uuid}.json"));
        let mut stats = Statistics::default();
        match fs::read(&stats_path) {
            Ok(existing) => {
                if let Ok(json) = serde_json::from_slice::<Value>(&existing) {
                    stats.read_vanilla_json(&json);
                }
                let backup =
                    write_backup(&stats_path, &existing, version).map_err(at(&stats_path))?;
                log_backup(&stats_path, &backup);
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(at(&stats_path)(err)),
        }
        stats.read_nbt(&nbt);
        // The statistics file first: a run that stops before the player file is rewritten moves
        // the same statistics again.
        write_json(&stats_path, &stats.to_vanilla_json())?;

        let backup = write_backup(&path, &bytes, version).map_err(at(&path))?;
        log_backup(&path, &backup);
        nbt.child_tags.remove(LEGACY_NBT_KEY);
        let rewritten = write_gzip_compound_tag_to_bytes(nbt)
            .map_err(|err| at(&path)(io::Error::other(err)))?;
        write_file_atomically(&path, &rewritten, true).map_err(at(&path))?;
        moved += 1;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::statistics::{CustomStatistic, StatisticCategory};
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_world::chunk::format::anvil::WORLD_DATA_VERSION;
    use serde_json::json;

    const PLAYER: &str = "b50ad385-829d-3141-a216-7e7d7539ba7f";

    fn backups(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "bak"))
            .collect();
        names.sort();
        names
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    /// A PEX 1.x player directory: advancements in PEX's form, statistics in the player file,
    /// and a statistics file from a time the world ran on vanilla.
    fn pex_1_world(server: &Path) -> PathBuf {
        let players = server.join("world/players");
        fs::create_dir_all(players.join("data")).unwrap();
        fs::create_dir_all(players.join("advancements")).unwrap();
        fs::create_dir_all(players.join("stats")).unwrap();

        let mut legacy = NbtCompound::new();
        legacy.put_int(&format!("8:{}", CustomStatistic::Jump as i32), 7);
        let mut player = NbtCompound::new();
        player.put_compound(LEGACY_NBT_KEY, legacy);
        player.put_int("XpLevel", 3);
        fs::write(
            players.join(format!("data/{PLAYER}.dat")),
            write_gzip_compound_tag_to_bytes(player).unwrap(),
        )
        .unwrap();
        fs::write(
            players.join(format!("stats/{PLAYER}.json")),
            json!({
                "stats": { "minecraft:custom": { "minecraft:jump": 1, "minecraft:deaths": 2 } },
                "DataVersion": 4671,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            players.join(format!("advancements/{PLAYER}.json")),
            json!({
                "minecraft:story/root": {
                    "crafting_table": { "secs_since_epoch": 1_763_704_800, "nanos_since_epoch": 0 },
                },
            })
            .to_string(),
        )
        .unwrap();
        players
    }

    #[test]
    fn a_pex_1_player_directory_moves_to_vanillas_files_once() {
        let server = tempfile::tempdir().unwrap();
        let players = pex_1_world(server.path());

        migrate_player_files(server.path(), Path::new("world"), 3).unwrap();

        let mut stats = Statistics::default();
        stats.read_vanilla_json(&read_json(&players.join(format!("stats/{PLAYER}.json"))));
        assert_eq!(
            stats.get(StatisticCategory::Custom, CustomStatistic::Jump as i32),
            7
        );
        assert_eq!(
            stats.get(StatisticCategory::Custom, CustomStatistic::Deaths as i32),
            2
        );
        let player = read_gzip_compound_tag(Cursor::new(
            fs::read(players.join(format!("data/{PLAYER}.dat"))).unwrap(),
        ))
        .unwrap();
        assert!(!player.has(LEGACY_NBT_KEY));
        assert_eq!(player.get_int("XpLevel"), Some(3));
        assert_eq!(
            read_json(&players.join(format!("advancements/{PLAYER}.json"))),
            json!({
                "minecraft:story/root": {
                    "criteria": { "crafting_table": "2025-11-21 06:00:00 +0000" },
                    "done": true,
                },
                "DataVersion": WORLD_DATA_VERSION,
            })
        );
        let expected_backups = |name: &str| vec![format!("{PLAYER}.{name}.v3.bak")];
        assert_eq!(backups(&players.join("data")), expected_backups("dat"));
        assert_eq!(backups(&players.join("stats")), expected_backups("json"));
        assert_eq!(
            backups(&players.join("advancements")),
            expected_backups("json")
        );

        let before = read_json(&players.join(format!("stats/{PLAYER}.json")));
        migrate_player_files(server.path(), Path::new("world"), 3).unwrap();
        assert_eq!(
            read_json(&players.join(format!("stats/{PLAYER}.json"))),
            before
        );
        assert_eq!(backups(&players.join("data")), expected_backups("dat"));
        assert_eq!(
            backups(&players.join("advancements")),
            expected_backups("json")
        );
    }

    #[test]
    fn a_world_without_players_needs_nothing() {
        let server = tempfile::tempdir().unwrap();
        migrate_player_files(server.path(), Path::new("world"), 3).unwrap();
        assert!(!server.path().join("world").exists());
    }

    #[test]
    fn statistics_files_sit_next_to_the_player_data() {
        let world = tempfile::tempdir().unwrap();
        let data = crate::data::player_server::ServerPlayerData::new(
            world.path().join("players/data"),
            std::time::Duration::from_secs(60),
            true,
        );
        let uuid = Uuid::parse_str(PLAYER).unwrap();
        assert_eq!(
            data.stats_path(&uuid),
            world.path().join(format!("players/stats/{PLAYER}.json"))
        );
    }
}
