use crate::entity::passive::animal::create_offspring;
use crate::world::World;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::{vector3::Vector3, wrap_degrees};
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::control::{Control, MoveControlTrait, move_control::MoveControl},
    ai::goal::{
        active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal, breed::BreedGoal,
        climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal, escape_danger::EscapeDangerGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum RabbitVariant {
    #[default]
    Brown = 0,
    White = 1,
    Black = 2,
    WhiteSplotched = 3,
    Gold = 4,
    Salt = 5,
    Evil = 99,
}

impl RabbitVariant {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            99 => Self::Evil,
            _ => Self::Brown,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub fn random_variant() -> Self {
        let mut rng = rand::rng();
        match rng.random_range(0..6) {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            _ => Self::Brown,
        }
    }
}

pub struct RabbitEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub variant: AtomicI32,
    pub more_carrot_ticks: AtomicI32,
    /// Vanilla `jumpDelayTicks`: the pause after landing before the next hop.
    jump_delay_ticks: AtomicI32,
    was_on_ground: AtomicBool,
    /// Vanilla `jumping`: mid-hop, so the move control may push the rabbit along.
    hopping: AtomicBool,
    /// Vanilla `RabbitMoveControl.nextJumpSpeed`.
    next_jump_speed: AtomicCell<f64>,
    wanted: AtomicCell<Vector3<f64>>,
}

impl RabbitEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let variant = RabbitVariant::random_variant();
        let rabbit = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            variant: AtomicI32::new(variant.id()),
            more_carrot_ticks: AtomicI32::new(0),
            jump_delay_ticks: AtomicI32::new(0),
            was_on_ground: AtomicBool::new(false),
            hopping: AtomicBool::new(false),
            next_jump_speed: AtomicCell::new(0.0),
            wanted: AtomicCell::new(Vector3::default()),
        };
        let mob_arc = Arc::new(rabbit);
        *mob_arc
            .mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(RabbitMoveControl {
            rabbit: Arc::downgrade(&mob_arc),
            inner: MoveControl::default(),
        });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal));
            goal_selector.add_goal(1, EscapeDangerGoal::new(2.2));
            goal_selector.add_goal(2, BreedGoal::new(0.8));
            goal_selector.add_goal(
                3,
                Box::new(TemptGoal::new(
                    1.0,
                    |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_RABBIT_FOOD),
                    false,
                )),
            );
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::PLAYER, 8.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::WOLF, 10.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::FOX, 10.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::water_avoiding(0.6)));
            goal_selector.add_goal(
                11,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 10.0),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn get_variant(&self) -> RabbitVariant {
        RabbitVariant::from_id(self.variant.load(Ordering::Relaxed))
    }

    pub fn set_variant(&self, variant: RabbitVariant) {
        self.variant.store(variant.id(), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::rabbit::DATA_TYPE_ID,
            VarInt(variant.id()),
        );

        if variant == RabbitVariant::Evil {
            let goal_selector = &self.mob_entity.goals_selector;
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.4, true)));

            let target_selector = &self.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::WOLF, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::FOX, true),
            );
        }
    }
}

impl AgeableMob for RabbitEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for RabbitEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_RABBIT_FOOD)
    }

    /// Vanilla `Rabbit.getBreedOffspring`: a parent's variant, or one time in 20 the variant a
    /// new rabbit would spawn with.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        partner: &dyn Mob,
    ) -> Option<Arc<dyn EntityBase>> {
        let baby = create_offspring(&self.mob_entity, &EntityType::RABBIT, world);
        if let Some(offspring) = baby.cast_any().downcast_ref::<Self>() {
            let mut rng = self.get_random();
            if rng.random_range(0..20) != 0 {
                let variant = match partner.cast_any().downcast_ref::<Self>() {
                    Some(partner) if rng.random::<bool>() => partner.get_variant(),
                    _ => self.get_variant(),
                };
                offspring.set_variant(variant);
            }
        }
        Some(baby)
    }
}

