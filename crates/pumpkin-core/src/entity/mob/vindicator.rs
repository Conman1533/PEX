use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::Difficulty;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
        break_door::BreakDoorGoal, look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal,
        revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{PatrolData, PatrollingMonster},
        raider::{
            HoldGroundAttackGoal, Raider, RaiderData, RaiderOpenDoorGoal, is_abstract_villager,
            raider_can_attack, register_raider_goals,
        },
    },
};

pub struct VindicatorEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    /// Vanilla `isJohnny`: set for good once the vindicator is named Johnny.
    is_johnny: AtomicBool,
}

impl VindicatorEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let vindicator = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            is_johnny: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(vindicator);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            // Vanilla `Vindicator.registerGoals`.
            register_raider_goals(goal_selector);
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                1,
                Box::new(AvoidEntityGoal::new(&EntityType::CREAKING, 8.0, 1.0, 1.2)),
            );
            goal_selector.add_goal(2, Box::new(VindicatorBreakDoorGoal::new()));
            goal_selector.add_goal(3, Box::new(RaiderOpenDoorGoal::default()));
            goal_selector.add_goal(4, Box::new(HoldGroundAttackGoal::new(10.0)));
            goal_selector.add_goal(5, Box::new(MeleeAttackGoal::new(1.0, false)));
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 10, true, |target, _| {
                    is_abstract_villager(target.entity.entity_type)
                }),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
            // Vanilla `VindicatorJohnnyAttackGoal`: Johnny attacks anything attackable.
            target_selector.add_goal(
                4,
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 0, true, |_, _| true)
                    .navigable()
                    .gated_by(|mob| {
                        mob.cast_any()
                            .downcast_ref::<Self>()
                            .is_some_and(Self::is_johnny)
                    }),
            );
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak,
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );
        };

        mob_arc
    }
}

impl VindicatorEntity {
    #[must_use]
    pub fn is_johnny(&self) -> bool {
        self.is_johnny.load(Ordering::Relaxed)
    }

    /// Vanilla `Vindicator.setCustomName`: the name Johnny turns the vindicator into Johnny, and
    /// renaming it later doesn't turn it back.
    fn check_johnny_name(&self) {
        if self.is_johnny() {
            return;
        }
        let name = self.mob_entity.living_entity.entity.custom_name.load();
        if name
            .as_ref()
            .as_ref()
            .is_some_and(|name| name.clone().get_text() == "Johnny")
        {
            self.is_johnny.store(true, Ordering::Relaxed);
        }
    }
}

/// Vanilla `Vindicator.DOOR_BREAKING_PREDICATE`.
fn door_breaking_difficulty(difficulty: Difficulty) -> bool {
    difficulty == Difficulty::Normal || difficulty == Difficulty::Hard
}

/// Vanilla `Vindicator.VindicatorBreakDoorGoal`: during a raid, a vindicator breaks wooden doors
/// on Normal and Hard.
struct VindicatorBreakDoorGoal(BreakDoorGoal);

impl VindicatorBreakDoorGoal {
    fn new() -> Self {
        Self(BreakDoorGoal::with_door_break_time(
            6,
            Arc::new(door_breaking_difficulty),
        ))
    }

    fn has_active_raid(mob: &dyn Mob) -> bool {
        mob.as_raider().is_some_and(Raider::has_active_raid)
    }
}

impl Goal for VindicatorBreakDoorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::has_active_raid(mob)
            && mob.get_random().random_range(0..to_goal_ticks(10)) == 0
            && self.0.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        Self::has_active_raid(mob) && self.0.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.0.start(mob);
        mob.get_mob_entity()
            .living_entity
            .no_action_time
            .store(0, Ordering::Relaxed);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.0.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.0.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.0.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

impl Mob for VindicatorEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityVindicatorAmbient)
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        if self.is_johnny() {
            nbt.put_bool("Johnny", true);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        self.is_johnny
            .store(nbt.get_bool("Johnny").unwrap_or(false), Ordering::Relaxed);
    }

    /// Vanilla `Vindicator.customServerAiStep`: the vindicator paths through doors only while a
    /// raid is going on where it stands.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.check_johnny_name();
        let entity = &self.mob_entity.living_entity.entity;
        let block_pos = entity.block_pos.load();
        let raided = entity
            .world
            .load()
            .raids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_raid_at(&block_pos)
            .is_some();
        self.mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_can_open_doors(raided);
    }

    fn can_attack(&self, target: &dyn EntityBase) -> bool {
        raider_can_attack(self, target)
    }
}

impl PatrollingMonster for VindicatorEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for VindicatorEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityVindicatorCelebrate
    }
}
