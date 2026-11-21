use pumpkin_data::dimension::Dimension;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use super::super::BrainTick;
use super::super::memory::{GlobalPos, MemoryStatus, types};
use super::OneShot;
use crate::block::blocks::doors::DoorBlock;
use crate::entity::EntityBase;
use crate::world::World;

/// Vanilla `COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE`.
const COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE: i32 = 20;
/// Vanilla `SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN`.
const SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN: f64 = 3.0;
/// Vanilla `MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS`.
const MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS: f64 = 2.0;

/// Vanilla `InteractWithDoor`.
///
/// Opens the mob-interactable doors on the path nodes the mob walks between, remembers them in
/// `doors_to_close`, and closes the ones it has passed unless another mob of its type is about to
/// walk through.
#[must_use]
pub fn create() -> OneShot {
    let mut last_checked_node: Option<BlockPos> = None;
    let mut remaining_cooldown = 0;
    OneShot::new(
        "InteractWithDoor",
        vec![
            (types::PATH.id(), MemoryStatus::ValuePresent),
            (types::DOORS_TO_CLOSE.id(), MemoryStatus::Registered),
            (
                types::NEAREST_LIVING_ENTITIES.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            let Some(path) = tick.state.get(types::PATH) else {
                return false;
            };
            if path.not_started() || path.is_done() {
                return false;
            }
            let (Some(from_pos), Some(to_pos)) = (
                path.get_previous_node().map(|node| node.pos),
                path.get_next_node().map(|node| node.pos),
            ) else {
                return false;
            };
            if last_checked_node == Some(to_pos) {
                remaining_cooldown = COOLDOWN_BEFORE_RERUNNING_IN_SAME_NODE;
            } else {
                remaining_cooldown -= 1;
                if remaining_cooldown > 0 {
                    return false;
                }
            }
            last_checked_node = Some(to_pos);

            let world = tick.world;
            let mut doors = tick.state.get(types::DOORS_TO_CLOSE).cloned();
            if is_mob_interactable_door(world, &from_pos) {
                if !DoorBlock::is_open(world, &from_pos) {
                    DoorBlock::set_open(world, &from_pos, true);
                }
                remember_door_to_close(&mut doors, world, from_pos);
            }
            if is_mob_interactable_door(world, &to_pos) && !DoorBlock::is_open(world, &to_pos) {
                DoorBlock::set_open(world, &to_pos, true);
                remember_door_to_close(&mut doors, world, to_pos);
            }
            if let Some(mut door_set) = doors {
                close_doors_that_i_have_opened_or_passed_through(
                    tick,
                    Some(from_pos),
                    Some(to_pos),
                    &mut door_set,
                );
                tick.state.set(types::DOORS_TO_CLOSE, door_set);
            }
            true
        },
    )
}

/// Vanilla's `is(BlockTags.MOB_INTERACTABLE_DOORS, s -> s.getBlock() instanceof DoorBlock)`.
fn is_mob_interactable_door(world: &World, pos: &BlockPos) -> bool {
    let (block, _) = world.get_block_and_state_id(pos);
    block.has_tag(&tag::Block::MINECRAFT_MOB_INTERACTABLE_DOORS)
        && block.has_tag(&tag::Block::MINECRAFT_DOORS)
}

fn remember_door_to_close(doors: &mut Option<FxHashSet<GlobalPos>>, world: &World, pos: BlockPos) {
    let Some(dimension) = Dimension::from_name(world.dimension.minecraft_name) else {
        return;
    };
    doors
        .get_or_insert_with(FxHashSet::default)
        .insert(GlobalPos::new(dimension, pos));
}

/// Vanilla `InteractWithDoor.closeDoorsThatIHaveOpenedOrPassedThrough`.
///
/// Every door it looks at leaves the set; the ones still open, close by and not about to be
/// walked through by another mob of the same type are closed.
pub fn close_doors_that_i_have_opened_or_passed_through<S: std::hash::BuildHasher>(
    tick: &BrainTick<'_>,
    moving_from: Option<BlockPos>,
    moving_to: Option<BlockPos>,
    doors: &mut std::collections::HashSet<GlobalPos, S>,
) {
    let world = tick.world;
    let body = tick.mob.get_entity();
    let nearest = tick.state.get(types::NEAREST_LIVING_ENTITIES);
    doors.retain(|door| {
        let door_pos = door.pos;
        if moving_from == Some(door_pos) || moving_to == Some(door_pos) {
            return true;
        }
        if door.dimension.id != world.dimension.id
            || !closer_to_center_than(
                &door_pos,
                &body.pos.load(),
                SKIP_CLOSING_DOOR_IF_FURTHER_AWAY_THAN,
            )
        {
            return false;
        }
        if !is_mob_interactable_door(world, &door_pos) || !DoorBlock::is_open(world, &door_pos) {
            return false;
        }
        let others_coming = nearest.is_some_and(|entities| {
            entities.iter().any(|other| {
                let other_entity = other.get_entity();
                other_entity.entity_type == body.entity_type
                    && closer_to_center_than(
                        &door_pos,
                        &other_entity.pos.load(),
                        MAX_DISTANCE_TO_HOLD_DOOR_OPEN_FOR_OTHER_MOBS,
                    )
                    && is_mob_coming_through_door(other.as_ref(), door_pos)
            })
        });
        if !others_coming {
            DoorBlock::set_open(world, &door_pos, false);
        }
        false
    });
}

/// Vanilla `isMobComingThroughDoor`, read from the other mob's brain. A brain that is busy
/// ticking elsewhere counts as not coming through.
fn is_mob_coming_through_door(other: &dyn EntityBase, door_pos: BlockPos) -> bool {
    let Some(mob) = other.get_mob() else {
        return false;
    };
    mob.get_mob_entity()
        .try_with_brain(mob, |brain| {
            brain.get(types::PATH).is_some_and(|path| {
                !path.is_done()
                    && path.get_previous_node().is_some_and(|from| {
                        from.pos == door_pos
                            || path.get_next_node().is_some_and(|to| to.pos == door_pos)
                    })
            })
        })
        .unwrap_or(false)
}

/// Vanilla `Vec3i.closerToCenterThan`.
fn closer_to_center_than(
    pos: &BlockPos,
    position: &pumpkin_util::math::vector3::Vector3<f64>,
    distance: f64,
) -> bool {
    pos.to_centered_f64().squared_distance_to_vec(position) < distance * distance
}
