use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data::guardian::{DATA_ID_ATTACK_TARGET, DATA_ID_MOVING};
use pumpkin_data::{damage::DamageType, sound::Sound};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::Difficulty;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ai::control::move_control::{MoveControl, Operation};
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::ai::goal::move_towards_restriction::MoveTowardsRestrictionGoal;
use crate::entity::ai::pathfinder::Navigator;
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::custom_sound::CustomSound;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};

/// Vanilla `Guardian.getAttackDuration`.
const ATTACK_TIME: i32 = 80;
/// Vanilla `ElderGuardian.getAttackDuration`.
pub const ELDER_ATTACK_TIME: i32 = 60;
/// Vanilla `Guardian.registerGoals`: `new RandomStrollGoal(this, 1.0, 80)`.
const STROLL_INTERVAL: i32 = 80;
/// Vanilla `ElderGuardian`: `randomStrollGoal.setInterval(400)`.
const ELDER_STROLL_INTERVAL: i32 = 400;

/// State the guardian's goals, move control and hooks share: vanilla keeps it on the
/// `Guardian` itself (`DATA_ID_MOVING`, `DATA_ID_ATTACK_TARGET`, `randomStrollGoal`).
#[derive(Default)]
pub struct GuardianState {
    moving: AtomicBool,
    active_attack_target: AtomicI32,
    /// Vanilla `randomStrollGoal.trigger()`, read by the stroll goal on its next check.
    stroll_trigger: Arc<AtomicBool>,
}

