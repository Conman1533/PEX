use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::Block;
use pumpkin_data::Enchantment;
use pumpkin_data::damage::DamageType;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::boundingbox::EntityDimensions;
use pumpkin_util::math::position::BlockPos;

use crate::entity::ai::brain::{Brain, BrainState, memory::PackedMemories};
use crate::entity::item::ItemEntity;
use crate::entity::mob::spawn::SpawnGroupData;
use crate::entity::mob::{MobMetadata, equipment as mob_equipment};
use crate::entity::player::Player;
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, crossbow_attack_mob::CrossbowAttackMob, equipment::RegionalDifficulty},
};
use crate::world::World;

pub mod ai;

pub struct PiglinEntity {
    pub mob_entity: MobEntity,
    pub immune_to_zombification: AtomicBool,
    pub time_in_overworld: AtomicI32,
    pub is_baby: AtomicBool,
    pub cannot_hunt: AtomicBool,
    pub is_charging_crossbow: AtomicBool,
    pub is_dancing: AtomicBool,
    pub inventory: Mutex<Vec<ItemStack>>,
}

impl PiglinEntity {
    pub const CONVERSION_TIME: i32 = 300;
    pub const INVENTORY_SIZE: usize = 8;
    pub const XP_REWARD: u32 = 5;

    pub const ADULT_DIMENSIONS: EntityDimensions = EntityDimensions {
        width: 0.6,
        height: 1.95,
        eye_height: 1.79,
    };
    pub const BABY_DIMENSIONS: EntityDimensions = EntityDimensions {
        width: 0.49,
        height: 0.98,
        eye_height: 0.78,
    };

