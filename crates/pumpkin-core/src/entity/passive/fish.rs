//! Vanilla `AbstractFish`, with the parts of `WaterAnimal` it relies on. Shared by cod, salmon,
//! tropical fish and pufferfish.

use std::sync::atomic::Ordering;

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use rand::RngExt;

use crate::entity::EntityBase;
use crate::entity::ai::control::fish_move_control::FishMoveControl;
use crate::entity::ai::goal::avoid_entity::AvoidEntityGoal;
use crate::entity::ai::goal::escape_danger::EscapeDangerGoal;
use crate::entity::ai::goal::goal_selector::GoalSelector;
use crate::entity::ai::goal::wander_around::WanderAroundGoal;
use crate::entity::ai::pathfinder::Navigator;
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::mob::{Mob, MobEntity};

/// The `AbstractFish` and `WaterAnimal` constructors: fish move control, water navigation, and
/// no pathfinding penalty for water.
pub fn init(mob_entity: &MobEntity) {
    *mob_entity
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(FishMoveControl::default());
    let mut navigator = Navigator::water_bound(false);
    navigator.set_pathfinding_malus(PathType::Water, 0.0);
    *mob_entity
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = navigator;
}

/// Vanilla `AbstractFish.registerGoals`.
pub fn register_goals(goal_selector: &mut GoalSelector) {
    goal_selector.add_goal(0, EscapeDangerGoal::new(1.25));
    goal_selector.add_goal(
        2,
        Box::new(AvoidEntityGoal::new(&EntityType::PLAYER, 8.0, 1.6, 1.4)),
    );
    // `FishSwimGoal`
    goal_selector.add_goal(4, Box::new(WanderAroundGoal::swimming(1.0, 40)));
}

/// Vanilla `AbstractFish.travelInWater`: no buoyancy, and a slow sink while not chasing anything.
pub fn travel_in_water(mob: &dyn Mob, caller: &dyn EntityBase) -> bool {
    let mob_entity = mob.get_mob_entity();
    let living_entity = &mob_entity.living_entity;
    let entity = &living_entity.entity;
    entity.update_velocity_from_input(living_entity.movement_input.load(), 0.01);
    living_entity.make_move(caller);
    let mut velocity = entity.velocity.load() * 0.9;
    if mob_entity.get_target().is_none() {
        velocity.y -= 0.005;
    }
    entity.velocity.store(velocity);
    true
}

/// The flop at the start of vanilla `AbstractFish.aiStep`, for a fish stranded on land.
pub fn flop_on_land(mob: &dyn Mob, flop_sound: Sound) {
    let entity = &mob.get_mob_entity().living_entity.entity;
    if entity.is_in_water()
        || !entity.on_ground.load(Ordering::Relaxed)
        || !entity.vertical_collision.load(Ordering::Relaxed)
    {
        return;
    }
    let mut rng = mob.get_random();
    let x = (rng.random::<f32>() * 2.0 - 1.0) * 0.05;
    let z = (rng.random::<f32>() * 2.0 - 1.0) * 0.05;
    entity.velocity.store(
        entity
            .velocity
            .load()
            .add_raw(f64::from(x), 0.4, f64::from(z)),
    );
    entity.on_ground.store(false, Ordering::Relaxed);
    entity.velocity_dirty.store(true, Ordering::Relaxed);
    entity.play_sound(flop_sound);
}
