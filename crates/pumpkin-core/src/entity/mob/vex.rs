use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use rand::RngExt;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        control::{Control, MoveControlTrait, move_control::Operation},
        goal::{
            Controls, Goal, active_target::ActiveTargetGoal, look_at_entity::LookAtEntityGoal,
            revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks, track_target::TrackTargetGoal,
        },
        target_predicate::TargetPredicate,
    },
    mob::{Mob, MobEntity},
};

/// Vanilla `Vex.FLAG_IS_CHARGING`.
const FLAG_IS_CHARGING: u8 = 1;

pub struct VexEntity {
    pub mob_entity: MobEntity,
    owner: Mutex<Option<Weak<dyn EntityBase>>>,
    owner_uuid: Mutex<Option<Uuid>>,
    bound_origin: Mutex<Option<BlockPos>>,
    has_limited_life: AtomicBool,
    limited_life_ticks: AtomicI32,
    flags: AtomicU8,
}

impl VexEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Box::new(VexMoveControl::default());
        let vex = Self {
            mob_entity,
            owner: Mutex::new(None),
            owner_uuid: Mutex::new(None),
            bound_origin: Mutex::new(None),
            has_limited_life: AtomicBool::new(false),
            limited_life_ticks: AtomicI32::new(0),
            flags: AtomicU8::new(0),
        };
        let mob_arc = Arc::new(vex);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(4, Box::new(VexChargeAttackGoal));
            goal_selector.add_goal(8, Box::new(VexRandomMoveGoal));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak,
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(2, Box::new(VexCopyOwnerTargetGoal::new()));
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
        };

        mob_arc
    }

    /// Vanilla `Vex.setOwner`.
    pub fn set_owner(&self, owner: &Arc<dyn EntityBase>) {
        *self.owner.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::downgrade(owner));
        *self
            .owner_uuid
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(owner.get_entity().entity_uuid);
    }

    /// Vanilla `Vex.getOwner`: the summoner, looked up again after a reload.
    pub fn get_owner(&self) -> Option<Arc<dyn EntityBase>> {
        let mut owner = self.owner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = owner.as_ref().and_then(Weak::upgrade) {
            return Some(found);
        }
        let uuid = (*self
            .owner_uuid
            .lock()
            .unwrap_or_else(PoisonError::into_inner))?;
        let found = self
            .mob_entity
            .living_entity
            .entity
            .world
            .load()
            .get_entity_by_uuid(uuid)?;
        *owner = Some(Arc::downgrade(&found));
        Some(found)
    }

    /// Vanilla `Vex.setBoundOrigin`: the spot its random flights stay around.
    pub fn set_bound_origin(&self, pos: Option<BlockPos>) {
        *self
            .bound_origin
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = pos;
    }

    fn bound_origin(&self) -> Option<BlockPos> {
        *self
            .bound_origin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Vanilla `Vex.setLimitedLife`: hurt by starvation every second once `ticks` run out.
    pub fn set_limited_life(&self, ticks: i32) {
        self.has_limited_life.store(true, Ordering::Relaxed);
        self.limited_life_ticks.store(ticks, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_charging(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & FLAG_IS_CHARGING != 0
    }

    pub fn set_is_charging(&self, charging: bool) {
        let old = self.flags.load(Ordering::Relaxed);
        let new = if charging {
            old | FLAG_IS_CHARGING
        } else {
            old & !FLAG_IS_CHARGING
        };
        if new != old {
            self.flags.store(new, Ordering::Relaxed);
            self.mob_entity
                .living_entity
                .entity
                .set_synced_data(tracked_data::vex::DATA_FLAGS_ID, new as i8);
        }
    }
}

fn as_vex(mob: &dyn Mob) -> Option<&VexEntity> {
    mob.cast_any().downcast_ref::<VexEntity>()
}

fn has_wanted(mob: &dyn Mob) -> bool {
    mob.get_mob_entity()
        .move_control
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .has_wanted()
}

fn set_wanted(mob: &dyn Mob, pos: Vector3<f64>, speed: f64) {
    mob.get_mob_entity()
        .move_control
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .set_wanted_position(pos.x, pos.y, pos.z, speed);
}

impl Mob for VexEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityVexAmbient)
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn mob_tick(&self, caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        // Vanilla sets `noPhysics` around its own tick: vexes fly through blocks.
        entity.no_physics.store(true, Ordering::Relaxed);
        if !entity.has_no_gravity() {
            entity.set_has_no_gravity(true);
        }
        if self.has_limited_life.load(Ordering::Relaxed)
            && self.limited_life_ticks.fetch_sub(1, Ordering::Relaxed) - 1 <= 0
        {
            self.limited_life_ticks.store(20, Ordering::Relaxed);
            caller.damage(caller, 1.0, DamageType::STARVE);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(pos) = self.bound_origin() {
            nbt.put(
                "bound_pos",
                NbtTag::IntArray(vec![pos.0.x, pos.0.y, pos.0.z]),
            );
        }
        if self.has_limited_life.load(Ordering::Relaxed) {
            nbt.put_int(
                "life_ticks",
                self.limited_life_ticks.load(Ordering::Relaxed),
            );
        }
        if let Some(owner) = *self
            .owner_uuid
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            nbt.put_uuid("owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(&[x, y, z]) = nbt.get_int_array("bound_pos") {
            self.set_bound_origin(Some(BlockPos::new(x, y, z)));
        }
        if let Some(ticks) = nbt.get_int("life_ticks") {
            self.set_limited_life(ticks);
        }
        if let Some(owner) = nbt.get_uuid("owner") {
            *self
                .owner_uuid
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(owner);
        }
    }
}

/// Vanilla `Vex.VexMoveControl`: flies straight at the wanted spot, ignoring blocks.
#[derive(Default)]
struct VexMoveControl {
    wanted: Vector3<f64>,
    speed_modifier: f64,
    operation: Operation,
}

impl Control for VexMoveControl {}

impl MoveControlTrait for VexMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if self.operation != Operation::MoveTo {
            return;
        }
        let entity = &mob.get_mob_entity().living_entity.entity;
        let pos = entity.pos.load();
        let delta = self.wanted - pos;
        let length = delta.length();
        let bb = entity.bounding_box.load();
        let size = ((bb.max.x - bb.min.x) + (bb.max.y - bb.min.y) + (bb.max.z - bb.min.z)) / 3.0;
        if length < size {
            self.operation = Operation::Wait;
            entity.velocity.store(entity.velocity.load() * 0.5);
            return;
        }
        let velocity = entity.velocity.load() + delta * (self.speed_modifier * 0.05 / length);
        entity.velocity.store(velocity);
        let yaw = mob.get_mob_entity().get_target().map_or_else(
            || -(velocity.x.atan2(velocity.z).to_degrees() as f32),
            |target| {
                let target_pos = target.get_entity().pos.load();
                -((target_pos.x - pos.x)
                    .atan2(target_pos.z - pos.z)
                    .to_degrees() as f32)
            },
        );
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.wanted = Vector3::new(x, y, z);
        self.speed_modifier = speed_modifier;
        self.operation = Operation::MoveTo;
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }
}

