use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockDirection;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityType, MobCategory};
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::CEntityPositionSync;
use pumpkin_util::Difficulty;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use rand::RngExt;

use crate::entity::ai::goal::active_target::ActiveTargetGoal;
use crate::entity::ai::goal::look_around::RandomLookAroundGoal;
use crate::entity::ai::goal::look_at_entity::LookAtEntityGoal;
use crate::entity::ai::goal::revenge::RevengeGoal;
use crate::entity::ai::goal::{Controls, Goal, to_goal_ticks};
use crate::entity::living::LivingEntity;
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::projectile::shulker_bullet::ShulkerBulletEntity;
use crate::entity::{Entity, EntityBase};
use crate::world::game_event::GameEventContext;

const DEFAULT_ATTACH_FACE: BlockDirection = BlockDirection::Down;
const NO_COLOR: u8 = 16;
const PEEK_PER_TICK: f32 = 0.05;
const MAX_TELEPORT_DISTANCE: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

trait BlockDirectionExt {
    fn axis_of(self) -> Axis;
}

impl BlockDirectionExt for BlockDirection {
    fn axis_of(self) -> Axis {
        match self {
            Self::East | Self::West => Axis::X,
            Self::Up | Self::Down => Axis::Y,
            Self::North | Self::South => Axis::Z,
        }
    }
}

pub struct ShulkerEntity {
    pub mob_entity: MobEntity,
    attach_face: AtomicU8,
    peek_amount: AtomicU8,
    color: AtomicU8,
    current_peek_amount: AtomicCell<f32>,
    prev_peek_amount: AtomicCell<f32>,
}