    /// Vanilla `Piglin`, on the Brain of `PiglinAi` (`ai.rs`).
    pub fn new(entity: Entity) -> Arc<Self> {
        let piglin = Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            immune_to_zombification: AtomicBool::new(false),
            time_in_overworld: AtomicI32::new(0),
            is_baby: AtomicBool::new(false),
            cannot_hunt: AtomicBool::new(false),
            is_charging_crossbow: AtomicBool::new(false),
            is_dancing: AtomicBool::new(false),
            inventory: Mutex::new(Vec::new()),
        });
        piglin.mob_entity.set_can_pick_up_loot(true);
        piglin
    }

    #[must_use]
    pub fn is_immune_to_zombification(&self) -> bool {
        self.immune_to_zombification.load(Ordering::Relaxed)
    }

    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.immune_to_zombification
            .store(immune, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::piglin::DATA_IMMUNE_TO_ZOMBIFICATION, immune);
    }

    #[must_use]
    pub fn is_converting(&self, world: &World) -> bool {
        !self.is_immune_to_zombification()
            && !self.mob_entity.is_no_ai()
            && world.dimension.piglins_zombify
    }

    #[must_use]
    pub fn is_baby(&self) -> bool {
        self.is_baby.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn is_adult(&self) -> bool {
        !self.is_baby()
    }

    pub fn set_baby(&self, baby: bool) {
        self.mob_entity
            .set_baby_flag(&self.is_baby, tracked_data::piglin::DATA_BABY_ID, baby);
        let entity = &self.mob_entity.living_entity.entity;
        if baby {
            entity.entity_dimension.store(Self::BABY_DIMENSIONS);
        } else {
            entity.entity_dimension.store(Self::ADULT_DIMENSIONS);
        }
    }

    #[must_use]
    pub fn is_charging_crossbow(&self) -> bool {
        self.is_charging_crossbow.load(Ordering::Relaxed)
    }

    pub fn set_charging_crossbow(&self, is_charging: bool) {
        self.is_charging_crossbow
            .store(is_charging, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::piglin::DATA_IS_CHARGING_CROSSBOW, is_charging);
    }

    #[must_use]
    pub fn is_dancing(&self) -> bool {
        self.is_dancing.load(Ordering::Relaxed)
    }

    pub fn set_dancing(&self, is_dancing: bool) {
        self.is_dancing.store(is_dancing, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::piglin::DATA_IS_DANCING, is_dancing);
    }

    /// Vanilla `Piglin.canHunt`: the hunt cooldown is the brain's `hunted_recently`.
    #[must_use]
    pub fn can_hunt(&self) -> bool {
        !self.cannot_hunt.load(Ordering::Relaxed)
    }

    pub fn set_cannot_hunt(&self, cannot_hunt: bool) {
        self.cannot_hunt.store(cannot_hunt, Ordering::Relaxed);
    }

    #[must_use]
    pub fn off_hand_item(&self) -> ItemStack {
        self.mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::OFF_HAND)
    }

    #[must_use]
    pub fn is_holding_item_in_off_hand(&self) -> bool {
        !self.off_hand_item().is_empty()
    }

    #[must_use]
    pub fn can_add_to_inventory(&self, item: &ItemStack) -> bool {
        let inv = self
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inv.len() < Self::INVENTORY_SIZE
            || inv.iter().any(|slot| {
                slot.are_items_and_components_equal(item)
                    && slot.item_count < slot.get_max_stack_size()
            })
    }

    pub fn add_to_inventory(&self, item: ItemStack) -> Option<ItemStack> {
        if item.is_empty() {
            return None;
        }
        let mut inv = self
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut remaining = item;
        for slot in inv.iter_mut() {
            if !slot.are_items_and_components_equal(&remaining) {
                continue;
            }
            let space = slot.get_max_stack_size().saturating_sub(slot.item_count);
            let moved = remaining.item_count.min(space);
            slot.increment(moved);
            remaining.decrement(moved);
            if remaining.is_empty() {
                return None;
            }
        }
        if inv.len() < Self::INVENTORY_SIZE {
            inv.push(remaining);
            return None;
        }
        Some(remaining)
    }

    #[must_use]
    pub fn can_replace_current_item_for(&self, new_item: &ItemStack) -> bool {
        let slot = mob_equipment::get_equipment_slot_for_item(new_item);
        let current = self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&slot);
        Mob::can_replace_current_item(self, new_item, &current, &slot)
    }

    /// Vanilla `PiglinAi.holdInOffhand` with `Piglin.holdInOffHand`: whatever was in the off hand
    /// drops, and anything but a gold ingot keeps the piglin from despawning.
    fn hold_in_off_hand(&self, item: ItemStack) {
        if self.is_holding_item_in_off_hand() {
            self.mob_entity
                .living_entity
                .entity
                .spawn_at_location(self.off_hand_item());
        }
        let keep_loaded = !ai::is_barter_currency(&item);
        self.mob_entity
            .set_item_slot_and_drop_when_killed(&EquipmentSlot::OFF_HAND, item);
        if keep_loaded {
            self.mob_entity
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
    }

    fn hold_in_main_hand(&self, item: ItemStack) {
        self.mob_entity
            .set_item_slot_and_drop_when_killed(&EquipmentSlot::MAIN_HAND, item);
        self.mob_entity
            .persistence_required
            .store(true, Ordering::Relaxed);
    }

    fn main_hand_item(&self) -> ItemStack {
        self.mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND)
    }

    /// Vanilla `Mob.aiStep`'s loot pickup with `Piglin.wantsToPickUp` and `pickUpItem`.
    fn pick_up_nearby_items(&self) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        if !self.mob_entity.can_pick_up_loot()
            || !entity.is_alive()
            || living.health.load() <= 0.0
            || living.dead.load(Ordering::Relaxed)
        {
            return;
        }
        let world = entity.world.load();
        if !world.level_info.load().game_rules.mob_griefing {
            return;
        }

        let reach = entity.bounding_box.load().expand(1.0, 0.0, 1.0);
        let candidates = world.get_entities_at_box(&reach);
        self.mob_entity.with_brain(self, |brain| {
            for candidate in &candidates {
                let Some(item) = candidate.get_item_entity() else {
                    continue;
                };
                if item.get_entity().is_alive()
                    && item.get_pickup_delay() == 0
                    && self.wants_item_entity(brain, item)
                {
                    ai::pick_up_item(brain, self, item);
                }
            }
        });
    }

    fn wants_item_entity(&self, brain: &BrainState, item: &ItemEntity) -> bool {
        let stack = item
            .get_item_stack()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        !stack.is_empty() && ai::wants_to_pickup(brain, self, &stack)
    }

    pub fn drop_inventory(&self) {
        let items = self
            .inventory
            .try_lock()
            .map_or_else(|_| Vec::new(), |mut inv| std::mem::take(&mut *inv));
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();
        for item in items {
            if !item.is_empty() {
                let item_entity =
                    ItemEntity::new(Entity::new(world.clone(), pos, &EntityType::ITEM), item);
                world.spawn_entity(Arc::new(item_entity));
            }
        }
    }

    #[must_use]
    pub fn check_piglin_spawn_rules(world: &World, pos: &BlockPos) -> bool {
        let below = BlockPos::new(pos.0.x, pos.0.y - 1, pos.0.z);
        let state = world.get_block_state(&below);
        state.id != Block::NETHER_WART_BLOCK.default_state.id
    }

    /// Vanilla `Piglin.finishConversion`: what it was admiring and its inventory drop first.
    fn convert_to_zombified(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();

        if world.level_info.load().difficulty != pumpkin_util::Difficulty::Peaceful {
            world.play_sound(
                Sound::EntityPiglinConvertedToZombified,
                SoundCategory::Hostile,
                &pos,
            );
        }

        self.mob_entity
            .try_with_brain(self, |brain| ai::cancel_admiring(brain, self));
        self.drop_inventory();

        let zombified = crate::entity::r#type::from_type(
            &EntityType::ZOMBIFIED_PIGLIN,
            pos,
            &world,
            uuid::Uuid::new_v4(),
        );

        let zombified_base = zombified.get_entity();
        zombified_base.set_rotation(entity.yaw.load(), entity.pitch.load());
        zombified_base.head_yaw.store(entity.head_yaw.load());
        zombified_base.velocity.store(entity.velocity.load());

        if let Some(living) = zombified.get_living_entity() {
            living.set_health(self.mob_entity.living_entity.health.load());
        }

        if let Some(custom_name) = &**entity.custom_name.load() {
            zombified_base.set_custom_name(custom_name.clone());
        }

        {
            let src_equip = self
                .mob_entity
                .living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(living) = zombified.get_living_entity() {
                let mut dst_equip = living
                    .entity_equipment
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for (slot, item) in &src_equip.equipment {
                    dst_equip.put(slot, item.clone());
                }
            }
        }

        world.spawn_entity(zombified);
        entity.remove();
    }

    /// Vanilla `Piglin.getTopPassenger`: a baby piglin climbs a hoglin's stack, up to three high.
    fn top_passenger(vehicle: Arc<dyn EntityBase>, counter: i32) -> Arc<dyn EntityBase> {
        if counter == 1 {
            return vehicle;
        }
        let first = vehicle
            .get_entity()
            .passengers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .cloned();
        first.map_or(vehicle, |passenger| {
            Self::top_passenger(passenger, counter - 1)
        })
    }

    /// Vanilla `Entity.stopRiding`.
    pub fn stop_riding(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        if let Some(vehicle) = entity.get_vehicle() {
            vehicle.get_entity().remove_passenger_sync(entity.entity_id);
        }
    }
}

