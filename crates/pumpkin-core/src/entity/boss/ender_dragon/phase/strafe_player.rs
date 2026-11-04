use super::{EnderDragonPhase, distance_to_target, next_path_target, wrap_node_index};
use crate::entity::EntityBase;
use crate::entity::boss::ender_dragon::{DragonNode, DragonPath, EnderDragonEntity};
use crate::entity::projectile::dragon_fireball::DragonFireballEntity;
use crate::entity::{Entity, boss::ender_dragon::HEAD_SIZE};
use pumpkin_data::entity::EntityType;
use pumpkin_data::world::WorldEvent;
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use tracing::warn;

const FIREBALL_CHARGE_AMOUNT: i32 = 5;

#[derive(Default)]
struct State {
    fireball_charge: i32,
    current_path: Option<DragonPath>,
    target_location: Option<Vector3<f64>>,
    attack_target: Option<Weak<dyn EntityBase>>,
    holding_pattern_clockwise: bool,
}

/// Vanilla `DragonStrafePlayerPhase`: fly past a player and shoot a dragon fireball at it.
#[derive(Default)]
pub struct StrafePlayerPhase {
    state: Mutex<State>,
}

/// Vanilla `heightOffset`: how far above the target the dragon aims, by horizontal distance.
fn height_offset(horizontal_dist: f64) -> f64 {
    (f64::from(0.4f32) + horizontal_dist / 80.0 - 1.0).min(10.0)
}

impl StrafePlayerPhase {
    pub fn set_target(&self, dragon: &EnderDragonEntity, target: &Arc<dyn EntityBase>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.attack_target = Some(Arc::downgrade(target));
        let target_pos = target.get_entity().pos.load();
        let current_node = dragon.find_closest_node();
        let target_node = dragon.find_closest_node_to(target_pos);
        let final_x = target_pos.x.floor() as i32;
        let final_z = target_pos.z.floor() as i32;
        let pos = dragon.get_entity().pos.load();
        let xd = f64::from(final_x) - pos.x;
        let zd = f64::from(final_z) - pos.z;
        let final_y = (target_pos.y + height_offset(xd.hypot(zd))).floor() as i32;
        state.current_path = dragon.find_path(
            current_node,
            target_node,
            Some(DragonNode::new(final_x, final_y, final_z)),
        );
        if let Some(path) = state.current_path.as_mut() {
            path.advance();
            if let Some(target) = next_path_target(path) {
                state.target_location = Some(target);
            }
        }
    }

    fn find_new_target(dragon: &EnderDragonEntity, state: &mut State) {
        if state.current_path.as_ref().is_none_or(DragonPath::is_done) {
            let current_node = dragon.find_closest_node();
            let mut target_node = current_node as i32;
            if rand::random_range(0..8) == 0 {
                state.holding_pattern_clockwise = !state.holding_pattern_clockwise;
                target_node += 6;
            }
            if state.holding_pattern_clockwise {
                target_node += 1;
            } else {
                target_node -= 1;
            }

            let outer_ring = dragon
                .fight_alive_crystals()
                .is_some_and(|crystals| crystals > 0);
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

    fn shoot_fireball(dragon: &EnderDragonEntity, target: &Entity) {
        let entity = dragon.get_entity();
        let world = entity.world.load_full();
        let view = dragon.get_looking_vector();
        let head = dragon.parts[0].entity.pos.load();
        let start = Vector3::new(
            head.x - view.x,
            head.y + HEAD_SIZE * 0.5 + 0.5,
            head.z - view.z,
        );
        let target_pos = target.pos.load();
        let aim_y = target_pos.y + f64::from(target.entity_dimension.load().height) * 0.5;
        let direction = Vector3::new(target_pos.x, aim_y, target_pos.z) - start;

        if !entity.is_silent() {
            world.sync_world_event(
                WorldEvent::SoundDragonFireball,
                entity.pos.load().to_block_pos(),
                0,
            );
        }

        let fireball_entity = Entity::new(world.clone(), start, &EntityType::DRAGON_FIREBALL);
        let fireball = DragonFireballEntity::new_shot(fireball_entity, entity, direction);
        fireball.get_entity().set_pos(start);
        world.spawn_entity(Arc::new(fireball));
    }
}

impl super::Phase for StrafePlayerPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(target) = state.attack_target.as_ref().and_then(Weak::upgrade) else {
            drop(state);
            warn!("Skipping player strafe phase because no player was found");
            dragon.set_phase(EnderDragonPhase::HoldingPattern);
            return;
        };
        let target = target.get_entity();
        let target_pos = target.pos.load();
        let pos = dragon.get_entity().pos.load();

        if state.current_path.as_ref().is_some_and(DragonPath::is_done) {
            let dist = (target_pos.x - pos.x).hypot(target_pos.z - pos.z);
            state.target_location = Some(Vector3::new(
                target_pos.x,
                target_pos.y + height_offset(dist),
                target_pos.z,
            ));
        }

        let dist_to_target = distance_to_target(dragon, state.target_location);
        if !(100.0..=22500.0).contains(&dist_to_target) {
            Self::find_new_target(dragon, &mut state);
        }

        if target_pos.squared_distance_to_vec(&pos) < 4096.0 {
            if dragon.get_entity().has_line_of_sight(target) {
                state.fireball_charge += 1;
                let aim = Vector3::new(target_pos.x - pos.x, 0.0, target_pos.z - pos.z).normalize();
                let yaw = dragon.get_entity().yaw.load().to_radians();
                let dir = Vector3::new(
                    f64::from(pumpkin_util::math::sin(yaw)),
                    0.0,
                    f64::from(-pumpkin_util::math::cos(yaw)),
                )
                .normalize();
                let dot = dir.dot(&aim) as f32;
                let angle_degs = f64::from(dot).acos().to_degrees() as f32 + 0.5;
                if state.fireball_charge >= FIREBALL_CHARGE_AMOUNT
                    && (0.0..10.0).contains(&angle_degs)
                {
                    Self::shoot_fireball(dragon, target);
                    state.fireball_charge = 0;
                    if let Some(path) = state.current_path.as_mut() {
                        path.finish();
                    }
                    drop(state);
                    dragon.set_phase(EnderDragonPhase::HoldingPattern);
                }
            } else if state.fireball_charge > 0 {
                state.fireball_charge -= 1;
            }
        } else if state.fireball_charge > 0 {
            state.fireball_charge -= 1;
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.fireball_charge = 0;
        state.target_location = None;
        state.current_path = None;
        state.attack_target = None;
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .target_location
    }
}
