use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnPlaceArgs,
    OnScheduledTickArgs, PathComputationType, push_entities_up,
};
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;
use pumpkin_world::world::BlockFlags;

#[pumpkin_block("minecraft:dirt_path")]
pub struct DirtPathBlock;

impl BlockBehaviour for DirtPathBlock {
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        push_entities_up(args.world, state, Block::DIRT.default_state, args.position);
        args.world.set_block_state(
            args.position,
            Block::DIRT.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        args.world.emit_game_event(
            GameEvent::BlockChange.name(),
            args.position.to_centered_f64(),
        );
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            push_entities_up(
                args.world,
                args.block.default_state,
                Block::DIRT.default_state,
                args.position,
            );
            return Block::DIRT.default_state.id;
        }

        args.block.default_state.id
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Up && !can_place_at(args.world, args.position) {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_at(args.block_accessor, args.position)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    let (block, state) = world.get_block_and_state(&block_pos.up());
    !state.is_solid() || block.has_tag(&tag::Block::MINECRAFT_FENCE_GATES)
}
