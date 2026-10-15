use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_nbt::compound::NbtCompound;
use std::sync::Arc;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, skeleton::SkeletonEntityBase},
};

pub struct WitherSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl WitherSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::build(
            entity,
            SkeletonEntityBase::HARD_ATTACK_INTERVAL,
            SkeletonEntityBase::NORMAL_ATTACK_INTERVAL,
            // `AbstractPiglin`
            &[&EntityType::PIGLIN, &EntityType::PIGLIN_BRUTE],
        );
        let skeleton = Self { entity };
        Arc::new(skeleton)
    }
}

impl Mob for WitherSkeletonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.entity.mob_read_nbt(nbt);
    }

    fn mob_tick(&self, caller: &dyn EntityBase) {
        self.entity.mob_tick(caller);
    }

    fn on_equip_item(&self, slot: &EquipmentSlot) {
        self.entity.on_equip_item(slot);
    }
}
