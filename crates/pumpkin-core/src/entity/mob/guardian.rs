use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::tracked_data::guardian::DATA_ID_ATTACK_TARGET;
use pumpkin_data::{damage::DamageType, sound::Sound};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::Difficulty;

use crate::entity::custom_sound::CustomSound;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};

/// Vanilla `Guardian.getAttackDuration`.
const ATTACK_TIME: i32 = 80;
/// Vanilla `ElderGuardian.getAttackDuration`.
pub const ELDER_ATTACK_TIME: i32 = 60;

/// Vanilla `Guardian.GuardianAttackGoal`: lock the beam on for the attack duration, then
/// magic damage plus a melee hit. Also used by the elder guardian.
pub struct GuardianAttackGoal {
    attack_time: i32,
    elder: bool,
}

impl GuardianAttackGoal {
    #[must_use]
    pub const fn new(elder: bool) -> Self {
        Self {
            attack_time: 0,
            elder,
        }
    }

    const fn attack_duration(&self) -> i32 {
        if self.elder {
            ELDER_ATTACK_TIME
        } else {
            ATTACK_TIME
        }
    }

    /// Vanilla `Guardian.setActiveAttackTarget`.
    fn set_active_attack_target(mob: &dyn Mob, entity_id: i32) {
        mob.get_entity()
            .set_synced_data(DATA_ID_ATTACK_TARGET, VarInt(entity_id));
    }
}

impl Goal for GuardianAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|t| t.get_entity().is_alive())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        target.get_entity().is_alive()
            && (self.elder
                || mob
                    .get_entity()
                    .pos
                    .load()
                    .squared_distance_to_vec(&target.get_entity().pos.load())
                    > 9.0)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.attack_time = -10;
        let mob_entity = mob.get_mob_entity();
        mob_entity.steered().navigator().stop();
        if let Some(target) = mob_entity.get_target() {
            mob_entity
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity_with_range(&target, 90.0, 90.0);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        Self::set_active_attack_target(mob, 0);
        mob.get_mob_entity().set_target(None);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };
        mob_entity.steered().navigator().stop();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 90.0, 90.0);
        if !mob.has_line_of_sight(target.get_entity()) {
            mob_entity.set_target(None);
            return;
        }
        self.attack_time += 1;
        let entity = mob.get_entity();
        if self.attack_time == 0 {
            Self::set_active_attack_target(mob, target.get_entity().entity_id);
            if !entity.is_silent() {
                entity.world.load().send_entity_status(
                    entity,
                    EntityStatus::GuardianAttackSound,
                    ActorEventID::GuardianAttackSound,
                );
            }
        } else if self.attack_time >= self.attack_duration() {
            let mut magic_damage = 1.0;
            if entity.world.load().get_difficulty() == Difficulty::Hard {
                magic_damage += 2.0;
            }
            if self.elder {
                magic_damage += 2.0;
            }
            let caller: &dyn EntityBase = mob;
            target.damage_with_context(
                target.as_ref(),
                magic_damage,
                DamageType::INDIRECT_MAGIC,
                None,
                Some(caller),
                Some(caller),
            );
            if mob_entity.try_attack(caller, target.as_ref()) {
                mob.on_attack(target.as_ref());
            }
            mob_entity.set_target(None);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

pub struct GuardianEntity {
    pub mob_entity: MobEntity,
}

impl GuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let guardian = Self { mob_entity };
        let mob_arc = Arc::new(guardian);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(4, Box::new(GuardianAttackGoal::new(false)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                5,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::SQUID, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::GLOW_SQUID, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::AXOLOTL, true),
            );
        };

        mob_arc
    }
}

impl CustomSound for GuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        Some(if is_water {
            Sound::EntityGuardianHurt
        } else {
            Sound::EntityGuardianHurtLand
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        Some(if is_water {
            Sound::EntityGuardianDeath
        } else {
            Sound::EntityGuardianDeathLand
        })
    }
}

impl Mob for GuardianEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(
            if self.get_mob_entity().living_entity.entity.is_in_water() {
                pumpkin_data::sound::Sound::EntityGuardianAmbient
            } else {
                pumpkin_data::sound::Sound::EntityGuardianAmbientLand
            },
        )
    }

    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::Events
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if let Some(src) = source
            && let Some(living) = src.get_living_entity()
        {
            let _ = living.damage(src, 2.0, DamageType::THORNS);
        }
    }
}
