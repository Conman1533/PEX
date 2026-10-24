use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_data::entity::EntityPose;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal, breed::BreedGoal,
        leap_at_target::LeapAtTargetGoal, look_at_entity::LookAtEntityGoal,
        ocelot_attack::OcelotAttackGoal, swim::SwimGoal, tempt::TemptGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::{animal::Animal, turtle::baby_on_land_selector},
    player::Player,
};

/// Represents an Ocelot, a shy passive mob found in jungles.
///
/// Wiki: <https://minecraft.wiki/w/Ocelot>
pub struct OcelotEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub is_trusting: AtomicBool,
}

impl OcelotEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let ocelot = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            is_trusting: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(ocelot);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            // Goal 1: FloatGoal (SwimGoal)
            goal_selector.add_goal(1, Box::new(SwimGoal::default()));
            // Goal 3: OcelotTemptGoal
            goal_selector.add_goal(
                3,
                Box::new(
                    TemptGoal::new(
                        0.6,
                        |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_OCELOT_FOOD),
                        true,
                    )
                    .scared_only_if(|mob| {
                        mob.cast_any()
                            .downcast_ref::<Self>()
                            .is_some_and(|ocelot| !ocelot.is_trusting())
                    }),
                ),
            );
            // Goal 4: OcelotAvoidEntityGoal (when not trusting)
            goal_selector.add_goal(
                4,
                Box::new(
                    AvoidEntityGoal::new(&EntityType::PLAYER, 16.0, 0.8, 1.33).gated_by(|mob| {
                        mob.cast_any()
                            .downcast_ref::<Self>()
                            .is_some_and(|ocelot| !ocelot.is_trusting())
                    }),
                ),
            );
            // Goal 7: LeapAtTargetGoal
            goal_selector.add_goal(7, Box::new(LeapAtTargetGoal::new(0.3)));
            // Goal 8: OcelotAttackGoal
            goal_selector.add_goal(8, Box::new(OcelotAttackGoal::new()));
            // Goal 9: BreedGoal
            goal_selector.add_goal(9, BreedGoal::new(0.8));
            // Goal 10: WaterAvoidingRandomStrollGoal (WanderAroundGoal)
            goal_selector.add_goal(
                10,
                Box::new(WanderAroundGoal::water_avoiding_with_probability(
                    0.8,
                    1.000_000_1e-5,
                )),
            );
            // Goal 11: LookAtPlayerGoal
            goal_selector.add_goal(
                11,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 10.0),
            );
        };

        {
            let target_selector = &mob_arc.mob_entity.target_selector;

            // Target Goal 1: NearestAttackableTargetGoal for Chicken and Turtle
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::CHICKEN, false),
            );
            target_selector.add_goal(
                1,
                Box::new(ActiveTargetGoal::new(
                    &mob_arc.mob_entity,
                    &EntityType::TURTLE,
                    10,
                    false,
                    false,
                    Some(baby_on_land_selector),
                )),
            );
        };

        mob_arc
    }

    pub fn is_trusting(&self) -> bool {
        self.is_trusting.load(Ordering::Relaxed)
    }

    pub fn set_trusting(&self, trusting: bool) {
        self.is_trusting.store(trusting, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::ocelot::TRUSTING, trusting);
    }
}

impl Animal for OcelotEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack
            .get_item()
            .has_tag(&tag::Item::MINECRAFT_OCELOT_FOOD)
    }
}

impl AgeableMob for OcelotEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

/// Vanilla `Ocelot.customServerAiStep` and its copy in `Cat`: a feline creeps at the tempt
/// speed and sprints at the flee speed. The crouch is cosmetic, so the hitbox stays put.
pub(crate) fn feline_server_ai_step(mob_entity: &MobEntity) {
    const CROUCH_SPEED: f64 = 0.6;
    const SPRINT_SPEED: f64 = 1.33;
    let (moving, speed) = {
        let navigator = mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (!navigator.is_idle(), navigator.get_speed_modifier())
    };
    let living = &mob_entity.living_entity;
    // The speeds are the goals' own constants, so exact comparison is intended.
    let (pose, sprinting) = if moving && speed == CROUCH_SPEED {
        (EntityPose::Crouching, false)
    } else if moving && speed == SPRINT_SPEED {
        (EntityPose::Standing, true)
    } else {
        (EntityPose::Standing, false)
    };
    living.entity.set_pose_keep_dimensions(pose);
    living.set_sprinting(sprinting);
}

impl Mob for OcelotEntity {
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        feline_server_ai_step(&self.mob_entity);
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityOcelotAmbient)
    }

    /// Vanilla `Ocelot.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        900
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    /// Vanilla `Ocelot.removeWhenFarAway`, without the 2400 tick minimum age Pumpkin does not track.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        !self.is_trusting.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("Trusting", self.is_trusting.load(Ordering::Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(trusting) = nbt.get_bool("Trusting") {
            self.is_trusting.store(trusting, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::ocelot::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::ocelot::TRUSTING,
            self.is_trusting.load(Ordering::Relaxed),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let is_food = self.is_food(item_stack);
        let dist_sqr = self
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&player.get_entity().pos.load());

        // Vanilla only rolls for trust while the ocelot is following the fish.
        let tempted = self.mob_entity.goals_selector.is_running::<TemptGoal>();
        if tempted && !self.is_trusting() && is_food && dist_sqr < 9.0 {
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);

            let mut rng = rand::rng();
            if rng.random_range(0..3) == 0 {
                self.set_trusting(true);
                self.get_entity().world.load().send_entity_status(
                    self.get_entity(),
                    EntityStatus::TrustingSucceeded,
                    Some(ActorEventID::TamingSucceeded),
                );
            } else {
                self.get_entity().world.load().send_entity_status(
                    self.get_entity(),
                    EntityStatus::TrustingFailed,
                    Some(ActorEventID::TamingFailed),
                );
            }

            return true;
        }

        self.animal_interact(player, item_stack)
    }
}
