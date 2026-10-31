#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use pumpkin_config::chunk::ChunkConfig;
use pumpkin_config::migration::{CONFIG_VERSION_KEY, latest_version};
use pumpkin_config::{LoadConfiguration, LoadMessage, PendingSave, PumpkinConfig};
use tempfile::TempDir;

/// The `pumpkin.toml` each release wrote on its first start in an empty directory.
const RELEASE_FILES: [(&str, &str); 3] = [
    ("v1.0.0", include_str!("fixtures/pumpkin-v1.0.0.toml")),
    ("v1.1.0", include_str!("fixtures/pumpkin-v1.1.0.toml")),
    ("v1.2.0", include_str!("fixtures/pumpkin-v1.2.0.toml")),
];

/// The keys `config_version` 3 removes, as table and key.
const DEAD_KEYS: [(&str, &str); 8] = [
    ("world.chunk", "write_in_place"),
    ("networking.java.authentication", "enabled"),
    ("networking.java.authentication.textures", "enabled"),
    ("networking.java.authentication.textures.types", "skin"),
    ("networking.java.authentication.textures.types", "cape"),
    ("networking.java.authentication.textures.types", "elytra"),
    ("networking.bedrock.compression", "enabled"),
    ("pvp", "swing"),
];

fn load_file(dir: &Path, contents: &str) -> (PumpkinConfig, Vec<LoadMessage>, Option<PendingSave>) {
    fs::write(dir.join("pumpkin.toml"), contents).unwrap();
    PumpkinConfig::load(dir)
}

/// Loads like a server whose file migrations succeed: saves the pending file right away.
fn load_and_save(dir: &Path) -> Vec<LoadMessage> {
    let (_, mut messages, pending) = PumpkinConfig::load(dir);
    if let Some(pending) = pending {
        messages.extend(pending.save());
    }
    messages
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

fn info(line: &str) -> LoadMessage {
    LoadMessage::Info(line.to_owned())
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?}");
    text.replace(from, to)
}

#[test]
fn release_files_migrate_to_the_current_version() {
    let latest = latest_version(PumpkinConfig::MIGRATIONS);
    for (release, original) in RELEASE_FILES {
        let dir = TempDir::new().unwrap();
        let (config, messages, pending) = load_file(dir.path(), original);
        assert!(messages.is_empty(), "{release}: {messages:?}");
        let pending = pending.expect("a 1.x file waits for the offline UUID migration");
        assert_eq!(pending.version(), 1, "{release}");
        let messages = pending.save();

        assert_eq!(
            read(dir.path(), "pumpkin.toml.v1.bak"),
            original,
            "{release}"
        );
        // Upstream had already removed `write_in_place` when 1.2.0 came out.
        let removed: Vec<_> = DEAD_KEYS
            .iter()
            .filter(|&&(table, _)| release != "v1.2.0" || table != "world.chunk")
            .collect();
        let mut expected = vec![info(
            "Saved a copy of pumpkin.toml as pumpkin.toml.v1.bak before changing it",
        )];
        expected.extend(removed.iter().map(|(table, key)| {
            info(&format!(
                "pumpkin.toml: removed {table}.{key}, which had no effect"
            ))
        }));
        expected.extend([
            info("pumpkin.toml: added [world.entity_islands] with default values"),
            info("pumpkin.toml: added networking.java.validate_player_names = true"),
            info(&format!(
                "pumpkin.toml: set {CONFIG_VERSION_KEY} to {latest}"
            )),
        ]);
        assert_eq!(messages, expected, "{release}");

        let migrated = read(dir.path(), "pumpkin.toml");
        let table: toml::Table = toml::from_str(&migrated).unwrap();
        assert_eq!(
            table[CONFIG_VERSION_KEY].as_integer(),
            Some(latest.into()),
            "{release}"
        );
        // Nothing else the release wrote is lost or reordered: the other changes are additions.
        // Removing the texture types empties their table, which goes with its blank lines.
        let mut header = "";
        let mut remaining = migrated.lines();
        for line in original.lines() {
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|line| line.strip_suffix(']'))
            {
                header = name;
            }
            let key = line.split_once(" = ").map_or("", |(key, _)| key);
            if removed.contains(&&(header, key))
                || header == "networking.java.authentication.textures.types"
            {
                continue;
            }
            assert!(
                remaining.any(|kept| kept == line),
                "{release} lost {line:?}"
            );
        }
        assert!(!migrated.contains("write_in_place"), "{release}");
        assert_eq!(
            config.basic.seed.0.to_string(),
            table["seed"].as_str().unwrap(),
            "{release}"
        );
        assert_eq!(
            matches!(config.advanced.world.chunk, ChunkConfig::Anvil(_)),
            release != "v1.2.0",
            "{release} chunk format"
        );

        // A migrated file is current: loading it again changes nothing.
        let (_, messages, pending) = PumpkinConfig::load(dir.path());
        assert!(messages.is_empty(), "{release}: {messages:?}");
        assert!(pending.is_none(), "{release}");
        assert_eq!(read(dir.path(), "pumpkin.toml"), migrated, "{release}");
    }
}

