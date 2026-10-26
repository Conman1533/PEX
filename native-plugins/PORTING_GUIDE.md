# Porting plugins to PEX native plugins: guide for LLM agents

You are an AI coding agent. Your job is to turn an existing Minecraft server plugin (Bukkit/Spigot/Paper, Pumpkin Wasm, or a description of one) into a **native PEX plugin**: a Rust `cdylib` that the PEX server loads from `plugins/`.

Read this whole file before writing code. Follow the rules exactly. When this guide and your own memory of Pumpkin, Bukkit or Minecraft disagree, trust this guide and the PEX source code. Everything here was checked against PEX at commit `2009aa6d` (MC 26.3).

Native plugins live in the PEX repository under `native-plugins/`, as members of the main Cargo workspace. Building PEX builds them too, from the same source, lockfile and compiler as the server, which is what makes them safe to load. Paths in this guide are relative to `native-plugins/` unless they start with `../`, which means the repository root.

Three folders here are your references:

- `template/`: the **starting point for every new plugin**. It builds, loads and was boot-tested. It shows config, a permission, a command with an optional argument, a cancelling event handler and a tick handler. Copy the folder, rename the package, then change it.
- `worldpumpkin/`: a full plugin (WorldPumpkin, ported from Wasm) with a queue, undo history and unit tests that run without a server.
- `common/`: shared helpers (`export_plugin!`, `load_config`, `Prefix`). Use them instead of writing your own.

When unsure how to do something, find how one of these does it and copy that pattern.

---

## 0. Hard rules

1. **Plugins only work with the server build they were built with.** Native plugins share memory layouts with the server. The server refuses a plugin whose build id (commit, compiler, target, `Cargo.lock` hash, features, debug assertions) differs from its own, which catches plugins from another PEX commit, from upstream Pumpkin or from another Rust compiler. The id can't see uncommitted changes or a plugin built outside this workspace against its own lockfile; such a plugin loads *without any error* and then crashes or corrupts memory. That's why plugins are workspace members: `cargo build` produces the server and every plugin together. Never build a plugin outside this workspace or ship it with a different server binary.
2. **Use workspace dependency versions.** Prefer `<crate>.workspace = true` for anything in `../Cargo.toml`'s `[workspace.dependencies]`. Before adding a new crate, run `grep -n '^name = "<crate>"' -A1 ../Cargo.lock`; if it's already locked, use that version. Never run `cargo update`.
3. **Plugin changes don't touch the server crates.** Don't edit anything under `../crates/` in a plugin commit. If PEX is missing an API or event, write that down in your report; fixing the server is a separate change.
4. **No `unwrap`, `expect`, `panic!`, `todo!`, `println!` or `eprintln!`.** Clippy denies them in this workspace. Use `?`, `unwrap_or_else(PoisonError::into_inner)` for mutexes, and `tracing::{info, warn, error}` for logs.
5. **A panic inside a plugin can take down the server.** Treat every input (command args, config files, NBT) as untrusted and return errors instead.
6. **Do not use `#[plugin_impl]` or `#[plugin_method]`** from `pumpkin-api-macros`. Use `pex_common::export_plugin!` (see §2). The macros start a whole extra Tokio runtime per plugin.
7. **Never claim something works without running it.** "It compiles" is not "it works". Section 8 says what must be tested, and how.
8. **Keep the original plugin's license and credit** if you port its code. MIT/Apache code can be ported with its license file and copyright headers kept (see `worldpumpkin/LICENSE` and the headers in `worldpumpkin/src`). Do not copy GPL-incompatible or unlicensed code; reimplement the behaviour from its documentation instead.

---

## 1. What a native PEX plugin is

| Fact | Consequence for you |
| :-- | :-- |
| It is a Rust `cdylib` (`.dll` on Windows, `.so` on Linux, `.dylib` on macOS). | Put `crate-type = ["cdylib"]` (or `["cdylib", "rlib"]` if other crates or tests need it) in `[lib]`. |
| It statically links its **own copy** of `pumpkin-core`, `tokio`, `tracing` and `std`. | Global state inside those crates is duplicated. See §6 for the two things this breaks: logging and the Tokio runtime. |
| The server reads four exported symbols: `PUMPKIN_BUILD_ID`, `PUMPKIN_API_VERSION`, `METADATA`, `plugin`. | `export_plugin!` generates all four. |
| The plugin gets an `Arc<Context>` in `on_load`. `context.server` is the whole `Arc<Server>`. | There is no sandbox. You can call any public server function. Use that power carefully. |
| The plugin's name is its Cargo package name. | Package `pexguard` gets data folder `plugins/data/pexguard/` and permission namespace `pexguard:`. Choose a short, lowercase, unique package name. |

Wasm plugins (`pumpkin-plugin-api`, `.wasm` files) are a different system. **Do not mix them.** If you are porting a Wasm plugin, you are replacing its `pumpkin_plugin_api::*` calls with native server calls.

