use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError, RwLock},
};

use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

const DATA_FOLDER: &str = "data/";

/// Files in `data/` that exist but failed to load, with the error. Their lists start out empty,
/// so saving one would replace the admin's entries; saves skip these files until they load.
static NOT_LOADED: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());

pub mod op;

pub mod advancement_data;
pub mod banlist_serializer;
pub mod banned_ip;
pub mod banned_player;
pub mod datapack;
pub mod player_server;
pub mod usercache;
pub mod whitelist;

#[derive(Default)]
pub struct VanillaData {
    pub banned_ip_list: RwLock<banned_ip::BannedIpList>,
    pub banned_player_list: RwLock<banned_player::BannedPlayerList>,
    pub operator_config: RwLock<op::OperatorConfig>,
    pub user_cache: RwLock<usercache::UserCache>,
    pub whitelist_config: RwLock<whitelist::WhitelistConfig>,
}

impl VanillaData {
    #[must_use]
    pub fn load() -> Self {
        Self {
            banned_ip_list: RwLock::new(banned_ip::BannedIpList::load()),
            banned_player_list: RwLock::new(banned_player::BannedPlayerList::load()),
            operator_config: RwLock::new(op::OperatorConfig::load()),
            user_cache: RwLock::new(usercache::UserCache::load()),
            whitelist_config: RwLock::new(whitelist::WhitelistConfig::load()),
        }
    }
}

/// Reads the JSON list at `path`, or `None` if there is no file.
fn read_list<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> io::Result<Option<T>> {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    // Gson reads an empty file as null, which vanilla `StoredUserList.load` takes as an empty list.
    if json.trim_ascii().is_empty() {
        return Ok(Some(T::default()));
    }
    Ok(Some(serde_json::from_str(&json)?))
}

/// Loads the JSON list at `path`, or `None` if there is no file. A file that can't be read or
/// parsed gives an empty list and is kept: [`may_save`] refuses to save over it until it loads.
fn load_list<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let result = read_list(path);
    let mut not_loaded = NOT_LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    match result {
        Ok(list) => {
            not_loaded.remove(path);
            list
        }
        Err(err) => {
            error!(
                "Couldn't load {}, so it is kept and changes to it won't be saved: {err}",
                path.display()
            );
            not_loaded.insert(path.to_owned(), err.to_string());
            Some(T::default())
        }
    }
}

/// Whether the list at `path` may be saved. It may not while the file there failed to load, and
/// that is logged.
fn may_save(path: &Path) -> bool {
    let err = NOT_LOADED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(path)
        .cloned();
    let Some(err) = err else {
        return true;
    };
    warn!(
        "Didn't save {} because it failed to load: {err}",
        path.display()
    );
    false
}

pub trait LoadJSONConfiguration {
    #[must_use]
    fn load() -> Self
    where
        Self: Sized + Default + Serialize + for<'de> Deserialize<'de>,
    {
        let exe_dir = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let data_dir = exe_dir.join(DATA_FOLDER);
        if !data_dir.exists() {
            debug!("creating new data root folder");
            let _ = fs::create_dir(&data_dir);
        }
        Self::load_from(&data_dir)
    }

    /// Loads the list from its file in `data_dir`, and writes an empty one if there is none.
    ///
    /// A file that can't be read or parsed gives an empty list, and saves leave it alone until
    /// it loads.
    #[must_use]
    fn load_from(data_dir: &Path) -> Self
    where
        Self: Sized + Default + Serialize + for<'de> Deserialize<'de>,
    {
        let path = data_dir.join(Self::get_path());
        let config = load_list(&path).unwrap_or_else(|| {
            let content = Self::default();

            if let Ok(json_str) = serde_json::to_string_pretty(&content) {
                let _ = fs::write(&path, json_str);
            }

            content
        });

        config.validate();
        config
    }

    fn get_path() -> &'static Path;

    fn validate(&self);
}

pub trait SaveJSONConfiguration: LoadJSONConfiguration {
    fn save(&self)
    where
        Self: Sized + Default + Serialize + for<'de> Deserialize<'de>,
    {
        let exe_dir = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let data_dir = exe_dir.join(DATA_FOLDER);
        if !data_dir.exists() {
            debug!("creating new data root folder");
            let _ = fs::create_dir(&data_dir);
        }
        self.save_to(&data_dir);
    }

    /// Saves the list to its file in `data_dir`, unless that file failed to load.
    fn save_to(&self, data_dir: &Path)
    where
        Self: Sized + Default + Serialize + for<'de> Deserialize<'de>,
    {
        let path = data_dir.join(Self::get_path());
        if !may_save(&path) {
            return;
        }

        let content = match serde_json::to_string_pretty(self) {
            Ok(content) => content,
            Err(err) => {
                warn!(
                    "Couldn't serialize operator data config to {}. Reason: {err}",
                    path.display()
                );
                return;
            }
        };

        if let Err(err) = std::fs::write(&path, content) {
            warn!(
                "Couldn't write operator config to {}. Reason: {err}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pumpkin_config::op::Op;
    use pumpkin_util::PermissionLvl;
    use tempfile::TempDir;

    use super::op::OperatorConfig;
    use super::{LoadJSONConfiguration, SaveJSONConfiguration};

    /// Two operators with the comma between them missing.
    const BROKEN_OPS: &str = r#"[
  {
    "uuid": "b50ad385-829d-3141-a216-7e7d7539ba7f",
    "name": "Notch",
    "level": 4,
    "bypassesPlayerLimit": false
  }
  {
    "uuid": "a762f560-4fce-3236-812a-b80efff0b62b",
    "name": "jeb_",
    "level": 4,
    "bypassesPlayerLimit": false
  }
]
"#;

    fn alex() -> Op {
        let uuid = "00000000-0000-0000-0000-000000000001".parse().unwrap();
        Op::new(uuid, "Alex".to_owned(), PermissionLvl::Four, false)
    }

    #[test]
    fn keeps_a_list_that_failed_to_load_until_it_loads() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ops.json");
        fs::write(&path, BROKEN_OPS).unwrap();

        let mut config = OperatorConfig::load_from(dir.path());
        assert!(config.ops.is_empty());
        config.ops.push(alex());
        config.save_to(dir.path());
        assert_eq!(fs::read_to_string(&path).unwrap(), BROKEN_OPS);

        // Once the admin's fix loads, saves go through again.
        fs::write(&path, BROKEN_OPS.replace("  }\n  {", "  },\n  {")).unwrap();
        let mut config = OperatorConfig::load_from(dir.path());
        assert_eq!(config.ops.len(), 2);
        config.ops.push(alex());
        config.save_to(dir.path());
        assert_eq!(OperatorConfig::load_from(dir.path()).ops.len(), 3);
    }

    #[test]
    fn saves_over_an_empty_list_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ops.json");
        fs::write(&path, "\n").unwrap();

        let mut config = OperatorConfig::load_from(dir.path());
        assert!(config.ops.is_empty());
        config.ops.push(alex());
        config.save_to(dir.path());
        assert_eq!(OperatorConfig::load_from(dir.path()).ops.len(), 1);
    }
}
