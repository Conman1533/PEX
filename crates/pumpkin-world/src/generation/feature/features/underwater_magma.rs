use crate::generation::proto_chunk::GenerationCache;
use pumpkin_data::{Block, BlockDirection, BlockId, BlockStateId, HorizontalFacingExt};
use pumpkin_util::{
    math::position::BlockPos,
    random::{RandomGenerator, RandomImpl},
};

/// Vanilla `UnderwaterMagmaFeature`: magma blocks around the floor under a column of water.
pub struct UnderwaterMagmaFeature {
    /// vertical search limit from origin for a water column floor
    pub floor_search_range: i32,
    /// horizontal radius from the floor spot to sample
    pub placement_radius: i32,
    /// chance each inspected block turns into magma
    pub placement_probability: f32,
}

impl UnderwaterMagmaFeature {
    /// Vanilla `getFloorY`: `Column.scan` of the water column at the origin. The origin itself
    /// has to be water; the floor is the first non-water block below it within the search range.
    fn get_floor_y<T: GenerationCache>(&self, chunk: &T, origin: BlockPos) -> Option<i32> {
        let (x, z) = (origin.0.x, origin.0.z);
        let is_water = |y: i32| {
            GenerationCache::get_block_state(chunk, &BlockPos::new(x, y, z).0).to_block_id()
                == BlockId::WATER
        };
        let mut y = origin.0.y;
        if !is_water(y) {
            return None;
        }
        let mut i = 1;
        while i < self.floor_search_range && is_water(y) {
            y -= 1;
            i += 1;
        }
        (!is_water(y)).then_some(y)
    }

    /// Vanilla `isValidPlacement`: not water or air, and covered by full faces below and on
    /// all four sides.
    fn is_valid_placement<T: GenerationCache>(chunk: &T, target: BlockPos) -> bool {
        let state = GenerationCache::get_block_state(chunk, &target.0);
        if state.to_block_id() == BlockId::WATER || state.to_state().is_air() {
            return false;
        }
        let below = target.offset(BlockDirection::Down.to_offset());
        if Self::is_visible_from_outside(chunk, below, BlockDirection::Up) {
            return false;
        }
        for dir in BlockDirection::horizontal_worldgen() {
            let neighbour = target.offset(dir.to_offset());
            if Self::is_visible_from_outside(chunk, neighbour, dir.to_block_direction().opposite())
            {
                return false;
            }
        }
        true
    }

    /// Vanilla `isVisibleFromOutside`: the face of `pos` towards the target is not a full face
    /// occluder. A block that cannot occlude has an empty face occlusion shape.
    fn is_visible_from_outside<T: GenerationCache>(
        chunk: &T,
        pos: BlockPos,
        covered_direction: BlockDirection,
    ) -> bool {
        let state: BlockStateId = GenerationCache::get_block_state(chunk, &pos.0);
        !(state.can_occlude() && state.is_side_solid(covered_direction))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        _min_y: i8,
        _height: u16,
        _feature_name: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let Some(floor_y) = self.get_floor_y(chunk, pos) else {
            return false;
        };

        // Vanilla walks the box with `BlockPos.betweenClosedStream`: x fastest, then y, then z,
        // drawing one random float per position before the validity check.
        let mut placed = 0i32;
        let r = self.placement_radius;
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    if random.next_f32() >= self.placement_probability {
                        continue;
                    }
                    let target_pos = BlockPos::new(pos.0.x + dx, floor_y + dy, pos.0.z + dz);
                    if !Self::is_valid_placement(chunk, target_pos) {
                        continue;
                    }
                    chunk.set_block_state(&target_pos.0, Block::MAGMA_BLOCK.default_state);
                    placed += 1;
                }
            }
        }
        placed > 0
    }
}
