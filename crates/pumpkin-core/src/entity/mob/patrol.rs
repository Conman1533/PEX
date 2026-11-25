use std::sync::atomic::{AtomicBool, Ordering};

use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use pumpkin_world::chunk::ChunkHeightmapType;
use rand::RngExt;

use crate::entity::EntityBase;
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::mob::Mob;
use crate::entity::mob::raider::create_ominous_banner;

pub struct PatrolData {
    pub patrol_target: AtomicCell<Option<BlockPos>>,
    pub patrol_leader: AtomicBool,
    pub patrolling: AtomicBool,
}

impl Default for PatrolData {
    fn default() -> Self {
        Self {
            patrol_target: AtomicCell::new(None),
            patrol_leader: AtomicBool::new(false),
            patrolling: AtomicBool::new(false),
        }
    }
}

pub trait PatrollingMonster: Mob {
    fn get_patrol_data(&self) -> &PatrolData;

    fn can_be_leader(&self) -> bool {
        true
    }

    fn is_patrol_leader(&self) -> bool {
        self.get_patrol_data().patrol_leader.load(Ordering::Relaxed)
    }

    fn set_patrol_leader(&self, is_leader: bool) {
        self.get_patrol_data()
            .patrol_leader
            .store(is_leader, Ordering::Relaxed);
        self.set_patrolling(true);
    }

    fn is_patrolling(&self) -> bool {
        self.get_patrol_data().patrolling.load(Ordering::Relaxed)
    }

    fn set_patrolling(&self, patrolling: bool) {
        self.get_patrol_data()
            .patrolling
            .store(patrolling, Ordering::Relaxed);
    }

    fn get_patrol_target(&self) -> Option<BlockPos> {
        self.get_patrol_data().patrol_target.load()
    }

    fn set_patrol_target(&self, target: BlockPos) {
        self.get_patrol_data().patrol_target.store(Some(target));
        self.set_patrolling(true);
    }

    fn has_patrol_target(&self) -> bool {
        self.get_patrol_data().patrol_target.load().is_some()
    }

    /// Vanilla `PatrollingMonster.canJoinPatrol`, and `Raider`'s override: not while in a raid.
    fn can_join_patrol(&self) -> bool {
        self.as_raider()
            .is_none_or(|raider| !raider.has_active_raid())
    }

    fn find_patrol_target(&self) {
        let block_pos = self.get_mob_entity().living_entity.entity.block_pos.load();
        let dx: i32 = rand::random::<i32>().rem_euclid(1000) - 500;
        let dz: i32 = rand::random::<i32>().rem_euclid(1000) - 500;
        let target = BlockPos(Vector3::new(
            block_pos.0.x + dx,
            block_pos.0.y,
            block_pos.0.z + dz,
        ));
        self.set_patrol_target(target);
    }

    fn finalize_patrol_spawn(&self, is_patrol_spawn: bool) {
        if !is_patrol_spawn && self.can_be_leader() {
            let r: f32 = rand::random();
            if r < 0.06 {
                self.set_patrol_leader(true);
            }
        }

        if self.is_patrol_leader() {
            let banner = create_ominous_banner();
            let living = &self.get_mob_entity().living_entity;
            if let Ok(mut equipment) = living.entity_equipment.try_lock() {
                equipment.put(&EquipmentSlot::HEAD, banner.clone());
                drop(equipment);
                living.send_equipment_changes(&[(EquipmentSlot::HEAD, banner)]);
            }
        }

        if is_patrol_spawn {
            self.set_patrolling(true);
        }
    }

    fn write_patrol_nbt(&self, nbt: &mut NbtCompound) {
        let data = self.get_patrol_data();
        if let Some(target) = data.patrol_target.load() {
            nbt.put_int("PatrolTargetX", target.0.x);
            nbt.put_int("PatrolTargetY", target.0.y);
            nbt.put_int("PatrolTargetZ", target.0.z);
        }
        nbt.put_bool("PatrolLeader", data.patrol_leader.load(Ordering::Relaxed));
        nbt.put_bool("Patrolling", data.patrolling.load(Ordering::Relaxed));
    }

    fn read_patrol_nbt(&self, nbt: &NbtCompound) {
        let data = self.get_patrol_data();
        if let (Some(x), Some(y), Some(z)) = (
            nbt.get_int("PatrolTargetX"),
            nbt.get_int("PatrolTargetY"),
            nbt.get_int("PatrolTargetZ"),
        ) {
            data.patrol_target
                .store(Some(BlockPos(Vector3::new(x, y, z))));
        }
        if let Some(leader) = nbt.get_bool("PatrolLeader") {
            data.patrol_leader.store(leader, Ordering::Relaxed);
        }
        if let Some(patrolling) = nbt.get_bool("Patrolling") {
            data.patrolling.store(patrolling, Ordering::Relaxed);
        }
    }
}

/// Vanilla `PatrollingMonster.LongDistancePatrolGoal.NAVIGATION_FAILED_COOLDOWN`.
const NAVIGATION_FAILED_COOLDOWN: i64 = 200;

