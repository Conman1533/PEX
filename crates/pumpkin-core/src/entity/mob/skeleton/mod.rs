use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::Difficulty;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Goal, active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
        bow_attack::BowAttackGoal, flee_sun::FleeSunGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal,
        restrict_sun::RestrictSunGoal, revenge::RevengeGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity, equipment::RegionalDifficulty},
    passive::turtle::baby_on_land_selector,
};
use crate::world::World;

pub mod bogged;
pub mod parched;
#[allow(clippy::module_inception)]
pub mod skeleton;
pub mod stray;
pub mod wither;

pub struct SkeletonEntityBase {
    pub mob_entity: MobEntity,
    hard_attack_interval: i32,
    attack_interval: i32,
    /// Vanilla runs `onEquipItem` on the server thread, but `/item replace` and `/loot replace`
    /// reach it while the AI tick holds the goal selectors, so the reassessment waits for the next
    /// tick.
    reassess_weapon_goal_queued: AtomicBool,
}

impl SkeletonEntityBase {
    pub const HARD_ATTACK_INTERVAL: i32 = 20;
    pub const NORMAL_ATTACK_INTERVAL: i32 = 40;
    pub const INCREASED_HARD_ATTACK_INTERVAL: i32 = 50;
    pub const INCREASED_NORMAL_ATTACK_INTERVAL: i32 = 70;

    pub fn new(entity: Entity) -> Arc<Self> {
        Self::build(
            entity,
            Self::HARD_ATTACK_INTERVAL,
            Self::NORMAL_ATTACK_INTERVAL,
            &[],
        )
    }

    /// For the variants that override `getHardAttackInterval` and `getAttackInterval`.
    pub fn with_attack_intervals(
        entity: Entity,
        hard_attack_interval: i32,
        attack_interval: i32,
    ) -> Arc<Self> {
        Self::build(entity, hard_attack_interval, attack_interval, &[])
    }

    /// `extra_targets` are registered at priority 3 before the shared targets, like
    /// `WitherSkeleton.registerGoals` does before calling super.
    pub fn build(
        entity: Entity,
        hard_attack_interval: i32,
        attack_interval: i32,
        extra_targets: &[&'static EntityType],
    ) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let mob = Self {
            mob_entity,
            hard_attack_interval,
            attack_interval,
            reassess_weapon_goal_queued: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(mob);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        {
            let mut goal_selector = mob_arc
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut target_selector = mob_arc
                .mob_entity
                .target_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            goal_selector.add_goal(2, Box::new(RestrictSunGoal::new()));
            goal_selector.add_goal(3, Box::new(FleeSunGoal::new(1.0)));
            goal_selector.add_goal(
                3,
                Box::new(AvoidEntityGoal::new(&EntityType::WOLF, 6.0, 1.0, 1.2)),
            );
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));

            for target_type in extra_targets {
                target_selector.add_goal(
                    3,
                    ActiveTargetGoal::with_default(&mob_arc.mob_entity, target_type, true),
                );
            }
            target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
            target_selector.add_goal(
                3,
                Box::new(ActiveTargetGoal::new(
                    &mob_arc.mob_entity,
                    &EntityType::TURTLE,
                    10,
                    true,
                    false,
                    Some(baby_on_land_selector),
                )),
            );
        };

        mob_arc.reassess_weapon_goal();
        mob_arc
    }

    /// Vanilla `AbstractSkeleton.reassessWeaponGoal`: a bow in either hand gets the bow goal,
    /// anything else the melee goal, both at priority 4.
    pub fn reassess_weapon_goal(&self) {
        let holding_bow = {
            let equipment = self
                .mob_entity
                .living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // `ProjectileUtil.getWeaponHoldingHand` falls back to the off hand.
            equipment.get(&EquipmentSlot::MAIN_HAND).item.id == Item::BOW.id
                || equipment.get(&EquipmentSlot::OFF_HAND).item.id == Item::BOW.id
        };
        let mut stopped = {
            let mut goal_selector = self
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut stopped = goal_selector.remove_goals::<MeleeAttackGoal>();
            stopped.extend(goal_selector.remove_goals::<BowAttackGoal>());
            if holding_bow {
                let difficulty = self
                    .mob_entity
                    .living_entity
                    .entity
                    .world
                    .load()
                    .level_info
                    .load()
                    .difficulty;
                let min_attack_interval = if difficulty == Difficulty::Hard {
                    self.hard_attack_interval
                } else {
                    self.attack_interval
                };
                goal_selector.add_goal(
                    4,
                    Box::new(BowAttackGoal::new(1.0, min_attack_interval, 15.0)),
                );
            } else {
                goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.2, false)));
            }
            stopped
        };
        for goal in &mut stopped {
            goal.stop(self);
        }
    }
}

impl Mob for SkeletonEntityBase {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_read_nbt(&self, _nbt: &NbtCompound) {
        self.reassess_weapon_goal();
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        if self
            .reassess_weapon_goal_queued
            .swap(false, Ordering::Relaxed)
        {
            self.reassess_weapon_goal();
        }
    }

    fn on_equip_item(&self, _slot: &EquipmentSlot) {
        self.reassess_weapon_goal_queued
            .store(true, Ordering::Relaxed);
    }

    fn populate_default_equipment_slots(
        &self,
        _world: &Arc<World>,
        difficulty: &RegionalDifficulty,
    ) {
        // Default armor slots (super.populateDefaultEquipmentSlots)
        if rand::random::<f32>()
            < MobEntity::MAX_WEARING_ARMOR_CHANCE * difficulty.special_multiplier
        {
            let mut armor_type = rand::random_range(0..3);
            for _ in 1..=3 {
                if rand::random::<f32>() < MobEntity::WEARING_ARMOR_UPGRADE_MATERIAL_CHANCE {
                    armor_type += 1;
                }
            }

            let partial_chance = if difficulty.base_difficulty == Difficulty::Hard {
                0.1f32
            } else {
                0.25f32
            };

            let living = &self.mob_entity.living_entity;
            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut first = true;

            for slot in &MobEntity::EQUIPMENT_POPULATION_ORDER {
                let current = equipment.get(slot);
                if !first && rand::random::<f32>() < partial_chance {
                    break;
                }
                first = false;
                if current.is_empty()
                    && let Some(item) = MobEntity::get_equipment_for_slot(slot, armor_type)
                {
                    equipment.put(slot, ItemStack::new(1, item));
                }
            }
        }

        // AbstractSkeleton sets BOW on MAIN_HAND
        let living = &self.mob_entity.living_entity;
        let mut equipment = living
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        equipment.put(&EquipmentSlot::MAIN_HAND, ItemStack::new(1, &Item::BOW));
    }
}
