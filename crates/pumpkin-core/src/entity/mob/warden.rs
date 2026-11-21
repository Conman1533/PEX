use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicI32, Ordering},
};

use pumpkin_data::{
    attributes::Attributes,
    damage::DamageType,
    effect::StatusEffect,
    entity::{EntityPose, EntityStatus, EntityType},
    game_event::GameEvent,
    particle::Particle,
    potion::Effect,
    sound::{Sound, SoundCategory},
    tag::{self, Tag},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::{
    GameMode,
    math::{position::BlockPos, vector3::Vector3},
};
use rand::RngExt;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{
            Controls, Goal, look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
            melee_attack::MeleeAttackGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
        },
        pathfinder::NavigatorGoal,
    },
    mob::{Mob, MobEntity},
};
use crate::plugin::api::events::entity::warden_anger_change::WardenAngerChangeEvent;
use crate::world::{
    World,
    vibration::{self, GameEventContext, VibrationData, VibrationUser},
};

const EMERGE_DURATION: i32 = 134;
const DARKNESS_DISPLAY_LIMIT: i32 = 200;
const DARKNESS_DURATION: i32 = 260;
const DARKNESS_AURA_RADIUS: f64 = 20.0;
const VIBRATION_COOLDOWN_TICKS: i32 = 40;
const RECENT_PROJECTILE_EXPIRY_TICKS: i32 = 100;
const DISTURBANCE_EXPIRY_TICKS: i32 = 100;
const DEFAULT_ANGER_INCREMENT: i32 = 35;
const PROJECTILE_INITIAL_ANGER_INCREMENT: i32 = 10;
const PROJECTILE_MAX_ANGER_DIST_SQ: f64 = 900.0;
const MAX_ANGER: i32 = 150;
const INVESTIGATE_SPEED: f64 = 0.7;

/// Vanilla `AngerLevel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AngerLevel {
    Calm,
    Agitated,
    Angry,
}

impl AngerLevel {
    pub const ANGRY_THRESHOLD: i32 = 80;
    pub const AGITATED_THRESHOLD: i32 = 40;

    #[must_use]
    pub const fn by_anger(anger: i32) -> Self {
        if anger >= Self::ANGRY_THRESHOLD {
            Self::Angry
        } else if anger >= Self::AGITATED_THRESHOLD {
            Self::Agitated
        } else {
            Self::Calm
        }
    }

    #[must_use]
    pub const fn is_angry(self) -> bool {
        matches!(self, Self::Angry)
    }

    #[must_use]
    pub const fn ambient_sound(self) -> Sound {
        match self {
            Self::Calm => Sound::EntityWardenAmbient,
            Self::Agitated => Sound::EntityWardenAgitated,
            Self::Angry => Sound::EntityWardenAngry,
        }
    }

    #[must_use]
    pub const fn listening_sound(self) -> Sound {
        match self {
            Self::Calm => Sound::EntityWardenListening,
            Self::Agitated | Self::Angry => Sound::EntityWardenListeningAngry,
        }
    }
}

#[derive(Clone, Debug)]
struct SuspectEntry {
    uuid: Uuid,
    anger: i32,
    is_player: bool,
}

/// Vanilla `AngerManagement`.
#[derive(Default)]
pub struct AngerManagement {
    suspects: Vec<SuspectEntry>,
}

