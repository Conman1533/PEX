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
    (fluid.id == Fluid::WATER.id && state.is_source)
        || holds_water_source(GenerationCache::get_block_state(chunk, pos))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::{
        feature::{
            configured_features::CONFIGURED_FEATURES,
            placed_features::{PLACED_FEATURES, PlacementModifier},
        },
        get_world_gen,
        proto_chunk::ProtoChunk,
    };
    use crate::world::WorldPortalExt;
    use pumpkin_data::{BlockId, dimension::Dimension};
    use pumpkin_util::{random::xoroshiro128::Xoroshiro, world_seed::Seed};

    struct TestBlockRegistry;
    impl WorldPortalExt for TestBlockRegistry {
        fn can_place_at(
            &self,
            _block: &pumpkin_data::Block,
            _state: &pumpkin_data::BlockState,
            _block_accessor: &dyn crate::world::BlockAccessor,
            _block_pos: &BlockPos,
        ) -> bool {
            true
        }

        fn mirror(
            &self,
            block: &pumpkin_data::Block,
            state_id: BlockStateId,
            mirror: pumpkin_data::Mirror,
        ) -> &'static pumpkin_data::BlockState {
            block.mirror(state_id, mirror)
        }

        fn rotate(
            &self,
            block: &pumpkin_data::Block,
            state_id: BlockStateId,
            rotation: pumpkin_data::Rotation,
        ) -> &'static pumpkin_data::BlockState {
            block.rotate(state_id, rotation)
        }

        fn spawn_mobs_for_chunk_generation(
            &self,
            _cache: &mut dyn crate::generation::proto_chunk::GenerationCache,
            _biome: &'static pumpkin_data::chunk::Biome,
            _chunk_x: i32,
            _chunk_z: i32,
        ) {
        }
    }

    #[test]
    fn sulfur_spring_places_tuff_patch_and_spring_template_with_spikes_and_water() {
        let world_gen = get_world_gen(
            Seed(2026),
            Dimension::OVERWORLD,
            false,
            Vec::new(),
            String::new(),
        );
        let mut chunk = ProtoChunk::new(0, 0, &world_gen);
        for x in 0..16 {
            for z in 0..16 {
                for y in 40..=64 {
                    chunk.set_block_state(x, y, z, Block::STONE.default_state);
                }
            }
        }

        let feature = CONFIGURED_FEATURES
            .get(&pumpkin_data::configured_feature::ConfiguredFeature::SulfurSpring)
            .expect("sulfur_spring configured feature");
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(42));
        let placed = feature.generate(
            &mut chunk,
            &TestBlockRegistry,
            -64,
            384,
            pumpkin_data::placed_feature::PlacedFeature::RootedSulfurSpring,
            &mut random,
            BlockPos::new(8, 65, 8),
        );
        assert!(placed, "sulfur_spring should place on a solid cave floor");

        let mut tuff_count = 0;
        let mut sulfur_count = 0;
        let mut water_count = 0;
        let mut spike_count = 0;
        for x in 0..16 {
            for z in 0..16 {
                for y in 45..=70 {
                    let state_id = GenerationCache::get_block_state(&chunk, &Vector3::new(x, y, z));
                    let block_id = state_id.to_block_id();
                    if block_id == BlockId::TUFF {
                        tuff_count += 1;
                    } else if block_id == BlockId::SULFUR {
                        sulfur_count += 1;
                    } else if block_id == BlockId::WATER {
                        water_count += 1;
                    } else if block_id == BlockId::SULFUR_SPIKE {
                        spike_count += 1;
                    }
                }
            }
        }

        assert!(tuff_count > 0, "sequence step 1 should place tuff patch");
        assert!(
            sulfur_count > 0,
            "sequence step 2 should place sulfur blocks"
        );
        assert!(water_count > 0, "spring template should place water");
        assert!(
            spike_count > 0 || !chunk.fluid_ticks.is_empty(),
            "spring template should schedule water fluid ticks"
        );
    }

    #[test]
    fn desert_well_volume_match_rejects_air_pockets_and_places_on_solid_sand() {
        let world_gen = get_world_gen(
            Seed(2026),
            Dimension::OVERWORLD,
            false,
            Vec::new(),
            String::new(),
        );
        let mut chunk = ProtoChunk::new(0, 0, &world_gen);
        for x in 0..16 {
            for z in 0..16 {
                for y in 55..=64 {
                    chunk.set_block_state(x, y, z, Block::SAND.default_state);
                }
            }
        }

        let placed_well = PLACED_FEATURES
            .get(&pumpkin_data::placed_feature::PlacedFeature::DesertWell)
            .expect("desert_well placed feature");
        let filter = placed_well
            .placement
            .iter()
            .find_map(|m| match m {
                PlacementModifier::BlockPredicateFilter(f) => Some(&f.predicate),
                _ => None,
            })
            .expect("desert_well BlockPredicateFilter");

        // After the (0, -1, 0) offset, the well checks pos = (8, 64, 8), which requires
        // minecraft:sand at (8, 64, 8) and no #minecraft:air in (6..=10, 62..=63, 6..=10).
        let check_pos = BlockPos::new(8, 64, 8);
        chunk.set_block_state(7, 62, 8, Block::AIR.default_state);
        assert!(
            !filter.test(&TestBlockRegistry, &chunk, &check_pos),
            "volume_match must reject desert well placement over an air pocket"
        );

        chunk.set_block_state(7, 62, 8, Block::SAND.default_state);
        assert!(
            filter.test(&TestBlockRegistry, &chunk, &check_pos),
            "volume_match must accept desert well placement when underground volume has no air"
        );

        let configured_well = CONFIGURED_FEATURES
            .get(&pumpkin_data::configured_feature::ConfiguredFeature::DesertWell)
            .expect("desert_well configured feature");
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(7));
        let generated = configured_well.generate(
            &mut chunk,
            &TestBlockRegistry,
            -64,
            384,
            pumpkin_data::placed_feature::PlacedFeature::DesertWell,
            &mut random,
            check_pos,
        );
        assert!(
            generated,
            "desert_well should generate at valid sand position"
        );

        let mut sandstone = 0;
        let mut water = 0;
        let mut suspicious_sand = 0;
        for x in 4..=12 {
            for z in 4..=12 {
                for y in 60..=70 {
                    let block_id = GenerationCache::get_block_state(&chunk, &Vector3::new(x, y, z))
                        .to_block_id();
                    if block_id == BlockId::SANDSTONE || block_id == BlockId::SANDSTONE_SLAB {
                        sandstone += 1;
                    } else if block_id == BlockId::WATER {
                        water += 1;
                    } else if block_id == BlockId::SUSPICIOUS_SAND {
                        suspicious_sand += 1;
                    }
                }
            }
        }
        assert!(sandstone > 0, "desert_well should place sandstone");
        assert!(water > 0, "desert_well should place water");
        assert_eq!(
            suspicious_sand, 2,
            "desert_well should place 2 suspicious sand blocks"
        );
        assert!(
            chunk.pending_block_entities.iter().any(|nbt| {
                nbt.get_string("id") == Some("minecraft:brushable_block")
                    && nbt.get_string("LootTable") == Some("minecraft:archaeology/desert_well")
            }),
            "desert_well suspicious sand should carry its archaeology loot table NBT"
        );
    }
}
