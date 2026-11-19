use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::Block;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::{EntityPose, EntityType};
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::clip::{ClipBlock, ClipContext, ClipFluid};
use rand::RngExt;
use rand::seq::SliceRandom;

use crate::entity::util::RandomExt;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    ai::pathfinder::NavigatorGoal,
    ai::util::default_random_pos,
    mob::{Mob, MobEntity},
    projectile::{
        ThrownItemEntity,
        wind_charge::{WIND_CHARGE_GRAVITY, WindChargeEntity},
    },
};
use crate::world::natural_spawner::is_block_dangerous;

/// Vanilla `BreezeAi` constants.
const SPEED_MULTIPLIER_WHEN_SLIDING: f64 = 0.6;
const JUMP_CIRCLE_INNER_RADIUS: f64 = 4.0;
const JUMP_CIRCLE_MIDDLE_RADIUS: f64 = 8.0;
const JUMP_CIRCLE_DISTANCE_Y: f64 = 10.0;
/// `SlideToTargetSink(20, 40)`.
const SLIDE_MIN_TIMEOUT: i64 = 20;
const SLIDE_MAX_TIMEOUT: i64 = 40;
/// How long `BREEZE_SHOOT` is granted after a slide, a landing and when stuck.
const SHOOT_AFTER_SLIDE_TICKS: i64 = 60;
const SHOOT_AFTER_JUMP_TICKS: i64 = 100;
const SHOOT_WHEN_STUCK_TICKS: i64 = 60;

/// Vanilla `Shoot` constants.
const ATTACK_RANGE_MAX_SQRT: f64 = 256.0;
const UNCERTAINTY_BASE: i32 = 5;
const UNCERTAINTY_MULTIPLIER: i32 = 4;
const PROJECTILE_MOVEMENT_SCALE: f64 = 0.7;
const SHOOT_INITIAL_DELAY_TICKS: i64 = 15;
const SHOOT_RECOVER_DELAY_TICKS: i64 = 4;
const SHOOT_COOLDOWN_TICKS: i64 = 10;

/// Vanilla `LongJump` constants.
const REQUIRED_AIR_BLOCKS_ABOVE: i32 = 4;
const JUMP_COOLDOWN_TICKS: i64 = 10;
const JUMP_COOLDOWN_WHEN_HURT_TICKS: i64 = 2;
const INHALING_DURATION_TICKS: i64 = 10;
const LONG_JUMP_MAX_DURATION: i64 = 200;
const MAX_JUMP_VELOCITY_MULTIPLIER: f64 = 0.058_333_334;
const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];

/// Vanilla `BreezeUtil.MAX_LINE_OF_SIGHT_TEST_RANGE`.
const MAX_LINE_OF_SIGHT_TEST_RANGE: f64 = 50.0;

pub struct BreezeEntity {
    pub mob_entity: MobEntity,
}

impl BreezeEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let breeze = Self { mob_entity };
        let mob_arc = Arc::new(breeze);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(BreezeFightGoal::default()));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
        };

        mob_arc
    }
}

impl Mob for BreezeEntity {
    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::Events
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }
}

enum Phase {
    Idle,
    /// `SlideToTargetSink` walking to the point `Slide` picked.
    Sliding {
        until: i64,
    },
    /// `Shoot`: charging, one wind charge, recovering.
    Shooting {
        started: i64,
        fired: bool,
    },
    /// `LongJump`: inhaling, then in the air.
    LongJump {
        started: i64,
        inhale_until: i64,
        target: BlockPos,
        jumped_at: Option<i64>,
        /// Vanilla `BREEZE_LEAVING_WATER`: set when jumping out of water, cleared once out of
        /// it, so a jump that ends back in water still lands.
        leaving_water: bool,
    },
}

/// The breeze's fight activity, run as one goal.
///
/// Vanilla `BreezeAi.initFightActivity` plus the idle activity's `SlideToTargetSink`.
/// Vanilla cycles slide, shoot, long jump, shoot; the memories that drive that cycle are
/// the `*_until` world ages here.
pub struct BreezeFightGoal {
    phase: Phase,
    /// `BREEZE_SHOOT` is present while the world age is below this.
    shoot_until: i64,
    /// `BREEZE_SHOOT_COOLDOWN`.
    shoot_cooldown_until: i64,
    /// `BREEZE_JUMP_COOLDOWN`.
    jump_cooldown_until: i64,
}

