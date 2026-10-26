//! Configuration management and serialization for the Pumpkin Minecraft server.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use fun::FunConfig;

use logging::LoggingConfig;
use metrics::MetricsConfig;
use pumpkin_util::world_seed::Seed;
use pumpkin_util::{Difficulty, GameMode, PermissionLvl, random};
use recipe::RecipeConfig;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use std::path::PathBuf;
use std::{fs, num::NonZero, path::Path};
use toml_edit::DocumentMut;
use tracing::{debug, error, info, warn};

/// Fun and experimental configuration options.
pub mod fun;
/// Server logging configuration options.
pub mod logging;
/// Tick metrics configuration options.
pub mod metrics;
/// The `config_version` key and the migrations that update older config files.
pub mod migration;
/// Networking and protocol configuration options.
pub mod networking;
/// Plugin management configuration options.
pub mod plugins;
/// Recipe system configuration options.
pub mod recipe;

/// Resource pack configuration options.
pub mod resource_pack;

pub use chat::{AntiSpamConfig, ChatConfig};
pub use commands::{CommandOverride, CommandsConfig};
pub use networking::auth::AuthenticationConfig;
pub use networking::bedrock::BedrockConfig;
pub use networking::compression::CompressionConfig;
pub use networking::java::JavaConfig;
pub use networking::lan_broadcast::LANBroadcastConfig;
pub use networking::management::ManagementServerConfig;
pub use networking::packet_limiter::PacketLimiterConfig;
pub use plugins::{PluginOverride, PluginsConfig};
pub use pvp::PVPConfig;
pub use server_links::ServerLinksConfig;
pub use telemetry::TelemetryConfig;

/// Telemetry configuration options.
pub mod telemetry;

mod commands;

mod chat;
/// Chunk loading and saving configuration options.
pub mod chunk;
/// Lighting engine configuration options.
pub mod lighting;
/// Operator permission level configuration options.
pub mod op;

mod advancement;
mod player_data;
mod pvp;
mod server_links;
/// Whitelist configuration options.
pub mod whitelist;
/// World generation and dimension configuration options.
pub mod world;

use advancement::AdvancementConfig;
use migration::{CONFIG_VERSION_KEY, Migration, Step, UNVERSIONED_CONFIG_VERSION};
use networking::NetworkingConfig;
use player_data::PlayerDataConfig;
use resource_pack::ResourcePackConfig;
use world::LevelConfig;

/// Root configuration container for Pumpkin server settings.
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct PumpkinConfig {
    /// Core basic configuration settings.
    #[serde(flatten)]
    pub basic: BasicConfiguration,
    /// Advanced and feature-specific configuration settings.
    #[serde(flatten)]
    pub advanced: AdvancedConfiguration,
    /// Anonymous telemetry configuration settings.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

/// The `config_version` from which offline-mode players are stored under vanilla's offline UUIDs
/// instead of the ones PEX 1.x gave them. The server moves their files before it saves this version.
pub const OFFLINE_UUID_VERSION: u32 = 2;

impl LoadConfiguration for PumpkinConfig {
    const MIGRATIONS: &'static [Migration] = &[Migration {
        version: OFFLINE_UUID_VERSION,
        step: Step::ServerFiles,
    }];

    fn get_path() -> &'static Path {
        Path::new("pumpkin.toml")
    }

    fn validate(&self) {
        self.basic.validate();
        self.advanced.validate();
        self.telemetry.validate();

        let Some(min_vd) = NonZero::new(2) else {
            return;
        };
        let Some(max_vd) = NonZero::new(64) else {
            return;
        };

        // Validate Java
        assert!(
            self.advanced.networking.java.keep_alive_time > 0,
            "Java Keep alive time must be greater than 0"
        );
        assert!(
            self.advanced.networking.java.view_distance >= min_vd,
            "Java View distance must be at least 2"
        );
        assert!(
            self.advanced.networking.java.view_distance <= max_vd,
            "Java View distance must be less than 64"
        );
        if self.advanced.networking.java.online_mode {
            assert!(
                self.advanced.networking.java.encryption,
                "When online mode is enabled, encryption must be enabled"
            );
        }

        // Validate Bedrock
        assert!(
            self.advanced.networking.bedrock.view_distance >= min_vd,
            "Bedrock View distance must be at least 2"
        );
        assert!(
            self.advanced.networking.bedrock.view_distance <= max_vd,
            "Bedrock View distance must be less than 64"
        );
        if self.basic.allow_chat_reports {
            assert!(
                self.advanced.networking.java.online_mode,
                "When allow_chat_reports is enabled, java.online_mode must be enabled"
            );
        }
    }
}

