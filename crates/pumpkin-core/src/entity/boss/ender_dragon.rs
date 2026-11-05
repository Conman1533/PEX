use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::atomic_f32::AtomicF32;
use pumpkin_util::math::boundingbox::EntityDimensions;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::math::{cos, sin};
use pumpkin_world::{chunk::ChunkHeightmapType, world::BlockFlags};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use crate::entity::{
    Entity, EntityBase,
    ai::target_predicate::TargetPredicate,
    living::{LivingEntity, can_be_seen_as_enemy},
    mob::{Mob, MobEntity},
    player::Player,
};

pub mod flight_history;
pub mod phase;

use flight_history::DragonFlightHistory;
use phase::{EnderDragonPhase, PhaseManager};

/// Vanilla `damage / 4 + min(damage, 1)` for every part but the head and neck.
fn reduce_body_damage(amount: f32) -> f32 {
    amount / 4.0 + amount.min(1.0)
}

pub const NODE_COUNT: usize = 24;
pub const NODE_Y: i32 = 105;
pub const DEATH_TIMER_MAX: i32 = 200;
const GROWL_COOLDOWN_BASE: i32 = 200;
const FLAP_SPEED: f32 = 0.2;
/// Width and height of the head part (vanilla `new EnderDragonPart(this, "head", 1.0F, 1.0F)`).
pub const HEAD_SIZE: f64 = 1.0;
const SITTING_ALLOWED_DAMAGE_PERCENTAGE: f32 = 0.25;
/// Width and height of each part in `EnderDragonEntity::parts`, in vanilla's order: head, neck,
/// body, three tail parts and two wings.
const PART_SIZES: [(f32, f32); 8] = [
    (1.0, 1.0),
    (3.0, 3.0),
    (5.0, 3.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (4.0, 2.0),
    (4.0, 2.0),
];
const HEAD_PART: usize = 0;
const NECK_PART: usize = 1;
const DRAGON_PHASE_KEY: &str = "DragonPhase";
const DRAGON_DEATH_TIME_KEY: &str = "DragonDeathTime";
const SITTING_DAMAGE_RECEIVED_KEY: &str = "sitting_damage_received";

const NODE_ADJACENCY: [u32; NODE_COUNT] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057,
    3166214, 2138120, 6373424, 4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712,
    8257536,
];

/// A node of the dragon's flight graph, or the final point appended to a path (vanilla `Node`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragonNode {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl DragonNode {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    fn distance_to_sqr(self, other: Self) -> f32 {
        let xd = (other.x - self.x) as f32;
        let yd = (other.y - self.y) as f32;
        let zd = (other.z - self.z) as f32;
        xd * xd + yd * yd + zd * zd
    }

    fn distance_to(self, other: Self) -> f32 {
        self.distance_to_sqr(other).sqrt()
    }
}

/// Vanilla `Path` as the dragon uses it: nodes from the start node on, and the index of the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragonPath {
    nodes: Vec<DragonNode>,
    next: usize,
}

impl DragonPath {
    pub const fn advance(&mut self) {
        self.next += 1;
    }

    /// Vanilla `while (!path.isDone()) path.advance()`.
    pub const fn finish(&mut self) {
        self.next = self.nodes.len();
    }

    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.next >= self.nodes.len()
    }

    #[must_use]
    pub fn next_node_pos(&self) -> Option<DragonNode> {
        self.nodes.get(self.next).copied()
    }
}

pub struct EnderDragonPart {
    pub entity: Entity,
    pub dragon_uuid: uuid::Uuid,
    /// Index in `EnderDragonEntity::parts`.
    pub index: usize,
}

impl EnderDragonPart {
    pub const fn new(entity: Entity, dragon_uuid: uuid::Uuid, index: usize) -> Self {
        Self {
            entity,
            dragon_uuid,
            index,
        }
    }
}

impl EntityBase for EnderDragonPart {
    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn damage(&self, caller: &dyn EntityBase, amount: f32, damage_type: DamageType) -> bool {
        self.damage_with_context(caller, amount, damage_type, None, None, None)
    }

    /// Vanilla `EnderDragonPart.hurtServer`: the dragon takes the hit for this part.
    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        amount: f32,
        damage_type: DamageType,
        position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        let world = self.entity.world.load();
        let Some(dragon) = world.get_entity_by_uuid(self.dragon_uuid) else {
            return false;
        };
        let Some(ender_dragon) = dragon.cast_any().downcast_ref::<EnderDragonEntity>() else {
            return false;
        };
        ender_dragon.hurt_part(
            dragon.as_ref(),
            self.index,
            amount,
            damage_type,
            position,
            source,
            cause,
        )
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }
    fn can_hit(&self) -> bool {
        true
    }
}

pub struct EnderDragonEntity {
    pub mob_entity: MobEntity,
    pub parts: Vec<Arc<EnderDragonPart>>,

    pub flight_history: Mutex<DragonFlightHistory>,
    pub flap_time: Mutex<f32>,
    pub o_flap_time: Mutex<f32>,

    pub in_wall: Mutex<bool>,
    pub dragon_death_time: Mutex<i32>,
    pub sitting_damage_received: Mutex<f32>,
    /// Health before the hit being handled, for `sitting_damage_received`.
    health_before_hurt: AtomicF32,

    pub fight_origin: Mutex<BlockPos>,

    pub phase_manager: PhaseManager,
    pub growl_time: Mutex<i32>,
    pub yaw_rot_accel: Mutex<f32>,

