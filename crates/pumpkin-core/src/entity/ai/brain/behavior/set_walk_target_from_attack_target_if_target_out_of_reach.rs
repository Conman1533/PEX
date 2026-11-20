use std::sync::Arc;

use super::super::memory::position_tracker::EntityTracker;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryStatus, types};
use super::{OneShot, utils};
use crate::entity::mob::Mob;

/// Vanilla `PROJECTILE_ATTACK_RANGE_BUFFER`.
const PROJECTILE_ATTACK_RANGE_BUFFER: i32 = 1;

/// Vanilla `SetWalkTargetFromAttackTargetIfTargetOutOfReach.create(speedModifier)`.
#[must_use]
pub fn create(speed_modifier: f32) -> OneShot {
    create_with(move |_| speed_modifier)
}

/// Walks toward `attack_target` while it is out of sight or out of attack range, and stops
/// walking once it is within range. Always succeeds.
#[must_use]
pub fn create_with(speed_modifier: impl Fn(&dyn Mob) -> f32 + Send + Sync + 'static) -> OneShot {
    OneShot::new(
        "SetWalkTargetFromAttackTargetIfTargetOutOfReach",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::Registered),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            if utils::entity_is_visible(&tick.visibility(), target.as_ref())
                && utils::is_within_attack_range(
                    tick.mob,
                    target.as_ref(),
                    PROJECTILE_ATTACK_RANGE_BUFFER,
                )
            {
                tick.state.erase(types::WALK_TARGET.id());
            } else {
                utils::look_at_entity(tick.state, Arc::clone(&target));
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::new(
                        Arc::new(EntityTracker::new(target, false)),
                        speed_modifier(tick.mob),
                        0,
                    ),
                );
            }
            true
        },
    )
}
