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
        look_at_entity::LookAtEntityGoal, move_to_block::MoveToBlockGoal, tempt::TemptGoal,
        to_goal_ticks, wander_around::WanderAroundGoal,
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
    /// Vanilla `travelPos`: where `TurtleTravelGoal` is swimming to. Shared with the navigation,
    /// whose stable destinations depend on it.
    pub travel_pos: Arc<Mutex<Option<BlockPos>>>,
    /// Vanilla `goingHome`, set while `TurtleGoHomeGoal` runs.
    pub going_home: AtomicBool,
}

/// Vanilla `Turtle.TurtleMoveControl`: slower in water away from home and as a baby, and on land,
/// and it swims up or down towards where it is heading.
struct TurtleMoveControl {
    inner: crate::entity::ai::control::move_control::MoveControl,
}

impl crate::entity::ai::control::Control for TurtleMoveControl {}

impl TurtleMoveControl {
    /// Vanilla `updateSpeed`.
    fn update_speed(mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let living = &mob_entity.living_entity;
        let entity = &living.entity;
        let turtle = mob.cast_any().downcast_ref::<TurtleEntity>();
        if entity.is_in_water() {
            entity
                .velocity
                .store(entity.velocity.load().add_raw(0.0, 0.005, 0.0));
            if turtle.is_some_and(|turtle| !turtle.is_near_home(16.0)) {
                mob_entity.set_speed((living.get_speed() / 2.0).max(0.08));
            }
            if entity.age.load(Ordering::Relaxed) < 0 {
                mob_entity.set_speed((living.get_speed() / 3.0).max(0.06));
            }
        } else if entity.on_ground.load(Ordering::Relaxed) {
            mob_entity.set_speed((living.get_speed() / 2.0).max(0.06));
        }
    }
}

impl crate::entity::ai::control::MoveControlTrait for TurtleMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        use crate::entity::ai::control::move_control::{MoveControl, Operation};
        use pumpkin_data::attributes::Attributes;

        Self::update_speed(mob);
        let mob_entity = mob.get_mob_entity();
        let living = &mob_entity.living_entity;
        let entity = &living.entity;
        if self.inner.operation != Operation::MoveTo || mob_entity.steered().navigator().is_done() {
            mob_entity.set_speed(0.0);
            return;
        }
        let pos = entity.pos.load();
        let xd = self.inner.wanted_x - pos.x;
        let yd = self.inner.wanted_y - pos.y;
        let zd = self.inner.wanted_z - pos.z;
        let distance = (xd * xd + yd * yd + zd * zd).sqrt();
        if distance < 1.0e-5 {
            mob_entity.set_speed(0.0);
            return;
        }
        let yd = yd / distance;
        let yaw = MoveControl::rotlerp(
            entity.yaw.load(),
            zd.atan2(xd).to_degrees() as f32 - 90.0,
            90.0,
        );
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);
        let target_speed = (self.inner.speed_modifier
            * living.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let speed = living.get_speed() + 0.125 * (target_speed - living.get_speed());
        mob_entity.set_speed(speed);
        entity.velocity.store(entity.velocity.load().add_raw(
            0.0,
            f64::from(speed) * yd * 0.1,
            0.0,
        ));
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.inner.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forwards: f32, right: f32) {
        self.inner.strafe(forwards, right);
    }

    fn has_wanted(&self) -> bool {
        self.inner.has_wanted()
    }
}

/// Vanilla `Turtle.TurtleGoHomeGoal`: an adult with an egg, or now and then one more than 64
/// blocks from home, swims and walks back to the beach it hatched on.
struct TurtleGoHomeGoal {
    speed: f64,
    stuck: bool,
    close_to_home_try_ticks: i32,
}

impl TurtleGoHomeGoal {
    const fn new(speed: f64) -> Self {
        Self {
            speed,
            stuck: false,
            close_to_home_try_ticks: 0,
        }
    }
}

