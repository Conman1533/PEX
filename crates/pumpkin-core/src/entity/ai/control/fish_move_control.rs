use crate::entity::ai::control::move_control::Operation;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;
use pumpkin_data::attributes::Attributes;
use pumpkin_util::math::vector3::Vector3;

/// Vanilla `AbstractFish.FishMoveControl`.
#[derive(Default)]
pub struct FishMoveControl {
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
    operation: Operation,
    /// Vanilla keeps this on the mob (`Mob.getSpeed`); only this control sets it for fish.
    speed: f32,
}

impl Control for FishMoveControl {}

impl MoveControlTrait for FishMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let living_entity = &mob_entity.living_entity;
        let entity = &living_entity.entity;

        if entity.is_submerged_in_water() {
            entity
                .velocity
                .store(entity.velocity.load().add_raw(0.0, 0.005, 0.0));
        }

        let navigation_done = mob_entity
            .navigator
            .try_lock()
            .is_ok_and(|navigator| navigator.is_done());
        if self.operation == Operation::MoveTo && !navigation_done {
            let target_speed = (self.speed_modifier
                * living_entity.get_attribute_value(&Attributes::MOVEMENT_SPEED))
                as f32;
            self.speed += (target_speed - self.speed) * 0.125;
            living_entity
                .movement_input
                .store(Vector3::new(0.0, 0.0, f64::from(self.speed)));

            let pos = entity.pos.load();
            let xd = self.wanted_x - pos.x;
            let yd = self.wanted_y - pos.y;
            let zd = self.wanted_z - pos.z;
            if yd != 0.0 {
                let dd = (xd * xd + yd * yd + zd * zd).sqrt();
                entity.velocity.store(entity.velocity.load().add_raw(
                    0.0,
                    f64::from(self.speed) * (yd / dd) * 0.1,
                    0.0,
                ));
            }

            if xd != 0.0 || zd != 0.0 {
                let y_rot_d = (zd.atan2(xd).to_degrees() as f32) - 90.0;
                let yaw = self.change_angle(entity.yaw.load(), y_rot_d, 90.0);
                entity.yaw.store(yaw);
                entity.body_yaw.store(yaw);
            }
        } else {
            self.speed = 0.0;
            living_entity.movement_input.store(Vector3::default());
        }
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.wanted_x = x;
        self.wanted_y = y;
        self.wanted_z = z;
        self.speed_modifier = speed_modifier;
        if self.operation != Operation::Jumping {
            self.operation = Operation::MoveTo;
        }
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }
}
