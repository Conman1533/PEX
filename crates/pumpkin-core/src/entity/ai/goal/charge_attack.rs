use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::math::vector3::Vector3;

use super::{Controls, Goal};
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::combat::handle_knockback;
use crate::entity::mob::Mob;

/// Vanilla `ChargeAttack`, the nautilus dash: a straight run at the attack target that hits
/// the first living entity it touches, then waits `time_between_attacks` ticks.
pub struct ChargeAttackGoal {
    time_between_attacks: i64,
    speed: f64,
    knockback_force: f64,
    max_charge_distance: f64,
    max_target_detection_distance: f64,
    charge_sound: Sound,
    cooldown_until: i64,
    start_position: Vector3<f64>,
    charge_velocity: Vector3<f64>,
}

impl ChargeAttackGoal {
    #[must_use]
    pub const fn new(
        time_between_attacks: i64,
        speed: f64,
        knockback_force: f64,
        max_charge_distance: f64,
        max_target_detection_distance: f64,
        charge_sound: Sound,
    ) -> Self {
        Self {
            time_between_attacks,
            speed,
            knockback_force,
            max_charge_distance,
            max_target_detection_distance,
            charge_sound,
            cooldown_until: 0,
            start_position: Vector3::new(0.0, 0.0, 0.0),
            charge_velocity: Vector3::new(0.0, 0.0, 0.0),
        }
    }

    fn on_cooldown(&self, mob: &dyn Mob) -> bool {
        mob.get_entity().world.load().get_world_age() < self.cooldown_until
    }

    /// Vanilla `ChargeAttack.canStillUse`.
    fn can_still_use(&self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        let pos = mob.get_entity().pos.load();
        let target_entity = target.get_entity();
        !mob.is_tamed()
            && target_entity.is_alive()
            && pos.squared_distance_to_vec(&self.start_position)
                < self.max_charge_distance * self.max_charge_distance
            && target_entity.pos.load().squared_distance_to_vec(&pos)
                < self.max_target_detection_distance * self.max_target_detection_distance
            && mob.get_entity().has_line_of_sight(target_entity)
            && !self.on_cooldown(mob)
    }

    fn effect_level(mob: &dyn Mob, effect: &'static StatusEffect) -> i32 {
        mob.get_mob_entity()
            .living_entity
            .get_effect(effect)
            .map_or(0, |e| i32::from(e.amplifier) + 1)
    }
}

impl Goal for ChargeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !self.on_cooldown(mob) && mob.get_mob_entity().get_target().is_some()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_still_use(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        self.start_position = entity.pos.load();
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let direction = (target.get_entity().pos.load() - self.start_position).normalize();
        self.charge_velocity = direction * self.speed;
        mob.get_mob_entity().steered().navigator().stop();
        if self.can_still_use(mob) {
            entity.world.load().play_sound(
                self.charge_sound,
                SoundCategory::Neutral,
                &self.start_position,
            );
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let entity = mob.get_entity();
        entity.look_at(target.get_entity().pos.load());
        entity.velocity.store(self.charge_velocity);

        let world = entity.world.load();
        let caller: &dyn EntityBase = mob;
        let targeting = TargetPredicate::create_attackable().ignore_visibility();
        let hit = world
            .get_entities_at_box(&entity.bounding_box.load())
            .into_iter()
            .find(|other| targeting.test(&world, Some(caller), other.as_ref()));
        let Some(hit) = hit else {
            return;
        };
        if entity.has_passenger(hit.get_entity().entity_id) {
            return;
        }

        let living = &mob.get_mob_entity().living_entity;
        let damage = living.get_attribute_value(&Attributes::ATTACK_DAMAGE) as f32;
        hit.damage_with_context(
            hit.as_ref(),
            damage,
            DamageType::MOB_ATTACK,
            None,
            Some(caller),
            Some(caller),
        );

        // Vanilla `dealKnockBack`, then `causeExtraKnockback` (`handle_knockback` halves).
        let speed_boost = 0.25
            * f64::from(
                Self::effect_level(mob, &StatusEffect::SPEED)
                    - Self::effect_level(mob, &StatusEffect::SLOWNESS),
            );
        let speed_factor = (self.speed * living.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            .clamp(0.2, 2.0)
            + speed_boost;
        handle_knockback(
            entity,
            hit.as_ref(),
            speed_factor * self.knockback_force * 2.0,
        );

        self.stop(mob);
        self.cooldown_until = world.get_world_age() + self.time_between_attacks;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.cooldown_until =
            mob.get_entity().world.load().get_world_age() + self.time_between_attacks;
        mob.get_mob_entity().set_target(None);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
