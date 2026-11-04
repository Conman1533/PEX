use super::EnderDragonPhase;
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::boss::ender_dragon::EnderDragonEntity;
use pumpkin_util::math::{cos, sin, vector3::Vector3, wrap_degrees_f64};
use std::sync::{Mutex, PoisonError};

const SITTING_SCANNING_IDLE_TICKS: i32 = 100;
const SITTING_ATTACK_Y_VIEW_RANGE: f64 = 10.0;
const SITTING_ATTACK_VIEW_RANGE: f64 = 20.0;
const SITTING_CHARGE_VIEW_RANGE: f64 = 150.0;
/// Ticks a player must stay in view before the dragon roars at it.
const SITTING_SCAN_TICKS_BEFORE_ATTACK: i32 = 25;

/// Vanilla `DragonSittingScanningPhase`: perched, turn toward a nearby player, roar at it, or take
/// off and charge a far one.
#[derive(Default)]
pub struct SittingScanningPhase {
    scanning_time: Mutex<i32>,
}

impl SittingScanningPhase {
    /// Turns the perched dragon toward `target_pos`, like the `angle > 10` branch of vanilla
    /// `doServerTick`.
    fn turn_toward(dragon: &EnderDragonEntity, target_pos: Vector3<f64>) {
        let entity = dragon.get_entity();
        let pos = entity.pos.load();
        let yaw = entity.yaw.load();
        let aim = Vector3::new(target_pos.x - pos.x, 0.0, target_pos.z - pos.z).normalize();
        let yaw_rad = yaw.to_radians();
        let dir = Vector3::new(f64::from(sin(yaw_rad)), 0.0, f64::from(-cos(yaw_rad))).normalize();
        let dot = dir.dot(&aim) as f32;
        let angle = f64::from(dot).acos().to_degrees() as f32 + 0.5;
        if (0.0..=10.0).contains(&angle) {
            return;
        }

        let head = dragon.parts[0].entity.pos.load();
        let x_attack_dist = target_pos.x - head.x;
        let z_attack_dist = target_pos.z - head.z;
        let y_rot_delta = wrap_degrees_f64(
            180.0 - x_attack_dist.atan2(z_attack_dist).to_degrees() - f64::from(yaw),
        )
        .clamp(-100.0, 100.0);
        let mut dist = x_attack_dist.hypot(z_attack_dist) as f32 + 1.0;
        let rot_speed = dist;
        dist = dist.min(40.0);

        let mut y_rot_a = dragon
            .yaw_rot_accel
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *y_rot_a *= 0.8;
        *y_rot_a += y_rot_delta as f32 * (0.7 / dist / rot_speed);
        entity.yaw.store(yaw + *y_rot_a);
    }
}

impl super::Phase for SittingScanningPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let scanning_time = {
            let mut scanning_time = self
                .scanning_time
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *scanning_time += 1;
            *scanning_time
        };

        let pos = dragon.get_entity().pos.load();
        let scan_targeting =
            TargetPredicate::create_attackable().set_base_max_distance(SITTING_ATTACK_VIEW_RANGE);
        let attack_target = dragon.nearest_player_matching(&scan_targeting, pos, |player| {
            (player.get_entity().pos.load().y - pos.y).abs() <= SITTING_ATTACK_Y_VIEW_RANGE
        });

        if let Some(attack_target) = attack_target {
            if scanning_time > SITTING_SCAN_TICKS_BEFORE_ATTACK {
                dragon.set_phase(EnderDragonPhase::SittingAttacking);
            } else {
                Self::turn_toward(dragon, attack_target.get_entity().pos.load());
            }
        } else if scanning_time >= SITTING_SCANNING_IDLE_TICKS {
            let charge_targeting = TargetPredicate::create_attackable()
                .set_base_max_distance(SITTING_CHARGE_VIEW_RANGE);
            let attack_target = dragon.nearest_player(&charge_targeting, pos);
            dragon.set_phase(EnderDragonPhase::Takeoff);
            if let Some(attack_target) = attack_target {
                dragon.set_phase(EnderDragonPhase::ChargingPlayer);
                dragon
                    .phase_manager
                    .charging_player
                    .set_target(attack_target.get_entity().pos.load());
            }
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        *self
            .scanning_time
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = 0;
    }
}
