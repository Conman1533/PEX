use crate::entity::passive::horse::HORSE_AMBIENT_SOUND_INTERVAL;
use crate::entity::passive::horse::can_parent;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU8, Ordering},
};

use pumpkin_data::entity::EntityPose;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::{BoundingBox, EntityDimensions};
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::brain::{Brain, memory::PackedMemories, memory::types},
    ai::control::{
        Control, LookControlTrait, MoveControlTrait,
        look_control::LookControl,
        move_control::{MoveControl, Operation},
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

mod ai;

pub const FLAG_TAME: u8 = 2;
pub const FLAG_SADDLE: u8 = 4;
pub const FLAG_BRED: u8 = 8;
pub const FLAG_EATING: u8 = 16;
pub const FLAG_STANDING: u8 = 32;

/// Vanilla `Camel.isInPoseTransition`: sitting down takes 40 ticks, standing up 52.
const SIT_DOWN_TICKS: i64 = 40;
const STAND_UP_TICKS: i64 = 52;
/// Vanilla `Camel.DASH_COOLDOWN_TICKS`.
const DASH_COOLDOWN_TICKS: i32 = 55;
/// Vanilla `Camel.MAX_HEAD_Y_ROT`.
const MAX_HEAD_Y_ROT: f32 = 30.0;
/// Vanilla `Camel.SITTING_HEIGHT_DIFFERENCE`: a sitting adult is this much lower.
const SITTING_HEIGHT_DIFFERENCE: f32 = 1.43;
/// Vanilla `Camel.ADULT_SITTING_DIMENSIONS` eye height.
const ADULT_SITTING_EYE_HEIGHT: f32 = 0.845;

/// The camel and the camel husk (vanilla `CamelHusk extends Camel`).
pub struct CamelEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub flags: AtomicU8,
    pub dashing: AtomicBool,
    /// Vanilla `Camel.dashCooldown`.
    pub dash_cooldown: AtomicI32,
    /// Vanilla `LAST_POSE_CHANGE_TICK`: game time of the last pose change, negative while
    /// sitting.
    pub last_pose_tick: AtomicI64,
    /// A camel husk: never a baby, no breeding, its own food and sounds, and it despawns.
    pub husk: bool,
}

