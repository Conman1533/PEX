use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag_to_bytes};
use std::fs::{self, File, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use time::OffsetDateTime;
use tracing::{debug, error, warn};
use uuid::Uuid;

const PLAYER_DATA_SUFFIX: &str = ".dat";
const PLAYER_DATA_OLD_SUFFIX: &str = ".dat_old";

/// How many times vanilla `Util.runWithRetries` tries each step of `Util.safeReplaceFile`.
const SAFE_REPLACE_ATTEMPTS: u32 = 10;

/// Manages the storage and retrieval of player data from disk and memory cache.
///
/// This struct provides functions to load and save player data to/from NBT files,
/// with a memory cache to handle player disconnections temporarily.
pub struct PlayerDataStorage {
    /// Path to the directory where player data is stored
    data_path: PathBuf,
    /// Whether player data saving is enabled
    save_enabled: bool,
    /// Keeps two saves of the same player from interleaving their renames. Autosaves run on
    /// Rayon, while disconnect and shutdown saves don't.
    replace_lock: Mutex<()>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerDataError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("NBT error: {0}")]
    Nbt(String),
}

impl PlayerDataStorage {
    /// Creates a new `PlayerDataStorage` with the specified data path and cache expiration time.
    pub fn new(data_path: impl Into<PathBuf>, enabled: bool) -> Self {
        let path = data_path.into();
        if !path.exists()
            && let Err(e) = create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }

        Self {
            data_path: path,
            save_enabled: enabled,
            replace_lock: Mutex::new(()),
        }
    }

    #[must_use]
    pub const fn get_data_path(&self) -> &PathBuf {
        &self.data_path
    }

    #[must_use]
    pub const fn is_save_enabled(&self) -> bool {
        self.save_enabled
    }

    pub const fn set_save_enabled(&mut self, enabled: bool) {
        self.save_enabled = enabled;
    }

    /// Returns the path for a player's data file based on their UUID.
    #[must_use]
    pub fn get_player_data_path(&self, uuid: &Uuid) -> PathBuf {
        self.player_file(uuid, PLAYER_DATA_SUFFIX)
    }

    fn player_file(&self, uuid: &Uuid, suffix: &str) -> PathBuf {
        self.data_path.join(format!("{uuid}{suffix}"))
    }

    /// Loads a player's data from `<uuid>.dat`, or from `<uuid>.dat_old` when `<uuid>.dat` is
    /// missing or can't be read, like vanilla `PlayerDataStorage.load`.
    ///
    /// Returns `false` and an empty compound when the player has no data yet.
    pub fn load_player_data(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        // If player data saving is disabled, return empty data
        if !self.is_save_enabled() {
            return Ok((false, NbtCompound::new()));
        }

        let error = match self.load(uuid, PLAYER_DATA_SUFFIX) {
            Ok(Some(nbt)) => {
                debug!("Loaded player data for {uuid} from disk");
                return Ok((true, nbt));
            }
            Ok(None) => None,
            Err(e) => {
                self.backup(uuid, PLAYER_DATA_SUFFIX);
                Some(e)
            }
        };

        match self.load(uuid, PLAYER_DATA_OLD_SUFFIX) {
            Ok(Some(nbt)) => {
                warn!(
                    "Loaded player data for {uuid} from {}",
                    self.player_file(uuid, PLAYER_DATA_OLD_SUFFIX).display()
                );
                Ok((true, nbt))
            }
            Ok(None) => error.map_or_else(
                || {
                    debug!("No player data file found for {uuid}");
                    Ok((false, NbtCompound::new()))
                },
                Err,
            ),
            Err(e) => Err(error.unwrap_or(e)),
        }
    }

    /// Reads `<uuid><suffix>`, or returns `None` when there is no such file.
    fn load(&self, uuid: &Uuid, suffix: &str) -> Result<Option<NbtCompound>, PlayerDataError> {
        let path = self.player_file(uuid, suffix);
        if !path.is_file() {
            return Ok(None);
        }

        File::open(&path)
            .map_err(PlayerDataError::Io)
            .and_then(|file| {
                read_gzip_compound_tag(file).map_err(|e| PlayerDataError::Nbt(e.to_string()))
            })
            .map(Some)
            .inspect_err(|e| warn!("Failed to load player data from {}: {e}", path.display()))
    }

    /// Copies an unreadable `<uuid><suffix>` to `<uuid>_corrupted_<time><suffix>`, because the
    /// next save replaces it (vanilla `PlayerDataStorage.backup`).
    fn backup(&self, uuid: &Uuid, suffix: &str) {
        let path = self.player_file(uuid, suffix);
        // Vanilla `FileNameDateFormatter`, in UTC instead of the local time zone.
        let now = OffsetDateTime::now_utc();
        let backup_path = self.data_path.join(format!(
            "{uuid}_corrupted_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}{suffix}",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        ));
        if let Err(e) = fs::copy(&path, &backup_path) {
            warn!(
                "Failed to copy {} to {}: {e}",
                path.display(),
                backup_path.display()
            );
        }
    }

    /// Saves a player's data like vanilla `PlayerDataStorage.save`.
    ///
    /// The data is written to a new file next to `<uuid>.dat`, which then replaces it, and the
    /// previous `<uuid>.dat` is kept as `<uuid>.dat_old`. A save that stops partway leaves data
    /// that [`Self::load_player_data`] can still load.
    pub fn save_player_data(&self, uuid: &Uuid, data: NbtCompound) -> Result<(), PlayerDataError> {
        // Skip saving if disabled in config
        if !self.is_save_enabled() {
            return Ok(());
        }

        if let Err(e) = create_dir_all(&self.data_path) {
            error!("Failed to create player data directory for {uuid}: {e}");
            return Err(PlayerDataError::Io(e));
        }

        let path = self.get_player_data_path(uuid);
        self.write_and_replace(uuid, &path, data)
            .inspect(|()| debug!("Saved player data for {uuid} to disk"))
            .inspect_err(|e| error!("Failed to save player data to {}: {e}", path.display()))
    }

    fn write_and_replace(
        &self,
        uuid: &Uuid,
        path: &Path,
        data: NbtCompound,
    ) -> Result<(), PlayerDataError> {
        let bytes = write_gzip_compound_tag_to_bytes(data)
            .map_err(|e| PlayerDataError::Nbt(e.to_string()))?;

        // Like vanilla `Files.createTempFile`, this is `<uuid>-<random>.dat` and readable only by
        // its owner. Unlike vanilla, it is removed again if the save fails.
        let mut new_file = tempfile::Builder::new()
            .prefix(&format!("{uuid}-"))
            .suffix(PLAYER_DATA_SUFFIX)
            .tempfile_in(&self.data_path)?;
        new_file.write_all(&bytes)?;
        // Vanilla `NbtIo.writeCompressed` opens the file with `SYNC`.
        new_file.as_file().sync_data()?;
        let mut new_path = new_file.into_temp_path();

        let _guard = self
            .replace_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        safe_replace_file(
            path,
            &new_path,
            &self.player_file(uuid, PLAYER_DATA_OLD_SUFFIX),
        )?;
        // The new file is `<uuid>.dat` now.
        new_path.disable_cleanup(true);
        Ok(())
    }
}

