//! `PEXGuard`: WorldGuard-style region protection for PEX.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod commands;
mod listeners;
mod region;
mod store;
mod uuid_migration;

use pex_common::Prefix;
use pumpkin::plugin::{Context, Plugin, PluginFuture};
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault};
use pumpkin_util::text::color::NamedColor;
use region::RegionSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pex_common::export_plugin!(
    PexGuard,
    "Region protection: build, interact, PvP, explosions, fire, entry and more."
);

pub(crate) const PREFIX: Prefix = Prefix {
    name: "Guard",
    color: NamedColor::DarkAqua,
};

type Selection = (Option<[i32; 3]>, Option<[i32; 3]>);

#[derive(Clone)]
pub(crate) struct Shared {
    regions: Arc<Mutex<RegionSet>>,
    selections: Arc<Mutex<HashMap<String, Selection>>>,
    data_folder: PathBuf,
}

impl Shared {
    pub(crate) fn regions(&self) -> MutexGuard<'_, RegionSet> {
        self.regions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn selections(&self) -> MutexGuard<'_, HashMap<String, Selection>> {
        self.selections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn save(&self, regions: &RegionSet) -> Result<(), String> {
        store::save(&self.data_folder, regions).map_err(|err| {
            tracing::error!("PEXGuard: could not save regions: {err}");
            "Could not save regions; see the server log.".to_owned()
        })
    }
}

pub struct PexGuard;

impl PexGuard {
    const fn new() -> Self {
        Self
    }
}

impl Plugin for PexGuard {
    fn on_load(&self, context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            context.init_log();
            let data_folder = context.get_data_folder();
            // Refuse to start on a broken file rather than run unprotected and overwrite it.
            let mut regions = store::load(&data_folder)?;
            let world_dir = context.server.basic_config.get_world_path();
            uuid_migration::migrate(&data_folder, &world_dir, &mut regions);
            let count = regions.regions.len();
            let shared = Shared {
                regions: Arc::new(Mutex::new(regions)),
                selections: Arc::new(Mutex::new(HashMap::new())),
                data_folder,
            };

            context.register_permission(Permission::new(
                commands::PERM_COMMAND,
                "Allows managing regions with /rg",
                PermissionDefault::Op(PermissionLvl::Two),
            ))?;
            context.register_permission(Permission::new(
                listeners::PERM_BYPASS,
                "Ignores all region protection",
                PermissionDefault::Op(PermissionLvl::Two),
            ))?;
            commands::register(&context, &shared);
            listeners::register(&context, &shared);
            tracing::info!("PEXGuard loaded with {count} regions");
            Ok(())
        })
    }
}
