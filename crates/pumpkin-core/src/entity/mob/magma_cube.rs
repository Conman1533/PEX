use pumpkin_data::attributes::Attributes;
use pumpkin_data::sound::Sound;

use crate::entity::mob::cube_mob::{ContactDamage, CubeKind, CubeSounds, SizedSound};
use crate::entity::mob::slime::SLIME;

pub const MAGMA_CUBE: CubeKind = CubeKind {
    sounds: CubeSounds {
        jump: SizedSound {
            normal: Sound::EntityMagmaCubeJump,
            small: Sound::EntityMagmaCubeJump,
        },
        squish: SizedSound {
            normal: Sound::EntityMagmaCubeSquish,
            small: Sound::EntityMagmaCubeSquishSmall,
        },
        hurt: SizedSound {
            normal: Sound::EntityMagmaCubeHurt,
            small: Sound::EntityMagmaCubeHurtSmall,
        },
        death: SizedSound {
            normal: Sound::EntityMagmaCubeDeath,
            small: Sound::EntityMagmaCubeDeathSmall,
        },
    },
    size_attributes: &[
        (Attributes::ATTACK_DAMAGE, |size| f64::from(size)),
        (Attributes::ARMOR, |size| f64::from(size * 3)),
    ],
    jump_delay_multiplier: 4,
    contact_damage: ContactDamage::Always,
    // `MagmaCube.getAttackDamage` adds 2 to the attribute.
    attack_damage_bonus: 2.0,
    squish_decay: 0.9,
    jump_boost_per_size: 0.1,
    lava_jump: Some((0.22, 0.05)),
    ..SLIME
};