impl CamelEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, false)
    }

    pub fn new_husk(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, true)
    }

    /// Vanilla `Camel` constructor: the Brain of `CamelAi` (`ai.rs`) instead of goals, its own
    /// move and look controls, and a navigation that floats and walks over fences.
    fn with_kind(entity: Entity, husk: bool) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(CamelMoveControl(MoveControl::default()));
        *mob_entity
            .look_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(CamelLookControl(LookControl::default()));
        let navigator = mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        navigator.set_can_float(true);
        navigator.set_can_walk_over_fences(true);
        Arc::new(Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            flags: AtomicU8::new(0),
            dashing: AtomicBool::new(false),
            dash_cooldown: AtomicI32::new(0),
            last_pose_tick: AtomicI64::new(0),
            husk,
        })
    }

    #[must_use]
    pub fn has_flag(&self, flag: u8) -> bool {
        (self.flags.load(Ordering::Relaxed) & flag) != 0
    }

    pub fn set_flag(&self, flag: u8, val: bool) {
        let current = self.flags.load(Ordering::Relaxed);
        let new_flags = if val { current | flag } else { current & !flag };
        self.flags.store(new_flags, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::camel::DATA_ID_FLAGS,
            new_flags as i8,
        );
    }

    #[must_use]
    pub fn is_saddled(&self) -> bool {
        self.has_flag(FLAG_SADDLE)
    }

    pub fn set_saddled(&self, val: bool) {
        self.set_flag(FLAG_SADDLE, val);
    }

    #[must_use]
    pub fn is_dashing(&self) -> bool {
        self.dashing.load(Ordering::Relaxed)
    }

    /// Vanilla `setDashing`, with the server side of `onSyncedDataUpdated`: a change of the dash
    /// flag starts the dash cooldown if none is running.
    pub fn set_dashing(&self, dashing: bool) {
        if self.dashing.swap(dashing, Ordering::Relaxed) != dashing {
            let _ = self.dash_cooldown.compare_exchange(
                0,
                DASH_COOLDOWN_TICKS,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::camel::DASH, dashing);
    }

    fn game_time(&self) -> i64 {
        self.get_entity().world.load().get_world_age()
    }

    fn set_last_pose_tick(&self, tick: i64) {
        self.last_pose_tick.store(tick, Ordering::Relaxed);
        self.get_entity().set_synced_data(
            pumpkin_data::tracked_data::camel::LAST_POSE_CHANGE_TICK,
            tick,
        );
    }

    #[must_use]
    pub fn is_camel_sitting(&self) -> bool {
        self.last_pose_tick.load(Ordering::Relaxed) < 0
    }

    #[must_use]
    pub fn pose_time(&self) -> i64 {
        self.game_time() - self.last_pose_tick.load(Ordering::Relaxed).abs()
    }

    #[must_use]
    pub fn is_in_pose_transition(&self) -> bool {
        self.pose_time()
            < if self.is_camel_sitting() {
                SIT_DOWN_TICKS
            } else {
                STAND_UP_TICKS
            }
    }

    /// Vanilla `Camel.refuseToMove`: sitting, or still sitting down or getting up.
    #[must_use]
    pub fn refuse_to_move(&self) -> bool {
        self.is_camel_sitting() || self.is_in_pose_transition()
    }

    pub fn sit_down(&self) {
        if !self.is_camel_sitting() {
            self.mob_entity.living_entity.make_sound(if self.husk {
                Sound::EntityCamelHuskSit
            } else {
                Sound::EntityCamelSit
            });
            self.set_camel_pose(EntityPose::Sitting);
            self.emit_entity_action();
            self.set_last_pose_tick(-self.game_time());
        }
    }

    pub fn stand_up(&self) {
        if self.is_camel_sitting() {
            self.mob_entity.living_entity.make_sound(if self.husk {
                Sound::EntityCamelHuskStand
            } else {
                Sound::EntityCamelStand
            });
            self.set_camel_pose(EntityPose::Standing);
            self.emit_entity_action();
            self.set_last_pose_tick(self.game_time());
        }
    }

    /// Vanilla `Camel.standUpInstantly`: skips the getting-up animation.
    pub fn stand_up_instantly(&self) {
        self.set_camel_pose(EntityPose::Standing);
        self.emit_entity_action();
        self.set_last_pose_tick((self.game_time() - STAND_UP_TICKS - 1).max(0));
    }

    /// Vanilla `gameEvent(GameEvent.ENTITY_ACTION)`.
    fn emit_entity_action(&self) {
        let entity = self.get_entity();
        entity.world.load().emit_game_event_with_context(
            "entity_action",
            entity.pos.load(),
            &crate::world::game_event::GameEventContext {
                source_entity: Some(self),
            },
        );
    }

    /// Vanilla `Camel.getDefaultDimensions`: an adult sitting camel is 1.43 blocks lower. Babies
    /// keep their box, since this server does not scale babies.
    fn camel_dimensions(&self, pose: EntityPose) -> EntityDimensions {
        let entity = self.get_entity();
        let standing = Entity::type_dimensions(entity.entity_type);
        if pose == EntityPose::Sitting && !self.is_baby() {
            EntityDimensions::new(
                standing.width,
                standing.height - SITTING_HEIGHT_DIFFERENCE,
                ADULT_SITTING_EYE_HEIGHT,
            )
        } else {
            standing
        }
    }

    /// Vanilla `setPose` followed by `refreshDimensions` with the camel's sitting box.
    fn set_camel_pose(&self, pose: EntityPose) {
        let entity = self.get_entity();
        entity.set_pose_keep_dimensions(pose);
        let dimensions = self.camel_dimensions(pose);
        let pos = entity.pos.load();
        entity
            .bounding_box
            .store(BoundingBox::new_from_pos(pos.x, pos.y, pos.z, &dimensions));
        entity.entity_dimension.store(dimensions);
    }

    /// Vanilla `Camel.canCamelChangePose`: the box of the other pose is free of blocks.
    #[must_use]
    pub fn can_camel_change_pose(&self) -> bool {
        let entity = self.get_entity();
        let target = if self.is_camel_sitting() {
            EntityPose::Standing
        } else {
            EntityPose::Sitting
        };
        let pos = entity.pos.load();
        let bounding_box =
            BoundingBox::new_from_pos(pos.x, pos.y, pos.z, &self.camel_dimensions(target));
        entity.world.load().is_space_empty(bounding_box)
    }

    /// Vanilla `AbstractHorse.isMobControlled` (false) and `CamelHusk.isMobControlled`: a camel
    /// husk is steered by the mob riding in front.
    #[must_use]
    pub fn is_mob_controlled(&self) -> bool {
        self.husk
            && self
                .get_entity()
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .first()
                .is_some_and(|passenger| passenger.get_mob().is_some())
    }
}

