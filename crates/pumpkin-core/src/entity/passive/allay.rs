use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::Block;
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Tag};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        escape_danger::EscapeDangerGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    player::Player,
};
use crate::world::World;
use crate::world::vibration::{self, GameEventContext, VibrationData, VibrationUser};

const NOTEBLOCK_COOLDOWN_TICKS: i32 = 600;
const NOTEBLOCK_MAX_DIST_MANHATTAN: i32 = 1024;
const JUKEBOX_RADIUS_SQ: f64 = 100.0;

pub struct AllayEntity {
    pub mob_entity: MobEntity,
    pub dancing: AtomicBool,
    pub can_duplicate: AtomicBool,
    pub duplication_cooldown: AtomicI32,
    pub owner: AtomicCell<Option<Uuid>>,
    pub vibration: Mutex<VibrationData>,
    pub liked_noteblock_pos: Mutex<Option<(String, BlockPos)>>,
    pub liked_noteblock_cooldown: AtomicI32,
    pub jukebox_pos: Mutex<Option<BlockPos>>,
}

impl AllayEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let allay = Self {
            mob_entity,
            dancing: AtomicBool::new(false),
            can_duplicate: AtomicBool::new(true),
            duplication_cooldown: AtomicI32::new(0),
            owner: AtomicCell::new(None),
            vibration: Mutex::new(VibrationData::new()),
            liked_noteblock_pos: Mutex::new(None),
            liked_noteblock_cooldown: AtomicI32::new(0),
            jukebox_pos: Mutex::new(None),
        };
        let mob_arc = Arc::new(allay);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let mut goal_selector = mob_arc
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(1.25));
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                7,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn is_dancing(&self) -> bool {
        self.dancing.load(Ordering::Relaxed)
    }

    pub fn set_dancing(&self, dancing: bool) {
        if self.mob_entity.is_no_ai() || (dancing && self.is_panicking()) {
            return;
        }
        self.dancing.store(dancing, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::allay::DATA_DANCING, dancing);
    }

    /// Vanilla `Allay.setJukeboxPlaying`.
    pub fn set_jukebox_playing(&self, jukebox: BlockPos, is_playing: bool) {
        if is_playing {
            if !self.is_dancing() {
                *self
                    .jukebox_pos
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(jukebox);
                self.set_dancing(true);
            }
        } else {
            let mut guard = self
                .jukebox_pos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_none_or(|pos| pos == jukebox) {
                *guard = None;
                drop(guard);
                self.set_dancing(false);
            }
        }
    }

    fn should_stop_dancing(&self, world: &World) -> bool {
        let Some(jukebox_pos) = *self
            .jukebox_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        else {
            return true;
        };
        let pos = self.get_entity().pos.load();
        jukebox_pos.to_centered_f64().squared_distance_to_vec(&pos) > JUKEBOX_RADIUS_SQ
            || world.get_block(&jukebox_pos) != &Block::JUKEBOX
    }

    /// Vanilla `AllayAi.hearNoteblock`.
    pub fn hear_noteblock(&self, world: &World, pos: BlockPos) {
        let dim = world.dimension.minecraft_name.to_string();
        let mut guard = self
            .liked_noteblock_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*guard {
            None => {
                *guard = Some((dim, pos));
                self.liked_noteblock_cooldown
                    .store(NOTEBLOCK_COOLDOWN_TICKS, Ordering::Relaxed);
            }
            Some((existing_dim, existing_pos)) if existing_dim == &dim && *existing_pos == pos => {
                self.liked_noteblock_cooldown
                    .store(NOTEBLOCK_COOLDOWN_TICKS, Ordering::Relaxed);
            }
            Some(_) => {}
        }
    }

    #[must_use]
    pub fn can_duplicate(&self) -> bool {
        self.can_duplicate.load(Ordering::Relaxed)
    }

    pub fn set_can_duplicate(&self, can_duplicate: bool) {
        self.can_duplicate.store(can_duplicate, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::allay::DATA_CAN_DUPLICATE,
            can_duplicate,
        );
    }
}