---

## 2. Project layout and templates

### 2.1 Workspace

Start by copying `template/` to `<yourplugin>/` and renaming the package in its `Cargo.toml`. Then add it to `members` in the root `../Cargo.toml`, next to the other plugins:

```toml
members = [
    # ...server crates...
    "native-plugins/common",
    "native-plugins/template",
    "native-plugins/<yourplugin>",
]
```

PEX's workspace lints are strict (clippy `pedantic` and `nursery` are denied). Tests may use `unwrap` and `panic` through the line every PEX crate puts at the top of `src/lib.rs`:

```rust
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
```

### 2.2 `<yourplugin>/Cargo.toml`

```toml
[package]
name = "<yourplugin>"            # lowercase; becomes the plugin name, data folder and permission namespace
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license = "GPL-3.0"              # or the original plugin's license if you ported its code
description = "<one sentence>"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
pex-common = { path = "../common" }
# pumpkin-core, imported as `pumpkin::...` (cargo can't rename a `workspace = true` dependency).
pumpkin = { path = "../../crates/pumpkin-core", package = "pumpkin-core" }
pumpkin-data.workspace = true     # blocks, items, entity types, sounds
pumpkin-util.workspace = true     # BlockPos, Vector3, TextComponent, permissions
pumpkin-world.workspace = true    # only if you need BlockFlags
pumpkin-nbt.workspace = true      # only if you read or write NBT
serde.workspace = true
tracing.workspace = true

[lints]
workspace = true
```

### 2.3 `<yourplugin>/src/lib.rs` minimal template

Copy this, then fill it in. It compiles as-is.

```rust
use std::sync::Arc;

use pumpkin::plugin::{Context, Plugin, PluginFuture};

pex_common::export_plugin!(MyPlugin, "One sentence describing the plugin.");

pub struct MyPlugin;

impl MyPlugin {
    // `export_plugin!` calls this. Keep it cheap: no I/O, no server access.
    const fn new() -> Self {
        Self
    }
}

impl Plugin for MyPlugin {
    fn on_load(&self, context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            context.init_log(); // must be first, see §6.1
            let data_folder = context.get_data_folder(); // plugins/data/<name>/, created for you
            // 1. load config      (§4.1)
            // 2. register permissions (§3.3)
            // 3. register commands    (§3.1)
            // 4. register events      (§3.2)
            tracing::info!("{} loaded", env!("CARGO_PKG_NAME"));
            Ok(())
        })
    }

    fn on_unload(&self, _context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            // Save anything that must survive a restart.
            Ok(())
        })
    }
}
```

Returning `Err(String)` from `on_load` makes the server log the error and skip the plugin.

### 2.4 Shared state

Commands and event handlers are separate objects, so put shared state behind `Arc<Mutex<_>>` and clone the `Arc` into each one. Look at `worldpumpkin/src/commands.rs`, `struct Shared`.

```rust
#[derive(Clone)]
struct Shared {
    state: Arc<std::sync::Mutex<State>>,
}

impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
```

- Use `std::sync::Mutex`, not `tokio::sync::Mutex`. Command executors are synchronous.
- **Never hold a lock while calling into the server** if that call can fire an event that your own handler also locks: that deadlocks. Copy the data out, drop the guard, then call the server.

---

## 3. The three integration points

### 3.1 Commands

Commands are Brigadier-style trees with **synchronous** executors.

```rust
use pumpkin::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use pumpkin::command::argument_types::core::string::StringArgumentType;
use pumpkin::command::context::command_context::CommandContext;
use pumpkin::command::node::{CommandExecutor, CommandExecutorResult};

struct HelloCommand { shared: Shared }

impl CommandExecutor for HelloCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let source = &context.source;
        let _runtime = source.server().runtime.enter(); // §6.2
        // Optional argument: returns Err if it wasn't given.
        let text = StringArgumentType::get(context, "text").unwrap_or_default();
        source.send_message(pumpkin_util::text::TextComponent::text(format!("Hello {text}")));
        Ok(1) // 1 = success, 0 = handled failure
    }
}

// in on_load:
let node = command("hello", "Says hello")
    .executes(HelloCommand { shared: shared.clone() })
    .then(argument("text", StringArgumentType::GreedyPhrase)
        .executes(HelloCommand { shared: shared.clone() }));
context.register_command(node, "myplugin:command.hello");
```

Rules:

