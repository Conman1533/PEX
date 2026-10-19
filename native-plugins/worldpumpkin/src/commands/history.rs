// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::Shared;
use crate::{
    engine::{EditOperation, ReplayDirection},
    host::{Actor, World},
};
use std::sync::Arc;

/// Queues the actor's latest undo or redo entry and reports the admitted block count.
pub(super) fn handle(
    actor: &Actor<'_>,
    shared: &Shared,
    direction: ReplayDirection,
) -> Result<(), String> {
    let blocks = enqueue(actor.name(), actor.world(), shared, direction)?;
    actor.send_ok(&message(direction, blocks));
    Ok(())
}

/// Validates world and queue limits before removing and queuing the latest history entry.
///
/// Locks the queue before state so admission and history removal are atomic.
/// Returns the queued block count, or an error without consuming history.
fn enqueue(
    owner: String,
    world: World,
    shared: &Shared,
    direction: ReplayDirection,
) -> Result<u64, String> {
    let world_id = world.get_id();
    // Tick processing uses the same lock order. Admission and history removal are atomic.
    let mut queue = shared.queue();
    let mut state_guard = shared.state();
    let empty = match direction {
        ReplayDirection::Undo => "Nothing to undo.",
        ReplayDirection::Redo => "Nothing to redo.",
    };
    let info = match direction {
        ReplayDirection::Undo => state_guard.latest_undo_history(&owner),
        ReplayDirection::Redo => state_guard.latest_redo_history(&owner),
    }
    .ok_or(empty)?;
    if info.world_id != world_id {
        return Err("That edit was made in another world.".to_owned());
    }
    let blocks = info.blocks as u64;
    queue.can_enqueue(blocks, state_guard.config())?;
    let history = match direction {
        ReplayDirection::Undo => state_guard.pop_undo_history(&owner),
        ReplayDirection::Redo => state_guard.pop_redo_history(&owner),
    }
    .ok_or(empty)?;
    drop(state_guard);
    queue.enqueue(EditOperation::replay(
        owner,
        world,
        history,
        direction,
        Arc::clone(&shared.state),
    ));
    Ok(blocks)
}

/// Formats the queue confirmation for the selected replay direction.
fn message(direction: ReplayDirection, blocks: u64) -> String {
    match direction {
        ReplayDirection::Undo => format!("Undo queued ({blocks} blocks)."),
        ReplayDirection::Redo => format!("Redo queued ({blocks} blocks)."),
    }
}
