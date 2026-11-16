use crate::entity::passive::animal::animal_can_mate;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicI32, AtomicI64, Ordering},
};

use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, GoalReach, breed::BreedGoal, escape_danger::EscapeDangerGoal,
        follow_parent::FollowParentGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, tempt::TemptGoal, to_goal_ticks,
        wander_around::WanderAroundGoal,
    },
    ai::{pathfinder::NavigatorGoal, util::land_random_pos},
    item::ItemEntity,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;

pub const SNIFFER_BABY_START_AGE: i32 = -48000;
pub const DIGGING_DROP_SEED_OFFSET_TICKS: i32 = 120;
/// Vanilla `SnifferAi.SNIFFING_COOLDOWN_TICKS`, after a finished dig.
const SNIFFING_COOLDOWN_TICKS: i64 = 9600;
/// How often the idle sniffer starts scenting or sniffing. Vanilla rolls them as two of its
/// idle choices, each taking a second or two.
const SNIFF_CHOICE_INTERVAL: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum SnifferState {
    #[default]
    Idling = 0,
    FeelingHappy = 1,
    Scenting = 2,
    Sniffing = 3,
    Searching = 4,
    Digging = 5,
    Rising = 6,
}

impl SnifferState {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::FeelingHappy,
            2 => Self::Scenting,
            3 => Self::Sniffing,
            4 => Self::Searching,
            5 => Self::Digging,
            6 => Self::Rising,
            _ => Self::Idling,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }
}

/// Represents a Sniffer, a large passive mob that digs up ancient seeds.
///
/// Wiki: <https://minecraft.wiki/w/Sniffer>
pub struct SnifferEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub state: AtomicI32,
    pub drop_seed_at_tick: AtomicI32,
    pub explored_positions: std::sync::Mutex<Vec<BlockPos>>,
    /// Vanilla `SNIFF_COOLDOWN`, as the game time it ends.
    sniff_cooldown_until: AtomicI64,
}