- `register_command(node, permission)` adds a permission requirement to the whole command. Pass the full node `"<plugin>:command.<name>"`; a node without `:` gets your plugin name prefixed automatically.
- **Executors may not block for long and may not `.await`.** For slow work (big world edits, database queries, HTTP), queue it and do it from the tick handler (§3.2) or a background task (§6.2).
- **WorldEdit-style `//cmd` commands: register both `"cmd"` and `"/cmd"`.** PEX strips one leading `/` from player commands before dispatching, so a player's `//cmd` arrives as `cmd` (the same as `/cmd`), while console input `/cmd` arrives unchanged. Registering only `"/cmd"` works from the console and silently fails for every player. Verified with a bot; see `worldpumpkin/src/commands.rs`, `register`. Make sure the plain name doesn't collide with another command.
- **Prefer one greedy string argument and parse it yourself** when the syntax is complex (patterns, flags, durations). That's what `worldpumpkin/` does. Brigadier argument types are strict and harder to get right.
- The command source can be a player, the console or a command block. Use `source.as_player()` (returns `Option<Arc<Player>>`) and handle `None` with a clear error. Never assume a player.
- Report errors to the user with `source.send_error(...)` or a red message, and return `Ok(0)`. Use `Err(...)` only for Brigadier syntax errors.
- Do not register a name that vanilla or PEX already uses (`give`, `tp`, `home` is fine, `set` without a slash is not WorldEdit's). Check `../crates/pumpkin-core/src/command/commands/` for taken names.

Useful `CommandSource` members (`pumpkin::command::context::command_source::CommandSource`):

| Need | Use |
| :-- | :-- |
| Player who ran it | `source.as_player() -> Option<Arc<Player>>` |
| Name (player name or `Server`) | `source.name` |
| World | `source.world() -> &Arc<World>` (always set; console gets the overworld) |
| Position / rotation | `source.position: Vector3<f64>`, `source.rotation: Vector2<f32>` (`x` = pitch, `y` = yaw) |
| Entity | `source.entity: Option<Arc<dyn EntityBase>>` |
| Permission check | `source.has_permission("myplugin:x")` |
| Reply | `source.send_message(TextComponent)`, `source.send_error(TextComponent)` |
| Server | `source.server() -> &Arc<Server>` |

### 3.2 Events

```rust
use pumpkin::plugin::{BoxFuture, EventHandler, EventPriority};
use pumpkin::plugin::block::block_break::BlockBreakEvent;
use pumpkin::server::Server;

struct BreakHandler { shared: Shared }

impl EventHandler<BlockBreakEvent> for BreakHandler {
    fn handle_blocking<'a>(&'a self, server: &'a Arc<Server>, event: &'a mut BlockBreakEvent)
        -> BoxFuture<'a, ()>
    {
        let _runtime = server.runtime.enter(); // see §6.2
        if should_block(&self.shared, event) {
            event.cancelled = true;
        }
        Box::pin(async {})
    }
}

// in on_load:
context.register_event::<BlockBreakEvent, _>(
    Arc::new(BreakHandler { shared: shared.clone() }),
    EventPriority::Normal,
    true, // blocking: REQUIRED if you cancel or modify the event
);
```

Rules:

- **Blocking (`true`) handlers get `&mut E` in `handle_blocking`** and can cancel or modify the event. Non-blocking (`false`) handlers get `&E` in `handle` and run afterwards; they can only observe. If you cancel, you must use blocking.
- Do the work **synchronously before** `Box::pin(async {})`. Only put work inside the async block if you really need `.await`, and then don't touch `&mut event` from it.
- Cancel with `event.cancelled = true` (every `#[cancellable]` event has a public `cancelled` field), or `pumpkin::plugin::Cancellable::set_cancelled`.
- Priorities: `Highest`, `High`, `Normal`, `Low`, `Lowest`. Protection plugins use `High` so they run before gameplay plugins.
- Event types live under `pumpkin::plugin::<category>::<file>::<EventName>`. Categories: `block`, `dialog`, `enchantment`, `entity`, `hanging`, `inventory`, `player`, `raid`, `server`, `vehicle`, `world`. Find the file with `ls ../crates/pumpkin-core/src/plugin/api/events/<category>/`.
- **Read the event struct before using it.** Field names differ from Bukkit, and several events are missing fields you'd expect. For example `BlockBreakEvent` has no world (use `event.player`'s world), and `BlockExplodeEvent` has neither a world nor a block list.
- **Check that the event is actually fired** (§3.4). 53 of PEX's 277 declared events are never fired.

**Per-tick work** is done by handling `pumpkin::plugin::server::server_tick_start::ServerTickStartEvent` (blocking). There is no other scheduler for native plugins. Keep each tick's work bounded; `worldpumpkin/src/lib.rs` processes at most `blocks_per_tick` blocks.

### 3.3 Permissions

```rust
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault};

context.register_permission(Permission::new(
    "myplugin:command.hello",            // MUST start with "<plugin name>:" or registration fails
    "Allows /hello",
    PermissionDefault::Op(PermissionLvl::Two), // or PermissionDefault::Allow / PermissionDefault::Deny
))?;
```

- Register every node before using it, in `on_load`.
- Players: `player.has_permission(&server, "myplugin:x")`. Command sources: `source.has_permission("myplugin:x")`.
- Bypass and admin nodes should default to `Deny` or `Op`, never `Allow`.

### 3.4 Which events fire

Before you rely on an event, check it is fired somewhere outside the event definitions:

