use pumpkin_data::effect::StatusEffect;
use pumpkin_data::tag::{self, Tag, Taggable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::sync::atomic::Ordering::Relaxed;

use super::{Controls, Goal, GoalReach};
use crate::entity::ai::goal::follow_owner::FollowOwnerGoal;
use crate::entity::ai::goal::try_find_water::TryFindWaterGoal;
use crate::entity::ai::util::default_random_pos;
use crate::entity::{ai::pathfinder::NavigatorGoal, mob::Mob};

const WATER_SEARCH_XZ: i32 = 5;
/// Vanilla `PanicGoal.WATER_CHECK_DISTANCE_VERTICAL`.
const WATER_CHECK_DISTANCE_VERTICAL: i32 = 1;
const HORIZONTAL_RANGE: i32 = 5;
const VERTICAL_RANGE: i32 = 4;

/// Vanilla's `panicCausingDamageTypes` function.
pub type PanicCauses = fn(&dyn Mob) -> &'static Tag;

pub struct EscapeDangerGoal {
    speed: f64,
    goal_control: Controls,
    target: Option<Vector3<f64>>,
    running: bool,
    panic_causes: PanicCauses,
    teleport_to_owner: bool,
    seek_water_xz: Option<i32>,
}

impl EscapeDangerGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            speed,
            goal_control: Controls::MOVE,
            target: None,
            running: false,
            panic_causes: |_| &tag::DamageType::MINECRAFT_PANIC_CAUSES,
            teleport_to_owner: false,
            seek_water_xz: None,
        })
    }

    /// Damage that makes the mob panic, `#minecraft:panic_causes` unless set here.
    #[must_use]
    pub fn with_panic_causes(mut self: Box<Self>, panic_causes: PanicCauses) -> Box<Self> {
        self.panic_causes = panic_causes;
        self
    }

    /// Vanilla `TamableAnimalPanicGoal`: teleports to the owner when it gets too far away.
    #[must_use]
    pub fn teleporting_to_owner(mut self: Box<Self>) -> Box<Self> {
        self.teleport_to_owner = true;
        self
    }

    /// Vanilla `TurtlePanicGoal`: runs for water within `xz_dist` blocks even when not burning.
    #[must_use]
    pub fn seeking_water(mut self: Box<Self>, xz_dist: i32) -> Box<Self> {
        self.seek_water_xz = Some(xz_dist);
        self
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Only panic-causing damage makes a mob flee, and only while the source is still remembered.
    fn should_panic(&self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .living_entity
            .get_last_damage_type()
            .is_some_and(|damage_type| damage_type.has_tag((self.panic_causes)(mob)))
    }

    /// Nearest water within `xz_dist` blocks horizontally, only when the mob is not stuck in a
    /// block.
    fn look_for_water(mob: &dyn Mob, xz_dist: i32) -> Option<BlockPos> {
        let entity = mob.get_entity();
        let world = entity.world.load();
        let mob_pos = entity.block_pos.load();

        if world
            .get_block_state(&mob_pos)
            .get_block_collision_shapes_at(&mob_pos)
            .next()
            .is_some()
        {
            return None;
        }

        let mut best: Option<(i32, BlockPos)> = None;
        for dx in -xz_dist..=xz_dist {
            for dy in -WATER_CHECK_DISTANCE_VERTICAL..=WATER_CHECK_DISTANCE_VERTICAL {
                for dz in -xz_dist..=xz_dist {
                    let pos = BlockPos::new(mob_pos.0.x + dx, mob_pos.0.y + dy, mob_pos.0.z + dz);
                    if !TryFindWaterGoal::is_water(&world, &pos) {
                        continue;
                    }
                    let distance = dx.abs() + dy.abs() + dz.abs();
                    if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                        best = Some((distance, pos));
                    }
                }
            }
        }

        best.map(|(_, pos)| pos)
    }

    fn find_random_position(mob: &dyn Mob) -> Option<Vector3<f64>> {
        default_random_pos::get_pos(mob, HORIZONTAL_RANGE, VERTICAL_RANGE)
    }
}

impl Goal for EscapeDangerGoal {
    fn tick_reach(&self, mob: &dyn Mob, running: bool) -> Option<GoalReach> {
        // Teleporting to the owner is not audited, like `FollowOwnerGoal`.
        if self.teleport_to_owner {
            return None;
        }
        // Fleeing is the only fast part. Self-inflicted panic before the goals run comes from
        // fire, lava, freezing or effects; hits from others already widen the reach.
        let living = &mob.get_mob_entity().living_entity;
        let entity = &living.entity;
        let may_flee = running
            || self.should_panic(mob)
            || entity.fire_ticks.load(Relaxed) > 0
            || entity.frozen_ticks.load(Relaxed) > 0
            || entity.touching_lava.load(Relaxed)
            || living.has_effect(&StatusEffect::POISON)
            || living.has_effect(&StatusEffect::WITHER);
        Some(GoalReach {
            speed: if may_flee { self.speed } else { 0.0 },
            ..GoalReach::default()
        })
    }

    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if !self.should_panic(mob) {
            return false;
        }

        let water_xz = self
            .seek_water_xz
            .or_else(|| (mob.get_entity().fire_ticks.load(Relaxed) > 0).then_some(WATER_SEARCH_XZ));
        if let Some(xz_dist) = water_xz
            && let Some(water) = Self::look_for_water(mob, xz_dist)
        {
            self.target = Some(water.to_f64());
            return true;
        }

        self.target = Self::find_random_position(mob);
        self.target.is_some()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.is_navigator_idle()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = self.target {
            let pos = mob.get_mob_entity().living_entity.entity.pos.load();
            mob.get_mob_entity()
                .steered()
                .navigator()
                .set_progress(NavigatorGoal::new(pos, target, self.speed));
        }
        self.running = true;
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.target = None;
        self.running = false;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if !self.teleport_to_owner {
            return;
        }
        let owner = FollowOwnerGoal::find_owner(mob);
        if !FollowOwnerGoal::unable_to_move_to_owner(mob, owner.as_deref())
            && let Some(owner) = owner
            && FollowOwnerGoal::should_try_teleport_to_owner(mob, &owner)
        {
            FollowOwnerGoal::try_teleport_to_owner(mob, &owner);
        }
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
