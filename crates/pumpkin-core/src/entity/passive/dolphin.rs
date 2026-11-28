use std::sync::{
    Arc, Mutex, PoisonError, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::control::{
        smooth_swimming_look_control::SmoothSwimmingLookControl,
        smooth_swimming_move_control::SmoothSwimmingMoveControl,
    },
    ai::goal::{
        Controls, Goal, avoid_entity::AvoidEntityGoal, breath_air::BreathAirGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
        melee_attack::MeleeAttackGoal, random_swimming::RandomSwimmingGoal, revenge::RevengeGoal,
        to_goal_ticks, try_find_water::TryFindWaterGoal,
    },
    ai::pathfinder::{Navigator, NavigatorGoal},
    ai::util::default_random_pos,
    mob::{Mob, MobEntity},
    player::Player,
};

/// Vanilla `Dolphin.TOTAL_AIR_SUPPLY`.
pub const TOTAL_AIR_SUPPLY: i32 = 4800;
/// Vanilla `Dolphin.TOTAL_MOISTNESS_LEVEL`.
const TOTAL_MOISTNESS_LEVEL: i32 = 2400;
/// Vanilla `LivingEntity.shouldTakeDrowningDamage`: air at or below this hurts.
const DROWNING_AIR: i32 = -20;

pub struct DolphinEntity {
    pub mob_entity: MobEntity,
    pub got_fish: AtomicBool,
    pub moistness_level: AtomicI32,
    /// Vanilla `treasurePos`: the structure a fed dolphin leads players to.
    pub treasure_pos: Mutex<Option<BlockPos>>,
}

