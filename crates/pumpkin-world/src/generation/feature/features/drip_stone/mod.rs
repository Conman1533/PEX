//! Speleothem (pointed dripstone, sulfur spike) world generation: vanilla `SpeleothemUtils` and
//! `Column`, shared by the three features in this module.

use crate::generation::proto_chunk::GenerationCache;
use pumpkin_data::block_properties::{
    PointedDripstoneLikeProperties, SpeleothemThickness, VerticalDirection,
};
use pumpkin_data::tag::{self, Tag};
use pumpkin_data::{Block, BlockDirection, BlockId, BlockState, BlockStateId};
use pumpkin_util::math::position::BlockPos;

pub mod cluster;
pub mod large;
pub mod small;

/// The blocks a speleothem feature builds with: vanilla's `base_block`, `pointed_block` and
/// `replaceable_blocks` fields.
pub struct SpeleothemBlocks {
    pub base_block: &'static Block,
    pub pointed_block: &'static Block,
    pub replaceable_blocks: Tag,
}

impl SpeleothemBlocks {
    /// Vanilla `SpeleothemUtils.isBase`.
    fn is_base(&self, state: BlockStateId) -> bool {
        let id = state.to_block_id();
        id == self.base_block.id || id.has_tag(self.replaceable_blocks)
    }

    /// Vanilla `SpeleothemUtils.placeBaseBlockIfPossible`.
    fn place_base_block_if_possible<T: GenerationCache>(
        &self,
        chunk: &mut T,
        pos: BlockPos,
    ) -> bool {
        let id = GenerationCache::get_block_state(chunk, &pos.0).to_block_id();
        if id.has_tag(self.replaceable_blocks) {
            chunk.set_block_state(&pos.0, self.base_block.default_state);
            return true;
        }
        false
    }

    /// Vanilla `SpeleothemUtils.growSpeleothem`: a column of pointed blocks from `start_pos`
    /// towards `tip_direction`, only when the block behind the start is a base block.
    fn grow<T: GenerationCache>(
        &self,
        chunk: &mut T,
        start_pos: BlockPos,
        tip_direction: BlockDirection,
        height: i32,
        merged_tip: bool,
    ) {
        let behind = start_pos.offset(tip_direction.opposite().to_offset());
        if !self.is_base(GenerationCache::get_block_state(chunk, &behind.0)) {
            return;
        }
        let vertical_direction = if tip_direction == BlockDirection::Down {
            VerticalDirection::Down
        } else {
            VerticalDirection::Up
        };
        let mut pos = start_pos;
        for thickness in base_to_tip_column(height, merged_tip) {
            let mut props = PointedDripstoneLikeProperties::default(self.pointed_block);
            props.thickness = thickness;
            props.vertical_direction = vertical_direction;
            props.waterlogged = is_water_at(GenerationCache::get_block_state(chunk, &pos.0));
            let state = BlockState::from_id(props.to_state_id(self.pointed_block));
            chunk.set_block_state(&pos.0, state);
            pos = pos.offset(tip_direction.to_offset());
        }
    }
}

/// Vanilla `SpeleothemUtils.buildBaseToTipColumn`: the thicknesses from base to tip.
fn base_to_tip_column(length: i32, merged_tip: bool) -> Vec<SpeleothemThickness> {
    let mut column = Vec::with_capacity(length.max(0) as usize);
    if length >= 3 {
        column.push(SpeleothemThickness::Base);
        for _ in 0..length - 3 {
            column.push(SpeleothemThickness::Middle);
        }
    }
    if length >= 2 {
        column.push(SpeleothemThickness::Frustum);
    }
    if length >= 1 {
        column.push(if merged_tip {
            SpeleothemThickness::TipMerge
        } else {
            SpeleothemThickness::Tip
        });
    }
    column
}

/// Vanilla `LevelReader.isWaterAt`: the fluid at the position is water (also waterlogged blocks).
fn is_water_at(state: BlockStateId) -> bool {
    state.to_block_id() == BlockId::WATER || state.is_waterlogged()
}

/// Vanilla `SpeleothemUtils.isEmptyOrWater`.
pub(super) fn is_empty_or_water(state: BlockStateId) -> bool {
    state.to_state().is_air() || state.to_block_id() == BlockId::WATER
}

