use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::timed::Behavior;
use crate::entity::EntityBase;
use crate::entity::mob::Mob;

/// Vanilla `FollowTemptation.TEMPTATION_COOLDOWN`.
pub const TEMPTATION_COOLDOWN: i32 = 100;
pub const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.5;
pub const BACKED_UP_CLOSE_ENOUGH_DIST: f64 = 3.5;
/// Vanilla overrides `timedOut` so following never times out.
const NEVER_TIMES_OUT: i32 = i32::MAX / 2;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 6] = [
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
    (types::WALK_TARGET.id(), MemoryStatus::Registered),
    (
        types::TEMPTATION_COOLDOWN_TICKS.id(),
        MemoryStatus::ValueAbsent,
    ),
    (types::TEMPTING_PLAYER.id(), MemoryStatus::ValuePresent),
    (types::BREED_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::IS_PANICKING.id(), MemoryStatus::ValueAbsent),
];

/// A value vanilla computes from the mob, like `Function<LivingEntity, Float>`.
pub type MobValue<T> = fn(&dyn Mob) -> T;

fn default_close_enough(_mob: &dyn Mob) -> f64 {
    DEFAULT_CLOSE_ENOUGH_DIST
}

/// Vanilla `FollowTemptation`: walks to and looks at the player holding a temptation.
pub struct FollowTemptation {
    speed_modifier: MobValue<f32>,
    close_enough_distance: MobValue<f64>,
    look_in_the_eyes: bool,
}

impl FollowTemptation {
    #[must_use]
    pub const fn new(speed_modifier: MobValue<f32>) -> Self {
        Self::with_close_enough(speed_modifier, default_close_enough)
    }

    #[must_use]
    pub const fn with_close_enough(
        speed_modifier: MobValue<f32>,
        close_enough_distance: MobValue<f64>,
    ) -> Self {
        Self::with_look_in_the_eyes(speed_modifier, close_enough_distance, false)
    }

    #[must_use]
    pub const fn with_look_in_the_eyes(
        speed_modifier: MobValue<f32>,
        close_enough_distance: MobValue<f64>,
        look_in_the_eyes: bool,
    ) -> Self {
        Self {
            speed_modifier,
            close_enough_distance,
            look_in_the_eyes,
        }
    }
}

impl Behavior for FollowTemptation {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        NEVER_TIMES_OUT
    }

    fn max_duration(&self) -> i32 {
        NEVER_TIMES_OUT
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::TEMPTING_PLAYER.id())
            && !tick.state.has_memory_value(types::BREED_TARGET.id())
            && !tick.state.has_memory_value(types::IS_PANICKING.id())
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        tick.state
            .set(types::TEMPTATION_COOLDOWN_TICKS, TEMPTATION_COOLDOWN);
        tick.state.erase(types::WALK_TARGET.id());
        tick.state.erase(types::LOOK_TARGET.id());
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(player) = tick.state.get(types::TEMPTING_PLAYER).cloned() else {
            return;
        };
        let player: Arc<dyn EntityBase> = player;
        tick.state.set(
            types::LOOK_TARGET,
            Arc::new(EntityTracker::new(player.clone(), true)) as Arc<dyn PositionTracker>,
        );
        let close_enough = (self.close_enough_distance)(tick.mob);
        let distance_sq = tick
            .mob
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&player.get_entity().pos.load());
        if distance_sq < close_enough * close_enough {
            tick.state.erase(types::WALK_TARGET.id());
        } else {
            let tracker = EntityTracker::with_target_eye_height(
                player,
                self.look_in_the_eyes,
                self.look_in_the_eyes,
            );
            tick.state.set(
                types::WALK_TARGET,
                WalkTarget::new(Arc::new(tracker), (self.speed_modifier)(tick.mob), 2),
            );
        }
    }

    fn debug_name(&self) -> &'static str {
        "FollowTemptation"
    }
}
