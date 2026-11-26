//! Vanilla's `AbstractFish`: what cod, salmon, tropical fish and pufferfish share.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use crate::entity::{
    EntityBase,
    ai::{
        control::fish_move_control::FishMoveControl,
        goal::{Controls, Goal, random_swimming::RandomSwimmingGoal},
        pathfinder::{Navigator, node::PathType},
    },
    mob::{Mob, MobEntity},
};
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::water_animal::WaterAnimal;

/// What every fish keeps beyond the mob: whether it came out of a bucket.
#[derive(Default)]
pub struct FishData {
    pub from_bucket: AtomicBool,
}

const WATER_ACCELERATION: f32 = 0.01;
const WATER_FRICTION: f64 = 0.9;
const SINK_SPEED: f64 = 0.005;
const FLOP_HORIZONTAL: f32 = 0.05;
const FLOP_VERTICAL: f32 = 0.4;

/// Vanilla `AbstractFish`'s navigation and move control, in place of the walking
/// ones every mob starts with.
pub fn initialize_fish_movement(mob: &mut MobEntity) {
    let navigator = mob
        .navigator
        .get_mut()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *navigator = Navigator::water_bound(false);
    navigator.set_pathfinding_malus(PathType::Water, 0.0);
    *mob.move_control
        .get_mut()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(FishMoveControl::default());
}

const PANIC_SPEED: f64 = 1.25;
const AVOID_PLAYER_RANGE: f64 = 8.0;
const AVOID_PLAYER_WALK_SPEED: f64 = 1.6;
const AVOID_PLAYER_SPRINT_SPEED: f64 = 1.4;
const RANDOM_SWIM_SPEED: f64 = 1.0;
const RANDOM_SWIM_INTERVAL: i32 = 40;

/// Vanilla `AbstractFish.registerGoals`: panic when hurt, keep clear of players and
/// swim to random spots. Schooling fish add `FollowFlockLeaderGoal` at 5.
pub fn register_fish_goals(mob: &MobEntity) {
    use crate::entity::ai::goal::{avoid_entity::AvoidEntityGoal, escape_danger::EscapeDangerGoal};
    let goals = &mob.goals_selector;
    goals.add_goal(0, EscapeDangerGoal::new(PANIC_SPEED));
    goals.add_goal(
        2,
        Box::new(AvoidEntityGoal::new(
            &pumpkin_data::entity::EntityType::PLAYER,
            AVOID_PLAYER_RANGE,
            AVOID_PLAYER_WALK_SPEED,
            AVOID_PLAYER_SPRINT_SPEED,
        )),
    );
    goals.add_goal(4, Box::new(FishSwimGoal::default()));
}

struct FishSwimGoal {
    inner: RandomSwimmingGoal,
}

impl Default for FishSwimGoal {
    fn default() -> Self {
        Self {
            inner: RandomSwimmingGoal::new(RANDOM_SWIM_SPEED, RANDOM_SWIM_INTERVAL),
        }
    }
}

impl Goal for FishSwimGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.as_fish().is_some_and(AbstractFish::can_random_swim) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

pub trait AbstractFish: WaterAnimal + crate::entity::bucketable::Bucketable {
    fn flop_sound(&self) -> Sound;

    fn can_random_swim(&self) -> bool {
        self.as_schooling_fish()
            .is_none_or(|fish| !fish.is_follower())
    }

    fn fish_travel_in_water(&self, caller: &dyn EntityBase) {
        let mob = self.get_mob_entity();
        let living = &mob.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(
            living.movement_input.load(),
            f64::from(WATER_ACCELERATION),
        );
        living.make_move(caller);
        let mut velocity = entity.velocity.load() * WATER_FRICTION;
        if mob
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            velocity.y -= SINK_SPEED;
        }
        entity.velocity.store(velocity);
    }

    fn fish_ai_step(&self) {
        let entity = self.get_entity();
        if !entity.is_in_water()
            && entity.on_ground.load(Relaxed)
            && entity.vertical_collision.load(Relaxed)
        {
            let mut rng = self.get_random();
            entity.velocity.store(
                entity.velocity.load()
                    + Vector3::new(
                        f64::from((rng.random::<f32>() * 2.0 - 1.0) * FLOP_HORIZONTAL),
                        f64::from(FLOP_VERTICAL),
                        f64::from((rng.random::<f32>() * 2.0 - 1.0) * FLOP_HORIZONTAL),
                    ),
            );
            entity.on_ground.store(false, Relaxed);
            entity.velocity_dirty.store(true, Relaxed);
            if !entity.is_silent() {
                entity.world.load().play_sound_raw(
                    self.flop_sound() as u16,
                    pumpkin_data::sound::SoundCategory::Neutral,
                    &entity.pos.load(),
                    1.0,
                    self.get_mob_entity().living_entity.get_pitch(),
                );
            }
        }
    }

    fn fish_data(&self) -> &FishData;

    fn fish_is_from_bucket(&self) -> bool {
        self.fish_data().from_bucket.load(Relaxed)
    }

    fn fish_set_from_bucket(&self, from_bucket: bool) {
        self.fish_data().from_bucket.store(from_bucket, Relaxed);
        self.get_entity().set_synced_data(
            pumpkin_data::tracked_data::abstract_fish::FROM_BUCKET,
            from_bucket,
        );
    }

    fn fish_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("FromBucket", self.is_from_bucket());
    }

    fn fish_read_nbt(&self, nbt: &NbtCompound) {
        self.set_from_bucket(nbt.get_bool("FromBucket").unwrap_or(false));
    }

    /// Vanilla `Mob.playAmbientSound`, also played when a fish is let out of a bucket.
    fn play_ambient_sound(&self) {
        if let Some(sound) = self.get_ambient_sound() {
            self.make_sound(sound);
        }
    }

    /// Vanilla `WaterAnimal.baseTick`: out of water a fish runs out of air.
    fn fish_base_tick(&self, caller: &dyn crate::entity::EntityBase) {
        self.handle_air_supply(caller);
    }

    /// Vanilla `AbstractFish.removeWhenFarAway`.
    fn fish_remove_when_far_away(&self) -> bool {
        !self.is_from_bucket() && self.get_entity().custom_name.load().is_none()
    }

    /// Vanilla `AbstractFish.requiresCustomPersistence`.
    fn fish_requires_custom_persistence(&self) -> bool {
        let entity = self.get_entity();
        self.is_from_bucket()
            || entity
                .vehicle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
            || entity
                .leashed_to
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
    }
}