impl SnifferEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let sniffer = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            state: AtomicI32::new(SnifferState::Idling.id()),
            drop_seed_at_tick: AtomicI32::new(0),
            explored_positions: std::sync::Mutex::new(Vec::new()),
            sniff_cooldown_until: AtomicI64::new(0),
        };
        let mob_arc = Arc::new(sniffer);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            // Vanilla `SnifferAi`: `AnimalPanic(2.0)` in the core activity.
            goal_selector.add_goal(0, EscapeDangerGoal::new(2.0).lasting());
            goal_selector.add_goal(1, BreedGoal::new(1.0));
            goal_selector.add_goal(
                2,
                Box::new(TemptGoal::new(
                    1.25,
                    |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_SNIFFER_FOOD),
                    false,
                )),
            );
            goal_selector.add_goal(3, Box::new(FollowParentGoal::new(1.1)));
            goal_selector.add_goal(4, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(4, Box::new(SnifferSniffGoal::default()));
            goal_selector.add_goal(
                5,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn get_state(&self) -> SnifferState {
        SnifferState::from_id(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: SnifferState) {
        self.state.store(state.id(), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::STATE,
            VarInt(state.id()),
        );
    }

    pub fn transition_to(&self, state: SnifferState) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();

        match state {
            SnifferState::Idling => {
                self.set_state(SnifferState::Idling);
            }
            SnifferState::FeelingHappy => {
                world.play_sound(Sound::EntitySnifferHappy, SoundCategory::Neutral, &pos);
                self.set_state(SnifferState::FeelingHappy);
            }
            SnifferState::Scenting => {
                self.set_state(SnifferState::Scenting);
                self.on_scenting_start();
            }
            SnifferState::Sniffing => {
                world.play_sound(Sound::EntitySnifferSniffing, SoundCategory::Neutral, &pos);
                self.set_state(SnifferState::Sniffing);
            }
            SnifferState::Searching => {
                self.set_state(SnifferState::Searching);
            }
            SnifferState::Digging => {
                self.set_state(SnifferState::Digging);
                self.on_digging_start();
            }
            SnifferState::Rising => {
                world.play_sound(
                    Sound::EntitySnifferDiggingStop,
                    SoundCategory::Neutral,
                    &pos,
                );
                self.set_state(SnifferState::Rising);
            }
        }
    }

    fn on_scenting_start(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        let pitch = if self.is_baby() { 1.3 } else { 1.0 };
        world.play_sound_fine(
            Sound::EntitySnifferScenting,
            SoundCategory::Neutral,
            &pos,
            1.0,
            pitch,
        );
    }

    fn on_digging_start(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let current_ticks = world.level_time.try_lock().map_or(0, |t| t.world_age);
        let drop_tick = current_ticks as i32 + DIGGING_DROP_SEED_OFFSET_TICKS;
        self.drop_seed_at_tick.store(drop_tick, Ordering::Relaxed);
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::DROP_SEED_AT_TICK,
            VarInt(drop_tick),
        );
        world.send_entity_status(
            entity,
            EntityStatus::SnifferDiggingSound,
            ActorEventID::None,
        );
    }

    pub fn on_digging_complete(&self, success: bool) {
        if success {
            let head_block = self.get_head_block().down();
            self.store_explored_position(head_block);
        }
    }

    #[must_use]
    pub fn get_head_position(&self) -> Vector3<f64> {
        let entity = self.get_entity();
        let pos = entity.pos.load();
        let yaw = f64::from(entity.yaw.load());
        let yaw_rad = yaw.to_radians();
        let forward = Vector3::new(-yaw_rad.sin(), 0.0, yaw_rad.cos());
        pos + forward * 2.25
    }

    #[must_use]
    pub fn get_head_block(&self) -> BlockPos {
        let head_pos = self.get_head_position();
        let entity = self.get_entity();
        let pos = entity.pos.load();
        BlockPos::floored(head_pos.x, pos.y + 0.2, head_pos.z)
    }

    #[must_use]
    pub fn can_dig(&self) -> bool {
        let entity = self.get_entity();
        !self.is_panicking()
            && !self.is_baby()
            && !self.mob_entity.living_entity.is_in_water()
            && entity.on_ground.load(Ordering::Relaxed)
            && self.can_dig_at(self.get_head_block().down())
    }

    fn can_dig_at(&self, pos: BlockPos) -> bool {
        let entity = self.get_entity();
        let world = entity.world.load();
        let block_state = world.get_block_state(&pos);
        let block = pumpkin_data::Block::from_state_id(block_state.id);

        if !block.has_tag(&tag::Block::MINECRAFT_SNIFFER_DIGGABLE_BLOCK) {
            return false;
        }

        let explored = self
            .explored_positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !explored.contains(&pos)
    }

    /// Vanilla `Sniffer.canSniff`.
    #[must_use]
    pub fn can_sniff(&self) -> bool {
        let entity = self.get_entity();
        !self.is_panicking()
            && !entity.is_in_water()
            && !self.mob_entity.is_in_love()
            && entity.on_ground.load(Ordering::Relaxed)
            && entity.get_vehicle().is_none()
            && !entity.is_leashed()
    }

    /// Vanilla `Sniffer.calculateDigPosition`, without the reachable-path check.
    fn calculate_dig_position(&self) -> Option<BlockPos> {
        (0..5).find_map(|idx| {
            let pos = land_random_pos::get_pos(self, 10 + 2 * idx, 3)?;
            let block = BlockPos::floored(pos.x, pos.y, pos.z).down();
            self.can_dig_at(block).then_some(block)
        })
    }

    fn store_explored_position(&self, pos: BlockPos) {
        let mut explored = self
            .explored_positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if explored.len() >= 20 {
            explored.pop();
        }
        explored.insert(0, pos);
    }

    pub fn drop_seed(&self) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let current_tick = world.get_world_age() as i32;

        if self.drop_seed_at_tick.load(Ordering::Relaxed) == current_tick {
            let head_pos = self.get_head_position();
            let seed_item = if rand::random::<bool>() {
                &Item::TORCHFLOWER_SEEDS
            } else {
                &Item::PITCHER_POD
            };
            let item_stack = ItemStack::new(1, seed_item);

            let item_entity = Entity::new(world.clone(), head_pos, &EntityType::ITEM);
            let item_arc = Arc::new(ItemEntity::new(item_entity, item_stack));
            world.spawn_entity_non_save(item_arc as Arc<dyn EntityBase>);

            world.play_sound(
                Sound::EntitySnifferDropSeed,
                SoundCategory::Neutral,
                &head_pos,
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SniffPhase {
    #[default]
    Scenting,
    Sniffing,
    Searching,
    Digging,
    Rising,
    Happy,
}

/// Vanilla `SnifferAi`'s idle scenting and its sniff, search, dig, rise and feel-happy cycle,
/// in one goal.
#[derive(Default)]
struct SnifferSniffGoal {
    phase: SniffPhase,
    ticks: i32,
    done: bool,
}

impl SnifferSniffGoal {
    fn sniffer(mob: &dyn Mob) -> Option<&SnifferEntity> {
        mob.cast_any().downcast_ref::<SnifferEntity>()
    }

    fn finish(&mut self, sniffer: &SnifferEntity) {
        sniffer.transition_to(SnifferState::Idling);
        self.done = true;
    }
}

impl Goal for SnifferSniffGoal {
    fn tick_reach(&self, _mob: &dyn Mob, running: bool) -> Option<GoalReach> {
        Some(GoalReach {
            speed: if running { 1.25 } else { 0.0 },
            ..GoalReach::default()
        })
    }

    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(sniffer) = Self::sniffer(mob) else {
            return false;
        };
        if !sniffer.can_sniff()
            || mob
                .get_random()
                .random_range(0..to_goal_ticks(SNIFF_CHOICE_INTERVAL))
                != 0
        {
            return false;
        }
        let now = sniffer.get_entity().world.load().get_world_age();
        let can_sniff_out =
            !sniffer.is_baby() && now >= sniffer.sniff_cooldown_until.load(Ordering::Relaxed);
        self.phase = if can_sniff_out && mob.get_random().random::<bool>() {
            SniffPhase::Sniffing
        } else {
            SniffPhase::Scenting
        };
        self.ticks = mob.get_random().random_range(40..=80);
        self.done = false;
        true
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        !self.done
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(sniffer) = Self::sniffer(mob) else {
            return;
        };
        mob.get_mob_entity().steered().navigator().stop();
        sniffer.transition_to(if self.phase == SniffPhase::Sniffing {
            SnifferState::Sniffing
        } else {
            SnifferState::Scenting
        });
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(sniffer) = Self::sniffer(mob)
            && sniffer.get_state() != SnifferState::Idling
        {
            sniffer.transition_to(SnifferState::Idling);
        }
        if self.phase == SniffPhase::Searching {
            mob.get_mob_entity().steered().navigator().stop();
        }
        self.done = false;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(sniffer) = Self::sniffer(mob) else {
            self.done = true;
            return;
        };
        self.ticks -= 1;
        match self.phase {
            SniffPhase::Scenting | SniffPhase::Happy => {
                if self.ticks <= 0 {
                    self.finish(sniffer);
                }
            }
            SniffPhase::Sniffing => {
                if !sniffer.can_sniff() {
                    self.finish(sniffer);
                } else if self.ticks <= 0 {
                    sniffer.transition_to(SnifferState::Idling);
                    match sniffer.calculate_dig_position() {
                        Some(dig_pos) => {
                            self.phase = SniffPhase::Searching;
                            self.ticks = 600;
                            sniffer.transition_to(SnifferState::Searching);
                            let pos = sniffer.get_entity().pos.load();
                            let target = Vector3::new(
                                f64::from(dig_pos.0.x) + 0.5,
                                f64::from(dig_pos.0.y) + 1.0,
                                f64::from(dig_pos.0.z) + 0.5,
                            );
                            mob.get_mob_entity()
                                .steered()
                                .navigator()
                                .set_progress(NavigatorGoal::new(pos, target, 1.25));
                        }
                        None => self.done = true,
                    }
                }
            }
            SniffPhase::Searching => {
                if !sniffer.can_sniff() {
                    self.finish(sniffer);
                } else if self.ticks <= 0 || mob.is_navigator_idle() {
                    if sniffer.can_dig() {
                        mob.get_mob_entity().steered().navigator().stop();
                        self.phase = SniffPhase::Digging;
                        self.ticks = mob.get_random().random_range(160..=180);
                        sniffer.transition_to(SnifferState::Digging);
                    } else {
                        self.finish(sniffer);
                    }
                }
            }
            SniffPhase::Digging => {
                if !sniffer.can_dig() || sniffer.mob_entity.is_in_love() {
                    self.finish(sniffer);
                } else if self.ticks <= 0 {
                    let now = sniffer.get_entity().world.load().get_world_age();
                    sniffer
                        .sniff_cooldown_until
                        .store(now + SNIFFING_COOLDOWN_TICKS, Ordering::Relaxed);
                    self.phase = SniffPhase::Rising;
                    self.ticks = 40;
                    sniffer.transition_to(SnifferState::Rising);
                }
            }
            SniffPhase::Rising => {
                if self.ticks <= 0 {
                    sniffer.transition_to(SnifferState::Idling);
                    sniffer.on_digging_complete(true);
                    self.phase = SniffPhase::Happy;
                    self.ticks = mob.get_random().random_range(40..=100);
                    sniffer.transition_to(SnifferState::FeelingHappy);
                }
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

impl AgeableMob for SnifferEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn get_baby_start_age(&self) -> i32 {
        SNIFFER_BABY_START_AGE
    }
}

impl Animal for SnifferEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_SNIFFER_FOOD)
    }

    /// Vanilla `Sniffer.canMate`: only while both are idle, scenting or happy.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        let can_breed_in = |state| {
            matches!(
                state,
                SnifferState::Idling | SnifferState::Scenting | SnifferState::FeelingHappy
            )
        };
        partner
            .cast_any()
            .downcast_ref::<Self>()
            .is_some_and(|sniffer| {
                can_breed_in(self.get_state()) && can_breed_in(sniffer.get_state())
            })
            && animal_can_mate(&self.mob_entity, partner)
    }

    fn play_eating_sound(&self) {
        let entity = self.get_entity();
        let pitch = self.get_random().random_range(0.8..1.2);
        entity.world.load().play_sound_fine(
            Sound::EntitySnifferEat,
            SoundCategory::Neutral,
            &entity.pos.load(),
            1.0,
            pitch,
        );
    }

    /// Vanilla `Sniffer.spawnChildFromBreeding`: lays an egg instead of a baby.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Mob) {
        let entity = self.get_entity();
        let pos = entity.pos.load();
        let egg = Arc::new(ItemEntity::new(
            Entity::new(world.clone(), pos, &EntityType::ITEM),
            ItemStack::new(1, &Item::SNIFFER_EGG),
        ));
        self.finalize_spawn_child_from_breeding(world, partner, None);
        let mut rng = self.get_random();
        let pitch = (rng.random::<f32>() - rng.random::<f32>()).mul_add(0.2, 0.5);
        world.play_sound_fine(
            Sound::BlockSnifferEggPlop,
            SoundCategory::Neutral,
            &pos,
            1.0,
            pitch,
        );
        world.spawn_entity(egg);
    }
}

impl Mob for SnifferEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        (!matches!(
            self.get_state(),
            SnifferState::Digging | SnifferState::Searching
        ))
        .then_some(pumpkin_data::sound::Sound::EntitySnifferIdle)
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("State", self.get_state().id());
        nbt.put_int(
            "DropSeedAtTick",
            self.drop_seed_at_tick.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(state_id) = nbt.get_int("State") {
            self.state.store(state_id, Ordering::Relaxed);
        }
        if let Some(drop_tick) = nbt.get_int("DropSeedAtTick") {
            self.drop_seed_at_tick.store(drop_tick, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let state = self.get_state();
        // Vanilla plays the searching sound on the client only.
        if state == SnifferState::Digging {
            self.drop_seed();
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::sniffer::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::STATE,
            VarInt(self.get_state().id()),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::sniffer::DROP_SEED_AT_TICK,
            VarInt(self.drop_seed_at_tick.load(Ordering::Relaxed)),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}
