use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use crate::entity::EntityBase;

/// Vanilla `StartAttacking`: remembers the target the finder picks as `attack_target`, if the mob
/// can attack it.
#[must_use]
pub fn create(
    target_finder: impl Fn(&BrainTick<'_>) -> Option<Arc<dyn EntityBase>> + Send + Sync + 'static,
) -> OneShot {
    create_with(|_| true, target_finder)
}

/// Vanilla `StartAttacking.create(canAttackPredicate, targetFinder)`.
#[must_use]
pub fn create_with(
    can_attack: impl Fn(&BrainTick<'_>) -> bool + Send + Sync + 'static,
    target_finder: impl Fn(&BrainTick<'_>) -> Option<Arc<dyn EntityBase>> + Send + Sync + 'static,
) -> OneShot {
    OneShot::new(
        "StartAttacking",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValueAbsent),
            (
                types::CANT_REACH_WALK_TARGET_SINCE.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            if !can_attack(tick) {
                return false;
            }
            let Some(target) = target_finder(tick) else {
                return false;
            };
            if !tick.mob.can_attack(target.as_ref()) {
                return false;
            }
            tick.state.set(types::ATTACK_TARGET, target);
            tick.state.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
            true
        },
    )
}
