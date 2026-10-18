//! Template PEX native plugin. Copy this crate, rename it, and replace the example parts.
//!
//! It shows every integration point from `PORTING_GUIDE.md`: config, a permission, a command
//! with an optional argument, a cancelling event handler and a tick handler.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pex_common::Prefix;
use pumpkin::command::argument_builder::{ArgumentBuilder, argument, command};
use pumpkin::command::argument_types::core::string::StringArgumentType;
use pumpkin::command::context::command_context::CommandContext;
use pumpkin::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin::plugin::block::block_break::BlockBreakEvent;
use pumpkin::plugin::server::server_tick_start::ServerTickStartEvent;
use pumpkin::plugin::{BoxFuture, Context, EventHandler, EventPriority, Plugin, PluginFuture};
use pumpkin::server::Server;
use pumpkin_util::permission::{Permission, PermissionDefault};
use pumpkin_util::text::color::NamedColor;
use serde::{Deserialize, Serialize};

pex_common::export_plugin!(TemplatePlugin, "Template plugin.");

const PREFIX: Prefix = Prefix {
    name: "Template",
    color: NamedColor::Gold,
};
const PERM_HELLO: &str = "pextemplate:command.hello";

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    greeting: String,
    /// Blocks players can't break, as block names like `diamond_ore`.
    protected_blocks: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            greeting: "Hello".to_owned(),
            protected_blocks: vec!["bedrock".to_owned()],
        }
    }
}

struct State {
    config: Config,
    ticks: u64,
}

#[derive(Clone)]
struct Shared(Arc<Mutex<State>>);

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct TemplatePlugin;

impl TemplatePlugin {
    const fn new() -> Self {
        Self
    }
}

impl Plugin for TemplatePlugin {
    fn on_load(&self, context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            context.init_log();
            let config: Config = pex_common::load_config(&context.get_data_folder(), "config.toml");
            let shared = Shared(Arc::new(Mutex::new(State { config, ticks: 0 })));

            context.register_permission(Permission::new(
                PERM_HELLO,
                "Allows /hello",
                PermissionDefault::Allow,
            ))?;

            let node = command("hello", "Greets you")
                .executes(HelloCommand(shared.clone()))
                .then(
                    argument("name", StringArgumentType::GreedyPhrase)
                        .executes(HelloCommand(shared.clone())),
                );
            context.register_command(node, PERM_HELLO);

            context.register_event::<BlockBreakEvent, _>(
                Arc::new(BreakHandler(shared.clone())),
                EventPriority::Normal,
                true,
            );
            context.register_event::<ServerTickStartEvent, _>(
                Arc::new(TickHandler(shared)),
                EventPriority::Normal,
                true,
            );

            tracing::info!("{} loaded", env!("CARGO_PKG_NAME"));
            Ok(())
        })
    }
}

struct HelloCommand(Shared);

impl CommandExecutor for HelloCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let source = &context.source;
        let _runtime = source.server().runtime.enter();
        let name = StringArgumentType::get(context, "name").unwrap_or(source.name.as_str());
        let greeting = self.0.state().config.greeting.clone();
        source.send_message(PREFIX.info(format!("{greeting}, {name}!")));
        Ok(1)
    }
}

struct BreakHandler(Shared);

impl EventHandler<BlockBreakEvent> for BreakHandler {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        let _runtime = server.runtime.enter();
        if !event.cancelled {
            let protected = self
                .0
                .state()
                .config
                .protected_blocks
                .iter()
                .any(|name| name.trim_start_matches("minecraft:") == event.block.name);
            if protected {
                event.cancelled = true;
                if let Some(player) = &event.player {
                    PREFIX.send_error(player, "That block is protected.");
                }
            }
        }
        Box::pin(async {})
    }
}

struct TickHandler(Shared);

impl EventHandler<ServerTickStartEvent> for TickHandler {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut ServerTickStartEvent,
    ) -> BoxFuture<'a, ()> {
        let mut state = self.0.state();
        state.ticks += 1;
        // Example of timed work without Tokio: once a minute.
        if state.ticks.is_multiple_of(20 * 60) {
            tracing::debug!("one minute passed");
        }
        Box::pin(async {})
    }
}
