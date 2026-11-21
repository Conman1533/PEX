use pumpkin_data::data_component_impl::{EquipmentSlot, KineticWeaponImpl};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::Hand;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::ai::goal::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::util::land_random_pos;
use crate::entity::mob::Mob;

/// Vanilla `SpearUseGoal.MAX_FLEEING_TIME`.
const MAX_FLEEING_TIME: i32 = to_goal_ticks(100);
/// Vanilla `MIN_COOLDOWN_DISTANCE` and `MAX_COOLDOWN_DISTANCE`: how far the mob backs off after
/// a charge.
const MIN_COOLDOWN_DISTANCE: f64 = 9.0;
const MAX_COOLDOWN_DISTANCE: f64 = 11.0;

/// Vanilla `SpearUseGoal.SpearUseState`.
struct SpearUseState {
    engage_time: i32,
    fleeing_time: i32,
    away_pos: Option<Vector3<f64>>,
    done: bool,
}

impl SpearUseState {
    const fn new() -> Self {
        Self {
            engage_time: -1,
            fleeing_time: -1,
            away_pos: None,
            done: false,
        }
    }

    const fn not_engaged_yet(&self) -> bool {
        self.engage_time < 0
    }

    const fn tick_and_check_engagement(&mut self) -> bool {
        if self.engage_time > 0 {
            self.engage_time -= 1;
            if self.engage_time == 0 {
                return true;
            }
        }
        false
    }

    const fn tick_and_check_fleeing(&mut self) -> bool {
        if self.fleeing_time > 0 {
            self.fleeing_time += 1;
            if self.fleeing_time > MAX_FLEEING_TIME {
                self.done = true;
                return true;
            }
        }
        false
    }
}

/// Vanilla `SpearUseGoal`: a mob holding a kinetic weapon walks up to its target, lowers the
/// spear, charges through, backs off and charges again.
pub struct SpearUseGoal {
    state: Option<SpearUseState>,
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f64,
    target_in_range_radius_sq: f64,
}

impl SpearUseGoal {
    #[must_use]
    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        approach_distance: f32,
        target_in_range_radius: f32,
    ) -> Box<Self> {
        Box::new(Self {
            state: None,
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            approach_distance_sq: f64::from(approach_distance * approach_distance),
            target_in_range_radius_sq: f64::from(target_in_range_radius * target_in_range_radius),
        })
    }

    fn main_hand_item(mob: &dyn Mob) -> ItemStack {
        mob.get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND)
    }

    /// Vanilla `ableToAttack`: a target, and a kinetic weapon in the main hand.
    fn able_to_attack(mob: &dyn Mob) -> bool {
        mob.get_mob_entity().get_target().is_some()
            && Self::main_hand_item(mob)
                .get_data_component::<KineticWeaponImpl>()
                .is_some()
    }

    fn is_using_item(mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .living_entity
            .active_hand
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Vanilla `getKineticWeaponUseDuration`: `KineticWeapon.computeDamageUseDuration`, the
    /// delay plus the damage window, in goal ticks.
    fn kinetic_weapon_use_duration(stack: &ItemStack) -> i32 {
        let ticks = stack
            .get_data_component::<KineticWeaponImpl>()
            .map_or(0, |weapon| {
                weapon.delay_ticks
                    + weapon
                        .damage_conditions
                        .as_ref()
                        .map_or(0, |condition| condition.max_duration_ticks)
            });
        to_goal_ticks(ticks)
    }

    fn move_to(mob: &dyn Mob, destination: Vector3<f64>, speed: f64) {
        mob.get_mob_entity()
            .steered()
            .navigator()
            .set_progress(NavigatorGoal {
                current_progress: mob.get_entity().pos.load(),
                destination,
                speed,
                reach_range: 1,
            });
    }
}

impl Goal for SpearUseGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::able_to_attack(mob) && !Self::is_using_item(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.state.as_ref().is_some_and(|state| !state.done) && Self::able_to_attack(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().set_attacking(true);
        self.state = Some(SpearUseState::new());
    }

    fn stop(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
        mob.get_mob_entity().set_attacking(false);
        self.state = None;
        mob.get_mob_entity().living_entity.clear_active_hand();
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let target_pos = target.get_entity().pos.load();
        let target_dist_sqr = mob
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target_pos);
        // Vanilla scales by the mount's `chargeSpeedModifier`, 1 for every mount a spear user
        // rides here.
        let speed_modifier = 1.0;
        let mount_distance = if mob.get_entity().has_vehicle() {
            2.0
        } else {
            0.0
        };
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 30.0, 30.0);

        if state.not_engaged_yet() {
            if target_dist_sqr > self.approach_distance_sq {
                Self::move_to(
                    mob,
                    target_pos,
                    speed_modifier * self.speed_modifier_when_repositioning,
                );
                return;
            }
            let stack = Self::main_hand_item(mob);
            state.engage_time = Self::kinetic_weapon_use_duration(&stack);
            mob.get_mob_entity()
                .living_entity
                .set_active_hand(Hand::Right, stack, i32::MAX);
        }

        if state.tick_and_check_engagement() {
            mob.get_mob_entity().living_entity.clear_active_hand();
            let distance = target_dist_sqr.sqrt();
            state.away_pos = land_random_pos::get_pos_away_between(
                mob,
                (MIN_COOLDOWN_DISTANCE + mount_distance - distance).max(0.0),
                (MAX_COOLDOWN_DISTANCE + mount_distance - distance).max(1.0),
                7,
                target_pos,
            );
            state.fleeing_time = 1;
        }

        if !state.tick_and_check_fleeing() {
            if let Some(away_pos) = state.away_pos {
                Self::move_to(
                    mob,
                    away_pos,
                    speed_modifier * self.speed_modifier_when_repositioning,
                );
                if mob.is_navigator_idle() {
                    if state.fleeing_time > 0 {
                        state.done = true;
                        return;
                    }
                    state.away_pos = None;
                }
            } else {
                Self::move_to(
                    mob,
                    target_pos,
                    speed_modifier * self.speed_modifier_when_charging,
                );
                if target_dist_sqr < self.target_in_range_radius_sq || mob.is_navigator_idle() {
                    let distance = target_dist_sqr.sqrt();
                    state.away_pos = land_random_pos::get_pos_away_between(
                        mob,
                        6.0 + mount_distance - distance,
                        7.0 + mount_distance - distance,
                        7,
                        target_pos,
                    );
                }
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
