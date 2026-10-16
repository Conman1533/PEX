use super::BlockEntity;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use std::sync::{Arc, Mutex};

use pumpkin_data::game_event::GameEvent;
use pumpkin_util::math::vector3::Vector3;

use crate::block::blocks::redstone::sculk_sensor::SculkSensorBlock;
use crate::entity::EntityBase;
use crate::world::World;
use crate::world::vibration::{self, GameEventContext, VibrationData, VibrationUser};

pub struct SculkSensorBlockEntity {
    pub position: BlockPos,
    pub last_vibration_frequency: Mutex<i32>,
    pub vibration: Mutex<VibrationData>,
}

impl BlockEntity for SculkSensorBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let last_vibration_frequency = nbt.get_int("last_vibration_frequency").unwrap_or(0);
        Self {
            position,
            last_vibration_frequency: Mutex::new(last_vibration_frequency),
            vibration: Mutex::default(),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Ok(last_vibration_frequency) = self.last_vibration_frequency.lock() {
            nbt.put_int("last_vibration_frequency", *last_vibration_frequency);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_int(
            "last_vibration_frequency",
            *self.last_vibration_frequency.try_lock().ok()?,
        );
        Some(nbt)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn tick(&self, world: &Arc<World>) {
        vibration::tick(self, world);
    }

    fn as_vibration_user(&self) -> Option<&dyn VibrationUser> {
        Some(self)
    }
}

impl VibrationUser for SculkSensorBlockEntity {
    fn vibration_data(&self) -> &Mutex<VibrationData> {
        &self.vibration
    }

    fn listener_radius(&self) -> i32 {
        8
    }

    fn listener_pos(&self) -> Vector3<f64> {
        self.position.to_centered_f64()
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        origin: &BlockPos,
        event: GameEvent,
        _context: &GameEventContext,
    ) -> bool {
        SculkSensorBlock::can_receive_vibration(world, &self.position, origin, event)
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _origin: &BlockPos,
        event: GameEvent,
        source: Option<&Arc<dyn EntityBase>>,
        distance: f32,
    ) {
        SculkSensorBlock::receive_vibration(
            world,
            &self.position,
            &self.last_vibration_frequency,
            event,
            source,
            vibration::redstone_strength_for_distance(distance, self.listener_radius()),
        );
    }

    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        true
    }
}

impl SculkSensorBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_sensor";
    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            last_vibration_frequency: Mutex::new(0),
            vibration: Mutex::new(VibrationData {
                current: None,
                selector: vibration::VibrationSelector::new(),
                travel_time_in_ticks: 0,
            }),
        }
    }
}
