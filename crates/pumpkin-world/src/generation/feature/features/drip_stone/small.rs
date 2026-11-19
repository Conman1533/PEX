use super::SpeleothemBlocks;
use crate::generation::proto_chunk::GenerationCache;
use pumpkin_data::BlockDirection;
use pumpkin_util::{
    math::position::BlockPos,
    random::{RandomGenerator, RandomImpl},
};

/// Vanilla `SpeleothemFeature`: a single pointed dripstone or sulfur spike on a small patch of
/// base blocks.
pub struct SmallDripstoneFeature {
    pub blocks: SpeleothemBlocks,
    pub taller_dripstone: f32,
    pub directional_spread: f32,
    pub spread_radius2: f32,
    pub spread_radius3: f32,
}

impl SmallDripstoneFeature {
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let Some(dir) = self.get_direction(chunk, pos, random) else {
            return false;
        };
        let root_pos = pos.offset(dir.opposite().to_offset());
        self.gen_base_blocks(chunk, root_pos, random);
        let next_pos = pos.offset(dir.to_offset());
        let height = if random.next_f32() < self.taller_dripstone
            && super::is_empty_or_water(GenerationCache::get_block_state(chunk, &next_pos.0))
        {
            2
        } else {
            1
        };
        self.blocks.grow(chunk, pos, dir, height, false);
        true
    }

    fn get_direction<T: GenerationCache>(
        &self,
        chunk: &T,
        pos: BlockPos,
        random: &mut RandomGenerator,
    ) -> Option<BlockDirection> {
        let up = self
            .blocks
            .is_base(GenerationCache::get_block_state(chunk, &pos.up().0));
        let down = self
            .blocks
            .is_base(GenerationCache::get_block_state(chunk, &pos.down().0));
        if up && down {
            return if random.next_bool() {
                Some(BlockDirection::Down)
            } else {
                Some(BlockDirection::Up)
            };
        }
        if up {
            return Some(BlockDirection::Down);
        }
        if down {
            return Some(BlockDirection::Up);
        }
        None
    }

    fn gen_base_blocks<T: GenerationCache>(
        &self,
        chunk: &mut T,
        pos: BlockPos,
        random: &mut RandomGenerator,
    ) {
        self.blocks.place_base_block_if_possible(chunk, pos);
        for dir in BlockDirection::horizontal_worldgen() {
            if random.next_f32() > self.directional_spread {
                continue;
            }
            let pos = pos.offset(dir.to_offset());
            self.blocks.place_base_block_if_possible(chunk, pos);
            if random.next_f32() > self.spread_radius2 {
                continue;
            }
            let pos = pos.offset(BlockDirection::random(random).to_offset());
            self.blocks.place_base_block_if_possible(chunk, pos);
            if random.next_f32() > self.spread_radius3 {
                continue;
            }
            let pos = pos.offset(BlockDirection::random(random).to_offset());
            self.blocks.place_base_block_if_possible(chunk, pos);
        }
    }
}
