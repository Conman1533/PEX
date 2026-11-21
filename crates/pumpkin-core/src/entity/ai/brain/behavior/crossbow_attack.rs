use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::item::Item;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::Hand;
use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::{Behavior, utils};
use crate::entity::ai::goal::ranged_crossbow_attack::RangedCrossbowAttackGoal;
use crate::entity::mob::Mob;

/// Vanilla `CrossbowAttack.TIMEOUT`.
const TIMEOUT: i32 = 1200;
/// Vanilla `CrossbowItem.getChargeDuration` without Quick Charge, in ticks.
const CHARGE_DURATION: i32 = 25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CrossbowState {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

/// Vanilla `CrossbowAttack`: a mob holding a crossbow that sees its attack target within range
/// loads, waits 20 to 39 ticks and shoots, over and over.
pub struct CrossbowAttack {
    state: CrossbowState,
    attack_delay: i32,
    charge_ticks: i32,
}

impl Default for CrossbowAttack {
    fn default() -> Self {
        Self {
            state: CrossbowState::Uncharged,
            attack_delay: 0,
            charge_ticks: 0,
        }
    }
}

fn crossbow_hand(mob: &dyn Mob) -> Option<Hand> {
    let equipment = mob
        .get_mob_entity()
        .living_entity
        .entity_equipment
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if equipment.get(&EquipmentSlot::MAIN_HAND).item.id == Item::CROSSBOW.id {
        Some(Hand::Right)
    } else if equipment.get(&EquipmentSlot::OFF_HAND).item.id == Item::CROSSBOW.id {
        Some(Hand::Left)
    } else {
        None
    }
}

fn is_using_item(mob: &dyn Mob) -> bool {
    mob.get_mob_entity()
        .living_entity
        .active_hand
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

fn set_charging(mob: &dyn Mob, charging: bool) {
    if let Some(crossbow_mob) = mob.as_crossbow_attack_mob() {
        crossbow_mob.set_charging_crossbow(charging);
    }
}

fn play(mob: &dyn Mob, sound: Sound) {
    let entity = mob.get_entity();
    entity
        .world
        .load()
        .play_sound(sound, SoundCategory::Hostile, &entity.pos.load());
}

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::LOOK_TARGET.id(), MemoryStatus::Registered),
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
];

impl Behavior for CrossbowAttack {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        TIMEOUT
    }

    fn max_duration(&self) -> i32 {
        TIMEOUT
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return false;
        };
        crossbow_hand(tick.mob).is_some()
            && utils::can_see(&tick.visibility(), target.as_ref())
            && utils::is_within_attack_range(tick.mob, target.as_ref(), 0)
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::ATTACK_TARGET.id())
            && self.check_extra_start_conditions(tick)
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return;
        };
        utils::look_at_entity(tick.state, target.clone());
        let mob = tick.mob;
        let living = &mob.get_mob_entity().living_entity;
        match self.state {
            CrossbowState::Uncharged => {
                let Some(hand) = crossbow_hand(mob) else {
                    return;
                };
                let slot = if hand == Hand::Right {
                    EquipmentSlot::MAIN_HAND
                } else {
                    EquipmentSlot::OFF_HAND
                };
                let stack = living
                    .entity_equipment
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&slot);
                living.set_active_hand(hand, stack, i32::MAX);
                self.state = CrossbowState::Charging;
                self.charge_ticks = 0;
                set_charging(mob, true);
                play(mob, Sound::ItemCrossbowLoadingStart);
            }
            CrossbowState::Charging => {
                if !is_using_item(mob) {
                    self.state = CrossbowState::Uncharged;
                    return;
                }
                self.charge_ticks += 1;
                if self.charge_ticks == 10 {
                    play(mob, Sound::ItemCrossbowLoadingMiddle);
                }
                if self.charge_ticks >= CHARGE_DURATION {
                    living.clear_active_hand();
                    self.state = CrossbowState::Charged;
                    self.attack_delay = 20 + mob.get_random().random_range(0..20);
                    set_charging(mob, false);
                    play(mob, Sound::ItemCrossbowLoadingEnd);
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                RangedCrossbowAttackGoal::shoot(mob, &target);
                self.state = CrossbowState::Uncharged;
            }
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        let living = &tick.mob.get_mob_entity().living_entity;
        if is_using_item(tick.mob) {
            living.clear_active_hand();
        }
        if crossbow_hand(tick.mob).is_some() {
            set_charging(tick.mob, false);
        }
    }

    fn debug_name(&self) -> &'static str {
        "CrossbowAttack"
    }
}
