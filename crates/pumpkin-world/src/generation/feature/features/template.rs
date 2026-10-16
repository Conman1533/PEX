use std::sync::Arc;

use pumpkin_data::block_properties::{
    PointedDripstoneLikeProperties, SpeleothemThickness, VerticalDirection,
};
use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockStateId, Rotation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::{pool::Pool, pool::Weighted, position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};
use rustc_hash::FxHashSet;

use crate::generation::{
    proto_chunk::GenerationCache,
    structure::template::{
        BlockPlacer, BlockState, StructureProcessor, get_template, parse_processor_list,
        place_template,
    },
};

pub struct TemplateFeature {
    pub templates: Vec<Weighted<TemplateEntry>>,
    pub processors: Arc<[StructureProcessor]>,
}

pub struct TemplateEntry {
    pub template: &'static str,
    pub rotations: Vec<Rotation>,
}

impl TemplateFeature {
    /// Parses an inline processor list from a configured feature.
    #[must_use]
    pub fn inline_processors(json: &str) -> Arc<[StructureProcessor]> {
        parse_processor_list(json).unwrap_or_else(|e| {
            tracing::error!("Invalid inline processor list in template feature: {e}");
            Arc::from([])
        })
    }

    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let Some(entry) = Pool::get(&self.templates, random) else {
            return false;
        };
        // Util.getRandom
        let rotation =
            entry.rotations[random.next_bounded_i32(entry.rotations.len() as i32) as usize];
        let Some(template) = get_template(entry.template) else {
            return false;
        };
        if template.palettes().is_empty() {
            return false;
        }
        // StructurePlaceSettings.getRandomPalette draws from the feature random even with one
        // palette. place_template only places the first palette, which is all these templates have.
        random.next_bounded_i32(template.palettes().len() as i32);
        if template.blocks.is_empty() && template.entities.is_empty() {
            return false;
        }

        // Vanilla rotates around the template origin and then centres it with getRotatedOffset,
        // while place_template rotates within the template bounds, so shift by the rotated corner.
        let corner = rotation.rotate_offset(template.size.x - 1, template.size.z - 1);
        let origin = Vector3::new(
            pos.0.x + corner.0.min(0),
            pos.0.y,
            pos.0.z + corner.1.min(0),
        );
        // Waterlogging is done afterwards from the fluid each block replaced, like vanilla.
        let mut placer = GenerationCachePlacer {
            chunk,
            placed: Vec::new(),
        };
        place_template(
            &mut placer,
            &template,
            origin,
            (-(template.size.x / 2), -(template.size.z / 2)),
            rotation,
            false,
            false,
            &self.processors,
            None,
        );
        let placed = placer.placed;
        place_fluids(chunk, &placed);
        update_shapes(chunk, &placed);
        true
    }
}

/// A block set by `place_template`, with whether the position held a water source before.
struct Placed {
    pos: Vector3<i32>,
    replaced_water_source: bool,
}

struct GenerationCachePlacer<'a, T> {
    chunk: &'a mut T,
    placed: Vec<Placed>,
}

impl<T: GenerationCache> BlockPlacer for GenerationCachePlacer<'_, T> {
    fn get_block_state(&self, pos: &Vector3<i32>) -> BlockStateId {
        GenerationCache::get_block_state(self.chunk, pos)
    }

    fn set_block_state(&mut self, pos: &Vector3<i32>, state: &BlockState) {
        self.placed.push(Placed {
            pos: *pos,
            replaced_water_source: is_water_source(&*self.chunk, pos),
        });
        self.chunk.set_block_state(pos, state);
    }

    fn add_block_entity(&mut self, nbt: NbtCompound) {
        // place_template always writes the block entity position.
        let pos = Vector3::new(
            nbt.get_int("x").unwrap_or_default(),
            nbt.get_int("y").unwrap_or_default(),
            nbt.get_int("z").unwrap_or_default(),
        );
        self.chunk.add_block_entity(&pos, nbt);
    }
}

/// Where vanilla's template fill looks for a water source.
const FILL_FROM: [BlockDirection; 5] = [
    BlockDirection::Up,
    BlockDirection::North,
    BlockDirection::East,
    BlockDirection::South,
    BlockDirection::West,
];

/// The six faces of a block.
const ALL: [BlockDirection; 6] = [
    BlockDirection::Down,
    BlockDirection::Up,
    BlockDirection::North,
    BlockDirection::South,
    BlockDirection::West,
    BlockDirection::East,
];

