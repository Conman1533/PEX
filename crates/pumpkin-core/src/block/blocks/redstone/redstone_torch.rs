use std::sync::Arc;

use crate::block::BlockIsReplacing;
use crate::block::CanPlaceAtArgs;
use crate::block::EmitsRedstonePowerArgs;
use crate::block::GetRedstonePowerArgs;
use crate::block::GetStateForNeighborUpdateArgs;
use crate::block::OnNeighborUpdateArgs;
use crate::block::OnPlaceArgs;
use crate::block::OnScheduledTickArgs;
use crate::block::OnStateReplacedArgs;
use crate::block::PlacedArgs;
use crate::entity::EntityBase;
use pumpkin_data::Block;
use pumpkin_data::BlockDirection;
use pumpkin_data::BlockId;
use pumpkin_data::BlockStateId;
use pumpkin_data::FacingExt;
use pumpkin_data::HorizontalFacingExt;
use pumpkin_data::block_properties::Facing;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;
use pumpkin_world::world::BlockFlags;

type RWallTorchProps = pumpkin_data::block_properties::FurnaceLikeProperties;
type RTorchProps = pumpkin_data::block_properties::RedstoneOreLikeProperties;

use crate::block::{BlockBehaviour, BlockMetadata};
use crate::world::World;
use pumpkin_data::world::WorldEvent;

/// Vanilla `RedstoneTorchBlock.RECENT_TOGGLE_TIMER`: how long a toggle counts, in game ticks.
const RECENT_TOGGLE_TIMER: i64 = 60;
/// Vanilla `RedstoneTorchBlock.MAX_RECENT_TOGGLES`.
const MAX_RECENT_TOGGLES: usize = 8;
/// Vanilla `RedstoneTorchBlock.RESTART_DELAY`.
const RESTART_DELAY: u8 = 160;

/// `state` of a torch or wall torch with `lit` set.
fn with_lit(block: &Block, state: BlockStateId, lit: bool) -> BlockStateId {
    if block == &Block::REDSTONE_WALL_TORCH {
        let mut props = RWallTorchProps::from_state_id(state);
        props.lit = lit;
        props.to_state_id(block)
    } else {
        let mut props = RTorchProps::from_state_id(state);
        props.lit = lit;
        props.to_state_id(block)
    }
}

/// Drops the toggles older than [`RECENT_TOGGLE_TIMER`], as each torch tick does in vanilla.
fn forget_old_toggles(world: &World, now: i64) {
    let mut toggles = world
        .recent_torch_toggles
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while toggles
        .front()
        .is_some_and(|(_, when)| now - when > RECENT_TOGGLE_TIMER)
    {
        toggles.pop_front();
    }
}

/// Vanilla `RedstoneTorchBlock.isToggledTooFrequently`: records a toggle at `pos` when `now` is
/// given, then whether `pos` has [`MAX_RECENT_TOGGLES`] recent ones.
fn toggled_too_frequently(world: &World, pos: BlockPos, now: Option<i64>) -> bool {
    let mut toggles = world
        .recent_torch_toggles
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(now) = now {
        toggles.push_back((pos, now));
    }
    toggles.iter().filter(|(at, _)| *at == pos).count() >= MAX_RECENT_TOGGLES
}

use super::get_redstone_power;

pub struct RedstoneTorchBlock;

impl BlockMetadata for RedstoneTorchBlock {
    fn ids() -> Box<[BlockId]> {
        [BlockId::REDSTONE_TORCH, BlockId::REDSTONE_WALL_TORCH].into()
    }
}

