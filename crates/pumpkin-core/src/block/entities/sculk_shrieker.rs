use super::BlockEntity;
use crate::block::blocks::sculk::sculk_shrieker::SculkShriekerBlock;
use crate::entity::EntityBase;
use crate::world::World;
use crate::world::vibration::{self, GameEventContext, VibrationData, VibrationUser};
use pumpkin_data::block_properties::SculkShriekerLikeProperties;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::tag::{self, Tag};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct SculkShriekerBlockEntity {
    pub position: BlockPos,
    pub warning_level: Mutex<i32>,
    pub shrieking_can_summon: AtomicBool,
    pub vibration: Mutex<VibrationData>,
}

impl BlockEntity for SculkShriekerBlockEntity {
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
        let warning_level = nbt.get_int("warning_level").unwrap_or(0);
        Self {
            position,
            warning_level: Mutex::new(warning_level),
            shrieking_can_summon: AtomicBool::new(false),
            vibration: Mutex::new(VibrationData::read_nbt(nbt)),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Ok(warning_level) = self.warning_level.lock() {
            nbt.put_int("warning_level", *warning_level);
        }
        if let Ok(vibration) = self.vibration.lock() {
            vibration.write_nbt(nbt);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_int("warning_level", *self.warning_level.try_lock().ok()?);
        Some(nbt)
    }

    fn on_block_replaced(self: Arc<Self>, world: &Arc<World>, position: &BlockPos) {
        if self.shrieking_can_summon.swap(false, Ordering::Relaxed) {
            let warning_level = *self
                .warning_level
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            SculkShriekerBlock::respond(world, position, warning_level);
        }
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

/// `SculkShriekerBlockEntity.VibrationUser`: hears sculk sensors clicking within 8 blocks when a
/// player caused it, and shrieks for that player.
impl VibrationUser for SculkShriekerBlockEntity {
    fn vibration_data(&self) -> &Mutex<VibrationData> {
        &self.vibration
    }

    fn listener_radius(&self) -> i32 {
        8
    }

    fn listener_pos(&self) -> Vector3<f64> {
        self.position.to_centered_f64()
    }

    fn listenable_events(&self) -> &'static Tag {
        &tag::GameEvent::MINECRAFT_SHRIEKER_CAN_LISTEN
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        _origin: &BlockPos,
        _event: GameEvent,
        context: &GameEventContext,
    ) -> bool {
        let state = world.get_block_state(&self.position);
        !SculkShriekerLikeProperties::from_state_id(state.id).shrieking
            && context
                .source
                .as_ref()
                .and_then(|source| SculkShriekerBlock::try_get_player(world, source.as_ref()))
                .is_some()
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _origin: &BlockPos,
        _event: GameEvent,
        source: Option<&Arc<dyn EntityBase>>,
        projectile_owner: Option<&Arc<dyn EntityBase>>,
        _distance: f32,
    ) {
        if let Some(player) = projectile_owner
            .or(source)
            .and_then(|entity| SculkShriekerBlock::try_get_player(world, entity.as_ref()))
        {
            SculkShriekerBlock::try_shriek(world, &self.position, &player);
        }
    }

    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        true
    }
}

impl SculkShriekerBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_shrieker";
    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            warning_level: Mutex::new(0),
            shrieking_can_summon: AtomicBool::new(false),
            vibration: Mutex::default(),
        }
    }
}
