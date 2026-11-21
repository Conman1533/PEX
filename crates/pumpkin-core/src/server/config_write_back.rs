//! Settings changed while the server runs are written back to `pumpkin.toml`, as vanilla
//! `DedicatedServerSettings.update` does for `server.properties` (D3-4, G3-06).
//!
//! The writes run in order on a thread of their own, never on the tick thread. A server started
//! without a `pumpkin.toml` in its directory, such as a test, writes nothing.

use std::sync::{OnceLock, mpsc};

use serde::Serialize;

type Write = Box<dyn FnOnce() + Send>;

static WRITER: OnceLock<Option<mpsc::Sender<Write>>> = OnceLock::new();

fn writer() -> Option<&'static mpsc::Sender<Write>> {
    WRITER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<Write>();
            std::thread::Builder::new()
                .name("config-write-back".to_owned())
                .spawn(move || receiver.into_iter().for_each(|write| write()))
                .ok()
                .map(|_| sender)
        })
        .as_ref()
}

/// Sets `key` (dotted, as in `pumpkin.toml`) to `value` in the server's `pumpkin.toml`.
pub fn write_back<T: Serialize + Send + 'static>(key: &'static str, value: T) {
    let Ok(dir) = std::env::current_dir() else {
        return;
    };
    let path = dir.join("pumpkin.toml");
    let Some(writer) = writer() else {
        return;
    };
    let _ = writer.send(Box::new(move || {
        if !path.is_file() {
            return;
        }
        match pumpkin_config::set_config_key(&path, key, &value) {
            Ok(true) => tracing::info!("pumpkin.toml: saved {key}, changed while running"),
            Ok(false) => {}
            Err(err) => tracing::warn!("Couldn't save {key} to pumpkin.toml: {err}"),
        }
    }));
}

/// `easy` as `pumpkin.toml` writes it: `Easy`.
#[must_use]
pub fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
