//! Moves offline-mode players from the UUIDs PEX 1.x gave them to vanilla's.
//!
//! A PEX 1.x offline UUID ([`legacy_offline_uuid`]) is a hash of the name, so it can't be turned
//! back into one. The names come from `usercache.json` and the player lists instead, the way
//! vanilla `OldUsersConverter` takes them from the old name-keyed lists.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use pumpkin_config::migration::{backup_dir, write_backup};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
use pumpkin_world::chunk::io::write_file_atomically;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use uuid::{Uuid, Variant, Version};

use super::banned_player::BannedPlayerList;
use super::op::OperatorConfig;
use super::usercache::USER_CACHE_PATH;
use super::whitelist::WhitelistConfig;
use super::{DATA_FOLDER, LoadJSONConfiguration};
use crate::net::{legacy_offline_uuid, offline_uuid};

/// The file in the world's `players` directory that lists each migrated player's old and new
/// UUID, for migrating references to them in the world and in plugin data.
pub const MAPPING_FILE: &str = "uuid-migration.json";

const PLAYERS_DIR: &str = "players";

/// The per-player files under [`PLAYERS_DIR`], named `<uuid><suffix>` (vanilla
/// `LevelResource.PLAYER_DATA_DIR`, `PLAYER_ADVANCEMENTS_DIR` and `PLAYER_STATS_DIR`), and whether
/// they are player data. Vanilla `PlayerDataStorage` keeps the previous player data as `.dat_old`
/// and loads it when `.dat` can't be read. PEX keeps statistics in the player data, but a world
/// that ran on vanilla may have them.
const PLAYER_FILES: [(&str, &str, bool); 4] = [
    ("data", ".dat", true),
    ("data", ".dat_old", true),
    ("advancements", ".json", false),
    ("stats", ".json", false),
];

/// A file the migration couldn't read or write.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub struct MigrationError {
    path: PathBuf,
    source: io::Error,
}

fn at(path: &Path) -> impl FnOnce(io::Error) -> MigrationError + '_ {
    move |source| MigrationError {
        path: path.to_owned(),
        source,
    }
}

/// The contents of [`MAPPING_FILE`].
#[derive(Serialize, Deserialize)]
struct Mapping {
    players: Vec<MappedPlayer>,
    /// Players with files under a UUID that no name matched. They keep that UUID.
    unmatched: Vec<Uuid>,
}

#[derive(Serialize, Deserialize, Clone)]
struct MappedPlayer {
    name: String,
    old_uuid: Uuid,
    new_uuid: Uuid,
}

/// One of the JSON lists in `data/` with an entry per player.
struct PlayerList {
    path: PathBuf,
    original: String,
    entries: Vec<Value>,
    changed: bool,
}

/// A player file to move from its old UUID to the new one.
struct PlayerFile {
    from: PathBuf,
    to: PathBuf,
    old_uuid: Uuid,
    /// Whether it is player data: gzipped NBT with the player's own UUID in it.
    player_data: bool,
}

