use super::{
    EnderDragonPhase, distance_to_target, needs_new_target, next_path_target, wrap_node_index,
};
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::boss::ender_dragon::{DragonPath, EnderDragonEntity};
use crate::entity::mob::Mob;
use crate::entity::player::Player;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Default)]
struct State {
    current_path: Option<DragonPath>,
    target_location: Option<Vector3<f64>>,
    clockwise: bool,
}

/// Vanilla `DragonHoldingPatternPhase`: circle the pillars, then land or strafe a player.
#[derive(Default)]
pub struct HoldingPatternPhase {
    state: Mutex<State>,
}

impl HoldingPatternPhase {
    fn find_new_target(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.current_path.as_ref().is_some_and(DragonPath::is_done) {
            let egg = dragon.podium_pos(ChunkHeightmapType::MotionBlockingNoLeaves);
            let crystals = dragon.fight_alive_crystals().unwrap_or(0);
            if rand::random_range(0..crystals + 3) == 0 {
                drop(state);
                dragon.set_phase(EnderDragonPhase::LandingApproach);
                return;
            }

            // Vanilla `NEW_TARGET_TARGETING`.
            let targeting = TargetPredicate::create_attackable().ignore_visibility();
            let player = dragon.nearest_player(&targeting, egg.to_f64());
            let dist_sqr = player.as_ref().map_or(64.0, |player| {
                egg.to_centered_f64()
                    .squared_distance_to_vec(&player.get_entity().pos.load())
                    / 512.0
            });
            if let Some(player) = player
                && (rand::random_range(0..(dist_sqr + 2.0) as i32) == 0
                    || rand::random_range(0..crystals + 2) == 0)
            {
                drop(state);
                strafe_player(dragon, player);
                return;
            }
        }

        if state.current_path.as_ref().is_none_or(DragonPath::is_done) {
            let current_node = dragon.find_closest_node();
            let mut target_node = current_node as i32;
            if rand::random_range(0..8) == 0 {
                state.clockwise = !state.clockwise;
                target_node += 6;
            }
            if state.clockwise {
                target_node += 1;
            } else {
                target_node -= 1;
            }

            // Vanilla tests `aliveCrystals() >= 0` here, so with a fight it always keeps to the
            // outer ring.
            let outer_ring = dragon.fight_alive_crystals().is_some();
            let target_node = wrap_node_index(target_node, outer_ring);
            state.current_path = dragon.find_path(current_node, target_node, None);
            if let Some(path) = state.current_path.as_mut() {
                path.advance();
            }
        }

        if let Some(path) = state.current_path.as_mut()
            && let Some(target) = next_path_target(path)
        {
            state.target_location = Some(target);
        }
    }
}

fn strafe_player(dragon: &EnderDragonEntity, player: Arc<Player>) {
    dragon.set_phase(EnderDragonPhase::StrafePlayer);
    dragon
        .phase_manager
        .strafe_player
        .set_target(dragon, &(player as Arc<dyn EntityBase>));
}

impl super::Phase for HoldingPatternPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let target = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .target_location;
        if needs_new_target(dragon, distance_to_target(dragon, target)) {
            self.find_new_target(dragon);
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.current_path = None;
        state.target_location = None;
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .target_location
    }

    fn on_crystal_destroyed(&self, dragon: &EnderDragonEntity, player: Option<Arc<Player>>) {
        if let Some(player) = player
            && dragon.can_attack(player.as_ref())
        {
            strafe_player(dragon, player);
        }
    }
}