const _: () = assert!(
    migration::migrations_are_ordered(PumpkinConfig::MIGRATIONS),
    "pumpkin.toml migrations must be ordered by version, starting above 1"
);

/// Advanced configuration for optional and feature-specific server settings.
///
/// Allows enabling/disabling features, customizing behaviour, and
/// tweaking performance or experimental options.
///
/// `Important`: The configuration should match vanilla by default.
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct AdvancedConfiguration {
    /// Logging-related configuration such as log levels and output behaviour.
    pub logging: LoggingConfig,
    /// Resource pack configuration, including enforcement and pack metadata.
    pub resource_pack: ResourcePackConfig,
    /// World and level-related settings beyond basic configuration.
    pub world: LevelConfig,
    /// Networking-related features such as compression, authentication, and LAN broadcast.
    pub networking: NetworkingConfig,
    /// Command system configuration, including availability and permissions.
    pub commands: CommandsConfig,
    /// Chat-related features such as formatting, filtering, and message behaviour.
    pub chat: ChatConfig,
    /// Player-vs-player rules and mechanics.
    pub pvp: PVPConfig,
    /// Server links configuration exposed to clients.
    pub server_links: ServerLinksConfig,
    /// Persistent player data handling and storage behaviour.
    pub player_data: PlayerDataConfig,
    /// Optional fun and experimental features.
    pub fun: FunConfig,
    /// Recipe-related configuration.
    pub recipe: RecipeConfig,
    /// Plugin-related configuration.
    pub plugins: PluginsConfig,
    /// Advancement configuration
    pub advancement: AdvancementConfig,
    /// Tick metrics configuration.
    pub metrics: MetricsConfig,
}

/// Basic configuration for core server settings.
///
/// Covers edition support, world, networking, gameplay rules, and security options.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct BasicConfiguration {
    /// The seed for the world generation.
    pub seed: Seed,
    /// The default game difficulty.
    pub default_difficulty: Difficulty,
    /// The op level assigned by the /op command.
    pub op_permission_level: PermissionLvl,
    /// Whether the Nether dimension is enabled.
    pub allow_nether: bool,
    /// Whether the End dimension is enabled.
    pub allow_end: bool,
    /// Whether the server is in hardcore mode.
    pub hardcore: bool,
    /// The server's ticks per second.
    pub tps: f32,
    /// The default gamemode for players.
    pub default_gamemode: GameMode,
    /// If the server forces the gamemode on-join.
    pub force_gamemode: bool,
    /// Whether to remove IPs from logs or not.
    pub scrub_ips: bool,
    /// Whether to use a server favicon.
    pub use_favicon: bool,
    /// Path to optional server favicon.
    pub favicon_path: Option<String>,
    /// The default level name
    pub default_level_name: String,
    /// Whether chat messages should be signed or not.
    pub allow_chat_reports: bool,
    /// Whether to enable the whitelist.
    pub white_list: bool,
    /// Whether to enforce the whitelist.
    pub enforce_whitelist: bool,
    /// Whether this server accepts incoming transfers from other servers.
    #[serde(alias = "accepts-transfers", alias = "accepts_transfers")]
    pub accepts_transfers: bool,
    /// The radius of the spawn protection area around the world spawn point.
    /// Players without operator privileges cannot break or place blocks within this radius.
    /// Set to 0 to disable spawn protection.
    pub spawn_protection: u32,
}

impl Default for BasicConfiguration {
    fn default() -> Self {
        Self {
            seed: Seed(random::get_seed()),
            default_difficulty: Difficulty::Normal,
            op_permission_level: PermissionLvl::Four,
            allow_nether: true,
            allow_end: true,
            hardcore: false,
            tps: 20.0,
            default_gamemode: GameMode::Survival,
            force_gamemode: false,
            scrub_ips: true,
            use_favicon: true,
            favicon_path: None,
            default_level_name: "world".to_string(),
            allow_chat_reports: false,
            white_list: false,
            enforce_whitelist: false,
            accepts_transfers: true,
            spawn_protection: 16,
        }
    }
}