impl AngerManagement {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            suspects: Vec::new(),
        }
    }

    fn sort(&mut self) {
        self.suspects.sort_by(|a, b| {
            let angry_a = AngerLevel::by_anger(a.anger).is_angry();
            let angry_b = AngerLevel::by_anger(b.anger).is_angry();
            angry_b
                .cmp(&angry_a)
                .then_with(|| b.is_player.cmp(&a.is_player))
                .then_with(|| b.anger.cmp(&a.anger))
        });
    }

    #[must_use]
    pub fn get_anger(&self, uuid: Uuid) -> i32 {
        self.suspects
            .iter()
            .find(|s| s.uuid == uuid)
            .map_or(0, |s| s.anger)
    }

    pub fn set_anger(&mut self, uuid: Uuid, is_player: bool, anger: i32) -> i32 {
        let clamped = anger.clamp(0, MAX_ANGER);
        if clamped == 0 {
            self.clear_anger(uuid);
            return 0;
        }
        if let Some(entry) = self.suspects.iter_mut().find(|s| s.uuid == uuid) {
            entry.anger = clamped;
            entry.is_player = is_player;
        } else {
            self.suspects.push(SuspectEntry {
                uuid,
                anger: clamped,
                is_player,
            });
        }
        self.sort();
        clamped
    }

    pub fn increase_anger(&mut self, uuid: Uuid, is_player: bool, increment: i32) -> i32 {
        let next = (self.get_anger(uuid) + increment).clamp(0, MAX_ANGER);
        self.set_anger(uuid, is_player, next)
    }

    pub fn clear_anger(&mut self, uuid: Uuid) {
        self.suspects.retain(|s| s.uuid != uuid);
        self.sort();
    }

    #[must_use]
    pub fn active_anger(&self, current_target: Option<Uuid>) -> i32 {
        if let Some(target_uuid) = current_target {
            let anger = self.get_anger(target_uuid);
            if anger > 0 {
                return anger;
            }
        }
        self.suspects.iter().map(|s| s.anger).max().unwrap_or(0)
    }

    #[must_use]
    pub fn active_entity(
        &self,
        world: &World,
        warden: &WardenEntity,
    ) -> Option<Arc<dyn EntityBase>> {
        for entry in &self.suspects {
            let entity = world
                .get_player_by_uuid(entry.uuid)
                .map(|p| p as Arc<dyn EntityBase>)
                .or_else(|| world.get_entity_by_uuid(entry.uuid));
            if let Some(entity) = entity
                && warden.can_target_entity(entity.as_ref())
            {
                return Some(entity);
            }
        }
        None
    }

    pub fn tick(&mut self, world: &World, warden: &WardenEntity) {
        self.suspects.retain_mut(|entry| {
            if entry.anger <= 1 {
                return false;
            }
            let resolved = world
                .get_player_by_uuid(entry.uuid)
                .map(|p| p as Arc<dyn EntityBase>)
                .or_else(|| world.get_entity_by_uuid(entry.uuid));
            if let Some(entity) = resolved {
                entry.is_player = entity.get_player().is_some();
                if !warden.can_target_entity(entity.as_ref()) {
                    return false;
                }
            }
            entry.anger -= 1;
            true
        });
        self.sort();
    }

    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        let list: Vec<NbtTag> = self
            .suspects
            .iter()
            .map(|entry| {
                let mut pair = NbtCompound::new();
                pair.put_uuid("uuid", entry.uuid);
                pair.put_int("anger", entry.anger);
                NbtTag::Compound(pair)
            })
            .collect();
        if !list.is_empty() {
            nbt.put("suspects", NbtTag::List(list));
        }
        nbt
    }

    #[must_use]
    pub fn from_nbt(nbt: &NbtCompound) -> Self {
        let mut mgr = Self::new();
        if let Some(list) = nbt.get_list("suspects") {
            for tag in list {
                if let NbtTag::Compound(pair) = tag
                    && let Some(uuid) = pair.get_uuid("uuid")
                    && let Some(anger) = pair.get_int("anger")
                    && anger > 0
                {
                    mgr.suspects.push(SuspectEntry {
                        uuid,
                        anger: anger.min(MAX_ANGER),
                        is_player: false,
                    });
                }
            }
            mgr.sort();
        }
        mgr
    }
}

/// Vanilla `WardenAi.ROAR_DURATION`.
const ROAR_DURATION: i32 = 84;
/// Vanilla `Roar.TICKS_BEFORE_PLAYING_ROAR_SOUND`.
const ROAR_SOUND_DELAY: i32 = 25;
/// Vanilla `Roar.ROAR_ANGER_INCREASE`.
const ROAR_ANGER_INCREASE: i32 = 20;
/// Vanilla `SonicBoom.DURATION` and `TICKS_BEFORE_PLAYING_SOUND`.
const SONIC_BOOM_DURATION: i32 = 60;
const SONIC_BOOM_FIRE_TICK: i32 = 34;
/// Vanilla `SonicBoom.COOLDOWN`, also set by every melee hit.
const SONIC_BOOM_COOLDOWN: i32 = 40;
/// Vanilla `WardenAi.SNIFFING_DURATION` (ceil 83.2).
const SNIFFING_DURATION: i32 = 84;
/// Vanilla `Warden.increaseAngerAt(entity)` default.
const DEFAULT_ANGER_INCREASE: i32 = 35;
/// Vanilla `NearestLivingEntitySensor` radius used for `NEAREST_ATTACKABLE`.
const NEAREST_ATTACKABLE_RANGE: f64 = 24.0;
/// Vanilla `WardenAi.MELEE_ATTACK_COOLDOWN`.
const MELEE_ATTACK_COOLDOWN: i32 = 18;

