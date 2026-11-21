use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::HorizontalFacing;
use pumpkin_data::block_properties::RailShape;
use pumpkin_macros::pumpkin_block;
use pumpkin_world::world::BlockFlags;

use crate::block::BlockBehaviour;
use crate::block::CanPlaceAtArgs;
use crate::block::OnNeighborUpdateArgs;
use crate::block::OnPlaceArgs;
use crate::block::PlacedArgs;
use crate::block::blocks::redstone::{block_receives_redstone_power, is_signal_source};

use super::StraightRailShapeExt;
use super::common::{can_place_rail_at, rail_placement_is_valid, update_flanking_rails_shape};
use super::rail_state::RailState;
use super::{HorizontalFacingRailExt, Rail, RailElevation, RailProperties};

#[pumpkin_block("minecraft:rail")]
pub struct RailBlock;

impl BlockBehaviour for RailBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let world = args.world;
        let block_pos = args.position;
        let mut rail_props = RailProperties::default(args.block);
        rail_props.set_waterlogged(args.replacing.water_source());

        let shape = if let Some(east_rail) =
            Rail::find_if_unlocked(world, block_pos, HorizontalFacing::East)
        {
            if Rail::find_if_unlocked(world, block_pos, HorizontalFacing::South).is_some() {
                RailShape::SouthEast
            } else if Rail::find_if_unlocked(world, block_pos, HorizontalFacing::North).is_some() {
                RailShape::NorthEast
            } else {
                match Rail::find_if_unlocked(world, block_pos, HorizontalFacing::West) {
                    Some(west_rail) if west_rail.elevation == RailElevation::Up => {
                        RailShape::AscendingWest
                    }
                    _ => {
                        if east_rail.elevation == RailElevation::Up {
                            RailShape::AscendingEast
                        } else {
                            RailShape::EastWest
                        }
                    }
                }
            }
        } else if let Some(south_rail) =
            Rail::find_if_unlocked(world, block_pos, HorizontalFacing::South)
        {
            if Rail::find_if_unlocked(world, block_pos, HorizontalFacing::West).is_some() {
                RailShape::SouthWest
            } else if south_rail.elevation == RailElevation::Up {
                RailShape::AscendingSouth
            } else {
                match Rail::find_if_unlocked(world, block_pos, HorizontalFacing::North) {
                    Some(north_rail) if north_rail.elevation == RailElevation::Up => {
                        RailShape::AscendingNorth
                    }
                    _ => RailShape::NorthSouth,
                }
            }
        } else if let Some(west_rail) =
            Rail::find_if_unlocked(world, block_pos, HorizontalFacing::West)
        {
            if Rail::find_if_unlocked(world, block_pos, HorizontalFacing::North).is_some() {
                RailShape::NorthWest
            } else if west_rail.elevation == RailElevation::Up {
                RailShape::AscendingWest
            } else {
                RailShape::EastWest
            }
        } else if let Some(north_rail) =
            Rail::find_if_unlocked(world, block_pos, HorizontalFacing::North)
        {
            if north_rail.elevation == RailElevation::Up {
                RailShape::AscendingNorth
            } else {
                RailShape::NorthSouth
            }
        } else {
            args.player
                .living_entity
                .entity
                .get_horizontal_facing()
                .to_rail_shape_flat()
                .as_shape()
        };

        rail_props.set_shape(shape);
        rail_props.to_state_id(args.block)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        update_flanking_rails_shape(args.world, args.block, args.state_id, args.position);
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        if !rail_placement_is_valid(args.world, args.block, args.position) {
            args.world
                .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
            return;
        }
        // Vanilla `RailBlock.updateState`: a signal source updating a three-way junction picks
        // which curve it takes.
        if is_signal_source(args.source_block)
            && let Some(rail) = RailState::new(args.world, *args.position)
            && rail.count_potential_connections(args.world) == 3
        {
            let shape = rail.shape();
            let has_signal = block_receives_redstone_power(args.world, args.position);
            rail.place(args.world, has_signal, false, shape);
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_rail_at(args.block_accessor, args.position)
    }
}