/// Vanilla `Block.UPDATE_SHAPE_ORDER`.
const UPDATE_SHAPE_ORDER: [BlockDirection; 6] = [
    BlockDirection::West,
    BlockDirection::East,
    BlockDirection::North,
    BlockDirection::South,
    BlockDirection::Down,
    BlockDirection::Up,
];

fn is_water_source<T: GenerationCache>(chunk: &T, pos: &Vector3<i32>) -> bool {
    let (fluid, state) = chunk.get_fluid_and_fluid_state(pos);
    fluid.id == Fluid::WATER.id && state.is_source
}

/// Whether the block's own fluid is a source: water at level 0, or a waterlogged block.
fn holds_water_source(state: BlockStateId) -> bool {
    state == Block::WATER.default_state.id || Block::from_state_id(state).is_waterlogged(state)
}

/// `SimpleWaterloggedBlock.placeLiquid` with a water source: waterlogs the block if it can be.
fn waterlog<T: GenerationCache>(chunk: &mut T, pos: &Vector3<i32>) -> bool {
    let state = GenerationCache::get_block_state(chunk, pos);
    let block = Block::from_state_id(state);
    if block.is_waterlogged(state) {
        return false;
    }
    let Some(waterlogged) = block.set_waterlogged(state, true) else {
        return false;
    };
    chunk.set_block_state(pos, BlockState::from_id(waterlogged));
    if let Some(proto) = chunk.get_chunk_mut(pos.x >> 4, pos.z >> 4) {
        proto.schedule_fluid_tick(pos.x, pos.y, pos.z, &Fluid::WATER);
    }
    true
}

/// The fluid steps of vanilla `StructureTemplate.placeInWorld`: a placed block that can hold water
/// takes the water source it replaced; one that replaced no source is filled from a neighbouring
/// source (up, north, east, south, west) that wasn't itself placed as a source by the template.
/// Of vanilla's liquid containers this covers waterloggable blocks, which is what templates use.
fn place_fluids<T: GenerationCache>(chunk: &mut T, placed: &[Placed]) {
    let mut locked = FxHashSet::default();
    let mut to_fill = Vec::new();
    for block in placed {
        let state = GenerationCache::get_block_state(chunk, &block.pos);
        if holds_water_source(state) {
            locked.insert(block.pos);
        } else if Block::from_state_id(state)
            .set_waterlogged(state, true)
            .is_some()
        {
            if block.replaced_water_source {
                waterlog(chunk, &block.pos);
            } else {
                to_fill.push(block.pos);
            }
        }
    }
    let mut filled = true;
    while filled && !to_fill.is_empty() {
        filled = false;
        to_fill.retain(|pos| {
            let has_source = is_water_source(&*chunk, pos)
                || FILL_FROM.iter().any(|dir| {
                    let neighbor = *pos + dir.to_offset();
                    is_water_source(&*chunk, &neighbor) && !locked.contains(&neighbor)
                });
            if has_source && waterlog(chunk, pos) {
                filled = true;
                return false;
            }
            true
        });
    }
}

/// The shape steps of vanilla `StructureTemplate.placeInWorld`: blocks on the edge of the template
/// and their outside neighbours update against each other, then every placed block updates from
/// all its neighbours (`Block.updateFromNeighbourShapes`).
fn update_shapes<T: GenerationCache>(chunk: &mut T, placed: &[Placed]) {
    let inside: FxHashSet<Vector3<i32>> = placed.iter().map(|b| b.pos).collect();
    for block in placed {
        for dir in ALL {
            let neighbor = block.pos + dir.to_offset();
            if inside.contains(&neighbor) {
                continue;
            }
            let state = GenerationCache::get_block_state(chunk, &block.pos);
            let neighbor_state = GenerationCache::get_block_state(chunk, &neighbor);
            let new_state = update_shape(chunk, state, &block.pos, dir, neighbor_state);
            if new_state != state {
                chunk.set_block_state(&block.pos, BlockState::from_id(new_state));
            }
            let new_neighbor =
                update_shape(chunk, neighbor_state, &neighbor, dir.opposite(), new_state);
            if new_neighbor != neighbor_state {
                chunk.set_block_state(&neighbor, BlockState::from_id(new_neighbor));
            }
        }
    }
    for block in placed {
        let state = GenerationCache::get_block_state(chunk, &block.pos);
        let mut new_state = state;
        for dir in UPDATE_SHAPE_ORDER {
            let neighbor_state =
                GenerationCache::get_block_state(chunk, &(block.pos + dir.to_offset()));
            new_state = update_shape(chunk, new_state, &block.pos, dir, neighbor_state);
        }
        if new_state != state {
            chunk.set_block_state(&block.pos, BlockState::from_id(new_state));
        }
    }
}