pub struct WardenEntity {
    pub mob_entity: MobEntity,
    /// Ticks left in the roar; vanilla `Roar` runs before every fight.
    roar_ticks: AtomicI32,
    roar_target: Mutex<Option<Arc<dyn EntityBase>>>,
    /// Ticks left while charging a sonic boom.
    sonic_ticks: AtomicI32,
    sonic_cooldown: AtomicI32,
    /// Ticks left in a sniff (vanilla `Sniffing`).
    sniff_ticks: AtomicI32,
    sniff_cooldown: AtomicI32,
    emerge_ticks: Arc<AtomicI32>,
    pub vibration: Mutex<VibrationData>,
    pub anger: Mutex<AngerManagement>,
    pub vibration_cooldown: AtomicI32,
    pub recent_projectile_ticks: AtomicI32,
    disturbance_pos: Arc<Mutex<Option<(BlockPos, i32)>>>,
}

struct EmergeGoal {
    emerge_ticks: Arc<AtomicI32>,
}

impl Goal for EmergeGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.emerge_ticks.load(Ordering::Relaxed) > 0
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let entity = &mob.get_mob_entity().living_entity.entity;
        if entity.pose.load() == EntityPose::Emerging {
            entity.set_pose(EntityPose::Standing);
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        self.emerge_ticks.fetch_sub(1, Ordering::Relaxed);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn can_stop(&self) -> bool {
        false
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK | Controls::JUMP | Controls::TARGET
    }
}

struct InvestigateGoal {
    disturbance_pos: Arc<Mutex<Option<(BlockPos, i32)>>>,
    current_target: Option<BlockPos>,
}

