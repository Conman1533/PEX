use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
};

use pumpkin_data::data_component_impl::{EquipmentSlot, EquippableImpl};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity, equipment::is_equippable_in_slot},
    passive::animal::Animal,
    player::Player,
};

/// Vanilla `HappyGhast.canAddPassenger`: four riders at most.
const MAX_PASSENGERS: usize = 4;

/// Represents a Happy Ghast, a passive flying mob.
///
/// Wiki: <https://minecraft.wiki/w/Happy_Ghast>
pub struct HappyGhastEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub server_still_timeout: AtomicI32,
    pub leash_holder_time: AtomicI32,
    pub is_leash_holder: AtomicBool,
    pub stays_still: AtomicBool,
    /// Riders seen last tick, to run vanilla `removePassenger` side effects.
    pub passenger_count: AtomicUsize,
}

impl HappyGhastEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let happy_ghast = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            server_still_timeout: AtomicI32::new(0),
            leash_holder_time: AtomicI32::new(0),
            is_leash_holder: AtomicBool::new(false),
            stays_still: AtomicBool::new(false),
            passenger_count: AtomicUsize::new(0),
        };
        let mob_arc = Arc::new(happy_ghast);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                1,
                Box::new(TemptGoal::with_stop_distance(
                    1.0,
                    |mob, stack| {
                        let tag = if !mob.get_mob_entity().is_wearing_body_armor()
                            && !mob.as_ageable().is_some_and(AgeableMob::is_baby)
                        {
                            &tag::Item::MINECRAFT_HAPPY_GHAST_TEMPT_ITEMS
                        } else {
                            &tag::Item::MINECRAFT_HAPPY_GHAST_FOOD
                        };
                        stack.get_item().has_tag(tag)
                    },
                    false,
                    7.0,
                )),
            );
            goal_selector.add_goal(2, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                3,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(4, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    pub fn set_server_still_timeout(&self, timeout: i32) {
        self.server_still_timeout.store(timeout, Ordering::Relaxed);
        self.sync_stay_still_flag();
    }

    #[must_use]
    pub fn is_on_still_timeout(&self) -> bool {
        self.stays_still.load(Ordering::Relaxed)
            || self.server_still_timeout.load(Ordering::Relaxed) > 0
    }

    fn set_leash_holder(&self, is_leash_holder: bool) {
        self.is_leash_holder
            .store(is_leash_holder, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::IS_LEASH_HOLDER,
            is_leash_holder,
        );
    }

    /// Vanilla `scanPlayerAboveGhast`: a player (not riding a happy ghast) standing on top.
    fn scan_player_above_ghast(&self) -> bool {
        let entity = self.get_entity();
        let bb = entity.bounding_box.load();
        let height = bb.max.y - bb.min.y;
        let (min_x, max_x) = (bb.min.x - 1.0, bb.max.x + 1.0);
        let (min_z, max_z) = (bb.min.z - 1.0, bb.max.z + 1.0);
        let (min_y, max_y) = (bb.max.y - 1.0e-5, bb.max.y + height / 2.0);
        entity.world.load().players.load().iter().any(|player| {
            if player.is_spectator() {
                return false;
            }
            let root = player.get_entity().get_vehicle().map_or_else(
                || player.get_entity().pos.load(),
                |v| v.get_entity().pos.load(),
            );
            if player
                .get_entity()
                .get_vehicle()
                .is_some_and(|v| v.get_entity().entity_type == &EntityType::HAPPY_GHAST)
            {
                return false;
            }
            (min_x..max_x).contains(&root.x)
                && (min_y..max_y).contains(&root.y)
                && (min_z..max_z).contains(&root.z)
        })
    }

    /// The harness's `equip_on_interact` (vanilla `Equippable.equipOnTarget`): into the body
    /// slot of a live adult not already wearing one.
    fn try_equip_harness(&self, item_stack: &mut ItemStack) -> bool {
        let sound = {
            let Some(equippable) = item_stack.get_data_component::<EquippableImpl>() else {
                return false;
            };
            if !equippable.equip_on_interact || *equippable.slot != EquipmentSlot::BODY {
                return false;
            }
            equippable.equip_sound.clone()
        };
        let entity = self.get_entity();
        if !is_equippable_in_slot(entity.entity_type, item_stack, &EquipmentSlot::BODY)
            || self.mob_entity.is_wearing_body_armor()
            || !entity.is_alive()
        {
            return false;
        }
        let harness = item_stack.split(1);
        self.mob_entity
            .set_item_slot_and_drop_when_killed(&EquipmentSlot::BODY, harness);
        entity
            .world
            .load()
            .play_sound_event(&sound, SoundCategory::Neutral, &entity.pos.load());
        true
    }

    /// Vanilla `doPlayerRide` plus the `addPassenger` side effects.
    fn do_player_ride(&self, player: &Arc<Player>) {
        let entity = self.get_entity();
        let count = entity
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        if count >= MAX_PASSENGERS {
            return;
        }
        let world = entity.world.load();
        let (Some(vehicle), Some(passenger)) = (
            world.get_entity_by_id(entity.entity_id),
            world.get_player_by_id(player.entity_id()),
        ) else {
            return;
        };
        if count == 0 {
            entity.play_sound(Sound::EntityHappyGhastHarnessGogglesDown);
        }
        entity.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
        self.passenger_count.store(count + 1, Ordering::Relaxed);
        if !self.scan_player_above_ghast() {
            self.set_server_still_timeout(0);
        } else if self.server_still_timeout.load(Ordering::Relaxed) > 10 {
            self.set_server_still_timeout(10);
        }
    }

    /// The first rider steers once harnessed and not holding still.
    fn player_controller(&self) -> Option<Arc<dyn EntityBase>> {
        if !self.mob_entity.is_wearing_body_armor() || self.is_on_still_timeout() {
            return None;
        }
        self.get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .filter(|passenger| passenger.get_player().is_some())
            .cloned()
    }

    fn sync_stay_still_flag(&self) {
        let stays_still = self.server_still_timeout.load(Ordering::Relaxed) > 0;
        self.stays_still.store(stays_still, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::STAYS_STILL,
            stays_still,
        );
    }
}

impl AgeableMob for HappyGhastEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for HappyGhastEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack
            .item
            .has_tag(&tag::Item::MINECRAFT_HAPPY_GHAST_FOOD)
    }
}