/// `BlockState.updateShape` for the blocks templates contain that react to their neighbours:
/// water and waterlogged blocks schedule a water tick, and speleothems (pointed dripstone, sulfur
/// spikes) recalculate their thickness. Every other block keeps its state.
fn update_shape<T: GenerationCache>(
    chunk: &mut T,
    state: BlockStateId,
    pos: &Vector3<i32>,
    dir: BlockDirection,
    _neighbor_state: BlockStateId,
) -> BlockStateId {
    let block = Block::from_state_id(state);
    if (state == Block::WATER.default_state.id || block.is_waterlogged(state))
        && let Some(proto) = chunk.get_chunk_mut(pos.x >> 4, pos.z >> 4)
    {
        proto.schedule_fluid_tick(pos.x, pos.y, pos.z, &Fluid::WATER);
    }
    if !block.has_tag(&tag::Block::MINECRAFT_SPELEOTHEMS) {
        return state;
    }
    speleothem_update_shape(chunk, block, state, pos, dir)
}

/// Vanilla `SpeleothemBlock.updateShape`. An unsupported speleothem keeps its state here: vanilla
/// schedules a block tick to drop it, and generation has no block ticks to schedule.
fn speleothem_update_shape<T: GenerationCache>(
    chunk: &T,
    block: &Block,
    state: BlockStateId,
    pos: &Vector3<i32>,
    dir: BlockDirection,
) -> BlockStateId {
    if dir != BlockDirection::Up && dir != BlockDirection::Down {
        return state;
    }
    let mut props = PointedDripstoneLikeProperties::from_state_id(state);
    let tip = match props.vertical_direction {
        VerticalDirection::Up => BlockDirection::Up,
        VerticalDirection::Down => BlockDirection::Down,
    };
    if dir == tip.opposite() && !speleothem_can_survive(chunk, block, pos, tip) {
        return state;
    }
    let merge = props.thickness == SpeleothemThickness::TipMerge;
    props.thickness = speleothem_thickness(chunk, block, pos, tip, merge);
    props.to_state_id(block)
}

/// `SpeleothemBlock.isSpeleothemWithDirection`.
fn is_speleothem_with_direction(state: BlockStateId, tip: BlockDirection) -> bool {
    let block = Block::from_state_id(state);
    if !block.has_tag(&tag::Block::MINECRAFT_SPELEOTHEMS) {
        return false;
    }
    let direction = PointedDripstoneLikeProperties::from_state_id(state).vertical_direction;
    matches!(
        (direction, tip),
        (VerticalDirection::Up, BlockDirection::Up)
            | (VerticalDirection::Down, BlockDirection::Down)
    )
}

/// `SpeleothemBlock.isValidSpeleothemPlacement`, which is also its `canSurvive`.
fn speleothem_can_survive<T: GenerationCache>(
    chunk: &T,
    block: &Block,
    pos: &Vector3<i32>,
    tip: BlockDirection,
) -> bool {
    let behind = GenerationCache::get_block_state(chunk, &(*pos + tip.opposite().to_offset()));
    BlockState::from_id(behind).is_side_solid(tip)
        || (is_speleothem_with_direction(behind, tip)
            && Block::from_state_id(behind).id == block.id)
}

/// `SpeleothemBlock.calculateSpeleothemThickness`.
fn speleothem_thickness<T: GenerationCache>(
    chunk: &T,
    block: &Block,
    pos: &Vector3<i32>,
    tip: BlockDirection,
    merge: bool,
) -> SpeleothemThickness {
    let base = tip.opposite();
    let in_front = GenerationCache::get_block_state(chunk, &(*pos + tip.to_offset()));
    if is_speleothem_with_direction(in_front, base) && Block::from_state_id(in_front).id == block.id
    {
        let front_merge = PointedDripstoneLikeProperties::from_state_id(in_front).thickness
            == SpeleothemThickness::TipMerge;
        return if !merge && !front_merge {
            SpeleothemThickness::Tip
        } else {
            SpeleothemThickness::TipMerge
        };
    }
    if !is_speleothem_with_direction(in_front, tip) {
        return SpeleothemThickness::Tip;
    }
    let front = PointedDripstoneLikeProperties::from_state_id(in_front).thickness;
    if front != SpeleothemThickness::Tip && front != SpeleothemThickness::TipMerge {
        let behind = GenerationCache::get_block_state(chunk, &(*pos + base.to_offset()));
        if is_speleothem_with_direction(behind, tip) {
            SpeleothemThickness::Middle
        } else {
            SpeleothemThickness::Base
        }
    } else {
        SpeleothemThickness::Frustum
    }
}