/// Vanilla `Vex.VexChargeAttackGoal`: dives at the target's eyes.
struct VexChargeAttackGoal;

impl Goal for VexChargeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target().clone() else {
            return false;
        };
        if has_wanted(mob) || mob.get_random().random_range(0..to_goal_ticks(7)) != 0 {
            return false;
        }
        mob.get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target.get_entity().pos.load())
            > 4.0
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        has_wanted(mob)
            && as_vex(mob).is_some_and(VexEntity::is_charging)
            && mob
                .get_mob_entity()
                .get_target()
                .as_ref()
                .is_some_and(|target| target.get_entity().is_alive())
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target().clone() {
            set_wanted(mob, target.get_entity().get_eye_pos(), 1.0);
        }
        if let Some(vex) = as_vex(mob) {
            vex.set_is_charging(true);
        }
        let entity = mob.get_entity();
        entity.world.load().play_sound_fine(
            Sound::EntityVexCharge,
            SoundCategory::Hostile,
            &entity.pos.load(),
            1.0,
            1.0,
        );
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(vex) = as_vex(mob) {
            vex.set_is_charging(false);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target().clone() else {
            return;
        };
        let entity = mob.get_entity();
        if entity
            .bounding_box
            .load()
            .intersects(&target.get_entity().bounding_box.load())
        {
            if mob.get_mob_entity().try_attack(entity, target.as_ref()) {
                mob.on_attack(target.as_ref());
            }
            if let Some(vex) = as_vex(mob) {
                vex.set_is_charging(false);
            }
        } else if entity
            .pos
            .load()
            .squared_distance_to_vec(&target.get_entity().pos.load())
            < 9.0
        {
            set_wanted(mob, target.get_entity().get_eye_pos(), 1.0);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Vex.VexRandomMoveGoal`: drifts to a random open spot near its bound origin.
struct VexRandomMoveGoal;

impl Goal for VexRandomMoveGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !has_wanted(mob) && mob.get_random().random_range(0..to_goal_ticks(7)) == 0
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    // Vanilla does this in its single `tick`, which runs right after `start`.
    fn start(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let origin = as_vex(mob)
            .and_then(VexEntity::bound_origin)
            .unwrap_or_else(|| entity.block_pos.load());
        let world = entity.world.load();
        let mut random = mob.get_random();
        for _ in 0..3 {
            let pos = origin.offset(Vector3::new(
                random.random_range(0..15) - 7,
                random.random_range(0..11) - 5,
                random.random_range(0..15) - 7,
            ));
            if world.get_block_state(&pos).is_air() {
                let center = pos.to_centered_f64();
                set_wanted(mob, center, 0.25);
                if mob.get_mob_entity().get_target().is_none() {
                    mob.get_mob_entity()
                        .look_control
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .look_at_with_range(center.x, center.y, center.z, 180.0, 20.0);
                }
                break;
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Vex.VexCopyOwnerTargetGoal`: attacks whatever its summoner is fighting.
struct VexCopyOwnerTargetGoal {
    track_target_goal: TrackTargetGoal,
    target_predicate: TargetPredicate,
}

impl VexCopyOwnerTargetGoal {
    fn new() -> Self {
        Self {
            track_target_goal: TrackTargetGoal::with_default(false),
            target_predicate: TargetPredicate::create_non_attackable().ignore_visibility(),
        }
    }

    fn owner_target(mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let owner = as_vex(mob)?.get_owner()?;
        owner.get_mob()?.get_mob_entity().get_target()
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let target = Self::owner_target(mob);
        self.track_target_goal
            .can_track(mob, target.as_deref(), &self.target_predicate)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.track_target_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.set_mob_target(Self::owner_target(mob));
        self.track_target_goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.track_target_goal.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.track_target_goal.controls()
    }
}
