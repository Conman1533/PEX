use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal, swim::SwimGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            HoldGroundAttackGoal, PathfindToRaidGoal, Raider, RaiderCelebrationGoal, RaiderData,
            RaiderMoveThroughVillageGoal,
        },
    },
};

pub struct RavagerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    /// Vanilla `attackTick`: the ravager stands still while its bite plays out.
    attack_tick: AtomicI32,
}

impl RavagerEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let ravager = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            attack_tick: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(ravager);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(2, Box::new(HoldGroundAttackGoal::new(10.0)));
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, true)));
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(4, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::VILLAGER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
        };

        mob_arc
    }
}

impl Mob for RavagerEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn is_mob_immobile(&self) -> bool {
        self.mob_entity.living_entity.is_dead_or_dying()
            || self.attack_tick.load(Ordering::Relaxed) > 0
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let tick = self.attack_tick.load(Ordering::Relaxed);
        if tick > 0 {
            self.attack_tick.store(tick - 1, Ordering::Relaxed);
        }
    }

    fn before_melee_attack(&self, _target: &dyn EntityBase) {
        self.attack_tick.store(10, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        world.send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        world.play_sound_fine(
            Sound::EntityRavagerAttack,
            SoundCategory::Hostile,
            &entity.pos.load(),
            1.0,
            1.0,
        );
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
    }
}

impl PatrollingMonster for RavagerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }

    fn can_be_leader(&self) -> bool {
        false
    }
}

impl Raider for RavagerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityRavagerCelebrate
    }
}