```bash
grep -rlE "\bMyEvent(::new|\s*\{)" ../crates/pumpkin-core/src --include=*.rs | grep -v plugin/api/events
```

No output means **the event is never fired**. Your handler will register without error and never run. Appendix A lists every such event at `2009aa6d`.

If the plugin needs an event that isn't fired, do not patch PEX (rule 3). Either use a fired event that covers the case (table below), or implement the feature without it and say what's missing in your report.

---

## 4. Common building blocks

### 4.1 Config files

```rust
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)] // missing keys fall back to Default, so old configs keep working
struct Config { radius: u32, message: String }

impl Default for Config { fn default() -> Self { Self { radius: 16, message: "hi".into() } } }

let config: Config = pex_common::load_config(&data_folder, "config.toml");
```

`load_config` writes the defaults if the file is missing, and logs an error and uses the defaults if the file can't be parsed. It never panics. Validate value ranges yourself after loading.

For data that changes at runtime (regions, homes, logs), write your own file next to the config. Write to `<file>.tmp` and then rename it over the real file, so a crash mid-write can't destroy the data.

### 4.2 Chat messages

```rust
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::{NamedColor, RGBColor};

let msg = TextComponent::text("[MyPlugin] ").color_named(NamedColor::Gold)
    .add_child(TextComponent::text("Done.").color_named(NamedColor::Gray));
player.send_system_message(&msg);
```

Style methods take no arguments: `.bold()`, `.italic()`, `.color_rgb(RGBColor::new(r, g, b))`. `pex_common::Prefix` builds consistent `[Name] message` lines.

There is no MiniMessage or `&`-color-code parser. If the original plugin used color codes in its config, parse them yourself or drop them.

### 4.3 Players

| Bukkit | PEX native |
| :-- | :-- |
| `Bukkit.getOnlinePlayers()` | `server.get_all_players() -> Vec<Arc<Player>>` |
| `Bukkit.getPlayer(name)` | `server.get_player_by_name(name)` |
| `Bukkit.getPlayer(uuid)` | `server.get_player_by_uuid(uuid)` |
| `player.getUniqueId()` / `getName()` | `player.gameprofile.id` / `player.gameprofile.name` |
| `player.getWorld()` | `player.world() -> Arc<World>` |
| `player.getLocation()` | `player.position() -> Vector3<f64>`; yaw and pitch: `player.living_entity.entity.yaw.load()` and `.pitch.load()` |
| `player.sendMessage(..)` | `player.send_system_message(&TextComponent)` |
| action bar | `player.send_system_message_raw(&TextComponent, true)` |
| title | `player.show_title(&TextComponent, &pumpkin::entity::player::TitleMode::Title)` (also `SubTitle`, `ActionBar`), then `player.send_title_animation(fade_in, stay, fade_out)` |
| `player.teleport(loc)` | `player.request_teleport(pos, yaw, pitch)`; for another world use `player.teleport_world(..)` (async) |
| `player.kickPlayer(msg)` | `player.kick(DisconnectReason::Kicked, &TextComponent)` |
| `player.setHealth` / heal | `player.set_health(f32)`, `player.heal(f32)` |
| `player.getGameMode()` | `player.gamemode.load()` |
| `player.isSleeping()` | `player.sleeping_since.load().is_some()` |
| `player.getInventory()` | `player.inventory` (`Arc<PlayerInventory>`) |
| `player.hasPermission(node)` | `player.has_permission(&server, node)` |

Search `../crates/pumpkin-core/src/entity/player.rs` for `pub fn` before inventing a method name.

### 4.4 Worlds and blocks

| Bukkit | PEX native |
| :-- | :-- |
| `Bukkit.getWorlds()` | `server.worlds.load()` (a `Vec<Arc<World>>`) |
| world name | `world.get_world_name()` |
| `world.getMinHeight()` / max | `world.min_y` / `world.min_y + world.dimension.height - 1` |
| `world.getBlockAt(..).getType()` | `world.get_block_state(&pos)` (`&BlockState`) or `world.get_block_state_id(&pos)`; block: `pumpkin_data::Block::from_state_id(id)` |
| `block.setType(..)` | `world.set_block_state(&pos, state_id, BlockFlags::NOTIFY_ALL)` (`pumpkin_world::world::BlockFlags`) |
| break naturally | `world.break_block(&pos, None, BlockFlags::NOTIFY_ALL)` |
| highest block | `world.get_top_block(Vector2::new(x, z))` |
| time of day | `world.get_time_of_day()` / `world.set_time_of_day(i64)` |
| players in world | `world.players.load()` |
| raytrace | `world.raycast(start, end, \|pos, world\| !world.get_block_state(pos).is_air())` (see `worldpumpkin/src/host.rs`, `target_block`) |
| block entity (chest etc.) | `world.get_block_entity(&pos)`; save it with `entity.write_internal(&mut NbtCompound)`, restore with `world.add_block_entity_nbt(pos, &nbt)` |

