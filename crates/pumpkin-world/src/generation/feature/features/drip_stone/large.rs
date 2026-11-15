use pumpkin_data::tag::Tag;
use pumpkin_data::{Block, BlockId, BlockStateId};
use pumpkin_util::math::float_provider::FloatProvider;
use pumpkin_util::math::int_provider::IntProvider;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::random::{RandomGenerator, RandomImpl};

use super::cluster::random_between_inclusive;
use super::{Column, is_circle_mostly_embedded_in_stone, is_empty_or_water_or_lava, state_at};
use crate::generation::proto_chunk::GenerationCache;

/// Vanilla `LargeDripstoneFeature`: a pair of large dripstone columns from the ceiling and the
/// floor of a cave, optionally bent by wind.
pub struct LargeDripstoneFeature {
    pub replaceable_blocks: Tag,
    pub floor_to_ceiling_search_range: i32,
    pub column_radius: IntProvider,
    pub height_scale: FloatProvider,
    pub max_column_radius_to_cave_height_ratio: f32,
    pub stalactite_bluntness: FloatProvider,
    pub stalagmite_bluntness: FloatProvider,
    pub wind_speed: FloatProvider,
    pub min_radius_for_wind: i32,
    pub min_bluntness_for_wind: f32,
}

impl LargeDripstoneFeature {
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        if !super::is_empty_or_water(GenerationCache::get_block_state(chunk, &pos.0)) {
            return false;
        }
        let column = Column::scan(
            chunk,
            pos,
            self.floor_to_ceiling_search_range,
            super::is_empty_or_water,
            |state| self.is_base_or_lava(state),
        );
        let Some(Column {
            floor: Some(floor),
            ceiling: Some(ceiling),
        }) = column
        else {
            return false;
        };
        let column_height = ceiling - floor - 1;
        if column_height < 4 {
            return false;
        }

        let max_radius_for_height =
            (column_height as f32 * self.max_column_radius_to_cave_height_ratio) as i32;
        let max_column_radius =
            max_radius_for_height.clamp(self.column_radius.get_min(), self.column_radius.get_max());
        let radius =
            random_between_inclusive(random, self.column_radius.get_min(), max_column_radius);
        let (x, z) = (pos.0.x, pos.0.z);
        let mut stalactite = LargeDripstone::new(
            BlockPos::new(x, ceiling - 1, z),
            false,
            random,
            radius,
            &self.stalactite_bluntness,
            &self.height_scale,
        );
        let mut stalagmite = LargeDripstone::new(
            BlockPos::new(x, floor + 1, z),
            true,
            random,
            radius,
            &self.stalagmite_bluntness,
            &self.height_scale,
        );
        let wind = if stalactite
            .is_suitable_for_wind(self.min_radius_for_wind, self.min_bluntness_for_wind)
            && stalagmite
                .is_suitable_for_wind(self.min_radius_for_wind, self.min_bluntness_for_wind)
        {
            WindOffsetter::new(pos.0.y, random, &self.wind_speed, 16 - radius)
        } else {
            WindOffsetter::no_wind()
        };

        let stalactite_embedded = stalactite.move_back_until_base_is_inside_stone(chunk, &wind);
        let stalagmite_embedded = stalagmite.move_back_until_base_is_inside_stone(chunk, &wind);
        if stalactite_embedded {
            stalactite.place_blocks(chunk, random, &wind);
        }
        if stalagmite_embedded {
            stalagmite.place_blocks(chunk, random, &wind);
        }
        true
    }

    /// Vanilla `SpeleothemUtils.isBaseOrLava` with the hard-coded dripstone block.
    fn is_base_or_lava(&self, state: BlockStateId) -> bool {
        let id = state.to_block_id();
        id == BlockId::DRIPSTONE_BLOCK || id.has_tag(self.replaceable_blocks) || id == BlockId::LAVA
    }
}

struct LargeDripstone {
    root: BlockPos,
    pointing_up: bool,
    radius: i32,
    bluntness: f64,
    scale: f64,
}

impl LargeDripstone {
    /// Vanilla `makeDripstone`: samples the bluntness, then the height scale.
    fn new(
        root: BlockPos,
        pointing_up: bool,
        random: &mut RandomGenerator,
        radius: i32,
        bluntness: &FloatProvider,
        height_scale: &FloatProvider,
    ) -> Self {
        let bluntness = f64::from(bluntness.get(random));
        let scale = f64::from(height_scale.get(random));
        Self {
            root,
            pointing_up,
            radius,
            bluntness,
            scale,
        }
    }