impl Goal for InvestigateGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_mob_entity().get_target().is_some() {
            return false;
        }
        let guard = self
            .disturbance_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((pos, ticks)) = *guard
            && ticks > 0
        {
            self.current_target = Some(pos);
            true
        } else {
            false
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_mob_entity().get_target().is_some() || mob.is_navigator_idle() {
            return false;
        }
        self.disturbance_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|(_, ticks)| ticks > 0)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = self.current_target {
            let pos = mob.get_entity().pos.load();
            mob.get_mob_entity()
                .steered()
                .navigator()
                .set_progress(NavigatorGoal::new(
                    pos,
                    target.to_centered_f64(),
                    INVESTIGATE_SPEED,
                ));
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.current_target = None;
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

impl WardenEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let emerge_ticks = Arc::new(AtomicI32::new(0));
        let disturbance_pos = Arc::new(Mutex::new(None));
        let warden = Self {
            mob_entity,
            emerge_ticks: emerge_ticks.clone(),
            roar_ticks: AtomicI32::new(0),
            roar_target: Mutex::new(None),
            sonic_ticks: AtomicI32::new(0),
            sonic_cooldown: AtomicI32::new(0),
            sniff_ticks: AtomicI32::new(0),
            sniff_cooldown: AtomicI32::new(0),
            vibration: Mutex::new(VibrationData::new()),
            anger: Mutex::new(AngerManagement::new()),
            vibration_cooldown: AtomicI32::new(0),
            recent_projectile_ticks: AtomicI32::new(0),
            disturbance_pos: disturbance_pos.clone(),
        };
        let mob_arc = Arc::new(warden);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(EmergeGoal { emerge_ticks }));
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(2, Box::new(MeleeAttackGoal::new(1.2, true)));
            goal_selector.add_goal(
                3,
                Box::new(InvestigateGoal {
                    disturbance_pos,
                    current_target: None,
                }),
            );
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(0.5)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    pub fn emerge(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_pose(EntityPose::Emerging);
        self.emerge_ticks.store(EMERGE_DURATION, Ordering::Relaxed);
        entity.world.load().play_sound_fine(
            Sound::EntityWardenAgitated,
            SoundCategory::Hostile,
            &entity.pos.load(),
            5.0,
            1.0,
        );
    }

    #[must_use]
    pub fn is_digging_or_emerging(&self) -> bool {
        matches!(
            self.get_entity().pose.load(),
            EntityPose::Digging | EntityPose::Emerging
        )
    }

    /// Vanilla `Warden.canTargetEntity`.
    #[must_use]
    pub fn can_target_entity(&self, target: &dyn EntityBase) -> bool {
        let Some(living) = target.get_living_entity() else {
            return false;
        };
        let target_entity = target.get_entity();
        if target_entity.entity_id == self.get_entity().entity_id {
            return false;
        }
        if let Some(player) = target.get_player()
            && (player.is_spectator() || player.gamemode.load() == GameMode::Creative)
        {
            return false;
        }
        if self.considers_entity_as_ally(target) {
            return false;
        }
        let target_type = target_entity.entity_type;
        if target_type == &EntityType::ARMOR_STAND || target_type == &EntityType::WARDEN {
            return false;
        }
        if target_entity.invulnerable.load(Ordering::Relaxed) {
            return false;
        }
        target_entity.is_alive() && !living.dead.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn get_active_anger(&self) -> i32 {
        let target_uuid = self
            .mob_entity
            .get_target()
            .map(|t| t.get_entity().entity_uuid);
        self.anger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active_anger(target_uuid)
    }

    #[must_use]
    pub fn get_anger_level(&self) -> AngerLevel {
        AngerLevel::by_anger(self.get_active_anger())
    }

    fn sync_client_anger_level(&self) {
        self.get_entity().set_synced_data(
            pumpkin_data::tracked_data::warden::CLIENT_ANGER_LEVEL,
            self.get_active_anger(),
        );
    }

    /// Vanilla `Warden.increaseAngerAt`.
    pub fn increase_anger_at(
        &self,
        target: Option<&Arc<dyn EntityBase>>,
        amount: i32,
        play_sound: bool,
    ) {
        let Some(target) = target else {
            return;
        };
        if self.mob_entity.is_no_ai() || !self.can_target_entity(target.as_ref()) {
            return;
        }
        let target_entity = target.get_entity();
        let target_uuid = target_entity.entity_uuid;
        let is_player = target.get_player().is_some();

        let old_anger = self
            .anger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_anger(target_uuid);
        let proposed = (old_anger + amount).clamp(0, MAX_ANGER);

        let mut event = WardenAngerChangeEvent {
            entity_id: self.get_entity().entity_id,
            target_id: target_entity.entity_id,
            old_anger,
            new_anger: proposed,
            cancelled: false,
        };
        if let Some(server) = self.get_entity().world.load().server.upgrade() {
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
        }

        let new_anger = self
            .anger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_anger(target_uuid, is_player, event.new_anger);
        self.sync_client_anger_level();

        // Vanilla `Warden.increaseAngerAt`: anger at a player drops a non-player target, and the
        // warden then roars at whoever it is angriest at before fighting (`mob_tick`).
        if is_player
            && AngerLevel::by_anger(new_anger).is_angry()
            && self
                .mob_entity
                .get_target()
                .is_some_and(|current| current.get_player().is_none())
        {
            self.mob_entity.set_target(None);
        }

        if play_sound && self.get_entity().pose.load() != EntityPose::Roaring {
            let entity = self.get_entity();
            entity.world.load().play_sound_fine(
                self.get_anger_level().listening_sound(),
                SoundCategory::Hostile,
                &entity.pos.load(),
                10.0,
                1.0,
            );
        }
    }

    /// Vanilla `Warden.closerThan(entity, horizontal, vertical)`.
    fn closer_than(&self, target: &dyn EntityBase, horizontal: f64, vertical: f64) -> bool {
        let pos = self.get_entity().pos.load();
        let other = target.get_entity().pos.load();
        let (dx, dz) = (other.x - pos.x, other.z - pos.z);
        dx * dx + dz * dz < horizontal * horizontal && (other.y - pos.y).abs() < vertical
    }

    fn is_roaring(&self) -> bool {
        self.roar_ticks.load(Ordering::Relaxed) > 0
    }

    /// Vanilla `NEAREST_ATTACKABLE`, players only.
    fn nearest_attackable(&self, world: &World) -> Option<Arc<dyn EntityBase>> {
        world
            .get_nearest_player(
                self.get_entity().pos.load(),
                NEAREST_ATTACKABLE_RANGE,
                |player| self.can_target_entity(player.as_ref()),
            )
            .map(|player| player as Arc<dyn EntityBase>)
    }

    /// Vanilla `Roar.start`.
    fn start_roar(&self, target: &Arc<dyn EntityBase>) {
        self.end_sniffing();
        self.mob_entity.steered().navigator().stop();
        let target_pos = target.get_entity().pos.load();
        self.mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_with_range(
                target_pos.x,
                target.get_entity().get_eye_y(),
                target_pos.z,
                180.0,
                90.0,
            );
        self.get_entity().set_pose(EntityPose::Roaring);
        self.roar_ticks.store(ROAR_DURATION, Ordering::Relaxed);
        *self
            .roar_target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(target.clone());
        self.increase_anger_at(Some(target), ROAR_ANGER_INCREASE, false);
    }

    /// Vanilla `Roar.tick` and `Roar.stop`: the roar sound after 25 ticks, the fight after 84.
    fn tick_roar(&self) {
        let ticks = self.roar_ticks.load(Ordering::Relaxed);
        if ticks <= 0 {
            return;
        }
        let ticks = ticks - 1;
        self.roar_ticks.store(ticks, Ordering::Relaxed);
        let entity = self.get_entity();
        if ROAR_DURATION - ticks == ROAR_SOUND_DELAY {
            entity.world.load().play_sound_fine(
                Sound::EntityWardenRoar,
                SoundCategory::Hostile,
                &entity.pos.load(),
                3.0,
                1.0,
            );
        }
        if ticks == 0 {
            if entity.pose.load() == EntityPose::Roaring {
                entity.set_pose(EntityPose::Standing);
            }
            let target = self
                .roar_target
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(target) = target.filter(|target| self.can_target_entity(target.as_ref())) {
                self.set_attack_target(target);
            }
        }
    }

    /// Vanilla `Warden.setAttackTarget`: ends any roar and holds the sonic boom back for 200
    /// ticks.
    fn set_attack_target(&self, target: Arc<dyn EntityBase>) {
        *self
            .roar_target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        if self.roar_ticks.swap(0, Ordering::Relaxed) > 0 {
            let entity = self.get_entity();
            if entity.pose.load() == EntityPose::Roaring {
                entity.set_pose(EntityPose::Standing);
            }
        }
        // The fight activity cuts a sniff short.
        if self.sniff_ticks.load(Ordering::Relaxed) > 0 {
            self.end_sniffing();
        }
        self.mob_entity.set_target(Some(target));
        self.sonic_cooldown.store(200, Ordering::Relaxed);
    }

    fn end_sniffing(&self) {
        self.sniff_ticks.store(0, Ordering::Relaxed);
        let entity = self.get_entity();
        if entity.pose.load() == EntityPose::Sniffing {
            entity.set_pose(EntityPose::Standing);
        }
    }

    /// Vanilla `TryToSniff` and `Sniffing`: with nobody to fight and nothing to investigate, the
    /// warden stops to sniff now and then, and gets angry at a player close by.
    fn tick_sniffing(&self, world: &World) {
        let cooldown = self.sniff_cooldown.load(Ordering::Relaxed);
        if cooldown > 0 {
            self.sniff_cooldown.store(cooldown - 1, Ordering::Relaxed);
        }
        let ticks = self.sniff_ticks.load(Ordering::Relaxed);
        if ticks > 0 {
            self.sniff_ticks.store(ticks - 1, Ordering::Relaxed);
            if ticks == 1 {
                self.end_sniffing();
                if let Some(target) = self.nearest_attackable(world) {
                    if self.closer_than(target.as_ref(), 6.0, 20.0) {
                        self.increase_anger_at(Some(&target), DEFAULT_ANGER_INCREASE, true);
                    }
                    let no_disturbance = self
                        .disturbance_pos
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .is_none();
                    if no_disturbance {
                        self.set_disturbance_location(target.get_entity().block_pos.load());
                    }
                }
            }
            return;
        }
        let entity = self.get_entity();
        let idle = cooldown <= 1
            && !self.is_roaring()
            && !self.is_digging_or_emerging()
            && entity.pose.load() == EntityPose::Standing
            && self.mob_entity.get_target().is_none()
            && self
                .disturbance_pos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none();
        if idle && self.nearest_attackable(world).is_some() {
            self.sniff_ticks.store(SNIFFING_DURATION, Ordering::Relaxed);
            self.sniff_cooldown
                .store(rand::rng().random_range(100..=200), Ordering::Relaxed);
            self.mob_entity.steered().navigator().stop();
            entity.set_pose(EntityPose::Sniffing);
            world.play_sound_fine(
                Sound::EntityWardenSniff,
                SoundCategory::Hostile,
                &entity.pos.load(),
                5.0,
                1.0,
            );
        }
    }

    /// Vanilla `SonicBoom`: a 3-second charge at a target within 15 blocks across and 20 up,
    /// firing after 34 ticks.
    fn tick_sonic_boom(&self, world: &World) {
        let cooldown = self.sonic_cooldown.load(Ordering::Relaxed);
        if cooldown > 0 {
            self.sonic_cooldown.store(cooldown - 1, Ordering::Relaxed);
        }
        let target = self.mob_entity.get_target();
        let entity = self.get_entity();
        let ticks = self.sonic_ticks.load(Ordering::Relaxed);
        if ticks > 0 {
            let ticks = ticks - 1;
            self.sonic_ticks.store(ticks, Ordering::Relaxed);
            if let Some(target) = &target {
                let target_pos = target.get_entity().pos.load();
                self.mob_entity
                    .look_control
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .look_at_with_range(target_pos.x, target_pos.y, target_pos.z, 10.0, 40.0);
            }
            if SONIC_BOOM_DURATION - ticks == SONIC_BOOM_FIRE_TICK
                && let Some(target) = target.filter(|target| {
                    self.can_target_entity(target.as_ref())
                        && self.closer_than(target.as_ref(), 15.0, 20.0)
                })
            {
                self.fire_sonic_boom(world, &target);
            }
            if ticks == 0 {
                self.sonic_cooldown
                    .store(SONIC_BOOM_COOLDOWN, Ordering::Relaxed);
            }
            return;
        }
        if cooldown <= 1
            && !self.is_roaring()
            && target.is_some_and(|target| {
                self.can_target_entity(target.as_ref())
                    && self.closer_than(target.as_ref(), 15.0, 20.0)
            })
        {
            self.sonic_ticks
                .store(SONIC_BOOM_DURATION, Ordering::Relaxed);
            world.send_entity_status(entity, EntityStatus::SonicCharge, ActorEventID::None);
            world.play_sound_fine(
                Sound::EntityWardenSonicCharge,
                SoundCategory::Hostile,
                &entity.pos.load(),
                3.0,
                1.0,
            );
        }
    }

    fn fire_sonic_boom(&self, world: &World, target: &Arc<dyn EntityBase>) {
        let entity = self.get_entity();
        // Vanilla `EntityAttachment.WARDEN_CHEST` is 1.6 blocks up.
        let source = entity.pos.load() + Vector3::new(0.0, 1.6, 0.0);
        let delta = target.get_entity().get_eye_pos() - source;
        let length = delta.length();
        if length <= 0.0 {
            return;
        }
        let normal = delta * (1.0 / length);
        let steps = length.floor() as i32 + 7;
        for i in 1..steps {
            world.spawn_particle(
                source + normal * f64::from(i),
                Vector3::new(0.0, 0.0, 0.0),
                0.0,
                1,
                Particle::SonicBoom,
            );
        }
        world.play_sound_fine(
            Sound::EntityWardenSonicBoom,
            SoundCategory::Hostile,
            &entity.pos.load(),
            3.0,
            1.0,
        );
        let Some(warden) = world.get_entity_by_id(entity.entity_id) else {
            return;
        };
        if target.damage_with_context(
            target.as_ref(),
            10.0,
            DamageType::SONIC_BOOM,
            None,
            Some(warden.as_ref()),
            Some(warden.as_ref()),
        ) {
            let resistance = target.get_living_entity().map_or(0.0, |living| {
                living.get_attribute_value(&Attributes::KNOCKBACK_RESISTANCE)
            });
            let vertical = 0.5 * (1.0 - resistance);
            let horizontal = 2.5 * (1.0 - resistance);
            target.get_entity().add_velocity(Vector3::new(
                normal.x * horizontal,
                normal.y * vertical,
                normal.z * horizontal,
            ));
        }
    }

    /// Vanilla `WardenAi.setDisturbanceLocation`.
    pub fn set_disturbance_location(&self, pos: BlockPos) {
        if self.get_anger_level().is_angry() || self.mob_entity.get_target().is_some() {
            return;
        }
        *self
            .disturbance_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((pos, DISTURBANCE_EXPIRY_TICKS));
    }

    pub fn apply_darkness_around(world: &World, position: Vector3<f64>, radius: f64) {
        for player in world.get_nearby_players(position, radius) {
            if !matches!(
                player.gamemode.load(),
                GameMode::Survival | GameMode::Adventure
            ) {
                continue;
            }
            let current = player.living_entity.get_effect(&StatusEffect::DARKNESS);
            if current.is_some_and(|effect| {
                effect.duration < 0 || effect.duration >= DARKNESS_DISPLAY_LIMIT
            }) {
                continue;
            }
            let darkness = Effect {
                effect_type: &StatusEffect::DARKNESS,
                duration: DARKNESS_DURATION,
                amplifier: 0,
                ambient: false,
                show_particles: false,
                show_icon: false,
                blend: true,
            };
            player.send_effect(&darkness);
            player.living_entity.add_effect(darkness);
        }
    }
}

