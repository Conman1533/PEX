// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

mod history;
mod move_command;
mod pos;
mod selection;

use crate::{
    blocks::{BlockPattern, parse_block_pattern, parse_block_state},
    config::{
        Config, PERM_LIMIT_BYPASS, PERM_MOVE, PERM_POS, PERM_REDO, PERM_RELOAD, PERM_REPLACE,
        PERM_SET, PERM_STATUS, PERM_UNDO, PERM_WALLS,
    },
    engine::{BlockPos, Cuboid, EditOperation, EditQueue, ReplayDirection, Selection},
    host::{Actor, World},
    state::{PluginState, SelectionSlot},
};
use pumpkin::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use pumpkin::command::argument_types::core::string::StringArgumentType;
use pumpkin::command::context::command_context::CommandContext;
use pumpkin::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin::plugin::Context;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

const ARG_ARGS: &str = "args";

/// State every command needs: selections and history, the edit queue, and the config folder.
#[derive(Clone)]
pub struct Shared {
    pub state: Arc<Mutex<PluginState>>,
    pub queue: Arc<Mutex<EditQueue>>,
    pub data_folder: PathBuf,
}

impl Shared {
    pub fn state(&self) -> MutexGuard<'_, PluginState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn queue(&self) -> MutexGuard<'_, EditQueue> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Every `//` command, its permission and its description.
const COMMANDS: &[(&str, &str, &str)] = &[
    ("pos1", PERM_POS, "Sets the first selection position"),
    ("pos2", PERM_POS, "Sets the second selection position"),
    (
        "hpos1",
        PERM_POS,
        "Sets the first position to the targeted block",
    ),
    (
        "hpos2",
        PERM_POS,
        "Sets the second position to the targeted block",
    ),
    ("chunk", PERM_POS, "Selects the current chunk"),
    ("expand", PERM_POS, "Expands the current selection"),
    ("set", PERM_SET, "Fills the selection with a block pattern"),
    (
        "replace",
        PERM_REPLACE,
        "Replaces one block with a pattern in the selection",
    ),
    ("walls", PERM_WALLS, "Builds walls around the selection"),
    ("move", PERM_MOVE, "Moves the selection's blocks"),
    ("undo", PERM_UNDO, "Undoes your latest edit"),
    ("redo", PERM_REDO, "Redoes your latest undone edit"),
];

/// Registers the `//` commands and `/wp`.
///
/// Each takes one greedy argument and is parsed
/// by [`dispatch`], so patterns like `50%stone,oak_stairs[half=top]` reach the plugin intact.
pub fn register(context: &Context, shared: &Shared) {
    for &(name, permission, description) in COMMANDS {
        // PEX strips one leading `/` from player commands, so a player's `//pos1` arrives as
        // `pos1`, while the console's `/pos1` arrives unchanged. Register both spellings.
        for literal_name in [format!("/{name}"), name.to_owned()] {
            let executor = TextCommand {
                name,
                shared: shared.clone(),
            };
            let node = command(literal_name, description)
                .executes(executor.clone())
                .then(argument(ARG_ARGS, StringArgumentType::GreedyPhrase).executes(executor));
            context.register_command(node, permission);
        }
    }

    let wp = command("wp", "WorldPumpkin info, status and config reload")
        .executes(WpCommand::Info)
        .then(literal("info").executes(WpCommand::Info))
        .then(literal("status").executes(WpCommand::Status(shared.clone())))
        .then(literal("reload").executes(WpCommand::Reload(shared.clone())));
    context.register_command(wp, PERM_STATUS);
}

#[derive(Clone)]
struct TextCommand {
    name: &'static str,
    shared: Shared,
}

impl CommandExecutor for TextCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let _runtime = context.source.server().runtime.enter();
        let args = StringArgumentType::get(context, ARG_ARGS).unwrap_or_default();
        let actor = Actor::new(&context.source);
        match dispatch(&actor, self.name, args, &self.shared) {
            Ok(()) => Ok(1),
            Err(message) => {
                actor.send_error(&message);
                Ok(0)
            }
        }
    }
}

/// Runs one `//` command. Permissions were already checked by the command's requirement.
fn dispatch(actor: &Actor<'_>, name: &str, args: &str, shared: &Shared) -> Result<(), String> {
    let mut parts = args.split_whitespace();
    match name {
        "pos1" => pos::handle(actor, shared, SelectionSlot::Pos1, args),
        "pos2" => pos::handle(actor, shared, SelectionSlot::Pos2, args),
        "hpos1" => {
            ensure_no_extra_args(parts)?;
            selection::handle_hpos(actor, shared, SelectionSlot::Pos1)
        }
        "hpos2" => {
            ensure_no_extra_args(parts)?;
            selection::handle_hpos(actor, shared, SelectionSlot::Pos2)
        }
        "chunk" => {
            ensure_no_extra_args(parts)?;
            selection::handle_chunk(actor, shared);
            Ok(())
        }
        "expand" => selection::handle_expand(actor, shared, args),
        "set" => {
            let to = parse_required_pattern(&mut parts, "Usage: //set <pattern>")?;
            queue_edit(actor, shared, |owner, world, cuboid| {
                EditOperation::set(owner, world, cuboid, to, Arc::clone(&shared.state))
            })
        }
        "replace" => {
            let from = parts
                .next()
                .ok_or_else(|| "Usage: //replace <from> <to>".to_owned())
                .and_then(parse_block_state)?;
            let to = parse_required_pattern(&mut parts, "Usage: //replace <from> <to>")?;
            queue_edit(actor, shared, |owner, world, cuboid| {
                EditOperation::replace(owner, world, cuboid, from, to, Arc::clone(&shared.state))
            })
        }
        "walls" => {
            let to = parse_required_pattern(&mut parts, "Usage: //walls <pattern>")?;
            let (owner, world, cuboid) = selection_context(actor, shared)?;
            let blocks = cuboid.wall_volume();
            enqueue(actor, shared, blocks, || {
                EditOperation::walls(owner, world, cuboid, to, Arc::clone(&shared.state))
            })
        }
        "move" => move_command::handle(actor, shared, args),
        "undo" => {
            ensure_no_extra_args(parts)?;
            history::handle(actor, shared, ReplayDirection::Undo)
        }
        "redo" => {
            ensure_no_extra_args(parts)?;
            history::handle(actor, shared, ReplayDirection::Redo)
        }
        _ => Err(format!("Unknown command //{name}.")),
    }
}

