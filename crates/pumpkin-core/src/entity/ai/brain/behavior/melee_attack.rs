use std::sync::Arc;

use super::super::BrainTick;
use super::super::memory::{MemoryStatus, types};
use super::{OneShot, utils};
use crate::entity::ai::goal::melee_attack::MeleeAttackGoal;

/// Vanilla `MeleeAttack.create(cooldownBetweenAttacks)`.
#[must_use]
pub fn create(cooldown_between_attacks: i32) -> OneShot {
    create_with(|_| true, cooldown_between_attacks)
}

/// Vanilla `MeleeAttack.create(canAttackPredicate, cooldownBetweenAttacks)`.
///
/// Hits `attack_target` once it is visible and in melee range, unless the mob holds a ranged weapon it
/// can use, then sets `attack_cooling_down` for the cooldown.
#[must_use]
pub fn create_with(
    can_attack: impl Fn(&BrainTick<'_>) -> bool + Send + Sync + 'static,
    cooldown_between_attacks: i32,
) -> OneShot {
    OneShot::new(
        "MeleeAttack",
        vec![
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (types::ATTACK_COOLING_DOWN.id(), MemoryStatus::ValueAbsent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            if !can_attack(tick)
                || utils::is_holding_usable_non_melee_weapon(tick.mob)
                || !tick
                    .mob
                    .get_mob_entity()
                    .is_in_attack_range(target.as_ref())
                || !utils::entity_is_visible(&tick.visibility(), target.as_ref())
            {
                return false;
            }
            utils::look_at_entity(tick.state, Arc::clone(&target));
            tick.mob.get_mob_entity().living_entity.swing_hand();
            MeleeAttackGoal::attack(tick.mob, target.as_ref());
            tick.state.set_with_expiry(
                types::ATTACK_COOLING_DOWN,
                true,
                i64::from(cooldown_between_attacks),
            );
            true
        },
    )
}