/// Vanilla `SpeleothemUtils.isNeitherEmptyNorWater`.
fn is_neither_empty_nor_water(state: BlockStateId) -> bool {
    !is_empty_or_water(state)
}

/// Vanilla `SpeleothemUtils.isEmptyOrWaterOrLava`.
fn is_empty_or_water_or_lava(state: BlockStateId) -> bool {
    is_empty_or_water(state) || state.to_block_id() == BlockId::LAVA
}

fn is_base_stone_overworld(state: BlockStateId) -> bool {
    state
        .to_block_id()
        .has_tag(tag::Block::MINECRAFT_BASE_STONE_OVERWORLD)
}

fn state_at<T: GenerationCache>(chunk: &T, x: i32, y: i32, z: i32) -> BlockStateId {
    GenerationCache::get_block_state(chunk, &BlockPos::new(x, y, z).0)
}

/// Vanilla `SpeleothemUtils.getSpeleothemHeight`: the height of a large speleothem at a
/// horizontal distance from its centre.
fn speleothem_height(
    xz_distance_from_center: f64,
    speleothem_radius: f64,
    scale: f64,
    bluntness: f64,
) -> f64 {
    let distance = xz_distance_from_center.max(bluntness);
    let r = distance / speleothem_radius * 0.384;
    let part1 = 0.75 * r.powf(1.333_333_333_333_333_3);
    let part2 = r.powf(0.666_666_666_666_666_6);
    let part3 = 0.333_333_333_333_333_3 * r.ln();
    let height_relative_to_max_radius = (scale * (part1 - part2 - part3)).max(0.0);
    height_relative_to_max_radius / 0.384 * speleothem_radius
}

/// Vanilla `SpeleothemUtils.isCircleMostlyEmbeddedInStone`.
fn is_circle_mostly_embedded_in_stone<T: GenerationCache>(
    chunk: &T,
    center: BlockPos,
    xz_radius: i32,
) -> bool {
    let (x, y, z) = (center.0.x, center.0.y, center.0.z);
    if is_empty_or_water_or_lava(state_at(chunk, x, y, z)) {
        return false;
    }
    let angle_increment = 6.0f32 / xz_radius as f32;
    let full_turn = std::f64::consts::TAU as f32;
    let mut angle = 0.0f32;
    while angle < full_turn {
        let dx = (pumpkin_util::math::cos(angle) * xz_radius as f32) as i32;
        let dz = (pumpkin_util::math::sin(angle) * xz_radius as f32) as i32;
        if is_empty_or_water_or_lava(state_at(chunk, x + dx, y, z + dz)) {
            return false;
        }
        angle += angle_increment;
    }
    true
}

/// Vanilla `Column`: the open space found by scanning up and down from a position.
#[derive(Clone, Copy)]
struct Column {
    floor: Option<i32>,
    ceiling: Option<i32>,
}

impl Column {
    /// Vanilla `Column.getHeight`: only a column bounded on both sides has a height.
    const fn height(self) -> Option<i32> {
        match (self.floor, self.ceiling) {
            (Some(floor), Some(ceiling)) => Some(ceiling - floor - 1),
            _ => None,
        }
    }

    /// Vanilla `Column.scan`.
    fn scan<T: GenerationCache>(
        chunk: &T,
        pos: BlockPos,
        search_range: i32,
        inside_column: impl Fn(BlockStateId) -> bool,
        valid_edge: impl Fn(BlockStateId) -> bool,
    ) -> Option<Self> {
        let (x, y, z) = (pos.0.x, pos.0.y, pos.0.z);
        if !inside_column(state_at(chunk, x, y, z)) {
            return None;
        }
        let scan = |step: i32| {
            let mut cur = y;
            let mut i = 1;
            while i < search_range && inside_column(state_at(chunk, x, cur, z)) {
                cur += step;
                i += 1;
            }
            valid_edge(state_at(chunk, x, cur, z)).then_some(cur)
        };
        let ceiling = scan(1);
        let floor = scan(-1);
        Some(Self { floor, ceiling })
    }
}