/// Moves offline-mode players from their PEX 1.x UUIDs to vanilla's.
///
/// Renames their player data, advancements and statistics in the world's `players` directory,
/// and rewrites their entries in `usercache.json`, `ops.json`, `whitelist.json` and
/// `banned-players.json` in `data/`. Whatever it changes is copied to `<name>.v<version>.bak`
/// first, and the old and new UUIDs are added to [`MAPPING_FILE`].
///
/// Only UUIDs that are the PEX 1.x UUID of a name in those lists change, so Mojang UUIDs and
/// players already on vanilla's UUIDs are left alone, and a second run changes nothing. A run that
/// stopped halfway is finished by the next one.
pub fn migrate_offline_uuids(
    server_dir: &Path,
    world_dir: &Path,
    version: u32,
) -> Result<(), MigrationError> {
    let players_dir = server_dir.join(world_dir).join(PLAYERS_DIR);
    let mut lists = read_player_lists(&server_dir.join(DATA_FOLDER))?;
    let players = players_by_old_uuid(&lists);

    let mut migrated = BTreeSet::new();
    let (files, unmatched) = find_player_files(&players_dir, &players)?;
    migrated.extend(files.iter().map(|file| file.old_uuid));
    for list in &mut lists {
        update_entries(list, &players, &mut migrated);
    }
    if migrated.is_empty() && unmatched.is_empty() {
        return Ok(());
    }

    if !files.is_empty() {
        let backup = backup_dir(&players_dir, version).map_err(at(&players_dir))?;
        log_backup(&players_dir, &backup);
    }
    let changed_lists: Vec<&PlayerList> = lists.iter().filter(|list| list.changed).collect();
    for list in &changed_lists {
        let backup = write_backup(&list.path, &list.original, version).map_err(at(&list.path))?;
        log_backup(&list.path, &backup);
    }

    // Written before anything moves, so a run that stops halfway still records every player.
    let mapping_path = players_dir.join(MAPPING_FILE);
    write_mapping(
        &mapping_path,
        migrated.iter().map(|old_uuid| players[old_uuid].clone()),
        &unmatched,
    )?;

    for file in &files {
        move_player_file(file, &players[&file.old_uuid])?;
    }
    for list in changed_lists {
        let json = serde_json::to_string_pretty(&list.entries)
            .map_err(|err| at(&list.path)(err.into()))?;
        write_file_atomically(&list.path, json.as_bytes(), true).map_err(at(&list.path))?;
    }

    if !migrated.is_empty() {
        info!(
            "Moved {} offline player(s) to vanilla UUIDs. Their old and new UUIDs are in {}",
            migrated.len(),
            mapping_path.display()
        );
    }
    if !unmatched.is_empty() {
        warn!(
            "{} player UUID(s) match no name in {USER_CACHE_PATH} or the player lists, so their files keep them. They are listed in {}",
            unmatched.len(),
            mapping_path.display()
        );
    }
    Ok(())
}

fn read_player_lists(data_dir: &Path) -> Result<Vec<PlayerList>, MigrationError> {
    let mut lists = Vec::new();
    for file in [
        Path::new(USER_CACHE_PATH),
        OperatorConfig::get_path(),
        WhitelistConfig::get_path(),
        BannedPlayerList::get_path(),
    ] {
        let path = data_dir.join(file);
        let original = match fs::read_to_string(&path) {
            Ok(original) => original,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(at(&path)(err)),
        };
        match serde_json::from_str(&original) {
            Ok(entries) => lists.push(PlayerList {
                path,
                original,
                entries,
                changed: false,
            }),
            Err(err) => warn!(
                "Couldn't read {} as a list of players, so its entries keep their old UUIDs: {err}",
                path.display()
            ),
        }
    }
    Ok(lists)
}

/// Every name in the lists, by the UUID PEX 1.x gave it.
fn players_by_old_uuid(lists: &[PlayerList]) -> FxHashMap<Uuid, MappedPlayer> {
    let mut players = FxHashMap::default();
    let names = lists
        .iter()
        .flat_map(|list| &list.entries)
        .filter_map(|entry| entry.get("name")?.as_str());
    for name in names {
        let old_uuid = legacy_offline_uuid(name);
        players.entry(old_uuid).or_insert_with(|| MappedPlayer {
            name: name.to_owned(),
            old_uuid,
            new_uuid: offline_uuid(name),
        });
    }
    players
}

/// Finds the player files to move, and the UUIDs of files no name matches.
fn find_player_files(
    players_dir: &Path,
    players: &FxHashMap<Uuid, MappedPlayer>,
) -> Result<(Vec<PlayerFile>, BTreeSet<Uuid>), MigrationError> {
    let mut files = Vec::new();
    let mut unmatched = BTreeSet::new();
    for (dir, suffix, player_data) in PLAYER_FILES {
        let dir = players_dir.join(dir);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(at(&dir)(err)),
        };
        for entry in entries {
            let entry = entry.map_err(at(&dir))?;
            let file_name = entry.file_name();
            let Some(uuid) = file_name
                .to_str()
                .and_then(|name| name.strip_suffix(suffix))
                .and_then(|stem| Uuid::try_parse(stem).ok())
            else {
                continue;
            };
            if let Some(player) = players.get(&uuid) {
                files.push(PlayerFile {
                    from: entry.path(),
                    to: dir.join(format!("{}{suffix}", player.new_uuid)),
                    old_uuid: uuid,
                    player_data,
                });
            } else if !is_vanilla_uuid(uuid) {
                unmatched.insert(uuid);
            }
        }
    }
    files.sort_by(|a, b| a.from.cmp(&b.from));
    Ok((files, unmatched))
}

