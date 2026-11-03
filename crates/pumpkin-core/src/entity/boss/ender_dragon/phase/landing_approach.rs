use super::{EnderDragonPhase, distance_to_target, needs_new_target, next_path_target};
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::boss::ender_dragon::{DragonNode, DragonPath, EnderDragonEntity};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use std::sync::{Mutex, PoisonError};

#[derive(Default)]
struct State {
    current_path: Option<DragonPath>,
    target_location: Option<Vector3<f64>>,
}

/// Vanilla `DragonLandingApproachPhase`: path to the portal from the side away from the player
/// nearest to it.
#[derive(Default)]
pub struct LandingApproachPhase {
    state: Mutex<State>,
}

impl LandingApproachPhase {
    fn find_new_target(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.current_path.as_ref().is_none_or(DragonPath::is_done) {
            let current_node = dragon.find_closest_node();
            let egg = dragon.podium_pos(ChunkHeightmapType::MotionBlockingNoLeaves);
            // Vanilla `NEAR_EGG_TARGETING`.
            let targeting = TargetPredicate::create_attackable().ignore_visibility();
            let target_node = dragon.nearest_player(&targeting, egg.to_f64()).map_or_else(
                || dragon.find_closest_node_to(Vector3::new(40.0, f64::from(egg.0.y), 0.0)),
                |player| {
                    // The player's position seen from the world origin, not from the egg.
                    let player_pos = player.get_entity().pos.load();
                    let aim = Vector3::new(player_pos.x, 0.0, player_pos.z).normalize();
                    dragon.find_closest_node_to(Vector3::new(-aim.x * 40.0, 105.0, -aim.z * 40.0))
                },
            );

            let final_node = DragonNode::new(egg.0.x, egg.0.y, egg.0.z);
            state.current_path = dragon.find_path(current_node, target_node, Some(final_node));
            if let Some(path) = state.current_path.as_mut() {
                path.advance();
            }
        }

        if let Some(path) = state.current_path.as_mut()
            && let Some(target) = next_path_target(path)
        {
            state.target_location = Some(target);
        }
        if state.current_path.as_ref().is_some_and(DragonPath::is_done) {
            drop(state);
            dragon.set_phase(EnderDragonPhase::Landing);
        }
    }
}

impl super::Phase for LandingApproachPhase {
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
}
