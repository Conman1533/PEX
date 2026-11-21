//! Saving `/data ... storage`, vanilla's `CommandStorage`: one `data/<namespace>/command_storage.dat`
//! per namespace, holding `{contents: {<path>: <compound>}}`.

use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;
use std::path::Path;

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_world::world_info::data_files::{
    read_namespaced_data_file, write_namespaced_data_file,
};
use tracing::error;

use crate::entity::player::DATA_VERSION;

const COMMAND_STORAGE: &str = "command_storage.dat";

/// Namespaces that have a `command_storage.dat` in `<world>/data/`.
fn namespaces_on_disk(world_path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(world_path.join("data")) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join(COMMAND_STORAGE).is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// Reads every namespace's command storage, keyed by the full storage id (`namespace:path`).
#[must_use]
pub fn load(world_path: &Path) -> HashMap<String, NbtCompound> {
    let mut storage = HashMap::new();
    for namespace in namespaces_on_disk(world_path) {
        let Some(data) = read_namespaced_data_file(world_path, &namespace, COMMAND_STORAGE) else {
            continue;
        };
        let Some(contents) = data.get_compound("contents") else {
            continue;
        };
        for (path, tag) in &contents.child_tags {
            if let NbtTag::Compound(compound) = tag {
                storage.insert(format!("{namespace}:{path}"), compound.clone());
            }
        }
    }
    storage
}

/// Writes one `command_storage.dat` per namespace. A namespace whose storage was all cleared
/// gets an empty file, as vanilla writes it.
pub fn save<S: BuildHasher>(world_path: &Path, storage: &HashMap<String, NbtCompound, S>) {
    let mut namespaces: BTreeMap<String, NbtCompound> = namespaces_on_disk(world_path)
        .into_iter()
        .map(|namespace| (namespace, NbtCompound::new()))
        .collect();
    for (id, compound) in storage {
        let Some((namespace, path)) = id.split_once(':') else {
            continue;
        };
        let contents = namespaces.entry(namespace.to_string()).or_default();
        // Vanilla's `Container.put` drops a storage set to an empty compound.
        if !compound.is_empty() {
            contents.put_compound(path, compound.clone());
        }
    }
    for (namespace, contents) in namespaces {
        let mut data = NbtCompound::new();
        data.put_compound("contents", contents);
        if let Err(err) =
            write_namespaced_data_file(world_path, &namespace, COMMAND_STORAGE, DATA_VERSION, data)
        {
            error!("Failed to save command storage for {namespace}: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vanilla_file(bytes: &[u8]) -> NbtCompound {
        pumpkin_nbt::nbt_compress::read_gzip_compound_tag(std::io::Cursor::new(bytes)).unwrap()
    }

    /// The testdata files are what vanilla 26.3 saved after `/data modify storage` into
    /// `test:store`, `test:other` and `minecraft:plain`.
    #[test]
    fn vanilla_command_storage_loads_and_saves_unchanged() {
        let world = tempfile::tempdir().unwrap();
        let files: [(&str, &[u8]); 2] = [
            (
                "test",
                include_bytes!("testdata/command_storage_test_vanilla_26_3.dat"),
            ),
            (
                "minecraft",
                include_bytes!("testdata/command_storage_minecraft_vanilla_26_3.dat"),
            ),
        ];
        for (namespace, bytes) in files {
            let dir = world.path().join("data").join(namespace);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(COMMAND_STORAGE), bytes).unwrap();
        }

        let storage = load(world.path());
        let mut ids: Vec<_> = storage.keys().map(String::as_str).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["minecraft:plain", "test:other", "test:store"]);
        assert_eq!(storage["test:store"].get_int("a"), Some(5));
        assert_eq!(storage["test:store"].get_string("b"), Some("hi"));

        let out = tempfile::tempdir().unwrap();
        save(out.path(), &storage);
        for (namespace, bytes) in files {
            let saved = std::fs::read(
                out.path()
                    .join("data")
                    .join(namespace)
                    .join(COMMAND_STORAGE),
            )
            .unwrap();
            assert_eq!(vanilla_file(&saved), vanilla_file(bytes), "{namespace}");
        }
    }

    #[test]
    fn a_cleared_namespace_is_saved_empty() {
        let world = tempfile::tempdir().unwrap();
        let mut storage = HashMap::new();
        let mut compound = NbtCompound::new();
        compound.put_int("a", 1);
        storage.insert("gone:thing".to_string(), compound);
        save(world.path(), &storage);
        assert_eq!(load(world.path()).len(), 1);

        storage.insert("gone:thing".to_string(), NbtCompound::new());
        save(world.path(), &storage);
        assert!(load(world.path()).is_empty());
        let saved = read_namespaced_data_file(world.path(), "gone", COMMAND_STORAGE).unwrap();
        assert!(saved.get_compound("contents").unwrap().is_empty());
    }
}