Positions: `pumpkin_util::math::position::BlockPos::new(x, y, z)`, fields under `.0` (`pos.0.x`). Exact world positions: `pumpkin_util::math::vector3::Vector3<f64>`. Convert with `BlockPos::floored_v(vec)`.

Blocks by name: `pumpkin_data::Block::from_name("stone")` (with or without `minecraft:`). Block states with properties: `block.from_properties(&[("facing", "north")]).to_state_id(block)`. **That call panics on an unknown property value**, so wrap it in `std::panic::catch_unwind` exactly like `worldpumpkin/src/blocks.rs`, `resolve_block_state`.

`BlockFlags` cheat sheet: `NOTIFY_ALL` = normal placement (clients plus neighbour updates). `NOTIFY_LISTENERS` = clients only, no physics. Add `FORCE_STATE` to write even if unchanged. Add `SKIP_DROPS` so the replaced block drops nothing. Add `SKIP_BLOCK_ENTITY_REPLACED_CALLBACK` so a replaced chest doesn't spill its contents.

### 4.5 Bukkit concept → PEX equivalent

| Bukkit / Paper | PEX native |
| :-- | :-- |
| `JavaPlugin.onEnable` / `onDisable` | `Plugin::on_load` / `Plugin::on_unload` |
| `getDataFolder()` | `context.get_data_folder()` |
| `config.yml` | `config.toml` via `pex_common::load_config` |
| `getCommand(..).setExecutor` | `context.register_command(node, permission)` |
| `@EventHandler` + `registerEvents` | `context.register_event::<E, _>(Arc::new(handler), priority, blocking)` |
| `ignoreCancelled = true` | check `event.cancelled` at the top of your handler and return if set |
| `BukkitScheduler.runTaskTimer` | `ServerTickStartEvent` handler with a tick counter |
| `runTaskAsynchronously` | `std::thread::spawn` for blocking I/O (§6.2) |
| `PlayerCommandPreprocessEvent` | `pumpkin::plugin::player::player_command_send::PlayerCommandSendEvent` (fired before dispatch, cancellable, `command` has no leading `/`). The PEX type named `PlayerCommandPreprocessEvent` is **never fired**. |
| `AsyncPlayerChatEvent` | `pumpkin::plugin::player::player_chat::PlayerChatEvent`. The PEX `AsyncPlayerChatEvent` is **never fired**. |
| `PlayerQuitEvent` | `player::player_leave::PlayerLeaveEvent` |
| `PlayerLoginEvent` / pre-login | `player::player_login::PlayerLoginEvent` (fired). For pre-login use `player::async_player_pre_login::AsyncPlayerPreLoginEvent` (fired, Java edition only). `PlayerPreLoginEvent` is **never fired**. |
| `Vault` economy / permissions | No equivalent. Use plugin services: `context.register_service(name, Arc<T>).await` and `context.get_service::<T>(name).await` (both async, so call them inside `on_load`'s async block), but only between plugins built in this same workspace (§6.4). |
| `ProtocolLib` | `pumpkin::plugin::server::packet::*` packet events |
| `BukkitScheduler.runTaskLater(n)` | count ticks in a `ServerTickStartEvent` handler |
| `PersistentDataContainer` | No native equivalent. Keep your own data file keyed by UUID and position. |

---

## 5. Porting procedure

Follow these steps in order. Do not skip ahead.

1. **Inventory the original plugin.** List every command (with syntax), permission, config key, event listener, scheduled task and stored data file. Put the list in your notes. This is your definition of the feature set.
2. **Classify each item:**
   - **Direct:** a matching PEX API or fired event exists.
   - **Workaround:** possible with a different fired event or by polling in the tick handler. Write down the behaviour difference.
   - **Blocked:** needs an unfired event or a missing API. You'll skip it and report it.
3. **Decide the scope with the user if more than ~20% is blocked.** Otherwise continue with the direct and workaround items.
4. **Separate pure logic from server access.** Put everything that doesn't touch the server (parsing, region math, rule evaluation, undo history) in plain Rust modules with unit tests. Put every server call behind a small adapter: a struct or trait like `worldpumpkin/src/host.rs` (`World`, `Actor`) or `BlockAccess` in `worldpumpkin/src/engine/writer.rs`. Tests then run against an in-memory fake.
5. **Write the plugin from the template in §2**, adding one feature at a time. Run `cargo build -p <plugin>` after each feature. Don't write 1,000 lines and then compile.
6. **Port tests.** If the original has tests, port the ones for logic you kept. Add tests for parsing and rule evaluation. Don't write tests that just restate constants.
7. **Run the full check (§8).** Fix everything.
8. **Write the report (§9).**

When porting from **Pumpkin Wasm** (`pumpkin_plugin_api`): the logic usually carries over unchanged. Replace the host calls (`world.get_block_state_id`, `player.get_name`, `context.register_command`, `register_event_handler`, `schedule_repeating_task` and so on) with the native equivalents above, and replace `register_plugin!(T)` with `pex_common::export_plugin!`. `worldpumpkin/` is a worked example; compare it with upstream WorldPumpkin to see each replacement.

When porting from **Bukkit/Paper (Java)**: you are rewriting, not translating. Keep the command syntax, permission names (re-namespaced to `<plugin>:`), config keys and user-visible messages the same where you can, so server owners can switch easily. Don't port Java patterns like class hierarchies, listener reflection, or YAML config objects.

---

## 6. Native-specific pitfalls (verified)

### 6.1 Logging needs `context.init_log()`

The plugin has its own copy of `tracing`, with no subscriber until you call `context.init_log()`. Call it once, first thing in `on_load`. After that, `tracing::info!` and friends show up in the server log under your crate name. Before it, log lines are silently dropped.

### 6.2 Tokio: enter the server's runtime, and don't use Tokio APIs yourself

Your plugin has its own copy of Tokio, whose "current runtime" thread-local is empty even when the code runs on a server thread. Any Tokio call compiled into *your* plugin (including server functions you call, because they're compiled into your plugin too) that needs a current runtime (`tokio::spawn`, `tokio::time::sleep`, `Handle::current()`) will panic unless you enter the server's runtime first.

Do this at the top of every event handler, tick handler and command executor that calls server code:

```rust
let _runtime = server.runtime.enter(); // server: &Arc<Server>; keep the guard alive for the whole scope
```

(In a command executor: `let _runtime = context.source.server().runtime.enter();`.) `worldpumpkin/` does this in its tick handler. Treat it as mandatory even where it hasn't caused a crash yet: the server functions that spawn tasks aren't documented, so you can't know which calls need it.

Rules that follow from this:

- **Don't call `tokio::spawn`, `tokio::time::*` or `Handle::current()` in plugin code.** A future you spawn onto `server.runtime` is polled on server threads without your copy's context. The enter guard is `!Send`, so you can't hold it across an `.await` to work around that.
- **For delays and repeating work, count ticks** in a `ServerTickStartEvent` handler (20 ticks = 1 second).
- **For blocking work** (file writes, a SQLite database, HTTP), use `std::thread::spawn` or a long-lived worker thread fed through a `std::sync::mpsc` channel. Don't call server functions from that thread; send results back and apply them in the tick handler.
- **Never create your own `tokio::runtime::Runtime`.**

### 6.3 No stable ABI

Changing PEX (even one line in `pumpkin-core`), changing `Cargo.lock`, or changing the Rust version means **every native plugin must be rebuilt**. A mismatched plugin is not rejected; it misbehaves at runtime. When the user updates PEX, rebuild all plugins in this workspace against the new checkout.

### 6.4 Plugins can't safely share types

Two plugins each have their own copy of every type. Passing your own struct between two plugin dylibs through services is only safe when both were built in this workspace from the same source. For cross-plugin calls, prefer IPC with plain bytes: send with `context.plugin_manager.send_message(sender_name, recipient_name, &bytes).await`, and receive by implementing `Plugin::on_ipc_message`.

### 6.5 Command and console quirks

- Players and the console see command names differently. A player typing `//pos1` runs the command named `pos1`. In the console, `/pos1` runs the command named `/pos1` and `pos1` runs `pos1`. Register both spellings (§3.1) and test from both a player and the console.
- The console's world is the overworld and its position is the world spawn.
- `say` output does not appear in the server console, and `execute if block ...` without `run` is not implemented. Neither works as a test probe (§8).

### 6.6 Cancelling movement: don't teleport the player yourself

**When a `PlayerMoveEvent` is cancelled, PEX already teleports the player back** (`force_tp` in `net/java/play/player_position.rs`). Just set `event.cancelled = true`. Don't call `request_teleport` as well: like vanilla, PEX only waits for the newest teleport (`player.awaiting_teleport`) and ignores the client's confirmation of an older one, so your teleport replaces the one PEX just sent and the player rubber-bands twice. Older PEX builds kicked the player ("Wrong teleport id") when that older confirmation arrived; Guard's first version did both and its players were kicked within a second. Cancelling alone works (`pexguard/src/listeners.rs`, `MoveHandler`).

The same rule applies to any code that teleports in reaction to movement (freezing, anti-cheat, warps on step). Check `player.awaiting_teleport` first and skip the teleport while one is pending:

```rust
let pending = player.awaiting_teleport.lock().unwrap_or_else(std::sync::PoisonError::into_inner).position.is_some();
if !pending {
    player.request_teleport(pos, yaw, pitch);
}
```

PEX also ignores all movement packets while a teleport is pending, so your `PlayerMoveEvent` handler won't run during that time.

### 6.7 Events missing data

Some fired events lack fields you'd expect. Check the struct before designing around it:

| Event | Missing | Workaround |
| :-- | :-- | :-- |
| `BlockBreakEvent` | world | `event.player.as_ref().map(|p| p.world())`; if `player` is `None`, decide whether to allow or skip |
| `BlockExplodeEvent` | world, affected block list | can only cancel the whole explosion; test the position against every world, or allow |
| `EntityExplodeEvent` | world, affected block list; **`entity_id` is always `0`** (PEX passes a literal 0, `world/mod.rs` `run_explosion`) | don't look up the entity. Check `position` in every world. To log what was destroyed, snapshot the area in each world where the chunk is loaded and diff it next tick (`pexprotect/src/lib.rs`, `LogExplode`) |
| `EntityDamageByEntityEvent` | entity references | `entity_id` and `damager_id` only; look both up in the world |
| `BlockFromToEvent`, `BlockIgniteEvent` | world | test the position in every world, or allow |

---

## 7. Data and performance rules

- **Bound everything:** command argument lengths, selection volumes, list sizes in config, file sizes you read, queue lengths. `worldpumpkin/` shows limits per edit, per tick and per queue.
- **Never do a large world edit inside a command executor.** Queue it and process a fixed budget per tick.
- **Never do file or network I/O inside event handlers on hot events** (`PlayerMoveEvent`, `BlockBreakEvent`, chunk events). Buffer in memory and flush from a background task or every N ticks.
- `PlayerMoveEvent` fires many times per second per player. Compare block coordinates and return early if the player didn't change blocks.
- Hold locks for as short a time as possible (§2.4).

---

## 8. Testing (mandatory)

Run all of these and report the output.

### 8.1 Static checks

Run these from the repository root:

```bash
cargo build -p <plugin>
cargo clippy --no-deps -p <plugin> --all-targets -- -D warnings
cargo test -p <plugin>
cargo fmt --check
```

`--no-deps` keeps clippy to your crate. On Windows, `pumpkin-core` has a pre-existing `missing_const_for_fn` hit in `server/memory.rs` that would otherwise stop clippy before it reaches your plugin.

### 8.2 Boot test

1. Build the server and the plugin together: `cargo build -p pumpkin -p <plugin>` from the repository root.
2. Make a scratch server folder outside the repo and copy the plugin library into `<server>/plugins/`: `../target/debug/<plugin>.dll` on Windows, `lib<plugin>.so` on Linux, `lib<plugin>.dylib` on macOS.
3. Start the server with `tools/server_driver.py`, which forwards lines you append to a commands file to the server's stdin:
   `python tools/server_driver.py <path-to-pumpkin.exe> <server-dir> <cmds.txt> <log.txt>`
4. Wait for `Loaded <plugin>` and `Started server` in the log. `Failed to load plugin` or any panic is a failure.
5. Append test commands to `cmds.txt`, one per line, and read the log.
6. Finish with `stop`, then search the log for `ERROR`, `WARN` and `panicked` lines that mention your plugin.

### 8.3 Verifying world changes from the console

Chunks without players aren't loaded, so run `forceload add <x> <z>` first.

To check what blocks are in a box, use a no-op `fill` with `replace`. It reports how many blocks matched:

```
fill 0 100 0 4 104 4 stone replace stone
```

- `Successfully filled N block(s)`: N stone blocks are in the box.
- `No blocks were filled`: none.

This was used to verify `worldpumpkin/`: after `//set 50%stone,50%oak_planks`, `//walls glass` and `//replace stone diamond_block`, the counts were 80 glass, 14 diamond, 31 planks and 0 stone, which add up to the 125-block box.

### 8.4 In-game tests with bots

Anything that needs a player (player events, interacting, line of sight, movement, chat, permissions) must be tested with `tools/testbot`, which runs scripted players. Follow its README for setup and its "Lessons" list. In short:

- Start bots with `tools/testbot/target/debug/pex-testbot 127.0.0.1:25565 bots.txt Alice Bob > bots.log`.
- Drive them by appending lines like `Alice chat //pos1`, `Bob mine 106 99 105`, `Bob attack Alice` or `Bob walk forward 3000` to `bots.txt`.
- Position them with console `tp`, wait about 3 s, act, then check blocks with the `fill ... replace` probe (§8.3).
- Test both the allowed case and the denied case (for example an owner and a non-member, an op and a non-op).

Bot testing found three bugs that every console test had missed: `//` commands that only worked from the console, a teleport that got players kicked, and spawn protection hiding a protection rule. **A plugin that touches players isn't done until a bot has exercised it.**

If something can't be tested with a bot (rendering, sounds, GUIs, client prediction), list it in your report as "needs in-game test", with exact steps: what to do and what should happen.

## 9. Report format

End your work with a report containing:

1. **What was ported:** each command, permission, config key and event with its status: working (tested how), untested (why), or blocked (which missing event or API).
2. **Behaviour differences** from the original plugin.
3. **Test output:** build, clippy and test summaries, plus the boot-test log lines that matter.
4. **In-game test steps** for the user, in "do X, expect Y" form.
5. **PEX gaps found:** unfired events or missing APIs you needed. Don't patch PEX yourself.

---

## 10. Done checklist

- [ ] Builds with the workspace `Cargo.lock` unchanged except for the new crate's own entry
- [ ] No `unwrap`, `expect`, `panic!`, `println!`; clippy clean with `-D warnings`
- [ ] `context.init_log()` called first in `on_load`
- [ ] Server runtime entered in every event handler that calls server code
- [ ] Every permission registered with the `<plugin>:` prefix and a sensible default
- [ ] Every event handler that cancels or modifies is registered with `blocking = true`
- [ ] Every event used was checked as fired (§3.4)
- [ ] Config uses `#[serde(default)]` and is validated after loading
- [ ] Persistent data is written atomically (tmp file plus rename)
- [ ] No unbounded loops, lists or world edits in commands or hot events
- [ ] Unit tests for the pure logic pass
- [ ] Boot test: plugin loads, console commands behave, no errors in the log
- [ ] Original license and credit kept if code was ported
- [ ] Report written (§9)

---

## Appendix A: events declared but never fired (PEX `2009aa6d`)

Registering a handler for any of these succeeds, and the handler never runs:

```
AreaEffectCloudApplyEvent ArrowBodyCountChangeEvent AsyncPlayerChatEvent BatToggleSleepEvent
BellResonateEvent BlockDispenseArmorEvent BlockDispenseLootEvent BlockFadeEvent BlockMultiPlaceEvent
BlockReceiveGameEvent BlockShearEntityEvent CreeperPowerEvent DialogClearEvent DialogShowEvent
EnderDragonChangePhaseEvent EntityBlockFormEvent EntityCombustByBlockEvent EntityCombustByEntityEvent
EntityPortalEnterEvent EntityPortalExitEvent EntityRemoveEvent EntitySpellCastEvent
EntityTargetBlockEvent EntityTargetLivingEntityEvent ExpBottleEvent FluidLevelChangeEvent
HangingBreakByEntityEvent HangingBreakEvent HangingPlaceEvent HorseJumpEvent InventoryBlockStartEvent
MapInitializeEvent PigZapEvent PigZombieAngerEvent PlayerArmorStandManipulateEvent
PlayerCommandPreprocessEvent PlayerExpCooldownChangeEvent PlayerHideEntityEvent PlayerNameEntityEvent
PlayerPortalEvent PlayerPreLoginEvent PlayerShowEntityEvent PlayerTakeLecternBookEvent SculkBloomEvent
SheepDyeWoolEvent SheepRegrowWoolEvent SlimeSplitEvent StriderTemperatureChangeEvent
VaultDisplayItemEvent VillagerAcquireTradeEvent VillagerCareerChangeEvent VillagerReplenishTradeEvent
VillagerReputationChangeEvent
```

If PEX has moved past `2009aa6d`, regenerate this list with the command in §3.4 before trusting it.

## Appendix B: fired events useful for common plugin types

| Plugin type | Events (all fired) |
| :-- | :-- |
| Protection (WorldGuard, claims) | `block::block_break::BlockBreakEvent`, `block::block_place::BlockPlaceEvent`, `player::player_interact_event::PlayerInteractEvent`, `player::player_interact_entity_event::PlayerInteractEntityEvent`, `entity::entity_damage_by_entity::EntityDamageByEntityEvent`, `player::player_bucket::{PlayerBucketEmptyEvent, PlayerBucketFillEvent}`, `block::block_spread::BlockSpreadEvent`, `block::block_burn::BlockBurnEvent`, `block::block_ignite::BlockIgniteEvent`, `block::block_from_to::BlockFromToEvent`, `block::block_piston::*`, `block::block_explode::BlockExplodeEvent`, `entity::entity_explode::EntityExplodeEvent`, `entity::entity_change_block::EntityChangeBlockEvent`, `entity::creature_spawn::CreatureSpawnEvent`, `player::player_move::PlayerMoveEvent`, `inventory::inventory_open::InventoryOpenEvent` |
| Logging / rollback (CoreProtect) | the same block events, plus `inventory::inventory_interact::*` for container access |
| Chat / commands | `player::player_chat::PlayerChatEvent`, `player::player_command_send::PlayerCommandSendEvent` |
| Join / leave / sessions | `player::player_join::PlayerJoinEvent`, `player::player_leave::PlayerLeaveEvent`, `player::player_login::PlayerLoginEvent`, `player::async_player_pre_login::AsyncPlayerPreLoginEvent` |
| Sleep / time | `player::player_bed::{PlayerBedEnterEvent, PlayerBedLeaveEvent}`, `world::time_skip::TimeSkipEvent`, `server::server_tick_start::ServerTickStartEvent` |
| Teleport / homes | `player::player_teleport::PlayerTeleportEvent`, `player::player_respawn::PlayerRespawnEvent`, `entity::entity_death::EntityDeathEvent` |

Always open the event file and confirm the exact type name and fields before using it.
