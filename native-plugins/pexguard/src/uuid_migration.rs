//! Moves region owners and members from the offline UUIDs PEX 1.x gave them to vanilla's.
//!
//! On its first start, PEX 2.0 moves offline players to vanilla's UUIDs before plugins load,
//! and lists each player's old and new UUID in the world's `players/uuid-migration.json`.

use crate::region::RegionSet;
use crate::store;
use pumpkin::data::offline_uuid_migration::MAPPING_FILE;
use pumpkin::net::{legacy_offline_uuid, offline_uuid};
use pumpkin_config::OFFLINE_UUID_VERSION;
use pumpkin_config::migration::write_backup;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The directory in the world where the server writes [`MAPPING_FILE`].
const PLAYERS_DIR: &str = "players";

/// The backup of `regions.toml` is named like the server's own backups from this migration.
const BACKUP_VERSION: u32 = OFFLINE_UUID_VERSION - 1;

/// The part of the server's [`MAPPING_FILE`] needed here.
#[derive(Deserialize)]
struct Mapping {
    players: Vec<MappedPlayer>,
}

#[derive(Deserialize)]
struct MappedPlayer {
    old_uuid: Uuid,
    new_uuid: Uuid,
}

/// Gives region owners and members the UUIDs the server moved their players to.
///
/// Each mapping is applied once: a copy of it is kept in the data folder, and later starts with
/// the same mapping change nothing. `regions.toml` is copied to `regions.toml.v1.bak` before it
/// changes. If that or the save fails, `regions` and `regions.toml` stay as they were and the next
/// start tries again.
pub fn migrate(data_folder: &Path, world_dir: &Path, regions: &mut RegionSet) {
    let mapping_path = world_dir.join(PLAYERS_DIR).join(MAPPING_FILE);
    let mapping = match fs::read_to_string(&mapping_path) {
        Ok(mapping) => mapping,
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => return fail(&format!("{}: {err}", mapping_path.display())),
    };
    let applied_path = data_folder.join(MAPPING_FILE);
    if fs::read_to_string(&applied_path).is_ok_and(|applied| applied == mapping) {
        return;
    }
    let new_uuids: HashMap<Uuid, Uuid> = match serde_json::from_str::<Mapping>(&mapping) {
        Ok(mapping) => mapping
            .players
            .into_iter()
            .map(|player| (player.old_uuid, player.new_uuid))
            .collect(),
        Err(err) => return fail(&format!("{}: {err}", mapping_path.display())),
    };

    let mut migrated = regions.clone();
    let moved = move_members(&mut migrated, &new_uuids);
    if moved > 0 {
        match save_with_backup(data_folder, &migrated) {
            Ok(backup) => tracing::info!(
                "PEXGuard: moved {moved} region owner and member entries to vanilla UUIDs; the old regions.toml is saved as {}",
                backup.display()
            ),
            Err(err) => return fail(&err),
        }
        *regions = migrated;
    }
    if let Err(err) = store::write(&applied_path, &mapping) {
        // regions.toml is already migrated, so the next start only retries this.
        tracing::error!("PEXGuard: could not record the UUID migration: {err}");
    }
}

fn fail(err: &str) {
    tracing::error!(
        "PEXGuard: could not move region owners and members to vanilla UUIDs, so regions.toml is unchanged: {err}"
    );
}

/// Copies `regions.toml` to a backup that never overwrites an older one, then saves `regions`.
fn save_with_backup(data_folder: &Path, regions: &RegionSet) -> Result<PathBuf, String> {
    let path = store::path(data_folder);
    let at = |err: std::io::Error| format!("{}: {err}", path.display());
    let original = fs::read_to_string(&path).map_err(at)?;
    let backup = write_backup(&path, &original, BACKUP_VERSION).map_err(at)?;
    store::save(data_folder, regions)?;
    Ok(backup)
}

