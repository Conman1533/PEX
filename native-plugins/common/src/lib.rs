//! Shared helpers for the PEX native plugins: plugin exports, config files and chat messages.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

use std::fs;
use std::path::Path;

use pumpkin::entity::player::Player;
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::NamedColor;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use pumpkin;

/// Exports the symbols the PEX native loader looks for.
///
/// This replaces `#[plugin_impl]`, which starts a separate Tokio runtime per plugin.
/// `$plugin` must have a `new() -> Self` constructor and implement `pumpkin::plugin::Plugin`.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty, $description:expr) => {
        #[unsafe(no_mangle)]
        pub static METADATA: std::sync::LazyLock<$crate::pumpkin::plugin::PluginMetadata> =
            std::sync::LazyLock::new(|| $crate::pumpkin::plugin::PluginMetadata {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                authors: vec!["PEX".to_string()],
                description: $description.to_string(),
                dependencies: Vec::new(),
                permissions: Vec::new(),
            });

        #[unsafe(no_mangle)]
        pub static PUMPKIN_API_VERSION: u32 = $crate::pumpkin::plugin::PLUGIN_API_VERSION;

        #[unsafe(no_mangle)]
        pub fn plugin() -> Box<dyn $crate::pumpkin::plugin::Plugin> {
            Box::new(<$plugin>::new())
        }
    };
}

/// Loads `file` from `dir`, writing the default config first if it doesn't exist.
///
/// A config that fails to parse is reported and replaced by the defaults for this run,
/// without overwriting the admin's file.
#[must_use]
pub fn load_config<T: Serialize + DeserializeOwned + Default>(dir: &Path, file: &str) -> T {
    let path = dir.join(file);
    fs::read_to_string(&path).map_or_else(
        |_| write_default(&path),
        |text| {
            toml::from_str(&text).unwrap_or_else(|err| {
                tracing::error!("{}: invalid config, using defaults: {err}", path.display());
                T::default()
            })
        },
    )
}

fn write_default<T: Serialize + Default>(path: &Path) -> T {
    let config = T::default();
    match toml::to_string_pretty(&config) {
        Ok(text) => {
            if let Err(err) = fs::write(path, text) {
                tracing::warn!("{}: could not write default config: {err}", path.display());
            }
        }
        Err(err) => tracing::warn!("could not serialize default config: {err}"),
    }
    config
}

/// A chat prefix like `[Guard] ` used for every message a plugin sends.
#[derive(Clone, Copy)]
pub struct Prefix {
    pub name: &'static str,
    pub color: NamedColor,
}

impl Prefix {
    #[must_use]
    pub fn info(self, message: impl Into<String>) -> TextComponent {
        self.build(message.into(), NamedColor::Gray)
    }

    #[must_use]
    pub fn error(self, message: impl Into<String>) -> TextComponent {
        self.build(message.into(), NamedColor::Red)
    }

    fn build(self, message: String, body: NamedColor) -> TextComponent {
        TextComponent::text("")
            .add_child(TextComponent::text(format!("[{}] ", self.name)).color_named(self.color))
            .add_child(TextComponent::text(message).color_named(body))
    }

    pub fn send(self, player: &Player, message: impl Into<String>) {
        player.send_system_message(&self.info(message));
    }

    pub fn send_error(self, player: &Player, message: impl Into<String>) {
        player.send_system_message(&self.error(message));
    }
}
