use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::brain::{
        Brain, BrainTick,
        memory::{PackedMemories, types},
    },
    custom_sound::CustomSound,
    item::ItemEntity,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

mod ai;

pub const ARMADILLO_BABY_START_AGE: i32 = -48000;
pub const SCARE_CHECK_INTERVAL: i32 = 80;
pub const SCARE_DISTANCE_HORIZONTAL: f64 = 7.0;
pub const SCARE_DISTANCE_VERTICAL: f64 = 2.0;

fn pick_next_scute_drop_time() -> i32 {
    let rand_ticks = (rand::random::<u32>() % 6000) as i32;
    rand_ticks + 6000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ArmadilloState {
    #[default]
    Idle = 0,
    Rolling = 1,
    Scared = 2,
    Unrolling = 3,
}

impl ArmadilloState {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Rolling,
            2 => Self::Scared,
            3 => Self::Unrolling,
            _ => Self::Idle,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub const fn is_threatened(self) -> bool {
        !matches!(self, Self::Idle)
    }

    #[must_use]
    pub const fn animation_duration(self) -> u64 {
        match self {
            Self::Idle => 0,
            Self::Rolling => 10,
            Self::Scared => 50,
            Self::Unrolling => 30,
        }
    }

    #[must_use]
    pub const fn should_hide_in_shell(self, ticks_in_state: u64) -> bool {
        match self {
            Self::Idle => false,
            Self::Rolling => ticks_in_state > 5,
            Self::Scared => true,
            Self::Unrolling => ticks_in_state < 26,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Rolling => "rolling",
            Self::Scared => "scared",
            Self::Unrolling => "unrolling",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "rolling" => Self::Rolling,
            "scared" => Self::Scared,
            "unrolling" => Self::Unrolling,
            _ => Self::Idle,
        }
    }
}

/// Represents an Armadillo, a passive entity that can roll into a ball when threatened.
///
/// Wiki: <https://minecraft.wiki/w/Armadillo>
pub struct ArmadilloEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub state: AtomicI32,
    pub in_state_ticks: AtomicU64,
    pub scute_time: AtomicI32,
    /// Set when the hurt reaction already ran before the hurt sound.
    reacted_to_hurt: AtomicBool,
}

