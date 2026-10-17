use std::collections::{HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::{
    Block, BlockDirection,
    block_properties::{
        CreakingHeartLikeProperties, CreakingHeartState, GlowLichenLikeProperties,
        WaterLikeProperties,
    },
    damage::DamageType,
    entity::EntityType,
    game_event::GameEvent,
    particle::Particle,
    sound::{Sound, SoundCategory},
    tag::{self, Taggable},
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::java::client::play::TrailParticleOption;
use pumpkin_util::{
    Difficulty,
    math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3},
};
use pumpkin_world::world::BlockFlags;
use rand::{RngExt, seq::SliceRandom};
use uuid::Uuid;

use crate::{
    block::{blocks::creaking_heart::CreakingHeartBlock, entities::BlockEntity},
    entity::{
        Entity, EntityBase,
        mob::creaking::{CREAKING_GRAY, CREAKING_ORANGE, CreakingEntity},
    },
    world::{World, vibration::GameEventContext},
};

const PLAYER_DETECTION_RANGE: f64 = 32.0;
const DISTANCE_CREAKING_TOO_FAR: f64 = 34.0;
const SPAWN_RANGE_XZ: i32 = 16;
const SPAWN_RANGE_Y: i32 = 8;
const ATTEMPTS_PER_SPAWN: usize = 5;
const UPDATE_TICKS: i32 = 20;
const UPDATE_TICKS_VARIANCE: i32 = 5;
const HURT_CALL_TOTAL_TICKS: i32 = 100;
const HURT_CALL_INTERVAL: i32 = 10;
const HURT_CALL_PARTICLE_TICKS: i32 = 50;
const MAX_DEPTH: usize = 2;
const MAX_COUNT: usize = 64;
const TICKS_GRACE_PERIOD: u64 = 30;

pub struct CreakingHeartBlockEntity {
    pub position: BlockPos,
    creaking_uuid: AtomicCell<Option<Uuid>>,
    ticks_existed: AtomicU64,
    ticker: AtomicI32,
    emitter: AtomicI32,
    emitter_target: AtomicCell<Option<Vector3<f64>>>,
    output_signal: AtomicU8,
}

impl BlockEntity for CreakingHeartBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let creaking_uuid = nbt
            .get_uuid("creaking")
            .or_else(|| {
                nbt.get_int_array("creaking").and_then(|arr| {
                    (arr.len() == 4).then(|| {
                        let b0 = arr[0] as u32;
                        let b1 = arr[1] as u32;
                        let b2 = arr[2] as u32;
                        let b3 = arr[3] as u32;
                        let u1 = (u64::from(b0) << 32) | u64::from(b1);
                        let u2 = (u64::from(b2) << 32) | u64::from(b3);
                        Uuid::from_u64_pair(u1, u2)
                    })
                })
            })
            .or_else(|| {
                nbt.get_string("creaking_uuid")
                    .and_then(|uuid_str| Uuid::parse_str(uuid_str).ok())
            });
        Self {
            position,
            creaking_uuid: AtomicCell::new(creaking_uuid),
            ticks_existed: AtomicU64::new(0),
            ticker: AtomicI32::new(0),
            emitter: AtomicI32::new(0),
            emitter_target: AtomicCell::new(None),
            output_signal: AtomicU8::new(0),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(uuid) = self.creaking_uuid.load() {
            nbt.put_uuid("creaking", uuid);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        if let Some(uuid) = self.creaking_uuid.load() {
            nbt.put_uuid("creaking", uuid);
        }
        Some(nbt)
    }

    fn tick(&self, world: &Arc<World>) {
        let (block, state_id) = world.get_block_and_state_id(&self.position);
        if block.id != Block::CREAKING_HEART.id {
            return;
        }
        let mut props = CreakingHeartLikeProperties::from_state_id(state_id);
        if props.creaking_heart_state == CreakingHeartState::Uprooted
            && self.creaking_uuid.load().is_none()
        {
            return;
        }

        self.ticks_existed.fetch_add(1, Ordering::Relaxed);

        let computed_signal = self.compute_analog_output_signal(world);
        if self.output_signal.swap(computed_signal, Ordering::Relaxed) != computed_signal {
            world.update_neighbors(&self.position, None);
        }

        self.tick_emitter(world);

        if self.ticker.fetch_sub(1, Ordering::Relaxed) >= 0 {
            return;
        }
        self.ticker.store(
            rand::rng().random_range(0..UPDATE_TICKS_VARIANCE) + UPDATE_TICKS,
            Ordering::Relaxed,
        );

        let updated_state = self.update_creaking_state(world, props);
        if updated_state != props.creaking_heart_state {
            props.creaking_heart_state = updated_state;
            world.set_block_state(
                &self.position,
                props.to_state_id(&Block::CREAKING_HEART),
                BlockFlags::NOTIFY_ALL,
            );
            if updated_state == CreakingHeartState::Uprooted {
                return;
            }
        }

        if self.creaking_uuid.load().is_some() {
            if let Some(protector) = self.get_creaking_protector(world)
                && let Some(creaking) = protector.cast_any().downcast_ref::<CreakingEntity>()
            {
                let distance = self.distance_to_creaking(creaking);
                let players = world.get_nearby_players(creaking.get_entity().pos.load(), 32.0);
                if (!world.creaking_active(&self.position)
                    && !creaking
                        .mob_entity
                        .persistence_required
                        .load(Ordering::Relaxed))
                    || distance > DISTANCE_CREAKING_TOO_FAR
                    || creaking.player_is_stuck_in_you(&players)
                {
                    self.remove_protector(world, None);
                }
            }
            return;
        }

        if updated_state != CreakingHeartState::Awake {
            return;
        }

        let info = world.level_info.load();
        if info.difficulty == Difficulty::Peaceful || !info.game_rules.spawn_mobs {
            return;
        }

        let heart_origin = Vector3::new(
            f64::from(self.position.0.x),
            f64::from(self.position.0.y),
            f64::from(self.position.0.z),
        );
        let has_player = world
            .get_nearby_players(heart_origin, PLAYER_DETECTION_RANGE)
            .iter()
            .any(|p| p.living_entity.entity.is_alive() && !p.is_spectator() && !p.is_creative());
        if has_player && let Some(creaking) = self.spawn_protector(world) {
            self.set_creaking_uuid(Some(creaking.get_entity().entity_uuid));
            world.play_sound_fine(
                Sound::EntityCreakingSpawn,
                SoundCategory::Hostile,
                &creaking.get_entity().pos.load(),
                1.0,
                1.0,
            );
            world.play_sound(
                Sound::BlockCreakingHeartSpawn,
                SoundCategory::Blocks,
                &self.position.to_f64(),
            );
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl CreakingHeartBlockEntity {
    fn tick_emitter(&self, world: &Arc<World>) {
        let emitter = self.emitter.load(Ordering::Relaxed);
        if emitter <= 0 {
            return;
        }
        if emitter > HURT_CALL_PARTICLE_TICKS {
            self.emit_particles(world, 1, true);
            self.emit_particles(world, 1, false);
        }
        if emitter % HURT_CALL_INTERVAL == 0 {
            if let Some(protector) = self.get_creaking_protector(world) {
                let bbox = protector.get_entity().bounding_box.load();
                self.emitter_target.store(Some((bbox.min + bbox.max) * 0.5));
            }
            if let Some(target) = self.emitter_target.load() {
                let heart_pos = self.position.to_centered_f64();
                let progress = f64::from(0.2 + 0.8 * (100 - emitter) as f32 / 100.0);
                let sound_loc = (heart_pos - target) * progress + target;
                let sound_pos = BlockPos::floored_v(sound_loc).to_f64();
                let volume = emitter as f32 / 2.0 / 100.0 + 0.5;
                world.play_sound_fine(
                    Sound::BlockCreakingHeartHurt,
                    SoundCategory::Blocks,
                    &sound_pos,
                    volume,
                    1.0,
                );
            }
        }
        self.emitter.fetch_sub(1, Ordering::Relaxed);
    }

    pub const ID: &'static str = "minecraft:creaking_heart";

    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            creaking_uuid: AtomicCell::new(None),
            ticks_existed: AtomicU64::new(0),
            ticker: AtomicI32::new(0),
            emitter: AtomicI32::new(0),
            emitter_target: AtomicCell::new(None),
            output_signal: AtomicU8::new(0),
        }
    }

    pub fn is_protector(&self, creaking_uuid: Uuid) -> bool {
        self.creaking_uuid.load() == Some(creaking_uuid)
    }

    pub fn set_creaking_uuid(&self, uuid: Option<Uuid>) {
        self.creaking_uuid.store(uuid);
    }

    pub fn get_analog_output_signal(&self) -> u8 {
        self.output_signal.load(Ordering::Relaxed)
    }

    pub fn compute_analog_output_signal(&self, world: &World) -> u8 {
        let Some(protector) = self.get_creaking_protector(world) else {
            return 0;
        };
        let Some(creaking) = protector.cast_any().downcast_ref::<CreakingEntity>() else {
            return 0;
        };
        let distance = self.distance_to_creaking(creaking);
        let scaled = distance.clamp(0.0, 32.0) / 32.0;
        15 - (scaled * 15.0).floor() as u8
    }

    fn distance_to_creaking(&self, creaking: &CreakingEntity) -> f64 {
        let bottom_center = Vector3::new(
            f64::from(self.position.0.x) + 0.5,
            f64::from(self.position.0.y),
            f64::from(self.position.0.z) + 0.5,
        );
        creaking
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&bottom_center)
            .sqrt()
    }

    fn update_creaking_state(
        &self,
        world: &World,
        props: CreakingHeartLikeProperties,
    ) -> CreakingHeartState {
        if !CreakingHeartBlock::check_active_logs(world, &self.position, props.axis)
            && self.creaking_uuid.load().is_none()
        {
            return CreakingHeartState::Uprooted;
        }
        if world.creaking_active(&self.position) {
            CreakingHeartState::Awake
        } else {
            CreakingHeartState::Dormant
        }
    }

    fn get_creaking_protector(&self, world: &World) -> Option<Arc<dyn EntityBase>> {
        let uuid = self.creaking_uuid.load()?;
        if let Some(entity) = world.get_entity_by_uuid(uuid)
            && !entity.get_entity().is_removed()
            && entity.get_entity().entity_type == &EntityType::CREAKING
        {
            Some(entity)
        } else {
            if self.ticks_existed.load(Ordering::Relaxed) >= TICKS_GRACE_PERIOD {
                self.set_creaking_uuid(None);
            }
            None
        }
    }

    fn spawn_protector(&self, world: &Arc<World>) -> Option<Arc<CreakingEntity>> {
        let mut rng = rand::rng();
        let width = f64::from(EntityType::CREAKING.dimension[0]);
        let height = f64::from(EntityType::CREAKING.dimension[1]);

        for _ in 0..ATTEMPTS_PER_SPAWN {
            let dx = rng.random_range(-SPAWN_RANGE_XZ..=SPAWN_RANGE_XZ);
            let dz = rng.random_range(-SPAWN_RANGE_XZ..=SPAWN_RANGE_XZ);
            let x = self.position.0.x + dx;
            let z = self.position.0.z + dz;

            for dy in (-SPAWN_RANGE_Y..=SPAWN_RANGE_Y).rev() {
                let y = self.position.0.y + dy;
                let ground_pos = BlockPos::new(x, y - 1, z);
                let (ground_block, ground_state) = world.get_block_and_state(&ground_pos);
                if !ground_state.is_solid() || ground_block.has_tag(&tag::Block::MINECRAFT_LEAVES) {
                    continue;
                }

                let spawn_block_pos = BlockPos::new(x, y, z);
                let (_, spawn_state) = world.get_block_and_state(&spawn_block_pos);
                if spawn_state.is_solid() || spawn_state.is_liquid() {
                    continue;
                }

                let spawn_pos = Vector3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
                let half_w = width / 2.0;
                let bbox = BoundingBox::new(
                    Vector3::new(spawn_pos.x - half_w, spawn_pos.y, spawn_pos.z - half_w),
                    Vector3::new(
                        spawn_pos.x + half_w,
                        spawn_pos.y + height,
                        spawn_pos.z + half_w,
                    ),
                );
                if !world.is_space_empty(bbox) {
                    continue;
                }

                let entity = Entity::new(world.clone(), spawn_pos, &EntityType::CREAKING);
                let creaking = CreakingEntity::new(entity);
                creaking.set_transient(self.position);
                let dyn_creaking: Arc<dyn EntityBase> = creaking.clone();
                world.spawn_entity(dyn_creaking.clone());
                world.game_event(
                    GameEvent::EntityPlace,
                    spawn_pos,
                    GameEventContext::of(Some(dyn_creaking), None),
                );
                world.send_entity_status(creaking.get_entity(), EntityStatus::Poof, None);
                return Some(creaking);
            }
        }
        None
    }

    pub fn creaking_hurt(&self, world: &Arc<World>) {
        let Some(protector) = self.get_creaking_protector(world) else {
            return;
        };
        if self.emitter.load(Ordering::Relaxed) > 0 {
            return;
        }
        self.emit_particles(world, 20, false);

        let (block, state_id) = world.get_block_and_state_id(&self.position);
        if block.id == Block::CREAKING_HEART.id {
            let props = CreakingHeartLikeProperties::from_state_id(state_id);
            if props.creaking_heart_state == CreakingHeartState::Awake {
                let clumps = rand::rng().random_range(2..=3);
                for _ in 0..clumps {
                    if let Some(resin_pos) = self.spread_resin(world) {
                        world.play_sound(
                            Sound::BlockResinPlace,
                            SoundCategory::Blocks,
                            &resin_pos.to_f64(),
                        );
                        world.game_event(
                            GameEvent::BlockPlace,
                            resin_pos.to_centered_f64(),
                            GameEventContext::of(None, Some(state_id)),
                        );
                    }
                }
            }
        }

        let bbox = protector.get_entity().bounding_box.load();
        self.emitter_target.store(Some((bbox.min + bbox.max) * 0.5));
        self.emitter.store(HURT_CALL_TOTAL_TICKS, Ordering::Relaxed);
    }

    fn spread_resin(&self, world: &Arc<World>) -> Option<BlockPos> {
        let mut rng = rand::rng();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back((self.position, 0usize));
        visited.insert(self.position);

        let mut count = 0usize;
        while let Some((pos, depth)) = queue.pop_front() {
            if count >= MAX_COUNT {
                break;
            }
            count += 1;

            let pos_block = world.get_block(&pos);
            if pos != self.position && pos_block.has_tag(&tag::Block::MINECRAFT_PALE_OAK_LOGS) {
                let mut dirs = BlockDirection::all();
                dirs.shuffle(&mut rng);
                for dir in dirs {
                    let neighbour_pos = pos.offset(dir.to_offset());
                    let (nb_block, nb_state) = world.get_block_and_state(&neighbour_pos);
                    let mut props = if nb_state.is_air() {
                        GlowLichenLikeProperties::default(&Block::RESIN_CLUMP)
                    } else if nb_block.id == Block::WATER.id
                        && WaterLikeProperties::from_state_id(nb_state.id).level == 0
                    {
                        let mut p = GlowLichenLikeProperties::default(&Block::RESIN_CLUMP);
                        p.waterlogged = true;
                        p
                    } else if nb_block.id == Block::RESIN_CLUMP.id {
                        GlowLichenLikeProperties::from_state_id(nb_state.id)
                    } else {
                        continue;
                    };

                    let opposite = dir.opposite();
                    let has_face = match opposite {
                        BlockDirection::Up => &mut props.up,
                        BlockDirection::Down => &mut props.down,
                        BlockDirection::North => &mut props.north,
                        BlockDirection::South => &mut props.south,
                        BlockDirection::East => &mut props.east,
                        BlockDirection::West => &mut props.west,
                    };
                    if *has_face {
                        continue;
                    }
                    *has_face = true;
                    world.set_block_state(
                        &neighbour_pos,
                        props.to_state_id(&Block::RESIN_CLUMP),
                        BlockFlags::NOTIFY_ALL,
                    );
                    return Some(neighbour_pos);
                }
            }

            if depth < MAX_DEPTH {
                let mut dirs = BlockDirection::all();
                dirs.shuffle(&mut rng);
                for dir in dirs {
                    let next_pos = pos.offset(dir.to_offset());
                    if visited.contains(&next_pos) {
                        continue;
                    }
                    if world
                        .get_block(&next_pos)
                        .has_tag(&tag::Block::MINECRAFT_PALE_OAK_LOGS)
                    {
                        visited.insert(next_pos);
                        queue.push_back((next_pos, depth + 1));
                    }
                }
            }
        }
        None
    }

    fn emit_particles(&self, world: &World, count: usize, towards_creaking: bool) {
        let Some(protector) = self.get_creaking_protector(world) else {
            return;
        };
        let color = if towards_creaking {
            CREAKING_ORANGE
        } else {
            CREAKING_GRAY
        };
        let mut rng = rand::rng();
        let bbox = protector.get_entity().bounding_box.load();
        let size = bbox.max - bbox.min;
        let heart_corner = self.position.to_f64();

        for _ in 0..count {
            let mut source = Vector3::new(
                bbox.min.x + rng.random::<f64>() * size.x,
                bbox.min.y + rng.random::<f64>() * size.y,
                bbox.min.z + rng.random::<f64>() * size.z,
            );
            let mut destination = Vector3::new(
                heart_corner.x + rng.random::<f64>(),
                heart_corner.y + rng.random::<f64>(),
                heart_corner.z + rng.random::<f64>(),
            );
            if towards_creaking {
                std::mem::swap(&mut source, &mut destination);
            }
            let option = TrailParticleOption {
                target: destination,
                color,
                duration: rng.random_range(10..50),
            };
            let mut data = Vec::with_capacity(32);
            if option.write(&mut data).is_ok() {
                world.spawn_particles_with_data(
                    Particle::Trail,
                    source,
                    1,
                    Vector3::new(0.0, 0.0, 0.0),
                    0.0,
                    &data,
                );
            }
        }
    }

    pub fn remove_protector(&self, world: &Arc<World>, damage_type: Option<DamageType>) {
        if let Some(protector) = self.get_creaking_protector(world)
            && let Some(creaking) = protector.cast_any().downcast_ref::<CreakingEntity>()
        {
            if damage_type.is_none() {
                creaking.tear_down();
            } else {
                creaking.creaking_death_effects();
                creaking.set_tearing_down();
                creaking.mob_entity.living_entity.health.store(0.0);
            }
        }
        self.set_creaking_uuid(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{test_player, test_world};
    use pumpkin_data::block_properties::{Axis, PaleOakWoodLikeProperties};
    use pumpkin_util::math::vector2::Vector2;
    use pumpkin_world::chunk::ChunkData;

    #[tokio::test]
    async fn creaking_heart_spawns_protector_at_night_spreads_resin_on_hurt_and_despawns_at_dawn() {
        let world = test_world();
        for cx in -2..=2 {
            for cz in -2..=2 {
                world
                    .level
                    .loaded_chunks
                    .insert(Vector2::new(cx, cz), ChunkData::empty_sync(cx, cz));
            }
        }

        for x in -20..=20 {
            for z in -20..=20 {
                world.set_block_state(
                    &BlockPos::new(x, 63, z),
                    Block::PALE_OAK_PLANKS.default_state.id,
                    BlockFlags::empty(),
                );
            }
        }

        let mut log_props = PaleOakWoodLikeProperties::default(&Block::PALE_OAK_LOG);
        log_props.axis = Axis::Y;
        let log_state = log_props.to_state_id(&Block::PALE_OAK_LOG);
        world.set_block_state(&BlockPos::new(0, 64, 0), log_state, BlockFlags::empty());
        world.set_block_state(&BlockPos::new(0, 66, 0), log_state, BlockFlags::empty());

        let heart_pos = BlockPos::new(0, 65, 0);
        let heart_props = CreakingHeartLikeProperties {
            axis: Axis::Y,
            creaking_heart_state: CreakingHeartState::Dormant,
            natural: true,
        };
        world.set_block_state(
            &heart_pos,
            heart_props.to_state_id(&Block::CREAKING_HEART),
            BlockFlags::empty(),
        );
        let heart_be = Arc::new(CreakingHeartBlockEntity::new(heart_pos));
        world.add_block_entity(heart_be.clone());

        // Place player nearby, looking straight down (+90 pitch) so they aren't staring at the Creaking
        let player = test_player(&world, Vector3::new(4.5, 64.0, 4.5));
        player.living_entity.entity.set_rotation(0.0, 90.0);

        // 1. Daytime: heart stays Dormant and does not spawn a Creaking
        world.set_time_of_day(6000);
        for _ in 0..25 {
            heart_be.tick(&world);
        }
        let state_day =
            CreakingHeartLikeProperties::from_state_id(world.get_block_state(&heart_pos).id);
        assert_eq!(state_day.creaking_heart_state, CreakingHeartState::Dormant);
        assert!(heart_be.creaking_uuid.load().is_none());

        // 2. Nighttime: heart wakes up and spawns its Creaking protector
        world.set_time_of_day(18000);
        for _ in 0..25 {
            heart_be.tick(&world);
        }
        let state_night =
            CreakingHeartLikeProperties::from_state_id(world.get_block_state(&heart_pos).id);
        assert_eq!(state_night.creaking_heart_state, CreakingHeartState::Awake);
        assert!(
            heart_be.creaking_uuid.load().is_some(),
            "creaking heart should spawn a Creaking protector at night"
        );
        assert!(
            heart_be.get_analog_output_signal() > 0,
            "comparator analog output should be positive while protector is nearby"
        );

        // 3. Hurting the Creaking spreads resin clumps onto adjacent pale oak logs
        heart_be.creaking_hurt(&world);
        let mut resin_clumps = 0;
        for y in 64..=66 {
            for dir in BlockDirection::all() {
                let check = BlockPos::new(0, y, 0).offset(dir.to_offset());
                if world.get_block(&check).id == Block::RESIN_CLUMP.id {
                    resin_clumps += 1;
                }
            }
        }
        assert!(
            resin_clumps > 0,
            "creaking_hurt should spread resin clumps onto neighbouring pale oak logs"
        );

        // 4. Dawn: heart goes Dormant and despawns the Creaking when no player is looking
        world.set_time_of_day(6000);
        for _ in 0..25 {
            heart_be.tick(&world);
        }
        let state_dawn =
            CreakingHeartLikeProperties::from_state_id(world.get_block_state(&heart_pos).id);
        assert_eq!(state_dawn.creaking_heart_state, CreakingHeartState::Dormant);
        assert!(
            heart_be.creaking_uuid.load().is_none(),
            "creaking protector should despawn at dawn"
        );
    }
}
