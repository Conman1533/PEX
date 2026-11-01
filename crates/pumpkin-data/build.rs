use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

const MC_VERSION: &str = "26.3";
/// SHA-1 of Mojang's `server.jar` download for `MC_VERSION` (the bundler jar, which holds
/// `server-<version>.jar`), as listed in the version package and pinned in `flake.nix`.
const SERVER_JAR_SHA1: &str = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c";
/// Written into `assets/datapack/` after extraction; names the jar the files came from.
const MARKER_FILE: &str = ".vanilla-jar";
const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or("Failed to resolve repository root directory")?;
    let assets_dir = repo_root.join("assets");
    let datapack_dir = assets_dir.join("datapack");
    let mc_data_dir = datapack_dir.join("data").join("minecraft");
    let marker_path = datapack_dir.join(MARKER_FILE);

    println!("cargo:rerun-if-changed={}", mc_data_dir.display());
    println!("cargo:rerun-if-changed={}", marker_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        assets_dir.join("conventional_tags").display()
    );

    let expected_marker = marker_contents();
    let marker = fs::read_to_string(&marker_path).ok();
    if mc_data_dir.is_dir() && marker.as_deref() == Some(expected_marker.as_str()) {
        copy_conventional_tags(&assets_dir, &datapack_dir);
        return Ok(());
    }

    if mc_data_dir.is_dir() {
        let found = marker.map_or_else(
            || "no marker".to_string(),
            |m| m.split_whitespace().collect::<Vec<_>>().join(" "),
        );
        println!(
            "cargo:warning=Vanilla datapack at {} is not from the Minecraft {MC_VERSION} server jar ({found}). Extracting it again...",
            datapack_dir.display()
        );
        fs::remove_dir_all(datapack_dir.join("data"))?;
    } else {
        println!(
            "cargo:warning=Vanilla datapack missing at {}. Downloading Minecraft {MC_VERSION} server jar...",
            mc_data_dir.display()
        );
    }

    if let Err(e) = ensure_datapack(&assets_dir, MC_VERSION) {
        return Err(format!(
            "Failed to download and extract vanilla datapack: {e}\n\
             Please ensure you have an active internet connection or set PUMPKIN_MINECRAFT_SERVER_JAR\n\
             or manually place Minecraft {MC_VERSION} data in {}",
            mc_data_dir.display()
        )
        .into());
    }

    Ok(())
}

fn marker_contents() -> String {
    format!("version={MC_VERSION}\nsha1={SERVER_JAR_SHA1}\n")
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn copy_conventional_tags(assets_dir: &Path, datapack_dir: &Path) {
    let src = assets_dir.join("conventional_tags").join("data").join("c");
    let dst = datapack_dir.join("data").join("c");
    if src.is_dir() && !dst.is_dir() {
        let _ = copy_dir_all(&src, &dst);
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn ensure_datapack(assets_dir: &Path, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let datapack_dir = assets_dir.join("datapack");

    let jar_bytes = if let Ok(path) = std::env::var("PUMPKIN_MINECRAFT_SERVER_JAR") {
        let path = PathBuf::from(path);
        if path.is_file() {
            println!(
                "cargo:warning=Using local server jar from {}",
                path.display()
            );
            fs::read(&path)?
        } else {
            download_server_jar(version)?
        }
    } else {
        download_server_jar(version)?
    };

    let actual = sha1_hex(&jar_bytes);
    if actual != SERVER_JAR_SHA1 {
        return Err(format!(
            "Minecraft {version} server jar has SHA-1 {actual}, expected {SERVER_JAR_SHA1}"
        )
        .into());
    }

    println!("cargo:warning=Extracting datapack from server jar...");
    extract_datapack(&jar_bytes, version, &datapack_dir)?;
    copy_conventional_tags(assets_dir, &datapack_dir);
    fs::write(datapack_dir.join(MARKER_FILE), marker_contents())?;
    println!("cargo:warning=Datapack extraction complete.");
    Ok(())
}

fn download_server_jar(version: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .user_agent("Pumpkin-Build/0.1.0")
        .build()?;

    println!("cargo:warning=Fetching version manifest from {MANIFEST_URL}...");
    let manifest: serde_json::Value = client.get(MANIFEST_URL).send()?.json()?;

    let version_entry = manifest["versions"]
        .as_array()
        .and_then(|arr| arr.iter().find(|v| v["id"].as_str() == Some(version)))
        .ok_or_else(|| format!("Minecraft version {version} not found in version manifest"))?;

    let package_url = version_entry["url"]
        .as_str()
        .ok_or("Version entry missing package url")?;

    println!("cargo:warning=Fetching version package metadata from {package_url}...");
    let pkg: serde_json::Value = client.get(package_url).send()?.json()?;

    let server_url = pkg["downloads"]["server"]["url"]
        .as_str()
        .ok_or("Package metadata missing server download url")?;
    let listed_sha1 = pkg["downloads"]["server"]["sha1"].as_str();
    if listed_sha1 != Some(SERVER_JAR_SHA1) {
        return Err(format!(
            "Version package lists server jar SHA-1 {}, expected {SERVER_JAR_SHA1}",
            listed_sha1.unwrap_or("(none)")
        )
        .into());
    }

    println!("cargo:warning=Downloading server jar from {server_url}...");
    let resp = client.get(server_url).send()?;
    if !resp.status().is_success() {
        return Err(format!("Server jar download failed with status {}", resp.status()).into());
    }

    let bytes = resp.bytes()?.to_vec();
    Ok(bytes)
}

fn extract_datapack(
    server_jar_bytes: &[u8],
    version: &str,
    datapack_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let inner_bytes = {
        let mut outer_zip = zip::ZipArchive::new(Cursor::new(server_jar_bytes))?;
        let inner_jar_name = format!("META-INF/versions/{version}/server-{version}.jar");
        let mut inner_file = outer_zip.by_name(&inner_jar_name)?;
        let mut inner_bytes = Vec::with_capacity(inner_file.size() as usize);
        inner_file.read_to_end(&mut inner_bytes)?;
        inner_bytes
    };

    let mut inner_zip = zip::ZipArchive::new(Cursor::new(inner_bytes))?;
    fs::create_dir_all(datapack_dir)?;

    for i in 0..inner_zip.len() {
        let mut file = inner_zip.by_index(i)?;
        let name = file.name()?;
        if !name.starts_with("data/") {
            continue;
        }
        let outpath = datapack_dir.join(&*name);
        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}
