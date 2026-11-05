#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use pumpkin_config::{LoadConfiguration, LoadMessage, PumpkinConfig};
use tempfile::TempDir;

/// The file a new server writes in `dir`.
fn generated_file(dir: &Path) -> String {
    let (_, _, pending) = PumpkinConfig::load(dir).unwrap();
    let _ = pending.expect("a new file is saved by the server").save();
    fs::read_to_string(dir.join("pumpkin.toml")).unwrap()
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?}");
    text.replace(from, to)
}

#[test]
fn unknown_key_is_reported_by_full_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pumpkin.toml");
    let edited = replace_once(
        &generated_file(dir.path()),
        "[networking.java]\n",
        "[networking.java]\nonlinemode = false\n",
    );
    fs::write(&path, &edited).unwrap();

    let (config, messages, pending) = PumpkinConfig::load(dir.path()).unwrap();
    assert_eq!(
        messages,
        [LoadMessage::Warn(
            "pumpkin.toml: unknown key networking.java.onlinemode".to_owned()
        )]
    );
    assert!(config.advanced.networking.java.online_mode);
    assert!(pending.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}

#[test]
fn optional_and_aliased_keys_are_known() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pumpkin.toml");
    let mut edited = generated_file(dir.path());
    for (from, to) in [
        (
            "use_favicon = true\n",
            "use_favicon = true\nfavicon_path = \"icon.png\"\n",
        ),
        ("keep_alive_time = 15\n", "keep-alive-time = 30\n"),
        (
            "[commands]\n",
            "[commands.overrides.tp]\nenabled = false\n\n[commands]\n",
        ),
        (
            "[plugins]\n",
            "[plugins.overrides.x.environment]\nHOME = \"/srv\"\n\n[plugins]\n",
        ),
        ("[telemetry]\n", "[telemetry]\nserver_name = \"Survival\"\n"),
        (
            "[world.chunk]\ntype = \"pump\"\n",
            "[world.chunk]\ntype = \"pump\"\n\n[world.chunk.compression]\nalgorithm = \"LZ4\"\nlevel = 6\n",
        ),
    ] {
        edited = replace_once(&edited, from, to);
    }
    fs::write(&path, &edited).unwrap();

    let (config, messages, pending) = PumpkinConfig::load(dir.path()).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(config.basic.favicon_path.as_deref(), Some("icon.png"));
    assert_eq!(config.advanced.networking.java.keep_alive_time, 30);
    assert!(!config.advanced.commands.overrides["tp"].enabled);
    assert_eq!(config.telemetry.server_name.as_deref(), Some("Survival"));
    // The file has the keep-alive time under its other name, so it isn't added again.
    assert!(pending.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}
