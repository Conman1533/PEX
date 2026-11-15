use pumpkin_data::{
    Block, BlockDirection, BlockState, BlockStateId, block_properties::GlowLichenLikeProperties,
};
use pumpkin_util::{
    math::position::BlockPos,
    random::{RandomGenerator, RandomImpl},
};

use super::sculk::SculkLevel;
use super::sculk::vein::{SpreadType, VeinRules};
use crate::generation::proto_chunk::GenerationCache;

/// Vanilla `MultifaceSpreader.DEFAULT_SPREAD_ORDER`.
const SPREAD_ORDER: [SpreadType; 3] = [
    SpreadType::SamePosition,
    SpreadType::SamePlane,
    SpreadType::WrapAround,
];

pub struct MultifaceGrowthFeature {
    pub place_block: pumpkin_data::BlockId,
    pub search_range: i32,
    pub can_place_on_floor: bool,
    pub can_place_on_ceiling: bool,
    pub can_place_on_wall: bool,
    pub chance_of_spreading: f32,
    pub can_be_placed_on: Vec<pumpkin_data::BlockId>,
}

impl MultifaceGrowthFeature {
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        _min_y: i8,
        _height: u16,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let origin_state = GenerationCache::get_block_state(chunk, &pos.0);
        if !Self::is_air_or_water(origin_state) {
            return false;
        }

        let shuffled_dirs = self.get_shuffled_directions(random);
        if self.place_growth_if_possible(chunk, pos, origin_state, random, &shuffled_dirs) {
            return true;
        }

        for search_direction in shuffled_dirs {
            let placement_directions =
                self.get_shuffled_directions_except(random, search_direction.opposite());

            // Vanilla offsets from the origin on every one of its
            // `search_range` iterations, so it tests the same position each
            // time. A failed attempt changes nothing, so one test is enough.
            if self.search_range <= 0 {
                continue;
            }
            let cur_pos = pos.offset(search_direction.to_offset());
            let state = GenerationCache::get_block_state(chunk, &cur_pos.0);
            if !Self::is_air_or_water(state) && state.to_block_id() != self.place_block {
                continue;
            }

            if self.place_growth_if_possible(chunk, cur_pos, state, random, &placement_directions) {
                return true;
            }
        }

