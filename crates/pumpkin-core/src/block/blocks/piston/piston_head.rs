use pumpkin_data::block_properties::{Facing, MovingPistonLikeProperties, PistonType};
use pumpkin_data::{Block, BlockState, BlockStateId, FacingExt};
use pumpkin_macros::pumpkin_block;
use pumpkin_world::world::BlockFlags;

use crate::block::blocks::piston::piston::try_move;
use crate::block::{
    BlockBehaviour, BrokenArgs, GetStateForNeighborUpdateArgs, OnNeighborUpdateArgs,
    PathComputationType,
};
use crate::world::World;

use super::piston::PistonProps;

pub(crate) type PistonHeadProperties = pumpkin_data::block_properties::PistonHeadLikeProperties;

#[pumpkin_block("minecraft:piston_head")]
pub struct PistonHeadBlock;

impl BlockBehaviour for PistonHeadBlock {
    fn broken(&self, args: BrokenArgs<'_>) {
        let props = PistonHeadProperties::from_state_id(args.state.id);
        let pos = args
            .position
            .offset(props.facing.opposite().to_block_direction().to_offset());
        let (new_block, new_state) = args.world.get_block_and_state_id(&pos);
        if &Block::PISTON == new_block || &Block::STICKY_PISTON == new_block {
            let props = PistonProps::from_state_id(new_state);
            if props.extended {
                // TODO: use player
                args.world.break_block(&pos, None, BlockFlags::SKIP_DROPS);
            }
        }
    }
    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let head_state_id = args.world.get_block_state_id(args.position);
        let head_props = PistonHeadProperties::from_state_id(head_state_id);
        if head_props.facing != Facing::Up {
            return;
        }
        let piston_pos = args.position.offset(
            head_props
                .facing
                .opposite()
                .to_block_direction()
                .to_offset(),
        );
        let piston_block = args.world.get_block(&piston_pos);
        if &Block::PISTON == piston_block || &Block::STICKY_PISTON == piston_block {
            let up_pos = args
                .position
                .offset(head_props.facing.to_block_direction().to_offset());
            let upper_block = args.world.get_block(&up_pos);
            if upper_block != &Block::REDSTONE_BLOCK {
                //Then somebody probably broke the redstone block, try to check if piston should still be extended.
                try_move(args.world, piston_block, &piston_pos);
            }
        }
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        let props = PistonHeadProperties::from_state_id(args.state_id);
        if args.direction.opposite().to_facing() == props.facing
            && !can_survive(args.world, args.position, props)
        {
            return Block::AIR.default_state.id;
        }
        args.state_id
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Vanilla `PistonHeadBlock.canSurvive`: the block behind is the head's extended piston, or a
/// moving piston facing the same way.
fn can_survive(
    world: &World,
    position: &pumpkin_util::math::position::BlockPos,
    head: PistonHeadProperties,
) -> bool {
    let base_pos = position.offset(head.facing.opposite().to_block_direction().to_offset());
    let (base, base_state) = world.get_block_and_state_id(&base_pos);
    let fitting_base = if head.r#type == PistonType::Normal {
        &Block::PISTON
    } else {
        &Block::STICKY_PISTON
    };
    if base == fitting_base {
        let props = PistonProps::from_state_id(base_state);
        return props.extended && props.facing == head.facing;
    }
    base == &Block::MOVING_PISTON
        && MovingPistonLikeProperties::from_state_id(base_state).facing == head.facing
}
