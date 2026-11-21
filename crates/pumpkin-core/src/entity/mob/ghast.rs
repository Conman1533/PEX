use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tracked_data;
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use rand::RngExt;

use crate::entity::ai::control::move_control::Operation;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::ai::goal::active_target::{ActiveTargetGoal, TargetCondition};
use crate::entity::living::LivingEntity;
use crate::entity::projectile::fireball::FireballEntity;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{Controls, Goal},
    mob::{Mob, MobEntity},
};
use crate::world::World;

pub struct GhastEntity {
    pub mob_entity: MobEntity,
    pub is_charging: AtomicBool,
    pub explosion_power: AtomicU8,
}

impl GhastEntity {
    pub const DEFAULT_EXPLOSION_POWER: u8 = 1;
    pub const XP_REWARD: u32 = 5;
    pub const FLYING_SPEED: f64 = 0.06;

    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(GhastMoveControl::default());
        let ghast = Self {
            mob_entity,
            is_charging: AtomicBool::new(false),
            explosion_power: AtomicU8::new(Self::DEFAULT_EXPLOSION_POWER),
        };

        let mob_arc = Arc::new(ghast);

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            // Priority 5: Random floating around
            goal_selector.add_goal(5, Box::new(RandomFloatAroundGoal));

            // Priority 7: Face target / movement direction
            goal_selector.add_goal(7, Box::new(GhastLookGoal::new()));

            // Priority 7: Shoot large fireballs at target
            goal_selector.add_goal(
                7,
                Box::new(GhastShootFireballGoal::new(Arc::downgrade(&mob_arc))),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;

            // Priority 1: the nearest player at most 4 blocks above or below the ghast.
            target_selector.add_goal(
                1,
                Box::new(ActiveTargetGoal::new(
                    &mob_arc.mob_entity,
                    &EntityType::PLAYER,
                    10,
                    true,
                    false,
                    None::<fn(&LivingEntity, &World) -> bool>,
                ))
                .when(TargetCondition::MaxHeightDifference(4.0)),
            );
        };

        mob_arc
    }

    pub fn set_charging(&self, charging: bool) {
        self.is_charging.store(charging, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_synced_data(tracked_data::ghast::DATA_IS_CHARGING, charging);
    }

    #[must_use]
    pub fn is_charging(&self) -> bool {
        self.is_charging.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn get_explosion_power(&self) -> u8 {
        self.explosion_power.load(Ordering::Relaxed)
    }

    pub fn set_explosion_power(&self, power: u8) {
        self.explosion_power.store(power, Ordering::Relaxed);
    }

    #[must_use]
    pub fn check_ghast_spawn_rules(world: &World, pos: &BlockPos) -> bool {
        if world.level_info.load().difficulty == pumpkin_util::Difficulty::Peaceful {
            return false;
        }
        if rand::random_range(0..20) != 0 {
            return false;
        }
        let state = world.get_block_state(pos);
        state.is_air()
    }
}

impl Mob for GhastEntity {
    fn swim_splash_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityGenericSplash
    }

    fn get_sound_volume(&self) -> f32 {
        5.0
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityGhastAmbient)
    }

    fn swim_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityGenericSwim
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0 // Ghasts fly, no gravity applied in standard travel
    }

    fn get_mob_y_velocity_drag(&self) -> Option<f64> {
        Some(0.95)
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        if self.is_charging() {
            entity.set_synced_data(tracked_data::ghast::DATA_IS_CHARGING, true);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        let power = self.get_explosion_power();
        nbt.put_byte("ExplosionPower", i8::try_from(power).unwrap_or(1));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(power) = nbt.get_byte("ExplosionPower") {
            self.set_explosion_power(u8::try_from(power).unwrap_or(Self::DEFAULT_EXPLOSION_POWER));
        }
    }

    fn modify_incoming_damage(&self, amount: f32, damage_type: DamageType) -> f32 {
        if damage_type.id == DamageType::FIREBALL.id {
            1000.0
        } else {
            amount
        }
    }

    fn get_base_experience_reward(&self) -> u32 {
        Self::XP_REWARD
    }
}

pub struct GhastLookGoal {
    goal_control: Controls,
}