/// Vanilla `Camel.CamelMoveControl`: a sitting camel told to walk gets up first, unless leashed.
struct CamelMoveControl(MoveControl);

impl Control for CamelMoveControl {}

impl MoveControlTrait for CamelMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if self.0.operation == Operation::MoveTo
            && !mob.get_entity().is_leashed()
            && let Some(camel) = mob.cast_any().downcast_ref::<CamelEntity>()
            && camel.is_camel_sitting()
            && !camel.is_in_pose_transition()
            && camel.can_camel_change_pose()
        {
            camel.stand_up();
        }
        self.0.tick(mob);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.0.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forward: f32, right: f32) {
        self.0.strafe(forward, right);
    }

    fn has_wanted(&self) -> bool {
        self.0.has_wanted()
    }

    fn wanted_position(&self) -> Option<Vector3<f64>> {
        MoveControlTrait::wanted_position(&self.0)
    }
}

/// Vanilla `Camel.CamelLookControl`: no head turning while someone steers it.
struct CamelLookControl(LookControl);

impl Control for CamelLookControl {}

impl LookControlTrait for CamelLookControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if mob.mob_controlling_passenger().is_none() {
            LookControlTrait::tick(&mut self.0, mob);
        }
    }

    fn look_at_with_range(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        max_yaw_change: f32,
        max_pitch_change: f32,
    ) {
        LookControlTrait::look_at_with_range(
            &mut self.0,
            x,
            y,
            z,
            max_yaw_change,
            max_pitch_change,
        );
    }

    fn looking_at(&self) -> Option<Vector3<f64>> {
        LookControlTrait::looking_at(&self.0)
    }
}

impl AgeableMob for CamelEntity {
    /// Vanilla `AbstractHorse.finalizeSpawn`: a new group has a 20% baby chance.
    fn ageable_spawn_group(
        &self,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        crate::entity::mob::spawn::ageable_group_or(group_data, true, 0.2)
    }

    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    /// Vanilla `CamelHusk.canBeABaby`.
    fn can_be_a_baby(&self) -> bool {
        !self.husk
    }
}

impl Animal for CamelEntity {
    fn play_eating_sound(&self) {
        crate::entity::passive::animal::play_eating_sound(
            self,
            if self.husk {
                pumpkin_data::sound::Sound::EntityCamelHuskEat
            } else {
                pumpkin_data::sound::Sound::EntityCamelEat
            },
            1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2,
        );
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(if self.husk {
            &tag::Item::MINECRAFT_CAMEL_HUSK_FOOD
        } else {
            &tag::Item::MINECRAFT_CAMEL_FOOD
        })
    }

    /// Vanilla `Camel.canMate`. Camels always count as tamed; camel husks never mate.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        !self.husk
            && partner.get_entity().entity_id != self.get_entity().entity_id
            && partner
                .cast_any()
                .downcast_ref::<Self>()
                .is_some_and(|other| !other.husk)
            && can_parent(self, true)
            && can_parent(partner, true)
    }
}

