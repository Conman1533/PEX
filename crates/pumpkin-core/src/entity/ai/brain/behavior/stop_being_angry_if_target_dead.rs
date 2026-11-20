use std::sync::Arc;

use pumpkin_data::entity::EntityType;

use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use crate::entity::EntityBase;

/// Vanilla `StopBeingAngryIfTargetDead`: forgets `angry_at` once the target is dead or dying,
/// unless it is a player and `forgiveDeadPlayers` is off. Always succeeds.
#[must_use]
pub fn create() -> OneShot {
    OneShot::new(
        "StopBeingAngryIfTargetDead",
        vec![(types::ANGRY_AT.id(), MemoryStatus::ValuePresent)],
        |tick| {
            let Some(uuid) = tick.state.get(types::ANGRY_AT).copied() else {
                return false;
            };
            let target = tick.world.get_entity_by_uuid(uuid).or_else(|| {
                tick.world
                    .get_player_by_uuid(uuid)
                    .map(|player| player as Arc<dyn EntityBase>)
            });
            let forgive = target.is_some_and(|target| {
                target
                    .get_living_entity()
                    .is_some_and(crate::entity::living::LivingEntity::is_dead_or_dying)
                    && (target.get_entity().entity_type != &EntityType::PLAYER
                        || tick.world.level_info.load().game_rules.forgive_dead_players)
            });
            if forgive {
                tick.state.erase(types::ANGRY_AT.id());
            }
            true
        },
    )
}
