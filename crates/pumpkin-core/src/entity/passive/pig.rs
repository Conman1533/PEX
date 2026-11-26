use crate::entity::passive::animal::create_offspring;
use crate::world::World;
use rand::RngExt;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::pig_sound_variant::PigSoundVariant;
use pumpkin_data::pig_variant::PigVariant;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{entity::EntityType, item::Item};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;

use crate::entity::EntityBase;
use crate::entity::custom_sound::CustomSound;
use crate::entity::item_steerable::{ItemBasedSteering, ItemSteerable};
use crate::entity::{
    Entity,
    ageable::AgeableMob,
    ai::goal::{
        breed::BreedGoal, escape_danger::EscapeDangerGoal, follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

/// Represents a Pig, a common passive mob that provides porkchops.
///
/// Wiki: <https://minecraft.wiki/w/Pig>
pub struct PigEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: crate::entity::ageable::AgeableData,
    pub steering: ItemBasedSteering,
    pub saddled: AtomicBool,
    pub variant: AtomicU8,
    pub sound_variant: AtomicU8,
}

impl PigEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let pig = Self {
            mob_entity,
            ageable_data: crate::entity::ageable::AgeableData::default(),
            steering: ItemBasedSteering::default(),
            saddled: AtomicBool::new(false),
            variant: AtomicU8::new(PigVariant::default().id()),
            sound_variant: AtomicU8::new(PigSoundVariant::Classic as u8),
        };
        let mob_arc = Arc::new(pig);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(1.25));
            goal_selector.add_goal(3, BreedGoal::new(1.0));
            goal_selector.add_goal(
                4,
                Box::new(TemptGoal::new(
                    1.2,
                    |_, stack| stack.get_item() == &Item::CARROT_ON_A_STICK,
                    false,
                )),
            );
            goal_selector.add_goal(
                4,
                Box::new(TemptGoal::new(
                    1.2,
                    |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_PIG_FOOD),
                    false,
                )),
            );
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(1.1)));
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::water_avoiding(1.0)));
            goal_selector.add_goal(
                7,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    pub fn set_variant(&self, variant: PigVariant) {
        self.variant.store(variant.id(), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::pig::DATA_VARIANT_ID,
            VarInt(variant.id() as i32),
        );
    }

    pub fn set_sound_variant(&self, sound_variant: PigSoundVariant) {
        self.sound_variant
            .store(sound_variant as u8, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::pig::DATA_SOUND_VARIANT_ID,
            VarInt(sound_variant as u8 as i32),
        );
    }
}

impl CustomSound for PigEntity {
    fn death_sound(&self) -> Option<Sound> {
        let is_baby = self.is_baby();
        let sound_variant = PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
            .unwrap_or_default();
        Some(sound_variant.death_sound(is_baby))
    }

    fn hurt_sound(&self) -> Option<Sound> {
        let is_baby = self.is_baby();
        let sound_variant = PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
            .unwrap_or_default();
        Some(sound_variant.hurt_sound(is_baby))
    }
}

impl AgeableMob for PigEntity {
    fn get_ageable_data(&self) -> &crate::entity::ageable::AgeableData {
        &self.ageable_data
    }
}

impl Animal for PigEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_PIG_FOOD)
    }

    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        partner: &dyn Mob,
    ) -> Option<Arc<dyn EntityBase>> {
        let baby = create_offspring(&self.mob_entity, &EntityType::PIG, world);
        if let Some(child) = baby.cast_any().downcast_ref::<Self>()
            && let Some(partner) = partner.cast_any().downcast_ref::<Self>()
        {
            let parent = if self.get_random().random::<bool>() {
                self
            } else {
                partner
            };
            child.set_variant(
                PigVariant::from_id(parent.variant.load(Ordering::Relaxed)).unwrap_or_default(),
            );
        }
        Some(baby)
    }

    fn play_eating_sound(&self) {
        let sound_variant = PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
            .unwrap_or_default();
        self.mob_entity
            .living_entity
            .make_sound(sound_variant.eat_sound(self.is_baby()));
    }
}

impl Mob for PigEntity {
    fn mob_controlling_passenger(&self) -> Option<std::sync::Arc<dyn crate::entity::EntityBase>> {
        crate::entity::mob::item_steering_player_controller(
            self.get_entity(),
            self.is_saddled(),
            &pumpkin_data::item::Item::CARROT_ON_A_STICK,
        )
        .or_else(|| {
            crate::entity::mob::mob_rider_controller(
                self.get_entity(),
                self.get_mob_entity().is_no_ai(),
            )
        })
    }

    fn travel(&self, _caller: &dyn crate::entity::EntityBase) -> bool {
        crate::entity::mob::player_steers(self)
    }