impl Mob for HappyGhastEntity {
    fn get_sound_volume(&self) -> f32 {
        if self
            .get_mob_entity()
            .living_entity
            .entity
            .age
            .load(std::sync::atomic::Ordering::Relaxed)
            < 0
        {
            1.0
        } else {
            4.0
        }
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Silent
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.is_baby() {
            pumpkin_data::sound::Sound::EntityGhastlingAmbient
        } else {
            pumpkin_data::sound::Sound::EntityHappyGhastAmbient
        })
    }

    /// Vanilla `HappyGhast.getAmbientSoundInterval`: six times longer while ridden.
    fn get_ambient_sound_interval(&self) -> i32 {
        let interval = crate::entity::mob::ANIMAL_AMBIENT_SOUND_INTERVAL;
        if self.get_entity().has_passengers() {
            interval * 6
        } else {
            interval
        }
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int(
            "still_timeout",
            self.server_still_timeout.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(timeout) = nbt.get_int("still_timeout") {
            self.set_server_still_timeout(timeout);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `getControllingPassenger`: a player rider steers a harnessed ghast; otherwise
    /// the usual mob rider rule.
    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        if let Some(player) = self.player_controller() {
            return Some(player);
        }
        if self.mob_entity.is_no_ai() {
            return None;
        }
        self.get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .filter(|passenger| passenger.get_mob().is_some() && passenger.can_control_vehicle())
            .cloned()
    }

    /// The steering player's client moves the ghast (vanilla `isControlledByLocalInstance` is
    /// false on the server), so the server skips its own travel step meanwhile.
    fn travel(&self, _caller: &dyn EntityBase) -> bool {
        self.player_controller().is_some()
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        // Vanilla `removePassenger`: hold still for 10 ticks, goggles up once empty.
        let riders = self
            .get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        let previous = self.passenger_count.swap(riders, Ordering::Relaxed);
        if riders < previous {
            self.set_server_still_timeout(10);
            if riders == 0 {
                self.get_entity()
                    .play_sound(Sound::EntityHappyGhastHarnessGogglesUp);
            }
        }

        let leash_time = self.leash_holder_time.load(Ordering::Relaxed);
        if leash_time > 0 {
            self.leash_holder_time.fetch_sub(1, Ordering::Relaxed);
        }
        self.set_leash_holder(leash_time > 0);

        let still_timeout = self.server_still_timeout.load(Ordering::Relaxed);
        if still_timeout > 0 {
            let entity = self.get_entity();
            if entity.tick_count.load(Ordering::Relaxed) > 60 {
                self.server_still_timeout.fetch_sub(1, Ordering::Relaxed);
            }
            self.sync_stay_still_flag();
        }

        // Continuous healing
        let entity = self.get_entity();
        if entity.is_alive() {
            let living = &self.mob_entity.living_entity;
            let current_health = living.health.load();
            let max_health = living.get_max_health();
            if current_health < max_health {
                let world = entity.world.load();
                let ticks = world.get_world_age();
                let heal_interval = 600;
                if ticks % heal_interval == 0 {
                    living.set_health(current_health + 1.0);
                }
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::happy_ghast::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::IS_LEASH_HOLDER,
            self.is_leash_holder.load(Ordering::Relaxed),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::STAYS_STILL,
            self.stays_still.load(Ordering::Relaxed),
        );
    }

    /// Vanilla `HappyGhast.mobInteract`: a harness goes on, then a harnessed ghast is ridden
    /// unless the player sneaks. Leads and name tags keep their own handling.
    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.is_baby() {
            return self.animal_interact(player, item_stack);
        }
        let item = item_stack.get_item();
        if item == &Item::LEAD || item == &Item::NAME_TAG {
            return self.animal_interact(player, item_stack);
        }
        if !item_stack.is_empty() && self.try_equip_harness(item_stack) {
            return true;
        }
        if self.mob_entity.is_wearing_body_armor() && !player.get_entity().is_sneaking() {
            self.do_player_ride(player);
            return true;
        }

        self.animal_interact(player, item_stack)
    }
}
