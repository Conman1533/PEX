//! Driver for the sculk-patch configured feature.
//!
//! This module keeps the public struct / field layout required by the
//! codegen (`configured_features_generated.rs`) and the two generation entry
//! points (`generate` / `generate_in_proto_chunk`). The actual spreading
//! algorithm lives in the [`sculk`] sub-module.

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::random::RandomGenerator;

use crate::generation::feature::features::sculk;
use crate::generation::feature::features::sculk::ProtoChunkSculkView;
use crate::generation::feature::features::sculk::spreader::SculkSpreader;
use crate::generation::proto_chunk::GenerationCache;
use crate::world::WorldPortalExt;

pub struct SculkPatchFeature {
    pub charge_count: i32,
    pub amount_per_charge: i32,
    pub spread_attempts: i32,
    pub growth_rounds: i32,
    pub spread_rounds: i32,
}

impl SculkPatchFeature {
    /// Post-terrain generation entry point. `T: GenerationCache` also
    /// implements `SculkLevel` via the blanket impl.
    pub fn generate<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        chunk: &mut T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        if !sculk::can_spread_from(chunk, pos) {
            return false;
        }

        let mut spreader = SculkSpreader::new_world_gen_spreader();
        let total_rounds = self.spread_rounds + self.growth_rounds;

        for round in 0..total_rounds {
            for _ in 0..self.charge_count {
                spreader.add_cursors(pos, self.amount_per_charge);
            }

            for _ in 0..self.spread_attempts {
                spreader.update_cursors(chunk, pos, random, round < self.spread_rounds);
            }

            spreader.clear();
        }

        true
    }

    /// In-chunk (proto-chunk) generation entry point used during terrain.
    pub fn generate_in_proto_chunk(
        &self,
        chunk: &mut crate::ProtoChunk,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let mut view = ProtoChunkSculkView::new(chunk);
        if !sculk::can_spread_from(&view, pos) {
            return false;
        }

        let mut spreader = SculkSpreader::new_world_gen_spreader();
        let total_rounds = self.spread_rounds + self.growth_rounds;
        for round in 0..total_rounds {
            for _ in 0..self.charge_count {
                spreader.add_cursors(pos, self.amount_per_charge);
            }
            for _ in 0..self.spread_attempts {
                spreader.update_cursors(&mut view, pos, random, round < self.spread_rounds);
            }
            spreader.clear();
        }

        true
    }
}