impl ShulkerEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let shulker = Self {
            mob_entity,
            attach_face: AtomicU8::new(DEFAULT_ATTACH_FACE as u8),
            peek_amount: AtomicU8::new(0),
            color: AtomicU8::new(NO_COLOR),
            current_peek_amount: AtomicCell::new(0.0),
            prev_peek_amount: AtomicCell::new(0.0),
        };
        let mob_arc = Arc::new(shulker);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            goal_selector.add_goal(
                1,
                Box::new(LookAtEntityGoal::new(
                    mob_weak,
                    &EntityType::PLAYER,
                    8.0,
                    0.02,
                    true,
                )),
            );
            goal_selector.add_goal(4, Box::new(ShulkerAttackGoal::new(&mob_arc)));
            goal_selector.add_goal(7, Box::new(ShulkerPeekGoal::new(&mob_arc)));
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));

            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| entity_type == &EntityType::SHULKER)
                        .alerting_others(),
                ),
            );
            // Vanilla `ShulkerNearestAttackGoal`. Its search box only applies to non-players.
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true)
                    .gated_by(is_not_peaceful),
            );
            // Vanilla `ShulkerDefenseAttackGoal`: a shulker on a team shoots monsters in front
            // of its shell.
            let defender = Arc::downgrade(&mob_arc);
            target_selector.add_goal(
                3,
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 10, true, move |target, _| {
                    target.entity.entity_type.category == &MobCategory::MONSTER
                        && defender
                            .upgrade()
                            .is_some_and(|shulker| shulker.is_in_defense_area(target))
                })
                .gated_by(|mob| mob.get_team().is_some()),
            );
        };

        mob_arc
    }

    pub fn get_attach_face(&self) -> BlockDirection {
        BlockDirection::try_from(i32::from(self.attach_face.load(Ordering::Relaxed)))
            .unwrap_or(DEFAULT_ATTACH_FACE)
    }

    pub fn set_attach_face(&self, face: BlockDirection) {
        self.attach_face.store(face as u8, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_synced_data(
            pumpkin_data::tracked_data::shulker::ATTACH_FACE_ID,
            VarInt(face as i32),
        );
    }

    pub fn get_color(&self) -> Option<u8> {
        let c = self.color.load(Ordering::Relaxed);
        if c == NO_COLOR { None } else { Some(c) }
    }

    pub fn set_color(&self, color: Option<u8>) {
        let val = color.unwrap_or(NO_COLOR);
        self.color.store(val, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_synced_data(pumpkin_data::tracked_data::shulker::COLOR, val as i8);
    }

    pub fn get_raw_peek(&self) -> u8 {
        self.peek_amount.load(Ordering::Relaxed)
    }

    pub fn set_raw_peek(&self, amount: u8) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let pos = entity.pos.load();

        if amount == 0 {
            world.play_sound_fine(
                Sound::EntityShulkerClose,
                SoundCategory::Hostile,
                &pos,
                1.0,
                1.0,
            );
        } else if self.peek_amount.load(Ordering::Relaxed) == 0 {
            world.play_sound_fine(
                Sound::EntityShulkerOpen,
                SoundCategory::Hostile,
                &pos,
                1.0,
                1.0,
            );
        }

        self.peek_amount.store(amount, Ordering::Relaxed);

        entity.set_synced_data(pumpkin_data::tracked_data::shulker::PEEK_ID, amount);
    }

    /// Vanilla sets `DATA_PEEK_ID` straight after a teleport, without the close sound.
    fn set_raw_peek_silently(&self, amount: u8) {
        self.peek_amount.store(amount, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::shulker::PEEK_ID, amount);
    }

    /// Vanilla `getTargetSearchArea`: four blocks out from the attach face, follow range across.
    fn is_in_defense_area(&self, target: &LivingEntity) -> bool {
        let range = self
            .mob_entity
            .living_entity
            .get_attribute_value(&Attributes::FOLLOW_RANGE);
        let bb = self.mob_entity.living_entity.entity.bounding_box.load();
        let area = match self.get_attach_face().axis_of() {
            Axis::X => bb.expand(4.0, range, range),
            Axis::Z => bb.expand(range, range, 4.0),
            Axis::Y => bb.expand(range, 4.0, range),
        };
        target.entity.bounding_box.load().intersects(&area)
    }

    pub fn is_closed(&self) -> bool {
        self.peek_amount.load(Ordering::Relaxed) == 0
    }

    fn update_peek_amount(&self) -> bool {
        let current = self.current_peek_amount.load();
        self.prev_peek_amount.store(current);
        let target = f32::from(self.get_raw_peek()) * 0.01;

        if (current - target).abs() < 1e-6 {
            return false;
        }

        let new = if current > target {
            (current - PEEK_PER_TICK).max(target)
        } else {
            (current + PEEK_PER_TICK).min(target)
        };

        self.current_peek_amount.store(new);
        true
    }

    fn find_attachable_face(&self, pos: &BlockPos) -> Option<BlockDirection> {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        for dir in BlockDirection::all() {
            let neighbour = pos.offset_direction(dir);
            let state = world.get_block_state(&neighbour);
            if state.is_solid() {
                return Some(dir);
            }
        }
        None
    }

    fn can_stay_at(&self, pos: &BlockPos, face: BlockDirection) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();

        let own_state = world.get_block_state(pos);
        if !own_state.is_air() {
            return false;
        }

        let neighbour = pos.offset_direction(face);
        let state = world.get_block_state(&neighbour);
        state.is_solid()
    }

    fn find_new_attachment(&self) {
        let pos = self.mob_entity.living_entity.entity.block_pos.load();
        if let Some(dir) = self.find_attachable_face(&pos) {
            self.set_attach_face(dir);
        } else {
            self.teleport_somewhere();
        }
    }

    /// Vanilla `Shulker.teleportSomewhere`.
    pub fn teleport_somewhere(&self) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        if self.mob_entity.is_no_ai() || !entity.is_alive() {
            return false;
        }
        let old_pos = entity.block_pos.load();
        let world = entity.world.load();

        for _ in 0..5 {
            let target = {
                let mut rng = self.get_random();
                BlockPos::new(
                    old_pos.0.x + rng.random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
                    old_pos.0.y + rng.random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
                    old_pos.0.z + rng.random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
                )
            };
            if target.0.y <= world.dimension.min_y
                || !world.get_block_state(&target).is_air()
                || !world
                    .worldborder
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_within_bounds(&target)
                || !world.is_space_empty(BoundingBox::from_block(&target).contract_all(1.0e-6))
            {
                continue;
            }
            let Some(dir) = self.find_attachable_face(&target) else {
                continue;
            };
            if world
                .get_block(&target.offset_direction(dir))
                .has_tag(&tag::Block::MINECRAFT_SHULKER_DOES_NOT_TELEPORT_TO)
            {
                continue;
            }

            self.set_attach_face(dir);
            world.play_sound_fine(
                Sound::EntityShulkerTeleport,
                SoundCategory::Hostile,
                &entity.pos.load(),
                1.0,
                1.0,
            );

            let new_pos = Vector3::new(
                f64::from(target.0.x) + 0.5,
                f64::from(target.0.y),
                f64::from(target.0.z) + 0.5,
            );
            entity.set_pos(new_pos);
            world.broadcast_to_chunk(
                entity.chunk_pos.load(),
                &CEntityPositionSync::new(
                    entity.entity_id.into(),
                    new_pos,
                    Vector3::new(0.0, 0.0, 0.0),
                    entity.yaw.load(),
                    entity.pitch.load(),
                    entity.on_ground.load(Ordering::Relaxed),
                ),
            );
            entity.last_sent_pos.store(new_pos);

            world.emit_game_event_with_context(
                "teleport",
                old_pos.to_centered_f64(),
                &GameEventContext {
                    source_entity: Some(self),
                },
            );
            let packed = old_pos.pack_difference_in_position(
                &target,
                MAX_TELEPORT_DISTANCE,
                MAX_TELEPORT_DISTANCE,
                MAX_TELEPORT_DISTANCE,
            );
            world.sync_world_event(WorldEvent::ParticlesShulkerTeleport, old_pos, packed);

            self.set_raw_peek_silently(0);
            self.mob_entity.set_target(None);
            return true;
        }
        false
    }

    /// Vanilla `Shulker.hitByShulkerBullet`: an open shulker hit by a bullet teleports away and
    /// may leave a new shulker behind, less likely the more shulkers are already around.
    fn hit_by_shulker_bullet(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let old_pos = entity.pos.load();
        let old_bb = entity.bounding_box.load();
        if self.is_closed() || !self.teleport_somewhere() {
            return;
        }
        let world = entity.world.load();
        let shulkers = world
            .get_entities_at_box(&old_bb.expand(8.0, 8.0, 8.0))
            .iter()
            .filter(|other| {
                let other = other.get_entity();
                other.entity_type == &EntityType::SHULKER && other.is_alive()
            })
            .count();
        let failure_chance = (shulkers as f32 - 1.0) / 5.0;
        if rand::rng().random::<f32>() < failure_chance {
            return;
        }
        let baby = Self::new(Entity::new(world.clone(), old_pos, &EntityType::SHULKER));
        baby.set_color(self.get_color());
        world.spawn_entity(baby);
    }
}

