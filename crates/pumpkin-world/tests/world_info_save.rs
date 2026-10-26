#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use pumpkin_util::world_seed::Seed;
use pumpkin_world::world_info::{
    LevelData, WorldInfoError, WorldInfoReader, WorldInfoWriter,
    anvil::{AnvilLevelInfo, LEVEL_DAT_BACKUP_FILE_NAME, LEVEL_DAT_FILE_NAME},
};
use tempfile::TempDir;

fn save(level_folder: &Path, level_name: &str) {
    let mut data = LevelData::default(Seed(3510));
    data.level_name = level_name.to_string();
    AnvilLevelInfo
        .write_world_info(&data, level_folder)
        .unwrap();
}

fn loaded_level_name(level_folder: &Path) -> String {
    AnvilLevelInfo
        .read_world_info(level_folder)
        .unwrap()
        .level_name
}

fn level_file_names(level_folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(level_folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("level"))
        .collect();
    names.sort();
    names
}

#[test]
fn write_world_info_reports_backup_failure_and_keeps_previous_level_dat() {
    let temp_dir = TempDir::new().unwrap();

    let mut original = LevelData::default(Seed(3510));
    original.level_name = "Original World".to_string();
    AnvilLevelInfo
        .write_world_info(&original, temp_dir.path())
        .unwrap();

    fs::create_dir(temp_dir.path().join(LEVEL_DAT_BACKUP_FILE_NAME)).unwrap();

    let mut updated = original;
    updated.level_name = "Updated World".to_string();

    let error = AnvilLevelInfo
        .write_world_info(&updated, temp_dir.path())
        .expect_err("a failed level.dat backup must make the save fail");

    assert!(matches!(error, WorldInfoError::IoError(_)));

    let reloaded = AnvilLevelInfo.read_world_info(temp_dir.path()).unwrap();
    assert_eq!(reloaded.level_name, "Original World");
    // The new file written before the failure is gone.
    assert_eq!(
        level_file_names(temp_dir.path()),
        [LEVEL_DAT_FILE_NAME, LEVEL_DAT_BACKUP_FILE_NAME]
    );
}

#[test]
fn save_interrupted_between_renames_loads_level_dat_old() {
    let temp_dir = TempDir::new().unwrap();
    let level_folder = temp_dir.path();
    save(level_folder, "First Save");
    save(level_folder, "Second Save");

    // A crash between the two renames of a save leaves no `level.dat`: it has been moved to
    // `level.dat_old`, and the new data hasn't replaced it yet.
    fs::rename(
        level_folder.join(LEVEL_DAT_FILE_NAME),
        level_folder.join(LEVEL_DAT_BACKUP_FILE_NAME),
    )
    .unwrap();

    assert_eq!(loaded_level_name(level_folder), "Second Save");
    assert_eq!(level_file_names(level_folder), [LEVEL_DAT_FILE_NAME]);
}

#[test]
fn broken_level_dat_loads_level_dat_old() {
    let temp_dir = TempDir::new().unwrap();
    let level_folder = temp_dir.path();
    save(level_folder, "First Save");
    save(level_folder, "Second Save");

    // A `level.dat` cut short, as a power loss after an unsynced save can leave it.
    let path = level_folder.join(LEVEL_DAT_FILE_NAME);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() / 2);
    fs::write(&path, &bytes).unwrap();

    assert_eq!(loaded_level_name(level_folder), "First Save");
    // `level.dat_old` is `level.dat` again, and the broken file is kept.
    let names = level_file_names(level_folder);
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0], LEVEL_DAT_FILE_NAME);
    let time = names[1].strip_prefix("level.dat_corrupted_");
    assert_eq!(time.map(str::len), Some("2026-10-26_17-35-17".len()));
    assert_eq!(fs::read(level_folder.join(&names[1])).unwrap(), bytes);

    save(level_folder, "Third Save");
    assert_eq!(loaded_level_name(level_folder), "Third Save");
}

#[test]
fn broken_level_dat_without_level_dat_old_is_not_a_new_world() {
    let temp_dir = TempDir::new().unwrap();
    let level_folder = temp_dir.path();
    save(level_folder, "First Save");

    let path = level_folder.join(LEVEL_DAT_FILE_NAME);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() / 2);
    fs::write(&path, &bytes).unwrap();

    let error = AnvilLevelInfo.read_world_info(level_folder).unwrap_err();
    assert!(matches!(error, WorldInfoError::DeserializationError(_)));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
