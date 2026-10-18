use std::sync::Arc;

use crate::block::blocks::redstone::get_redstone_power;
use crate::block::entities::calibrated_sculk_sensor::CalibratedSculkSensorBlockEntity;
use crate::block::entities::sculk_sensor::SculkSensorBlockEntity;
use crate::block::{
    BlockBehaviour, BlockMetadata, EmitsRedstonePowerArgs, GetComparatorOutputArgs,
    GetRedstonePowerArgs, OnEntityStepArgs, OnPlaceArgs, OnScheduledTickArgs, PathComputationType,
    PlacedArgs,
};
use crate::entity::EntityBase;
use crate::world::World;
use crate::world::vibration::{self, GameEventContext};
use pumpkin_data::block_properties::{
    CalibratedSculkSensorLikeProperties, HorizontalFacing, SculkSensorLikeProperties,
    SculkSensorPhase,
};
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockId, BlockState, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;

pub struct SculkSensorBlock;

impl BlockMetadata for SculkSensorBlock {
    fn ids() -> Box<[BlockId]> {
        [BlockId::SCULK_SENSOR, BlockId::CALIBRATED_SCULK_SENSOR].into()
    }
}

const fn horizontal_facing_to_dir(facing: HorizontalFacing) -> BlockDirection {
    match facing {
        HorizontalFacing::North => BlockDirection::North,
        HorizontalFacing::South => BlockDirection::South,
        HorizontalFacing::West => BlockDirection::West,
        HorizontalFacing::East => BlockDirection::East,
    }
}

/// Both sensor variants carry the same phase property under different types.
fn sculk_sensor_phase(block: &Block, state_id: BlockStateId) -> SculkSensorPhase {
    if block.id == BlockId::CALIBRATED_SCULK_SENSOR {
        CalibratedSculkSensorLikeProperties::from_state_id(state_id).sculk_sensor_phase
    } else {
        SculkSensorLikeProperties::from_state_id(state_id).sculk_sensor_phase
    }
}

impl SculkSensorBlock {
    /// `SculkSensorBlock.getActiveTicks`: 30 for a sculk sensor, 10 for a calibrated one.
    fn active_ticks(block: &Block) -> u8 {
        if block.id == BlockId::CALIBRATED_SCULK_SENSOR {
            10
        } else {
            30
        }
    }

    /// `SculkSensorBlock.canActivate`.
    fn can_activate(block: &Block, state_id: BlockStateId) -> bool {
        sculk_sensor_phase(block, state_id) == SculkSensorPhase::Inactive
    }

    /// `SculkSensorBlockEntity.VibrationUser.canReceiveVibration`.
    pub fn can_receive_vibration(
        world: &World,
        pos: &BlockPos,
        origin: &BlockPos,
        event: GameEvent,
    ) -> bool {
        if origin == pos && matches!(event, GameEvent::BlockDestroy | GameEvent::BlockPlace) {
            return false;
        }
        if vibration::event_frequency(event) == 0 {
            return false;
        }
        let (block, state) = world.get_block_and_state(pos);
        (block.id == BlockId::SCULK_SENSOR || block.id == BlockId::CALIBRATED_SCULK_SENSOR)
            && Self::can_activate(block, state.id)
    }

    /// `CalibratedSculkSensorBlockEntity.VibrationUser`: a signal into the back of the sensor
    /// limits it to vibrations of that frequency.
    pub fn calibrated_frequency_matches(world: &World, pos: &BlockPos, event: GameEvent) -> bool {
        let state = world.get_block_state(pos);
        if Block::from_state_id(state.id).id != BlockId::CALIBRATED_SCULK_SENSOR {
            return false;
        }
        let props = CalibratedSculkSensorLikeProperties::from_state_id(state.id);
        let direction = horizontal_facing_to_dir(props.facing).opposite();
        let back_pos = pos.offset(direction.to_offset());
        let (back_block, back_state) = world.get_block_and_state(&back_pos);
        let signal = get_redstone_power(back_block, back_state, world, &back_pos, direction);
        signal == 0 || i32::from(signal) == vibration::event_frequency(event)
    }

    /// `SculkSensorBlockEntity.VibrationUser.onReceiveVibration`.
    pub fn receive_vibration(
        world: &Arc<World>,
        pos: &BlockPos,
        last_frequency: &std::sync::Mutex<i32>,
        event: GameEvent,
        source: Option<&Arc<dyn EntityBase>>,
        power: u8,
    ) {
        let (block, state) = world.get_block_and_state(pos);
        if !Self::can_activate(block, state.id) {
            return;
        }
        let frequency = vibration::event_frequency(event);
        *last_frequency
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = frequency;
        Self::activate(world, pos, block, state.id, source, power, frequency);
    }