fn is_not_peaceful(mob: &dyn Mob) -> bool {
    mob.get_entity().world.load().level_info.load().difficulty != Difficulty::Peaceful
}

impl Mob for ShulkerEntity {
    fn swim_splash_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityGenericSplash
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        // Vanilla `Shulker.playAmbientSound`: silent while closed.
        (self.peek_amount.load(std::sync::atomic::Ordering::Relaxed) != 0)
            .then_some(pumpkin_data::sound::Sound::EntityShulkerAmbient)
    }

    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::None
    }

    fn swim_sound(&self) -> pumpkin_data::sound::Sound {
        pumpkin_data::sound::Sound::EntityGenericSwim
    }

    /// Vanilla `AbstractGolem.removeWhenFarAway`.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        false
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_byte("AttachFace", self.attach_face.load(Ordering::Relaxed) as i8);
        nbt.put_byte("PeekAmount", self.peek_amount.load(Ordering::Relaxed) as i8);
        nbt.put_byte("Color", self.color.load(Ordering::Relaxed) as i8);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(face) = nbt.get_byte("AttachFace") {
            self.attach_face.store(face as u8, Ordering::Relaxed);
        }
        if let Some(peek) = nbt.get_byte("PeekAmount") {
            self.peek_amount.store(peek as u8, Ordering::Relaxed);
        }
        if let Some(color) = nbt.get_byte("Color") {
            self.color.store(color as u8, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;

        if !entity.is_alive() {
            return;
        }

        entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));

        self.update_peek_amount();

        let pos = entity.block_pos.load();
        let face = self.get_attach_face();
        if !self.can_stay_at(&pos, face) {
            self.find_new_attachment();
        }
    }

    /// Vanilla `Shulker.hurtServer`, after the damage landed.
    fn on_damage(&self, damage_type: DamageType, source: Option<&dyn EntityBase>) {
        let living = &self.mob_entity.living_entity;
        if living.health.load() < living.get_max_health() * 0.5
            && self.get_random().random_range(0..4) == 0
        {
            self.teleport_somewhere();
        } else if damage_type == DamageType::MOB_PROJECTILE
            && source.is_some_and(|source| {
                let source_type = source.get_entity().entity_type;
                source_type == &EntityType::SHULKER || source_type == &EntityType::SHULKER_BULLET
            })
        {
            // Shulkers fire nothing but bullets, and the bullet credits its shooter.
            self.hit_by_shulker_bullet();
        }
    }

    /// A closed shell turns away any arrow, spectral arrow or trident.
    fn pre_damage(&self, damage_type: DamageType, _source: Option<&dyn EntityBase>) -> bool {
        !(self.is_closed()
            && (damage_type == DamageType::ARROW || damage_type == DamageType::TRIDENT))
    }

    fn modify_incoming_damage(&self, amount: f32, damage_type: DamageType) -> f32 {
        if self.is_closed() && !crate::entity::living::bypasses_armor_durability(&damage_type) {
            const ARMOR: f32 = 20.0;
            let f1 = (ARMOR - amount * 0.5f32).clamp(ARMOR * 0.2f32, 20.0f32);
            (amount * (1.0f32 - f1 / 25.0f32)).max(0.0)
        } else {
            amount
        }
    }
}