impl VibrationUser for WardenEntity {
    fn vibration_data(&self) -> &Mutex<VibrationData> {
        &self.vibration
    }

    fn listener_radius(&self) -> i32 {
        16
    }

    fn listener_pos(&self) -> Vector3<f64> {
        self.get_entity().get_eye_pos()
    }

    fn listener_entity(&self) -> Option<(i32, f32)> {
        let entity = self.get_entity();
        Some((entity.entity_id, entity.get_eye_height() as f32))
    }

    fn listenable_events(&self) -> &'static Tag {
        &tag::GameEvent::MINECRAFT_WARDEN_CAN_LISTEN
    }

    fn can_receive_vibration(
        &self,
        _world: &World,
        _origin: &BlockPos,
        _event: GameEvent,
        context: &GameEventContext,
    ) -> bool {
        if self.mob_entity.is_no_ai()
            || !self.get_entity().is_alive()
            || self.mob_entity.living_entity.dead.load(Ordering::Relaxed)
            || self.vibration_cooldown.load(Ordering::Relaxed) > 0
            || self.is_digging_or_emerging()
        {
            return false;
        }
        context.source.as_ref().is_none_or(|source| {
            source.get_living_entity().is_none() || self.can_target_entity(source.as_ref())
        })
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        origin: &BlockPos,
        _event: GameEvent,
        source: Option<&Arc<dyn EntityBase>>,
        projectile_owner: Option<&Arc<dyn EntityBase>>,
        _distance: f32,
    ) {
        if !self.get_entity().is_alive()
            || self.mob_entity.living_entity.dead.load(Ordering::Relaxed)
        {
            return;
        }
        self.vibration_cooldown
            .store(VIBRATION_COOLDOWN_TICKS, Ordering::Relaxed);
        let entity = self.get_entity();
        world.send_entity_status(entity, EntityStatus::TendrilsShiver, ActorEventID::None);
        world.play_sound_fine(
            Sound::EntityWardenTendrilClicks,
            SoundCategory::Hostile,
            &entity.pos.load(),
            5.0,
            1.0,
        );

        let mut suspicious_pos = *origin;
        if let Some(owner) = projectile_owner {
            if entity
                .pos
                .load()
                .squared_distance_to_vec(&owner.get_entity().pos.load())
                < PROJECTILE_MAX_ANGER_DIST_SQ
            {
                if self.recent_projectile_ticks.load(Ordering::Relaxed) > 0 {
                    if self.can_target_entity(owner.as_ref()) {
                        suspicious_pos = owner.get_entity().block_pos.load();
                    }
                    self.increase_anger_at(Some(owner), DEFAULT_ANGER_INCREMENT, true);
                } else {
                    self.increase_anger_at(Some(owner), PROJECTILE_INITIAL_ANGER_INCREMENT, true);
                }
            }
            self.recent_projectile_ticks
                .store(RECENT_PROJECTILE_EXPIRY_TICKS, Ordering::Relaxed);
        } else {
            self.increase_anger_at(source, DEFAULT_ANGER_INCREMENT, true);
        }

        if !self.get_anger_level().is_angry() {
            let active = self
                .anger
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .active_entity(world, self);
            let matches_active = projectile_owner.is_some()
                || active.as_ref().is_none_or(|a| {
                    source.is_some_and(|s| a.get_entity().entity_id == s.get_entity().entity_id)
                });
            if matches_active {
                self.set_disturbance_location(suspicious_pos);
            }
        }
    }
}

