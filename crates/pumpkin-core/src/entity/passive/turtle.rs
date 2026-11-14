use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::Block;
use pumpkin_data::block_properties::TurtleEggLikeProperties;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::blocks::turtle_egg::TurtleEggBlock;
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, breed::BreedGoal, escape_danger::EscapeDangerGoal,
        look_at_entity::LookAtEntityGoal, move_to_block::MoveToBlockGoal, swim::SwimGoal,
        tempt::TemptGoal, try_find_water::TryFindWaterGoal, wander_around::WanderAroundGoal,
    },
    experience_orb::ExperienceOrbEntity,
    living::LivingEntity,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
    passive::animal::{Animal, PARENT_AGE_AFTER_BREEDING, animal_can_mate},
    player::Player,
};
use crate::world::World;

/// Vanilla `TurtleLayEggGoal`: a turtle with an egg near home walks to sand with air above,
/// digs for 200 ticks and lays one to four eggs.
struct TurtleLayEggGoal {
    turtle: Weak<TurtleEntity>,
    move_to_block: MoveToBlockGoal,
}

impl TurtleLayEggGoal {
    fn new(turtle: Weak<TurtleEntity>) -> Self {
        let move_to_block = MoveToBlockGoal::new(
            1.0,
            16,
            1,
            Box::new(|world, pos| {
                world.get_block_state(&pos.up()).is_air() && TurtleEggBlock::is_sand(world, &pos)
            }),
        );
        Self {
            turtle,
            move_to_block,
        }
    }

    /// `hasEgg() && homePos.closerToCenterThan(position, 9)`.
    fn ready(&self) -> bool {
        self.turtle
            .upgrade()
            .is_some_and(|turtle| turtle.has_egg() && turtle.is_near_home(9.0))
    }
}

impl Goal for TurtleLayEggGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.ready() && self.move_to_block.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.move_to_block.should_continue(mob) && self.ready()
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_block.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.move_to_block.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_block.tick(mob);
        let Some(turtle) = self.turtle.upgrade() else {
            return;
        };
        let entity = mob.get_entity();
        if entity.is_in_water() || !self.move_to_block.is_reached_target() {
            return;
        }
        let counter = turtle.lay_egg_counter.load(Ordering::Relaxed);
        if counter < 1 {
            turtle.set_laying_egg(true);
        } else if counter > 200 {
            let world = entity.world.load();
            let pitch = 0.9 + mob.get_random().random::<f32>() * 0.2;
            world.play_sound_fine(
                Sound::EntityTurtleLayEgg,
                SoundCategory::Blocks,
                &entity.block_pos.load().to_f64(),
                0.3,
                pitch,
            );
            let egg_pos = self.move_to_block.target_pos.up();
            let mut egg =
                TurtleEggLikeProperties::from_state_id(Block::TURTLE_EGG.default_state.id);
            egg.eggs = mob.get_random().random_range(1..=4);
            world.set_block_state(
                &egg_pos,
                egg.to_state_id(&Block::TURTLE_EGG),
                BlockFlags::NOTIFY_ALL,
            );
            turtle.set_has_egg(false);
            turtle.set_laying_egg(false);
            turtle.mob_entity.love_ticks.store(600, Ordering::Relaxed);
        }
        if turtle.is_laying_egg() {
            turtle.lay_egg_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.move_to_block.controls()
    }
}

/// Vanilla `Turtle.BABY_ON_LAND_SELECTOR`: baby turtles out of the water are prey.
pub fn baby_on_land_selector(target: &LivingEntity, _world: &World) -> bool {
    target.entity.age.load(Ordering::Relaxed) < 0 && !target.is_in_water()
}

pub struct TurtleEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub has_egg: AtomicBool,
    pub laying_egg: AtomicBool,
    /// Vanilla `layEggCounter`: ticks spent digging.
    pub lay_egg_counter: AtomicI32,
    /// Vanilla `homePos`: where it spawned; `None` until first needed (vanilla falls back to
    /// the current block on load).
    pub home_pos: Mutex<Option<BlockPos>>,
}

impl TurtleEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let turtle = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            has_egg: AtomicBool::new(false),
            laying_egg: AtomicBool::new(false),
            lay_egg_counter: AtomicI32::new(0),
            home_pos: Mutex::new(None),
        };
        let mob_arc = Arc::new(turtle);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(TryFindWaterGoal));
            goal_selector.add_goal(1, Box::new(SwimGoal::default()));
            goal_selector.add_goal(0, EscapeDangerGoal::new(1.2).seeking_water(7));
            goal_selector.add_goal(1, BreedGoal::new(1.0));
            goal_selector.add_goal(1, Box::new(TurtleLayEggGoal::new(Arc::downgrade(&mob_arc))));
            goal_selector.add_goal(
                2,
                Box::new(TemptGoal::new(
                    1.1,
                    |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_TURTLE_FOOD),
                    false,
                )),
            );
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(9, Box::new(WanderAroundGoal::with_interval(1.0, 100)));
        };

        mob_arc
    }

    #[must_use]
    pub fn has_egg(&self) -> bool {
        self.has_egg.load(Ordering::Relaxed)
    }

    pub fn set_has_egg(&self, has_egg: bool) {
        self.has_egg.store(has_egg, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::HAS_EGG, has_egg);
    }

    #[must_use]
    pub fn is_laying_egg(&self) -> bool {
        self.laying_egg.load(Ordering::Relaxed)
    }

    pub fn set_laying_egg(&self, laying_egg: bool) {
        self.laying_egg.store(laying_egg, Ordering::Relaxed);
        self.lay_egg_counter
            .store(i32::from(laying_egg), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::LAYING_EGG, laying_egg);
    }
}

