// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{Shared, selection_message, set_pos};
use crate::{
    engine::{BlockPos, Selection},
    host::{Actor, World},
    state::SelectionSlot,
};

const HPOS_RANGE: f64 = 300.0;

/// Selects the actor's chunk from the world's bottom to the highest block in it.
pub(super) fn handle_chunk(actor: &Actor<'_>, shared: &Shared) {
    let pos = actor.block_pos();
    let world = actor.world();
    let min = BlockPos {
        x: pos.x.div_euclid(16) * 16,
        y: world.get_min_y(),
        z: pos.z.div_euclid(16) * 16,
    };
    let max = BlockPos {
        x: min.x + 15,
        y: chunk_top_y(&world, min),
        z: min.z + 15,
    };
    let selection = {
        let mut state = shared.state();
        let world_id = world.get_id();
        state.set_position(actor.name(), world_id.clone(), SelectionSlot::Pos1, min);
        state.set_position(actor.name(), world_id, SelectionSlot::Pos2, max)
    };
    actor.send_ok(&selection_message(selection));
}

pub(super) fn handle_hpos(
    actor: &Actor<'_>,
    shared: &Shared,
    slot: SelectionSlot,
) -> Result<(), String> {
    let pos = actor
        .target_block(HPOS_RANGE)
        .ok_or_else(|| "No block in sight.".to_owned())?;
    set_pos(actor, shared, slot, pos);
    Ok(())
}

pub(super) fn handle_expand(actor: &Actor<'_>, shared: &Shared, args: &str) -> Result<(), String> {
    let request = parse_expand(args, || actor_direction(actor))?;
    let selection = expand_selection(actor.name(), shared, request, &actor.world())?;
    actor.send_ok(&selection_message(selection));
    Ok(())
}

fn expand_selection(
    owner: String,
    shared: &Shared,
    request: ExpandRequest,
    world: &World,
) -> Result<Selection, String> {
    let world_id = world.get_id();
    let mut state = shared.state();
    let selection = state
        .selection(&owner, &world_id)
        .ok_or_else(|| "Select two positions first.".to_owned())?;
    let mut cuboid = ExpandedCuboid::from_selection(selection)?;
    match request {
        ExpandRequest::Directional {
            amount,
            reverse_amount,
            direction,
        } => {
            cuboid.expand(direction, amount);
            if reverse_amount != 0 {
                cuboid.expand(direction.opposite(), reverse_amount);
            }
        }
        ExpandRequest::Vertical => {
            cuboid.min.y = world.get_min_y();
            cuboid.max.y = world.get_max_y();
        }
    }
    state.set_position(
        owner.clone(),
        world_id.clone(),
        SelectionSlot::Pos1,
        cuboid.min,
    );
    Ok(state.set_position(owner, world_id, SelectionSlot::Pos2, cuboid.max))
}

fn chunk_top_y(world: &World, min: BlockPos) -> i32 {
    let mut top = world.get_min_y();
    for x in min.x..=min.x + 15 {
        for z in min.z..=min.z + 15 {
            top = top.max(world.get_top_block_y(x, z));
        }
    }
    top
}

fn parse_expand(
    args: &str,
    default_direction: impl FnOnce() -> Direction,
) -> Result<ExpandRequest, String> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(
            "Usage: //expand <amount> [reverseAmount] [direction] or //expand vert".to_owned(),
        );
    }
    if parts.len() == 1 && parts[0].eq_ignore_ascii_case("vert") {
        return Ok(ExpandRequest::Vertical);
    }
    let amount = parse_expand_amount(parts[0])?;
    let mut reverse_amount = 0;
    let mut direction = None;
    match parts.len() {
        1 => {}
        2 => {
            if let Ok(parsed_direction) = parse_direction(parts[1]) {
                direction = Some(parsed_direction);
            } else {
                reverse_amount = parse_expand_amount(parts[1])?;
            }
        }
        3 => {
            reverse_amount = parse_expand_amount(parts[1])?;
            direction = Some(parse_direction(parts[2])?);
        }
        _ => {
            return Err(
                "Usage: //expand <amount> [reverseAmount] [direction] or //expand vert".to_owned(),
            );
        }
    }
    Ok(ExpandRequest::Directional {
        amount,
        reverse_amount,
        direction: direction.unwrap_or_else(default_direction),
    })
}

fn parse_expand_amount(input: &str) -> Result<i32, String> {
    let amount = input
        .parse::<i32>()
        .map_err(|err| format!("failed to parse expand amount `{input}`: {err}"))?;
    if amount < 0 {
        return Err("Expand amount must not be negative.".to_owned());
    }
    Ok(amount)
}

pub(super) fn parse_direction(input: &str) -> Result<Direction, String> {
    match input.to_ascii_lowercase().as_str() {
        "n" | "north" => Ok(Direction::North),
        "s" | "south" => Ok(Direction::South),
        "e" | "east" => Ok(Direction::East),
        "w" | "west" => Ok(Direction::West),
        "u" | "up" => Ok(Direction::Up),
        "d" | "down" => Ok(Direction::Down),
        _ => Err(format!("unknown direction `{input}`")),
    }
}

pub(super) fn actor_direction(actor: &Actor<'_>) -> Direction {
    direction_from_rotation(actor.yaw(), actor.pitch())
}

fn direction_from_rotation(yaw: f32, pitch: f32) -> Direction {
    if pitch <= -45.0 {
        return Direction::Up;
    }
    if pitch >= 45.0 {
        return Direction::Down;
    }
    match ((yaw / 90.0).round() as i32).rem_euclid(4) {
        0 => Direction::South,
        1 => Direction::West,
        2 => Direction::North,
        _ => Direction::East,
    }
}

#[derive(Clone, Copy)]
enum ExpandRequest {
    Directional {
        amount: i32,
        reverse_amount: i32,
        direction: Direction,
    },
    Vertical,
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl Direction {
    pub(super) const fn offset(self, amount: i32) -> BlockPos {
        match self {
            Self::North => BlockPos {
                z: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::South => BlockPos {
                z: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::East => BlockPos {
                x: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::West => BlockPos {
                x: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::Up => BlockPos {
                y: amount,
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
            Self::Down => BlockPos {
                y: amount.saturating_neg(),
                ..BlockPos { x: 0, y: 0, z: 0 }
            },
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

struct ExpandedCuboid {
    min: BlockPos,
    max: BlockPos,
}

impl ExpandedCuboid {
    fn from_selection(selection: Selection) -> Result<Self, String> {
        let pos1 = selection
            .pos1
            .ok_or_else(|| "Select two positions first.".to_owned())?;
        let pos2 = selection
            .pos2
            .ok_or_else(|| "Select two positions first.".to_owned())?;
        Ok(Self {
            min: BlockPos {
                x: pos1.x.min(pos2.x),
                y: pos1.y.min(pos2.y),
                z: pos1.z.min(pos2.z),
            },
            max: BlockPos {
                x: pos1.x.max(pos2.x),
                y: pos1.y.max(pos2.y),
                z: pos1.z.max(pos2.z),
            },
        })
    }

    const fn expand(&mut self, direction: Direction, amount: i32) {
        match direction {
            Direction::North => self.min.z = self.min.z.saturating_sub(amount),
            Direction::South => self.max.z = self.max.z.saturating_add(amount),
            Direction::East => self.max.x = self.max.x.saturating_add(amount),
            Direction::West => self.min.x = self.min.x.saturating_sub(amount),
            Direction::Up => self.max.y = self.max.y.saturating_add(amount),
            Direction::Down => self.min.y = self.min.y.saturating_sub(amount),
        }
    }
}