impl Mob for CamelEntity {
    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        crate::entity::mob::saddled_player_controller(self.get_entity(), self.is_saddled()).or_else(
            || {
                crate::entity::mob::mob_rider_controller(
                    self.get_entity(),
                    self.get_mob_entity().is_no_ai(),
                )
            },
        )
    }

    fn travel(&self, _caller: &dyn EntityBase) -> bool {
        crate::entity::mob::player_steers(self)
    }

    /// Vanilla `Camel.canJump`: not while sitting or getting up, or before the last dash's
    /// cooldown is over.
    fn can_rider_jump(&self) -> bool {
        self.is_saddled()
            && !self.refuse_to_move()
            && self.dash_cooldown.load(Ordering::Relaxed) <= 0
    }

    /// Vanilla `Camel.handleStartJump`: the dash sound, and the dash flag others see, which
    /// starts the cooldown.
    fn handle_start_jump(&self, _power: i32) {
        self.mob_entity
            .living_entity
            .make_sound(Sound::EntityCamelDash);
        self.set_dashing(true);
    }

    fn get_sound_volume(&self) -> f32 {
        0.8
    }

    fn step_sound_on(
        &self,
        _world: &crate::world::World,
        _pos: &pumpkin_util::math::position::BlockPos,
        block: &'static pumpkin_data::Block,
        state: &'static pumpkin_data::BlockState,
    ) -> Option<(pumpkin_data::sound::Sound, f32, f32)> {
        let sand = block.has_tag(&pumpkin_data::tag::Block::MINECRAFT_CAMEL_SAND_STEP_SOUND_BLOCKS);
        let _ = state;
        Some(match (self.husk, sand) {
            (true, true) => (
                pumpkin_data::sound::Sound::EntityCamelHuskStepSand,
                0.4,
                1.0,
            ),
            (true, false) => (pumpkin_data::sound::Sound::EntityCamelHuskStep, 0.4, 1.0),
            (false, true) => (pumpkin_data::sound::Sound::EntityCamelStepSand, 1.0, 1.0),
            (false, false) => (pumpkin_data::sound::Sound::EntityCamelStep, 1.0, 1.0),
        })
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.husk {
            Sound::EntityCamelHuskAmbient
        } else {
            Sound::EntityCamelAmbient
        })
    }

    /// Vanilla `CamelHusk.removeWhenFarAway`.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        self.husk
    }

    /// Vanilla `CamelHusk.canBeLeashed`.
    fn can_be_leashed(&self) -> bool {
        !self.is_mob_controlled()
    }

    fn get_max_head_rotation(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        ai::make_brain(self, packed)
    }

    /// Vanilla `PathfinderMob.isPanicking`: the brain's `is_panicking` memory.
    fn mob_is_panicking(&self) -> bool {
        self.mob_entity
            .try_with_brain(self, |brain| {
                brain.has_memory_value(types::IS_PANICKING.id())
            })
            .unwrap_or(false)
    }

    /// Vanilla `Camel.tick`: the dash ends on landing and its cooldown counts down.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = self.get_entity();
        if self.is_dashing()
            && self.dash_cooldown.load(Ordering::Relaxed) < 50
            && (entity.on_ground.load(Ordering::Relaxed)
                || entity.is_in_water()
                || entity.touching_lava.load(Ordering::Relaxed)
                || entity.has_vehicle())
        {
            self.set_dashing(false);
        }
        let cooldown = self.dash_cooldown.load(Ordering::Relaxed);
        if cooldown > 0 {
            self.dash_cooldown.store(cooldown - 1, Ordering::Relaxed);
            if cooldown == 1 {
                entity.world.load().play_sound(
                    if self.husk {
                        Sound::EntityCamelHuskDashReady
                    } else {
                        Sound::EntityCamelDashReady
                    },
                    SoundCategory::Neutral,
                    &entity.block_pos.load().to_f64(),
                );
            }
        }
    }

    fn spawn_as_baby(&self) -> bool {
        !self.husk && {
            self.set_baby(true);
            true
        }
    }

    /// Vanilla `AbstractHorse.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        HORSE_AMBIENT_SOUND_INTERVAL
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        crate::entity::write_saddle_equipment(nbt, self.is_saddled());
        nbt.put_long("LastPoseTick", self.last_pose_tick.load(Ordering::Relaxed));
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(saddled) = crate::entity::saved_saddle(nbt) {
            self.set_saddled(saddled);
        }
        if let Some(pose_tick) = nbt.get_long("LastPoseTick") {
            if pose_tick < 0 {
                self.set_camel_pose(EntityPose::Sitting);
            }
            self.set_last_pose_tick(pose_tick);
        }
    }

    /// Vanilla `Camel.customServerAiStep` (the brain, then `CamelAi.updateActivity`), the water
    /// check of `Camel.tick`, and `travel`: a sitting camel, or one sitting down or getting up,
    /// stays put.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
        ai::update_activity(self);
        let entity = self.get_entity();
        if self.is_camel_sitting() && entity.is_in_water() {
            self.stand_up_instantly();
        }
        if self.refuse_to_move() && entity.on_ground.load(Ordering::Relaxed) {
            self.mob_entity.steered().navigator().stop();
            let pos = entity.pos.load();
            self.mob_entity
                .move_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_wanted_position(pos.x, pos.y, pos.z, 0.0);
            let velocity = entity.velocity.load();
            entity.velocity.store(Vector3::new(0.0, velocity.y, 0.0));
        }
    }

    /// Vanilla `Camel.actuallyHurt`.
    fn on_damage(
        &self,
        _damage_type: pumpkin_data::damage::DamageType,
        _source: Option<&dyn EntityBase>,
    ) {
        self.stand_up_instantly();
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::camel::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::camel::DATA_ID_FLAGS,
            self.flags.load(Ordering::Relaxed) as i8,
        );
        entity.set_synced_data(pumpkin_data::tracked_data::camel::DASH, self.is_dashing());
        // Vanilla `finalizeSpawn`: a new camel starts fully standing.
        if self.last_pose_tick.load(Ordering::Relaxed) == 0 {
            self.last_pose_tick.store(
                (self.game_time() - STAND_UP_TICKS - 1).max(0),
                Ordering::Relaxed,
            );
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::camel::LAST_POSE_CHANGE_TICK,
            self.last_pose_tick.load(Ordering::Relaxed),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.husk {
            // Vanilla `CamelHusk.interact`.
            self.mob_entity
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
        let item = item_stack.get_item();

        if item == &Item::SADDLE && !self.is_saddled() && !self.is_baby() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                if self.husk {
                    Sound::EntityCamelHuskSaddle
                } else {
                    Sound::EntityCamelSaddle
                },
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            return true;
        }

        if self.is_saddled() && !self.is_baby() && !self.is_food(item_stack) {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                // Vanilla stands up once the rider pushes forward; nothing steers a sitting
                // camel here, so it gets up when mounted.
                self.stand_up();
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack)
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_util::math::vector3::Vector3;

    use super::CamelEntity;
    use crate::entity::ageable::AgeableMob;
    use crate::entity::mob::Mob;
    use crate::entity::passive::animal::Animal;
    use crate::entity::r#type::from_type;
    use crate::world::test_world;

    #[tokio::test]
    async fn camel_husk_is_a_camel_that_eats_rabbit_feet_and_never_breeds() {
        let world = test_world();
        let spawn = |entity_type| {
            from_type(
                entity_type,
                Vector3::new(0.5, 64.0, 0.5),
                &world,
                uuid::Uuid::new_v4(),
            )
        };
        let husk = spawn(&EntityType::CAMEL_HUSK);
        let other_husk = spawn(&EntityType::CAMEL_HUSK);
        let camel = spawn(&EntityType::CAMEL);
        let husk_camel = husk
            .cast_any()
            .downcast_ref::<CamelEntity>()
            .expect("a camel entity");
        let camel_entity = camel
            .cast_any()
            .downcast_ref::<CamelEntity>()
            .expect("a camel entity");
        assert!(husk_camel.husk && !camel_entity.husk);

        let rabbit_foot = ItemStack::new(1, &Item::RABBIT_FOOT);
        let cactus = ItemStack::new(1, &Item::CACTUS);
        assert!(husk_camel.is_food(&rabbit_foot) && !husk_camel.is_food(&cactus));
        assert!(camel_entity.is_food(&cactus) && !camel_entity.is_food(&rabbit_foot));

        assert!(!husk_camel.can_be_a_baby() && !husk_camel.spawn_as_baby());
        assert!(husk_camel.remove_when_far_away(0.0));
        assert!(!camel_entity.remove_when_far_away(0.0));
        let other = other_husk.get_mob().expect("a mob");
        assert!(!husk_camel.can_mate(other));
    }
}