impl GuardianState {
    /// Vanilla `Guardian.isMoving`.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.moving.load(Ordering::Relaxed)
    }

    /// Vanilla `Guardian.setMoving`.
    fn set_moving(&self, entity: &Entity, moving: bool) {
        if self.moving.swap(moving, Ordering::Relaxed) != moving {
            entity.set_synced_data(DATA_ID_MOVING, moving);
        }
    }

    /// Vanilla `Guardian.hasActiveAttackTarget`.
    #[must_use]
    pub fn has_active_attack_target(&self) -> bool {
        self.active_attack_target.load(Ordering::Relaxed) != 0
    }

    /// Vanilla `Guardian.setActiveAttackTarget`.
    fn set_active_attack_target(&self, entity: &Entity, entity_id: i32) {
        self.active_attack_target
            .store(entity_id, Ordering::Relaxed);
        entity.set_synced_data(DATA_ID_ATTACK_TARGET, VarInt(entity_id));
    }

    fn trigger_stroll(&self) {
        self.stroll_trigger.store(true, Ordering::Relaxed);
    }

    /// The guardian side of vanilla's constructor: `WaterBoundPathNavigation`, no water malus
    /// and `GuardianMoveControl`.
    pub fn init_controls(self: &Arc<Self>, mob_entity: &mut MobEntity) {
        let mut navigator = Navigator::water_bound(false);
        navigator.set_pathfinding_malus(PathType::Water, 0.0);
        *mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = navigator;
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(GuardianMoveControl {
            inner: MoveControl::default(),
            state: self.clone(),
        });
    }

    /// Vanilla `Guardian.registerGoals`, with `ElderGuardian`'s stroll interval for `elder`.
    pub fn register_goals(
        self: &Arc<Self>,
        mob_entity: &MobEntity,
        mob_weak: &Weak<dyn Mob>,
        elder: bool,
    ) {
        let move_and_look = Controls::MOVE | Controls::LOOK;
        let goal_selector = &mob_entity.goals_selector;
        goal_selector.add_goal(4, Box::new(GuardianAttackGoal::new(elder, self.clone())));
        goal_selector.add_goal(
            5,
            Box::new(MoveTowardsRestrictionGoal::new(1.0).with_controls(move_and_look)),
        );
        let interval = if elder {
            ELDER_STROLL_INTERVAL
        } else {
            STROLL_INTERVAL
        };
        goal_selector.add_goal(
            7,
            Box::new(
                WanderAroundGoal::with_interval(1.0, interval)
                    .with_controls(move_and_look)
                    .with_shared_trigger(self.stroll_trigger.clone()),
            ),
        );
        goal_selector.add_goal(
            8,
            LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
        );
        goal_selector.add_goal(
            8,
            Box::new(LookAtEntityGoal::new(
                mob_weak.clone(),
                &EntityType::GUARDIAN,
                12.0,
                0.01,
                false,
            )),
        );
        goal_selector.add_goal(9, Box::new(RandomLookAroundGoal::default()));

        // Vanilla `GuardianAttackSelector`: players, squid (glow squid too) and axolotls more
        // than three blocks away.
        let guardian = mob_weak.clone();
        mob_entity.target_selector.add_goal(
            1,
            ActiveTargetGoal::predicated(mob_entity, 10, true, move |target, _world| {
                let entity_type = target.entity.entity_type;
                (entity_type == &EntityType::PLAYER
                    || entity_type == &EntityType::SQUID
                    || entity_type == &EntityType::GLOW_SQUID
                    || entity_type == &EntityType::AXOLOTL)
                    && guardian.upgrade().is_some_and(|guardian| {
                        target
                            .entity
                            .pos
                            .load()
                            .squared_distance_to_vec(&guardian.get_entity().pos.load())
                            > 9.0
                    })
            }),
        );
    }

    /// The server part of vanilla `Guardian.aiStep`: breath in water, flop on land, and face
    /// the beam's target.
    pub fn ai_step(&self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        if !entity.is_alive() {
            return;
        }
        if entity.is_in_water() {
            entity.set_air_supply(300);
        } else if entity.on_ground.load(Ordering::Relaxed) {
            let mut rng = mob.get_random();
            entity.velocity.store(
                entity.velocity.load()
                    + Vector3::new(
                        f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.4),
                        0.5,
                        f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.4),
                    ),
            );
            entity.yaw.store(rng.random::<f32>() * 360.0);
            entity.on_ground.store(false, Ordering::Relaxed);
            entity.velocity_dirty.store(true, Ordering::Relaxed);
        }
        if self.has_active_attack_target() {
            entity.yaw.store(entity.head_yaw.load());
        }
    }

    /// Vanilla `Guardian.travelInWater`: slow drift, sinking only while idle with no target.
    pub fn travel_in_water(&self, mob: &dyn Mob, caller: &dyn EntityBase) {
        let mob_entity = mob.get_mob_entity();
        let living = &mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(living.movement_input.load(), 0.1);
        living.make_move(caller);
        let mut velocity = entity.velocity.load() * 0.9;
        if !self.is_moving() && mob_entity.get_target().is_none() {
            velocity.y -= 0.005;
        }
        entity.velocity.store(velocity);
    }

    /// Vanilla `Guardian.getWalkTargetValue`: water is worth 10 plus its light cost; elsewhere
    /// the `Monster` value, darker is better.
    pub fn walk_target_value(mob: &dyn Mob, pos: &BlockPos) -> f32 {
        let world = mob.get_entity().world.load();
        let light_cost = world.get_light_level_dependent_magic_value(pos) - 0.5;
        if world.get_fluid(pos).has_tag(&tag::Fluid::MINECRAFT_WATER) {
            10.0 + light_cost
        } else {
            -light_cost
        }
    }

    /// The thorns half of vanilla `Guardian.hurtServer`: a guardian with its spikes out (not
    /// moving) hurts a living direct attacker for 2, then strolls off. PEX passes a projectile's
    /// owner as `source`, so projectile damage is left out: vanilla's direct entity there is the
    /// projectile, never a living entity.
    pub fn on_hurt(
        &self,
        guardian: &dyn EntityBase,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
    ) {
        if !self.is_moving()
            && !damage_type.has_tag(&tag::DamageType::MINECRAFT_AVOIDS_GUARDIAN_THORNS)
            && !damage_type.has_tag(&tag::DamageType::MINECRAFT_IS_PROJECTILE)
            && damage_type != DamageType::THORNS
            && let Some(attacker) = source
            && attacker.get_living_entity().is_some()
        {
            attacker.damage_with_context(
                attacker,
                2.0,
                DamageType::THORNS,
                None,
                Some(guardian),
                Some(guardian),
            );
        }
        self.trigger_stroll();
    }
}

/// Vanilla `Guardian.GuardianMoveControl`: swims straight at the wanted spot with a wobble,
/// eases the look towards the way it swims, and marks the guardian as moving.
struct GuardianMoveControl {
    inner: MoveControl,
    state: Arc<GuardianState>,
}

impl Control for GuardianMoveControl {}