impl BlockBehaviour for RedstoneTorchBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let world = args.world;
        let block = args.block;
        let location = args.position;

        if args.direction == BlockDirection::Down {
            let support_block = world.get_block_state(&location.down());
            if support_block.is_center_solid(BlockDirection::Up) {
                return block.default_state.id;
            }
        }
        let mut directions = args.player.get_entity().get_entity_facing_order();

        if args.replacing == BlockIsReplacing::None {
            let face = args.direction.to_facing();
            let mut i = 0;
            while i < directions.len() && directions[i] != face {
                i += 1;
            }

            if i > 0 {
                directions.copy_within(0..i, 1);
                directions[0] = face;
            }
        } else if directions[0] == Facing::Down {
            let support_block = world.get_block_state(&location.down());
            if support_block.is_center_solid(BlockDirection::Up) {
                return block.default_state.id;
            }
        }

        for dir in directions {
            if dir != Facing::Up
                && dir != Facing::Down
                && can_place_at(world, location, dir.to_block_direction())
            {
                let mut torch_props = RWallTorchProps::default(&Block::REDSTONE_WALL_TORCH);
                if let Some(facing) = dir.opposite().to_horizontal_facing() {
                    torch_props.facing = facing;
                    return torch_props.to_state_id(&Block::REDSTONE_WALL_TORCH);
                }
            }
        }

        let support_block = world.get_block_state(&location.down());
        if support_block.is_center_solid(BlockDirection::Up) {
            block.default_state.id
        } else {
            BlockStateId::AIR
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        let support_block = args.block_accessor.get_block_state(&args.position.down());
        if support_block.is_center_solid(BlockDirection::Up) {
            return true;
        }
        for dir in BlockDirection::horizontal() {
            if can_place_at(args.block_accessor, args.position, dir.to_block_direction()) {
                return true;
            }
        }
        false
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.block == &Block::REDSTONE_WALL_TORCH {
            let props = RWallTorchProps::from_state_id(args.state_id);
            if props.facing.to_block_direction().opposite() == args.direction
                && !can_place_at(
                    args.world,
                    args.position,
                    props.facing.to_block_direction().opposite(),
                )
            {
                return BlockStateId::AIR;
            }
        } else if args.direction == BlockDirection::Down {
            let support_block = args.world.get_block_state(&args.position.down());
            if !support_block.is_center_solid(BlockDirection::Up) {
                return BlockStateId::AIR;
            }
        }
        args.state_id
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        {
            let state = args.world.get_block_state(args.position);

            if args
                .world
                .will_block_tick_this_tick(args.position, args.block)
            {
                return;
            }

            if args.block == &Block::REDSTONE_WALL_TORCH {
                let props = RWallTorchProps::from_state_id(state.id);
                if props.lit
                    != should_be_lit(
                        args.world,
                        args.position,
                        props.facing.to_block_direction().opposite(),
                    )
                {
                    args.world.schedule_block_tick(
                        args.block,
                        *args.position,
                        2,
                        TickPriority::Normal,
                    );
                }
            } else if args.block == &Block::REDSTONE_TORCH {
                let props = RTorchProps::from_state_id(state.id);
                if props.lit != should_be_lit(args.world, args.position, BlockDirection::Down) {
                    args.world.schedule_block_tick(
                        args.block,
                        *args.position,
                        2,
                        TickPriority::Normal,
                    );
                }
            }
        }
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        if args.block == &Block::REDSTONE_WALL_TORCH {
            let props = RWallTorchProps::from_state_id(args.state.id);
            if props.lit && args.direction != props.facing.to_block_direction() {
                return 15;
            }
        } else if args.block == &Block::REDSTONE_TORCH {
            let props = RTorchProps::from_state_id(args.state.id);
            if props.lit && args.direction != BlockDirection::Up {
                return 15;
            }
        }
        0
    }

    fn get_strong_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        if args.direction == BlockDirection::Down {
            if args.block == &Block::REDSTONE_WALL_TORCH {
                let props = RWallTorchProps::from_state_id(args.state.id);
                if props.lit {
                    return 15;
                }
            } else if args.block == &Block::REDSTONE_TORCH {
                let props = RTorchProps::from_state_id(args.state.id);
                if props.lit {
                    return 15;
                }
            }
        }
        0
    }

    /// Vanilla `RedstoneTorchBlock.tick`: a torch that turns off too often burns out, stays off
    /// and tries again 160 ticks later.
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let (block, state) = args.world.get_block_and_state(args.position);
        let (lit, should_be_lit_now) = if block == &Block::REDSTONE_WALL_TORCH {
            let props = RWallTorchProps::from_state_id(state.id);
            let facing = props.facing.to_block_direction().opposite();
            (props.lit, should_be_lit(args.world, args.position, facing))
        } else if block == &Block::REDSTONE_TORCH {
            let props = RTorchProps::from_state_id(state.id);
            (
                props.lit,
                should_be_lit(args.world, args.position, BlockDirection::Down),
            )
        } else {
            return;
        };
        let set_lit = |lit| with_lit(block, state.id, lit);

        let now = args.world.get_world_age();
        forget_old_toggles(args.world, now);
        if lit {
            if !should_be_lit_now {
                args.world
                    .set_block_state(args.position, set_lit(false), BlockFlags::NOTIFY_ALL);
                if toggled_too_frequently(args.world, *args.position, Some(now)) {
                    args.world.sync_world_event(
                        WorldEvent::RedstoneTorchBurnout,
                        *args.position,
                        0,
                    );
                    args.world.schedule_block_tick(
                        block,
                        *args.position,
                        RESTART_DELAY,
                        TickPriority::Normal,
                    );
                }
            }
        } else if should_be_lit_now && !toggled_too_frequently(args.world, *args.position, None) {
            args.world
                .set_block_state(args.position, set_lit(true), BlockFlags::NOTIFY_ALL);
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        update_neighbors(args.world, args.position);
    }

    /// Vanilla `RedstoneTorchBlock.onPlace` runs on every state change, lighting and going out
    /// included, inside the `setBlock`, before the torch's own neighbours hear of it.
    fn state_changed(&self, args: PlacedArgs<'_>) {
        update_neighbors(args.world, args.position);
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        update_neighbors(args.world, args.position);
    }
}

pub fn should_be_lit(world: &World, pos: &BlockPos, face: BlockDirection) -> bool {
    let other_pos = pos.offset(face.to_offset());
    let (block, state) = world.get_block_and_state(&other_pos);
    get_redstone_power(block, state, world, &other_pos, face) == 0
}

pub fn update_neighbors(world: &Arc<World>, pos: &BlockPos) {
    for dir in BlockDirection::all() {
        let other_pos = pos.offset(dir.to_offset());
        world.update_neighbors(&other_pos, None);
    }
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos, facing: BlockDirection) -> bool {
    world
        .get_block_state(&block_pos.offset(facing.to_offset()))
        .is_side_solid(facing.opposite())
}