impl Default for BreezeFightGoal {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            shoot_until: 0,
            shoot_cooldown_until: 0,
            jump_cooldown_until: 0,
        }
    }
}

fn world_age(mob: &dyn Mob) -> i64 {
    mob.get_entity().world.load().get_world_age()
}

fn follow_range(mob: &dyn Mob) -> f64 {
    mob.get_mob_entity()
        .living_entity
        .get_attribute_value(&Attributes::FOLLOW_RANGE)
}

/// Vanilla `BreezeUtil.randomPointBehindTarget`.
fn random_point_behind_target(enemy: &Entity) -> Vector3<f64> {
    let mut rng = rand::rng();
    let gaussian = rng.gaussian();
    let view_angle = enemy.head_yaw.load() + 180.0 + gaussian as f32 * 90.0 / 2.0;
    let r = rng.random_range(0.0f32..1.0).mul_add(4.0, 4.0);
    let yaw = view_angle.to_radians();
    let direction = Vector3::new(f64::from(-yaw.sin()), 0.0, f64::from(yaw.cos()));
    enemy
        .pos
        .load()
        .add(&direction.multiply(f64::from(r), f64::from(r), f64::from(r)))
}

/// Vanilla `BreezeUtil.hasLineOfSight`.
fn has_line_of_sight(mob: &dyn Mob, target: Vector3<f64>) -> bool {
    let from = mob.get_entity().pos.load();
    if target.squared_distance_to_vec(&from)
        > MAX_LINE_OF_SIGHT_TEST_RANGE.max(follow_range(mob)).powi(2)
    {
        return false;
    }
    mob.get_entity()
        .world
        .load()
        .clip(&ClipContext::new(
            from,
            target,
            ClipBlock::Collider,
            ClipFluid::None,
        ))
        .is_miss
}

/// Vanilla `LongJump.snapToSurface`.
fn snap_to_surface(mob: &dyn Mob, target: Vector3<f64>) -> Option<BlockPos> {
    let world = mob.get_entity().world.load();
    for dy in [-10.0, 10.0] {
        let hit = world.clip(&ClipContext::new(
            target,
            target.add_raw(0.0, dy, 0.0),
            ClipBlock::Collider,
            ClipFluid::None,
        ));
        if !hit.is_miss {
            let loc = hit.location;
            return Some(BlockPos::floored(loc.x, loc.y, loc.z).up());
        }
    }
    None
}

/// Vanilla `LongJump.canJumpFromCurrentPosition`.
fn can_jump_from_current_position(mob: &dyn Mob) -> bool {
    let entity = mob.get_entity();
    let world = entity.world.load();
    let pos = entity.block_pos.load();
    if world.get_block(&pos).id == Block::HONEY_BLOCK.id {
        return false;
    }
    (1..=REQUIRED_AIR_BLOCKS_ABOVE).all(|i| {
        let above = BlockPos::new(pos.0.x, pos.0.y + i, pos.0.z);
        world.get_block_state(&above).is_air()
            || world
                .get_fluid(&above)
                .has_tag(&tag::Fluid::MINECRAFT_WATER)
    })
}

/// Vanilla `LongJumpUtil.calculateJumpVectorForAngle` without the collision samples
/// (`LongJump` passes `checkCollision = false`).
fn calculate_jump_vector_for_angle(
    mob: &dyn Mob,
    target: Vector3<f64>,
    max_jump_velocity: f64,
    angle: i32,
) -> Option<Vector3<f64>> {
    let mob_pos = mob.get_entity().pos.load();
    let plane = Vector3::new(target.x - mob_pos.x, 0.0, target.z - mob_pos.z)
        .normalize()
        .multiply(0.5, 0.5, 0.5);
    let direction = target.sub(&plane).sub(&mob_pos);
    let angrad = f64::from(angle as f32 * std::f32::consts::PI / 180.0);
    let xz_ang = direction.z.atan2(direction.x);
    let r2 = direction.x * direction.x + direction.z * direction.z;
    let r = r2.sqrt();
    let y = direction.y;
    let g = mob
        .get_mob_entity()
        .living_entity
        .get_attribute_value(&Attributes::GRAVITY);
    let v0sqr = r2 * g / (r * (2.0 * angrad).sin() - 2.0 * y * angrad.cos().powi(2));
    if v0sqr < 0.0 {
        return None;
    }
    let v0 = v0sqr.sqrt();
    if v0 > max_jump_velocity {
        return None;
    }
    let v0r = v0 * angrad.cos();
    let v0y = v0 * angrad.sin();
    let scale = f64::from(0.95f32);
    Some(Vector3::new(v0r * xz_ang.cos(), v0y, v0r * xz_ang.sin()).multiply(scale, scale, scale))
}

