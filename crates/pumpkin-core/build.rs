use std::{env, fs, process::Command};

use sha2::{Digest, Sha256};

fn main() {
    // Get short hash (7 chars) for display
    let short_output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output();

    let git_hash_short = match short_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    // Get full hash for hover text
    let full_output = Command::new("git").args(["rev-parse", "HEAD"]).output();

    let git_hash_full = match full_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    println!("cargo::rerun-if-changed=../../.git/HEAD");
    println!("cargo::rerun-if-changed=../../.git/refs/heads/");
    println!("cargo::rerun-if-changed={CARGO_LOCK}");
    println!("cargo::rustc-env=GIT_HASH={git_hash_short}");
    println!("cargo::rustc-env=GIT_HASH_FULL={git_hash_full}");
    println!(
        "cargo::rustc-env=PUMPKIN_BUILD_ID={}",
        build_id(&git_hash_full)
    );
}

/// The workspace lockfile, relative to this crate.
const CARGO_LOCK: &str = "../../Cargo.lock";

/// Everything a native plugin must share with the server for their Rust types to match.
fn build_id(commit: &str) -> String {
    let rustc = rustc_version::version_meta().map_or_else(
        |_| "unknown".to_string(),
        |meta| match meta.commit_hash {
            Some(hash) => format!("{}-{}", meta.semver, hash.get(..9).unwrap_or(&hash)),
            None => meta.semver.to_string(),
        },
    );
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let lock = fs::read(CARGO_LOCK).map_or_else(
        |_| "unknown".to_string(),
        |lock| hex::encode(&Sha256::digest(lock)[..8]),
    );
    // This crate's own features. Build scripts can't see the features of dependencies.
    let features = env::var("CARGO_CFG_FEATURE").unwrap_or_default();
    let features = if features.is_empty() {
        "none"
    } else {
        &features
    };
    // Fields under `#[cfg(debug_assertions)]` (like `entity_islands::Islands::island_of`)
    // change struct layouts, so debug and release builds differ.
    let debug_assertions = if env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() {
        "on"
    } else {
        "off"
    };
    format!(
        "commit={commit} rustc={rustc} target={target} lock={lock} features={features} debug-assertions={debug_assertions}"
    )
}
