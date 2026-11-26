use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{
            active_target::ActiveTargetGoal, look_at_entity::LookAtEntityGoal,
            melee_attack::MeleeAttackGoal, revenge::RevengeGoal, swim::SwimGoal,
            wander_around::WanderAroundGoal,
        },
        pathfinder::node::PathType,
    },
    mob::{
        Mob, MobEntity,
        patrol::{PatrolData, PatrollingMonster},
        raider::{
            Raider, RaiderData, is_abstract_villager, raider_can_attack, register_raider_goals,
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
        // Vanilla `Ravager`'s constructor: leaves are no obstacle, it breaks through them.
        mob_arc
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_pathfinding_malus(PathType::Leaves, 0.0);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            // Vanilla `Ravager.registerGoals`.
            register_raider_goals(goal_selector);
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, true)));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::water_avoiding(0.4)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );

            target_selector.add_goal(
                2,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                4,
                // Vanilla `(target, level) -> !target.isBaby()`: a baby villager is left alone.
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 10, true, |target, _| {
                    is_abstract_villager(target.entity.entity_type)
                        && target.entity.age.load(Ordering::Relaxed) >= 0
                }),
            );
            target_selector.add_goal(
                4,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
        };

        mob_arc
    }
}

impl Mob for RavagerEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityRavagerAmbient)
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(pumpkin_data::sound::Sound::EntityRavagerStep, 0.15)
    }

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

    /// Vanilla `Ravager.aiStep`.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let living = &self.mob_entity.living_entity;
        if !living.is_alive() {
            return;
        }
        let speed = if self.is_mob_immobile() {
            0.0
        } else {
            let max_speed = if self.mob_entity.get_target().is_some() {
                0.35
            } else {
                0.3
            };
            let base = living.get_attribute_base(&Attributes::MOVEMENT_SPEED);
            pumpkin_util::math::lerp(0.1, base, max_speed)
        };
        living.set_attribute_base(&Attributes::MOVEMENT_SPEED, speed);

        let entity = &living.entity;
        let world = entity.world.load_full();
        if entity.horizontal_collision.load(Ordering::Relaxed)
            && world.level_info.load().game_rules.mob_griefing
        {
            let bb = entity.bounding_box.load().expand(0.2, 0.2, 0.2);
            let mut destroyed = false;
            for x in bb.min.x.floor() as i32..=bb.max.x.floor() as i32 {
                for y in bb.min.y.floor() as i32..=bb.max.y.floor() as i32 {
                    for z in bb.min.z.floor() as i32..=bb.max.z.floor() as i32 {
                        let pos = BlockPos::new(x, y, z);
                        if world.get_block(&pos).has_tag(&tag::Block::MINECRAFT_LEAVES) {
                            destroyed |= world
                                .break_block(&pos, None, BlockFlags::NOTIFY_ALL)
                                .is_some();
                        }
                    }
                }
            }
            if !destroyed && entity.on_ground.load(Ordering::Relaxed) {
                living.jump();
            }
        }

        let tick = self.attack_tick.load(Ordering::Relaxed);
        if tick > 0 {
            self.attack_tick.store(tick - 1, Ordering::Relaxed);
        }
    }

    fn can_attack(&self, target: &dyn EntityBase) -> bool {
        raider_can_attack(self, target)
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