impl BasicConfiguration {
    /// Returns the path to the server's default world folder.
    #[must_use]
    pub fn get_world_path(&self) -> PathBuf {
        PathBuf::from(&self.default_level_name)
    }

    /// Validates basic configuration options.
    pub const fn validate(&self) {}
}

impl AdvancedConfiguration {
    /// Validates advanced configuration options.
    pub const fn validate(&self) {
        //self.resource_pack.validate();
    }
}

/// A log line from loading a config file, kept until the logger is set up.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadMessage {
    /// A change made to the file.
    Info(String),
    /// A problem the server works around, such as a change it couldn't save.
    Warn(String),
    /// The file couldn't be read or parsed, so the defaults are used.
    Error(String),
}

impl LoadMessage {
    /// Logs the line at its level.
    pub fn log(&self) {
        match self {
            Self::Info(line) => info!("{line}"),
            Self::Warn(line) => warn!("{line}"),
            Self::Error(line) => error!("{line}"),
        }
    }
}

/// A migrated config file that [`LoadConfiguration::load`] didn't write, because its new version
/// also stands for [`Step::ServerFiles`] work the server still has to do.
#[must_use = "the file keeps its old version until it is saved"]
pub struct PendingSave {
    path: PathBuf,
    /// The file as it was, or `None` if there was no file.
    original: Option<String>,
    document: DocumentMut,
    version: u32,
    changes: Vec<String>,
}

impl PendingSave {
    /// The version the file had, which the server files are also at. A missing file counts as
    /// unversioned, since an old world may sit next to it.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Backs up the file and writes the migrated one. Call it once the server files are migrated
    /// too. Returns the lines to log.
    #[must_use]
    pub fn save(self) -> Vec<LoadMessage> {
        let mut messages = Vec::new();
        match &self.original {
            Some(original) => save_migrated(
                &self.path,
                original,
                &self.document,
                self.version,
                self.changes,
                &mut messages,
            ),
            None => {
                if let Err(err) = fs::write(&self.path, self.document.to_string()) {
                    messages.push(LoadMessage::Warn(format!(
                        "Couldn't write default config to {:?}. Reason: {}",
                        self.path.display(),
                        err
                    )));
                }
            }
        }
        messages
    }
}

/// Trait for loading and validating configuration from a TOML file.
///
/// Provides default implementations for loading, migrating and filling in
/// missing values. Also requires validation logic.
pub trait LoadConfiguration {
    /// The steps that bring an older version of this file up to date, ordered by version.
    const MIGRATIONS: &'static [Migration];

    /// Load configuration from the given directory.
    ///
    /// Creates the directory if it doesn't exist and reads the TOML file. An older file is
    /// updated by [`Self::MIGRATIONS`], missing keys get their defaults, and a changed file is
    /// backed up before it is rewritten. Returns the validated configuration and the lines to
    /// log once the logger is ready. While a [`Step::ServerFiles`] migration is pending, the file
    /// isn't written: the server writes it with the returned [`PendingSave`] once that work is done.
    #[must_use]
    fn load(config_dir: &Path) -> (Self, Vec<LoadMessage>, Option<PendingSave>)
    where
        Self: Sized + Default + Serialize + DeserializeOwned,
    {
        if !config_dir.exists() {
            debug!("creating new config root folder");
            let _ = fs::create_dir(config_dir);
        }

        let path = config_dir.join(Self::get_path());
        let mut messages = Vec::new();

        let (config, save) = if path.exists() {
            let file_content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(err) => {
                    messages.push(LoadMessage::Error(format!(
                        "Couldn't read configuration file at {}: {err}",
                        path.display()
                    )));
                    return (Self::default(), messages, None);
                }
            };
            load_existing(&path, &file_content, &mut messages)
        } else {
            let content = Self::default();
            let save = match default_document(&content) {
                Ok(mut document) => {
                    migration::set_config_version(
                        &mut document,
                        migration::latest_version(Self::MIGRATIONS),
                    );
                    Some(PendingSave {
                        path,
                        original: None,
                        document,
                        version: UNVERSIONED_CONFIG_VERSION,
                        changes: Vec::new(),
                    })
                }
                Err(err) => {
                    messages.push(LoadMessage::Warn(format!(
                        "Couldn't serialize default config for {:?}. Reason: {}",
                        path.display(),
                        err
                    )));
                    None
                }
            };
            (content, save)
        };

