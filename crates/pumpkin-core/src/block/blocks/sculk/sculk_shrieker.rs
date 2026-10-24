use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::block::entities::sculk_shrieker::SculkShriekerBlockEntity;
use crate::block::{
    BlockBehaviour, BlockMetadata, OnEntityStepArgs, OnPlaceArgs, OnScheduledTickArgs,
};
use crate::entity::EntityBase;
use crate::entity::mob::warden::WardenEntity;
use crate::entity::mob::warden_spawn_tracker::{MAX_WARNING_LEVEL, WardenSpawnTracker};
use crate::entity::player::Player;
use crate::entity::spawn_util::{SpawnStrategy, try_spawn_mob};
use crate::world::World;
use pumpkin_data::{
    BlockId, BlockStateId,
    block_properties::SculkShriekerLikeProperties,
    entity::EntityType,
    game_event::GameEvent,
    sound::{Sound, SoundCategory},
    world::WorldEvent,
};
use pumpkin_util::Difficulty;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

const WARNING_SOUND_RADIUS: i32 = 10;
const WARDEN_SPAWN_ATTEMPTS: i32 = 20;
const WARDEN_SPAWN_RANGE_XZ: i32 = 5;
const WARDEN_SPAWN_RANGE_Y: i32 = 6;
const DARKNESS_RADIUS: f64 = 40.0;
const SHRIEKING_TICKS: u8 = 90;

pub struct SculkShriekerBlock;

impl BlockMetadata for SculkShriekerBlock {
    fn ids() -> Box<[BlockId]> {
        [BlockId::SCULK_SHRIEKER].into()
    }
}

impl SculkShriekerBlock {
    /// `SculkShriekerBlockEntity.tryGetPlayer`: the player, their rider, a projectile's owner, or
    /// a dropped item's thrower.
    pub fn try_get_player(world: &World, source: &dyn EntityBase) -> Option<Arc<Player>> {
        let entity = source.get_entity();
        if let Some(player) = world.get_player_by_id(entity.entity_id) {
            return Some(player);
        }
        {
            let passengers = entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(rider) = passengers.first()
                && let Some(player) = world.get_player_by_id(rider.get_entity().entity_id)
            {
                return Some(player);
            }
        }
        if let Some(owner_id) = source.get_owner_id()
            && let Some(player) = world.get_player_by_id(owner_id)
        {
            return Some(player);
        }
        if let Some(item) = source.get_item_entity()
            && let Some(thrower) = item.get_thrower()
        {
            return world.get_player_by_uuid(thrower);
        }
        None
    }

    fn with_shrieker<R>(
        world: &World,
        pos: &BlockPos,
        f: impl FnOnce(&SculkShriekerBlockEntity) -> R,
    ) -> Option<R> {
        let entity = world.get_block_entity(pos)?;
        entity
            .as_any()
            .downcast_ref::<SculkShriekerBlockEntity>()
            .map(f)
    }

    fn can_respond(world: &World, can_summon: bool) -> bool {
        let level_info = world.level_info.load();
        can_summon
            && level_info.difficulty != Difficulty::Peaceful
            && level_info.game_rules.spawn_wardens
    }

    pub fn try_shriek(world: &Arc<World>, pos: &BlockPos, player: &Arc<Player>) {
        let block = world.get_block(pos);
        if block.id != BlockId::SCULK_SHRIEKER {
            return;
        }
        let state = world.get_block_state(pos);
        let mut props = SculkShriekerLikeProperties::from_state_id(state.id);
        if props.shrieking {
            return;
        }

        let can_respond = Self::can_respond(world, props.can_summon);
        let warning_level = if can_respond {
            WardenSpawnTracker::try_warn(world, pos, player)
        } else {
            Some(0)
        };
        Self::with_shrieker(world, pos, |shrieker| {
            *shrieker
                .warning_level
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = warning_level.unwrap_or(0);
            shrieker.shrieking_can_summon.store(
                warning_level.is_some() && props.can_summon,
                Ordering::Relaxed,
            );
        });
        if warning_level.is_none() {
            return;
        }

        props.shrieking = true;
        world.set_block_state(pos, props.to_state_id(block), BlockFlags::NOTIFY_LISTENERS);
        world.schedule_block_tick(block, *pos, SHRIEKING_TICKS, TickPriority::Normal);
        world.sync_world_event(WorldEvent::ParticlesSculkShriek, *pos, 0);
        world.game_event(
            GameEvent::Shriek,
            pos.to_centered_f64(),
            crate::world::vibration::GameEventContext::of(Some(player.clone()), None),
        );
    }