impl BreezeFightGoal {
    fn pose(mob: &dyn Mob) -> EntityPose {
        mob.get_entity().pose.load()
    }

    /// Vanilla `LongJump.canRun`, minus the target memory it fills in (returned instead).
    fn long_jump_target(mob: &dyn Mob, target: &dyn EntityBase) -> Option<BlockPos> {
        let entity = mob.get_entity();
        if !entity.on_ground.load(Ordering::Relaxed) && !entity.is_in_water() {
            return None;
        }
        if entity.touching_lava.load(Ordering::Relaxed) {
            return None;
        }
        let target_entity = target.get_entity();
        let distance = entity
            .pos
            .load()
            .squared_distance_to_vec(&target_entity.pos.load())
            .sqrt();
        if distance >= follow_range(mob) || distance - JUMP_CIRCLE_INNER_RADIUS <= 0.0 {
            return None;
        }
        if !can_jump_from_current_position(mob) {
            return None;
        }
        let pos = snap_to_surface(mob, random_point_behind_target(target_entity))?;
        let world = entity.world.load();
        if is_block_dangerous(entity.entity_type, world.get_block_state(&pos.down())) {
            return None;
        }
        let center = pos.to_centered_f64();
        let above = BlockPos::new(pos.0.x, pos.0.y + REQUIRED_AIR_BLOCKS_ABOVE, pos.0.z);
        if !has_line_of_sight(mob, center) && !has_line_of_sight(mob, above.to_centered_f64()) {
            return None;
        }
        Some(pos)
    }

    /// Vanilla `Slide.start`: where to slide to.
    fn slide_target(mob: &dyn Mob, enemy: &Entity) -> Vector3<f64> {
        let entity = mob.get_entity();
        let enemy_pos = enemy.pos.load();
        let own_center = entity.block_pos.load().to_centered_f64();
        let within_inner_ring = {
            let dx = enemy_pos.x - own_center.x;
            let dz = enemy_pos.z - own_center.z;
            dx * dx + dz * dz < JUMP_CIRCLE_INNER_RADIUS * JUMP_CIRCLE_INNER_RADIUS
                && (enemy_pos.y - own_center.y).abs() < JUMP_CIRCLE_DISTANCE_Y
        };
        if within_inner_ring
            && let Some(away) = default_random_pos::get_pos_away(mob, 5, 5, enemy_pos)
            && has_line_of_sight(mob, away)
            && enemy_pos.squared_distance_to_vec(&away)
                > enemy_pos.squared_distance_to_vec(&entity.pos.load())
        {
            return away;
        }
        let mut rng = rand::rng();
        if rng.random_bool(0.5) {
            random_point_behind_target(enemy)
        } else {
            // Vanilla `randomPointInMiddleCircle`.
            let own = entity.pos.load();
            let direction = enemy_pos.sub(&own);
            let lerp = rng.random_range(0.0..1.0);
            let distance = direction.length()
                - (JUMP_CIRCLE_INNER_RADIUS - JUMP_CIRCLE_MIDDLE_RADIUS)
                    .mul_add(lerp, JUMP_CIRCLE_MIDDLE_RADIUS);
            own.add(&direction.normalize().multiply(distance, distance, distance))
        }
    }

    fn start_slide(&mut self, mob: &dyn Mob, enemy: &Entity, now: i64) {
        let target = Self::slide_target(mob, enemy);
        let walk_to = Vector3::new(
            target.x.floor() + 0.5,
            target.y.floor(),
            target.z.floor() + 0.5,
        );
        let entity = mob.get_entity();
        mob.get_mob_entity()
            .steered()
            .navigator()
            .set_progress(NavigatorGoal::new(
                entity.pos.load(),
                walk_to,
                SPEED_MULTIPLIER_WHEN_SLIDING,
            ));
        entity.play_sound(Sound::EntityBreezeSlide);
        entity.set_pose(EntityPose::Sliding);
        let timeout = rand::rng().random_range(SLIDE_MIN_TIMEOUT..=SLIDE_MAX_TIMEOUT);
        self.phase = Phase::Sliding {
            until: now + timeout,
        };
    }