impl Goal for TurtleGoHomeGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = TurtleEntity::of(mob) else {
            return false;
        };
        if turtle.get_entity().age.load(Ordering::Relaxed) < 0 {
            return false;
        }
        if turtle.has_egg() {
            return true;
        }
        mob.get_random().random_range(0..to_goal_ticks(700)) == 0 && !turtle.is_near_home(64.0)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        TurtleEntity::of(mob).is_some_and(|turtle| {
            !turtle.is_near_home(7.0)
                && !self.stuck
                && self.close_to_home_try_ticks <= self.get_tick_count(600)
        })
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = TurtleEntity::of(mob) {
            turtle.going_home.store(true, Ordering::Relaxed);
        }
        self.stuck = false;
        self.close_to_home_try_ticks = 0;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = TurtleEntity::of(mob) {
            turtle.going_home.store(false, Ordering::Relaxed);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        use crate::entity::ai::util::default_random_pos::get_pos_towards;
        use std::f64::consts::{FRAC_PI_2, PI};

        let Some(turtle) = TurtleEntity::of(mob) else {
            return;
        };
        let home = turtle.home_pos();
        let close_to_home = turtle.is_near_home(16.0);
        if close_to_home {
            self.close_to_home_try_ticks += 1;
        }
        if !mob.is_navigator_idle() {
            return;
        }
        let target = Vector3::new(
            f64::from(home.0.x) + 0.5,
            f64::from(home.0.y),
            f64::from(home.0.z) + 0.5,
        );
        let mut next = get_pos_towards(mob, 16, 3, target, PI / 10.0)
            .or_else(|| get_pos_towards(mob, 8, 7, target, FRAC_PI_2));
        if let Some(pos) = next
            && !close_to_home
            && turtle
                .get_entity()
                .world
                .load()
                .get_block(&pos.to_block_pos())
                != &Block::WATER
        {
            next = get_pos_towards(mob, 16, 5, target, FRAC_PI_2);
        }
        let Some(next) = next else {
            self.stuck = true;
            return;
        };
        mob.navigate_to(mob.get_entity().pos.load(), next, self.speed);
    }
}

/// Vanilla `Turtle.TurtleGoToWaterGoal`: a turtle on land heads for water within 24 blocks.
struct TurtleGoToWaterGoal {
    move_to_block: MoveToBlockGoal,
}

impl TurtleGoToWaterGoal {
    const GIVE_UP_TICKS: i32 = 1200;

    /// Vanilla passes `turtle.isBaby() ? 2.0 : speedModifier` while the turtle is being built,
    /// before any age is set, so it is always the given speed.
    fn new(speed: f64) -> Self {
        let mut move_to_block = MoveToBlockGoal::new(
            speed,
            24,
            1,
            Box::new(|world, pos| world.get_block(&pos) == &Block::WATER),
        )
        .with_recalculate_interval(160);
        move_to_block.vertical_search_start = -1;
        Self { move_to_block }
    }
}

impl Goal for TurtleGoToWaterGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = TurtleEntity::of(mob) else {
            return false;
        };
        let in_water = turtle.get_entity().is_in_water();
        let baby = turtle.get_entity().age.load(Ordering::Relaxed) < 0;
        if (baby && !in_water)
            || (!turtle.going_home.load(Ordering::Relaxed) && !in_water && !turtle.has_egg())
        {
            self.move_to_block.can_start(mob)
        } else {
            false
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        let world = entity.world.load();
        !entity.is_in_water()
            && self.move_to_block.try_ticks <= Self::GIVE_UP_TICKS
            && (self.move_to_block.is_valid_target)(&world, self.move_to_block.target_pos)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_block.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.move_to_block.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_block.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.move_to_block.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.move_to_block.controls()
    }
}

/// Vanilla `Turtle.TurtleTravelGoal`: in the water, a turtle without an egg swims towards a random
/// spot up to 512 blocks away.
struct TurtleTravelGoal {
    speed: f64,
    stuck: bool,
}

