use crate::entity::effect::MobEffect;
use crate::entity::living::LivingEntity;

pub struct HungerMobEffect;

/// Vanilla `HungerMobEffect.applyEffectTick`: a little exhaustion every tick, not a burst every
/// second (a burst leaves exhaustion behind that keeps draining food after the effect ends).
fn exhaustion_per_tick(amplifier: u8) -> f32 {
    0.005 * (f32::from(amplifier) + 1.0)
}

impl MobEffect for HungerMobEffect {
    fn should_apply_effect_tick(&self, duration: i32, _amplifier: u8) -> bool {
        duration > 0
    }

    fn apply_effect_tick(&self, living: &LivingEntity, amplifier: u8) {
        let world = living.entity.world.load();
        if let Some(entity) = world.get_entity_by_id(living.entity.entity_id)
            && let Some(player) = entity.get_player()
        {
            // Through the player like vanilla `causeFoodExhaustion`: skipped while invulnerable.
            player.add_exhaustion(exhaustion_per_tick(amplifier));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunger_applies_every_tick_like_vanilla() {
        assert!((1..=40).all(|duration| HungerMobEffect.should_apply_effect_tick(duration, 0)));
        assert!(!HungerMobEffect.should_apply_effect_tick(0, 0));
    }

    #[test]
    fn hunger_exhaustion_matches_vanilla() {
        assert!((exhaustion_per_tick(0) - 0.005).abs() < f32::EPSILON);
        assert!((exhaustion_per_tick(100) - 0.505).abs() < 1e-6);
    }
}
