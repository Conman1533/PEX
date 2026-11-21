use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use crate::entity::EntityBase;

/// Vanilla `TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE`.
const TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE: i64 = 200;

/// Vanilla `StopAttackingIfTargetInvalid.create()`.
#[must_use]
pub fn create() -> OneShot {
    create_with(|_, _| false, |_, _| {}, true)
}

/// Vanilla `StopAttackingIfTargetInvalid.create(stopAttackingWhen)`.
#[must_use]
pub fn create_when(
    stop_attacking_when: impl Fn(&BrainTick<'_>, &Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
) -> OneShot {
    create_with(stop_attacking_when, |_, _| {}, true)
}

/// Vanilla `StopAttackingIfTargetInvalid.create(stopAttackingWhen, onTargetErased,
/// canGrowTiredOfTryingToReachTarget)`.
///
/// Forgets `attack_target` once the mob can no longer
/// attack it, it is dead or in another world, the condition holds, or (when it can grow tired)
/// the mob has not been able to reach it for 10 seconds. Always succeeds.
#[must_use]
pub fn create_with(
    stop_attacking_when: impl Fn(&BrainTick<'_>, &Arc<dyn EntityBase>) -> bool + Send + Sync + 'static,
    mut on_target_erased: impl FnMut(&mut BrainTick<'_>, &Arc<dyn EntityBase>) + Send + Sync + 'static,
    can_grow_tired_of_trying_to_reach_target: bool,
) -> OneShot {
    OneShot::new(
        "StopAttackingIfTargetInvalid",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (
                types::CANT_REACH_WALK_TARGET_SINCE.id(),
                MemoryStatus::Registered,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            let keep = tick.mob.can_attack(target.as_ref())
                && (!can_grow_tired_of_trying_to_reach_target
                    || !is_tired_of_trying_to_reach_target(tick))
                && is_alive(target.as_ref())
                && Arc::ptr_eq(&*target.get_entity().world.load(), tick.world)
                && !stop_attacking_when(tick, &target);
            if !keep {
                on_target_erased(tick, &target);
                tick.state.erase(types::ATTACK_TARGET.id());
            }
            true
        },
    )
}

/// Vanilla `LivingEntity.isAlive`: not removed and health above zero.
fn is_alive(target: &dyn EntityBase) -> bool {
    target.get_entity().is_alive()
        && target
            .get_living_entity()
            .is_none_or(|living| !living.is_dead_or_dying())
}

fn is_tired_of_trying_to_reach_target(tick: &BrainTick<'_>) -> bool {
    tick.state
        .get(types::CANT_REACH_WALK_TARGET_SINCE)
        .is_some_and(|since| tick.time - since > TIMEOUT_TO_GET_WITHIN_ATTACK_RANGE)
}
