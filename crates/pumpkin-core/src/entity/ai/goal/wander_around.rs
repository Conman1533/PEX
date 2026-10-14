use super::{Controls, Goal, to_goal_ticks};
use crate::block::PathComputationType;
use crate::entity::ai::util::default_random_pos;
use crate::entity::{ai::pathfinder::NavigatorGoal, mob::Mob};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

pub const DEFAULT_INTERVAL: i32 = 120;

pub struct WanderAroundGoal {
    goal_control: Controls,
    speed: f64,
    target: Option<Vector3<f64>>,
    interval: i32,
    force_trigger: bool,
    get_position: fn(&dyn Mob) -> Option<Vector3<f64>>,
}

impl WanderAroundGoal {
    #[must_use]
    pub const fn new(speed: f64) -> Self {
        Self::with_interval(speed, DEFAULT_INTERVAL)
    }

    #[must_use]
    pub const fn with_interval(speed: f64, interval: i32) -> Self {
        Self {
            goal_control: Controls::MOVE,
            speed,
            target: None,
            interval,
            force_trigger: false,
            get_position: Self::get_position,
        }
    }

    /// Vanilla `RandomSwimmingGoal`: a stroll that only picks positions a mob can swim to.
    #[must_use]
    pub const fn swimming(speed: f64, interval: i32) -> Self {
        let mut goal = Self::with_interval(speed, interval);
        goal.get_position = Self::get_swimmable_position;
        goal
    }

    /// Run once on the next check, ignoring the interval.
    pub const fn trigger(&mut self) {
        self.force_trigger = true;
    }

    pub const fn set_interval(&mut self, interval: i32) {
        self.interval = interval;
    }

    fn get_position(mob: &dyn Mob) -> Option<Vector3<f64>> {
        default_random_pos::get_pos(mob, 10, 7)
    }

    /// Vanilla `BehaviorUtils.getRandomSwimmablePos(mob, 10, 7)`. After ten misses it keeps the
    /// last position even if it isn't water, like vanilla.
    fn get_swimmable_position(mob: &dyn Mob) -> Option<Vector3<f64>> {
        let world = mob.get_entity().world.load();
        let mut position = default_random_pos::get_pos(mob, 10, 7);
        let mut count = 0;
        while let Some(pos) = position {
            let (block, state) = world.get_block_and_state(&BlockPos::floored_v(pos));
            if world
                .block_registry
                .is_pathfindable(block, state, PathComputationType::Water)
                || count >= 10
            {
                break;
            }
            count += 1;
            position = default_random_pos::get_pos(mob, 10, 7);
        }
        position
    }
}

impl Goal for WanderAroundGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        // Every mob that carries a passenger is controlled by it.
        if mob.get_entity().has_passengers() {
            return false;
        }

        if !self.force_trigger {
            // TODO: also bail out on a long no-action time, which needs the despawn bookkeeping.
            if mob
                .get_random()
                .random_range(0..to_goal_ticks(self.interval))
                != 0
            {
                return false;
            }
        }

        self.target = (self.get_position)(mob);
        if self.target.is_none() {
            return false;
        }
        self.force_trigger = false;
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let idle = mob.is_navigator_idle();
        !idle && !mob.get_entity().has_passengers()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = self.target {
            let pos = mob.get_mob_entity().living_entity.entity.pos.load();
            mob.get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_progress(NavigatorGoal::new(pos, target, self.speed));
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.target = None;
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
