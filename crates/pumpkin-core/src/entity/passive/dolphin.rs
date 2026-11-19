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
        Controls, Goal, look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
        melee_attack::MeleeAttackGoal, revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks,
        try_find_water::TryFindWaterGoal, wander_around::WanderAroundGoal,
    },
    ai::pathfinder::{Navigator, NavigatorGoal},
    ai::util::default_random_pos,
    mob::{Mob, MobEntity},
    player::Player,
};

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
            moistness_level: AtomicI32::new(2400),
            treasure_pos: Mutex::new(None),
        };
        let mob_arc = Arc::new(dolphin);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(TryFindWaterGoal));
            // Not in vanilla: it stands in for `BreathAirGoal`, which is not ported yet.
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(DolphinSwimToTreasureGoal { stuck: false }));
            goal_selector.add_goal(4, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(4, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                5,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(6, Box::new(MeleeAttackGoal::new(1.2, true)));
        };

        {
            let target_selector = &mob_arc.mob_entity.target_selector;

            target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
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

impl Mob for DolphinEntity {
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
            self.moistness_level.store(moistness, Ordering::Relaxed);
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
