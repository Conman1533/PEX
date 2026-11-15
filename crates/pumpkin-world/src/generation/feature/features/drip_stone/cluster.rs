use pumpkin_data::{Block, BlockDirection, BlockId, BlockStateId};
use pumpkin_util::math::clamped_map;
use pumpkin_util::math::float_provider::FloatProvider;
use pumpkin_util::math::int_provider::IntProvider;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::random::{RandomGenerator, RandomImpl};

use super::{Column, SpeleothemBlocks, is_water_at, state_at};
use crate::generation::proto_chunk::GenerationCache;

/// Vanilla `SpeleothemClusterFeature`: a patch of stalactites and stalagmites over thickened
/// floors and ceilings, with small water pools on the floor.
pub struct DripstoneClusterFeature {
    pub blocks: SpeleothemBlocks,
    pub floor_to_ceiling_search_range: i32,
    pub height: IntProvider,
    pub radius: IntProvider,
    pub max_stalagmite_stalactite_height_diff: i32,
    pub height_deviation: i32,
    pub speleothem_block_layer_thickness: IntProvider,
    pub density: FloatProvider,
    pub wetness: FloatProvider,
    pub chance_of_speleothem_at_max_distance_from_center: f32,
    pub max_distance_from_edge_affecting_chance_of_speleothem: i32,
    pub max_distance_from_center_affecting_height_bias: i32,
}

/// The per-cluster values `placeColumn` shares between columns.
struct ClusterShape {
    chance_of_water: f32,
    height: i32,
    density: f32,
}

impl DripstoneClusterFeature {
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        if !super::is_empty_or_water(GenerationCache::get_block_state(chunk, &pos.0)) {
            return false;
        }
        let height = self.height.get(random);
        let wetness = self.wetness.get(random);
        let density = self.density.get(random);
        let x_radius = self.radius.get(random);
        let z_radius = self.radius.get(random);
        let shape = ClusterShape {
            chance_of_water: wetness,
            height,
            density,
        };