    fn try_respond(world: &Arc<World>, pos: &BlockPos, can_summon: bool) {
        let warning_level = Self::with_shrieker(world, pos, |shrieker| {
            shrieker
                .shrieking_can_summon
                .store(false, Ordering::Relaxed);
            *shrieker
                .warning_level
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        });
        if let Some(warning_level) = warning_level
            && can_summon
        {
            Self::respond(world, pos, warning_level);
        }
    }

    pub fn respond(world: &Arc<World>, pos: &BlockPos, warning_level: i32) {
        if !Self::can_respond(world, true) || warning_level <= 0 {
            return;
        }

        if !Self::try_summon_warden(world, pos, warning_level) {
            Self::play_warden_reply_sound(world, pos, warning_level);
        }
        WardenEntity::apply_darkness_around(world, pos.to_centered_f64(), DARKNESS_RADIUS);
    }

    fn try_summon_warden(world: &Arc<World>, pos: &BlockPos, warning_level: i32) -> bool {
        if warning_level < MAX_WARNING_LEVEL {
            return false;
        }
        let Some(warden) = try_spawn_mob(
            &EntityType::WARDEN,
            WardenEntity::new,
            world,
            pos,
            WARDEN_SPAWN_ATTEMPTS,
            WARDEN_SPAWN_RANGE_XZ,
            WARDEN_SPAWN_RANGE_Y,
            SpawnStrategy::OnTopOfCollider,
            false,
        ) else {
            return false;
        };
        warden.emerge();
        true
    }

    fn play_warden_reply_sound(world: &World, pos: &BlockPos, warning_level: i32) {
        let sound = match warning_level {
            1 => Sound::EntityWardenNearbyClose,
            2 => Sound::EntityWardenNearbyCloser,
            3 => Sound::EntityWardenNearbyClosest,
            4 => Sound::EntityWardenListeningAngry,
            _ => return,
        };
        let radius = WARNING_SOUND_RADIUS;
        let sound_pos = {
            let mut random = rand::rng();
            pos.add(
                random.random_range(-radius..=radius),
                random.random_range(-radius..=radius),
                random.random_range(-radius..=radius),
            )
        };
        world.play_sound_fine(sound, SoundCategory::Hostile, &sound_pos.to_f64(), 5.0, 1.0);
    }
}