    /// `SculkSensorBlock.activate`.
    fn activate(
        world: &Arc<World>,
        pos: &BlockPos,
        block: &Block,
        state_id: BlockStateId,
        source: Option<&Arc<dyn EntityBase>>,
        power: u8,
        frequency: i32,
    ) {
        let (new_state, waterlogged) = if block.id == BlockId::CALIBRATED_SCULK_SENSOR {
            let mut props = CalibratedSculkSensorLikeProperties::from_state_id(state_id);
            props.sculk_sensor_phase = SculkSensorPhase::Active;
            props.power = power;
            (props.to_state_id(block), props.waterlogged)
        } else {
            let mut props = SculkSensorLikeProperties::from_state_id(state_id);
            props.sculk_sensor_phase = SculkSensorPhase::Active;
            props.power = power;
            (props.to_state_id(block), props.waterlogged)
        };
        world.set_block_state(pos, new_state, BlockFlags::NOTIFY_ALL);
        world.schedule_block_tick(block, *pos, Self::active_ticks(block), TickPriority::Normal);
        Self::update_neighbours(world, pos);
        Self::try_resonate_vibration(world, pos, source, frequency);
        world.game_event(
            GameEvent::SculkSensorTendrilsClicking,
            pos.to_centered_f64(),
            GameEventContext::of(source.cloned(), None),
        );
        if !waterlogged {
            world.play_sound_fine(
                Sound::BlockSculkSensorClicking,
                SoundCategory::Blocks,
                &pos.to_centered_f64(),
                1.0,
                rand::random::<f32>() * 0.2 + 0.8,
            );
        }
    }

    /// `SculkSensorBlock.updateNeighbours`.
    fn update_neighbours(world: &Arc<World>, pos: &BlockPos) {
        world.update_neighbors(pos, None);
        let block = world.get_block(pos);
        world.update_neighbors_at(&pos.down(), block, None);
    }

    /// `SculkSensorBlock.tryResonateVibration`: amethyst next to the sensor repeats the vibration as
    /// the resonance event of the same frequency.
    fn try_resonate_vibration(
        world: &Arc<World>,
        pos: &BlockPos,
        source: Option<&Arc<dyn EntityBase>>,
        frequency: i32,
    ) {
        let Some(event) = vibration::resonance_event(frequency) else {
            return;
        };
        for direction in BlockDirection::all() {
            let neighbor = pos.offset(direction.to_offset());
            let (neighbor_block, neighbor_state) = world.get_block_and_state(&neighbor);
            if neighbor_block.has_tag(&tag::Block::MINECRAFT_VIBRATION_RESONATORS) {
                world.game_event(
                    event,
                    neighbor.to_centered_f64(),
                    GameEventContext::of(source.cloned(), Some(neighbor_state.id)),
                );
                world.play_sound_fine(
                    Sound::BlockAmethystBlockResonate,
                    SoundCategory::Blocks,
                    &neighbor.to_centered_f64(),
                    1.0,
                    resonance_pitch(frequency),
                );
            }
        }
    }
}