        for dx in -x_radius..=x_radius {
            for dz in -z_radius..=z_radius {
                let chance = self.chance_of_stalagmite_or_stalactite(x_radius, z_radius, dx, dz);
                let column_pos = BlockPos::new(pos.0.x + dx, pos.0.y, pos.0.z + dz);
                self.place_column(chunk, random, column_pos, dx, dz, &shape, chance);
            }
        }
        true
    }

    #[expect(clippy::too_many_arguments)]
    fn place_column<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
        dx: i32,
        dz: i32,
        shape: &ClusterShape,
        chance: f64,
    ) {
        let Some(base_column) = Column::scan(
            chunk,
            pos,
            self.floor_to_ceiling_search_range,
            super::is_empty_or_water,
            super::is_neither_empty_nor_water,
        ) else {
            return;
        };
        let ceiling = base_column.ceiling;
        let base_floor = base_column.floor;
        if ceiling.is_none() && base_floor.is_none() {
            return;
        }
        let (x, z) = (pos.0.x, pos.0.z);

        let want_pool = random.next_f32() < shape.chance_of_water;
        let column = match base_floor {
            Some(floor_y) if want_pool && self.can_place_pool(chunk, x, floor_y, z) => {
                chunk.set_block_state(&BlockPos::new(x, floor_y, z).0, Block::WATER.default_state);
                Column {
                    floor: Some(floor_y - 1),
                    ceiling,
                }
            }
            _ => base_column,
        };
        let floor = column.floor;

        let want_stalactite = random.next_f64() < chance;
        let stalactite_height = match ceiling {
            Some(ceiling_y) if want_stalactite && !is_lava(state_at(chunk, x, ceiling_y, z)) => {
                let thickness = self.speleothem_block_layer_thickness.get(random);
                self.replace_with_base_blocks(
                    chunk,
                    x,
                    ceiling_y,
                    z,
                    thickness,
                    BlockDirection::Up,
                );
                let max_height = floor.map_or(shape.height, |floor_y| {
                    shape.height.min(ceiling_y - floor_y)
                });
                self.speleothem_height(random, dx, dz, shape.density, max_height)
            }
            _ => 0,
        };

        let want_stalagmite = random.next_f64() < chance;
        let stalagmite_height = match floor {
            Some(floor_y) if want_stalagmite && !is_lava(state_at(chunk, x, floor_y, z)) => {
                let thickness = self.speleothem_block_layer_thickness.get(random);
                self.replace_with_base_blocks(
                    chunk,
                    x,
                    floor_y,
                    z,
                    thickness,
                    BlockDirection::Down,
                );
                if ceiling.is_some() {
                    let diff = self.max_stalagmite_stalactite_height_diff;
                    (stalactite_height + random_between_inclusive(random, -diff, diff)).max(0)
                } else {
                    self.speleothem_height(random, dx, dz, shape.density, shape.height)
                }
            }
            _ => 0,
        };

        let (actual_stalactite_height, actual_stalagmite_height) =
            resolve_overlap(random, ceiling, floor, stalactite_height, stalagmite_height);

        let merge_tips = random.next_bool()
            && actual_stalactite_height > 0
            && actual_stalagmite_height > 0
            && column.height() == Some(actual_stalactite_height + actual_stalagmite_height);
        if let Some(ceiling_y) = ceiling {
            self.blocks.grow(
                chunk,
                BlockPos::new(x, ceiling_y - 1, z),
                BlockDirection::Down,
                actual_stalactite_height,
                merge_tips,
            );
        }
        if let Some(floor_y) = floor {
            self.blocks.grow(
                chunk,
                BlockPos::new(x, floor_y + 1, z),
                BlockDirection::Up,
                actual_stalagmite_height,
                merge_tips,
            );
        }
    }

    fn speleothem_height(
        &self,
        random: &mut RandomGenerator,
        dx: i32,
        dz: i32,
        density: f32,
        max_height: i32,
    ) -> i32 {
        if random.next_f32() > density {
            return 0;
        }
        let distance_from_center = dx.abs() + dz.abs();
        let height_mean = clamped_map(
            f64::from(distance_from_center),
            0.0,
            f64::from(self.max_distance_from_center_affecting_height_bias),
            f64::from(max_height) / 2.0,
            0.0,
        ) as f32;
        // Vanilla `ClampedNormalFloat.sample`: `mean + (float) nextGaussian() * deviation`,
        // clamped, then truncated.
        let value = height_mean + random.next_gaussian() as f32 * self.height_deviation as f32;
        value.clamp(0.0, max_height as f32) as i32
    }

    fn can_place_pool<T: GenerationCache>(&self, chunk: &T, x: i32, y: i32, z: i32) -> bool {
        let id = state_at(chunk, x, y, z).to_block_id();
        if id == BlockId::WATER
            || id == self.blocks.base_block.id
            || id == self.blocks.pointed_block.id
        {
            return false;
        }
        if is_water_at(state_at(chunk, x, y + 1, z)) {
            return false;
        }
        for dir in BlockDirection::horizontal_worldgen() {
            let offset = dir.to_offset();
            if !can_be_adjacent_to_water(state_at(chunk, x + offset.x, y, z + offset.z)) {
                return false;
            }
        }
        can_be_adjacent_to_water(state_at(chunk, x, y - 1, z))
    }

    fn replace_with_base_blocks<T: GenerationCache>(
        &self,
        chunk: &mut T,
        x: i32,
        y: i32,
        z: i32,
        max_count: i32,
        direction: BlockDirection,
    ) {
        let step = direction.to_offset().y;
        for i in 0..max_count {
            let pos = BlockPos::new(x, y + i * step, z);
            if !self.blocks.place_base_block_if_possible(chunk, pos) {
                return;
            }
        }
    }

    fn chance_of_stalagmite_or_stalactite(
        &self,
        x_radius: i32,
        z_radius: i32,
        dx: i32,
        dz: i32,
    ) -> f64 {
        let x_distance_from_edge = x_radius - dx.abs();
        let z_distance_from_edge = z_radius - dz.abs();
        let distance_from_edge = x_distance_from_edge.min(z_distance_from_edge);
        f64::from(clamped_map(
            distance_from_edge as f32,
            0.0,
            self.max_distance_from_edge_affecting_chance_of_speleothem as f32,
            self.chance_of_speleothem_at_max_distance_from_center,
            1.0,
        ))
    }
}

/// When the stalactite and the stalagmite would overlap, vanilla picks a random meeting point
/// between them.
fn resolve_overlap(
    random: &mut RandomGenerator,
    ceiling: Option<i32>,
    floor: Option<i32>,
    stalactite_height: i32,
    stalagmite_height: i32,
) -> (i32, i32) {
    match (ceiling, floor) {
        (Some(ceiling_y), Some(floor_y))
            if ceiling_y - stalactite_height <= floor_y + stalagmite_height =>
        {
            let lowest_stalactite_bottom = (ceiling_y - stalactite_height).max(floor_y + 1);
            let highest_stalagmite_top = (floor_y + stalagmite_height).min(ceiling_y - 1);
            let stalactite_bottom = random_between_inclusive(
                random,
                lowest_stalactite_bottom,
                highest_stalagmite_top + 1,
            );
            let stalagmite_top = stalactite_bottom - 1;
            (ceiling_y - stalactite_bottom, stalagmite_top - floor_y)
        }
        _ => (stalactite_height, stalagmite_height),
    }
}

fn is_lava(state: BlockStateId) -> bool {
    state.to_block_id() == BlockId::LAVA
}

fn can_be_adjacent_to_water(state: BlockStateId) -> bool {
    super::is_base_stone_overworld(state) || is_water_at(state)
}

/// Vanilla `Mth.randomBetweenInclusive`.
pub(super) fn random_between_inclusive(
    random: &mut RandomGenerator,
    min: i32,
    max_inclusive: i32,
) -> i32 {
    random.next_bounded_i32(max_inclusive - min + 1) + min
}