    nodes: Mutex<Option<[DragonNode; NODE_COUNT]>>,
    /// The crystal healing the dragon (vanilla `nearestCrystal`).
    nearest_crystal: Mutex<Option<uuid::Uuid>>,
}

impl EnderDragonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        entity.no_physics.store(true, Ordering::Relaxed);
        let base_id = entity.entity_id;
        let dragon_uuid = entity.entity_uuid;
        let world = entity.world.load();

        let _ = Entity::reserve_ids(8);

        let mut parts = Vec::new();
        for (i, (width, height)) in (1..).zip(PART_SIZES) {
            let part_entity = Entity::from_uuid_with_id(
                base_id + i,
                uuid::Uuid::new_v4(),
                world.clone(),
                entity.pos.load(),
                &EntityType::ENDER_DRAGON,
            );
            part_entity
                .entity_dimension
                .store(EntityDimensions::new(width, height, height * 0.85));
            let part = Arc::new(EnderDragonPart::new(
                part_entity,
                dragon_uuid,
                i as usize - 1,
            ));
            // TODO: world.add_entity_silent(part.clone() as Arc<dyn EntityBase>);
            parts.push(part);
        }

        Arc::new(Self {
            mob_entity: MobEntity::new(entity),
            parts,
            flight_history: Mutex::new(DragonFlightHistory::default()),
            flap_time: Mutex::new(0.0),
            o_flap_time: Mutex::new(0.0),
            in_wall: Mutex::new(false),
            dragon_death_time: Mutex::new(0),
            sitting_damage_received: Mutex::new(0.0),
            health_before_hurt: AtomicF32::new(0.0),
            fight_origin: Mutex::new(BlockPos::new(0, 128, 0)),
            phase_manager: PhaseManager::new(),
            growl_time: Mutex::new(GROWL_COOLDOWN_BASE),
            yaw_rot_accel: Mutex::new(0.0),
            nodes: Mutex::new(None),
            nearest_crystal: Mutex::new(None),
        })
    }

    /// Vanilla `EnderDragon.hurt(part, ...)`: only the head and neck take full damage, and only
    /// players and `always_hurts_ender_dragons` damage hurt the dragon at all.
    #[expect(clippy::too_many_arguments)]
    pub fn hurt_part(
        &self,
        caller: &dyn EntityBase,
        part: usize,
        amount: f32,
        damage_type: DamageType,
        position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        if !self.pre_damage_with_context(damage_type, source, cause) {
            return false;
        }
        let amount = if part == HEAD_PART || part == NECK_PART {
            amount
        } else {
            reduce_body_damage(amount)
        };
        if amount < 0.01 {
            return false;
        }
        let damaged = self.mob_entity.living_entity.damage_with_context(
            caller,
            amount,
            damage_type,
            position,
            source,
            cause,
        );
        if damaged {
            self.reset_ambient_sound_time();
            self.on_damage(damage_type, source);
        }
        damaged
    }

    pub fn set_fight_origin(&self, pos: BlockPos) {
        if let Ok(mut origin) = self.fight_origin.try_lock() {
            *origin = pos;
        }
    }

    #[must_use]
    pub fn phase(&self) -> EnderDragonPhase {
        self.phase_manager.current()
    }

    /// Vanilla `EnderDragonPhaseManager.setPhase`: ends the old phase, syncs `DATA_PHASE` and
    /// begins the new one.
    pub fn set_phase(&self, target: EnderDragonPhase) {
        let previous = self.phase_manager.current();
        if previous == target {
            return;
        }
        self.phase_manager.get(previous).end(self);
        self.phase_manager.set_current(target);
        self.get_entity()
            .set_synced_data(tracked_data::ender_dragon::DATA_PHASE, VarInt(target.id()));
        self.phase_manager.get(target).begin(self);
    }

    /// Vanilla `getDragonFight().aliveCrystals()`; `None` when this dragon isn't the fight's
    /// dragon, which vanilla treats like a fight without crystals in most places.
    #[must_use]
    pub fn fight_alive_crystals(&self) -> Option<i32> {
        let world = self.get_entity().world.load();
        let fight = world
            .dragon_fight
            .as_ref()?
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (fight.dragon_uuid() == Some(self.get_entity().entity_uuid)).then(|| fight.alive_crystals())
    }

    /// Vanilla `level.getHeightmapPos(heightmap, EnderDragonFight.getPodiumLocation(fightOrigin))`.
    #[must_use]
    pub fn podium_pos(&self, heightmap: ChunkHeightmapType) -> BlockPos {
        let origin = *self
            .fight_origin
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let world = self.get_entity().world.load();
        let y = world.get_heightmap_height(heightmap, origin.0.x, origin.0.z);
        BlockPos::new(origin.0.x, y, origin.0.z)
    }

    #[must_use]
    pub fn horizontal_speed(&self) -> f32 {
        self.get_entity().velocity.load().horizontal_length() as f32
    }

    /// Vanilla `level.getNearestPlayer(conditions, dragon, x, y, z)`.
    #[must_use]
    pub fn nearest_player(
        &self,
        targeting: &TargetPredicate,
        pos: Vector3<f64>,
    ) -> Option<Arc<Player>> {
        self.nearest_player_matching(targeting, pos, |_| true)
    }

    /// [`Self::nearest_player`] with an extra selector, like `TargetingConditions.selector`.
    pub fn nearest_player_matching(
        &self,
        targeting: &TargetPredicate,
        pos: Vector3<f64>,
        selector: impl Fn(&Player) -> bool,
    ) -> Option<Arc<Player>> {
        let world = self.get_entity().world.load_full();
        world.get_nearest_player(pos, f64::INFINITY, |player| {
            targeting.test(&world, Some(self as &dyn EntityBase), player.as_ref())
                && selector(player)
        })
    }

    /// Vanilla `getHeadLookVector(1.0F)`: while landing or taking off the head tilts down by the
    /// distance to the portal, while perched it looks 45 degrees down.
    #[must_use]
    pub fn head_look_vector(&self) -> Vector3<f64> {
        let entity = self.get_entity();
        let phase = self.phase();
        let pitch = if matches!(phase, EnderDragonPhase::Landing | EnderDragonPhase::Takeoff) {
            let egg = self.podium_pos(ChunkHeightmapType::MotionBlockingNoLeaves);
            let dist_sq = egg
                .to_centered_f64()
                .squared_distance_to_vec(&entity.pos.load());
            let dist = (dist_sq.sqrt() as f32 / 4.0).max(1.0);
            let y_offset = 6.0 / dist;
            -y_offset * 1.5 * 5.0
        } else if phase.is_sitting() {
            -45.0
        } else {
            entity.pitch.load()
        };
        Vector3::from_yaw_pitch(entity.yaw.load(), pitch)
    }

    /// Vanilla `findClosestNode()`, which builds the 24 nodes the first time: 12 on an outer ring
    /// around the world origin, 8 higher ones on a middle ring and 4 on an inner ring.
    pub fn find_closest_node(&self) -> usize {
        {
            let mut nodes = self.nodes.lock().unwrap_or_else(PoisonError::into_inner);
            if nodes.is_none() {
                let world = self.get_entity().world.load();
                *nodes = Some(std::array::from_fn(|i| {
                    let (radius, step, index, y_adjustment) = if i < 12 {
                        (60.0, std::f32::consts::PI / 12.0, i, 5)
                    } else if i < 20 {
                        (40.0, std::f32::consts::PI / 8.0, i - 12, 15)
                    } else {
                        (20.0, std::f32::consts::PI / 4.0, i - 20, 5)
                    };
                    let angle = 2.0 * (-std::f32::consts::PI + step * index as f32);
                    let x = (radius * cos(angle)).floor() as i32;
                    let z = (radius * sin(angle)).floor() as i32;
                    let height = world.get_heightmap_height(
                        ChunkHeightmapType::MotionBlockingNoLeaves,
                        x,
                        z,
                    );
                    DragonNode::new(x, 73.max(height + y_adjustment), z)
                }));
            }
        }
        self.find_closest_node_to(self.get_entity().pos.load())
    }

    /// Vanilla `findClosestNode(x, y, z)`: only the inner rings once the crystals are gone.
    pub fn find_closest_node_to(&self, pos: Vector3<f64>) -> usize {
        let start_index = if self
            .fight_alive_crystals()
            .is_none_or(|crystals| crystals == 0)
        {
            12
        } else {
            0
        };
        let current = DragonNode::new(
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        );
        let nodes = self.nodes.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(nodes) = nodes.as_ref() else {
            return 0;
        };
        let mut closest_dist = 10000.0;
        let mut closest_index = 0;
        for (i, node) in nodes.iter().enumerate().skip(start_index) {
            let dist = node.distance_to_sqr(current);
            if dist < closest_dist {
                closest_dist = dist;
                closest_index = i;
            }
        }
        closest_index
    }

    /// Vanilla `findPath`: A* over the node graph from `start` to `end`, then on to `final_node`.
    /// When `end` can't be reached the path stops at the node closest to it.
    pub fn find_path(
        &self,
        start: usize,
        end: usize,
        final_node: Option<DragonNode>,
    ) -> Option<DragonPath> {
        let minimum_index = if self
            .fight_alive_crystals()
            .is_none_or(|crystals| crystals == 0)
        {
            12
        } else {
            0
        };
        let nodes = (*self.nodes.lock().unwrap_or_else(PoisonError::into_inner))?;
        let to = nodes[end];

        let mut g = [0.0f32; NODE_COUNT];
        let mut f = [0.0f32; NODE_COUNT];
        let mut came_from = [None; NODE_COUNT];
        let mut closed = [false; NODE_COUNT];
        let mut open: Vec<usize> = vec![start];
        f[start] = nodes[start].distance_to(to);
        let mut closest = start;

        let reconstruct = |came_from: &[Option<usize>; NODE_COUNT], last: usize| {
            let mut path = vec![nodes[last]];
            let mut index = last;
            while let Some(previous) = came_from[index] {
                path.push(nodes[previous]);
                index = previous;
            }
            path.reverse();
            path.extend(final_node);
            DragonPath {
                nodes: path,
                next: 0,
            }
        };

        while let Some(open_pos) = open
            .iter()
            .enumerate()
            .min_by(|a, b| f[*a.1].total_cmp(&f[*b.1]))
            .map(|(pos, _)| pos)
        {
            let current = open.swap_remove(open_pos);
            if current == end {
                return Some(reconstruct(&came_from, end));
            }
            if nodes[current].distance_to(to) < nodes[closest].distance_to(to) {
                closest = current;
            }
            closed[current] = true;

            for neighbour in minimum_index..NODE_COUNT {
                if NODE_ADJACENCY[current] & (1 << neighbour) == 0 || closed[neighbour] {
                    continue;
                }
                let tentative_g = g[current] + nodes[current].distance_to(nodes[neighbour]);
                let in_open = open.contains(&neighbour);
                if !in_open || tentative_g < g[neighbour] {
                    came_from[neighbour] = Some(current);
                    g[neighbour] = tentative_g;
                    f[neighbour] = tentative_g + nodes[neighbour].distance_to(to);
                    if !in_open {
                        open.push(neighbour);
                    }
                }
            }
        }

        (closest != start).then(|| reconstruct(&came_from, closest))
    }

    /// Vanilla `onCrystalDestroyed`: the current phase reacts to the player who broke it, or to
    /// the nearest player when nobody did.
    pub fn on_crystal_destroyed(
        &self,
        crystal_uuid: uuid::Uuid,
        crystal_pos: Vector3<f64>,
        source: Option<&dyn EntityBase>,
    ) {
        let world = self.get_entity().world.load_full();
        let player = source
            .and_then(EntityBase::get_player)
            .and_then(|player| world.get_player_by_id(player.entity_id()))
            .or_else(|| {
                // Vanilla `CRYSTAL_DESTROY_TARGETING` without a targeter ignores its range.
                world.get_nearest_player(crystal_pos, f64::INFINITY, |player| {
                    TargetPredicate::create_attackable().test(&world, None, player.as_ref())
                })
            });
        let healing = *self
            .nearest_crystal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            == Some(crystal_uuid);
        if healing {
            let damage_type = if player.is_some() {
                DamageType::PLAYER_EXPLOSION
            } else {
                DamageType::EXPLOSION
            };
            self.hurt_part(
                self,
                HEAD_PART,
                10.0,
                damage_type,
                Some(crystal_pos),
                None,
                player.as_deref().map(|player| player as &dyn EntityBase),
            );
        }
        self.phase_manager
            .get(self.phase())
            .on_crystal_destroyed(self, player);
    }

    #[inline]
    #[must_use]
    pub fn yaw_toward(from: Vector3<f64>, to: Vector3<f64>) -> f32 {
        (-(to.z - from.z)).atan2(to.x - from.x).to_degrees() as f32 - 90.0
    }

    pub fn move_relative(&self, speed: f32, relative_movement: Vector3<f64>) {
        let yaw = self.mob_entity.living_entity.entity.yaw.load();
        let movement = Self::get_relative_movement(relative_movement, speed, yaw);
        let entity = &self.mob_entity.living_entity.entity;
        let vel = entity.velocity.load();
        entity.velocity.store(vel + movement);
    }

    fn get_relative_movement(
        relative_movement: Vector3<f64>,
        speed: f32,
        yaw: f32,
    ) -> Vector3<f64> {
        let dist = relative_movement.length_squared();
        if dist < 1.0E-7 {
            return Vector3::new(0.0, 0.0, 0.0);
        }
        let vec = if dist > 1.0 {
            relative_movement.normalize()
        } else {
            relative_movement
        } * (speed as f64);
        let sin = (yaw * (std::f32::consts::PI / 180.0)).sin() as f64;
        let cos = (yaw * (std::f32::consts::PI / 180.0)).cos() as f64;
        Vector3::new(vec.x * cos - vec.z * sin, vec.y, vec.z * cos + vec.x * sin)
    }

    pub fn steer_toward(
        &self,
        pos: Vector3<f64>,
        target: Vector3<f64>,
        fly_speed: f32,
        turn_speed: f32,
    ) {
        let xdd = target.x - pos.x;
        let mut ydd = target.y - pos.y;
        let zdd = target.z - pos.z;
        let dist_sq = xdd * xdd + ydd * ydd + zdd * zdd;

        let horizontal_dist = xdd.hypot(zdd);
        if horizontal_dist > 0.0 {
            ydd = (ydd / horizontal_dist).clamp(-(fly_speed as f64), fly_speed as f64);
        }

        let entity = &self.mob_entity.living_entity.entity;
        let mut vel = entity.velocity.load();
        vel.y += ydd * 0.01;
        entity.velocity.store(vel);

        let yaw = entity.yaw.load();
        let aim_diff = target - pos;
        let aim = if aim_diff.length_squared() > 1e-6 {
            aim_diff.normalize()
        } else {
            Vector3::new(0.0, 0.0, 0.0)
        };

        let dir_vec = Vector3::new(
            (yaw * (std::f32::consts::PI / 180.0)).sin() as f64,
            vel.y,
            -(yaw * (std::f32::consts::PI / 180.0)).cos() as f64,
        );
        let dir = if dir_vec.length_squared() > 1e-6 {
            dir_vec.normalize()
        } else {
            Vector3::new(0.0, 0.0, 0.0)
        };

        let dot = (dir.dot(&aim) as f32 + 0.5).max(0.0) / 1.5;
        if xdd.abs() > 1.0E-5 || zdd.abs() > 1.0E-5 {
            let mut y_rot_d =
                (180.0 - (xdd.atan2(zdd).to_degrees() as f32) - yaw).rem_euclid(360.0);
            if y_rot_d > 180.0 {
                y_rot_d -= 360.0;
            }
            y_rot_d = y_rot_d.clamp(-50.0, 50.0);

            let mut y_rot_a = self
                .yaw_rot_accel
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *y_rot_a *= 0.8;
            *y_rot_a += y_rot_d * turn_speed;
            entity.yaw.store(yaw + *y_rot_a * 0.1);
        }

        let span = (2.0 / (dist_sq + 1.0)) as f32;
        self.move_relative(
            0.06 * (dot * span + (1.0 - span)),
            Vector3::new(0.0, 0.0, -1.0),
        );

        let actual_vel = entity.velocity.load();
        let actual_dir = if actual_vel.length_squared() > 1e-6 {
            actual_vel.normalize()
        } else {
            Vector3::new(0.0, 0.0, 0.0)
        };
        let slide = 0.8 + 0.15 * (actual_dir.dot(&dir) + 1.0) / 2.0;
        entity.velocity.store(Vector3::new(
            actual_vel.x * slide,
            actual_vel.y * 0.91,
            actual_vel.z * slide,
        ));
    }

    fn update_flap_time(&self) {
        let sitting = self.phase().is_sitting();
        let in_wall = *self
            .in_wall
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut flap = self
            .flap_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut o_flap = self
            .o_flap_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        *o_flap = *flap;
        *flap += if sitting {
            0.1
        } else if in_wall {
            FLAP_SPEED * 0.5
        } else {
            FLAP_SPEED
        };
        if *flap > std::f32::consts::TAU {
            *flap -= std::f32::consts::TAU;
        }
    }

    fn tick_growl(&self) {
        if self.phase().is_sitting() {
            return;
        }
        let mut t = self
            .growl_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *t > 0 {
            *t -= 1;
        } else {
            *t = GROWL_COOLDOWN_BASE + rand::random_range(0..100);
        }
    }

    fn handle_player_collisions(&self) {
        let world = self.mob_entity.living_entity.entity.world.load();

        let dragon_bbox = self.mob_entity.living_entity.entity.bounding_box.load();
        let xm = f64::midpoint(dragon_bbox.min.x, dragon_bbox.max.x);
        let zm = f64::midpoint(dragon_bbox.min.z, dragon_bbox.max.z);

        for player in world.players.load().iter() {
            if player
                .living_entity
                .entity
                .bounding_box
                .load()
                .intersects(&dragon_bbox)
            {
                let player_pos = player.get_entity().pos.load();
                let xd = player_pos.x - xm;
                let zd = player_pos.z - zm;
                let dd = (xd * xd + zd * zd).max(0.1);

                player.living_entity.entity.apply_knockback(
                    4.0,
                    xd / dd,
                    zd / dd,
                    Some(&self.mob_entity.living_entity.entity),
                );
                player.get_entity().send_velocity();

                if !self.phase().is_sitting() {
                    player.damage(self, 5.0, DamageType::MOB_ATTACK);
                }
            }
        }
    }

    /// Vanilla `EnderDragon.checkCrystals`: the nearest crystal heals 1 every 10 ticks, and the
    /// dragon looks for it again on a random tenth of its ticks.
    fn tick_crystal_healing(&self) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        if living.is_dead_or_dying() || self.phase() == EnderDragonPhase::Dying {
            return;
        }
        let world = entity.world.load();
        let mut nearest = self
            .nearest_crystal
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(uuid) = *nearest {
            if world
                .get_entity_by_uuid(uuid)
                .is_none_or(|crystal| crystal.get_entity().is_removed())
            {
                *nearest = None;
            } else if entity.age.load(Ordering::Relaxed) % 10 == 0
                && living.health.load() < living.get_max_health()
            {
                living.heal(1.0);
            }
        }

        if rand::random_range(0..10) == 0 {
            let search = entity.bounding_box.load().expand_all(32.0);
            let pos = entity.pos.load();
            *nearest = world
                .entities
                .load()
                .iter()
                .filter(|other| {
                    let other = other.get_entity();
                    other.entity_type == &EntityType::END_CRYSTAL
                        && !other.is_removed()
                        && other.bounding_box.load().intersects(&search)
                })
                .min_by(|a, b| {
                    pos.distance_squared(a.get_entity().pos.load())
                        .total_cmp(&pos.distance_squared(b.get_entity().pos.load()))
                })
                .map(|crystal| crystal.get_entity().entity_uuid);
        }
    }

    fn tick_block_breaking(&self) {
        let phase_type = self.phase();
        if phase_type.is_sitting() || phase_type == EnderDragonPhase::Dying {
            return;
        }

        let world = self.mob_entity.living_entity.entity.world.load();
        let bbox = self.mob_entity.living_entity.entity.bounding_box.load();

        let min = bbox.min_block_pos();
        let max = bbox.max_block_pos();

        for pos in BlockPos::iterate(min, max) {
            let block = world.get_block(&pos);
            if block != &Block::BEDROCK
                && block != &Block::END_STONE
                && block != &Block::OBSIDIAN
                && block != &Block::IRON_BARS
                && block != &Block::END_PORTAL
                && block != &Block::END_PORTAL_FRAME
            {
                world.set_block_state(&pos, BlockStateId::AIR, BlockFlags::NOTIFY_ALL);
            }
        }
    }

    fn tick_parts(&self) {
        let history = self
            .flight_history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let p5 = history.get(5);
        let p10 = history.get(10);
        let p0 = history.get(0);

        let tilt = (p5.y - p10.y) as f32 * 10.0 * (std::f32::consts::PI / 180.0);
        let cc_tilt = tilt.cos() as f64;
        let ss_tilt = tilt.sin() as f64;

        let yaw = self.mob_entity.living_entity.entity.yaw.load();
        let rot1 = yaw * (std::f32::consts::PI / 180.0);
        let ss1 = rot1.sin() as f64;
        let cc1 = rot1.cos() as f64;

        let pos = self.mob_entity.living_entity.entity.pos.load();

        // Body
        self.parts[2]
            .entity
            .set_pos(Vector3::new(pos.x + ss1 * 0.5, pos.y, pos.z - cc1 * 0.5));

        // Wings
        self.parts[6].entity.set_pos(Vector3::new(
            pos.x + cc1 * 4.5,
            pos.y + 2.0,
            pos.z + ss1 * 4.5,
        ));
        self.parts[7].entity.set_pos(Vector3::new(
            pos.x - cc1 * 4.5,
            pos.y + 2.0,
            pos.z - ss1 * 4.5,
        ));

        let head_y_offset = if self.phase().is_sitting() {
            -1.0
        } else {
            p5.y - p0.y
        };

        let yaw_accel = *self
            .yaw_rot_accel
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let rot2 = (yaw - yaw_accel * 0.01) * (std::f32::consts::PI / 180.0);
        let ss2 = rot2.sin() as f64;
        let cc2 = rot2.cos() as f64;

        // Head & Neck
        self.parts[0].entity.set_pos(Vector3::new(
            pos.x + ss2 * 6.5 * cc_tilt,
            pos.y + head_y_offset + ss_tilt * 6.5,
            pos.z - cc2 * 6.5 * cc_tilt,
        ));
        self.parts[1].entity.set_pos(Vector3::new(
            pos.x + ss2 * 5.5 * cc_tilt,
            pos.y + head_y_offset + ss_tilt * 5.5,
            pos.z - cc2 * 5.5 * cc_tilt,
        ));

        // Tails
        for i in 0..3 {
            let pi = history.get(12 + i * 2);
            let rot = yaw * (std::f32::consts::PI / 180.0)
                + (pi.y_rot - p5.y_rot).rem_euclid(360.0).to_radians();
            let ss = rot.sin() as f64;
            let cc = rot.cos() as f64;
            let dd = (i + 1) as f64 * 2.0;

            self.parts[3 + i as usize].entity.set_pos(Vector3::new(
                pos.x - (ss1 * 1.5 + ss * dd) * cc_tilt,
                pos.y + (pi.y - p5.y) - (dd + 1.5) * ss_tilt + 1.5,
                pos.z + (cc1 * 1.5 + cc * dd) * cc_tilt,
            ));
        }
    }

    pub fn ai_step(&self) {
        if self.mob_entity.living_entity.entity.is_removed() {
            return;
        }
        self.mob_entity.living_entity.entity.update_last_pos();
        self.update_flap_time();

        {
            let y = self.mob_entity.living_entity.entity.pos.load().y;
            let yaw = self.mob_entity.living_entity.entity.yaw.load();
            self.flight_history
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(y, yaw);
        };

        self.tick_growl();
        self.tick_crystal_healing();

        {
            let world = self.mob_entity.living_entity.entity.world.load();
            if let Some(ref fight_mutex) = world.dragon_fight {
                let living = &self.mob_entity.living_entity;
                if let Ok(mut fight) = fight_mutex.lock() {
                    let custom_name = living.entity.custom_name.load();
                    fight.update_dragon(
                        &world,
                        living.entity.entity_uuid,
                        living.health.load(),
                        living.get_max_health(),
                        custom_name.as_ref().as_ref(),
                    );
                }
            }
        }

        let mut phase_type = self.phase();
        // PEX's stand-in for vanilla `checkWalls` and wing knockback stays off near the portal.
        let near_portal = phase_type.is_sitting()
            || matches!(
                phase_type,
                EnderDragonPhase::LandingApproach
                    | EnderDragonPhase::Landing
                    | EnderDragonPhase::Takeoff
            );
        if !near_portal {
            self.handle_player_collisions();
            self.tick_block_breaking();
        }

        // Vanilla ticks a phase that the current one switched to in the same tick.
        self.phase_manager.get(phase_type).do_server_tick(self);
        if self.phase() != phase_type {
            phase_type = self.phase();
            self.phase_manager.get(phase_type).do_server_tick(self);
        }

        if self.mob_entity.living_entity.entity.is_removed() {
            return;
        }

        let phase = self.phase_manager.get(phase_type);
        if let Some(target) = phase.get_fly_target_location() {
            let pos = self.mob_entity.living_entity.entity.pos.load();
            self.steer_toward(
                pos,
                target,
                phase.get_fly_speed(),
                phase.get_turn_speed(self),
            );
        }

        self.tick_parts();
    }
}