        let pending = save.and_then(|save| {
            if migration::needs_server_files(Self::MIGRATIONS, save.version) {
                Some(save)
            } else {
                messages.extend(save.save());
                None
            }
        });

        config.validate();
        (config, messages, pending)
    }

    /// Returns the path to the configuration file relative to the config directory.
    fn get_path() -> &'static Path;

    /// Validates the configuration after loading or merging.
    fn validate(&self);
}

/// Parses an existing config file and brings it up to date. Returns the config and, if anything
/// changed, the file to save.
fn load_existing<C>(
    path: &Path,
    file_content: &str,
    messages: &mut Vec<LoadMessage>,
) -> (C, Option<PendingSave>)
where
    C: LoadConfiguration + Default + Serialize + DeserializeOwned,
{
    let mut document: DocumentMut = match file_content.parse() {
        Ok(document) => document,
        Err(err) => {
            messages.push(LoadMessage::Error(format!(
                "Couldn't parse TOML at {}. Reason: {err}. Using default config.",
                path.display()
            )));
            return (C::default(), None);
        }
    };

    let default_config = C::default();
    let Ok(defaults) = default_document(&default_config) else {
        return (default_config, None);
    };

    let latest = migration::latest_version(C::MIGRATIONS);
    let mut save = None;
    if let Some(version) = migration::file_version(&document)
        .filter(|version| (UNVERSIONED_CONFIG_VERSION..=latest).contains(version))
    {
        let changes = migration::migrate(&mut document, version, C::MIGRATIONS, &defaults);
        if !changes.is_empty() {
            save = Some(PendingSave {
                path: path.to_owned(),
                original: Some(file_content.to_owned()),
                document: document.clone(),
                version,
                changes,
            });
        }
    } else {
        let found = document
            .get(CONFIG_VERSION_KEY)
            .map(|item| item.to_string().trim().to_owned())
            .unwrap_or_default();
        messages.push(LoadMessage::Warn(format!(
            "{} has {CONFIG_VERSION_KEY} {found}, which this server doesn't know (it writes {latest}). Leaving the file unchanged.",
            file_name(path)
        )));
        // Same as before versions existed: read what the file has, defaults for the rest.
        let _ = migration::add_missing_keys(&mut document, &defaults);
    }

    let config = toml::from_str(&document.to_string()).unwrap_or_else(|_| C::default());
    (config, save)
}

/// Backs the file up as it was, then writes the migrated document in its place.
fn save_migrated(
    path: &Path,
    original: &str,
    document: &DocumentMut,
    version: u32,
    changes: Vec<String>,
    messages: &mut Vec<LoadMessage>,
) {
    let file_name = file_name(path);
    match migration::write_backup(path, original, version) {
        Ok(backup) => {
            messages.push(LoadMessage::Info(format!(
                "Saved a copy of {file_name} as {} before changing it",
                backup.file_name().unwrap_or_default().to_string_lossy()
            )));
            if let Err(err) = fs::write(path, document.to_string()) {
                messages.push(LoadMessage::Warn(format!(
                    "Couldn't write the updated {file_name}: {err}. The changes below apply until the next start."
                )));
            }
        }
        Err(err) => messages.push(LoadMessage::Warn(format!(
            "Couldn't back up {file_name}, so it was left unchanged: {err}. The changes below apply until the next start."
        ))),
    }
    messages.extend(
        changes
            .into_iter()
            .map(|change| LoadMessage::Info(format!("{file_name}: {change}"))),
    );
}

/// The default config as an editable document, without a `config_version`.
fn default_document<C: Serialize>(config: &C) -> Result<DocumentMut, String> {
    toml::to_string(config)
        .map_err(|err| err.to_string())?
        .parse()
        .map_err(|err: toml_edit::TomlError| err.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |f| f.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use toml::from_str;

    use super::BasicConfiguration;

    #[test]
    fn accepts_transfers_defaults_to_true() {
        let config: BasicConfiguration = BasicConfiguration::default();
        assert!(config.accepts_transfers);
    }

    #[test]
    fn accepts_transfers_reads_vanilla_aliases() {
        let config: BasicConfiguration = from_str("accepts-transfers = false").unwrap();
        assert!(!config.accepts_transfers);

        let config: BasicConfiguration = from_str("accepts_transfers = false").unwrap();
        assert!(!config.accepts_transfers);
    }
}
