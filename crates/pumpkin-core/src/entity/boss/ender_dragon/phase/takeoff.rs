use super::{EnderDragonPhase, next_path_target, wrap_node_index};
use crate::entity::EntityBase;
use crate::entity::boss::ender_dragon::{DragonPath, EnderDragonEntity};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use std::sync::{Mutex, PoisonError};

#[derive(Default)]
struct State {
    first_tick: bool,
    current_path: Option<DragonPath>,
    target_location: Option<Vector3<f64>>,
}

/// Vanilla `DragonTakeoffPhase`: leave the portal the way the head points, then circle again.
#[derive(Default)]
pub struct TakeoffPhase {
    state: Mutex<State>,
}

impl TakeoffPhase {
    fn find_new_target(dragon: &EnderDragonEntity, state: &mut State) {
        let current_node = dragon.find_closest_node();
        let look = dragon.head_look_vector();
        let target_node =
            dragon.find_closest_node_to(Vector3::new(-look.x * 40.0, 105.0, -look.z * 40.0));
        let outer_ring = dragon
            .fight_alive_crystals()
            .is_some_and(|crystals| crystals > 0);
        let target_node = wrap_node_index(target_node as i32, outer_ring);

        state.current_path = dragon.find_path(current_node, target_node, None);
        if let Some(path) = state.current_path.as_mut() {
            path.advance();
            if let Some(target) = next_path_target(path) {
                state.target_location = Some(target);
            }
        }
    }
}

impl super::Phase for TakeoffPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.first_tick && state.current_path.is_some() {
            let egg = dragon.podium_pos(ChunkHeightmapType::MotionBlockingNoLeaves);
            let dist_sq = egg
                .to_centered_f64()
                .squared_distance_to_vec(&dragon.get_entity().pos.load());
            if dist_sq >= 10.0 * 10.0 {
                drop(state);
                dragon.set_phase(EnderDragonPhase::HoldingPattern);
            }
        } else {
            state.first_tick = false;
            Self::find_new_target(dragon, &mut state);
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.first_tick = true;
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