impl Mob for EnderDragonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.ai_step();
    }

    fn mob_init_data_tracker(&self) {
        self.get_entity().set_synced_data(
            tracked_data::ender_dragon::DATA_PHASE,
            VarInt(self.phase().id()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int(DRAGON_PHASE_KEY, self.phase().id());
        nbt.put_int(
            DRAGON_DEATH_TIME_KEY,
            *self
                .dragon_death_time
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        nbt.put_float(
            SITTING_DAMAGE_RECEIVED_KEY,
            *self
                .sitting_damage_received
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(phase_id) = nbt.get_int(DRAGON_PHASE_KEY) {
            self.set_phase(EnderDragonPhase::by_id(phase_id));
        }
        *self
            .dragon_death_time
            .lock()
            .unwrap_or_else(PoisonError::into_inner) =
            nbt.get_int(DRAGON_DEATH_TIME_KEY).unwrap_or(0);
        *self
            .sitting_damage_received
            .lock()
            .unwrap_or_else(PoisonError::into_inner) =
            nbt.get_float(SITTING_DAMAGE_RECEIVED_KEY).unwrap_or(0.0);
    }

    /// Vanilla `EnderDragon.canAttack`.
    fn can_attack(&self, target: &dyn EntityBase) -> bool {
        can_be_seen_as_enemy(target)
    }

    /// Vanilla `EnderDragon.kill`: no death sequence, the fight counts the dragon as killed.
    fn mob_kill(&self) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        for part in &self.parts {
            part.entity.remove();
        }
        entity.remove();
        if let Some(ref fight_mutex) = world.dragon_fight {
            fight_mutex
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .set_dragon_killed(&world, entity.entity_uuid);
        }
        true
    }

    /// A hit on the dragon itself is a hit on its body (vanilla `EnderDragon.hurtServer`).
    fn modify_incoming_damage(&self, amount: f32, _damage_type: DamageType) -> f32 {
        reduce_body_damage(amount)
    }

    /// The early returns of vanilla `EnderDragon.hurt`: no damage while dying, the sitting
    /// phases (`AbstractDragonSittingPhase.onHurt`) shrug off arrows and wind charges, and only
    /// a player or `always_hurts_ender_dragons` damage hurts it.
    fn pre_damage_with_context(
        &self,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        let by_player = cause
            .or(source)
            .is_some_and(|entity| entity.get_entity().entity_type == &EntityType::PLAYER);
        if !by_player
            && !damage_type.has_tag(&tag::DamageType::MINECRAFT_ALWAYS_HURTS_ENDER_DRAGONS)
        {
            return false;
        }
        self.pre_damage(damage_type, source)
    }

    fn pre_damage(&self, damage_type: DamageType, _source: Option<&dyn EntityBase>) -> bool {
        let phase = self.phase();
        if phase == EnderDragonPhase::Dying {
            return false;
        }
        if phase.is_sitting()
            && [
                DamageType::ARROW,
                DamageType::TRIDENT,
                DamageType::WIND_CHARGE,
            ]
            .contains(&damage_type)
        {
            return false;
        }
        self.health_before_hurt.store(
            self.mob_entity.living_entity.health.load(),
            Ordering::Relaxed,
        );
        true
    }

    fn on_damage(&self, _damage_type: DamageType, _source: Option<&dyn EntityBase>) {
        let living = &self.mob_entity.living_entity;
        if self.phase().is_sitting() {
            let mut received = self
                .sitting_damage_received
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *received += self.health_before_hurt.load(Ordering::Relaxed) - living.health.load();
            if *received > SITTING_ALLOWED_DAMAGE_PERCENTAGE * living.get_max_health() {
                *received = 0.0;
                drop(received);
                self.set_phase(EnderDragonPhase::Takeoff);
            }
        }
        // Vanilla `EnderDragon.handleKillingBlow`: a dragon killed in flight stays at 1 health
        // while it dies.
        if living.health.load() <= 0.0 {
            if !self.phase().is_sitting() {
                living.health.store(1.0);
            }
            self.set_phase(EnderDragonPhase::Dying);
        }
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    fn checks_despawn(&self) -> bool {
        false
    }
}

pub trait Vector3Ext {
    fn distance_squared(self, other: Self) -> f64;
}

impl Vector3Ext for Vector3<f64> {
    fn distance_squared(self, other: Self) -> f64 {
        let (dx, dy, dz) = (self.x - other.x, self.y - other.y, self.z - other.z);
        dx * dx + dy * dy + dz * dz
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_data::damage::DamageType;
    use pumpkin_data::entity::EntityType;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_util::math::vector3::Vector3;

    use super::{EnderDragonEntity, phase::EnderDragonPhase};
    use crate::entity::ai::test_support::world;
    use crate::entity::mob::Mob;
    use crate::entity::{Entity, EntityBase};
    use crate::world::test_player;

    fn dragon(world: &Arc<crate::world::World>, pos: Vector3<f64>) -> Arc<EnderDragonEntity> {
        EnderDragonEntity::new(Entity::new(world.clone(), pos, &EntityType::ENDER_DRAGON))
    }

    #[tokio::test]
    async fn dragon_phase_round_trips_through_nbt() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let saved = dragon(&world, Vector3::new(0.0, 70.0, 0.0));
        saved.set_phase(EnderDragonPhase::SittingScanning);

        let mut nbt = NbtCompound::new();
        saved.write_nbt(&mut nbt);
        // Vanilla `EnderDragonPhase.SITTING_SCANNING.getId()`.
        assert_eq!(nbt.get_int("DragonPhase"), Some(6));

        let loaded = dragon(&world, Vector3::new(0.0, 70.0, 0.0));
        assert_eq!(loaded.phase(), EnderDragonPhase::Hovering);
        loaded.read_nbt_non_mut(&nbt);
        assert_eq!(loaded.phase(), EnderDragonPhase::SittingScanning);

        let mut resaved = NbtCompound::new();
        loaded.write_nbt(&mut resaved);
        assert_eq!(resaved.get_int("DragonPhase"), Some(6));
        world.level.shutdown().await;
    }

    #[tokio::test]
    async fn parts_resolve_by_id_with_their_own_sizes() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let fighting = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        world.spawn_entity_non_save(fighting.clone());
        let id = fighting.get_entity().entity_id;

        // Clients attack a part by its own id, the dragon's id plus 1 to 8.
        for i in 1..=8 {
            let part = world
                .get_entity_or_part(id + i)
                .expect("each part id resolves");
            assert_eq!(part.get_entity().entity_id, id + i);
        }
        let head = fighting.parts[0].get_entity().entity_dimension.load();
        assert!((head.width - 1.0).abs() < f32::EPSILON);
        let body = fighting.parts[2].get_entity().entity_dimension.load();
        assert!((body.width - 5.0).abs() < f32::EPSILON);
        world.level.shutdown().await;
    }

    #[tokio::test]
    async fn projectiles_find_the_parts_not_the_dragon() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let fighting = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        let everywhere = pumpkin_util::math::boundingbox::BoundingBox::new(
            Vector3::new(-64.0, 0.0, -64.0),
            Vector3::new(64.0, 200.0, 64.0),
        );
        let dragon_id = fighting.get_entity().entity_id;
        let found =
            crate::entity::projectile::with_dragon_parts(vec![fighting.clone()], &everywhere, true);
        assert_eq!(found.len(), fighting.parts.len());
        assert!(found.iter().all(|e| e.get_entity().entity_id != dragon_id));
        // The dragon's own fireballs pass through it.
        assert!(
            crate::entity::projectile::with_dragon_parts(vec![fighting], &everywhere, false)
                .is_empty()
        );
        world.level.shutdown().await;
    }

    #[test]
    fn body_parts_take_a_quarter_plus_one() {
        assert!((super::reduce_body_damage(8.0) - 3.0).abs() < f32::EPSILON);
        assert!((super::reduce_body_damage(0.5) - 0.625).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn only_players_and_dragon_damage_types_hurt_it() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let fighting = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        let player = test_player(&world, Vector3::new(0.5, 100.0, 10.5));
        fighting.set_phase(EnderDragonPhase::HoldingPattern);

        assert!(fighting.pre_damage_with_context(
            DamageType::PLAYER_ATTACK,
            Some(player.as_ref()),
            Some(player.as_ref()),
        ));
        assert!(!fighting.pre_damage_with_context(DamageType::MOB_ATTACK, None, None));
        assert!(fighting.pre_damage_with_context(DamageType::EXPLOSION, None, None));
        fighting.set_phase(EnderDragonPhase::Dying);
        assert!(!fighting.pre_damage_with_context(
            DamageType::PLAYER_ATTACK,
            Some(player.as_ref()),
            Some(player.as_ref()),
        ));
        world.level.shutdown().await;
    }

    #[tokio::test]
    async fn killing_blow_in_flight_keeps_one_health_and_dies_over_200_ticks() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let fighting = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        world.spawn_entity_non_save(fighting.clone());
        fighting.set_phase(EnderDragonPhase::HoldingPattern);

        // What the living damage code leaves after a lethal hit.
        fighting.mob_entity.living_entity.health.store(0.0);
        fighting.on_damage(DamageType::PLAYER_ATTACK, None);
        assert_eq!(fighting.phase(), EnderDragonPhase::Dying);
        assert!((fighting.mob_entity.living_entity.health.load() - 1.0).abs() < f32::EPSILON);

        // Vanilla `tickDeath` removes the dragon on its 200th tick.
        for _ in 0..199 {
            fighting
                .phase_manager
                .get(EnderDragonPhase::Dying)
                .do_server_tick(&fighting);
        }
        assert!(!fighting.get_entity().is_removed());
        fighting
            .phase_manager
            .get(EnderDragonPhase::Dying)
            .do_server_tick(&fighting);
        assert!(fighting.get_entity().is_removed());
        world.level.shutdown().await;
    }

    #[tokio::test]
    async fn kill_removes_the_dragon_at_once() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        let killed = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        world.spawn_entity_non_save(killed.clone());
        killed.kill(killed.as_ref());
        assert!(killed.get_entity().is_removed());
        assert!(killed.parts.iter().all(|part| part.entity.is_removed()));
        world.level.shutdown().await;
    }

    #[tokio::test]
    async fn strafe_phase_shoots_dragon_fireball() {
        let folder = tempfile::tempdir().unwrap();
        let world = world(&folder);
        // Yaw 0 faces north (-z), straight at the player 30 blocks away.
        let strafing = dragon(&world, Vector3::new(0.5, 100.0, 0.5));
        let player = test_player(&world, Vector3::new(0.5, 100.0, -29.5));
        strafing.set_phase(EnderDragonPhase::StrafePlayer);
        strafing
            .phase_manager
            .strafe_player
            .set_target(&strafing, &(player as Arc<dyn EntityBase>));

        // Vanilla `FIREBALL_CHARGE_AMOUNT` ticks of charging.
        for _ in 0..5 {
            assert!(
                !world
                    .entities
                    .load()
                    .iter()
                    .any(|e| e.get_entity().entity_type == &EntityType::DRAGON_FIREBALL),
                "the fireball came before the charge was full"
            );
            strafing
                .phase_manager
                .get(strafing.phase())
                .do_server_tick(&strafing);
        }

        let entities = world.entities.load();
        let fireball = entities
            .iter()
            .find(|e| e.get_entity().entity_type == &EntityType::DRAGON_FIREBALL)
            .expect("the strafing dragon shot a dragon fireball");
        assert_eq!(
            fireball.get_owner_id(),
            Some(strafing.get_entity().entity_id)
        );
        let velocity = fireball.get_entity().velocity.load();
        assert!(
            velocity.z < 0.0 && velocity.x.abs() < 1e-6,
            "the fireball flies toward the player, got {velocity:?}"
        );
        assert_eq!(strafing.phase(), EnderDragonPhase::HoldingPattern);
        world.level.shutdown().await;
    }
}