/// Whether `uuid` can be one vanilla gives players: version 3 in offline mode, version 4 from
/// Mojang. A PEX 1.x UUID is raw hash bytes, so about 1 in 32 look like one of them by chance.
fn is_vanilla_uuid(uuid: Uuid) -> bool {
    uuid.get_variant() == Variant::RFC4122
        && matches!(uuid.get_version(), Some(Version::Md5 | Version::Random))
}

fn update_entries(
    list: &mut PlayerList,
    players: &FxHashMap<Uuid, MappedPlayer>,
    migrated: &mut BTreeSet<Uuid>,
) {
    for entry in &mut list.entries {
        let Some(uuid) = entry.get_mut("uuid") else {
            continue;
        };
        let Some(player) = uuid
            .as_str()
            .and_then(|uuid| Uuid::try_parse(uuid).ok())
            .and_then(|old_uuid| players.get(&old_uuid))
        else {
            continue;
        };
        *uuid = Value::String(player.new_uuid.to_string());
        migrated.insert(player.old_uuid);
        list.changed = true;
    }
}

/// Adds `migrated` to the mapping file, keeping the players an earlier run recorded.
fn write_mapping(
    path: &Path,
    migrated: impl Iterator<Item = MappedPlayer>,
    unmatched: &BTreeSet<Uuid>,
) -> Result<(), MigrationError> {
    let previous: Option<Mapping> = match fs::read(path) {
        Ok(json) => Some(serde_json::from_slice(&json).map_err(|err| at(path)(err.into()))?),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(at(path)(err)),
    };
    let mut players: BTreeMap<Uuid, MappedPlayer> = previous
        .iter()
        .flat_map(|mapping| mapping.players.iter().cloned())
        .map(|player| (player.old_uuid, player))
        .collect();
    for player in migrated {
        players.entry(player.old_uuid).or_insert(player);
    }
    let mut players: Vec<MappedPlayer> = players.into_values().collect();
    players.sort_by(|a, b| a.name.cmp(&b.name).then(a.old_uuid.cmp(&b.old_uuid)));
    let mapping = Mapping {
        players,
        unmatched: unmatched.iter().copied().collect(),
    };
    let json = serde_json::to_string_pretty(&mapping).map_err(|err| at(path)(err.into()))?;
    write_file_atomically(path, json.as_bytes(), true).map_err(at(path))
}

#[derive(PartialEq)]
enum Contents {
    PlayerData(NbtCompound),
    Bytes(Vec<u8>),
}

fn read_contents(path: &Path, player_data: bool) -> Result<Contents, MigrationError> {
    let bytes = fs::read(path).map_err(at(path))?;
    if player_data && let Ok(nbt) = read_gzip_compound_tag(Cursor::new(&bytes)) {
        return Ok(Contents::PlayerData(nbt));
    }
    Ok(Contents::Bytes(bytes))
}

/// Writes `file` under the new UUID and removes the old one. Player data also gets the new UUID
/// in its `UUID` tag, because vanilla `Entity.load` gives it to the player. Keeps both if the new
/// UUID already has a different file.
fn move_player_file(file: &PlayerFile, player: &MappedPlayer) -> Result<(), MigrationError> {
    let mut contents = read_contents(&file.from, file.player_data)?;
    if let Contents::PlayerData(nbt) = &mut contents
        && nbt.get_uuid("UUID") == Some(player.old_uuid)
    {
        nbt.put_uuid("UUID", player.new_uuid);
    }
    if file.to.exists() {
        // A run that stopped between writing the new file and removing the old one leaves both.
        if read_contents(&file.to, file.player_data)? == contents {
            return fs::remove_file(&file.from).map_err(at(&file.from));
        }
        warn!(
            "Kept {}: {} already has {}",
            file.from.display(),
            player.name,
            file.to.display()
        );
        return Ok(());
    }
    let bytes = match contents {
        Contents::PlayerData(nbt) => write_gzip_compound_tag_to_bytes(nbt)
            .map_err(|err| at(&file.from)(io::Error::other(err)))?,
        Contents::Bytes(bytes) => bytes,
    };
    write_file_atomically(&file.to, &bytes, true).map_err(at(&file.to))?;
    fs::remove_file(&file.from).map_err(at(&file.from))
}

