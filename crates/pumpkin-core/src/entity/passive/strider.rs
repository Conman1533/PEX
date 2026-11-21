use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;

use crate::block::PathComputationType;
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, breed::BreedGoal, escape_danger::EscapeDangerGoal,
        follow_parent::FollowParentGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, move_to_block::MoveToBlockGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    ai::pathfinder::{Navigator, node::PathType},
    item_steerable::{ItemBasedSteering, ItemSteerable},
    mob::{Mob, MobEntity, is_in_water_or_rain},
    passive::animal::Animal,
    player::Player,
};

pub struct StriderEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub steering: ItemBasedSteering,
    pub saddled: AtomicBool,
    pub suffocating: AtomicBool,
}

impl StriderEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        let navigator = mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *navigator = Navigator::strider();
        navigator.set_pathfinding_malus(PathType::Water, -1.0);
        navigator.set_pathfinding_malus(PathType::Lava, 0.0);
        navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
        navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
        let strider = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            steering: ItemBasedSteering::default(),
            saddled: AtomicBool::new(false),
            suffocating: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(strider);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            // Not in vanilla, which floats striders on lava in `Strider.floatStrider` instead.
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(1.65));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                Box::new(TemptGoal::new(
                    1.4,
                    |_, stack| {
                        stack
                            .get_item()
                            .has_tag(&tag::Item::MINECRAFT_STRIDER_TEMPT_ITEMS)
                    },
                    false,
                )),
            );
            goal_selector.add_goal(4, Box::new(StriderGoToLavaGoal::new()));
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(1.0)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::with_interval(1.0, 60)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::STRIDER, 8.0),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_suffocating(&self) -> bool {
        self.suffocating.load(Ordering::Relaxed)
    }

    pub fn set_suffocating(&self, suffocating: bool) {
        self.suffocating.store(suffocating, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            suffocating,
        );
    }
}

impl AgeableMob for StriderEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for StriderEntity {
    fn play_eating_sound(&self) {
        crate::entity::passive::animal::play_eating_sound(
            self,
            pumpkin_data::sound::Sound::EntityStriderEat,
            1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2,
        );
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_STRIDER_FOOD)
            || item_stack.item == &Item::WARPED_FUNGUS
    }
}

impl Mob for StriderEntity {
    fn mob_controlling_passenger(&self) -> Option<std::sync::Arc<dyn crate::entity::EntityBase>> {
        crate::entity::mob::item_steering_player_controller(
            self.get_entity(),
            self.is_saddled(),
            &pumpkin_data::item::Item::WARPED_FUNGUS_ON_A_STICK,
        )
        .or_else(|| {
            crate::entity::mob::mob_rider_controller(
                self.get_entity(),
                self.get_mob_entity().is_no_ai(),
            )
        })
    }