impl TurtleTravelGoal {
    const fn new(speed: f64) -> Self {
        Self {
            speed,
            stuck: false,
        }
    }
}

impl Goal for TurtleTravelGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        TurtleEntity::of(mob).is_some_and(|turtle| {
            !turtle.going_home.load(Ordering::Relaxed)
                && !turtle.has_egg()
                && turtle.get_entity().is_in_water()
        })
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        TurtleEntity::of(mob).is_some_and(|turtle| {
            !mob.is_navigator_idle()
                && !self.stuck
                && !turtle.going_home.load(Ordering::Relaxed)
                && turtle.mob_entity.love_ticks.load(Ordering::Relaxed) <= 0
                && !turtle.has_egg()
        })
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(turtle) = TurtleEntity::of(mob) else {
            return;
        };
        let entity = turtle.get_entity();
        let x = mob.get_random().random_range(0..1025) - 512;
        let mut y = mob.get_random().random_range(0..9) - 4;
        let z = mob.get_random().random_range(0..1025) - 512;
        let pos = entity.pos.load();
        let sea_level = entity.world.load().sea_level;
        if f64::from(y) + pos.y > f64::from(sea_level - 1) {
            y = 0;
        }
        *turtle
            .travel_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(BlockPos::floored(
            f64::from(x) + pos.x,
            f64::from(y) + pos.y,
            f64::from(z) + pos.z,
        ));
        self.stuck = false;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = TurtleEntity::of(mob) {
            *turtle
                .travel_pos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        use crate::entity::ai::util::default_random_pos::get_pos_towards;
        use std::f64::consts::{FRAC_PI_2, PI};

        let Some(turtle) = TurtleEntity::of(mob) else {
            return;
        };
        let travel = *turtle
            .travel_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(travel) = travel else {
            self.stuck = true;
            return;
        };
        if !mob.is_navigator_idle() {
            return;
        }
        let target = Vector3::new(
            f64::from(travel.0.x) + 0.5,
            f64::from(travel.0.y),
            f64::from(travel.0.z) + 0.5,
        );
        let mut next = get_pos_towards(mob, 16, 3, target, PI / 10.0)
            .or_else(|| get_pos_towards(mob, 8, 7, target, FRAC_PI_2));
        // Vanilla `hasChunksAt(x - 34, z - 34, x + 34, z + 34)`.
        if let Some(pos) = next {
            let world = turtle.get_entity().world.load();
            let (x, z) = (pos.x.floor() as i32, pos.z.floor() as i32);
            let y = pos.y.floor() as i32;
            let loaded = ((x - 34) >> 4..=(x + 34) >> 4).all(|cx| {
                ((z - 34) >> 4..=(z + 34) >> 4)
                    .all(|cz| world.is_loaded(&BlockPos::new(cx << 4, y, cz << 4)))
            });
            if !loaded {
                next = None;
            }
        }
        let Some(next) = next else {
            self.stuck = true;
            return;
        };
        mob.navigate_to(mob.get_entity().pos.load(), next, self.speed);
    }
}

/// Vanilla `Turtle.TurtleRandomStrollGoal`: strolls only on land, and not on the way home or with
/// an egg.
struct TurtleRandomStrollGoal {
    stroll: WanderAroundGoal,
}

impl TurtleRandomStrollGoal {
    fn allowed(mob: &dyn Mob) -> bool {
        TurtleEntity::of(mob).is_some_and(|turtle| {
            !turtle.get_entity().is_in_water()
                && !turtle.going_home.load(Ordering::Relaxed)
                && !turtle.has_egg()
        })
    }
}

impl Goal for TurtleRandomStrollGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::allowed(mob) && self.stroll.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.stroll.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.stroll.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.stroll.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.stroll.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.stroll.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.stroll.controls()
    }
}