impl Mob for WardenEntity {
    fn get_sound_volume(&self) -> f32 {
        4.0
    }

    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(pumpkin_data::sound::Sound::EntityWardenStep, 10.0)
    }

    /// Vanilla `Warden.removeWhenFarAway`.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        false
    }

    fn dampens_vibrations(&self) -> bool {
        true
    }

    fn as_vibration_user(&self) -> Option<&dyn VibrationUser> {
        Some(self)
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        if self.get_entity().pose.load() == EntityPose::Roaring || self.is_digging_or_emerging() {
            None
        } else {
            Some(self.get_anger_level().ambient_sound())
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn check_spawn_obstruction(&self, world: &World) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let bounding_box = entity.bounding_box.load();
        !world.contains_any_liquid(bounding_box)
            && world.get_entities_at_box(&bounding_box).is_empty()
            && world.is_space_empty(bounding_box)
    }

    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if self.mob_entity.is_no_ai() || self.is_digging_or_emerging() {
            return;
        }
        let Some(attacker) = source else {
            return;
        };
        let world = self.get_entity().world.load();
        let attacker_arc = vibration::entity_handle(&world, attacker);
        self.increase_anger_at(
            attacker_arc.as_ref(),
            AngerLevel::ANGRY_THRESHOLD + 20,
            false,
        );
        // Vanilla `Warden.hurtServer`: hit directly with nobody to fight, it turns on the attacker
        // at once, without roaring.
        if self.mob_entity.get_target().is_none()
            && let Some(attacker) = attacker_arc.filter(|attacker| {
                attacker.get_living_entity().is_some() && self.can_target_entity(attacker.as_ref())
            })
        {
            self.set_attack_target(attacker);
        }
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = self.get_entity();
        let world = entity.world.load_full();

        let vc = self.vibration_cooldown.load(Ordering::Relaxed);
        if vc > 0 {
            self.vibration_cooldown.store(vc - 1, Ordering::Relaxed);
        }
        let rp = self.recent_projectile_ticks.load(Ordering::Relaxed);
        if rp > 0 {
            self.recent_projectile_ticks
                .store(rp - 1, Ordering::Relaxed);
        }
        {
            let mut dist = self
                .disturbance_pos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((_, ref mut ticks)) = *dist {
                *ticks -= 1;
                if *ticks <= 0 {
                    *dist = None;
                }
            }
        }

        vibration::tick(self, &world);

        let tick_count = entity.tick_count.load(Ordering::Relaxed);
        if (tick_count + entity.entity_id) % 120 == 0 {
            Self::apply_darkness_around(&world, entity.pos.load(), DARKNESS_AURA_RADIUS);
        }
        if tick_count % 20 == 0 {
            self.anger
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .tick(&world, self);
            self.sync_client_anger_level();
        }

        self.tick_roar();
        if self.get_anger_level().is_angry() {
            let target = self.mob_entity.get_target();
            if target
                .as_ref()
                .is_some_and(|t| !self.can_target_entity(t.as_ref()))
            {
                self.mob_entity.set_target(None);
            }
            // Vanilla `SetRoarTarget`: no fight starts without a roar first.
            if self.mob_entity.get_target().is_none()
                && !self.is_roaring()
                && !self.is_digging_or_emerging()
            {
                let next_target = self
                    .anger
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .active_entity(&world, self);
                if let Some(next_target) = next_target {
                    self.start_roar(&next_target);
                }
            }
        } else if self.mob_entity.get_target().is_some() {
            self.mob_entity.set_target(None);
        }
        self.tick_sniffing(&world);
        self.tick_sonic_boom(&world);
    }

    fn mob_init_data_tracker(&self) {
        self.sync_client_anger_level();
    }

    /// Roaring and sniffing warden stand still, as no vanilla movement runs in those activities.
    fn is_mob_immobile(&self) -> bool {
        self.mob_entity.living_entity.is_dead_or_dying()
            || self.is_roaring()
            || self.sniff_ticks.load(Ordering::Relaxed) > 0
    }

    fn melee_attack_interval(&self) -> i32 {
        MELEE_ATTACK_COOLDOWN
    }

    fn can_melee_attack(&self) -> bool {
        self.sonic_ticks.load(Ordering::Relaxed) == 0 && !self.is_roaring()
    }

    /// Vanilla `Warden.doHurtTarget`.
    fn before_melee_attack(&self, _target: &dyn EntityBase) {
        let entity = self.get_entity();
        let world = entity.world.load();
        world.send_entity_status(
            entity,
            EntityStatus::StartAttacking,
            ActorEventID::StartAttacking,
        );
        let mut rng = self.get_random();
        let pitch = (rng.random::<f32>() - rng.random::<f32>()) * 0.2 + 1.0;
        world.play_sound_fine(
            Sound::EntityWardenAttackImpact,
            SoundCategory::Hostile,
            &entity.pos.load(),
            10.0,
            pitch,
        );
        self.sonic_cooldown
            .store(SONIC_BOOM_COOLDOWN, Ordering::Relaxed);
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Ok(anger) = self.anger.lock() {
            nbt.put_compound("anger", anger.to_nbt());
        }
        if let Ok(vibration) = self.vibration.lock() {
            vibration.write_nbt(nbt);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(anger_nbt) = nbt.get_compound("anger")
            && let Ok(mut anger) = self.anger.lock()
        {
            *anger = AngerManagement::from_nbt(anger_nbt);
        }
        self.sync_client_anger_level();
        if let Ok(mut vibration) = self.vibration.lock() {
            *vibration = VibrationData::read_nbt(nbt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anger_management_sorts_angry_and_players_first_and_round_trips_nbt() {
        let mut mgr = AngerManagement::new();
        let mob_uuid = Uuid::from_u128(1);
        let player_uuid = Uuid::from_u128(2);
        let angry_mob_uuid = Uuid::from_u128(3);

        mgr.increase_anger(mob_uuid, false, 50);
        mgr.increase_anger(player_uuid, true, 40);
        assert_eq!(mgr.suspects[0].uuid, player_uuid);

        mgr.increase_anger(angry_mob_uuid, false, 90);
        assert_eq!(mgr.suspects[0].uuid, angry_mob_uuid);
        assert_eq!(mgr.active_anger(None), 90);
        assert_eq!(
            AngerLevel::by_anger(mgr.active_anger(None)),
            AngerLevel::Angry
        );

        let nbt = mgr.to_nbt();
        let restored = AngerManagement::from_nbt(&nbt);
        assert_eq!(restored.get_anger(angry_mob_uuid), 90);
        assert_eq!(restored.get_anger(mob_uuid), 50);
        assert_eq!(restored.get_anger(player_uuid), 40);
    }
}