impl ArmadilloEntity {
    /// Vanilla `Armadillo`, on the Brain of `ArmadilloAi` (`ai.rs`).
    pub fn new(entity: Entity) -> Arc<Self> {
        Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            ageable_data: AgeableData::default(),
            state: AtomicI32::new(ArmadilloState::Idle.id()),
            in_state_ticks: AtomicU64::new(0),
            scute_time: AtomicI32::new(pick_next_scute_drop_time()),
            reacted_to_hurt: AtomicBool::new(false),
        })
    }

    #[must_use]
    pub fn get_state(&self) -> ArmadilloState {
        ArmadilloState::from_id(self.state.load(Ordering::Relaxed))
    }

    pub fn switch_to_state(&self, state: ArmadilloState) {
        self.state.store(state.id(), Ordering::Relaxed);
        self.in_state_ticks.store(0, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::armadillo::ARMADILLO_STATE,
            VarInt(state.id()),
        );
    }

    #[must_use]
    pub fn is_scared(&self) -> bool {
        self.get_state() != ArmadilloState::Idle
    }

    #[must_use]
    pub fn should_hide_in_shell(&self) -> bool {
        self.get_state()
            .should_hide_in_shell(self.in_state_ticks.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn should_switch_to_scared_state(&self) -> bool {
        self.get_state() == ArmadilloState::Rolling
            && self.in_state_ticks.load(Ordering::Relaxed)
                > ArmadilloState::Rolling.animation_duration()
    }

    /// Vanilla `PathfinderMob.isPanicking`: the brain's `is_panicking` memory. While the
    /// armadillo's own brain ticks, its behaviours read the memory from `BrainTick` instead.
    #[must_use]
    pub fn is_panicking(&self) -> bool {
        self.mob_entity
            .try_with_brain(self, |brain| {
                brain.has_memory_value(types::IS_PANICKING.id())
            })
            .unwrap_or(false)
    }

    #[must_use]
    pub fn can_stay_rolled_up(&self) -> bool {
        self.can_stay_rolled_up_with(self.is_panicking())
    }

    /// Vanilla `Armadillo.canStayRolledUp`, given whether it is panicking.
    #[must_use]
    pub fn can_stay_rolled_up_with(&self, panicking: bool) -> bool {
        let entity = self.get_entity();
        !panicking
            && !entity.is_in_water()
            && !entity.touching_lava.load(Ordering::Relaxed)
            && !entity.is_leashed()
            && !entity.has_vehicle()
            && !entity.has_passengers()
    }

    /// Vanilla `Entity.playSound(sound)`: full volume, normal pitch.
    fn play_sound(&self, sound: Sound) {
        let entity = self.get_entity();
        entity.world.load().play_sound_fine(
            sound,
            SoundCategory::Neutral,
            &entity.pos.load(),
            1.0,
            1.0,
        );
    }

    pub fn roll_up(&self) {
        if !self.is_scared() {
            self.mob_entity.stop_in_place();
            self.reset_love();
            self.mob_entity
                .living_entity
                .make_sound(Sound::EntityArmadilloRoll);
            self.switch_to_state(ArmadilloState::Rolling);
        }
    }

    pub fn roll_out(&self) {
        if self.is_scared() {
            self.mob_entity
                .living_entity
                .make_sound(Sound::EntityArmadilloUnrollFinish);
            self.switch_to_state(ArmadilloState::Idle);
        }
    }

    pub fn brush_off_scute(&self, player: &Player, item_stack: &mut ItemStack) -> bool {
        if self.is_baby() || !self.get_entity().is_alive() {
            return false;
        }
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        let item_entity = Arc::new(ItemEntity::new(
            Entity::new(world.clone(), pos, &EntityType::ITEM),
            ItemStack::new(1, &Item::ARMADILLO_SCUTE),
        ));
        world.spawn_entity(item_entity);
        world.play_sound(Sound::EntityArmadilloBrush, SoundCategory::Neutral, &pos);
        if player.gamemode.load() != pumpkin_util::GameMode::Creative {
            let _ = item_stack.damage_item(16);
        }
        true
    }

    /// Vanilla `Armadillo.actuallyHurt`.
    fn react_to_hurt(&self, damage_type: DamageType, attacker_is_living: bool) {
        if self.mob_entity.is_no_ai() || self.mob_entity.living_entity.is_dead_or_dying() {
            return;
        }
        if attacker_is_living {
            self.mob_entity.try_with_brain(self, |brain| {
                brain.set_with_expiry(
                    types::DANGER_DETECTED_RECENTLY,
                    true,
                    i64::from(SCARE_CHECK_INTERVAL),
                );
            });
            if self.can_stay_rolled_up() {
                self.roll_up();
            }
        } else if damage_type.has_tag(&tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES) {
            self.roll_out();
        }
    }

    /// Vanilla `Armadillo.isScaredBy`.
    pub fn is_scared_by(&self, living_entity: &dyn EntityBase) -> bool {
        let target = living_entity.get_entity();
        if !self
            .get_entity()
            .bounding_box
            .load()
            .expand(
                SCARE_DISTANCE_HORIZONTAL,
                SCARE_DISTANCE_VERTICAL,
                SCARE_DISTANCE_HORIZONTAL,
            )
            .intersects(&target.bounding_box.load())
        {
            return false;
        }
        if target
            .entity_type
            .has_tag(&tag::EntityType::MINECRAFT_UNDEAD)
        {
            return true;
        }
        if self.mob_entity.living_entity.get_last_hurt_by_id() == Some(target.entity_id) {
            return true;
        }
        if target.entity_type == &EntityType::PLAYER {
            return !living_entity.is_spectator()
                && (target.is_sprinting() || target.has_vehicle());
        }
        false
    }
}

/// The `armadillo_scare_detected` sensor's mob test.
pub(crate) fn is_scared_by_mob(mob: &dyn Mob, entity: &dyn EntityBase) -> bool {
    mob.cast_any()
        .downcast_ref::<ArmadilloEntity>()
        .is_some_and(|armadillo| armadillo.is_scared_by(entity))
}

/// The `armadillo_scare_detected` sensor's ready test, `canStayRolledUp`.
pub(crate) fn can_stay_rolled_up_in_brain(tick: &BrainTick<'_>) -> bool {
    let panicking = tick.state.has_memory_value(types::IS_PANICKING.id());
    tick.mob
        .cast_any()
        .downcast_ref::<ArmadilloEntity>()
        .is_some_and(|armadillo| armadillo.can_stay_rolled_up_with(panicking))
}

impl CustomSound for ArmadilloEntity {
    /// Vanilla `Armadillo.getHurtSound`: muffled while curled up.
    fn hurt_sound(&self) -> Option<Sound> {
        Some(if self.is_scared() {
            Sound::EntityArmadilloHurtReduced
        } else {
            Sound::EntityArmadilloHurt
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        Some(Sound::EntityArmadilloDeath)
    }

    fn sound_category(&self) -> SoundCategory {
        SoundCategory::Neutral
    }

    /// Vanilla rolls up in `actuallyHurt`, before the hurt sound picks the muffled variant.
    fn on_hurt_sound(&self) {
        let living = &self.mob_entity.living_entity;
        let attacker_is_living = living.get_last_hurt_by_id().is_some_and(|id| {
            self.get_entity()
                .world
                .load()
                .get_entity_by_id(id)
                .is_some_and(|attacker| attacker.get_living_entity().is_some())
        });
        if let Some(damage_type) = living.get_last_damage_type() {
            self.react_to_hurt(damage_type, attacker_is_living);
            self.reacted_to_hurt.store(true, Ordering::Relaxed);
        }
    }
}

impl AgeableMob for ArmadilloEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn get_baby_start_age(&self) -> i32 {
        ARMADILLO_BABY_START_AGE
    }
}

impl Animal for ArmadilloEntity {
    /// Vanilla `Armadillo.canFallInLove`: not while curled up.
    fn can_fall_in_love(&self) -> bool {
        self.get_mob_entity().love_ticks.load(Ordering::Relaxed) <= 0 && !self.is_scared()
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack
            .item
            .has_tag(&tag::Item::MINECRAFT_ARMADILLO_FOOD)
    }

    fn play_eating_sound(&self) {
        self.mob_entity
            .living_entity
            .make_sound(Sound::EntityArmadilloEat);
    }
}

impl Mob for ArmadilloEntity {
    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(pumpkin_data::sound::Sound::EntityArmadilloStep, 0.15)
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!self.is_scared()).then_some(pumpkin_data::sound::Sound::EntityArmadilloAmbient)
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_string("state", self.get_state().name().to_string());
        nbt.put_int("scute_time", self.scute_time.load(Ordering::Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(state_name) = nbt.get_string("state") {
            self.switch_to_state(ArmadilloState::from_name(state_name));
        } else if let Some(state_id) = nbt.get_int("state") {
            self.switch_to_state(ArmadilloState::from_id(state_id));
        }
        if let Some(scute_time) = nbt.get_int("scute_time") {
            self.scute_time.store(scute_time, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn modify_incoming_damage(&self, amount: f32, _damage_type: DamageType) -> f32 {
        if self.is_scared() {
            (amount - 1.0).max(0.0) / 2.0
        } else {
            amount
        }
    }

    fn on_damage(&self, damage_type: DamageType, source: Option<&dyn EntityBase>) {
        // Hits with no hurt sound (inside the invulnerability window) react here.
        if !self.reacted_to_hurt.swap(false, Ordering::Relaxed) {
            self.react_to_hurt(
                damage_type,
                source.is_some_and(|src| src.get_living_entity().is_some()),
            );
        }
    }

    fn as_custom_sound(&self) -> Option<&dyn CustomSound> {
        Some(self)
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        ai::make_brain(self, packed)
    }

    fn mob_is_panicking(&self) -> bool {
        self.is_panicking()
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.in_state_ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        ai::update_activity(self);

        let entity = self.get_entity();
        let world = entity.world.load();

        if entity.is_alive() && !self.is_baby() {
            let scute_time = self.scute_time.fetch_sub(1, Ordering::Relaxed) - 1;
            if scute_time <= 0 {
                let pos = entity.pos.load();
                let item_entity = Arc::new(ItemEntity::new(
                    Entity::new(world.clone(), pos, &EntityType::ITEM),
                    ItemStack::new(1, &Item::ARMADILLO_SCUTE),
                ));
                world.spawn_entity_non_save(item_entity as Arc<dyn EntityBase>);
                world.play_sound(
                    Sound::EntityArmadilloScuteDrop,
                    SoundCategory::Neutral,
                    &pos,
                );
                self.scute_time
                    .store(pick_next_scute_drop_time(), Ordering::Relaxed);
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::armadillo::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::armadillo::ARMADILLO_STATE,
            VarInt(self.get_state().id()),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if item_stack.item == &Item::BRUSH && self.brush_off_scute(player, item_stack) {
            return true;
        }
        if self.is_scared() {
            return false;
        }
        self.animal_interact(player, item_stack)
    }
}