/// Vanilla `PatrollingMonster.LongDistancePatrolGoal`.
///
/// A patrol walks toward its patrol target in steps of 10 blocks, the leader picks a new target
/// near the old one, and a patrol whose companions are all gone disbands.
pub struct LongDistancePatrolGoal {
    speed_modifier: f64,
    leader_speed_modifier: f64,
    cooldown_until: i64,
}

impl LongDistancePatrolGoal {
    #[must_use]
    pub const fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            leader_speed_modifier,
            cooldown_until: -1,
        }
    }

    fn game_time(mob: &dyn Mob) -> i64 {
        mob.get_entity()
            .world
            .load()
            .level_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_gametime()
    }

    /// Vanilla `findPatrolCompanions`.
    fn find_patrol_companions(mob: &dyn Mob) -> Vec<Arc<dyn EntityBase>> {
        let entity = mob.get_entity();
        let area = entity.bounding_box.load().expand(16.0, 16.0, 16.0);
        entity
            .world
            .load()
            .get_entities_at_box(&area)
            .into_iter()
            .filter(|other| {
                other.get_entity().entity_id != entity.entity_id
                    && other
                        .get_mob()
                        .and_then(Mob::as_patrolling_monster)
                        .is_some_and(PatrollingMonster::can_join_patrol)
            })
            .collect()
    }

    /// Vanilla `Level.getHeightmapPos(MOTION_BLOCKING_NO_LEAVES, pos)`.
    fn surface(mob: &dyn Mob, x: i32, z: i32) -> BlockPos {
        let world = mob.get_entity().world.load();
        let y = world.get_heightmap_height(ChunkHeightmapType::MotionBlockingNoLeaves, x, z);
        BlockPos::new(x, y, z)
    }

    /// Vanilla `PathNavigation.moveTo(x, y, z, speed)`: false when there is no path.
    fn move_to(mob: &dyn Mob, pos: BlockPos, speed: f64) -> bool {
        let living = &mob.get_mob_entity().living_entity;
        let steered = mob.get_mob_entity().steered();
        let mut navigator = steered.navigator();
        let path = navigator.create_path(living, pos.to_f64(), 1);
        navigator.move_to_path(path, speed, living)
    }

    /// Vanilla `moveRandomly`.
    fn move_randomly(&self, mob: &dyn Mob) -> bool {
        let block_pos = mob.get_entity().block_pos.load();
        let mut rng = mob.get_random();
        let x = block_pos.0.x - 8 + rng.random_range(0..16);
        let z = block_pos.0.z - 8 + rng.random_range(0..16);
        drop(rng);
        Self::move_to(mob, Self::surface(mob, x, z), self.speed_modifier)
    }
}

impl Goal for LongDistancePatrolGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(patrol) = mob.as_patrolling_monster() else {
            return false;
        };
        let is_on_cooldown = Self::game_time(mob) < self.cooldown_until;
        patrol.is_patrolling()
            && mob.get_mob_entity().get_target().is_none()
            && !mob.get_entity().has_passengers()
            && patrol.has_patrol_target()
            && !is_on_cooldown
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(patrol) = mob.as_patrolling_monster() else {
            return;
        };
        if !mob.get_mob_entity().steered().navigator().is_done() {
            return;
        }
        let is_leader = patrol.is_patrol_leader();
        let companions = Self::find_patrol_companions(mob);
        let Some(patrol_target) = patrol.get_patrol_target() else {
            return;
        };
        let pos = mob.get_entity().pos.load();
        if patrol.is_patrolling() && companions.is_empty() {
            patrol.set_patrolling(false);
        } else if is_leader
            && patrol_target
                .to_centered_f64()
                .squared_distance_to_vec(&pos)
                < 10.0 * 10.0
        {
            patrol.find_patrol_target();
        } else {
            // Vanilla rotates the offset by 90 radians, not degrees.
            let target = Vector3::new(
                f64::from(patrol_target.0.x) + 0.5,
                f64::from(patrol_target.0.y),
                f64::from(patrol_target.0.z) + 0.5,
            );
            let offset = pos.sub(&target);
            let (sin, cos) = 90.0f32.sin_cos();
            let (sin, cos) = (f64::from(sin), f64::from(cos));
            let rotated = Vector3::new(
                offset.x * cos + offset.z * sin,
                offset.y,
                offset.z * cos - offset.x * sin,
            );
            let long_distance_target = rotated.multiply(0.4, 0.4, 0.4).add(&target);
            let move_target = long_distance_target
                .sub(&pos)
                .normalize()
                .multiply(10.0, 10.0, 10.0)
                .add(&pos);
            let path_target = BlockPos::floored_v(move_target);
            let path_target = Self::surface(mob, path_target.0.x, path_target.0.z);
            let speed = if is_leader {
                self.leader_speed_modifier
            } else {
                self.speed_modifier
            };
            if !Self::move_to(mob, path_target, speed) {
                self.move_randomly(mob);
                self.cooldown_until = Self::game_time(mob) + NAVIGATION_FAILED_COOLDOWN;
            } else if is_leader {
                for companion in &companions {
                    if let Some(companion) =
                        companion.get_mob().and_then(Mob::as_patrolling_monster)
                    {
                        companion.set_patrol_target(path_target);
                    }
                }
            }
        }
    }
}
