use pumpkin_data::data_component_impl::{EquipmentSlot, StatusEffectInstance};
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use std::sync::Arc;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, skeleton::SkeletonEntityBase},
};

pub struct ParchedSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl ParchedSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::with_attack_intervals(
            entity,
            SkeletonEntityBase::INCREASED_HARD_ATTACK_INTERVAL,
            SkeletonEntityBase::INCREASED_NORMAL_ATTACK_INTERVAL,
        );
        let parched = Self { entity };
        Arc::new(parched)
    }
}

impl Mob for ParchedSkeletonEntity {
    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(pumpkin_data::sound::Sound::EntityParchedStep, 0.15)
    }

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

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntityParchedAmbient)
    }

    /// Vanilla `Parched.getArrow`: every arrow carries 30 seconds of weakness.
    fn arrow_effect(&self) -> Option<StatusEffectInstance> {
        Some(super::arrow_effect_instance("minecraft:weakness", 600))
    }
}