struct ShulkerAttackGoal {
    shulker: Weak<ShulkerEntity>,
    attack_cooldown: AtomicI32,
}

impl ShulkerAttackGoal {
    fn new(shulker: &Arc<ShulkerEntity>) -> Self {
        Self {
            shulker: Arc::downgrade(shulker),
            attack_cooldown: AtomicI32::new(20),
        }
    }
}

impl Goal for ShulkerAttackGoal {
    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }

    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let target = mob.get_mob_entity().get_target();
        target.as_ref().is_some_and(|t| t.is_alive()) && is_not_peaceful(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob)
    }

    fn start(&mut self, _mob: &dyn Mob) {
        let Some(shulker) = self.shulker.upgrade() else {
            return;
        };
        self.attack_cooldown.store(20, Ordering::Relaxed);
        shulker.set_raw_peek(100);
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        let Some(shulker) = self.shulker.upgrade() else {
            return;
        };
        shulker.set_raw_peek(0);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(shulker) = self.shulker.upgrade() else {
            return;
        };
        if !is_not_peaceful(mob) {
            return;
        }
        let cooldown = self.attack_cooldown.fetch_sub(1, Ordering::Relaxed) - 1;
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };

        let entity = &mob_entity.living_entity.entity;
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_with_range(
                target_pos.x,
                target_entity.get_eye_y(),
                target_pos.z,
                180.0,
                180.0,
            );

        let shulker_pos = entity.pos.load();
        if shulker_pos.squared_distance_to_vec(&target_pos) >= 400.0 {
            mob_entity.set_target(None);
            return;
        }
        if cooldown > 0 {
            return;
        }
        // Vanilla `20 + random.nextInt(10) * 20 / 2`.
        let new_cooldown = 20 + mob.get_random().random_range(0..10) * 10;
        self.attack_cooldown.store(new_cooldown, Ordering::Relaxed);

        let world = entity.world.load();
        let bullet = ShulkerBulletEntity::new(
            entity,
            target_entity.entity_id,
            target_pos,
            shulker.get_attach_face().axis_of(),
        );
        world.spawn_entity_non_save(Arc::new(bullet));

        let pitch =
            1.0 + (mob.get_random().random::<f32>() - mob.get_random().random::<f32>()) * 0.2;
        world.play_sound_fine(
            Sound::EntityShulkerShoot,
            SoundCategory::Hostile,
            &shulker_pos,
            2.0,
            pitch,
        );
    }
}

struct ShulkerPeekGoal {
    shulker: Weak<ShulkerEntity>,
    peek_time: AtomicI32,
}

impl ShulkerPeekGoal {
    fn new(shulker: &Arc<ShulkerEntity>) -> Self {
        Self {
            shulker: Arc::downgrade(shulker),
            peek_time: AtomicI32::new(0),
        }
    }
}

impl Goal for ShulkerPeekGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(shulker) = self.shulker.upgrade() else {
            return false;
        };
        let has_target = mob.get_mob_entity().get_target().is_some();
        if has_target {
            return false;
        }
        if mob.get_random().random_range(0..to_goal_ticks(40)) != 0 {
            return false;
        }
        let pos = mob.get_mob_entity().living_entity.entity.block_pos.load();
        let face = shulker.get_attach_face();
        shulker.can_stay_at(&pos, face)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let has_target = mob.get_mob_entity().get_target().is_some();
        !has_target && self.peek_time.load(Ordering::Relaxed) > 0
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(shulker) = self.shulker.upgrade() else {
            return;
        };
        let duration = to_goal_ticks(20 * (1 + mob.get_random().random_range(0..3)));
        self.peek_time.store(duration, Ordering::Relaxed);
        shulker.set_raw_peek(30);
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        let Some(shulker) = self.shulker.upgrade() else {
            return;
        };
        let has_target = shulker.mob_entity.get_target().is_some();
        if !has_target {
            shulker.set_raw_peek(0);
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        self.peek_time.fetch_sub(1, Ordering::Relaxed);
    }
}

trait BlockPosExt {
    fn offset_direction(&self, dir: BlockDirection) -> Self;
}

impl BlockPosExt for BlockPos {
    fn offset_direction(&self, dir: BlockDirection) -> Self {
        let offset = dir.to_offset();
        Self::new(
            self.0.x + offset.x,
            self.0.y + offset.y,
            self.0.z + offset.z,
        )
    }
}