impl MoveControlTrait for GuardianMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let living = &mob_entity.living_entity;
        let entity = &living.entity;
        let pos = entity.pos.load();
        let delta = Vector3::new(
            self.inner.wanted_x - pos.x,
            self.inner.wanted_y - pos.y,
            self.inner.wanted_z - pos.z,
        );
        let length = delta.length();
        if self.inner.operation != Operation::MoveTo
            || mob_entity.steered().navigator().is_done()
            || length == 0.0
        {
            mob_entity.set_speed(0.0);
            self.state.set_moving(entity, false);
            return;
        }
        let xd = delta.x / length;
        let yd = delta.y / length;
        let zd = delta.z / length;
        let y_rot_d = delta.z.atan2(delta.x).to_degrees() as f32 - 90.0;
        let yaw = MoveControl::rotlerp(entity.yaw.load(), y_rot_d, 90.0);
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);
        let target_speed = (self.inner.speed_modifier
            * living.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let new_speed = living.get_speed() + 0.125 * (target_speed - living.get_speed());
        mob_entity.set_speed(new_speed);
        let phase = f64::from(
            entity
                .tick_count
                .load(Ordering::Relaxed)
                .wrapping_add(entity.entity_id),
        );
        let push = (phase * 0.5).sin() * 0.05;
        let (sin, cos) = f64::from(yaw.to_radians()).sin_cos();
        let y_push = (phase * 0.75).sin() * 0.05;
        entity.velocity.store(entity.velocity.load().add_raw(
            push * cos,
            y_push * (sin + cos) * 0.25 + f64::from(new_speed) * yd * 0.1,
            push * sin,
        ));
        let new_look = Vector3::new(
            pos.x + xd * 2.0,
            entity.get_eye_y() + yd / length,
            pos.z + zd * 2.0,
        );
        let mut look_control = mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let old_look = look_control.looking_at().unwrap_or(new_look);
        look_control.look_at_with_range(
            old_look.x + 0.125 * (new_look.x - old_look.x),
            old_look.y + 0.125 * (new_look.y - old_look.y),
            old_look.z + 0.125 * (new_look.z - old_look.z),
            10.0,
            40.0,
        );
        drop(look_control);
        self.state.set_moving(entity, true);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.inner.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forwards: f32, right: f32) {
        self.inner.strafe(forwards, right);
    }

    fn has_wanted(&self) -> bool {
        self.inner.has_wanted()
    }

    fn wanted_position(&self) -> Option<Vector3<f64>> {
        self.inner.has_wanted().then(|| {
            Vector3::new(
                self.inner.wanted_x,
                self.inner.wanted_y,
                self.inner.wanted_z,
            )
        })
    }
}

/// Vanilla `Guardian.GuardianAttackGoal`: lock the beam on for the attack duration, then
/// magic damage plus a melee hit. Also used by the elder guardian.
pub struct GuardianAttackGoal {
    attack_time: i32,
    elder: bool,
    state: Arc<GuardianState>,
}

impl GuardianAttackGoal {
    #[must_use]
    pub const fn new(elder: bool, state: Arc<GuardianState>) -> Self {
        Self {
            attack_time: 0,
            elder,
            state,
        }
    }

    const fn attack_duration(&self) -> i32 {
        if self.elder {
            ELDER_ATTACK_TIME
        } else {
            ATTACK_TIME
        }
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
        mob.get_entity()
            .velocity_dirty
            .store(true, Ordering::Relaxed);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.state.set_active_attack_target(mob.get_entity(), 0);
        mob.get_mob_entity().set_target(None);
        self.state.trigger_stroll();
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
            self.state
                .set_active_attack_target(entity, target.get_entity().entity_id);
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
    pub state: Arc<GuardianState>,
}

impl GuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        let state = Arc::new(GuardianState::default());
        state.init_controls(&mut mob_entity);
        let mob_arc = Arc::new(Self { mob_entity, state });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        mob_arc
            .state
            .register_goals(&mob_arc.mob_entity, &mob_weak, false);
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

    /// Vanilla `Guardian.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        160
    }

    /// Vanilla `Guardian.getMaxHeadXRot`.
    fn get_max_look_pitch_change(&self) -> f32 {
        180.0
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

    fn before_mob_ai_step(&self) {
        self.state.ai_step(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        self.state.travel_in_water(self, caller);
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        GuardianState::walk_target_value(self, pos)
    }

    fn pre_damage_with_context(
        &self,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
        _cause: Option<&dyn EntityBase>,
    ) -> bool {
        self.state.on_hurt(self, damage_type, source);
        true
    }
}