    /// Vanilla `Pig.finalizeSpawn` picks the variant for the biome and a random sound variant.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        let biome = world.get_biome(&self.get_entity().block_pos.load());
        self.set_variant(PigVariant::select_for_biome(biome.registry_id));
        let sound_variants = PigSoundVariant::ALL;
        self.set_sound_variant(
            sound_variants[self.get_random().random_range(0..sound_variants.len())],
        );
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    /// Vanilla `Pig.thunderHit`: outside peaceful the pig becomes a persistent zombified piglin
    /// (`ConversionType.SINGLE`, equipment not kept) and takes no lightning damage itself.
    fn mob_on_lightning_strike(
        &self,
        caller: &dyn EntityBase,
        lightning: &crate::entity::lightning::LightningBoltEntity,
    ) {
        let entity = self.get_entity();
        let world = entity.world.load_full();
        if world.level_info.load().difficulty == pumpkin_util::Difficulty::Peaceful
            || entity.is_removed()
        {
            self.mob_entity
                .living_entity
                .on_lightning_strike(caller, lightning);
            return;
        }
        let zombified = crate::entity::r#type::from_type(
            &EntityType::ZOMBIFIED_PIGLIN,
            entity.pos.load(),
            &world,
            uuid::Uuid::new_v4(),
        );
        let to = zombified.get_entity();
        to.set_rotation(entity.yaw.load(), entity.pitch.load());
        to.head_yaw.store(entity.head_yaw.load());
        to.body_yaw.store(entity.body_yaw.load());
        to.velocity.store(entity.velocity.load());
        to.on_ground
            .store(entity.on_ground.load(Ordering::Relaxed), Ordering::Relaxed);
        if let Some(custom_name) = &**entity.custom_name.load() {
            to.set_custom_name(custom_name.clone());
        }
        to.set_custom_name_visible(entity.custom_name_visible.load(Ordering::Relaxed));
        to.set_silent(entity.is_silent());
        to.set_has_no_gravity(entity.has_no_gravity());
        if let Some(mob) = zombified.get_mob() {
            let mob_entity = mob.get_mob_entity();
            mob_entity.set_no_ai(self.mob_entity.is_no_ai());
            mob_entity
                .persistence_required
                .store(true, Ordering::Relaxed);
            if self.is_baby() {
                mob.spawn_as_baby();
            }
        }
        world.spawn_entity(zombified);
        entity.remove();
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(
            PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
                .unwrap_or_default()
                .step_sound(self.is_baby()),
            0.15,
        )
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        let sound_variant = PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
            .unwrap_or_default();
        Some(sound_variant.ambient_sound(self.is_baby()))
    }

    fn has_audited_tick(&self) -> bool {
        true
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_custom_sound(&self) -> Option<&dyn CustomSound> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = self.is_baby();
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::pig::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::pig::DATA_VARIANT_ID,
            VarInt(self.variant.load(Ordering::Relaxed) as i32),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::pig::DATA_SOUND_VARIANT_ID,
            VarInt(self.sound_variant.load(Ordering::Relaxed) as i32),
        );
    }

    fn mob_set_variant_name(&self, name: &str) {
        if let Some(v) = PigVariant::from_name(name) {
            self.set_variant(v);
        }
    }

    fn mob_set_sound_variant_name(&self, name: &str) {
        if let Some(v) = PigSoundVariant::from_name(name) {
            self.set_sound_variant(v);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        crate::entity::write_saddle_equipment(nbt, self.is_saddled());
        let variant = PigVariant::from_id(self.variant.load(Ordering::Relaxed)).unwrap_or_default();
        nbt.put_string("variant", format!("minecraft:{}", variant.to_name()));
        let sound_variant = PigSoundVariant::from_id(self.sound_variant.load(Ordering::Relaxed))
            .unwrap_or_default();
        nbt.put_string(
            "sound_variant",
            format!("minecraft:{}", sound_variant.to_name()),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(saddled) = crate::entity::saved_saddle(nbt) {
            self.set_saddled(saddled);
        }
        if let Some(variant_str) = nbt.get_string("variant")
            && let Some(variant) = PigVariant::from_name(variant_str)
        {
            self.variant.store(variant.id(), Ordering::Relaxed);
        }
        if let Some(sound_str) = nbt.get_string("sound_variant")
            && let Some(sound_variant) = PigSoundVariant::from_name(sound_str)
        {
            self.sound_variant
                .store(sound_variant as u8, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_item_steerable(&self) -> Option<&dyn ItemSteerable> {
        Some(self)
    }

    fn is_saddled(&self) -> bool {
        self.saddled.load(Ordering::Relaxed)
    }

    fn can_be_saddled(&self) -> bool {
        self.mob_entity.living_entity.entity.is_alive() && !self.is_baby()
    }

    fn set_saddled(&self, saddled: bool) {
        self.saddled.store(saddled, Ordering::Relaxed);
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        // Pig.mobInteract: riding first, then feeding, then putting on a saddle.
        if !self.is_food(item_stack)
            && self.is_saddled()
            && !self.get_entity().has_passengers()
            && !player.get_entity().is_sneaking()
        {
            let world = player.world();
            if let Some(vehicle) = world.get_entity_by_id(self.get_entity().entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                self.get_entity()
                    .add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
            }
            return true;
        }

        if self.animal_interact(player, item_stack) {
            return true;
        }

        if item_stack.get_item() == &Item::SADDLE && self.can_be_saddled() && !self.is_saddled() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.play_sound(
                Sound::EntityPigSaddle,
                pumpkin_data::sound::SoundCategory::Neutral,
                &pos,
            );
            return true;
        }
        false
    }
}

impl ItemSteerable for PigEntity {
    fn boost(&self) -> bool {
        self.steering.boost()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