impl Default for GhastLookGoal {
    fn default() -> Self {
        Self {
            goal_control: Controls::LOOK,
        }
    }
}

impl GhastLookGoal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Goal for GhastLookGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        true
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let target_opt = mob_entity.get_target();

        if let Some(target) = target_opt {
            let mob_pos = mob_entity.living_entity.entity.pos.load();
            let target_pos = target.get_entity().pos.load();

            if mob_pos.squared_distance_to_vec(&target_pos) < 4096.0 {
                let dx = target_pos.x - mob_pos.x;
                let dz = target_pos.z - mob_pos.z;
                let yaw = (-f64::atan2(dx, dz).to_degrees()) as f32;
                mob_entity.living_entity.entity.yaw.store(yaw);
                mob_entity.living_entity.entity.head_yaw.store(yaw);
            }
        } else {
            let velocity = mob_entity.living_entity.entity.velocity.load();
            if velocity.x != 0.0 || velocity.z != 0.0 {
                let yaw = (-f64::atan2(velocity.x, velocity.z).to_degrees()) as f32;
                mob_entity.living_entity.entity.yaw.store(yaw);
                mob_entity.living_entity.entity.head_yaw.store(yaw);
            }
        }
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}

pub struct GhastShootFireballGoal {
    ghast: Weak<GhastEntity>,
    charge_time: i32,
}

impl GhastShootFireballGoal {
    #[must_use]
    pub const fn new(ghast: Weak<GhastEntity>) -> Self {
        Self {
            ghast,
            charge_time: 0,
        }
    }
}

