use std::sync::{Arc, Weak};

use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;

use super::{
    abstract_fish::{AbstractFish, FishData},
    abstract_schooling_fish::{AbstractSchoolingFish, SchoolingFishData},
    water_animal::{AMBIENT_SOUND_INTERVAL, WaterAnimal},
};
use crate::entity::{
    Entity, EntityBase,
    bucketable::Bucketable,
    mob::{Mob, MobEntity, StepSound},
};

/// Represents a Tropical Fish, a schooling fish of warm oceans.
///
/// Wiki: <https://minecraft.wiki/w/Tropical_Fish>
pub struct TropicalFishEntity {
    pub mob_entity: MobEntity,
    fish_data: FishData,
    schooling_data: SchoolingFishData,
}

impl TropicalFishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        super::abstract_fish::initialize_fish_movement(&mut mob_entity);
        let fish = Arc::new_cyclic(|weak: &Weak<Self>| Self {
            mob_entity,
            fish_data: FishData::default(),
            schooling_data: SchoolingFishData::new(weak.clone()),
        });
        super::abstract_schooling_fish::register_schooling_fish_goals(&fish.mob_entity);
        fish
    }
}

impl Mob for TropicalFishEntity {
    fn mob_step_sound(&self) -> StepSound {
        StepSound::Silent
    }

    fn swim_sound(&self) -> Sound {
        Sound::EntityFishSwim
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntityTropicalFishAmbient)
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn as_schooling_fish(&self) -> Option<&dyn AbstractSchoolingFish> {
        Some(self)
    }

    fn post_tick(&self) {
        self.schooling_tick();
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<crate::world::World>,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        self.finalize_school_spawn(group_data)
    }

    fn before_mob_ai_step(&self) {
        self.fish_ai_step();
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        self.fish_travel_in_water(caller);
    }

    fn as_fish(&self) -> Option<&dyn AbstractFish> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.fish_write_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.fish_read_nbt(nbt);
    }

    fn mob_init_data_tracker(&self) {
        self.set_from_bucket(self.is_from_bucket());
    }

    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        self.fish_remove_when_far_away()
    }

    fn requires_custom_persistence(&self) -> bool {
        self.fish_requires_custom_persistence()
    }

    fn check_spawn_obstruction(&self, world: &crate::world::World) -> bool {
        self.water_animal_spawn_obstruction(world)
    }

    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }

    fn can_be_leashed(&self) -> bool {
        false
    }

    fn get_base_experience_reward(&self) -> u32 {
        self.water_animal_experience_reward()
    }

    fn base_tick(&self) {
        self.fish_base_tick(self);
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }
}

impl WaterAnimal for TropicalFishEntity {}

impl AbstractFish for TropicalFishEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntityTropicalFishFlop
    }

    fn fish_data(&self) -> &FishData {
        &self.fish_data
    }
}

impl AbstractSchoolingFish for TropicalFishEntity {
    fn schooling_data(&self) -> &SchoolingFishData {
        &self.schooling_data
    }
}

/// Keeps vanilla's `FromBucket` flag. Bucket pickup still goes through the water
/// bucket's own path, which keeps the variant, so `as_bucketable` stays unset.
impl Bucketable for TropicalFishEntity {
    fn is_from_bucket(&self) -> bool {
        self.fish_is_from_bucket()
    }

    fn set_from_bucket(&self, from_bucket: bool) {
        self.fish_set_from_bucket(from_bucket);
    }

    fn bucket_item(&self) -> &'static pumpkin_data::item::Item {
        &pumpkin_data::item::Item::TROPICAL_FISH_BUCKET
    }

    fn pickup_sound(&self) -> Sound {
        Sound::ItemBucketFillFish
    }
}