impl AgeableMob for TurtleEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl TurtleEntity {
    pub fn home_pos(&self) -> BlockPos {
        *self
            .home_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(|| self.get_entity().block_pos.load())
    }

    pub fn set_home_pos(&self, pos: BlockPos) {
        *self
            .home_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pos);
    }

    /// Vanilla `homePos.closerToCenterThan(position(), distance)`.
    fn is_near_home(&self, distance: f64) -> bool {
        let home = self.home_pos();
        let center = Vector3::new(
            f64::from(home.0.x) + 0.5,
            f64::from(home.0.y) + 0.5,
            f64::from(home.0.z) + 0.5,
        );
        center.squared_distance_to_vec(&self.get_entity().pos.load()) < distance * distance
    }
}

impl Animal for TurtleEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_TURTLE_FOOD)
            || item_stack.item == &Item::SEAGRASS
    }

    /// Vanilla `TurtleBreedGoal.canUse`: a turtle already carrying an egg doesn't breed.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        !self.has_egg() && animal_can_mate(&self.mob_entity, partner)
    }

    /// Vanilla `TurtleBreedGoal.breed`: the turtle gets an egg to lay instead of a baby, with
    /// the breeding stats, the parents' cooldown and the experience, but no hearts.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Mob) {
        let cause = self
            .get_love_cause()
            .or_else(|| partner.as_animal().and_then(Animal::get_love_cause));
        if let Some(player) = cause {
            player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::AnimalsBred as i32,
                1,
            );
            player.trigger_advancement_criterion(
                pumpkin_data::advancement::Advancement::HUSBANDRY_BREED_AN_ANIMAL,
                "bred",
            );
            player.trigger_advancement_criterion(
                pumpkin_data::advancement::Advancement::HUSBANDRY_BRED_ALL_ANIMALS,
                "minecraft:turtle",
            );
        }
        self.set_has_egg(true);
        self.set_age(PARENT_AGE_AFTER_BREEDING);
        if let Some(ageable) = partner.as_ageable() {
            ageable.set_age(PARENT_AGE_AFTER_BREEDING);
        }
        self.reset_love();
        if let Some(animal) = partner.as_animal() {
            animal.reset_love();
        }
        if world.level_info.load().game_rules.mob_drops {
            ExperienceOrbEntity::spawn(
                world,
                self.get_entity().pos.load(),
                self.get_random().random_range(1..=7),
            );
        }
    }
}

impl Mob for TurtleEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        let entity = self.get_entity();
        (!entity.is_in_water() && entity.on_ground.load(Ordering::Relaxed) && !self.is_baby())
            .then_some(pumpkin_data::sound::Sound::EntityTurtleAmbientLand)
    }

    /// Vanilla `Turtle.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        200
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("HasEgg", self.has_egg());
        let home = self.home_pos();
        nbt.put(
            "home_pos",
            pumpkin_nbt::tag::NbtTag::IntArray(vec![home.0.x, home.0.y, home.0.z]),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(has_egg) = nbt.get_bool("HasEgg") {
            self.set_has_egg(has_egg);
        }
        if let Some(&[x, y, z]) = nbt.get_int_array("home_pos") {
            self.set_home_pos(BlockPos::new(x, y, z));
        }
    }

    /// Vanilla `Turtle.finalizeSpawn`: home is where it spawned.
    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.mob_entity.finalize_spawn_base();
        self.set_home_pos(self.get_entity().block_pos.load());
        group_data
    }

    /// Vanilla `Turtle.aiStep`: digging kicks up the sand under it every 5 ticks.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = self.get_entity();
        let counter = self.lay_egg_counter.load(Ordering::Relaxed);
        if entity.is_alive() && self.is_laying_egg() && counter >= 1 && counter % 5 == 0 {
            let pos = entity.block_pos.load();
            let world = entity.world.load();
            if TurtleEggBlock::on_sand(&**world, &pos) {
                let below = world.get_block_state_id(&pos.down());
                world.sync_world_event(
                    WorldEvent::ParticlesAndSoundDestroyBlock,
                    pos,
                    i32::from(below.as_u16()),
                );
            }
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::turtle::DATA_BABY_ID, true);
        }
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::HAS_EGG, self.has_egg());
        entity.set_synced_data(
            pumpkin_data::tracked_data::turtle::LAYING_EGG,
            self.is_laying_egg(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}
