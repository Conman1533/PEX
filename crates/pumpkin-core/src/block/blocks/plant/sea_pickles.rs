use crate::block::BlockIsReplacing;
use crate::block::blocks::plant::PlantBlockBase;
use crate::block::{
    BlockBehaviour, BonemealArgs, CanPlaceAtArgs, CanUpdateAtArgs, GetStateForNeighborUpdateArgs,
    OnPlaceArgs, PathComputationType,
};
use crate::entity::EntityBase;
use pumpkin_data::entity::EntityPose;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId, tag};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

type SeaPickleProperties = pumpkin_data::block_properties::SeaPickleLikeProperties;

const MAX_PICKLES: u8 = 4;

#[pumpkin_block("minecraft:sea_pickle")]
pub struct SeaPickleBlock;

impl BlockBehaviour for SeaPickleBlock {
    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        SeaPickleProperties::from_state_id(args.state_id).waterlogged
            && args
                .world
                .get_block(&args.position.down())
                .has_tag(&tag::Block::MINECRAFT_CORAL_BLOCKS)
    }

    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        let pos = args.position.0;
        let mut rng = rand::rng();
        let mut z_span = 1;
        let mut z_offset = 0;
        for (count, x) in (pos.x - 2..=pos.x + 2).enumerate() {
            for z in 0..z_span {
                let end_y = 2 + pos.y - 1;
                for y in (end_y - 2)..end_y {
                    let position = BlockPos::new(x, y, pos.z - z_offset + z);
                    if &position != args.position
                        && rng.random_range(0..6) == 0
                        && args.world.get_block(&position) == &Block::WATER
                        && args
                            .world
                            .get_block(&position.down())
                            .has_tag(&tag::Block::MINECRAFT_CORAL_BLOCKS)
                    {
                        let mut props = SeaPickleProperties::default(args.block);
                        props.pickles = rng.random_range(1..=MAX_PICKLES);
                        args.world.set_block_state(
                            &position,
                            props.to_state_id(args.block),
                            BlockFlags::NOTIFY_ALL,
                        );
                    }
                }
            }
            if count < 2 {
                z_span += 2;
                z_offset += 1;
            } else {
                z_span -= 2;
                z_offset -= 1;
            }
        }

        let mut props = SeaPickleProperties::from_state_id(args.state_id);
        props.pickles = MAX_PICKLES;
        args.world.set_block_state(
            args.position,
            props.to_state_id(args.block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if args.player.get_entity().pose.load() != EntityPose::Crouching
            && let BlockIsReplacing::Itself(state_id) = args.replacing
        {
            let mut sea_pickle_prop = SeaPickleProperties::from_state_id(state_id);
            if sea_pickle_prop.pickles < MAX_PICKLES {
                sea_pickle_prop.pickles += 1;
            }
            return sea_pickle_prop.to_state_id(args.block);
        }

        let mut sea_pickle_prop = SeaPickleProperties::default(args.block);
        sea_pickle_prop.waterlogged = args.replacing.water_source();
        sea_pickle_prop.to_state_id(args.block)
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        // Vanilla `mayPlaceOn`: anything whose collision shape reaches the top face.
        let below = args.block_accessor.get_block_state(&args.position.down());
        below
            .get_block_collision_shapes()
            .any(|shape| shape.max.y >= 1.0)
            || below.is_side_solid(BlockDirection::Up)
    }

    fn can_update_at(&self, args: CanUpdateAtArgs<'_>) -> bool {
        args.player.get_entity().pose.load() != EntityPose::Crouching
            && SeaPickleProperties::from_state_id(args.state_id).pickles < MAX_PICKLES
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        <Self as PlantBlockBase>::get_state_for_neighbor_update(
            self,
            args.world,
            args.position,
            args.state_id,
        )
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

impl PlantBlockBase for SeaPickleBlock {}
