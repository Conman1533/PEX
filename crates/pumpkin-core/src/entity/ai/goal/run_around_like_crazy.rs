use std::sync::Weak;
use std::sync::atomic::{AtomicI32, Ordering};

use super::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::util::default_random_pos;
use crate::entity::mob::Mob;
use crate::entity::player::Player;
use pumpkin_data::entity::EntityStatus;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

/// The vanilla `AbstractHorse` taming state `RunAroundLikeCrazyGoal` drives.
pub trait RideTamable: Mob {
    fn is_ride_tamed(&self) -> bool;
    fn temper(&self) -> &AtomicI32;
    /// Vanilla `getMaxTemper`: 100, llamas 30.
    fn max_temper(&self) -> i32 {
        100
    }
    /// Vanilla `tameWithName` without the entity event, which the goal sends.
    fn tame_with_name(&self, player: &Player);
    /// Vanilla `makeMad`.
    fn make_mad(&self);
    /// Vanilla `modifyTemper`: clamped to `0..=max_temper`.
    fn modify_temper(&self, amount: i32) {
        let max = self.max_temper();
        let temper = self.temper();
        let next = (temper.load(Ordering::Relaxed) + amount).clamp(0, max);
        temper.store(next, Ordering::Relaxed);
    }
}

/// Vanilla `RunAroundLikeCrazyGoal`: an untamed horse with a rider runs about, and every so
/// often either accepts a player rider (more likely the higher its temper) or bucks them off.
pub struct RunAroundLikeCrazyGoal {
    horse: Weak<dyn RideTamable>,
    goal_control: Controls,
    speed_modifier: f64,
    pos_x: f64,
    pos_y: f64,
    pos_z: f64,
}

impl RunAroundLikeCrazyGoal {
    #[must_use]
    pub const fn new(horse: Weak<dyn RideTamable>, speed_modifier: f64) -> Self {
        Self {
            horse,
            goal_control: Controls::MOVE,
            speed_modifier,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
        }
    }

    fn is_tamed(&self) -> bool {
        self.horse
            .upgrade()
            .is_none_or(|horse| horse.is_ride_tamed())
    }
}

impl Goal for RunAroundLikeCrazyGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if self.is_tamed() || !mob.get_entity().has_passengers() {
            return false;
        }

        let Some(pos) = default_random_pos::get_pos(mob, 5, 4) else {
            return false;
        };

        self.pos_x = pos.x;
        self.pos_y = pos.y;
        self.pos_z = pos.z;
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let is_idle = mob.is_navigator_idle();

        !self.is_tamed() && !is_idle && mob.get_entity().has_passengers()
    }

    fn start(&mut self, mob: &dyn Mob) {
        let mob_pos = mob.get_entity().pos.load();
        let steered = mob.get_mob_entity().steered();
        let mut navigator = steered.navigator();
        navigator.set_progress(NavigatorGoal::new(
            mob_pos,
            Vector3::new(self.pos_x, self.pos_y, self.pos_z),
            self.speed_modifier,
        ));
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if self.is_tamed() || mob.get_random().random_range(0..to_goal_ticks(50)) != 0 {
            return;
        }
        let Some(horse) = self.horse.upgrade() else {
            return;
        };
        let entity = mob.get_entity();
        let passengers = entity
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(first) = passengers.first() else {
            return;
        };
        let world = entity.world.load();
        if let Some(player) = first.get_player() {
            let temper = horse.temper().load(Ordering::Relaxed);
            let max_temper = horse.max_temper();
            if max_temper > 0 && mob.get_random().random_range(0..max_temper) < temper {
                horse.tame_with_name(player);
                world.send_entity_status(
                    entity,
                    EntityStatus::TamingSucceeded,
                    ActorEventID::TamingSucceeded,
                );
                return;
            }
            horse.modify_temper(5);
        }
        for passenger in &passengers {
            entity.remove_passenger(passenger.get_entity().entity_id);
        }
        horse.make_mad();
        world.send_entity_status(
            entity,
            EntityStatus::TamingFailed,
            ActorEventID::TamingFailed,
        );
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