    fn height_at_radius(&self, check_radius: f32) -> i32 {
        super::speleothem_height(
            f64::from(check_radius),
            f64::from(self.radius),
            self.scale,
            self.bluntness,
        ) as i32
    }

    const fn step(&self) -> i32 {
        if self.pointing_up { 1 } else { -1 }
    }

    /// Vanilla `moveBackUntilBaseIsInsideStoneAndShrinkRadiusIfNecessary`.
    fn move_back_until_base_is_inside_stone<T: GenerationCache>(
        &mut self,
        chunk: &T,
        wind: &WindOffsetter,
    ) -> bool {
        while self.radius > 1 {
            let mut new_root = self.root;
            let max_tries = self.height_at_radius(0.0).min(10);
            for _ in 0..max_tries {
                let (x, y, z) = (new_root.0.x, new_root.0.y, new_root.0.z);
                if state_at(chunk, x, y, z).to_block_id() == BlockId::LAVA {
                    return false;
                }
                if is_circle_mostly_embedded_in_stone(chunk, wind.offset(new_root), self.radius) {
                    self.root = new_root;
                    return true;
                }
                // Back into the rock: down for a stalagmite, up for a stalactite.
                new_root = BlockPos::new(x, y - self.step(), z);
            }
            self.radius /= 2;
        }
        false
    }

    fn place_blocks<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        wind: &WindOffsetter,
    ) {
        for dx in -self.radius..=self.radius {
            for dz in -self.radius..=self.radius {
                let current_radius = ((dx * dx + dz * dz) as f32).sqrt();
                if current_radius > self.radius as f32 {
                    continue;
                }
                let mut height = self.height_at_radius(current_radius);
                if height <= 0 {
                    continue;
                }
                if f64::from(random.next_f32()) < 0.2 {
                    let factor = random.next_f32() * (1.0 - 0.8) + 0.8;
                    height = (height as f32 * factor) as i32;
                }
                let (x, z) = (self.root.0.x + dx, self.root.0.z + dz);
                let mut y = self.root.0.y;
                let max_y = if self.pointing_up {
                    chunk.top_block_wg_height_exclusive(x, z)
                } else {
                    i32::MAX
                };
                let mut has_been_out_of_stone = false;
                let mut i = 0;
                while i < height && y < max_y {
                    let target = wind.offset(BlockPos::new(x, y, z));
                    let state = state_at(chunk, target.0.x, target.0.y, target.0.z);
                    if is_empty_or_water_or_lava(state) {
                        has_been_out_of_stone = true;
                        chunk.set_block_state(&target.0, Block::DRIPSTONE_BLOCK.default_state);
                    } else if has_been_out_of_stone && super::is_base_stone_overworld(state) {
                        break;
                    }
                    y += self.step();
                    i += 1;
                }
            }
        }
    }

    fn is_suitable_for_wind(&self, min_radius_for_wind: i32, min_bluntness_for_wind: f32) -> bool {
        self.radius >= min_radius_for_wind && self.bluntness >= f64::from(min_bluntness_for_wind)
    }
}

/// Vanilla `LargeDripstoneFeature.WindOffsetter`: shifts each layer sideways in proportion to
/// its distance from the origin height.
struct WindOffsetter {
    origin_y: i32,
    wind_speed: Option<(f64, f64)>,
    max_offset: i32,
}

impl WindOffsetter {
    fn new(
        origin_y: i32,
        random: &mut RandomGenerator,
        wind_speed_range: &FloatProvider,
        max_offset: i32,
    ) -> Self {
        let speed = wind_speed_range.get(random);
        // Vanilla `Mth.randomBetween(random, 0, (float) Math.PI)`.
        let direction = random.next_f32() * std::f32::consts::PI;
        let wind_x = f64::from(pumpkin_util::math::cos(direction) * speed);
        let wind_z = f64::from(pumpkin_util::math::sin(direction) * speed);
        Self {
            origin_y,
            wind_speed: Some((wind_x, wind_z)),
            max_offset,
        }
    }

    const fn no_wind() -> Self {
        Self {
            origin_y: 0,
            wind_speed: None,
            max_offset: 0,
        }
    }

    fn offset(&self, pos: BlockPos) -> BlockPos {
        let Some((wind_x, wind_z)) = self.wind_speed else {
            return pos;
        };
        let dy = f64::from(self.origin_y - pos.0.y);
        let dx = ((wind_x * dy).floor() as i32).clamp(-self.max_offset, self.max_offset);
        let dz = ((wind_z * dy).floor() as i32).clamp(-self.max_offset, self.max_offset);
        BlockPos::new(pos.0.x + dx, pos.0.y, pos.0.z + dz)
    }
}
