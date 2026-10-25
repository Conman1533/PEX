use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

/// The key that records which layout a config file follows.
pub const CONFIG_VERSION_KEY: &str = "config_version";

/// The version of a file without [`CONFIG_VERSION_KEY`]: the layout PEX 1.0 to 1.2 wrote.
pub const UNVERSIONED_CONFIG_VERSION: u32 = 1;

/// One step that brings a config file up to `version`.
pub struct Migration {
    /// The `config_version` a file has after this step.
    pub version: u32,
    /// Changes the file and returns one line for each change it made.
    pub apply: fn(&mut DocumentMut) -> Vec<String>,
}

/// The version files are brought up to: that of the last migration.
#[must_use]
pub const fn latest_version(migrations: &[Migration]) -> u32 {
    match migrations.last() {
        Some(migration) => migration.version,
        None => UNVERSIONED_CONFIG_VERSION,
    }
}

/// Whether every migration is newer than an unversioned file and none is older than the one
/// before it, which vanilla `FileFixerUpper.Builder.addFixer` also requires.
#[must_use]
pub const fn migrations_are_ordered(mut migrations: &[Migration]) -> bool {
    let mut lowest = UNVERSIONED_CONFIG_VERSION + 1;
    while let [migration, rest @ ..] = migrations {
        if migration.version < lowest {
            return false;
        }
        lowest = migration.version;
        migrations = rest;
    }
    true
}

/// The file's `config_version`, or [`UNVERSIONED_CONFIG_VERSION`] when it has none (like vanilla
/// `NbtUtils.getDataVersion` with a default). `None` if the key holds anything but a `u32`.
#[must_use]
pub(crate) fn file_version(document: &DocumentMut) -> Option<u32> {
    document
        .get(CONFIG_VERSION_KEY)
        .map_or(Some(UNVERSIONED_CONFIG_VERSION), |item| {
            item.as_integer()
                .and_then(|version| u32::try_from(version).ok())
        })
}

/// Brings `document` from `version` up to [`latest_version`]: runs the newer migrations in order
/// (vanilla `FileFixerUpper.getApplicableFixers` and `applyFileFixers`), adds the keys it lacks
/// from `defaults` and records the new version. Returns one line per change.
#[must_use]
pub(crate) fn migrate(
    document: &mut DocumentMut,
    version: u32,
    migrations: &[Migration],
    defaults: &DocumentMut,
) -> Vec<String> {
    let mut changes: Vec<String> = migrations
        .iter()
        .filter(|migration| migration.version > version)
        .flat_map(|migration| (migration.apply)(document))
        .collect();
    changes.extend(add_missing_keys(document, defaults));
    changes.extend(set_config_version(document, latest_version(migrations)));
    changes
}

/// Adds every key of `defaults` that `document` lacks, and returns one line per added key.
#[must_use]
pub(crate) fn add_missing_keys(document: &mut DocumentMut, defaults: &DocumentMut) -> Vec<String> {
    let mut changes = Vec::new();
    add_missing_table_keys(
        document.as_table_mut(),
        defaults.as_table(),
        "",
        &mut changes,
    );
    changes
}

fn add_missing_table_keys(
    table: &mut dyn TableLike,
    defaults: &dyn TableLike,
    path: &str,
    changes: &mut Vec<String>,
) {
    for (key, default) in defaults.iter() {
        let key_path = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        if let Some(item) = table.get_mut(key) {
            let changes_before = changes.len();
            if let (Some(table), Some(defaults)) =
                (item.as_table_like_mut(), default.as_table_like())
            {
                add_missing_table_keys(table, defaults, &key_path, changes);
            }
            // Keys added to `{ ... }` keep the spacing they had in the defaults, so respace it.
            if changes.len() > changes_before
                && let Some(inline) = item.as_inline_table_mut()
            {
                inline.fmt();
            }
            continue;
        }
        let mut item = default.clone();
        clear_positions(&mut item);
        changes.push(item.as_value().map_or_else(
            || format!("added [{key_path}] with default values"),
            |value| format!("added {key_path} = {}", value.to_string().trim()),
        ));
        table.insert(key, item);
    }
}

/// Forgets where a copied table stood in the defaults, so it is written after its new siblings.
fn clear_positions(item: &mut Item) {
    match item {
        Item::Table(table) => clear_table_positions(table),
        Item::ArrayOfTables(tables) => tables.iter_mut().for_each(clear_table_positions),
        Item::None | Item::Value(_) => {}
    }
}

fn clear_table_positions(table: &mut Table) {
    table.set_position(None);
    for (_, item) in table.iter_mut() {
        clear_positions(item);
    }
}

/// Sets the file's `config_version`, adding it as the first key when it is missing. Returns the
/// change line, or `None` if the file already had that version.
pub(crate) fn set_config_version(document: &mut DocumentMut, version: u32) -> Option<String> {
    let version = i64::from(version);
    match document
        .get_mut(CONFIG_VERSION_KEY)
        .and_then(Item::as_value_mut)
    {
        Some(value) if value.as_integer() == Some(version) => return None,
        Some(value) => {
            // Keep a comment the admin put after the number.
            let decor = value.decor().clone();
            *value = Value::from(version);
            *value.decor_mut() = decor;
        }
        None => {
            document.insert(CONFIG_VERSION_KEY, toml_edit::value(version));
            document.sort_values_by(|key, _, other, _| {
                (other.get() == CONFIG_VERSION_KEY).cmp(&(key.get() == CONFIG_VERSION_KEY))
            });
        }
    }
    Some(format!("set {CONFIG_VERSION_KEY} to {version}"))
}