impl DolphinEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        // Vanilla `Entity` starts with `getMaxAirSupply()`.
        mob_entity
            .living_entity
            .entity
            .air_supply
            .store(TOTAL_AIR_SUPPLY, Ordering::Relaxed);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(SmoothSwimmingMoveControl::new(85, 10, 0.02, 0.1, true));
        *mob_entity
            .look_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(SmoothSwimmingLookControl::new(10));
        *mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::water_bound(true);
        let dolphin = Self {
            mob_entity,
            got_fish: AtomicBool::new(false),
            moistness_level: AtomicI32::new(TOTAL_MOISTNESS_LEVEL),
            treasure_pos: Mutex::new(None),
        };
        let mob_arc = Arc::new(dolphin);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(BreathAirGoal::new()));
            goal_selector.add_goal(0, Box::new(TryFindWaterGoal));
            goal_selector.add_goal(1, Box::new(DolphinSwimToTreasureGoal { stuck: false }));
            goal_selector.add_goal(2, Box::new(DolphinSwimWithPlayerGoal::new(4.0)));
            goal_selector.add_goal(4, Box::new(RandomSwimmingGoal::new(1.0, 10)));
            goal_selector.add_goal(4, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                5,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(5, Box::new(DolphinJumpGoal::new(10)));
            goal_selector.add_goal(6, Box::new(MeleeAttackGoal::new(1.2, true)));
            // Vanilla `AvoidEntityGoal<>(Guardian.class, ..)` takes elder guardians too.
            goal_selector.add_goal(
                9,
                Box::new(AvoidEntityGoal::new(&EntityType::GUARDIAN, 8.0, 1.0, 1.0)),
            );
            goal_selector.add_goal(
                9,
                Box::new(AvoidEntityGoal::new(
                    &EntityType::ELDER_GUARDIAN,
                    8.0,
                    1.0,
                    1.0,
                )),
            );
        };

        {
            let target_selector = &mob_arc.mob_entity.target_selector;

            // Vanilla `HurtByTargetGoal(this, Guardian.class).setAlertOthers()`: guardians can't
            // make it angry.
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type == &EntityType::GUARDIAN
                                || entity_type == &EntityType::ELDER_GUARDIAN
                        })
                        .alerting_others(),
                ),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn got_fish(&self) -> bool {
        self.got_fish.load(Ordering::Relaxed)
    }

    pub fn set_got_fish(&self, val: bool) {
        self.got_fish.store(val, Ordering::Relaxed);
    }

    #[must_use]
    pub fn moistness_level(&self) -> i32 {
        self.moistness_level.load(Ordering::Relaxed)
    }

    pub fn set_moistness_level(&self, level: i32) {
        self.moistness_level.store(level, Ordering::Relaxed);
        self.get_entity().set_synced_data(
            pumpkin_data::tracked_data::dolphin::MOISTNESS_LEVEL,
            pumpkin_protocol::codec::var_int::VarInt(level),
        );
    }

    /// Vanilla `LivingEntity.baseTick`'s air: with its eyes under water (outside a bubble column)
    /// a dolphin loses one air a tick and takes 2 drowning damage each time it reaches -20.
    /// `increaseAirSupply` gives a dolphin its whole supply back at once, so out of the water, or
    /// with water breathing, it is full again. `Dolphin.handleAirSupply` is empty, so it doesn't
    /// also run out of air on land like other water creatures.
    fn tick_air(&self) {
        use pumpkin_data::effect::StatusEffect;

        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        if !entity.is_alive() || living.is_dead_or_dying() {
            return;
        }
        let air = entity.air_supply.load(Ordering::Relaxed);
        let world = entity.world.load();
        let pos = entity.pos.load();
        let eye = BlockPos::floored(pos.x, entity.get_eye_y(), pos.z);
        if entity.is_submerged_in_water()
            && world.get_block(&eye) != &pumpkin_data::Block::BUBBLE_COLUMN
        {
            let water_breathing = living.has_effect(&StatusEffect::WATER_BREATHING);
            let conduit_power = living.has_effect(&StatusEffect::CONDUIT_POWER);
            let nautilus = living.has_effect(&StatusEffect::BREATH_OF_THE_NAUTILUS);
            if !(water_breathing || conduit_power || nautilus) {
                entity.set_air_supply(air - 1);
                if air - 1 <= DROWNING_AIR {
                    entity.set_air_supply(0);
                    world.send_entity_status(
                        entity,
                        EntityStatus::DrownParticles,
                        ActorEventID::None,
                    );
                    self.damage(self, 2.0, pumpkin_data::damage::DamageType::DROWN);
                }
            } else if air < TOTAL_AIR_SUPPLY && (!nautilus || water_breathing || conduit_power) {
                entity.set_air_supply(TOTAL_AIR_SUPPLY);
            }
        } else if air < TOTAL_AIR_SUPPLY {
            entity.set_air_supply(TOTAL_AIR_SUPPLY);
        }
    }

    #[must_use]
    pub fn treasure_pos(&self) -> Option<BlockPos> {
        *self
            .treasure_pos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Vanilla `findNearestMapStructure(#dolphin_located, pos, 50, false)`: the closest ocean ruin
/// or shipwreck.
fn find_dolphin_treasure(mob: &dyn Mob) -> Option<BlockPos> {
    use pumpkin_data::structures::{StructureKeys, StructureSet};
    use pumpkin_world::generation::generator::structure_finder::find_nearest_structure_start;

    let entity = mob.get_entity();
    let origin = entity.block_pos.load();
    let generator = entity.world.load().level.world_gen();
    [
        (
            "ocean_ruins",
            [StructureKeys::OceanRuinCold, StructureKeys::OceanRuinWarm],
        ),
        (
            "shipwrecks",
            [StructureKeys::Shipwreck, StructureKeys::ShipwreckBeached],
        ),
    ]
    .into_iter()
    .filter_map(|(set, keys)| {
        find_nearest_structure_start(origin, StructureSet::get(set)?, &keys, 50, &generator)
            .map(|(pos, _)| pos)
    })
    .min_by_key(|pos| {
        let dx = i64::from(pos.0.x - origin.0.x);
        let dz = i64::from(pos.0.z - origin.0.z);
        dx * dx + dz * dz
    })
}

/// Vanilla `Dolphin.DolphinSwimToTreasureGoal`: a dolphin fed fish swims towards the nearest
/// ocean ruin or shipwreck.
struct DolphinSwimToTreasureGoal {
    stuck: bool,
}

impl DolphinSwimToTreasureGoal {
    fn dolphin(mob: &dyn Mob) -> Option<&DolphinEntity> {
        mob.cast_any().downcast_ref::<DolphinEntity>()
    }

    /// Vanilla `BlockPos.containing(treasure.x, dolphin.y, treasure.z).closerToCenterThan(.., 4)`.
    fn close_to(mob: &dyn Mob, treasure: BlockPos) -> bool {
        let pos = mob.get_entity().pos.load();
        let center = Vector3::new(
            f64::from(treasure.0.x) + 0.5,
            pos.y.floor() + 0.5,
            f64::from(treasure.0.z) + 0.5,
        );
        center.squared_distance_to_vec(&pos) < 16.0
    }

    fn has_air(mob: &dyn Mob) -> bool {
        mob.get_entity().air_supply.load(Ordering::Relaxed) >= 100
    }

    fn signal(mob: &dyn Mob) {
        let entity = mob.get_entity();
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::DolphinLookingForTreasure,
            ActorEventID::TreasureHunt,
        );
    }
}

impl Goal for DolphinSwimToTreasureGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::dolphin(mob).is_some_and(DolphinEntity::got_fish) && Self::has_air(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        Self::dolphin(mob)
            .and_then(DolphinEntity::treasure_pos)
            .is_some_and(|treasure| {
                !Self::close_to(mob, treasure) && !self.stuck && Self::has_air(mob)
            })
    }

    fn can_stop(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(dolphin) = Self::dolphin(mob) else {
            return;
        };
        self.stuck = false;
        mob.get_mob_entity().steered().navigator().stop();
        let treasure = find_dolphin_treasure(mob);
        *dolphin
            .treasure_pos
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = treasure;
        if treasure.is_some() {
            Self::signal(mob);
        } else {
            self.stuck = true;
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let Some(dolphin) = Self::dolphin(mob) else {
            return;
        };
        if self.stuck
            || dolphin
                .treasure_pos()
                .is_none_or(|treasure| Self::close_to(mob, treasure))
        {
            dolphin.set_got_fish(false);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(treasure) = Self::dolphin(mob).and_then(DolphinEntity::treasure_pos) else {
            return;
        };
        if !mob.is_navigator_idle() {
            return;
        }
        let world = mob.get_entity().world.load();
        let target = treasure.to_centered_f64();
        let mut next =
            default_random_pos::get_pos_towards(mob, 16, 1, target, std::f64::consts::FRAC_PI_8)
                .or_else(|| {
                    default_random_pos::get_pos_towards(
                        mob,
                        8,
                        4,
                        target,
                        std::f64::consts::FRAC_PI_2,
                    )
                });
        if let Some(pos) = next
            && !TryFindWaterGoal::is_water(&world, &BlockPos::floored(pos.x, pos.y, pos.z))
        {
            next =
                default_random_pos::get_pos_towards(mob, 8, 5, target, std::f64::consts::FRAC_PI_2);
        }
        let Some(next) = next else {
            self.stuck = true;
            return;
        };
        let mob_entity = mob.get_mob_entity();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .look_at_with_range(
                next.x,
                next.y,
                next.z,
                mob.get_max_head_rotation() + 20.0,
                mob.get_max_look_pitch_change(),
            );
        let pos = mob.get_entity().pos.load();
        mob_entity
            .steered()
            .navigator()
            .set_progress(NavigatorGoal::new(pos, next, 1.3));
        if mob.get_random().random_range(0..to_goal_ticks(80)) == 0 {
            Self::signal(mob);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// Vanilla `Entity.getMotionDirection` (`Direction.fromYRot`) as a step in x and z.
fn motion_step(mob: &dyn Mob) -> (i32, i32) {
    let yaw = mob.get_entity().yaw.load();
    match ((f64::from(yaw) / 90.0 + 0.5).floor() as i32) & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}

/// Vanilla `DolphinJumpGoal`: now and then a dolphin with clear water ahead and open air above
/// it leaps out of the water.
struct DolphinJumpGoal {
    interval: i32,
    breached: bool,
}

impl DolphinJumpGoal {
    const STEPS_TO_CHECK: [i32; 6] = [0, 1, 4, 5, 6, 7];

    const fn new(interval: i32) -> Self {
        Self {
            interval: to_goal_ticks(interval),
            breached: false,
        }
    }

    fn water_is_clear(world: &crate::world::World, pos: &BlockPos) -> bool {
        let (fluid, _) = world.get_fluid_and_fluid_state(pos);
        fluid.has_tag(&tag::Fluid::MINECRAFT_WATER)
            && !world
                .get_block(pos)
                .has_tag(&tag::Block::MINECRAFT_BLOCKS_DOLPHIN_JUMP)
    }

    fn surface_is_clear(world: &crate::world::World, pos: &BlockPos) -> bool {
        world.get_block_state(&pos.up()).is_air()
            && world.get_block_state(&pos.up_height(2)).is_air()
    }
}

impl Goal for DolphinJumpGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_random().random_range(0..self.interval) != 0 {
            return false;
        }
        let (step_x, step_z) = motion_step(mob);
        let entity = mob.get_entity();
        let world = entity.world.load();
        let origin = entity.block_pos.load();
        Self::STEPS_TO_CHECK.iter().all(|&step| {
            let pos = BlockPos::new(
                origin.0.x + step_x * step,
                origin.0.y,
                origin.0.z + step_z * step,
            );
            Self::water_is_clear(&world, &pos) && Self::surface_is_clear(&world, &pos)
        })
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        let yd = entity.velocity.load().y;
        let pitch = entity.pitch.load();
        (yd * yd >= 0.03 || pitch == 0.0 || pitch.abs() >= 10.0 || !entity.is_in_water())
            && !entity.on_ground.load(Ordering::Relaxed)
    }

    fn can_stop(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        let (step_x, step_z) = motion_step(mob);
        let entity = mob.get_entity();
        entity.velocity.store(
            entity.velocity.load()
                + Vector3::new(f64::from(step_x) * 0.6, 0.7, f64::from(step_z) * 0.6),
        );
        entity.velocity_dirty.store(true, Ordering::Relaxed);
        mob.stop_navigation();
    }

    fn stop(&mut self, mob: &dyn Mob) {
        mob.get_entity().pitch.store(0.0);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        // As in vanilla, `breached` is never reset, so only the first leap plays the sound.
        let already_breached = self.breached;
        if !already_breached {
            let world = entity.world.load();
            let (fluid, _) = world.get_fluid_and_fluid_state(&entity.block_pos.load());
            self.breached = fluid.has_tag(&tag::Fluid::MINECRAFT_WATER);
        }
        if self.breached && !already_breached {
            let world = entity.world.load();
            world.play_sound(
                Sound::EntityDolphinJump,
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
        }
        let movement = entity.velocity.load();
        let pitch = entity.pitch.load();
        if movement.y * movement.y < 0.03 && pitch != 0.0 {
            entity
                .pitch
                .store(pitch + 0.2 * pumpkin_util::math::wrap_degrees(0.0 - pitch));
        } else if movement.length() > 1.0e-5 {
            let horizontal = movement.horizontal_length();
            entity
                .pitch
                .store((-movement.y).atan2(horizontal).to_degrees() as f32);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::JUMP
    }
}

/// Vanilla `Dolphin.DolphinSwimWithPlayerGoal`: a dolphin swims alongside a swimming player within
/// 10 blocks and gives them Dolphin's Grace.
struct DolphinSwimWithPlayerGoal {
    speed: f64,
    player: Option<Arc<Player>>,
}

impl DolphinSwimWithPlayerGoal {
    const fn new(speed: f64) -> Self {
        Self {
            speed,
            player: None,
        }
    }

    fn give_grace(player: &Player) {
        player.add_effect(pumpkin_data::potion::Effect {
            effect_type: &pumpkin_data::effect::StatusEffect::DOLPHINS_GRACE,
            duration: 100,
            amplifier: 0,
            ambient: false,
            show_particles: true,
            show_icon: true,
            blend: false,
        });
    }

    fn is_target(mob: &dyn Mob, player: &Player) -> bool {
        mob.get_mob_entity()
            .target
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|target| target.get_entity().entity_id == player.get_entity().entity_id)
    }
}

impl Goal for DolphinSwimWithPlayerGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        let world = entity.world.load();
        // Vanilla `TargetingConditions.forNonCombat().range(10.0).ignoreLineOfSight()`.
        self.player = world.get_nearest_player(entity.pos.load(), 10.0, |player| {
            !player.is_spectator() && player.get_entity().is_alive()
        });
        self.player
            .as_ref()
            .is_some_and(|player| player.is_swimming() && !Self::is_target(mob, player))
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let pos = mob.get_entity().pos.load();
        self.player.as_ref().is_some_and(|player| {
            player.is_swimming()
                && player.get_entity().pos.load().squared_distance_to_vec(&pos) < 256.0
        })
    }

    fn start(&mut self, _mob: &dyn Mob) {
        if let Some(player) = &self.player {
            Self::give_grace(player);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.player = None;
        mob.stop_navigation();
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let player_entity = player.get_entity();
        let target = player_entity.pos.load();
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .look_at_with_range(
                target.x,
                player_entity.get_eye_y(),
                target.z,
                mob.get_max_head_rotation() + 20.0,
                mob.get_max_look_pitch_change(),
            );
        let pos = mob.get_entity().pos.load();
        if pos.squared_distance_to_vec(&target) < 6.25 {
            mob.stop_navigation();
        } else {
            mob.navigate_to(pos, target, self.speed);
        }
        if player.is_swimming() && mob.get_random().random_range(0..6) == 0 {
            Self::give_grace(&player);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

impl Mob for DolphinEntity {
    /// Vanilla `Dolphin.getMaxHeadYRot`.
    fn get_max_head_rotation(&self) -> f32 {
        1.0
    }

    /// Vanilla `Dolphin.getMaxHeadXRot`.
    fn get_max_look_pitch_change(&self) -> f32 {
        1.0
    }

    /// Vanilla `Dolphin.tick` runs the base tick inside `super.tick()` and only then fills a
    /// dolphin without AI back up, so one without AI always ends the tick with its whole supply.
    /// PEX runs `mob_tick` before the base tick, so the refill has to come after `tick_air` here.
    fn base_tick(&self) {
        self.tick_air();
        if self.mob_entity.is_no_ai() {
            self.get_entity().set_air_supply(TOTAL_AIR_SUPPLY);
        }
    }

    /// Vanilla `Dolphin.tick`: without AI it never runs out of air (see `base_tick`). Otherwise
    /// it keeps moist in water or rain and dries out elsewhere, taking 1 damage a tick once its
    /// moistness is gone, and flops about on the ground.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = self.get_entity();
        if self.mob_entity.is_no_ai() {
            return;
        }
        if entity.is_in_water_or_rain() {
            self.set_moistness_level(TOTAL_MOISTNESS_LEVEL);
            return;
        }
        let moistness = self.moistness_level() - 1;
        self.set_moistness_level(moistness);
        if moistness <= 0 {
            self.damage(self, 1.0, pumpkin_data::damage::DamageType::DRY_OUT);
        }
        if entity.on_ground.load(Ordering::Relaxed) {
            let dx = f64::from((self.get_random().random::<f32>() * 2.0 - 1.0) * 0.2);
            let dz = f64::from((self.get_random().random::<f32>() * 2.0 - 1.0) * 0.2);
            let yaw = self.get_random().random::<f32>() * 360.0;
            entity
                .velocity
                .store(entity.velocity.load() + Vector3::new(dx, 0.5, dz));
            entity.yaw.store(yaw);
            entity.on_ground.store(false, Ordering::Relaxed);
            entity.velocity_dirty.store(true, Ordering::Relaxed);
        }
    }

    /// Vanilla `Dolphin.finalizeSpawn`: a full air supply and a level head.
    fn finalize_spawn(
        &self,
        _world: &Arc<crate::world::World>,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        self.mob_entity.finalize_spawn_base();
        let entity = self.get_entity();
        entity.set_air_supply(TOTAL_AIR_SUPPLY);
        entity.pitch.store(0.0);
        group_data
    }

    fn swim_splash_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityDolphinSplash
    }

    fn swim_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityDolphinSwim
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.get_entity().is_in_water() {
            pumpkin_data::sound::Sound::EntityDolphinAmbientWater
        } else {
            pumpkin_data::sound::Sound::EntityDolphinAmbient
        })
    }

    /// Vanilla `WaterAnimal.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        crate::entity::mob::ANIMAL_AMBIENT_SOUND_INTERVAL
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(
            living.movement_input.load(),
            f64::from(living.get_speed()),
        );
        living.make_move(caller);
        let mut velocity = entity.velocity.load() * 0.9;
        if self
            .mob_entity
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            velocity.y -= 0.005;
        }
        entity.velocity.store(velocity);
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("GotFish", self.got_fish());
        nbt.put_int("Moistness", self.moistness_level.load(Ordering::Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(got_fish) = nbt.get_bool("GotFish") {
            self.set_got_fish(got_fish);
        }
        if let Some(moistness) = nbt.get_int("Moistness") {
            self.set_moistness_level(moistness);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();
        if item.has_tag(&tag::Item::MINECRAFT_FISHES) {
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            self.set_got_fish(true);
            self.mob_entity.living_entity.heal(2.0);

            let entity = self.get_entity();
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.spawn_particle(
                pos + Vector3::new(0.0, f64::from(entity.height()), 0.0),
                Vector3::new(0.5, 0.5, 0.5),
                1.0,
                7,
                Particle::HappyVillager,
            );
            world.play_sound(Sound::EntityDolphinEat, SoundCategory::Neutral, &pos);
            return true;
        }

        false
    }
}
