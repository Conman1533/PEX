// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT
//
// Native PEX adapter replacing WorldPumpkin's Wasm host calls.

use crate::engine::BlockPos;
use crate::messages::{self, MessageKind};
use pumpkin::command::context::command_source::CommandSource;
use pumpkin::world::World as ServerWorld;
use pumpkin_data::BlockStateId;
use pumpkin_nbt::{Nbt, NbtCompound};
use pumpkin_util::math::position::BlockPos as ServerPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use std::sync::Arc;

impl From<BlockPos> for ServerPos {
    fn from(pos: BlockPos) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }
}

impl From<ServerPos> for BlockPos {
    fn from(pos: ServerPos) -> Self {
        Self {
            x: pos.0.x,
            y: pos.0.y,
            z: pos.0.z,
        }
    }
}

/// A world handle, cheap to clone like the Wasm resource it replaces.
#[derive(Clone)]
pub struct World(pub Arc<ServerWorld>);

impl World {
    pub fn get_id(&self) -> String {
        self.0.get_world_name().to_owned()
    }

    pub fn get_min_y(&self) -> i32 {
        self.0.min_y
    }

    /// Highest buildable Y, read from the dimension instead of the hardcoded 319/255 `WorldPumpkin` used.
    pub fn get_max_y(&self) -> i32 {
        self.0.min_y + self.0.dimension.height - 1
    }

    pub fn get_top_block_y(&self, x: i32, z: i32) -> i32 {
        self.0.get_top_block(Vector2::new(x, z))
    }

    pub fn get_block_state_id(&self, pos: BlockPos) -> u16 {
        self.0.get_block_state_id(&pos.into()).as_u16()
    }

    pub fn set_block_state(&self, pos: BlockPos, state: u16, flags: BlockFlags) {
        if let Some(state) = BlockStateId::new(state) {
            self.0.set_block_state(&pos.into(), state, flags);
        }
    }

    /// Serializes the block entity at `pos`, matching what the Wasm host returned.
    pub fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<Vec<u8>> {
        let entity = self.0.get_block_entity(&pos.into())?;
        let mut nbt = NbtCompound::new();
        entity.write_internal(&mut nbt);
        Some(Nbt::from(nbt).write_unnamed().to_vec())
    }

    pub fn set_block_entity_nbt(&self, pos: BlockPos, bytes: &[u8]) -> Result<(), String> {
        let mut cursor = std::io::Cursor::new(bytes);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        );
        let mut nbt = Nbt::read_unnamed(&mut reader)
            .map_err(|err| format!("invalid NBT: {err}"))?
            .root_tag;
        // Moved and replayed entities must land where they are written, not where they were saved.
        nbt.put_int("x", pos.x);
        nbt.put_int("y", pos.y);
        nbt.put_int("z", pos.z);
        self.0.add_block_entity_nbt(pos.into(), &nbt);
        Ok(())
    }
}

/// Whoever ran a command: a player, or the console or a command block with a position.
pub struct Actor<'a> {
    source: &'a CommandSource,
}

impl<'a> Actor<'a> {
    pub const fn new(source: &'a CommandSource) -> Self {
        Self { source }
    }

    /// Identifies the history and selection owner, like the sender name `WorldPumpkin` used.
    pub fn name(&self) -> String {
        self.source.name.clone()
    }

    pub fn world(&self) -> World {
        World(self.source.world().clone())
    }

    pub const fn block_pos(&self) -> BlockPos {
        let pos = self.source.position;
        BlockPos {
            x: pos.x.floor() as i32,
            y: pos.y.floor() as i32,
            z: pos.z.floor() as i32,
        }
    }

    pub const fn yaw(&self) -> f32 {
        self.source.rotation.y
    }

    pub const fn pitch(&self) -> f32 {
        self.source.rotation.x
    }

    pub fn has_permission(&self, node: &str) -> bool {
        self.source.has_permission(node)
    }

    pub fn send_ok(&self, message: &str) {
        self.source
            .send_message(messages::prefixed(MessageKind::Info, message));
    }

    pub fn send_error(&self, message: &str) {
        self.source
            .send_message(messages::prefixed(MessageKind::Error, message));
    }

    /// Returns the first solid block within `range` along the actor's line of sight.
    pub fn target_block(&self, range: f64) -> Option<BlockPos> {
        let entity = self.source.entity.as_ref()?;
        let entity = entity.get_entity();
        let pos = entity.pos.load();
        let start = Vector3::new(pos.x, entity.get_eye_y(), pos.z);
        let (yaw, pitch) = (
            f64::from(entity.yaw.load()).to_radians(),
            f64::from(entity.pitch.load()).to_radians(),
        );
        let look = Vector3::new(
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let end = Vector3::new(
            start.x + look.x * range,
            start.y + look.y * range,
            start.z + look.z * range,
        );
        self.source
            .world()
            .raycast(start, end, |pos, world| {
                !world.get_block_state(pos).is_air()
            })
            .map(|(pos, _)| pos.into())
    }
}
