use std::sync::Arc;

use crate::block::blocks::plant::big_dripleaf::{can_grow_into, is_water_source, place_leaf};
use crate::block::{
    BlockBehaviour, BonemealArgs, CanPlaceAtArgs, GetStateForNeighborUpdateArgs,
    OnScheduledTickArgs,
};
use crate::world::World;
use pumpkin_data::block_properties::{HorizontalFacing, LadderLikeProperties};
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockStateId, tag};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::{BlockAccessor, BlockFlags};

type BigDripleafStemProperties = LadderLikeProperties;

#[pumpkin_block("minecraft:big_dripleaf_stem")]
pub struct BigDripleafStemBlock;

impl BlockBehaviour for BigDripleafStemBlock {
    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_survive(args.block_accessor, args.position)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if matches!(args.direction, BlockDirection::Down | BlockDirection::Up)
            && !can_survive(args.world, args.position)
        {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if !can_survive(args.world.as_ref(), args.position) {
            args.world
                .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
        }
    }

    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        top_leaf(args.world, args.position)
            .is_some_and(|head| can_grow_into(args.world, &head.up()))
    }

    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        if let Some(head) = top_leaf(args.world, args.position) {
            let facing = BigDripleafStemProperties::from_state_id(args.state_id).facing;
            place_stem(args.world, &head, facing);
            place_leaf(args.world, &head.up(), facing);
        }
    }
}

/// Vanilla `BigDripleafStemBlock.place`.
pub fn place_stem(world: &Arc<World>, pos: &BlockPos, facing: HorizontalFacing) {
    let mut props = BigDripleafStemProperties::default(&Block::BIG_DRIPLEAF_STEM);
    props.facing = facing;
    props.waterlogged = is_water_source(world, pos);
    world.set_block_state(
        pos,
        props.to_state_id(&Block::BIG_DRIPLEAF_STEM),
        BlockFlags::NOTIFY_ALL,
    );
}

/// Vanilla `BlockUtil.getTopConnectedBlock(level, pos, BIG_DRIPLEAF_STEM, UP, BIG_DRIPLEAF)`.
fn top_leaf(world: &World, pos: &BlockPos) -> Option<BlockPos> {
    let mut current = *pos;
    loop {
        current = current.up();
        let block = world.get_block(&current);
        if block == &Block::BIG_DRIPLEAF {
            return Some(current);
        }
        if block != &Block::BIG_DRIPLEAF_STEM {
            return None;
        }
    }
}

fn can_survive(block_accessor: &dyn BlockAccessor, pos: &BlockPos) -> bool {
    let below = block_accessor.get_block(&pos.down());
    let above = block_accessor.get_block(&pos.up());
    (below == &Block::BIG_DRIPLEAF_STEM
        || below.has_tag(&tag::Block::MINECRAFT_SUPPORTS_BIG_DRIPLEAF))
        && (above == &Block::BIG_DRIPLEAF_STEM || above == &Block::BIG_DRIPLEAF)
}