/// Replaces `target` with `new` and keeps the previous `target` as `backup`, like vanilla
/// `Util.safeReplaceFile`. If `new` can't be moved into place, the backup is moved back.
///
/// Vanilla deletes `backup` and `target` before moving onto them because Java's `Files.move`
/// doesn't replace files; `fs::rename` replaces them atomically.
fn safe_replace_file(target: &Path, new: &Path, backup: &Path) -> io::Result<()> {
    let backed_up = target.exists();
    if backed_up {
        rename_with_retries(target, backup)?;
    }
    rename_with_retries(new, target).inspect_err(|_| {
        if backed_up && let Err(e) = rename_with_retries(backup, target) {
            warn!(
                "Failed to restore {} from {}: {e}",
                target.display(),
                backup.display()
            );
        }
    })
}

/// `fs::rename`, tried up to [`SAFE_REPLACE_ATTEMPTS`] times like vanilla `Util.runWithRetries`.
fn rename_with_retries(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < SAFE_REPLACE_ATTEMPTS => {
                debug!(
                    "Failed to rename {} to {}, retrying {attempt}/{SAFE_REPLACE_ATTEMPTS}: {e}",
                    from.display(),
                    to.display()
                );
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PLAYER_DATA_OLD_SUFFIX, PlayerDataStorage};
    use pumpkin_nbt::compound::NbtCompound;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn data(score: i32) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_int("Score", score);
        nbt
    }

    fn loaded_score(storage: &PlayerDataStorage, uuid: &Uuid) -> i32 {
        let (found, nbt) = storage.load_player_data(uuid).unwrap();
        assert!(found, "no player data found");
        nbt.get_int("Score").unwrap()
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn interrupted_save_keeps_old_data_loadable() {
        let dir = TempDir::new().unwrap();
        let storage = PlayerDataStorage::new(dir.path(), true);
        let uuid = Uuid::new_v4();
        storage.save_player_data(&uuid, data(1)).unwrap();

        // A directory where `<uuid>.dat_old` goes stops the save after the new data is written
        // and before it replaces `<uuid>.dat`, also for root.
        let old_path = dir.path().join(format!("{uuid}{PLAYER_DATA_OLD_SUFFIX}"));
        fs::create_dir_all(old_path.join("blocker")).unwrap();
        assert!(storage.save_player_data(&uuid, data(2)).is_err());

        assert_eq!(loaded_score(&storage, &uuid), 1);
        assert_eq!(
            file_names(dir.path()),
            [format!("{uuid}.dat"), format!("{uuid}.dat_old")],
            "the failed save left a file behind"
        );

        fs::remove_dir_all(&old_path).unwrap();
        storage.save_player_data(&uuid, data(2)).unwrap();
        assert_eq!(loaded_score(&storage, &uuid), 2);
    }

    #[test]
    fn load_falls_back_to_dat_old() {
        let dir = TempDir::new().unwrap();
        let storage = PlayerDataStorage::new(dir.path(), true);
        let uuid = Uuid::new_v4();
        storage.save_player_data(&uuid, data(1)).unwrap();
        storage.save_player_data(&uuid, data(2)).unwrap();
        assert_eq!(loaded_score(&storage, &uuid), 2);

        // A `<uuid>.dat` cut short, as a crash during a direct write leaves it.
        let path = storage.get_player_data_path(&uuid);
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() / 2);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(loaded_score(&storage, &uuid), 1);
        let corrupted: Vec<String> = file_names(dir.path())
            .into_iter()
            .filter(|name| name.contains("_corrupted_"))
            .collect();
        assert_eq!(corrupted.len(), 1, "no copy of the broken file");
        let time = corrupted[0]
            .strip_prefix(&format!("{uuid}_corrupted_"))
            .and_then(|rest| rest.strip_suffix(".dat"));
        assert_eq!(time.map(str::len), Some("2026-10-26_17-35-17".len()));
        assert_eq!(fs::read(dir.path().join(&corrupted[0])).unwrap(), bytes);

        // No `<uuid>.dat`, as a crash between the two renames of a save leaves it.
        fs::remove_file(&path).unwrap();
        assert_eq!(loaded_score(&storage, &uuid), 1);
    }
}
