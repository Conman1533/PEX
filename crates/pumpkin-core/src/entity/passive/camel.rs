use crate::entity::passive::horse::HORSE_AMBIENT_SOUND_INTERVAL;
use crate::entity::passive::horse::can_parent;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        breed::BreedGoal, escape_danger::EscapeDangerGoal, follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

pub const FLAG_TAME: u8 = 2;
pub const FLAG_SADDLE: u8 = 4;
pub const FLAG_BRED: u8 = 8;
pub const FLAG_EATING: u8 = 16;
pub const FLAG_STANDING: u8 = 32;

/// The camel and the camel husk (vanilla `CamelHusk extends Camel`).
pub struct CamelEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub flags: AtomicU8,
    pub dashing: AtomicBool,
    /// A camel husk: never a baby, no breeding, its own food and sounds, and it despawns.
    pub husk: bool,
}

impl CamelEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, false)
    }

    pub fn new_husk(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, true)
    }

    fn with_kind(entity: Entity, husk: bool) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let camel = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            flags: AtomicU8::new(0),
            dashing: AtomicBool::new(false),
            husk,
        };
        let mob_arc = Arc::new(camel);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(2.0));
            if !husk {
                goal_selector.add_goal(2, BreedGoal::new(1.0));
            }
            goal_selector.add_goal(
                3,
                Box::new(TemptGoal::new(
                    1.25,
                    |mob, stack| mob.as_animal().is_some_and(|camel| camel.is_food(stack)),
                    false,
                )),
            );
            if !husk {
                goal_selector.add_goal(4, Box::new(FollowParentGoal::new(1.0)));
            }
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::new(0.7)));
            goal_selector.add_goal(
                7,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn has_flag(&self, flag: u8) -> bool {
        (self.flags.load(Ordering::Relaxed) & flag) != 0
    }

    pub fn set_flag(&self, flag: u8, val: bool) {
        let current = self.flags.load(Ordering::Relaxed);
        let new_flags = if val { current | flag } else { current & !flag };
        self.flags.store(new_flags, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::camel::DATA_ID_FLAGS,
            new_flags as i8,
        );
    }

    #[must_use]
    pub fn is_saddled(&self) -> bool {
        self.has_flag(FLAG_SADDLE)
    }

    pub fn set_saddled(&self, val: bool) {
        self.set_flag(FLAG_SADDLE, val);
    }

    #[must_use]
    pub fn is_dashing(&self) -> bool {
        self.dashing.load(Ordering::Relaxed)
    }

    pub fn set_dashing(&self, dashing: bool) {
        self.dashing.store(dashing, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::camel::DASH, dashing);
    }
}

impl AgeableMob for CamelEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    /// Vanilla `CamelHusk.canBeABaby`.
    fn can_be_a_baby(&self) -> bool {
        !self.husk
    }
}

impl Animal for CamelEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(if self.husk {
            &tag::Item::MINECRAFT_CAMEL_HUSK_FOOD
        } else {
            &tag::Item::MINECRAFT_CAMEL_FOOD
        })
    }

    /// Vanilla `Camel.canMate`. Camels always count as tamed; camel husks never mate.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        !self.husk
            && partner.get_entity().entity_id != self.get_entity().entity_id
            && partner
                .cast_any()
                .downcast_ref::<Self>()
                .is_some_and(|other| !other.husk)
            && can_parent(self, true)
            && can_parent(partner, true)
    }
}

impl Mob for CamelEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.husk {
            Sound::EntityCamelHuskAmbient
        } else {
            Sound::EntityCamelAmbient
        })
    }

    /// Vanilla `CamelHusk.removeWhenFarAway`.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        self.husk
    }

    fn spawn_as_baby(&self) -> bool {
        !self.husk && {
            self.set_baby(true);
            true
        }
    }

    /// Vanilla `AbstractHorse.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        HORSE_AMBIENT_SOUND_INTERVAL
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("Saddle", self.is_saddled());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(saddle) = nbt.get_bool("Saddle") {
            self.set_saddled(saddle);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::camel::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::camel::DATA_ID_FLAGS,
            self.flags.load(Ordering::Relaxed) as i8,
        );
        entity.set_synced_data(pumpkin_data::tracked_data::camel::DASH, self.is_dashing());
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.husk {
            // Vanilla `CamelHusk.interact`.
            self.mob_entity
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
        let item = item_stack.get_item();

        if item == &Item::SADDLE && !self.is_saddled() && !self.is_baby() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                if self.husk {
                    Sound::EntityCamelHuskSaddle
                } else {
                    Sound::EntityCamelSaddle
                },
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            return true;
        }

        if self.is_saddled() && !self.is_baby() && !self.is_food(item_stack) {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack)
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_util::math::vector3::Vector3;

    use super::CamelEntity;
    use crate::entity::ageable::AgeableMob;
    use crate::entity::mob::Mob;
    use crate::entity::passive::animal::Animal;
    use crate::entity::r#type::from_type;
    use crate::world::test_world;

    #[tokio::test]
    async fn camel_husk_is_a_camel_that_eats_rabbit_feet_and_never_breeds() {
        let world = test_world();
        let spawn = |entity_type| {
            from_type(
                entity_type,
                Vector3::new(0.5, 64.0, 0.5),
                &world,
                uuid::Uuid::new_v4(),
            )
        };
        let husk = spawn(&EntityType::CAMEL_HUSK);
        let other_husk = spawn(&EntityType::CAMEL_HUSK);
        let camel = spawn(&EntityType::CAMEL);
        let husk_camel = husk
            .cast_any()
            .downcast_ref::<CamelEntity>()
            .expect("a camel entity");
        let camel_entity = camel
            .cast_any()
            .downcast_ref::<CamelEntity>()
            .expect("a camel entity");
        assert!(husk_camel.husk && !camel_entity.husk);

        let rabbit_foot = ItemStack::new(1, &Item::RABBIT_FOOT);
        let cactus = ItemStack::new(1, &Item::CACTUS);
        assert!(husk_camel.is_food(&rabbit_foot) && !husk_camel.is_food(&cactus));
        assert!(camel_entity.is_food(&cactus) && !camel_entity.is_food(&rabbit_foot));

        assert!(!husk_camel.can_be_a_baby() && !husk_camel.spawn_as_baby());
        assert!(husk_camel.remove_when_far_away(0.0));
        assert!(!camel_entity.remove_when_far_away(0.0));
        let other = other_husk.get_mob().expect("a mob");
        assert!(!husk_camel.can_mate(other));
    }
}