#[test]
fn current_file_is_left_byte_for_byte() {
    let dir = TempDir::new().unwrap();
    let messages = load_and_save(dir.path());
    assert!(messages.is_empty(), "{messages:?}");

    let generated = read(dir.path(), "pumpkin.toml");
    let (_, messages, pending) = PumpkinConfig::load(dir.path());
    assert!(messages.is_empty(), "{messages:?}");
    assert!(pending.is_none());
    assert_eq!(read(dir.path(), "pumpkin.toml"), generated);

    let edited = format!(
        "# Survival server\n{}",
        generated.replace("tps = 20.0", "tps   =  20.0 # vanilla")
    );
    let (_, messages, pending) = load_file(dir.path(), &edited);
    assert!(messages.is_empty(), "{messages:?}");
    assert!(pending.is_none());
    assert_eq!(read(dir.path(), "pumpkin.toml"), edited);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no backup");
}

#[test]
fn an_earlier_backup_is_not_overwritten() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("pumpkin.toml.v1.bak"), "earlier").unwrap();
    let (_, original) = RELEASE_FILES[2];
    fs::write(dir.path().join("pumpkin.toml"), original).unwrap();
    let messages = load_and_save(dir.path());

    assert_eq!(read(dir.path(), "pumpkin.toml.v1.bak"), "earlier");
    assert_eq!(read(dir.path(), "pumpkin.toml.v1 (1).bak"), original);
    assert_eq!(
        messages[0],
        info("Saved a copy of pumpkin.toml as pumpkin.toml.v1 (1).bak before changing it")
    );
}

#[test]
fn file_from_a_newer_server_is_left_unchanged() {
    let dir = TempDir::new().unwrap();
    let newer = format!(
        "{CONFIG_VERSION_KEY} = {}\n",
        latest_version(PumpkinConfig::MIGRATIONS) + 1
    );
    let (_, messages, pending) = load_file(dir.path(), &newer);

    assert_eq!(read(dir.path(), "pumpkin.toml"), newer);
    assert!(matches!(messages.as_slice(), [LoadMessage::Warn(_)]));
    assert!(pending.is_none());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no backup");
}

/// The file only gets the version that stands for migrated player files once the server saves it
/// after migrating them, so a start that fails or crashes in between migrates them again.
#[test]
fn file_keeps_its_version_until_the_server_files_are_migrated() {
    let dir = TempDir::new().unwrap();
    let (_, original) = RELEASE_FILES[2];
    let (_, _, pending) = load_file(dir.path(), original);
    // The server stopped before its files were migrated.
    drop(pending);
    assert_eq!(read(dir.path(), "pumpkin.toml"), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no backup");

    let (_, _, pending) = PumpkinConfig::load(dir.path());
    assert_eq!(pending.map(|pending| pending.version()), Some(1));

    // Without a file, an old world could still be there, so it is checked too.
    let new = TempDir::new().unwrap();
    let (_, _, pending) = PumpkinConfig::load(new.path());
    assert_eq!(pending.map(|pending| pending.version()), Some(1));
    assert!(!new.path().join("pumpkin.toml").exists());
}

/// A file set back to `config_version = 1` runs every step again, which changes nothing more.
#[test]
fn dead_key_step_is_safe_to_run_twice() {
    let latest = latest_version(PumpkinConfig::MIGRATIONS);
    let dir = TempDir::new().unwrap();
    let (_, original) = RELEASE_FILES[0];
    fs::write(dir.path().join("pumpkin.toml"), original).unwrap();
    let _ = load_and_save(dir.path());
    let migrated = read(dir.path(), "pumpkin.toml");
    assert!(!migrated.contains("write_in_place"));

    let set_back = replace_once(
        &migrated,
        &format!("{CONFIG_VERSION_KEY} = {latest}\n"),
        &format!("{CONFIG_VERSION_KEY} = 1\n"),
    );
    fs::write(dir.path().join("pumpkin.toml"), set_back).unwrap();
    let messages = load_and_save(dir.path());

    assert_eq!(
        messages,
        [
            info("Saved a copy of pumpkin.toml as pumpkin.toml.v1 (1).bak before changing it"),
            info(&format!(
                "pumpkin.toml: set {CONFIG_VERSION_KEY} to {latest}"
            )),
        ]
    );
    assert_eq!(read(dir.path(), "pumpkin.toml"), migrated);
}

#[test]
fn inline_chunk_table_loses_write_in_place() {
    let latest = latest_version(PumpkinConfig::MIGRATIONS);
    let dir = TempDir::new().unwrap();
    let _ = load_and_save(dir.path());
    let generated = read(dir.path(), "pumpkin.toml");
    let version_line = format!("{CONFIG_VERSION_KEY} = {latest}\n");
    let earlier_line = format!("{CONFIG_VERSION_KEY} = {}\n", latest - 1);
    let inline = replace_once(
        &replace_once(
            &replace_once(&generated, &version_line, &earlier_line),
            "[world.chunk]\ntype = \"pump\"\n\n",
            "",
        ),
        "[world]\n",
        "[world]\nchunk = { type = \"anvil\", write_in_place = true }\n",
    );
    fs::write(dir.path().join("pumpkin.toml"), &inline).unwrap();
    let messages = load_and_save(dir.path());

    assert_eq!(
        messages,
        [
            info(&format!(
                "Saved a copy of pumpkin.toml as pumpkin.toml.v{}.bak before changing it",
                latest - 1
            )),
            info("pumpkin.toml: removed world.chunk.write_in_place, which had no effect"),
            info(&format!(
                "pumpkin.toml: set {CONFIG_VERSION_KEY} to {latest}"
            )),
        ]
    );
    assert_eq!(
        read(dir.path(), "pumpkin.toml"),
        replace_once(
            &replace_once(&inline, &earlier_line, &version_line),
            "chunk = { type = \"anvil\", write_in_place = true }",
            "chunk = { type = \"anvil\" }",
        )
    );
}