impl Goal for GhastShootFireballGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.ghast
            .upgrade()
            .is_some_and(|ghast| ghast.mob_entity.get_target().is_some())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob)
    }

    fn start(&mut self, _mob: &dyn Mob) {
        self.charge_time = 0;
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(ghast) = self.ghast.upgrade() {
            ghast.set_charging(false);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    /// Vanilla `GhastShootFireballGoal.tick`.
    fn tick(&mut self, mob: &dyn Mob) {
        let Some(ghast) = self.ghast.upgrade() else {
            return;
        };
        let Some(target) = ghast.mob_entity.get_target() else {
            return;
        };

        let entity = &ghast.mob_entity.living_entity.entity;
        let ghast_pos = entity.pos.load();
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();

        if ghast_pos.squared_distance_to_vec(&target_pos) < 4096.0
            && mob.has_line_of_sight(target_entity)
        {
            let world = entity.world.load();
            self.charge_time += 1;

            if self.charge_time == 10 && !entity.is_silent() {
                world.sync_world_event(WorldEvent::SoundGhastWarning, entity.block_pos.load(), 0);
            }

            if self.charge_time == 20 {
                let yaw_rad = f64::from(entity.yaw.load()).to_radians();
                let pitch_rad = f64::from(entity.pitch.load()).to_radians();
                let view_x = -pitch_rad.cos() * yaw_rad.sin();
                let view_z = pitch_rad.cos() * yaw_rad.cos();

                let half_height = f64::from(entity.entity_dimension.load().height) * 0.5;
                let spawn_pos = Vector3::new(
                    ghast_pos.x + view_x * 4.0,
                    ghast_pos.y + half_height + 0.5,
                    ghast_pos.z + view_z * 4.0,
                );
                // Vanilla aims at the middle of the target's body.
                let target_y =
                    target_pos.y + f64::from(target_entity.entity_dimension.load().height) * 0.5;
                let direction = Vector3::new(
                    target_pos.x - spawn_pos.x,
                    target_y - spawn_pos.y,
                    target_pos.z - spawn_pos.z,
                );

                if !entity.is_silent() {
                    world.sync_world_event(
                        WorldEvent::SoundGhastFireball,
                        entity.block_pos.load(),
                        0,
                    );
                }

                let fireball_base = Entity::from_uuid(
                    uuid::Uuid::new_v4(),
                    world.clone(),
                    spawn_pos,
                    &EntityType::FIREBALL,
                );
                let fireball = FireballEntity::new_shot(fireball_base, entity, direction);
                fireball
                    .explosion_power
                    .store(f32::from(ghast.get_explosion_power()), Ordering::Relaxed);

                world.spawn_entity_non_save(Arc::new(fireball));
                self.charge_time = -40;
            }
        } else if self.charge_time > 0 {
            self.charge_time -= 1;
        }

        ghast.set_charging(self.charge_time > 10);
    }

    fn controls(&self) -> Controls {
        Controls::empty()
    }
}

/// Vanilla `Ghast.RandomFloatAroundGoal`: picks a spot up to 16 blocks away on each axis for the
/// move control whenever the current one is reached, gone or more than 60 blocks off.
pub struct RandomFloatAroundGoal;

impl Goal for RandomFloatAroundGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let wanted = mob
            .get_mob_entity()
            .steered()
            .move_control()
            .wanted_position();
        wanted.is_none_or(|wanted| {
            let dist_sq = mob.get_entity().pos.load().squared_distance_to_vec(&wanted);
            !(1.0..=3600.0).contains(&dist_sq)
        })
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let pos = entity.pos.load();
        let mut target = {
            let mut rng = mob.get_random();
            let mut offset = || f64::from(rng.random::<f32>() * 2.0 - 1.0) * 16.0;
            Vector3::new(pos.x + offset(), pos.y + offset(), pos.z + offset())
        };
        // Vanilla `getSuitableFlyToPosition`: over open ground, fly down rather than up.
        let world = entity.world.load();
        let column = BlockPos::floored_v(target);
        let height =
            world.get_heightmap_height(ChunkHeightmapType::MotionBlocking, column.0.x, column.0.z)
                + 1;
        if height < column.0.y && height > world.dimension.min_y {
            target.y = pos.y - (pos.y - target.y).abs();
        }
        mob.get_mob_entity()
            .steered()
            .move_control()
            .set_wanted_position(target.x, target.y, target.z, 1.0);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Ghast.GhastMoveControl` (not careful, never stopped): every 2 to 6 ticks, push
/// towards the wanted spot if the way there is clear, otherwise give it up.
#[derive(Default)]
pub struct GhastMoveControl {
    wanted: Vector3<f64>,
    operation: Operation,
    float_duration: i32,
}

impl Control for GhastMoveControl {}

impl GhastMoveControl {
    /// Vanilla `canReach`, sampled along the way once per block: the box at each step must be
    /// free of block collisions. Blocks the ghast already overlaps don't count, as in vanilla.
    fn can_reach(world: &World, bb: &BoundingBox, travel: Vector3<f64>) -> bool {
        let start_clear = world.is_space_empty(*bb);
        let steps = travel.length().ceil().max(1.0);
        let mut step = 1.0;
        while step <= steps {
            let t = step / steps;
            let moved = bb.shift(travel.multiply(t, t, t));
            if (start_clear || step == steps) && !world.is_space_empty(moved) {
                return false;
            }
            step += 1.0;
        }
        true
    }
}

impl MoveControlTrait for GhastMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if self.operation != Operation::MoveTo {
            return;
        }
        let duration = self.float_duration;
        self.float_duration -= 1;
        if duration > 0 {
            return;
        }
        self.float_duration += mob.get_random().random_range(0..5) + 2;

        let living = &mob.get_mob_entity().living_entity;
        let entity = &living.entity;
        let pos = entity.pos.load();
        let travel = Vector3::new(
            self.wanted.x - pos.x,
            self.wanted.y - pos.y,
            self.wanted.z - pos.z,
        );
        let world = entity.world.load();
        if Self::can_reach(&world, &entity.bounding_box.load(), travel) {
            let speed = living.get_attribute_value(&Attributes::FLYING_SPEED) * 5.0 / 3.0;
            let push = travel.normalize().multiply(speed, speed, speed);
            let velocity = entity.velocity.load();
            entity.velocity.store(velocity.add(&push));
        } else {
            self.operation = Operation::Wait;
        }
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, _speed_modifier: f64) {
        self.wanted = Vector3::new(x, y, z);
        self.operation = Operation::MoveTo;
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }

    fn wanted_position(&self) -> Option<Vector3<f64>> {
        self.has_wanted().then_some(self.wanted)
    }
}
