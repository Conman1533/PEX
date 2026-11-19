use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        breed::BreedGoal, escape_danger::EscapeDangerGoal, follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    custom_sound::CustomSound,
    item::ItemEntity,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

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
    pub danger_detected_recently_ticks: AtomicI32,
    /// Vanilla `ArmadilloBallUp` behaviour state.
    ball_up_running: AtomicBool,
    /// Set when the hurt reaction already ran before the hurt sound.
    reacted_to_hurt: AtomicBool,
    ball_up_ticks: AtomicI32,
    next_peek_timer: AtomicI32,
    danger_was_around: AtomicBool,
}

impl ArmadilloEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let armadillo = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            state: AtomicI32::new(ArmadilloState::Idle.id()),
            in_state_ticks: AtomicU64::new(0),
            scute_time: AtomicI32::new(pick_next_scute_drop_time()),
            danger_detected_recently_ticks: AtomicI32::new(0),
            ball_up_running: AtomicBool::new(false),
            reacted_to_hurt: AtomicBool::new(false),
            ball_up_ticks: AtomicI32::new(0),
            next_peek_timer: AtomicI32::new(0),
            danger_was_around: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(armadillo);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            // Vanilla `ArmadilloPanic`: only fire, freezing and the like; a hit makes it roll up.
            goal_selector.add_goal(
                1,
                EscapeDangerGoal::new(2.0)
                    .with_panic_causes(|_| &tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES)
                    .lasting(),
            );
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                Box::new(TemptGoal::new(
                    1.25,
                    |_, stack| {
                        stack
                            .get_item()
                            .has_tag(&tag::Item::MINECRAFT_ARMADILLO_FOOD)
                    },
                    false,
                )),
            );
            goal_selector.add_goal(4, Box::new(FollowParentGoal::new(1.1)));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
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

    pub fn can_stay_rolled_up(&self) -> bool {
        !self.is_panicking()
            && !self.mob_entity.living_entity.is_in_water()
            && !self.get_entity().has_vehicle()
    }

    pub fn roll_up(&self) {
        if !self.is_scared() {
            // Vanilla `stopInPlace`.
            self.mob_entity.steered().navigator().stop();
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

    /// Vanilla `ARMADILLO_SCARE_DETECTED` sensor: every 5 ticks, a scary entity nearby keeps
    /// the danger memory at 80 ticks.
    fn tick_scare_sensor(&self) {
        let entity = self.get_entity();
        if (entity.tick_count.load(Ordering::Relaxed) + entity.entity_id).rem_euclid(5) != 0 {
            return;
        }
        if !self.can_stay_rolled_up() {
            self.danger_detected_recently_ticks
                .store(0, Ordering::Relaxed);
            return;
        }
        let area = entity.bounding_box.load().expand(
            SCARE_DISTANCE_HORIZONTAL,
            SCARE_DISTANCE_VERTICAL,
            SCARE_DISTANCE_HORIZONTAL,
        );
        let scared = entity
            .world
            .load()
            .get_all_at_box(&area)
            .iter()
            .any(|other| {
                other.get_entity().entity_id != entity.entity_id
                    && other.get_living_entity().is_some()
                    && other.get_entity().is_alive()
                    && self.is_scared_by(other.as_ref())
            });
        if scared {
            self.danger_detected_recently_ticks
                .store(SCARE_CHECK_INTERVAL, Ordering::Relaxed);
        }
    }

    fn pick_next_peek_timer() -> i32 {
        ArmadilloState::Scared.animation_duration() as i32 + rand::random_range(100..=400)
    }

    /// Vanilla `ArmadilloAi`: the core `ARMADILLO_ROLLING_OUT` step and the panic activity's
    /// `ArmadilloBallUp` behaviour.
    fn tick_ball_up(&self) {
        let danger = self.danger_detected_recently_ticks.load(Ordering::Relaxed);
        let entity = self.get_entity();
        let on_ground = entity.on_ground.load(Ordering::Relaxed);

        if danger == 0 && self.is_scared() {
            self.roll_out();
        }

        if !self.ball_up_running.load(Ordering::Relaxed) {
            if danger > 0
                && !self.is_panicking()
                && on_ground
                && !self.mob_entity.living_entity.is_in_water()
            {
                self.ball_up_running.store(true, Ordering::Relaxed);
                self.ball_up_ticks.store(0, Ordering::Relaxed);
                self.roll_up();
            } else {
                return;
            }
        }

        // Five-minute behaviour timeout, or nothing left to hide from.
        if self.ball_up_ticks.fetch_add(1, Ordering::Relaxed) >= 6000
            || !self.get_state().is_threatened()
        {
            self.ball_up_running.store(false, Ordering::Relaxed);
            if !self.can_stay_rolled_up() {
                self.roll_out();
            }
            return;
        }

        let peek = self.next_peek_timer.load(Ordering::Relaxed);
        if peek > 0 {
            self.next_peek_timer.store(peek - 1, Ordering::Relaxed);
        }

        if self.should_switch_to_scared_state() {
            self.switch_to_state(ArmadilloState::Scared);
            if on_ground {
                self.mob_entity
                    .living_entity
                    .make_sound(Sound::EntityArmadilloLand);
            }
            return;
        }

        let state = self.get_state();
        let danger_is_around = danger > 75;
        if self
            .danger_was_around
            .swap(danger_is_around, Ordering::Relaxed)
            != danger_is_around
        {
            self.next_peek_timer
                .store(Self::pick_next_peek_timer(), Ordering::Relaxed);
        }
        let unroll_ticks = ArmadilloState::Unrolling.animation_duration() as i32;
        if state == ArmadilloState::Scared {
            if self.next_peek_timer.load(Ordering::Relaxed) == 0 && on_ground && danger_is_around {
                entity.world.load().send_entity_status(
                    entity,
                    EntityStatus::ArmadilloPeek,
                    ActorEventID::None,
                );
                self.next_peek_timer
                    .store(Self::pick_next_peek_timer(), Ordering::Relaxed);
            }
            if danger < unroll_ticks {
                self.mob_entity
                    .living_entity
                    .make_sound(Sound::EntityArmadilloUnrollStart);
                self.switch_to_state(ArmadilloState::Unrolling);
            }
        } else if state == ArmadilloState::Unrolling && danger > unroll_ticks {
            self.switch_to_state(ArmadilloState::Scared);
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
            self.danger_detected_recently_ticks
                .store(SCARE_CHECK_INTERVAL, Ordering::Relaxed);
            if self.can_stay_rolled_up() {
                self.roll_up();
            }
        } else if damage_type.has_tag(&tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES) {
            self.roll_out();
        }
    }

    pub fn is_scared_by(&self, living_entity: &dyn EntityBase) -> bool {
        let entity = self.get_entity();
        let pos = entity.pos.load();
        let target_pos = living_entity.get_entity().pos.load();
        let dx = (pos.x - target_pos.x).abs();
        let dy = (pos.y - target_pos.y).abs();
        let dz = (pos.z - target_pos.z).abs();

        if dx > SCARE_DISTANCE_HORIZONTAL
            || dz > SCARE_DISTANCE_HORIZONTAL
            || dy > SCARE_DISTANCE_VERTICAL
        {
            return false;
        }

        let target_type = living_entity.get_entity().entity_type;
        if target_type.has_tag(&tag::EntityType::MINECRAFT_UNDEAD) {
            return true;
        }

        if self.mob_entity.living_entity.get_last_hurt_by_id()
            == Some(living_entity.get_entity().entity_id)
        {
            return true;
        }

        if target_type == &EntityType::PLAYER {
            let target_ent = living_entity.get_entity();
            if target_ent.is_sprinting() || target_ent.has_vehicle() {
                return true;
            }
        }

        false
    }
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

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.in_state_ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        // Vanilla `stopInPlace` on rolling up, and no `MoveToTargetSink` while rolled.
        if self.is_scared() {
            self.mob_entity.steered().navigator().stop();
            let pos = self.get_entity().pos.load();
            self.mob_entity
                .move_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_wanted_position(pos.x, pos.y, pos.z, 0.0);
        }
        let danger_ticks = self.danger_detected_recently_ticks.load(Ordering::Relaxed);
        if danger_ticks > 0 {
            self.danger_detected_recently_ticks
                .store(danger_ticks - 1, Ordering::Relaxed);
        }

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

        self.tick_scare_sensor();
        self.tick_ball_up();
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