    fn travel(&self, _caller: &dyn crate::entity::EntityBase) -> bool {
        crate::entity::mob::player_steers(self)
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(
            if self
                .mob_entity
                .living_entity
                .entity
                .touching_lava
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                pumpkin_data::sound::Sound::EntityStriderStepLava
            } else {
                pumpkin_data::sound::Sound::EntityStriderStep
            },
            1.0,
        )
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!self.is_panicking() && !self.is_tempted())
            .then_some(pumpkin_data::sound::Sound::EntityStriderAmbient)
    }

    /// Vanilla `Strider.tick`.
    fn mob_tick(&self, caller: &dyn EntityBase) {
        if self.is_tempted() && rand::random_range(0..140) == 0 {
            self.make_sound(pumpkin_data::sound::Sound::EntityStriderHappy);
        } else if self.is_panicking() && rand::random_range(0..60) == 0 {
            self.make_sound(pumpkin_data::sound::Sound::EntityStriderRetreat);
        }

        let entity = &self.mob_entity.living_entity.entity;
        if !self.mob_entity.is_no_ai() {
            let world = entity.world.load();
            let pos = entity.pos.load();
            let warm = &tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS;
            let in_warm_blocks = world.get_block(&entity.block_pos.load()).has_tag(warm)
                || world
                    .get_block(&BlockPos::floored(pos.x, pos.y - 0.2, pos.z))
                    .has_tag(warm)
                || entity.touching_lava.load(Ordering::SeqCst);
            let on_warm_strider = entity
                .vehicle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|vehicle| {
                    vehicle
                        .cast_any()
                        .downcast_ref::<Self>()
                        .is_some_and(|strider| !strider.is_suffocating())
                });
            let suffocating = !in_warm_blocks && !on_warm_strider;
            if suffocating != self.is_suffocating() {
                self.set_suffocating(suffocating);
            }
        }

        // Vanilla `isSensitiveToWater`.
        if entity.is_alive() && is_in_water_or_rain(entity) {
            caller.damage(caller, 1.0, pumpkin_data::damage::DamageType::DROWN);
        }
    }

    /// Vanilla `Strider.getWalkTargetValue`: lava is worth walking to, and a strider in lava never
    /// strolls out of it.
    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let entity = &self.mob_entity.living_entity.entity;
        if entity
            .world
            .load()
            .get_fluid(pos)
            .has_tag(&tag::Fluid::MINECRAFT_LAVA)
        {
            10.0
        } else if entity.touching_lava.load(Ordering::SeqCst) {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("Saddle", self.is_saddled());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(saddle) = nbt.get_bool("Saddle") {
            self.set_saddled(saddle);
        } else if let Some(saddle_byte) = nbt.get_byte("Saddle") {
            self.set_saddled(saddle_byte == 1);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_item_steerable(&self) -> Option<&dyn ItemSteerable> {
        Some(self)
    }

    fn is_saddled(&self) -> bool {
        self.saddled.load(Ordering::Relaxed)
    }

    fn can_be_saddled(&self) -> bool {
        self.mob_entity.living_entity.entity.is_alive() && !self.is_baby()
    }

    fn set_saddled(&self, saddled: bool) {
        self.saddled.store(saddled, Ordering::Relaxed);
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::strider::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            self.is_suffocating(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();

        if item == &Item::SADDLE && self.can_be_saddled() && !self.is_saddled() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.play_sound(Sound::EntityStriderSaddle, SoundCategory::Neutral, &pos);
            return true;
        }

        if self.is_saddled() && !self.is_food(item_stack) {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack)
    }
}

impl StriderEntity {
    /// Vanilla `isBeingTempted`: following a player who holds warped fungus.
    fn is_tempted(&self) -> bool {
        self.mob_entity
            .goals_selector
            .is_running::<crate::entity::ai::goal::tempt::TemptGoal>()
    }
}

impl ItemSteerable for StriderEntity {
    fn boost(&self) -> bool {
        self.steering.boost()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Vanilla `Strider.StriderGoToLavaGoal`: out of lava, walk to lava within 8 blocks that has
/// room above it.
struct StriderGoToLavaGoal {
    inner: MoveToBlockGoal,
}

impl StriderGoToLavaGoal {
    fn new() -> Self {
        let inner = MoveToBlockGoal::new(
            1.0,
            8,
            2,
            Box::new(|world, pos| {
                world.get_block(&pos).id == pumpkin_data::Block::LAVA.id && {
                    let (block, state) = world.get_block_and_state(&pos.up());
                    world
                        .block_registry
                        .is_pathfindable(block, state, PathComputationType::Land)
                }
            }),
        )
        .targeting_the_block_itself()
        .with_recalculate_interval(20);
        Self { inner }
    }
}

impl Goal for StriderGoToLavaGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !mob.get_entity().touching_lava.load(Ordering::SeqCst) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let world = mob.get_entity().world.load();
        !mob.get_entity().touching_lava.load(Ordering::SeqCst)
            && (self.inner.is_valid_target)(&world, self.inner.target_pos)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}
