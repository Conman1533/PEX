use pumpkin_data::{game_event::GameEvent, tag};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::{AllayEntity, ai};
use crate::entity::{Entity, EntityBase};
use crate::world::{
    World,
    game_event::{GameEventContext, GameEventListener},
    vibration::{self, VibrationData, VibrationInfo, VibrationUser},
};

const VIBRATION_EVENT_LISTENER_RANGE: i32 = 16;

pub(super) struct JukeboxListener<'a>(pub(super) &'a AllayEntity);

impl GameEventListener for JukeboxListener<'_> {
    fn listener_position(&self) -> Vector3<f64> {
        self.0.get_eye_pos()
    }

    fn listener_radius(&self) -> i32 {
        GameEvent::JukeboxPlay.notification_radius()
    }

    fn handle_game_event(
        &self,
        _world: &World,
        event: GameEvent,
        _context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool {
        let playing = match event {
            GameEvent::JukeboxPlay => true,
            GameEvent::JukeboxStopPlay => false,
            _ => return false,
        };
        self.0
            .set_jukebox_playing(BlockPos::floored_v(source_position), playing);
        true
    }
}

impl VibrationUser for AllayEntity {
    fn vibration_data(&self) -> &std::sync::Mutex<VibrationData> {
        &self.vibration
    }

    fn listener_entity(&self) -> &Entity {
        self.get_entity()
    }

    fn listener_radius(&self) -> i32 {
        VIBRATION_EVENT_LISTENER_RANGE
    }

    fn listenable_events(&self) -> &'static tag::Tag {
        &tag::GameEvent::MINECRAFT_ALLAY_CAN_LISTEN
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        _event: GameEvent,
        _context: &GameEventContext<'_>,
    ) -> bool {
        if self.mob_entity.is_no_ai() {
            return false;
        }
        let entity = self.get_entity();
        self.mob_entity.with_brain(self, |brain| {
            brain
                .get(crate::entity::ai::brain::memory::types::LIKED_NOTEBLOCK_POSITION)
                .is_none_or(|liked| {
                    liked.is_close_enough(
                        &world.dimension,
                        entity.block_pos.load(),
                        ai::MAX_NOTEBLOCK_DISTANCE,
                    ) && liked.pos == pos
                })
        })
    }

    fn on_receive_vibration(
        &self,
        _world: &World,
        pos: BlockPos,
        vibration: &VibrationInfo,
        _distance: f32,
    ) {
        if vibration.event == GameEvent::NoteBlockPlay {
            ai::hear_noteblock(self, pos);
        }
    }
}

impl AllayEntity {
    pub(super) fn tick_vibration(&self) {
        vibration::tick(self, &self.get_entity().world.load());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::{brain::memory::types, test_support::world};
    use crate::world::vibration::VibrationListener;
    use pumpkin_data::entity::EntityType;

    #[tokio::test]
    async fn wool_occludes_notes_but_a_solid_stone_block_does_not()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        world.level.loaded_chunks.insert(
            pumpkin_util::math::vector2::Vector2::new(0, 0),
            pumpkin_world::chunk::ChunkData::empty_sync(0, 0),
        );
        let allay = AllayEntity::new(Entity::new(
            world.clone(),
            Vector3::new(6.5, 64.0, 2.5),
            &EntityType::ALLAY,
        ));
        let blocker = BlockPos::new(4, 64, 2);
        let origin = BlockPos::new(2, 64, 2).to_centered_f64();
        world.set_block_state(
            &blocker,
            pumpkin_data::Block::STONE.default_state.id,
            pumpkin_world::world::BlockFlags::empty(),
        );
        VibrationListener(allay.as_ref()).handle_game_event(
            &world,
            GameEvent::NoteBlockPlay,
            &GameEventContext {
                source_entity: None,
            },
            origin,
        );
        {
            let mut data = allay
                .vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert!(data.selector.has_candidate());
            data.selector.start_over();
        }
        world.set_block_state(
            &blocker,
            pumpkin_data::Block::WHITE_WOOL.default_state.id,
            pumpkin_world::world::BlockFlags::empty(),
        );
        VibrationListener(allay.as_ref()).handle_game_event(
            &world,
            GameEvent::NoteBlockPlay,
            &GameEventContext {
                source_entity: None,
            },
            origin,
        );
        assert!(
            !allay
                .vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .selector
                .has_candidate()
        );
        world.level.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn note_delivery_waits_for_travel_and_survives_reload()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let allay = AllayEntity::new(Entity::new(
            world.clone(),
            Vector3::new(3.5, 64.0, 0.5),
            &EntityType::ALLAY,
        ));
        let origin = BlockPos::new(0, 64, 0);
        VibrationListener(allay.as_ref()).handle_game_event(
            &world,
            GameEvent::NoteBlockPlay,
            &GameEventContext {
                source_entity: None,
            },
            origin.to_centered_f64(),
        );
        allay.tick_vibration();
        assert!(
            allay
                .vibration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current
                .is_none()
        );
        world
            .level_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .world_age = 1;
        allay.tick_vibration();
        let nbt = allay
            .vibration
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .write_nbt();
        assert_eq!(nbt.get_int("event_delay"), Some(2));
        assert!(!allay.mob_entity.with_brain(allay.as_ref(), |brain| {
            brain.has_memory_value(types::LIKED_NOTEBLOCK_POSITION.id())
        }));
        *allay
            .vibration
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = VibrationData::read_nbt(&nbt);
        allay.tick_vibration();
        assert!(!allay.mob_entity.with_brain(allay.as_ref(), |brain| {
            brain.has_memory_value(types::LIKED_NOTEBLOCK_POSITION.id())
        }));
        allay.tick_vibration();
        assert_eq!(
            allay.mob_entity.with_brain(allay.as_ref(), |brain| brain
                .get(types::LIKED_NOTEBLOCK_POSITION)
                .map(|liked| liked.pos)),
            Some(origin)
        );
        world.level.shutdown().await;
        Ok(())
    }
}
