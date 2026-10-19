// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT
//
// Native PEX port of WorldPumpkin (https://github.com/NicDevTV/WorldPumpkin).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod blocks;
mod commands;
mod config;
mod engine;
mod host;
mod messages;
mod state;

use commands::Shared;
use config::{Config, PERMISSION_NODES};
use engine::EditQueue;
use pumpkin::plugin::server::server_tick_start::ServerTickStartEvent;
use pumpkin::plugin::{BoxFuture, Context, EventHandler, EventPriority, Plugin, PluginFuture};
use pumpkin::server::Server;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault};
use state::PluginState;
use std::sync::{Arc, Mutex};

pex_common::export_plugin!(
    WorldPumpkin,
    "World editing: selections, fill, replace, walls, move, undo and redo."
);

pub struct WorldPumpkin;

impl WorldPumpkin {
    const fn new() -> Self {
        Self
    }
}

impl Plugin for WorldPumpkin {
    fn on_load(&self, context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            context.init_log();
            let data_folder = context.get_data_folder();
            let shared = Shared {
                state: Arc::new(Mutex::new(PluginState::new(Config::load(&data_folder)))),
                queue: Arc::new(Mutex::new(EditQueue::default())),
                data_folder,
            };

            for node in PERMISSION_NODES {
                let default = if node.default_op {
                    PermissionDefault::Op(PermissionLvl::Two)
                } else {
                    PermissionDefault::Deny
                };
                context.register_permission(Permission::new(
                    node.node,
                    node.description,
                    default,
                ))?;
            }
            commands::register(&context, &shared);
            context.register_event::<ServerTickStartEvent, _>(
                Arc::new(TickHandler(shared)),
                EventPriority::Normal,
                true,
            );
            tracing::info!("WorldPumpkin {} loaded", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
    }
}

/// Spends the per-tick block budget on the front of the edit queue.
struct TickHandler(Shared);

impl EventHandler<ServerTickStartEvent> for TickHandler {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        _event: &'a mut ServerTickStartEvent,
    ) -> BoxFuture<'a, ()> {
        // This plugin links its own copy of Tokio; entering the server's runtime lets world
        // calls that spawn tasks find it.
        let _runtime = server.runtime.enter();
        let config = self.0.state().config().clone();
        self.0.queue().process_tick(&config);
        Box::pin(async {})
    }
}