fn log_backup(path: &Path, backup: &Path) {
    info!(
        "Saved a copy of {} as {} before moving offline players to vanilla UUIDs",
        path.display(),
        backup.file_name().unwrap_or_default().to_string_lossy()
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
    use serde_json::{Value, json};
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::migrate_offline_uuids;

    // The lists PEX 1.2.0 wrote in offline mode after `whitelist add` for five names, `op Notch`
    // and `ban jeb_ griefing`, with Notch, Player_123456789, 玩家一号 and Ghost logged in. Ghost is
    // taken out again, as if its cache entry had expired.
    const USERCACHE: &str = r#"[{"uuid":"e893d087-028b-377e-080a-c57e028f96e9","name":"玩家一号","expiresOn":"2026-11-24T05:32:34.945999766Z"},{"uuid":"fe3625b2-f47a-1462-c456-19d51eedef7d","name":"Player_123456789","expiresOn":"2026-11-24T05:32:32.892943521Z"},{"uuid":"ff4af744-2839-cc96-4605-0dd2214a30c1","name":"Notch","expiresOn":"2026-11-24T05:32:30.893413454Z"},{"uuid":"c0582bcb-b509-f2fb-1b73-d67254566a75","name":"jeb_","expiresOn":"2026-11-24T05:31:51.758243952Z"},{"uuid":"45399c0f-6260-6f57-26d6-1a2841727956","name":"🎮gamer","expiresOn":"2026-11-24T05:31:48.747146173Z"}]"#;
    const OPS: &str = r#"[
  {
    "uuid": "ff4af744-2839-cc96-4605-0dd2214a30c1",
    "name": "Notch",
    "level": 4,
    "bypassesPlayerLimit": false
  }
]"#;
    const WHITELIST: &str = r#"[
  {
    "uuid": "ff4af744-2839-cc96-4605-0dd2214a30c1",
    "name": "Notch"
  },
  {
    "uuid": "fe3625b2-f47a-1462-c456-19d51eedef7d",
    "name": "Player_123456789"
  },
  {
    "uuid": "e893d087-028b-377e-080a-c57e028f96e9",
    "name": "玩家一号"
  },
  {
    "uuid": "45399c0f-6260-6f57-26d6-1a2841727956",
    "name": "🎮gamer"
  }
]"#;
    const BANNED_PLAYERS: &str = r#"[
  {
    "uuid": "c0582bcb-b509-f2fb-1b73-d67254566a75",
    "name": "jeb_",
    "created": "2026-10-25 05:31:51+00:00",
    "source": "Server",
    "expires": "forever",
    "reason": "griefing"
  }
]"#;
    const LISTS: [(&str, &str); 4] = [
        ("usercache.json", USERCACHE),
        ("ops.json", OPS),
        ("whitelist.json", WHITELIST),
        ("banned-players.json", BANNED_PLAYERS),
    ];

    // (PEX 1.2.0 UUID from the lists above, UUID a vanilla 26.3 server gave the same name).
    const NOTCH: (&str, &str) = (
        "ff4af744-2839-cc96-4605-0dd2214a30c1",
        "b50ad385-829d-3141-a216-7e7d7539ba7f",
    );
    const PLAYER: (&str, &str) = (
        "fe3625b2-f47a-1462-c456-19d51eedef7d",
        "c0d79c73-221a-32dc-b983-3cfa4a8a971a",
    );
    const CJK: (&str, &str) = (
        "e893d087-028b-377e-080a-c57e028f96e9",
        "563807d4-80f7-3b64-9ebb-4dbce5bc9f41",
    );
    const GAMER: (&str, &str) = (
        "45399c0f-6260-6f57-26d6-1a2841727956",
        "501e0b86-57fc-33e9-9853-97f55e79973f",
    );
    const JEB: (&str, &str) = (
        "c0582bcb-b509-f2fb-1b73-d67254566a75",
        "a762f560-4fce-3236-812a-b80efff0b62b",
    );
    /// Ghost's PEX 1.2.0 UUID.
    const GHOST: &str = "df1bc498-4a05-5e10-b286-3eb03abf1170";
    /// Notch's Mojang UUID, which an online-mode player file has.
    const MOJANG: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";

    fn write(dir: &Path, path: &str, contents: &[u8]) {
        let path = dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn player_data(uuid: &str, xp_level: i32) -> Vec<u8> {
        let mut nbt = NbtCompound::new();
        nbt.put_uuid("UUID", uuid.parse().unwrap());
        nbt.put_int("XpLevel", xp_level);
        write_gzip_compound_tag_to_bytes(nbt).unwrap()
    }

    fn read_player_data(dir: &Path, uuid: &str) -> NbtCompound {
        read_nbt(dir, &format!("world/players/data/{uuid}.dat"))
    }

    fn read_nbt(dir: &Path, path: &str) -> NbtCompound {
        read_gzip_compound_tag(fs::File::open(dir.join(path)).unwrap()).unwrap()
    }

    /// A 1.2.0 server directory: the lists above, player data with a different `XpLevel` for
    /// each player, advancements, a vanilla statistics file, Ghost's data and an online player's.
    fn server_1_2(dir: &Path) {
        for (name, contents) in LISTS {
            write(dir, &format!("data/{name}"), contents.as_bytes());
        }
        for (xp_level, (old, _)) in (1..).zip([NOTCH, PLAYER, CJK]) {
            write(
                dir,
                &format!("world/players/data/{old}.dat"),
                &player_data(old, xp_level),
            );
            write(
                dir,
                &format!("world/players/advancements/{old}.json"),
                format!("{{\"player\": {xp_level}}}").as_bytes(),
            );
        }
        write(
            dir,
            &format!("world/players/stats/{}.json", NOTCH.0),
            br#"{"stats":{}}"#,
        );
        write(
            dir,
            &format!("world/players/data/{GHOST}.dat"),
            &player_data(GHOST, 9),
        );
        write(
            dir,
            &format!("world/players/data/{MOJANG}.dat"),
            &player_data(MOJANG, 10),
        );
    }

    /// Every file under `dir` with its contents.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, files);
                } else {
                    let contents = fs::read(&path).unwrap();
                    files.insert(path.strip_prefix(root).unwrap().to_owned(), contents);
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(dir, dir, &mut files);
        files
    }

    fn read_json(dir: &Path, path: &str) -> Value {
        serde_json::from_slice(&fs::read(dir.join(path)).unwrap()).unwrap()
    }

    fn mapped(name: &str, (old, new): (&str, &str)) -> Value {
        json!({ "name": name, "old_uuid": old, "new_uuid": new })
    }

    #[test]
    fn migrates_a_1_2_server_once() {
        let dir = TempDir::new().unwrap();
        let dir = dir.path();
        server_1_2(dir);
        // What a build that saves player data like vanilla `PlayerDataStorage` adds.
        let notch_dat_old = |uuid: &str| format!("world/players/data/{uuid}.dat_old");
        let notch_corrupted = format!(
            "world/players/data/{}_corrupted_2026-10-26_17-41-37.dat",
            NOTCH.0
        );
        write(dir, &notch_dat_old(NOTCH.0), &player_data(NOTCH.0, 11));
        write(dir, &notch_corrupted, b"not nbt");
        let before = snapshot(dir);

        migrate_offline_uuids(dir, Path::new("world"), 1).unwrap();

        for (xp_level, (old, new)) in (1..).zip([NOTCH, PLAYER, CJK]) {
            let nbt = read_player_data(dir, new);
            assert_eq!(nbt.get_uuid("UUID"), Some(new.parse().unwrap()));
            assert_eq!(nbt.get_int("XpLevel"), Some(xp_level));
            assert!(!dir.join(format!("world/players/data/{old}.dat")).exists());
            assert_eq!(
                fs::read(dir.join(format!("world/players/advancements/{new}.json"))).unwrap(),
                before[Path::new(&format!("world/players/advancements/{old}.json"))]
            );
        }
        assert!(
            dir.join(format!("world/players/stats/{}.json", NOTCH.1))
                .exists()
        );
        let dat_old = read_nbt(dir, &notch_dat_old(NOTCH.1));
        assert_eq!(dat_old.get_uuid("UUID"), Some(NOTCH.1.parse().unwrap()));
        assert_eq!(dat_old.get_int("XpLevel"), Some(11));
        assert!(!dir.join(notch_dat_old(NOTCH.0)).exists());
        for untouched in [
            format!("world/players/data/{GHOST}.dat"),
            format!("world/players/data/{MOJANG}.dat"),
            notch_corrupted,
        ] {
            assert_eq!(
                fs::read(dir.join(&untouched)).unwrap(),
                before[Path::new(&untouched)]
            );
        }

        for (name, original) in LISTS {
            let path = format!("data/{name}");
            let expected = [NOTCH, PLAYER, CJK, GAMER, JEB]
                .into_iter()
                .fold(original.to_owned(), |list, (old, new)| {
                    list.replace(old, new)
                });
            assert_eq!(
                read_json(dir, &path),
                serde_json::from_str::<Value>(&expected).unwrap(),
                "{name}"
            );
            assert_eq!(
                fs::read_to_string(dir.join(format!("{path}.v1.bak"))).unwrap(),
                original
            );
        }
        let players_backup: BTreeMap<_, _> = before
            .iter()
            .filter_map(|(path, contents)| {
                let path = path.strip_prefix("world/players").ok()?;
                Some((path.to_owned(), contents.clone()))
            })
            .collect();
        assert_eq!(snapshot(&dir.join("world/players.v1.bak")), players_backup);

        assert_eq!(
            read_json(dir, "world/players/uuid-migration.json"),
            json!({
                "players": [
                    mapped("Notch", NOTCH),
                    mapped("Player_123456789", PLAYER),
                    mapped("jeb_", JEB),
                    mapped("玩家一号", CJK),
                    mapped("🎮gamer", GAMER),
                ],
                "unmatched": [GHOST],
            })
        );

        let migrated = snapshot(dir);
        migrate_offline_uuids(dir, Path::new("world"), 1).unwrap();
        assert_eq!(snapshot(dir), migrated);
    }

    #[test]
    fn finishes_a_stopped_run_and_keeps_data_already_under_the_new_uuid() {
        let dir = TempDir::new().unwrap();
        let dir = dir.path();
        server_1_2(dir);
        // An earlier run wrote the mapping and Notch's new file, then stopped.
        let earlier: Uuid = "00000000-0000-0000-0000-000000000001".parse().unwrap();
        let alex = json!({ "name": "Alex", "old_uuid": earlier, "new_uuid": earlier });
        write(
            dir,
            "world/players/uuid-migration.json",
            json!({ "players": [alex], "unmatched": [] })
                .to_string()
                .as_bytes(),
        );
        write(
            dir,
            &format!("world/players/data/{}.dat", NOTCH.1),
            &player_data(NOTCH.1, 1),
        );
        // Player_123456789 has already played under the new UUID.
        write(
            dir,
            &format!("world/players/data/{}.dat", PLAYER.1),
            &player_data(PLAYER.1, 50),
        );

        migrate_offline_uuids(dir, Path::new("world"), 1).unwrap();

        assert!(
            !dir.join(format!("world/players/data/{}.dat", NOTCH.0))
                .exists()
        );
        assert_eq!(read_player_data(dir, NOTCH.1).get_int("XpLevel"), Some(1));
        assert_eq!(read_player_data(dir, PLAYER.0).get_int("XpLevel"), Some(2));
        assert_eq!(read_player_data(dir, PLAYER.1).get_int("XpLevel"), Some(50));
        assert_eq!(
            read_json(dir, "world/players/uuid-migration.json")["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|player| player["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "Alex",
                "Notch",
                "Player_123456789",
                "jeb_",
                "玩家一号",
                "🎮gamer"
            ]
        );
    }
}
