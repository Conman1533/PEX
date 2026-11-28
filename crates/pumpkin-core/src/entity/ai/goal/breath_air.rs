use pumpkin_data::Block;
use pumpkin_data::fluid::Fluid;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::{Controls, Goal};
use crate::entity::{EntityBase, ai::pathfinder::node::PathComputationType, mob::Mob};
use crate::world::World;

/// Air below which vanilla `BreathAirGoal` heads for the surface.
const LOW_AIR: i32 = 140;
/// How far up `BreathAirGoal` looks for air.
const SEARCH_HEIGHT: i32 = 8;

/// Vanilla `BreathAirGoal`: a mob that is running out of air swims up to the nearest spot with
/// air above it, and can't be interrupted until it has its breath back.
pub struct BreathAirGoal;

impl Default for BreathAirGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl BreathAirGoal {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn low_on_air(mob: &dyn Mob) -> bool {
        mob.get_entity()
            .air_supply
            .load(std::sync::atomic::Ordering::Relaxed)
            < LOW_AIR
    }

    /// Vanilla `givesAir`: no fluid (or a bubble column), and a mob could walk there.
    fn gives_air(world: &World, pos: &BlockPos) -> bool {
        let (block, state) = world.get_block_and_state(pos);
        let (fluid, _) = world.get_fluid_and_fluid_state(pos);
        (fluid == &Fluid::EMPTY || block == &Block::BUBBLE_COLUMN)
            && world
                .block_registry
                .is_pathfindable(block, state, PathComputationType::Land)
    }

    /// Vanilla `findAirPosition`: the first spot that gives air in `BlockPos.neighborColumn`
    /// (the mob's own column, then north, east, south and west, each from its feet up eight
    /// blocks), or eight blocks straight up. The mob swims to one block above it.
    fn find_air_position(mob: &dyn Mob) {
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        let origin = entity.block_pos.load();
        let steps = [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)];
        let destination = steps
            .into_iter()
            .flat_map(|(dx, dz)| {
                (0..=SEARCH_HEIGHT)
                    .map(move |dy| BlockPos::new(origin.0.x + dx, origin.0.y + dy, origin.0.z + dz))
            })
            .find(|candidate| Self::gives_air(&world, candidate))
            .unwrap_or_else(|| BlockPos::floored(pos.x, pos.y + f64::from(SEARCH_HEIGHT), pos.z));
        mob.navigate_to(
            pos,
            Vector3::new(
                f64::from(destination.0.x),
                f64::from(destination.0.y + 1),
                f64::from(destination.0.z),
            ),
            1.0,
        );
    }
}

impl Goal for BreathAirGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::low_on_air(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        Self::low_on_air(mob)
    }

    fn can_stop(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.stop_navigation();
        Self::find_air_position(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        Self::find_air_position(mob);
        // Vanilla `moveRelative(0.02F, (xxa, yya, zza))` and `move(SELF, deltaMovement)`: an extra
        // push along the mob's movement input on top of its own travel.
        let living = &mob.get_mob_entity().living_entity;
        living
            .entity
            .update_velocity_from_input(living.movement_input.load(), 0.02);
        let caller: &dyn EntityBase = mob;
        living.make_move(caller);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
