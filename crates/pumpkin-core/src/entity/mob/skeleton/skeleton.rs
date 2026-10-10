use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_nbt::compound::NbtCompound;
use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::{Mob, MobEntity, equipment::RegionalDifficulty, skeleton::SkeletonEntityBase},
};
use crate::world::World;

pub struct SkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl SkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity);
        let skeleton = Self { entity };
        Arc::new(skeleton)
    }
}

impl Mob for SkeletonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.entity.mob_read_nbt(nbt);
    }

    fn on_equip_item(&self, slot: &EquipmentSlot) {
        self.entity.on_equip_item(slot);
    }

    fn populate_default_equipment_slots(
        &self,
        world: &Arc<World>,
        difficulty: &RegionalDifficulty,
    ) {
        self.entity
            .populate_default_equipment_slots(world, difficulty);
    }

    fn populate_default_equipment_enchantments(&self, difficulty: &RegionalDifficulty) {
        self.entity
            .populate_default_equipment_enchantments(difficulty);
    }
}
