use std::sync::Arc;

use pumpkin_data::{BlockStateId, Rotation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::{pool::Pool, pool::Weighted, position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};

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
        place_template(
            &mut GenerationCachePlacer(chunk),
            &template,
            origin,
            (-(template.size.x / 2), -(template.size.z / 2)),
            rotation,
            false,
            true,
            &self.processors,
            None,
        );
        true
    }
}

struct GenerationCachePlacer<'a, T>(&'a mut T);

impl<T: GenerationCache> BlockPlacer for GenerationCachePlacer<'_, T> {
    fn get_block_state(&self, pos: &Vector3<i32>) -> BlockStateId {
        GenerationCache::get_block_state(self.0, pos)
    }

    fn set_block_state(&mut self, pos: &Vector3<i32>, state: &BlockState) {
        self.0.set_block_state(pos, state);
    }

    fn add_block_entity(&mut self, nbt: NbtCompound) {
        // place_template always writes the block entity position.
        let pos = Vector3::new(
            nbt.get_int("x").unwrap_or_default(),
            nbt.get_int("y").unwrap_or_default(),
            nbt.get_int("z").unwrap_or_default(),
        );
        self.0.add_block_entity(&pos, nbt);
    }
}