impl BlockBehaviour for SculkShriekerBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = SculkShriekerLikeProperties::default(args.block);
        props.shrieking = false;
        props.waterlogged = args.replacing.water_source();
        props.to_state_id(args.block)
    }

    fn on_entity_step(&self, args: OnEntityStepArgs<'_>) {
        if let Some(player) = Self::try_get_player(args.world, args.entity) {
            Self::try_shriek(args.world, args.position, &player);
        }
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        let mut props = SculkShriekerLikeProperties::from_state_id(state.id);
        if props.shrieking {
            props.shrieking = false;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL,
            );
            Self::try_respond(args.world, args.position, props.can_summon);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entities::BlockEntity;
    use crate::block::entities::sculk_sensor::SculkSensorBlockEntity;
    use crate::entity::Entity;
    use crate::entity::item::ItemEntity;
    use crate::entity::passive::allay::AllayEntity;
    use crate::entity::projectile::arrow::ArrowEntity;
    use crate::world::{test_player, test_world, vibration};
    use pumpkin_data::{Block, item::Item, item_stack::ItemStack};
    use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};
    use pumpkin_world::chunk::ChunkData;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn ancient_city_sensor_and_shrieker_four_warnings_summon_warden_and_entities_listen() {
        let world = test_world();
        for cx in -2..=2 {
            for cz in -2..=2 {
                world
                    .level
                    .loaded_chunks
                    .insert(Vector2::new(cx, cz), ChunkData::empty_sync(cx, cz));
            }
        }

        for x in -8..=8 {
            for z in -8..=8 {
                world.set_block_state(
                    &BlockPos::new(x, 63, z),
                    Block::DEEPSLATE_BRICKS.default_state.id,
                    BlockFlags::empty(),
                );
            }
        }

        let sensor_pos = BlockPos::new(0, 64, 0);
        world.set_block_state(
            &sensor_pos,
            Block::SCULK_SENSOR.default_state.id,
            BlockFlags::empty(),
        );
        let sensor_be = Arc::new(SculkSensorBlockEntity::new(sensor_pos));
        world.add_block_entity(sensor_be.clone());

        let shrieker_pos = BlockPos::new(2, 64, 0);
        let mut shrieker_props = SculkShriekerLikeProperties::default(&Block::SCULK_SHRIEKER);
        shrieker_props.can_summon = true;
        world.set_block_state(
            &shrieker_pos,
            shrieker_props.to_state_id(&Block::SCULK_SHRIEKER),
            BlockFlags::empty(),
        );
        let shrieker_be = Arc::new(SculkShriekerBlockEntity::new(shrieker_pos));
        world.add_block_entity(shrieker_be.clone());

        let player = test_player(&world, Vector3::new(1.5, 64.0, 1.5));

        // 1. Wool item entity dampens vibrations
        let wool_item = Arc::new(ItemEntity::new(
            Entity::new(
                world.clone(),
                Vector3::new(0.5, 64.0, 1.5),
                &EntityType::ITEM,
            ),
            ItemStack::new(1, &Item::WHITE_WOOL),
        ));
        wool_item.set_thrower(player.gameprofile.id);
        world.game_event(
            GameEvent::HitGround,
            Vector3::new(0.5, 64.0, 1.5),
            vibration::GameEventContext::of(Some(wool_item as Arc<dyn EntityBase>), None),
        );
        for _ in 0..5 {
            world.level_time.lock().unwrap().world_age += 1;
            sensor_be.tick(&world);
        }
        let sh_state = world.get_block_state(&shrieker_pos);
        assert!(
            !SculkShriekerLikeProperties::from_state_id(sh_state.id).shrieking,
            "wool item landing must not trigger sensor or shrieker"
        );

        // 2. Four warnings from player vibrations (step, projectile, dropped item, container)
        let cobble_item = Arc::new(ItemEntity::new(
            Entity::new(
                world.clone(),
                Vector3::new(0.5, 64.0, 1.5),
                &EntityType::ITEM,
            ),
            ItemStack::new(1, &Item::COBBLESTONE),
        ));
        cobble_item.set_thrower(player.gameprofile.id);
        world.spawn_entity(cobble_item.clone() as Arc<dyn EntityBase>);

        let arrow = Arc::new(ArrowEntity::new(
            Entity::new(
                world.clone(),
                Vector3::new(0.5, 64.0, 1.5),
                &EntityType::ARROW,
            ),
            Some(player.entity_id()),
        ));
        world.spawn_entity(arrow.clone() as Arc<dyn EntityBase>);

        let events: [(GameEvent, Arc<dyn EntityBase>); 4] = [
            (GameEvent::Step, player.clone() as Arc<dyn EntityBase>),
            (GameEvent::ProjectileLand, arrow as Arc<dyn EntityBase>),
            (GameEvent::HitGround, cobble_item as Arc<dyn EntityBase>),
            (
                GameEvent::ContainerOpen,
                player.clone() as Arc<dyn EntityBase>,
            ),
        ];

        for (idx, (event, source)) in events.into_iter().enumerate() {
            let expected_warning = (idx + 1) as i32;
            // Reset sensor phase to Inactive so it can immediately trigger for the next test step
            world.set_block_state(
                &sensor_pos,
                Block::SCULK_SENSOR.default_state.id,
                BlockFlags::empty(),
            );
            world.game_event(
                event,
                Vector3::new(0.5, 64.0, 1.5),
                vibration::GameEventContext::of(Some(source), None),
            );
            for _ in 0..10 {
                world.level_time.lock().unwrap().world_age += 1;
                sensor_be.tick(&world);
                shrieker_be.tick(&world);
            }
            let sh_props =
                SculkShriekerLikeProperties::from_state_id(world.get_block_state(&shrieker_pos).id);
            assert!(
                sh_props.shrieking,
                "shrieker should be shrieking on warning {expected_warning}"
            );
            assert_eq!(
                *shrieker_be.warning_level.lock().unwrap(),
                expected_warning,
                "warning level mismatch on warning {expected_warning}"
            );

            SculkShriekerBlock.on_scheduled_tick(OnScheduledTickArgs {
                world: &world,
                block: &Block::SCULK_SHRIEKER,
                position: &shrieker_pos,
            });
            for _ in 0..205 {
                player.warden_spawn_tracker.lock().unwrap().tick();
            }

            let warden_count = world
                .entities
                .load()
                .iter()
                .filter(|e| e.get_entity().entity_type == &EntityType::WARDEN)
                .count();
            if expected_warning < 4 {
                assert_eq!(warden_count, 0, "warden must not spawn before 4th warning");
            } else {
                assert_eq!(warden_count, 1, "warden must spawn on 4th warning");
            }
        }

        // 3. Verify the spawned Warden listens to vibrations and increases anger toward the player
        let warden_entity = world
            .entities
            .load()
            .iter()
            .find(|e| e.get_entity().entity_type == &EntityType::WARDEN)
            .cloned()
            .expect("spawned warden");
        let warden = warden_entity
            .cast_any()
            .downcast_ref::<WardenEntity>()
            .expect("downcast WardenEntity");
        warden
            .get_entity()
            .set_pose(pumpkin_data::entity::EntityPose::Standing);
        warden.vibration_cooldown.store(0, Ordering::Relaxed);
        world.game_event(
            GameEvent::Step,
            player.living_entity.entity.pos.load(),
            vibration::GameEventContext::of(Some(player.clone() as Arc<dyn EntityBase>), None),
        );
        for _ in 0..10 {
            world.level_time.lock().unwrap().world_age += 1;
            if let Some(user) = warden_entity.as_vibration_user() {
                vibration::tick(user, &world);
            }
        }
        let anger = warden
            .anger
            .lock()
            .unwrap()
            .active_anger(Some(player.living_entity.entity.entity_uuid));
        assert!(
            anger >= 35,
            "warden should gain anger toward the player after receiving their vibration, got {anger}"
        );

        // 4. Verify the allay's game event listener hears jukeboxes through `World::game_event`
        let allay = AllayEntity::new(Entity::new(
            world.clone(),
            Vector3::new(-2.5, 64.0, 0.5),
            &EntityType::ALLAY,
        ));
        world.spawn_entity(allay.clone() as Arc<dyn EntityBase>);

        let jukebox_pos = BlockPos::new(-2, 64, 1);
        world.set_block_state(
            &jukebox_pos,
            Block::JUKEBOX.default_state.id,
            BlockFlags::empty(),
        );
        world.game_event(
            GameEvent::JukeboxPlay,
            jukebox_pos.to_centered_f64(),
            vibration::GameEventContext::of(None, None),
        );
        assert!(allay.is_dancing(), "allay should dance on JukeboxPlay");
        world.game_event(
            GameEvent::JukeboxStopPlay,
            jukebox_pos.to_centered_f64(),
            vibration::GameEventContext::of(None, None),
        );
        assert!(
            !allay.is_dancing(),
            "allay should stop dancing on JukeboxStopPlay"
        );
    }
}
