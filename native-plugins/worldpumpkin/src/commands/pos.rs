// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{Shared, set_pos};
use crate::{engine::BlockPos, host::Actor, state::SelectionSlot};

/// Sets an endpoint from absolute or `~`-relative coordinates, or the actor's own position.
pub(super) fn handle(
    actor: &Actor<'_>,
    shared: &Shared,
    slot: SelectionSlot,
    args: &str,
) -> Result<(), String> {
    let pos = parse_position(args, actor.block_pos())?;
    set_pos(actor, shared, slot, pos);
    Ok(())
}

/// Parses an absolute coordinate or a checked `~` offset from the current coordinate.
fn coordinate(input: &str, current: i32) -> Result<i32, String> {
    if let Some(offset) = input.strip_prefix('~') {
        let offset = if offset.is_empty() {
            0
        } else {
            offset
                .parse::<i32>()
                .map_err(|_| format!("invalid coordinate `{input}`"))?
        };
        current
            .checked_add(offset)
            .ok_or_else(|| "coordinate is out of range".to_owned())
    } else {
        input
            .parse::<i32>()
            .map_err(|_| format!("invalid coordinate `{input}`"))
    }
}

/// Parses three integer coordinates with optional `~` offsets, or uses the current position.
///
/// Rejects extra or missing coordinates, invalid integers, and relative coordinate overflow.
pub(super) fn parse_position(args: &str, current: BlockPos) -> Result<BlockPos, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return Ok(current);
    }
    if parts.len() != 3 {
        return Err("Usage: //pos1 or //pos2 [x y z]".to_owned());
    }
    Ok(BlockPos {
        x: coordinate(parts[0], current.x)?,
        y: coordinate(parts[1], current.y)?,
        z: coordinate(parts[2], current.z)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Checks absolute and relative positions, the empty-input default, and invalid coordinates.
    #[test]
    fn positions_accept_explicit_and_relative_coordinates() {
        let current = BlockPos {
            x: 10,
            y: 64,
            z: -5,
        };
        assert_eq!(parse_position("", current).unwrap(), current);
        assert_eq!(
            parse_position("1 2 3", current).unwrap(),
            BlockPos { x: 1, y: 2, z: 3 }
        );
        assert_eq!(
            parse_position("~ ~10 ~-2", current).unwrap(),
            BlockPos {
                x: 10,
                y: 74,
                z: -7
            }
        );
        for input in ["1 2", "1 2 3 4", "bad 2 3", "~2147483647 ~ ~"] {
            assert!(parse_position(input, current).is_err(), "{input}");
        }
    }
}
