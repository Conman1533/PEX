//! Saving files without losing the previous copy, ported from vanilla `Util` and
//! `FileNameDateFormatter`. Used by the player data and `level.dat` saves.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use time::OffsetDateTime;
use tracing::{debug, warn};

/// How many times vanilla `Util.runWithRetries` tries each step of `Util.safeReplaceFile`.
const SAFE_REPLACE_ATTEMPTS: u32 = 10;

/// Saves `bytes` as `target` and keeps the previous `target` as `backup`.
///
/// The bytes are written to a new file `<prefix><random><suffix>` in `dir` and synced, like
/// vanilla `Files.createTempFile` followed by `NbtIo.writeCompressed`, which opens the file with
/// `SYNC`. The new file is readable only by its owner, like vanilla's. It then replaces `target`
/// through [`safe_replace_file`]. Unlike vanilla, the new file is removed again if the save fails
/// before it is moved into place.
pub fn write_and_replace(
    dir: &Path,
    prefix: &str,
    suffix: &str,
    bytes: &[u8],
    target: &Path,
    backup: &Path,
) -> io::Result<()> {
    let mut new_file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile_in(dir)?;
    new_file.write_all(bytes)?;
    new_file.as_file().sync_data()?;
    let mut new_path = new_file.into_temp_path();

    safe_replace_file(target, &new_path, backup)?;
    // The new file is `target` now.
    new_path.disable_cleanup(true);
    Ok(())
}

/// Replaces `target` with `new` and keeps the previous `target` as `backup`, like vanilla
/// `Util.safeReplaceFile`. If `new` can't be moved into place, the backup is moved back.
fn safe_replace_file(target: &Path, new: &Path, backup: &Path) -> io::Result<()> {
    safe_replace_or_move_file(target, new, backup, false)
}

/// Moves `target` to `backup` and `new` to `target`, like vanilla `Util.safeReplaceOrMoveFile`.
/// Unless `no_rollback` is set, `backup` is moved back when `new` can't be moved into place.
///
/// Vanilla deletes `backup` and `target` before moving onto them because Java's `Files.move`
/// doesn't replace files; `fs::rename` replaces them atomically.
pub fn safe_replace_or_move_file(
    target: &Path,
    new: &Path,
    backup: &Path,
    no_rollback: bool,
) -> io::Result<()> {
    let backed_up = target.exists();
    if backed_up {
        rename_with_retries(target, backup)?;
    }
    rename_with_retries(new, target).inspect_err(|_| {
        if backed_up
            && !no_rollback
            && let Err(e) = rename_with_retries(backup, target)
        {
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

/// The current time as vanilla `FileNameDateFormatter` puts it in file names
/// (`2026-10-26_17-35-17`), in UTC instead of the local time zone, which the `time` crate can't
/// read in a multi-threaded process on Linux.
pub fn file_name_date_now() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[cfg(test)]
mod tests {
    use super::safe_replace_file;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn failed_replace_restores_target() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("level.dat");
        let backup = dir.path().join("level.dat_old");
        fs::write(&target, "previous").unwrap();

        // A new file that isn't there makes the second rename fail after `target` has been moved
        // to `backup`, also for root.
        let missing = dir.path().join("level123.dat");
        assert!(safe_replace_file(&target, &missing, &backup).is_err());

        assert_eq!(fs::read_to_string(&target).unwrap(), "previous");
        assert!(!backup.exists());
    }
}