enum WpCommand {
    Info,
    Status(Shared),
    Reload(Shared),
}

impl CommandExecutor for WpCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let _runtime = context.source.server().runtime.enter();
        let actor = Actor::new(&context.source);
        match self {
            Self::Info => actor.send_ok(&format!(
                "WorldPumpkin {} (native PEX port). Commands: {}",
                env!("CARGO_PKG_VERSION"),
                COMMANDS
                    .iter()
                    .map(|(name, ..)| format!("//{name}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )),
            Self::Status(shared) => {
                let (operations, blocks) = {
                    let queue = shared.queue();
                    (queue.len(), queue.queued_blocks())
                };
                let state = shared.state();
                let config = state.config();
                actor.send_ok(&format!(
                    "Queue: {operations} edits, {blocks} blocks. Limits: {} per edit, {} per tick. Fast mode {}.",
                    config.max_blocks_per_operation,
                    config.blocks_per_tick,
                    enabled_text(config.fast_mode)
                ));
            }
            Self::Reload(shared) => {
                if !actor.has_permission(PERM_RELOAD) {
                    actor.send_error("You don't have permission for that.");
                    return Ok(0);
                }
                let config = Config::load(&shared.data_folder);
                shared.state().replace_config(config);
                actor.send_ok("Config reloaded.");
            }
        }
        Ok(1)
    }
}

fn parse_required_pattern<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    usage: &str,
) -> Result<BlockPattern, String> {
    let block = parts.next().ok_or_else(|| usage.to_owned())?;
    ensure_no_extra_args(parts)?;
    parse_block_pattern(block)
}

fn queue_edit(
    actor: &Actor<'_>,
    shared: &Shared,
    operation: impl FnOnce(String, World, Cuboid) -> EditOperation,
) -> Result<(), String> {
    let (owner, world, cuboid) = selection_context(actor, shared)?;
    enqueue(actor, shared, cuboid.volume(), || {
        operation(owner, world, cuboid)
    })
}

/// Checks the per-edit limit and queue capacity, then queues the edit and confirms it.
fn enqueue(
    actor: &Actor<'_>,
    shared: &Shared,
    blocks: u64,
    operation: impl FnOnce() -> EditOperation,
) -> Result<(), String> {
    enforce_limit(actor, shared, blocks)?;
    let config = shared.state().config().clone();
    let mut queue = shared.queue();
    queue.can_enqueue(blocks, &config)?;
    queue.enqueue(operation());
    actor.send_ok(&queued_message(blocks));
    Ok(())
}

/// Stores an endpoint in the actor's current world and sends the updated selection summary.
pub fn set_pos(actor: &Actor<'_>, shared: &Shared, slot: SelectionSlot, pos: BlockPos) {
    let selection = shared
        .state()
        .set_position(actor.name(), actor.world().get_id(), slot, pos);
    actor.send_ok(&selection_message(selection));
}

/// Returns the owner ID, world and complete selection, rejecting a selection from another world.
pub fn selection_context(
    actor: &Actor<'_>,
    shared: &Shared,
) -> Result<(String, World, Cuboid), String> {
    let owner = actor.name();
    let world = actor.world();
    let cuboid = shared
        .state()
        .selection(&owner, &world.get_id())
        .and_then(Selection::cuboid)
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    Ok((owner, world, cuboid))
}

pub fn enforce_limit(actor: &Actor<'_>, shared: &Shared, volume: u64) -> Result<(), String> {
    let max = shared.state().config().max_blocks_per_operation;
    if volume <= max || actor.has_permission(PERM_LIMIT_BYPASS) {
        return Ok(());
    }
    Err(format!(
        "Selection is too large: {volume} blocks, limit is {max}."
    ))
}

pub fn ensure_no_extra_args<'a>(mut parts: impl Iterator<Item = &'a str>) -> Result<(), String> {
    if parts.next().is_some() {
        Err("Too many arguments.".to_owned())
    } else {
        Ok(())
    }
}

pub fn selection_message(selection: Selection) -> String {
    let ends = format!(
        "Selection: {} -> {}",
        format_selection_pos(selection.pos1),
        format_selection_pos(selection.pos2)
    );
    selection.cuboid().map_or_else(
        || ends.clone(),
        |cuboid| format!("{ends} ({} blocks)", cuboid.volume()),
    )
}

fn format_selection_pos(pos: Option<BlockPos>) -> String {
    pos.map_or_else(
        || "-".to_owned(),
        |pos| format!("{}, {}, {}", pos.x, pos.y, pos.z),
    )
}

pub fn queued_message(blocks: u64) -> String {
    format!("Queued {blocks} blocks.")
}

const fn enabled_text(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}