    fn fire_wind_charge(mob: &dyn Mob, target: &dyn EntityBase) {
        let entity = mob.get_entity();
        let world = entity.world.load_full();
        let pos = entity.pos.load();
        // Vanilla `Breeze.getFiringYPosition`.
        let firing_y = pos.y + f64::from(entity.entity_dimension.load().height) / 2.0 + 0.3;
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();
        let height_scale = if target_entity.has_vehicle() {
            0.8
        } else {
            0.3
        };
        let xd = target_pos.x - pos.x;
        let yd = target_pos.y
            + f64::from(target_entity.entity_dimension.load().height) * height_scale
            - firing_y;
        let zd = target_pos.z - pos.z;

        let base = Entity::from_uuid(
            uuid::Uuid::new_v4(),
            world.clone(),
            Vector3::new(pos.x, firing_y, pos.z),
            &EntityType::BREEZE_WIND_CHARGE,
        );
        let thrown = ThrownItemEntity::with_owner(base, entity, WIND_CHARGE_GRAVITY);
        let uncertainty =
            UNCERTAINTY_BASE - world.level_info.load().difficulty as i32 * UNCERTAINTY_MULTIPLIER;
        thrown.set_velocity(
            xd,
            yd,
            zd,
            PROJECTILE_MOVEMENT_SCALE,
            f64::from(uncertainty),
        );
        world.spawn_entity(Arc::new(WindChargeEntity::new_breeze(thrown)));
        world.play_sound_fine(
            Sound::EntityBreezeShoot,
            pumpkin_data::sound::SoundCategory::Hostile,
            &pos,
            1.5,
            1.0,
        );
    }

    /// Vanilla `LongJump.tick` once inhaled: leap at the first reachable angle, if any.
    fn take_off(mob: &dyn Mob, jump_target: BlockPos) -> bool {
        let mut angles = ALLOWED_ANGLES;
        angles.shuffle(&mut rand::rng());
        let max_jump_velocity = MAX_JUMP_VELOCITY_MULTIPLIER * follow_range(mob);
        let target_pos = jump_target.to_f64().add_raw(0.5, 0.0, 0.5);
        let Some(velocity) = angles.iter().find_map(|&angle| {
            calculate_jump_vector_for_angle(mob, target_pos, max_jump_velocity, angle)
        }) else {
            return false;
        };
        let entity = mob.get_entity();
        entity.play_sound(Sound::EntityBreezeJump);
        entity.set_pose(EntityPose::LongJumping);
        mob.get_mob_entity()
            .living_entity
            .discard_friction
            .store(true, Ordering::Relaxed);
        entity.velocity.store(velocity);
        true
    }

    /// Vanilla `LongJump.tick` on landing: jump cooldown (shorter when hurt), then shoot.
    fn land(&mut self, mob: &dyn Mob, now: i64) {
        mob.get_entity().play_sound(Sound::EntityBreezeLand);
        let was_hurt = mob
            .get_mob_entity()
            .living_entity
            .hurt_cooldown
            .load(Ordering::Relaxed)
            > 0;
        self.jump_cooldown_until = now
            + if was_hurt {
                JUMP_COOLDOWN_WHEN_HURT_TICKS
            } else {
                JUMP_COOLDOWN_TICKS
            };
        self.shoot_until = now + SHOOT_AFTER_JUMP_TICKS;
        self.finish_long_jump(mob);
    }

    fn finish_long_jump(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let pose = Self::pose(mob);
        if pose == EntityPose::LongJumping || pose == EntityPose::Inhaling {
            entity.set_pose(EntityPose::Standing);
        }
        mob.get_mob_entity()
            .living_entity
            .discard_friction
            .store(false, Ordering::Relaxed);
        self.phase = Phase::Idle;
    }