/// The name of the `copy`th backup of `path` at `version`: `<file name>.v<version>.bak`, then
/// `<file name>.v<version> (<copy>).bak`, as in vanilla `FileUtil.findAvailableName`.
fn backup_path(path: &Path, version: u32, copy: u32) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(if copy == 0 {
        format!("{file_name}.v{version}.bak")
    } else {
        format!("{file_name}.v{version} ({copy}).bak")
    })
}

/// Copies `contents` to `<file name>.v<version>.bak` next to `path` and returns the copy's path.
/// An existing backup is never overwritten: a taken name gets a counter.
pub fn write_backup(path: &Path, contents: &str, version: u32) -> io::Result<PathBuf> {
    let mut copy = 0u32;
    loop {
        let backup = backup_path(path, version, copy);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(mut file) => {
                return match file.write_all(contents.as_bytes()) {
                    Ok(()) => Ok(backup),
                    Err(err) => {
                        // Don't leave a partial copy that looks like a good backup.
                        let _ = fs::remove_file(&backup);
                        Err(err)
                    }
                };
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => copy += 1,
            Err(err) => return Err(err),
        }
    }
}

/// Copies the directory `path` and everything in it to `<name>.v<version>.bak` next to it, like
/// [`write_backup`], and returns the copy's path.
pub fn backup_dir(path: &Path, version: u32) -> io::Result<PathBuf> {
    let mut copy = 0u32;
    loop {
        let backup = backup_path(path, version, copy);
        match fs::create_dir(&backup) {
            Ok(()) => {
                return match copy_dir_contents(path, &backup) {
                    Ok(()) => Ok(backup),
                    Err(err) => {
                        let _ = fs::remove_dir_all(&backup);
                        Err(err)
                    }
                };
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => copy += 1,
            Err(err) => return Err(err),
        }
    }
}

fn copy_dir_contents(from: &Path, to: &Path) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir(&target)?;
            copy_dir_contents(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use toml_edit::DocumentMut;

    use super::{Migration, migrate};

    fn times_ten(document: &mut DocumentMut) -> Vec<String> {
        let x = document["x"].as_integer().unwrap();
        document["x"] = toml_edit::value(x * 10);
        vec!["multiplied x by 10".to_owned()]
    }

    fn plus_one(document: &mut DocumentMut) -> Vec<String> {
        let x = document["x"].as_integer().unwrap();
        document["x"] = toml_edit::value(x + 1);
        vec!["added 1 to x".to_owned()]
    }

    const MIGRATIONS: &[Migration] = &[
        Migration {
            version: 2,
            apply: times_ten,
        },
        Migration {
            version: 3,
            apply: plus_one,
        },
    ];

    #[test]
    fn runs_only_newer_migrations_in_order() {
        let defaults = DocumentMut::new();

        let mut unversioned: DocumentMut = "x = 1\n".parse().unwrap();
        let changes = migrate(&mut unversioned, 1, MIGRATIONS, &defaults);
        assert_eq!(unversioned.to_string(), "config_version = 3\nx = 11\n");
        assert_eq!(
            changes,
            [
                "multiplied x by 10",
                "added 1 to x",
                "set config_version to 3"
            ]
        );

        let mut at_two: DocumentMut = "config_version = 2\nx = 1\n".parse().unwrap();
        let changes = migrate(&mut at_two, 2, MIGRATIONS, &defaults);
        assert_eq!(at_two.to_string(), "config_version = 3\nx = 2\n");
        assert_eq!(changes, ["added 1 to x", "set config_version to 3"]);
    }

    #[test]
    fn keeps_comments_and_unknown_keys_and_fills_inline_tables() {
        let original = "\
# Survival server
seed = \"1\" # don't change
unknown = 3

[world]
chunk = { type = \"anvil\" }

# Ops only
[commands]
use_console = true
";
        let defaults: DocumentMut = "\
seed = \"0\"
tps = 20.0

[world]
autosave_ticks = 6000

[world.chunk]
type = \"pump\"
level = 6

[world.entity_islands]
unaudited_reach = 16.0

[commands]
use_console = true
log_console = true

[metrics]
enabled = true
"
        .parse()
        .unwrap();

        let mut document: DocumentMut = original.parse().unwrap();
        let changes = migrate(&mut document, 1, &[], &defaults);
        assert_eq!(
            document.to_string(),
            "\
config_version = 1
# Survival server
seed = \"1\" # don't change
unknown = 3
tps = 20.0

[world]
chunk = { type = \"anvil\", level = 6 }
autosave_ticks = 6000

[world.entity_islands]
unaudited_reach = 16.0

# Ops only
[commands]
use_console = true
log_console = true

[metrics]
enabled = true
"
        );
        assert_eq!(
            changes,
            [
                "added tps = 20.0",
                "added world.autosave_ticks = 6000",
                "added world.chunk.level = 6",
                "added [world.entity_islands] with default values",
                "added commands.log_console = true",
                "added [metrics] with default values",
                "set config_version to 1",
            ]
        );
    }
}