impl Mob for PiglinEntity {
    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(Sound::EntityPiglinStep, 0.15)
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        ai::ambient_sound(self)
    }

    fn mob_metadata(&self) -> Option<MobMetadata> {
        self.is_baby().then(|| MobMetadata::new().baby(true))
    }

    fn spawn_as_baby(&self) -> bool {
        self.set_baby(true);
        true
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn populate_default_equipment_slots(
        &self,
        _world: &Arc<World>,
        _difficulty: &RegionalDifficulty,
    ) {
        if !self.is_baby.load(Ordering::Relaxed) {
            let living = &self.mob_entity.living_entity;
            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let weapon = if rand::random::<f32>() < 0.5 {
                &Item::CROSSBOW
            } else if rand::random_range(0..10) == 0 {
                &Item::GOLDEN_SPEAR
            } else {
                &Item::GOLDEN_SWORD
            };
            equipment.put(&EquipmentSlot::MAIN_HAND, ItemStack::new(1, weapon));

            if rand::random::<f32>() < 0.1 {
                equipment.put(
                    &EquipmentSlot::HEAD,
                    ItemStack::new(1, &Item::GOLDEN_HELMET),
                );
            }
            if rand::random::<f32>() < 0.1 {
                equipment.put(
                    &EquipmentSlot::CHEST,
                    ItemStack::new(1, &Item::GOLDEN_CHESTPLATE),
                );
            }
            if rand::random::<f32>() < 0.1 {
                equipment.put(
                    &EquipmentSlot::LEGS,
                    ItemStack::new(1, &Item::GOLDEN_LEGGINGS),
                );
            }
            if rand::random::<f32>() < 0.1 {
                equipment.put(&EquipmentSlot::FEET, ItemStack::new(1, &Item::GOLDEN_BOOTS));
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        if self.is_immune_to_zombification() {
            entity.set_synced_data(tracked_data::piglin::DATA_IMMUNE_TO_ZOMBIFICATION, true);
        }
        if self.is_baby() {
            entity.set_synced_data(tracked_data::piglin::DATA_BABY_ID, true);
        }
        if self.is_charging_crossbow() {
            entity.set_synced_data(tracked_data::piglin::DATA_IS_CHARGING_CROSSBOW, true);
        }
        if self.is_dancing() {
            entity.set_synced_data(tracked_data::piglin::DATA_IS_DANCING, true);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if self.is_immune_to_zombification() {
            nbt.put_bool("IsImmuneToZombification", true);
        }
        let time_in_overworld = self.time_in_overworld.load(Ordering::Relaxed);
        if time_in_overworld > 0 {
            nbt.put_int("TimeInOverworld", time_in_overworld);
        }
        if self.is_baby() {
            nbt.put_bool("IsBaby", true);
        }
        if !self.can_hunt() {
            nbt.put_bool("CannotHunt", true);
        }
        if self.is_charging_crossbow() {
            nbt.put_bool("IsChargingCrossbow", true);
        }
        if self.is_dancing() {
            nbt.put_bool("IsDancing", true);
        }

        let inv = self
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let items_tag: Vec<NbtTag> = inv
            .iter()
            .filter(|item| !item.is_empty())
            .map(|item| {
                let mut item_nbt = NbtCompound::new();
                item.write_item_stack(&mut item_nbt);
                NbtTag::Compound(item_nbt)
            })
            .collect();
        if !items_tag.is_empty() {
            nbt.put_list("Inventory", items_tag);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(immune) = nbt.get_bool("IsImmuneToZombification") {
            self.set_immune_to_zombification(immune);
        }
        if let Some(time) = nbt.get_int("TimeInOverworld") {
            self.time_in_overworld.store(time, Ordering::Relaxed);
        }
        if let Some(baby) = nbt.get_bool("IsBaby") {
            self.set_baby(baby);
        }
        if let Some(cannot_hunt) = nbt.get_bool("CannotHunt") {
            self.set_cannot_hunt(cannot_hunt);
        }
        if let Some(charging) = nbt.get_bool("IsChargingCrossbow") {
            self.set_charging_crossbow(charging);
        }
        if let Some(dancing) = nbt.get_bool("IsDancing") {
            self.set_dancing(dancing);
        }
        if let Some(inv_list) = nbt.get_list("Inventory") {
            let mut inv = self
                .inventory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inv.clear();
            for tag in inv_list {
                if let Some(compound) = tag.extract_compound()
                    && let Some(stack) = ItemStack::read_item_stack(compound)
                {
                    inv.push(stack);
                }
            }
        }
    }

    /// Vanilla `Piglin.mobInteract`: leashing and the like first, then `PiglinAi.mobInteract`.
    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.mob_entity
            .mob_interact(player, item_stack, self.can_be_leashed())
            || ai::mob_interact(self, player, item_stack)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }

        let world = entity.world.load();
        if self.is_converting(&world) {
            let time = self.time_in_overworld.fetch_add(1, Ordering::Relaxed) + 1;
            if time > Self::CONVERSION_TIME {
                self.convert_to_zombified();
            }
        } else {
            self.time_in_overworld.store(0, Ordering::Relaxed);
        }
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        ai::make_brain(self, packed)
    }

    /// Vanilla `Piglin.customServerAiStep`.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        ai::update_activity(self);
    }

    fn post_tick(&self) {
        self.pick_up_nearby_items();
    }

    /// Vanilla `Piglin.finalizeSpawn`: `PiglinAi.initMemories` puts off the first hunt.
    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        ai::init_memories(self);
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    fn wants_to_pick_up(&self, stack: &ItemStack) -> bool {
        self.mob_entity
            .try_with_brain(self, |brain| self.wants_to_pick_up_in_brain(stack, brain))
            .unwrap_or(false)
    }

    /// Vanilla `Piglin.wantsToPickUp`.
    fn wants_to_pick_up_in_brain(&self, stack: &ItemStack, brain: &BrainState) -> bool {
        self.mob_entity
            .living_entity
            .entity
            .world
            .load()
            .level_info
            .load()
            .game_rules
            .mob_griefing
            && self.mob_entity.can_pick_up_loot()
            && ai::wants_to_pickup(brain, self, stack)
    }

    /// Vanilla `Piglin.startRiding`.
    fn start_riding(&self, vehicle: Arc<dyn EntityBase>) -> bool {
        let vehicle = if self.is_baby() && vehicle.get_entity().entity_type == &EntityType::HOGLIN {
            Self::top_passenger(vehicle, 3)
        } else {
            vehicle
        };
        crate::entity::mob::start_riding_default(&self.mob_entity, &vehicle)
    }

    /// Vanilla `Piglin.hurtServer` (`PiglinAi.wasHurtBy`) and `dropCustomDeathLoot`.
    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if self.mob_entity.living_entity.dead.load(Ordering::Relaxed) {
            self.drop_inventory();
        } else if let Some(attacker) = source {
            ai::was_hurt_by(self, attacker);
        }
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static tag::Tag> {
        (!self.is_baby()).then_some(&tag::Item::MINECRAFT_PIGLIN_PREFERRED_WEAPONS)
    }

    fn can_replace_current_item(
        &self,
        new_item: &ItemStack,
        current_item: &ItemStack,
        slot: &EquipmentSlot,
    ) -> bool {
        if current_item.get_enchantment_level(&Enchantment::BINDING_CURSE) > 0 {
            return false;
        }
        let preferred = self.get_preferred_weapon_type();
        let new_wanted = ai::is_loved_item(new_item)
            || preferred.is_some_and(|weapons| new_item.item.has_tag(weapons));
        let current_wanted = ai::is_loved_item(current_item)
            || preferred.is_some_and(|weapons| current_item.item.has_tag(weapons));
        new_wanted && !current_wanted
            || (new_wanted || !current_wanted)
                && mob_equipment::can_replace_current_item(
                    &self.mob_entity,
                    preferred,
                    new_item,
                    current_item,
                    slot,
                )
    }

    /// Vanilla `Piglin.canUseNonMeleeWeapon` covers crossbows and kinetic weapons; mob spear
    /// attacks wait for G1-06, so a spear piglin melees until then.
    fn can_use_non_melee_weapon(&self, stack: &ItemStack) -> bool {
        stack.item.id == Item::CROSSBOW.id
    }

    fn as_crossbow_attack_mob(&self) -> Option<&dyn CrossbowAttackMob> {
        Some(self)
    }

    fn get_base_experience_reward(&self) -> u32 {
        Self::XP_REWARD
    }
}

impl CrossbowAttackMob for PiglinEntity {
    fn set_charging_crossbow(&self, is_charging: bool) {
        self.set_charging_crossbow(is_charging);
    }

    fn is_charging_crossbow(&self) -> bool {
        self.is_charging_crossbow()
    }
}
