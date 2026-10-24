use std::sync::Arc;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
    passive::squid::SquidMovement,
};

/// Represents a Glow Squid, a passive aquatic mob that emits a glowing particle effect.
///
/// Wiki: <https://minecraft.wiki/w/Glow_Squid>
pub struct GlowSquidEntity {
    pub mob_entity: MobEntity,
    pub movement: SquidMovement,
}

impl GlowSquidEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            movement: SquidMovement::new(),
        })
    }
}

impl Mob for GlowSquidEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityGlowSquidAmbient)
    }

    /// Vanilla `WaterAnimal.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        crate::entity::mob::ANIMAL_AMBIENT_SOUND_INTERVAL
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.movement.tick(self);
    }

    fn travel(&self, caller: &dyn EntityBase) -> bool {
        SquidMovement::travel(self, caller)
    }

    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }
}