        false
    }

    fn place_growth_if_possible<T: GenerationCache>(
        &self,
        chunk: &mut T,
        pos: BlockPos,
        old_state: BlockStateId,
        random: &mut RandomGenerator,
        placement_directions: &[BlockDirection],
    ) -> bool {
        for &direction in placement_directions {
            let neighbor_pos = pos.offset(direction.to_offset());
            let neighbor_id =
                GenerationCache::get_block_state(chunk, &neighbor_pos.0).to_block_id();
            if !self.can_be_placed_on.contains(&neighbor_id) {
                continue;
            }

            let Some(new_state) = self.get_state_for_placement(chunk, old_state, pos, direction)
            else {
                return false;
            };
            chunk.set_block_state(&pos.0, BlockState::from_id(new_state));

            if random.next_f32() < self.chance_of_spreading {
                self.spread_from_face_toward_random_direction(
                    chunk, new_state, pos, direction, random,
                );
            }
            return true;
        }

        false
    }

    /// Vanilla `MultifaceSpreader.spreadFromFaceTowardRandomDirection`
    /// with the placed block's spreader config.
    fn spread_from_face_toward_random_direction<T: GenerationCache>(
        &self,
        chunk: &mut T,
        state: BlockStateId,
        pos: BlockPos,
        starting_face: BlockDirection,
        random: &mut RandomGenerator,
    ) {
        let mut dirs = BlockDirection::all();
        shuffle(&mut dirs, random);

        for spread_direction in dirs {
            if spread_direction.to_axis() == starting_face.to_axis() {
                continue;
            }
            // Sculk veins and glow lichen are never "other" sources, so
            // the source must have the starting face and lack the
            // spread face.
            if !self.has_face(state, starting_face) || self.has_face(state, spread_direction) {
                continue;
            }

            for spread_type in SPREAD_ORDER {
                let (target_pos, target_face) =
                    spread_type.spread_pos(pos, spread_direction, starting_face);
                if !self.can_spread_into(chunk, pos, target_pos, target_face) {
                    continue;
                }
                let old_state = GenerationCache::get_block_state(chunk, &target_pos.0);
                if let Some(placed) =
                    self.get_state_for_placement(chunk, old_state, target_pos, target_face)
                {
                    chunk.set_block_state(&target_pos.0, BlockState::from_id(placed));
                    return;
                }
                // The first spread type that passes is the only one tried.
                break;
            }
        }
    }

    /// Vanilla `SpreadConfig.canSpreadInto`: the target can be replaced
    /// and the new face can attach there.
    fn can_spread_into<T: GenerationCache>(
        &self,
        chunk: &T,
        source_pos: BlockPos,
        pos: BlockPos,
        face: BlockDirection,
    ) -> bool {
        let existing = GenerationCache::get_block_state(chunk, &pos.0);
        let replaceable = if self.place_block == Block::SCULK_VEIN.id {
            VeinRules::can_spread_into(chunk, source_pos, pos, face)
        } else {
            let id = existing.to_block_id();
            existing.to_state().is_air()
                || id == self.place_block
                || (id == Block::WATER.id && holds_water_source(existing))
        };
        replaceable && self.is_valid_state_for_placement(chunk, existing, pos, face)
    }

    /// Vanilla `MultifaceBlock.isValidStateForPlacement`.
    fn is_valid_state_for_placement<T: GenerationCache>(
        &self,
        chunk: &T,
        old_state: BlockStateId,
        pos: BlockPos,
        face: BlockDirection,
    ) -> bool {
        if old_state.to_block_id() == self.place_block && self.has_face(old_state, face) {
            return false;
        }
        let neighbour = pos.offset(face.to_offset());
        chunk.sculk_is_face_sturdy(neighbour, face.opposite())
    }

    /// Vanilla `MultifaceBlock.getStateForPlacement(oldState, ...)`.
    fn get_state_for_placement<T: GenerationCache>(
        &self,
        chunk: &T,
        old_state: BlockStateId,
        pos: BlockPos,
        face: BlockDirection,
    ) -> Option<BlockStateId> {
        if !self.is_valid_state_for_placement(chunk, old_state, pos, face) {
            return None;
        }
        let block = Block::from_id(self.place_block);
        let mut props = if old_state.to_block_id() == self.place_block {
            GlowLichenLikeProperties::from_state_id(old_state)
        } else {
            let mut props = GlowLichenLikeProperties::from_state_id(block.default_state.id);
            props.waterlogged = holds_water_source(old_state);
            props
        };
        set_face(&mut props, face, true);
        Some(props.to_state_id(block))
    }

    fn has_face(&self, state: BlockStateId, face: BlockDirection) -> bool {
        if state.to_block_id() != self.place_block {
            return false;
        }
        let props = GlowLichenLikeProperties::from_state_id(state);
        match face {
            BlockDirection::Down => props.down,
            BlockDirection::Up => props.up,
            BlockDirection::North => props.north,
            BlockDirection::South => props.south,
            BlockDirection::West => props.west,
            BlockDirection::East => props.east,
        }
    }

    fn get_valid_directions(&self) -> Vec<BlockDirection> {
        let mut dirs = Vec::with_capacity(6);
        if self.can_place_on_ceiling {
            dirs.push(BlockDirection::Up);
        }
        if self.can_place_on_floor {
            dirs.push(BlockDirection::Down);
        }
        if self.can_place_on_wall {
            dirs.extend(
                BlockDirection::horizontal_worldgen().map(BlockDirection::from_cardinal_direction),
            );
        }
        dirs
    }

    fn get_shuffled_directions(&self, random: &mut RandomGenerator) -> Vec<BlockDirection> {
        let mut dirs = self.get_valid_directions();
        shuffle(&mut dirs, random);
        dirs
    }

    fn get_shuffled_directions_except(
        &self,
        random: &mut RandomGenerator,
        exclude: BlockDirection,
    ) -> Vec<BlockDirection> {
        let mut dirs: Vec<BlockDirection> = self
            .get_valid_directions()
            .into_iter()
            .filter(|&d| d != exclude)
            .collect();
        shuffle(&mut dirs, random);
        dirs
    }

    fn is_air_or_water(state: BlockStateId) -> bool {
        state.to_state().is_air() || state.to_block_id() == Block::WATER.id
    }
}

/// Vanilla `getFluidState().isSourceOfType(Fluids.WATER)`, read from the
/// block state: generation caches do not report fluid levels reliably.
fn holds_water_source(state: BlockStateId) -> bool {
    state == Block::WATER.default_state.id
        || (state.to_block_id() != Block::WATER.id && state.is_waterlogged())
}

/// Vanilla `Util.shuffle`.
fn shuffle(dirs: &mut [BlockDirection], random: &mut RandomGenerator) {
    for i in (1..dirs.len()).rev() {
        let j = random.next_bounded_i32((i + 1) as i32) as usize;
        dirs.swap(i, j);
    }
}

const fn set_face(props: &mut GlowLichenLikeProperties, face: BlockDirection, value: bool) {
    match face {
        BlockDirection::Down => props.down = value,
        BlockDirection::Up => props.up = value,
        BlockDirection::North => props.north = value,
        BlockDirection::South => props.south = value,
        BlockDirection::West => props.west = value,
        BlockDirection::East => props.east = value,
    }
}
