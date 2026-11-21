use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use crate::entity::EntityBase;

/// Vanilla `StartCelebratingIfTargetDead`.
///
/// Once the attack target is dead or dying, celebrates at its block (dancing too if `dance_predicate` says so) for `celebrate_duration` ticks, and drops
/// the target unless it is a player that isn't forgiven (`forgiveDeadPlayers`).
#[must_use]
pub fn create(
    celebrate_duration: i64,
    dance_predicate: impl Fn(&BrainTick<'_>, &Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
) -> OneShot {
    OneShot::new(
        "StartCelebratingIfTargetDead",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::ANGRY_AT.id(), MemoryStatus::Registered),
            (types::CELEBRATE_LOCATION.id(), MemoryStatus::ValueAbsent),
            (types::DANCING.id(), MemoryStatus::Registered),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            let dead = target
                .get_living_entity()
                .is_some_and(crate::entity::living::LivingEntity::is_dead_or_dying);
            if !dead {
                return false;
            }
            if dance_predicate(tick, &target) {
                tick.state
                    .set_with_expiry(types::DANCING, true, celebrate_duration);
            }
            let target_entity = target.get_entity();
            tick.state.set_with_expiry(
                types::CELEBRATE_LOCATION,
                target_entity.block_pos.load(),
                celebrate_duration,
            );
            if target_entity.entity_type != &EntityType::PLAYER
                || tick.world.level_info.load().game_rules.forgive_dead_players
            {
                tick.state.erase(types::ATTACK_TARGET.id());
                tick.state.erase(types::ANGRY_AT.id());
            }
            true
        },
    )
}