    /// One tick of the fight activity's behaviours, in vanilla's priority order.
    fn tick_idle(&mut self, mob: &dyn Mob, target: &Arc<dyn EntityBase>, now: i64) {
        let entity = mob.get_entity();
        let target_entity = target.get_entity();
        let shoot_present = now < self.shoot_until;

        // 1: `Shoot`.
        if shoot_present
            && now >= self.shoot_cooldown_until
            && Self::pose(mob) == EntityPose::Standing
        {
            if entity
                .pos
                .load()
                .squared_distance_to_vec(&target_entity.pos.load())
                < ATTACK_RANGE_MAX_SQRT
            {
                entity.set_pose(EntityPose::Shooting);
                entity.play_sound(Sound::EntityBreezeInhale);
                self.phase = Phase::Shooting {
                    started: now,
                    fired: false,
                };
                return;
            }
            self.shoot_until = 0;
        }
        if now < self.shoot_until {
            return;
        }

        // 2: `LongJump`.
        if now >= self.jump_cooldown_until
            && let Some(jump_target) = Self::long_jump_target(mob, target.as_ref())
        {
            entity.set_pose(EntityPose::Inhaling);
            entity.play_sound(Sound::EntityBreezeCharge);
            entity.look_at(jump_target.to_centered_f64());
            self.phase = Phase::LongJump {
                started: now,
                inhale_until: now + INHALING_DURATION_TICKS,
                target: jump_target,
                jumped_at: None,
                leaving_water: false,
            };
            return;
        }

        // 3: `ShootWhenStuck`.
        let living = &mob.get_mob_entity().living_entity;
        if entity.has_vehicle()
            || entity.is_in_water()
            || living.has_effect(&StatusEffect::LEVITATION)
        {
            self.shoot_until = now + SHOOT_WHEN_STUCK_TICKS;
            return;
        }

        // 4: `Slide`.
        if now >= self.jump_cooldown_until
            && entity.on_ground.load(Ordering::Relaxed)
            && !entity.is_in_water()
            && Self::pose(mob) == EntityPose::Standing
        {
            self.start_slide(mob, target_entity, now);
        }
    }
}

impl Goal for BreezeFightGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|t| t.get_entity().is_alive())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob)
    }

    fn stop(&mut self, mob: &dyn Mob) {
        match self.phase {
            Phase::Sliding { .. } => {
                mob.get_mob_entity().steered().navigator().stop();
                mob.get_entity().set_pose(EntityPose::Standing);
            }
            Phase::Shooting { .. } => {
                if Self::pose(mob) == EntityPose::Shooting {
                    mob.get_entity().set_pose(EntityPose::Standing);
                }
            }
            Phase::LongJump { .. } => self.finish_long_jump(mob),
            Phase::Idle => {}
        }
        self.phase = Phase::Idle;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let now = world_age(mob);
        let entity = mob.get_entity();
        match self.phase {
            Phase::Idle => self.tick_idle(mob, &target, now),
            Phase::Sliding { until } => {
                if mob.is_navigator_idle() || now >= until {
                    mob.get_mob_entity().steered().navigator().stop();
                    entity.set_pose(EntityPose::Standing);
                    self.shoot_until = now + SHOOT_AFTER_SLIDE_TICKS;
                    self.phase = Phase::Idle;
                }
            }
            Phase::Shooting { started, fired } => {
                entity.look_at(target.get_entity().pos.load());
                if !fired && now >= started + SHOOT_INITIAL_DELAY_TICKS {
                    Self::fire_wind_charge(mob, target.as_ref());
                    self.phase = Phase::Shooting {
                        started,
                        fired: true,
                    };
                }
                if now >= started + SHOOT_INITIAL_DELAY_TICKS + 1 + SHOOT_RECOVER_DELAY_TICKS {
                    if Self::pose(mob) == EntityPose::Shooting {
                        entity.set_pose(EntityPose::Standing);
                    }
                    self.shoot_cooldown_until = now + SHOOT_COOLDOWN_TICKS;
                    self.shoot_until = 0;
                    self.phase = Phase::Idle;
                }
            }
            Phase::LongJump {
                started,
                inhale_until,
                target: jump_target,
                jumped_at,
                leaving_water,
            } => {
                if now >= started + LONG_JUMP_MAX_DURATION {
                    self.finish_long_jump(mob);
                    return;
                }
                let in_water = entity.is_in_water();
                let leaving_water = leaving_water && in_water;
                if let Phase::LongJump {
                    leaving_water: stored,
                    ..
                } = &mut self.phase
                {
                    *stored = leaving_water;
                }
                let pose = Self::pose(mob);
                if pose == EntityPose::Inhaling && now >= inhale_until {
                    if Self::take_off(mob, jump_target) {
                        self.phase = Phase::LongJump {
                            started,
                            inhale_until,
                            target: jump_target,
                            jumped_at: Some(now),
                            leaving_water: in_water,
                        };
                    } else {
                        self.finish_long_jump(mob);
                    }
                } else if pose == EntityPose::LongJumping
                    && jumped_at.is_some_and(|t| now > t + 1)
                    && (entity.on_ground.load(Ordering::Relaxed) || (in_water && !leaving_water))
                {
                    self.land(mob, now);
                } else if pose == EntityPose::Standing {
                    self.finish_long_jump(mob);
                }
            }
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
