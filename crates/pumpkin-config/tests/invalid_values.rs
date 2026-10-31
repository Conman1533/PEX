#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use pumpkin_config::{LoadConfiguration, LoadMessage, PumpkinConfig};
use tempfile::TempDir;
use toml_edit::DocumentMut;

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

fn files_in(dir: &Path) -> usize {
    fs::read_dir(dir).unwrap().count()
}

#[test]
fn one_bad_value_keeps_every_other_value() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pumpkin.toml");
    let edited = replace_once(
        &replace_once(
            &generated_file(dir.path()),
            "default_level_name = \"world\"",
            "default_level_name = \"survival\"",
        ),
        "white_list = false",
        "white_list = true",
    );
    fs::write(&path, &edited).unwrap();
    let (expected, _, _) = PumpkinConfig::load(dir.path()).unwrap();

    let bad = replace_once(&edited, "tps = 20.0", "tps = \"fast\"");
    fs::write(&path, &bad).unwrap();
    let (config, messages, pending) = PumpkinConfig::load(dir.path()).unwrap();

    assert_eq!(
        messages,
        [LoadMessage::Warn(
            "pumpkin.toml: tps = \"fast\" isn't valid (invalid type: string \"fast\", expected f32). Using the default 20.0 instead."
                .to_owned()
        )]
    );
    assert_eq!(config.basic.default_level_name, "survival");
    assert!(config.basic.white_list);
    // Every value, tps included, is what the file with `tps = 20.0` gives.
    assert_eq!(
        toml::to_string(&config).unwrap(),
        toml::to_string(&expected).unwrap()
    );
    assert!(pending.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), bad);
    assert_eq!(files_in(dir.path()), 1, "no backup");
}

#[test]
fn out_of_range_view_distance_uses_the_default() {
    let dir = TempDir::new().unwrap();
    let generated = generated_file(dir.path());
    for edition in ["java", "bedrock"] {
        for distance in [0, 65] {
            let mut document: DocumentMut = generated.parse().unwrap();
            document["networking"][edition]["view_distance"] = toml_edit::value(distance);
            fs::write(dir.path().join("pumpkin.toml"), document.to_string()).unwrap();
            let (config, messages, _) = PumpkinConfig::load(dir.path()).unwrap();

            assert_eq!(
                messages,
                [LoadMessage::Warn(format!(
                    "pumpkin.toml: networking.{edition}.view_distance = {distance} isn't valid (invalid value: integer `{distance}`, expected a view distance from 2 to 64). Using the default 16 instead."
                ))]
            );
            assert_eq!(config.advanced.networking.java.view_distance.get(), 16);
            assert_eq!(config.advanced.networking.bedrock.view_distance.get(), 16);
        }
    }
}

#[test]
fn syntax_error_stops_with_line_and_column() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("pumpkin.toml");
    let broken = replace_once(
        &generated_file(dir.path()),
        "[networking.java]\n",
        "[networking.java\n",
    );
    fs::write(&path, &broken).unwrap();
    let line = broken
        .lines()
        .position(|line| line == "[networking.java")
        .unwrap()
        + 1;

    let err = PumpkinConfig::load(dir.path())
        .err()
        .expect("a syntax error stops the server");
    assert_eq!(
        err,
        format!(
            "Couldn't parse {} at line {line}, column 17: unclosed table, expected `]`",
            path.display()
        )
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    assert_eq!(files_in(dir.path()), 1, "no backup");
}