impl VibrationUser for AllayEntity {
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
        &tag::GameEvent::MINECRAFT_ALLAY_CAN_LISTEN
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        origin: &BlockPos,
        _event: GameEvent,
        _context: &GameEventContext,
    ) -> bool {
        if self.mob_entity.is_no_ai() {
            return false;
        }
        let guard = self
            .liked_noteblock_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some((ref dim, ref liked_pos)) = *guard else {
            return true;
        };
        dim == world.dimension.minecraft_name
            && self
                .get_entity()
                .block_pos
                .load()
                .manhattan_distance(*liked_pos)
                <= NOTEBLOCK_MAX_DIST_MANHATTAN
            && liked_pos == origin
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        origin: &BlockPos,
        event: GameEvent,
        _source: Option<&Arc<dyn EntityBase>>,
        _projectile_owner: Option<&Arc<dyn EntityBase>>,
        _distance: f32,
    ) {
        if event == GameEvent::NoteBlockPlay {
            self.hear_noteblock(world, *origin);
        }
    }
}

impl Mob for AllayEntity {
    fn as_vibration_user(&self) -> Option<&dyn VibrationUser> {
        Some(self)
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        let holds_item = !self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND)
            .is_empty();
        Some(if holds_item {
            pumpkin_data::sound::Sound::EntityAllayAmbientWithItem
        } else {
            pumpkin_data::sound::Sound::EntityAllayAmbientWithoutItem
        })
    }

    /// Vanilla `Allay.removeWhenFarAway`.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        false
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("CanDuplicate", self.can_duplicate());
        nbt.put_int(
            "DuplicationCooldown",
            self.duplication_cooldown.load(Ordering::Relaxed),
        );
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("Owner", owner);
        }
        if let Ok(vibration) = self.vibration.lock() {
            vibration.write_nbt(nbt);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(can) = nbt.get_bool("CanDuplicate") {
            self.set_can_duplicate(can);
        }
        if let Some(cd) = nbt.get_int("DuplicationCooldown") {
            self.duplication_cooldown.store(cd, Ordering::Relaxed);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner.store(Some(owner));
        }
        if let Ok(mut vibration) = self.vibration.lock() {
            *vibration = VibrationData::read_nbt(nbt);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = self.get_entity();
        let world = entity.world.load_full();
        vibration::tick(self, &world);
        if self.is_panicking() {
            self.set_dancing(false);
        }
        let tick_count = entity.tick_count.load(Ordering::Relaxed);
        if self.is_dancing() && tick_count % 20 == 0 && self.should_stop_dancing(&world) {
            *self
                .jukebox_pos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            self.set_dancing(false);
        }
        let nb_cd = self.liked_noteblock_cooldown.load(Ordering::Relaxed);
        if nb_cd > 0 {
            let next = nb_cd - 1;
            self.liked_noteblock_cooldown.store(next, Ordering::Relaxed);
            if next == 0 {
                *self
                    .liked_noteblock_pos
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            }
        }
        let cd = self.duplication_cooldown.load(Ordering::Relaxed);
        if cd > 0 {
            let next = cd - 1;
            self.duplication_cooldown.store(next, Ordering::Relaxed);
            if next == 0 {
                self.set_can_duplicate(true);
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::allay::DATA_DANCING,
            self.is_dancing(),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::allay::DATA_CAN_DUPLICATE,
            self.can_duplicate(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();

        if self.is_dancing() && self.can_duplicate() && item == &Item::AMETHYST_SHARD {
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            self.set_can_duplicate(false);
            self.duplication_cooldown.store(6000, Ordering::Relaxed);

            let entity = self.get_entity();
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.spawn_particle(
                pos + Vector3::new(0.0, f64::from(entity.height()), 0.0),
                Vector3::new(0.5, 0.5, 0.5),
                1.0,
                7,
                Particle::Heart,
            );
            world.play_sound(
                Sound::EntityAllayAmbientWithoutItem,
                SoundCategory::Neutral,
                &pos,
            );

            let new_allay = Self::new(Entity::new(world.clone(), pos, &EntityType::ALLAY));
            world.spawn_entity(new_allay);
            return true;
        }

        if self.owner.load().is_none() {
            self.owner.store(Some(player.gameprofile.id));
        }

        false
    }
}
