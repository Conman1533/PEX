//! Vanilla `JumpOnBed`.

use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;
use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::Behavior;
use super::utils::is_baby;

/// Vanilla `JumpOnBed.MAX_TIME_TO_REACH_BED`.
const MAX_TIME_TO_REACH_BED: i32 = 100;
/// Vanilla `JumpOnBed.MIN_JUMPS`.
const MIN_JUMPS: i32 = 3;
/// Vanilla `JumpOnBed.MAX_JUMPS`.
const MAX_JUMPS: i32 = 6;
/// Vanilla `JumpOnBed.COOLDOWN_BETWEEN_JUMPS`.
const COOLDOWN_BETWEEN_JUMPS: i32 = 5;

const ENTRY: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::NEAREST_BED.id(), MemoryStatus::ValuePresent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
];

/// Vanilla `JumpOnBed`: a baby walks to the nearest bed and bounces on it a few times.
pub struct JumpOnBed {
    speed_modifier: f32,
    target_bed: Option<BlockPos>,
    remaining_time_to_reach_bed: i32,
    remaining_jumps: i32,
    remaining_cooldown_until_next_jump: i32,
}

impl JumpOnBed {
    #[must_use]
    pub const fn new(speed_modifier: f32) -> Self {
        Self {
            speed_modifier,
            target_bed: None,
            remaining_time_to_reach_bed: 0,
            remaining_jumps: 0,
            remaining_cooldown_until_next_jump: 0,
        }
    }

    fn is_jumpable(tick: &BrainTick<'_>, pos: &BlockPos) -> bool {
        tick.world
            .get_block(pos)
            .has_tag(&tag::Block::MINECRAFT_VILLAGER_BABIES_CAN_JUMP_ON_BED)
    }

    fn on_or_over_bed(tick: &BrainTick<'_>) -> bool {
        let body = tick.mob.get_entity().block_pos.load();
        Self::is_jumpable(tick, &body) || Self::is_jumpable(tick, &body.down())
    }

    fn on_bed_surface(tick: &BrainTick<'_>) -> bool {
        Self::is_jumpable(tick, &tick.mob.get_entity().block_pos.load())
    }
}

impl Behavior for JumpOnBed {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY
    }

    // Vanilla `timedOut` is always false.
    fn min_duration(&self) -> i32 {
        i32::MAX
    }

    fn max_duration(&self) -> i32 {
        i32::MAX
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        is_baby(tick.mob)
            && (Self::on_or_over_bed(tick) || tick.state.has_memory_value(types::NEAREST_BED.id()))
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(bed) = tick.state.get(types::NEAREST_BED).copied() {
            self.target_bed = Some(bed);
            self.remaining_time_to_reach_bed = MAX_TIME_TO_REACH_BED;
            self.remaining_jumps = MIN_JUMPS
                + tick
                    .mob
                    .get_random()
                    .random_range(0..=MAX_JUMPS - MIN_JUMPS);
            self.remaining_cooldown_until_next_jump = 0;
            tick.state.set(
                types::WALK_TARGET,
                WalkTarget::from_block_pos(bed, self.speed_modifier, 0),
            );
        }
    }

    fn stop(&mut self, _tick: &mut BrainTick<'_>) {
        self.target_bed = None;
        self.remaining_time_to_reach_bed = 0;
        self.remaining_jumps = 0;
        self.remaining_cooldown_until_next_jump = 0;
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(bed) = self.target_bed else {
            return false;
        };
        let on_or_over = Self::on_or_over_bed(tick);
        // Vanilla: still walking there with time left, or on the bed with jumps left.
        let reaching = !on_or_over && self.remaining_time_to_reach_bed > 0;
        let jumping = on_or_over && self.remaining_jumps > 0;
        is_baby(tick.mob) && Self::is_jumpable(tick, &bed) && (reaching || jumping)
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        if !Self::on_or_over_bed(tick) {
            self.remaining_time_to_reach_bed -= 1;
        } else if self.remaining_cooldown_until_next_jump > 0 {
            self.remaining_cooldown_until_next_jump -= 1;
        } else if Self::on_bed_surface(tick) {
            tick.mob
                .get_mob_entity()
                .jump_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .jump();
            self.remaining_jumps -= 1;
            self.remaining_cooldown_until_next_jump = COOLDOWN_BETWEEN_JUMPS;
        }
    }

    fn debug_name(&self) -> &'static str {
        "JumpOnBed"
    }
}