/// Gives every owner and member the mapping lists their new UUID, and returns how many changed.
///
/// One it doesn't list still moves when their UUID is the PEX 1.x UUID of the name stored with
/// it. The server only gets names from its user cache and player lists, which can have forgotten a
/// player, while a region keeps the name of each member.
fn move_members(regions: &mut RegionSet, new_uuids: &HashMap<Uuid, Uuid>) -> usize {
    let mut moved = 0;
    for region in &mut regions.regions {
        for member in region.owners.iter_mut().chain(&mut region.members) {
            let Ok(uuid) = Uuid::parse_str(&member.uuid) else {
                continue;
            };
            let new_uuid = new_uuids.get(&uuid).copied().or_else(|| {
                (legacy_offline_uuid(&member.name) == uuid).then(|| offline_uuid(&member.name))
            });
            if let Some(new_uuid) = new_uuid {
                member.uuid = new_uuid.to_string();
                moved += 1;
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // Old UUIDs from the PEX 1.2.0 release, new ones from the vanilla 26.3 server.
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
    /// A Mojang UUID, as an online-mode server stores it.
    const JEB: &str = "853c80ef-3c37-49fd-aa49-938b674adae6";

    /// `regions.toml` in the format `PEXGuard` 1.2.0 writes.
    const REGIONS: &str = r#"[[regions]]
name = "home"
world = "world"
min = [
    80,
    190,
    80,
]
max = [
    140,
    220,
    120,
]
priority = 0

[[regions.owners]]
uuid = "ff4af744-2839-cc96-4605-0dd2214a30c1"
name = "Notch"

[[regions.members]]
uuid = "fe3625b2-f47a-1462-c456-19d51eedef7d"
name = "Player_123456789"

[[regions.members]]
uuid = "e893d087-028b-377e-080a-c57e028f96e9"
name = "玩家一号"

[[regions.members]]
uuid = "853c80ef-3c37-49fd-aa49-938b674adae6"
name = "jeb_"

[regions.flags]
greeting = "Welcome home"

[[regions]]
name = "shop"
world = "world"
min = [
    0,
    60,
    0,
]
max = [
    9,
    80,
    9,
]
priority = 5
members = []

[[regions.owners]]
uuid = "45399c0f-6260-6f57-26d6-1a2841727956"
name = "🎮gamer"

[regions.flags]
pvp = "deny"

[globals.world]
pvp = "deny"
"#;

    /// The server's mapping as `migrate_offline_uuids` writes it. 玩家一号 is unmatched, as when
    /// their user cache entry is gone.
    const MAPPING: &str = r#"{
  "players": [
    {
      "name": "Notch",
      "old_uuid": "ff4af744-2839-cc96-4605-0dd2214a30c1",
      "new_uuid": "b50ad385-829d-3141-a216-7e7d7539ba7f"
    },
    {
      "name": "Player_123456789",
      "old_uuid": "fe3625b2-f47a-1462-c456-19d51eedef7d",
      "new_uuid": "c0d79c73-221a-32dc-b983-3cfa4a8a971a"
    },
    {
      "name": "🎮gamer",
      "old_uuid": "45399c0f-6260-6f57-26d6-1a2841727956",
      "new_uuid": "501e0b86-57fc-33e9-9853-97f55e79973f"
    }
  ],
  "unmatched": [
    "e893d087-028b-377e-080a-c57e028f96e9"
  ]
}"#;

    /// A server directory with `REGIONS`, and `mapping` if given. Returns the data folder and the
    /// world.
    fn server(dir: &Path, mapping: Option<&str>) -> (PathBuf, PathBuf) {
        let data_folder = dir.join("plugins/data/pexguard");
        let world = dir.join("world");
        fs::create_dir_all(&data_folder).unwrap();
        fs::create_dir_all(world.join(PLAYERS_DIR)).unwrap();
        fs::write(store::path(&data_folder), REGIONS).unwrap();
        if let Some(mapping) = mapping {
            fs::write(world.join(PLAYERS_DIR).join(MAPPING_FILE), mapping).unwrap();
        }
        (data_folder, world)
    }

    fn files(dir: &Path) -> BTreeMap<String, String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let text = fs::read_to_string(entry.path()).unwrap_or_default();
                (entry.file_name().to_string_lossy().into_owned(), text)
            })
            .collect()
    }

    fn migrated_regions() -> String {
        [NOTCH, PLAYER, CJK, GAMER]
            .iter()
            .fold(REGIONS.to_owned(), |text, (old, new)| {
                text.replace(old, new)
            })
    }

    #[test]
    fn moves_owners_and_members_once() {
        let dir = tempfile::tempdir().unwrap();
        let (data_folder, world) = server(dir.path(), Some(MAPPING));
        let mut regions = store::load(&data_folder).unwrap();
        migrate(&data_folder, &world, &mut regions);

        // Only the UUIDs change; the Mojang one stays.
        let expected = migrated_regions();
        assert_eq!(
            files(&data_folder),
            BTreeMap::from([
                ("regions.toml".to_owned(), expected.clone()),
                ("regions.toml.v1.bak".to_owned(), REGIONS.to_owned()),
                (MAPPING_FILE.to_owned(), MAPPING.to_owned()),
            ])
        );
        assert_eq!(toml::to_string_pretty(&regions).unwrap(), expected);
        let home = regions.get("home").unwrap();
        for member in [NOTCH.1, PLAYER.1, CJK.1, JEB] {
            assert!(home.is_member(member), "{member}");
        }

        // A second start with the same mapping changes nothing.
        let before = files(&data_folder);
        let mut regions = store::load(&data_folder).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert_eq!(files(&data_folder), before);

        // Not even when the old UUIDs are back, until the plugin's copy of the mapping is removed.
        fs::write(store::path(&data_folder), REGIONS).unwrap();
        let mut regions = store::load(&data_folder).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert_eq!(
            fs::read_to_string(store::path(&data_folder)).unwrap(),
            REGIONS
        );
        fs::remove_file(data_folder.join(MAPPING_FILE)).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert_eq!(
            fs::read_to_string(store::path(&data_folder)).unwrap(),
            expected
        );
        assert!(data_folder.join("regions.toml.v1 (1).bak").exists());
    }

    #[test]
    fn failure_keeps_regions_toml_and_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        // No mapping: a server that never ran PEX 1.x offline.
        let (data_folder, world) = server(dir.path(), None);
        let mut regions = store::load(&data_folder).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert_eq!(files(&data_folder).len(), 1);

        fs::write(world.join(PLAYERS_DIR).join(MAPPING_FILE), "{").unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert_eq!(files(&data_folder).len(), 1);

        fs::write(world.join(PLAYERS_DIR).join(MAPPING_FILE), MAPPING).unwrap();
        let blocker = data_folder.join("regions.toml.tmp");
        fs::create_dir(&blocker).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert!(regions.get("home").unwrap().is_member(NOTCH.0));
        assert_eq!(
            fs::read_to_string(store::path(&data_folder)).unwrap(),
            REGIONS
        );
        assert_eq!(
            fs::read_to_string(data_folder.join("regions.toml.v1.bak")).unwrap(),
            REGIONS
        );
        assert!(!data_folder.join(MAPPING_FILE).exists());

        // The next start finishes it.
        fs::remove_dir(&blocker).unwrap();
        let mut regions = store::load(&data_folder).unwrap();
        migrate(&data_folder, &world, &mut regions);
        assert!(regions.get("home").unwrap().is_member(NOTCH.1));
        assert_eq!(
            fs::read_to_string(store::path(&data_folder)).unwrap(),
            migrated_regions()
        );
        assert_eq!(
            fs::read_to_string(data_folder.join(MAPPING_FILE)).unwrap(),
            MAPPING
        );
    }
}