impl TurtleEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        use crate::entity::ai::pathfinder::{AmphibiousPathNavigation, Navigator, node::PathType};

        let mut mob_entity = MobEntity::new(entity);
        let travel_pos: Arc<Mutex<Option<BlockPos>>> = Arc::new(Mutex::new(None));
        // Vanilla `TurtlePathNavigation`: while travelling, only water is a stable
        // destination; otherwise anything but air below.
        let mut navigation = AmphibiousPathNavigation::new(false);
        let travelling = travel_pos.clone();
        navigation.stable_destination = Some(Arc::new(move |world, pos| {
            if travelling
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
            {
                world.get_block(pos) == &Block::WATER
            } else {
                !world.get_block_state(&pos.down()).is_air()
            }
        }));
        let mut navigator = Navigator::new(navigation);
        navigator.set_pathfinding_malus(PathType::Water, 0.0);
        navigator.set_pathfinding_malus(PathType::DoorIronClosed, -1.0);
        navigator.set_pathfinding_malus(PathType::DoorWoodClosed, -1.0);
        navigator.set_pathfinding_malus(PathType::DoorOpen, -1.0);
        *mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = navigator;
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(TurtleMoveControl {
            inner: crate::entity::ai::control::move_control::MoveControl::default(),
        });
        let turtle = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            has_egg: AtomicBool::new(false),
            laying_egg: AtomicBool::new(false),
            lay_egg_counter: AtomicI32::new(0),
            home_pos: Mutex::new(None),
            travel_pos,
            going_home: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(turtle);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

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
            goal_selector.add_goal(3, Box::new(TurtleGoToWaterGoal::new(1.0)));
            goal_selector.add_goal(4, Box::new(TurtleGoHomeGoal::new(1.0)));
            goal_selector.add_goal(7, Box::new(TurtleTravelGoal::new(1.0)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(
                9,
                Box::new(TurtleRandomStrollGoal {
                    stroll: WanderAroundGoal::with_interval(1.0, 100),
                }),
            );
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
    fn of(mob: &dyn Mob) -> Option<&Self> {
        mob.cast_any().downcast_ref::<Self>()
    }

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

    /// Vanilla `level.getFluidState(pos).is(FluidTags.WATER)`.
    fn is_water_at(world: &World, pos: &BlockPos) -> bool {
        let (fluid, _) = world.get_fluid_and_fluid_state(pos);
        fluid.has_tag(&tag::Fluid::MINECRAFT_WATER)
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
    /// Vanilla `Turtle.getWalkTargetValue`: water (unless heading home) and sand are worth 10.
    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let world = self.get_entity().world.load();
        if !self.going_home.load(Ordering::Relaxed) && Self::is_water_at(&world, pos) {
            return 10.0;
        }
        if TurtleEggBlock::on_sand(&**world, pos) {
            return 10.0;
        }
        world.get_light_level_dependent_magic_value(pos) - 0.5
    }

    /// Vanilla `Turtle.travelInWater`: a slow swimmer, sinking unless it has a target or is on
    /// its way home and within 20 blocks of it.
    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(living.movement_input.load(), 0.1);
        living.make_move(caller);
        let mut velocity = entity.velocity.load() * 0.9;
        let has_target = self
            .mob_entity
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some();
        if !has_target && (!self.going_home.load(Ordering::Relaxed) || !self.is_near_home(20.0)) {
            velocity.y -= 0.005;
        }
        entity.velocity.store(velocity);
    }

    /// Vanilla `Turtle.isPushedByFluid`.
    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        let sound = if self.is_baby() {
            pumpkin_data::sound::Sound::EntityTurtleShambleBaby
        } else {
            pumpkin_data::sound::Sound::EntityTurtleShamble
        };
        crate::entity::mob::StepSound::Sound(sound, 0.15)
    }

    fn swim_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityTurtleSwim
    }

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
        nbt.put_bool("has_egg", self.has_egg());
        let home = self.home_pos();
        nbt.put(
            "home_pos",
            pumpkin_nbt::tag::NbtTag::IntArray(vec![home.0.x, home.0.y, home.0.z]),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(has_egg) = nbt.get_bool("has_egg").or_else(|| nbt.get_bool("HasEgg")) {
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