impl RabbitEntity {
    /// Vanilla `Rabbit.startJumping` followed by `jumpFromGround`.
    fn hop(&self) {
        self.hopping.store(true, Ordering::Relaxed);
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();
        let wanted = self.wanted.load();

        // Vanilla `facePoint`.
        let yaw =
            wrap_degrees(((wanted.z - pos.z).atan2(wanted.x - pos.x).to_degrees() as f32) - 90.0);
        entity.yaw.store(yaw);

        let mut rng = self.get_random();
        let pitch = ((rng.random::<f32>() - rng.random::<f32>()) * 0.2 + 1.0) * 0.8;
        world.play_sound_fine(
            Sound::EntityRabbitJump,
            SoundCategory::Neutral,
            &pos,
            1.0,
            pitch,
        );

        // Vanilla `Rabbit.getJumpPower`.
        let speed = self.next_jump_speed.load();
        let mut base = if speed <= 0.6 { 0.2 } else { 0.3 };
        let path_up = self
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_path()
            .and_then(|path| path.get_next_entity_pos(entity.width()))
            .is_some_and(|next| next.y > pos.y + 0.5);
        if path_up || entity.horizontal_collision.load(Ordering::Relaxed) || wanted.y > pos.y + 0.5
        {
            base = 0.5;
        }
        let mut velocity = entity.velocity.load();
        velocity.y = living.get_jump_velocity(base / 0.42).max(velocity.y);
        if speed > 0.0 && velocity.x * velocity.x + velocity.z * velocity.z < 0.01 {
            // Vanilla `moveRelative(0.1, (0, 1.5 or 0.5, 1))`.
            let input = Vector3::new(0.0, if self.is_baby() { 0.5 } else { 1.5 }, 1.0);
            let push = input * (0.1 / input.length());
            let (sin, cos) = f64::from(yaw.to_radians()).sin_cos();
            velocity += Vector3::new(-push.z * sin, push.y, push.z * cos);
        }
        entity.velocity.store(velocity);
        entity
            .velocity_dirty
            .store(true, std::sync::atomic::Ordering::SeqCst);
        world.send_entity_status(entity, EntityStatus::Jump, ActorEventID::Jump);
    }
}

/// Vanilla `Rabbit.RabbitMoveControl`: stands still on the ground between hops.
struct RabbitMoveControl {
    rabbit: Weak<RabbitEntity>,
    inner: MoveControl,
}

impl Control for RabbitMoveControl {}

impl MoveControlTrait for RabbitMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(rabbit) = self.rabbit.upgrade() {
            let on_ground = rabbit.get_entity().on_ground.load(Ordering::Relaxed);
            if on_ground && !rabbit.hopping.load(Ordering::Relaxed) {
                self.inner.speed_modifier = 0.0;
            } else if MoveControlTrait::has_wanted(&self.inner) {
                self.inner.speed_modifier = rabbit.next_jump_speed.load();
            }
        }
        self.inner.tick(mob);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        let mut speed = speed_modifier;
        if let Some(rabbit) = self.rabbit.upgrade() {
            if rabbit.get_entity().is_in_water() {
                speed = 1.5;
            }
            rabbit.wanted.store(Vector3::new(x, y, z));
            if speed > 0.0 {
                rabbit.next_jump_speed.store(speed);
            }
        }
        MoveControlTrait::set_wanted_position(&mut self.inner, x, y, z, speed);
    }

    fn strafe(&mut self, forward: f32, right: f32) {
        self.inner.strafe(forward, right);
    }

    fn has_wanted(&self) -> bool {
        MoveControlTrait::has_wanted(&self.inner)
    }
}

impl Mob for RabbitEntity {
    /// Vanilla `Rabbit.customServerAiStep`: rabbits get about in hops, each after a landing
    /// delay.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let delay = self.jump_delay_ticks.load(Ordering::Relaxed);
        if delay > 0 {
            self.jump_delay_ticks.store(delay - 1, Ordering::Relaxed);
        }
        let on_ground = self.get_entity().on_ground.load(Ordering::Relaxed);
        if on_ground {
            if !self.was_on_ground.load(Ordering::Relaxed) {
                self.hopping.store(false, Ordering::Relaxed);
                let landing_delay = if self.next_jump_speed.load() < 2.2 {
                    10
                } else {
                    1
                };
                self.jump_delay_ticks
                    .store(landing_delay, Ordering::Relaxed);
            }
            let wants_to_move = self
                .mob_entity
                .move_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .has_wanted();
            if wants_to_move
                && !self.hopping.load(Ordering::Relaxed)
                && self.jump_delay_ticks.load(Ordering::Relaxed) == 0
            {
                self.hop();
            }
        }
        self.was_on_ground.store(on_ground, Ordering::Relaxed);
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityRabbitAmbient)
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_int("RabbitType", self.get_variant().id());
        nbt.put_int(
            "MoreCarrotTicks",
            self.more_carrot_ticks.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(rabbit_type) = nbt.get_int("RabbitType") {
            self.set_variant(RabbitVariant::from_id(rabbit_type));
        }
        if let Some(ticks) = nbt.get_int("MoreCarrotTicks") {
            self.more_carrot_ticks.store(ticks, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::rabbit::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::rabbit::DATA_TYPE_ID,
            VarInt(self.get_variant().id()),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}
