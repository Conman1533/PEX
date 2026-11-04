pub mod light;
pub mod util;
pub mod v1_18;

pub use light::ChunkLightExt;

use pumpkin_data::packet::clientbound::play::LEVEL_CHUNK_WITH_LIGHT;
use pumpkin_protocol::ClientPacket;
use pumpkin_protocol::packet::MultiVersionJavaPacket;
use pumpkin_protocol::ser::WritingError;
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::chunk::ChunkData;
use std::io::Write;

/// Sent by the server to provide the client with the full data for a chunk.
///
/// This includes heightmaps, the actual block and biome data (organized into sections),
/// block entities (like signs or chests), and the light level information for both
/// sky and block light.
pub struct CChunkData<'a>(pub &'a ChunkData);

impl MultiVersionJavaPacket for CChunkData<'_> {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        LEVEL_CHUNK_WITH_LIGHT.to_id(version)
    }
}

impl<'a> CChunkData<'a> {
    #[must_use]
    pub const fn new(chunk: &'a ChunkData) -> Self {
        Self(chunk)
    }
}

impl ClientPacket for CChunkData<'_> {
    fn write_packet_data(
        &self,
        write: impl Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), WritingError> {
        v1_18::write_chunk_data(self.0, write, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_world::chunk::ChunkData;
    use pumpkin_world::chunk::format::LightContainer;

    #[test]
    fn chunk_data_all_versions() {
        let chunk = ChunkData::empty(0, 0);
        let packet = CChunkData(&chunk);

        let versions = [JavaMinecraftVersion::V_26_3];

        for version in versions {
            let mut buf = Vec::new();
            let id = CChunkData::to_id(version);
            assert_ne!(id, -1, "Packet ID for version {version:?} must be valid");
            assert!(
                packet.write_packet_data(&mut buf, &version).is_ok(),
                "Failed to serialize chunk data for version {version:?}"
            );
            assert!(
                !buf.is_empty(),
                "Serialized buffer must not be empty for version {version:?}"
            );
        }
    }

    #[test]
    fn populated_chunk_data_all_versions() {
        let chunk = ChunkData::empty(0, 0);
        chunk
            .section
            .set_block_absolute_y(0, 64, 0, pumpkin_data::Block::STONE.default_state.id);
        chunk
            .section
            .set_block_absolute_y(1, 64, 1, pumpkin_data::Block::DIRT.default_state.id);

        let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
        nbt.put_string("id", "minecraft:chest".to_string());
        chunk.pending_block_entities.lock().unwrap().insert(
            pumpkin_util::math::position::BlockPos(pumpkin_util::math::vector3::Vector3::new(
                0, 64, 0,
            )),
            nbt,
        );

        let packet = CChunkData(&chunk);

        let versions = [JavaMinecraftVersion::V_26_3];

        for version in versions {
            let mut buf = Vec::new();
            let id = CChunkData::to_id(version);
            assert_ne!(id, -1, "Packet ID for version {version:?} must be valid");
            assert!(
                packet.write_packet_data(&mut buf, &version).is_ok(),
                "Failed to serialize populated chunk data for version {version:?}"
            );
            assert!(
                !buf.is_empty(),
                "Serialized buffer must not be empty for version {version:?}"
            );
        }
    }

    /// The chunk packet's light block is the light data `CLightUpdate` would send, so the light
    /// tests in `light.rs` hold for chunk packets too.
    #[test]
    fn chunk_packet_light_matches_light_update_data() {
        let chunk = ChunkData::empty(0, 0);
        let mut light = chunk.light_engine.lock().unwrap();
        let sections = light.sky_light.len();
        light.sky_light = (0..sections)
            .map(|index| match index {
                0..3 => LightContainer::Empty(0),
                3 | 5 => LightContainer::Full(vec![0x9C; LightContainer::ARRAY_SIZE].into()),
                _ => LightContainer::Empty(15),
            })
            .collect();
        drop(light);
        let version = JavaMinecraftVersion::V_26_3;
        let mut packet = Vec::new();
        CChunkData(&chunk)
            .write_packet_data(&mut packet, &version)
            .unwrap();
        let mut light = Vec::new();
        light::light_data_from_chunk(&chunk, version)
            .unwrap()
            .write(&mut light, &version)
            .unwrap();

        assert!(
            light.len() < 6 * LightContainer::ARRAY_SIZE,
            "the fully lit top sections were sent: {} bytes of light",
            light.len()
        );
        assert!(
            packet.ends_with(&light),
            "the chunk packet's light differs from the light update's"
        );
    }
}