/// `SculkSensorBlock.RESONANCE_PITCH_BEND`: the note-block pitch of each frequency's tone.
fn resonance_pitch(frequency: i32) -> f32 {
    const TONE_MAP: [i32; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
    let note = TONE_MAP[frequency.clamp(0, 15) as usize];
    // NoteBlock.getPitchFromNote
    2.0f32.powf((note - 12) as f32 / 12.0)
}

impl BlockBehaviour for SculkSensorBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if args.block.id == BlockId::CALIBRATED_SCULK_SENSOR {
            let mut props = CalibratedSculkSensorLikeProperties::default(args.block);
            props.facing = args.player.living_entity.entity.get_horizontal_facing();
            props.to_state_id(args.block)
        } else {
            let props = SculkSensorLikeProperties::default(args.block);
            props.to_state_id(args.block)
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        if args.block.id == BlockId::CALIBRATED_SCULK_SENSOR {
            let entity = CalibratedSculkSensorBlockEntity::new(*args.position);
            args.world.add_block_entity(Arc::new(entity));
        } else if args.block.id == BlockId::SCULK_SENSOR {
            let entity = SculkSensorBlockEntity::new(*args.position);
            args.world.add_block_entity(Arc::new(entity));
        }
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        if args.block.id == BlockId::SCULK_SENSOR {
            let props = SculkSensorLikeProperties::from_state_id(args.state.id);
            if props.sculk_sensor_phase == SculkSensorPhase::Active {
                props.power
            } else {
                0
            }
        } else if args.block.id == BlockId::CALIBRATED_SCULK_SENSOR {
            let props = CalibratedSculkSensorLikeProperties::from_state_id(args.state.id);
            if props.sculk_sensor_phase == SculkSensorPhase::Active {
                props.power
            } else {
                0
            }
        } else {
            0
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        // Vanilla reads the frequency only while the sensor is active.
        if sculk_sensor_phase(args.block, args.state.id) != SculkSensorPhase::Active {
            return Some(0);
        }

        let be = args.world.get_block_entity(args.position)?;
        if let Some(sensor_be) = be.as_any().downcast_ref::<SculkSensorBlockEntity>() {
            return Some(
                *sensor_be
                    .last_vibration_frequency
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) as u8,
            );
        }
        if let Some(cal_be) = be
            .as_any()
            .downcast_ref::<CalibratedSculkSensorBlockEntity>()
        {
            return Some(
                *cal_be
                    .last_vibration_frequency
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) as u8,
            );
        }
        None
    }

    /// `SculkSensorBlock.tick`: an active sensor deactivates into cooldown, and a cooling one goes
    /// back to inactive.
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        let calibrated = args.block.id == BlockId::CALIBRATED_SCULK_SENSOR;
        let (phase, waterlogged) = if calibrated {
            let props = CalibratedSculkSensorLikeProperties::from_state_id(state.id);
            (props.sculk_sensor_phase, props.waterlogged)
        } else {
            let props = SculkSensorLikeProperties::from_state_id(state.id);
            (props.sculk_sensor_phase, props.waterlogged)
        };
        let with_phase = |phase: SculkSensorPhase, clear_power: bool| {
            if calibrated {
                let mut props = CalibratedSculkSensorLikeProperties::from_state_id(state.id);
                props.sculk_sensor_phase = phase;
                if clear_power {
                    props.power = 0;
                }
                props.to_state_id(args.block)
            } else {
                let mut props = SculkSensorLikeProperties::from_state_id(state.id);
                props.sculk_sensor_phase = phase;
                if clear_power {
                    props.power = 0;
                }
                props.to_state_id(args.block)
            }
        };
        match phase {
            // SculkSensorBlock.deactivate
            SculkSensorPhase::Active => {
                args.world.set_block_state(
                    args.position,
                    with_phase(SculkSensorPhase::Cooldown, true),
                    BlockFlags::NOTIFY_ALL,
                );
                args.world.schedule_block_tick(
                    args.block,
                    *args.position,
                    10,
                    TickPriority::Normal,
                );
                Self::update_neighbours(args.world, args.position);
            }
            SculkSensorPhase::Cooldown => {
                args.world.set_block_state(
                    args.position,
                    with_phase(SculkSensorPhase::Inactive, false),
                    BlockFlags::NOTIFY_ALL,
                );
                if !waterlogged {
                    args.world.play_sound_fine(
                        Sound::BlockSculkSensorClickingStop,
                        SoundCategory::Blocks,
                        &args.position.to_centered_f64(),
                        1.0,
                        rand::random::<f32>() * 0.2 + 0.8,
                    );
                }
            }
            SculkSensorPhase::Inactive => {}
        }
    }

    /// `SculkSensorBlock.stepOn`: an entity stepping on an inactive sensor vibrates it directly.
    fn on_entity_step(&self, args: OnEntityStepArgs<'_>) {
        if args.below_supporting_block
            || !Self::can_activate(args.block, args.state.id)
            || args.entity.get_entity().entity_type == &EntityType::WARDEN
            || !Self::can_receive_vibration(
                args.world,
                args.position,
                args.position,
                GameEvent::Step,
            )
        {
            return;
        }
        let Some(block_entity) = args.world.get_block_entity(args.position) else {
            return;
        };
        let Some(user) = block_entity.as_vibration_user() else {
            return;
        };
        vibration::force_schedule_vibration(
            user,
            args.world,
            GameEvent::Step,
            &GameEventContext::of(vibration::entity_handle(args.world, args.entity), None),
            args.entity.get_entity().pos.load(),
        );
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}
