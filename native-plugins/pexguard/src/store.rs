//! Loading and atomically saving `regions.toml`.

use crate::region::RegionSet;
use std::fs;
use std::path::{Path, PathBuf};

const FILE: &str = "regions.toml";

pub fn path(data_folder: &Path) -> PathBuf {
    data_folder.join(FILE)
}

/// Loads the regions, or an empty set if the file is missing. A broken file is kept on disk
/// and reported, so a typo can't silently wipe every region on the next save.
pub fn load(data_folder: &Path) -> Result<RegionSet, String> {
    let path = path(data_folder);
    fs::read_to_string(&path).map_or_else(
        |_| Ok(RegionSet::default()),
        |text| toml::from_str(&text).map_err(|err| format!("{}: {err}", path.display())),
    )
}

pub fn save(data_folder: &Path, regions: &RegionSet) -> Result<(), String> {
    let text = toml::to_string_pretty(regions).map_err(|err| err.to_string())?;
    write(&path(data_folder), &text)
}

/// Writes to a temporary file and renames it over the old one.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).map_err(|err| format!("{}: {err}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|err| format!("{}: {err}", path.display()))
}
