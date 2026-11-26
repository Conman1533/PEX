use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
    passive::squid::{SquidMovement, register_squid_goals, spawn_ink},
};
use pumpkin_data::damage::DamageType;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data::glow_squid::DATA_DARK_TICKS_REMAINING;
use pumpkin_nbt::compound::NbtCompound;

/// How long a hurt glow squid stays dark, in ticks.
const DARK_TICKS_ON_HURT: i32 = 100;

/// Represents a Glow Squid, a passive aquatic mob that emits a glowing particle effect.
///
/// Wiki: <https://minecraft.wiki/w/Glow_Squid>
pub struct GlowSquidEntity {
    pub mob_entity: MobEntity,
    pub movement: Arc<SquidMovement>,
    dark_ticks_remaining: AtomicI32,
}

impl GlowSquidEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let movement = Arc::new(SquidMovement::new());
        register_squid_goals(&mob_entity, &movement);
        Arc::new(Self {
            mob_entity,
            movement,
            dark_ticks_remaining: AtomicI32::new(0),
        })
    }

    #[must_use]
    pub fn dark_ticks_remaining(&self) -> i32 {
        self.dark_ticks_remaining.load(Ordering::Relaxed)
    }

    /// Vanilla `GlowSquid.setDarkTicks`: the client dims the squid while this runs.
    fn set_dark_ticks(&self, ticks: i32) {
        if self.dark_ticks_remaining.swap(ticks, Ordering::Relaxed) != ticks {
            self.mob_entity
                .living_entity
                .entity
                .set_synced_data(DATA_DARK_TICKS_REMAINING, ticks);
        }
    }
}

impl Mob for GlowSquidEntity {
    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::Events
    }

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

    /// Vanilla `GlowSquid.aiStep`. Its glow particles go through `Level.addParticle`,
    /// which does nothing on the server.
    fn mob_ai_step(&self) {
        self.movement.tick(self);
        let dark_ticks = self.dark_ticks_remaining();
        if dark_ticks > 0 {
            self.set_dark_ticks(dark_ticks - 1);
        }
    }

    fn travel(&self, caller: &dyn EntityBase) -> bool {
        SquidMovement::travel(self, caller)
    }

    /// Vanilla `GlowSquid.hurtServer`: any hurt turns it dark, and `Squid.hurtServer`
    /// inks with the glow squid's squirt and ink.
    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        self.set_dark_ticks(DARK_TICKS_ON_HURT);
        if source.is_some() {
            spawn_ink(self, Sound::EntityGlowSquidSquirt, Particle::GlowSquidInk);
        }
    }

    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("DarkTicksRemaining", self.dark_ticks_remaining());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_dark_ticks(nbt.get_int("DarkTicksRemaining").unwrap_or(0));
    }
}
