// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    Shared, enforce_limit,
    selection::{Direction, actor_direction, parse_direction},
    selection_context,
};
use crate::{engine::EditOperation, host::Actor};
use std::sync::Arc;

pub(super) fn handle(actor: &Actor<'_>, shared: &Shared, args: &str) -> Result<(), String> {
    let request = parse_request(args, || actor_direction(actor))?;
    let (owner, world, cuboid) = selection_context(actor, shared)?;
    let offset = request.direction.offset(request.amount);
    cuboid
        .translated(offset)
        .ok_or_else(|| "Move destination is outside the supported coordinate range.".to_owned())?;
    enforce_limit(actor, shared, cuboid.volume())?;

    let config = shared.state().config().clone();
    let mut queue = shared.queue();
    queue.can_enqueue(cuboid.volume(), &config)?;
    queue.enqueue(EditOperation::move_blocks(
        owner,
        world,
        cuboid,
        offset,
        Arc::clone(&shared.state),
    ));
    actor.send_ok(&format!("Move queued ({} blocks).", cuboid.volume()));
    Ok(())
}

struct MoveRequest {
    amount: i32,
    direction: Direction,
}

fn parse_request(
    args: &str,
    default_direction: impl FnOnce() -> Direction,
) -> Result<MoveRequest, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 2 {
        return Err("Usage: //move <amount> [direction]".to_owned());
    }

    let amount = parts[0]
        .parse::<i32>()
        .map_err(|err| format!("failed to parse move amount `{}`: {err}", parts[0]))?;
    if amount < 0 {
        return Err("Move amount must not be negative.".to_owned());
    }
    let direction = parts
        .get(1)
        .map_or_else(|| Ok(default_direction()), |input| parse_direction(input))?;
    Ok(MoveRequest { amount, direction })
}
