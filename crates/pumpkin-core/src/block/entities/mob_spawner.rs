use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI32, Ordering},
};

use pumpkin_data::{
    entity::{EntityType, MobCategory},
    world::WorldEvent,
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::{
    Difficulty,
    math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3},
};

use crate::{
    block::entities::{
        BlockEntity,
        trial_spawner::{SpawnData, WeightedSpawnData},
    },
    entity::{EntityBase, mob::spawn::finalize_spawn, r#type::check_spawn_rules_for},
    world::World,
};

pub struct MobSpawnerBlockEntity {
    pub position: BlockPos,
    pub delay: AtomicI32,
    pub max_delay: i32,
    pub min_delay: i32,
    pub spawn_count: i32,
    pub spawn_range: i32,
    pub max_nearby_entities: i32,
    pub required_player_range: i32,
    /// Vanilla `nextSpawnData`, kept whole so its entity NBT, custom spawn rules and equipment
    /// survive a save.
    spawn_data: Mutex<SpawnData>,
    /// Vanilla `spawnPotentials`. Empty when the spawner had none, which vanilla treats as a
    /// list holding `spawn_data` itself.
    spawn_potentials: Mutex<Vec<WeightedSpawnData>>,
}

impl MobSpawnerBlockEntity {
    pub const ID: &'static str = "minecraft:mob_spawner";
    pub const DEFAULT_DELAY: i32 = 20;
    pub const DEFAULT_MAX_SPAWN_DELAY: i32 = 800;
    pub const DEFAULT_MIN_SPAWN_DELAY: i32 = 200;
    pub const DEFAULT_SPAWN_COUNT: i32 = 4;
    pub const DEFAULT_SPAWN_RANGE: i32 = 4;
    pub const DEFAULT_MAX_NEARBY_ENTITIES: i32 = 6;
    pub const DEFAULT_REQUIRED_PLAYER_RANGE: i32 = 16;

    #[must_use]
    pub const fn new(position: BlockPos, entity_type: Option<&'static EntityType>) -> Self {
        Self {
            position,
            delay: AtomicI32::new(Self::DEFAULT_DELAY),
            max_delay: Self::DEFAULT_MAX_SPAWN_DELAY,
            min_delay: Self::DEFAULT_MIN_SPAWN_DELAY,
            spawn_count: Self::DEFAULT_SPAWN_COUNT,
            spawn_range: Self::DEFAULT_SPAWN_RANGE,
            max_nearby_entities: Self::DEFAULT_MAX_NEARBY_ENTITIES,
            required_player_range: Self::DEFAULT_REQUIRED_PLAYER_RANGE,
            spawn_data: Mutex::new(SpawnData {
                entity_type,
                custom_spawn_rules: None,
                equipment: None,
                raw_entity_nbt: None,
            }),
            spawn_potentials: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn entity_type(&self) -> Option<&'static EntityType> {
        self.spawn_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entity_type
    }

    /// Vanilla `BaseSpawner.save`; the client's copy leaves out `SpawnPotentials`.
    pub fn write_spawner_nbt(&self, nbt: &mut NbtCompound, with_potentials: bool) {
        nbt.put_short("Delay", self.delay.load(Ordering::Relaxed) as i16);
        nbt.put_short("MinSpawnDelay", self.min_delay as i16);
        nbt.put_short("MaxSpawnDelay", self.max_delay as i16);
        nbt.put_short("SpawnCount", self.spawn_count as i16);
        nbt.put_short("SpawnRange", self.spawn_range as i16);
        nbt.put_short("MaxNearbyEntities", self.max_nearby_entities as i16);
        nbt.put_short("RequiredPlayerRange", self.required_player_range as i16);

        let spawn_data = self.next_spawn_data();
        let mut spawn_entry = NbtCompound::new();
        spawn_data.write_nbt(&mut spawn_entry);
        if spawn_entry.get_compound("entity").is_none() {
            spawn_entry.put_compound("entity", NbtCompound::new());
        }
        if with_potentials {
            let potentials = self
                .spawn_potentials
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let list = if potentials.is_empty() {
                vec![Self::weighted_entry(spawn_entry.clone(), 1)]
            } else {
                potentials
                    .iter()
                    .map(|potential| {
                        let mut data = NbtCompound::new();
                        potential.data.write_nbt(&mut data);
                        Self::weighted_entry(data, potential.weight)
                    })
                    .collect()
            };
            nbt.put_list("SpawnPotentials", list);
        }
        nbt.put_compound("SpawnData", spawn_entry);
    }

    fn weighted_entry(data: NbtCompound, weight: i32) -> NbtTag {
        let mut entry = NbtCompound::new();
        entry.put_compound("data", data);
        entry.put_int("weight", weight);
        NbtTag::Compound(entry)
    }
}

impl MobSpawnerBlockEntity {
    /// Vanilla `delay`: a new delay, and the next spawn data drawn from the potentials.
    fn update_spawns(&self, world: &Arc<World>) {
        let min_delay = self.min_delay;
        let max_delay = self.max_delay;

        self.delay.store(
            if max_delay <= min_delay {
                min_delay
            } else {
                min_delay + rand::random_range(0..max_delay - min_delay)
            },
            Ordering::Relaxed,
        );
        let next = {
            let potentials = self
                .spawn_potentials
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let total: i32 = potentials.iter().map(|p| p.weight.max(0)).sum();
            if total > 0 {
                let mut roll = rand::random_range(0..total);
                potentials
                    .iter()
                    .find(|potential| {
                        roll -= potential.weight.max(0);
                        roll < 0
                    })
                    .map(|potential| potential.data.clone())
            } else {
                None
            }
        };
        if let Some(next) = next {
            *self
                .spawn_data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
        }
        world.add_synced_block_event(self.position, 1, 0);
    }

    /// Vanilla `setEntityId`: the next spawn data gets the new id and keeps its other NBT.
    pub fn set_entity_type(&self, entity_type: &'static EntityType) {
        self.spawn_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entity_type = Some(entity_type);
    }
}

impl BlockEntity for MobSpawnerBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn tick(&self, world: &Arc<World>) {
        if !self.is_near_player(world) || !world.level_info.load().game_rules.spawner_blocks_work {
            return;
        }
        if self.delay.load(Ordering::Relaxed) == -1 {
            self.update_spawns(world);
        }
        if self.delay.load(Ordering::Relaxed) > 0 {
            self.delay.fetch_sub(1, Ordering::Relaxed);
            return;
        }

        let spawn_data = self.next_spawn_data();
        let mut spawned_any = false;
        for _ in 0..self.spawn_count {
            match self.try_spawn(world, &spawn_data) {
                SpawnAttempt::Spawned => spawned_any = true,
                SpawnAttempt::Skipped => {}
                SpawnAttempt::Stop => {
                    self.update_spawns(world);
                    return;
                }
            }
        }
        if spawned_any {
            self.update_spawns(world);
        }
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let get_num = |name: &str| {
            nbt.get_short(name)
                .map(i32::from)
                .or_else(|| nbt.get_int(name))
                .or_else(|| nbt.get_byte(name).map(i32::from))
        };

        let delay = get_num("Delay").unwrap_or(Self::DEFAULT_DELAY);
        let min_delay = get_num("MinSpawnDelay").unwrap_or(Self::DEFAULT_MIN_SPAWN_DELAY);
        let max_delay = get_num("MaxSpawnDelay").unwrap_or(Self::DEFAULT_MAX_SPAWN_DELAY);
        let spawn_count = get_num("SpawnCount").unwrap_or(Self::DEFAULT_SPAWN_COUNT);
        let spawn_range = get_num("SpawnRange").unwrap_or(Self::DEFAULT_SPAWN_RANGE);
        let max_nearby_entities =
            get_num("MaxNearbyEntities").unwrap_or(Self::DEFAULT_MAX_NEARBY_ENTITIES);
        let required_player_range =
            get_num("RequiredPlayerRange").unwrap_or(Self::DEFAULT_REQUIRED_PLAYER_RANGE);

        let spawn_potentials: Vec<WeightedSpawnData> = nbt
            .get_list("SpawnPotentials")
            .map(|list| {
                list.iter()
                    .filter_map(|tag| {
                        let entry = tag.extract_compound()?;
                        let data = entry.get_compound("data")?;
                        Some(WeightedSpawnData {
                            data: SpawnData::from_nbt(data),
                            weight: entry.get_int("weight").unwrap_or(1),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let spawn_data = nbt
            .get_compound("SpawnData")
            .map(SpawnData::from_nbt)
            .or_else(|| spawn_potentials.first().map(|first| first.data.clone()))
            .unwrap_or_else(|| SpawnData {
                entity_type: nbt.get_string("EntityId").and_then(|id| {
                    EntityType::from_name(id.strip_prefix("minecraft:").unwrap_or(id))
                }),
                custom_spawn_rules: None,
                equipment: None,
                raw_entity_nbt: None,
            });

        Self {
            position,
            delay: AtomicI32::new(delay),
            max_delay,
            min_delay,
            spawn_count,
            spawn_range,
            max_nearby_entities,
            required_player_range,
            spawn_data: Mutex::new(spawn_data),
            spawn_potentials: Mutex::new(spawn_potentials),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_spawner_nbt(nbt, true);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut final_nbt = NbtCompound::new();
        self.write_spawner_nbt(&mut final_nbt, false);
        Some(final_nbt)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// What one of a spawner's `SpawnCount` attempts did (vanilla `BaseSpawner.serverTick`).
enum SpawnAttempt {
    Spawned,
    /// The spot or the rules didn't allow it; the next attempt goes on.
    Skipped,
    /// Vanilla returns from the tick after a new delay: unknown entity, too many nearby, or a
    /// plugin cancelled the spawn.
    Stop,
}

/// Mobs that are vanilla `Monster`s, whose `getWalkTargetValue` keeps spawner mobs out of bright
/// light. Other `MONSTER`-category mobs (slimes, ghasts, phantoms, shulkers, hoglins, the
/// dragon) aren't.
fn is_light_averse_monster(entity_type: &EntityType) -> bool {
    entity_type.category == &MobCategory::MONSTER
        && ![
            EntityType::SLIME.id,
            EntityType::MAGMA_CUBE.id,
            EntityType::GHAST.id,
            EntityType::PHANTOM.id,
            EntityType::SHULKER.id,
            EntityType::HOGLIN.id,
            EntityType::ENDER_DRAGON.id,
        ]
        .contains(&entity_type.id)
}

/// Vanilla `InclusiveRange<Integer>` in NBT: one value, `[min, max]`, or
/// `{min_inclusive, max_inclusive}`. Missing means any light (0 to 15).
fn light_limit(rules: &NbtCompound, name: &str) -> (u8, u8) {
    let clamp = |value: i32| value.clamp(0, 15) as u8;
    match rules.get(name) {
        Some(NbtTag::Int(value)) => (clamp(*value), clamp(*value)),
        Some(NbtTag::IntArray(values)) if values.len() == 2 => (clamp(values[0]), clamp(values[1])),
        Some(NbtTag::List(values)) if values.len() == 2 => match (&values[0], &values[1]) {
            (NbtTag::Int(min), NbtTag::Int(max)) => (clamp(*min), clamp(*max)),
            _ => (0, 15),
        },
        Some(NbtTag::Compound(range)) => (
            clamp(range.get_int("min_inclusive").unwrap_or(0)),
            clamp(range.get_int("max_inclusive").unwrap_or(15)),
        ),
        _ => (0, 15),
    }
}

impl MobSpawnerBlockEntity {
    /// Vanilla `isNearPlayer`: a living player that isn't a spectator within range of the
    /// block's centre.
    fn is_near_player(&self, world: &World) -> bool {
        let center = self.position.to_centered_f64();
        let range_sq = f64::from(self.required_player_range).powi(2);
        world.players.load().iter().any(|player| {
            !player.is_spectator()
                && player.living_entity.health.load() > 0.0
                && player
                    .get_entity()
                    .pos
                    .load()
                    .squared_distance_to_vec(&center)
                    < range_sq
        })
    }

    fn next_spawn_data(&self) -> SpawnData {
        self.spawn_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Vanilla `SpawnData.CustomSpawnRules.isValidPosition`.
    fn custom_rules_allow(rules: &NbtCompound, world: &World, pos: &BlockPos) -> bool {
        let (block_min, block_max) = light_limit(rules, "block_light_limit");
        let (sky_min, sky_max) = light_limit(rules, "sky_light_limit");
        let block = world.get_block_light_level(pos).unwrap_or(0);
        let sky = world.get_sky_light_level(pos);
        (block_min..=block_max).contains(&block) && (sky_min..=sky_max).contains(&sky)
    }

    #[expect(clippy::too_many_lines)]
    fn try_spawn(&self, world: &Arc<World>, spawn_data: &SpawnData) -> SpawnAttempt {
        let Some(entity_type) = spawn_data.entity_type else {
            return SpawnAttempt::Stop;
        };
        let entity_nbt = spawn_data.raw_entity_nbt.as_ref();
        let pos = self.position.0;
        let spawn_range = f64::from(self.spawn_range);
        let spawn_pos = entity_nbt
            .and_then(|nbt| nbt.get_list("Pos"))
            .and_then(|list| match list {
                [NbtTag::Double(x), NbtTag::Double(y), NbtTag::Double(z)] => {
                    Some(Vector3::new(*x, *y, *z))
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                Vector3::new(
                    f64::from(pos.x)
                        + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range
                        + 0.5,
                    f64::from(pos.y + rand::random_range(0..3) - 1),
                    f64::from(pos.z)
                        + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range
                        + 0.5,
                )
            });
        if !world.is_space_empty(entity_type.get_spawn_bounding_box(
            spawn_pos.x,
            spawn_pos.y,
            spawn_pos.z,
        )) {
            return SpawnAttempt::Skipped;
        }

        let spawn_block_pos = BlockPos::floored_v(spawn_pos);
        if let Some(rules) = &spawn_data.custom_spawn_rules {
            if !entity_type.category.is_friendly
                && world.level_info.load().difficulty == Difficulty::Peaceful
            {
                return SpawnAttempt::Skipped;
            }
            if !Self::custom_rules_allow(rules, world, &spawn_block_pos) {
                return SpawnAttempt::Skipped;
            }
        } else if !check_spawn_rules_for(entity_type, world, &spawn_block_pos, false, true) {
            return SpawnAttempt::Skipped;
        }

        let entity =
            crate::entity::r#type::from_type(entity_type, spawn_pos, world, uuid::Uuid::new_v4());
        // Vanilla loads the whole `entity` compound, then puts the mob at the spawn point.
        let only_id = entity_nbt.is_none_or(|nbt| nbt.child_tags.len() <= 1);
        if let Some(nbt) = entity_nbt
            && !only_id
        {
            entity.read_nbt_non_mut(nbt);
            entity.get_entity().set_pos(spawn_pos);
        }

        // Vanilla counts mobs of the same type in the spawner's block grown by `SpawnRange`.
        let search = BoundingBox::new(
            Vector3::new(
                f64::from(pos.x) - spawn_range,
                f64::from(pos.y) - spawn_range,
                f64::from(pos.z) - spawn_range,
            ),
            Vector3::new(
                f64::from(pos.x + 1) + spawn_range,
                f64::from(pos.y + 1) + spawn_range,
                f64::from(pos.z + 1) + spawn_range,
            ),
        );
        let nearby = world
            .entities
            .load()
            .iter()
            .filter(|other| {
                let other = other.get_entity();
                other.entity_type.id == entity_type.id
                    && other.bounding_box.load().intersects(&search)
            })
            .count();
        if nearby >= usize::try_from(self.max_nearby_entities).unwrap_or(0) {
            return SpawnAttempt::Stop;
        }

        entity
            .get_entity()
            .set_rotation(rand::random::<f32>() * 360.0, 0.0);
        if let Some(mob) = entity.get_mob() {
            // Vanilla `Mob.checkSpawnRules`: a `Monster` won't stand in bright light.
            if spawn_data.custom_spawn_rules.is_none()
                && is_light_averse_monster(entity_type)
                && world.get_light_level_dependent_magic_value(&spawn_block_pos) > 0.5
            {
                return SpawnAttempt::Skipped;
            }
            if !mob.check_spawn_obstruction(world) {
                return SpawnAttempt::Skipped;
            }
        }

        let mut event = crate::plugin::api::events::entity::spawner_spawn::SpawnerSpawnEvent::new(
            entity.get_entity().entity_id,
            self.position,
        );
        if let Some(server) = world.server.upgrade() {
            server.plugin_manager.fire_blocking(&server, &mut event);
        }
        if event.cancelled {
            // A cancelled spawn puts the spawner on its cooldown instead of retrying every tick.
            return SpawnAttempt::Stop;
        }

        if only_id {
            finalize_spawn(&entity, world, None);
        }
        world.spawn_entity(entity);
        world.sync_world_event(WorldEvent::ParticlesMobblockSpawn, self.position, 0);
        SpawnAttempt::Spawned
    }
}
#[cfg(test)]
mod tests {
    use super::{MobSpawnerBlockEntity, light_limit};
    use crate::block::entities::BlockEntity;
    use pumpkin_data::entity::EntityType;
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
    use pumpkin_util::math::position::BlockPos;

    fn spawner_nbt() -> NbtCompound {
        let mut entity = NbtCompound::new();
        entity.put_string("id", "minecraft:zombie".to_string());
        entity.put_bool("IsBaby", true);
        let mut rules = NbtCompound::new();
        rules.put("block_light_limit", NbtTag::Int(0));
        let mut sky = NbtCompound::new();
        sky.put_int("min_inclusive", 3);
        sky.put_int("max_inclusive", 9);
        rules.put_compound("sky_light_limit", sky);
        let mut data = NbtCompound::new();
        data.put_compound("entity", entity);
        data.put_compound("custom_spawn_rules", rules);

        let mut skeleton = NbtCompound::new();
        skeleton.put_string("id", "minecraft:skeleton".to_string());
        let mut skeleton_data = NbtCompound::new();
        skeleton_data.put_compound("entity", skeleton);
        let mut entry = NbtCompound::new();
        entry.put_compound("data", skeleton_data);
        entry.put_int("weight", 3);

        let mut nbt = NbtCompound::new();
        nbt.put_compound("SpawnData", data);
        nbt.put_list("SpawnPotentials", vec![NbtTag::Compound(entry)]);
        nbt.put_short("Delay", 7);
        nbt
    }

    #[test]
    fn spawn_data_and_potentials_survive_a_save() {
        let spawner = MobSpawnerBlockEntity::from_nbt(&spawner_nbt(), BlockPos::new(0, 64, 0));
        assert_eq!(spawner.entity_type(), Some(&EntityType::ZOMBIE));

        let mut saved = NbtCompound::new();
        spawner.write_nbt(&mut saved);
        let data = saved.get_compound("SpawnData").expect("spawn data saved");
        let entity = data.get_compound("entity").expect("entity saved");
        assert_eq!(entity.get_bool("IsBaby"), Some(true));
        let rules = data
            .get_compound("custom_spawn_rules")
            .expect("custom spawn rules saved");
        assert_eq!(light_limit(rules, "block_light_limit"), (0, 0));
        assert_eq!(light_limit(rules, "sky_light_limit"), (3, 9));
        assert_eq!(light_limit(rules, "missing"), (0, 15));

        let potentials = saved.get_list("SpawnPotentials").expect("potentials saved");
        assert_eq!(potentials.len(), 1);
        let entry = potentials[0].extract_compound().unwrap();
        assert_eq!(entry.get_int("weight"), Some(3));

        // The client's copy leaves out the potentials, like vanilla's update tag.
        let client = spawner.chunk_data_nbt().unwrap();
        assert!(client.get_list("SpawnPotentials").is_none());
        assert!(client.get_compound("SpawnData").is_some());
    }

    #[test]
    fn a_spawn_egg_keeps_the_rest_of_the_spawn_data() {
        let spawner = MobSpawnerBlockEntity::from_nbt(&spawner_nbt(), BlockPos::new(0, 64, 0));
        spawner.set_entity_type(&EntityType::HUSK);
        let mut saved = NbtCompound::new();
        spawner.write_nbt(&mut saved);
        let entity = saved
            .get_compound("SpawnData")
            .and_then(|data| data.get_compound("entity"))
            .unwrap();
        assert_eq!(entity.get_string("id"), Some("minecraft:husk"));
        assert_eq!(entity.get_bool("IsBaby"), Some(true));
    }

    #[test]
    fn a_spawner_without_potentials_saves_its_spawn_data_as_the_only_one() {
        let spawner = MobSpawnerBlockEntity::new(BlockPos::new(0, 64, 0), Some(&EntityType::PIG));
        let mut saved = NbtCompound::new();
        spawner.write_nbt(&mut saved);
        let potentials = saved.get_list("SpawnPotentials").unwrap();
        assert_eq!(potentials.len(), 1);
        let entry = potentials[0].extract_compound().unwrap();
        assert_eq!(entry.get_int("weight"), Some(1));
        assert_eq!(
            entry
                .get_compound("data")
                .and_then(|data| data.get_compound("entity"))
                .and_then(|entity| entity.get_string("id")),
            Some("minecraft:pig")
        );
    }
}
