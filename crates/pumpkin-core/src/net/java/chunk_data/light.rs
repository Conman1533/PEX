use pumpkin_protocol::codec::bit_set::BitSet;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::{CLightUpdate, LightData};
use pumpkin_protocol::ser::WritingError;
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::chunk::format::LightContainer;

pub trait ChunkLightExt {
    fn from_chunk(chunk: &ChunkData, version: JavaMinecraftVersion) -> Result<Self, WritingError>
    where
        Self: Sized;
}

impl ChunkLightExt for CLightUpdate {
    fn from_chunk(chunk: &ChunkData, version: JavaMinecraftVersion) -> Result<Self, WritingError> {
        let light_data = light_data_from_chunk(chunk, version)?;
        Ok(Self {
            chunk_x: VarInt(chunk.x),
            chunk_z: VarInt(chunk.z),
            light_data,
        })
    }
}

/// How many sections at the top of the column are fully lit, sky light 15 everywhere.
///
/// Vanilla keeps no sky light layer for that run, and the client gives every section above its
/// highest layer sky light 15 (`SkyLightSectionStorage.getLightValue`), so light data leaves it
/// out instead of sending 2048 bytes of 0xFF for each section. Only the top run counts: for a
/// fully lit section lower down the client would read the bottom of the next layer up, not 15.
pub(super) fn fully_lit_top_sections(sky_light: &[LightContainer]) -> usize {
    sky_light
        .iter()
        .rev()
        .take_while(|section| section.is_uniform(15))
        .count()
}

#[expect(clippy::too_many_lines)]
pub fn light_data_from_chunk(
    chunk: &ChunkData,
    version: JavaMinecraftVersion,
) -> Result<LightData, WritingError> {
    let light_engine = chunk
        .light_engine
        .lock()
        .map_err(|_| WritingError::Message("light_engine lock poisoned".into()))?;

    if version < JavaMinecraftVersion::V_1_18 {
        let base_section = (0 - chunk.section.min_y).max(0) as usize / 16;
        let mut sky_light_mask = 0u64;
        let mut block_light_mask = 0u64;
        let mut sky_light_empty_mask = 0u64;
        let mut block_light_empty_mask = 0u64;
        let mut sky_light_arrays = Vec::new();
        let mut block_light_arrays = Vec::new();

        // Bit 0: Y = -1 (below world section 0)
        if base_section > 0 && base_section - 1 < light_engine.sky_light.len() {
            match &light_engine.sky_light[base_section - 1] {
                LightContainer::Full(data) => {
                    sky_light_mask |= 1 << 0;
                    sky_light_arrays.push(data.to_vec());
                }
                LightContainer::Empty(val) if *val > 0 => {
                    sky_light_mask |= 1 << 0;
                    sky_light_arrays.push(vec![*val << 4 | *val; 2048]);
                }
                LightContainer::Empty(_) => {
                    sky_light_empty_mask |= 1 << 0;
                }
            }
        } else {
            sky_light_empty_mask |= 1 << 0;
        }

        if base_section > 0 && base_section - 1 < light_engine.block_light.len() {
            match &light_engine.block_light[base_section - 1] {
                LightContainer::Full(data) => {
                    block_light_mask |= 1 << 0;
                    block_light_arrays.push(data.to_vec());
                }
                LightContainer::Empty(val) if *val > 0 => {
                    block_light_mask |= 1 << 0;
                    block_light_arrays.push(vec![*val << 4 | *val; 2048]);
                }
                LightContainer::Empty(_) => {
                    block_light_empty_mask |= 1 << 0;
                }
            }
        } else {
            block_light_empty_mask |= 1 << 0;
        }

        // Bits 1..=16: world sections (Y = 0..15)
        for i in 0..16 {
            let bit_index = i + 1;
            let sec_idx = base_section + i;

            if sec_idx < light_engine.sky_light.len() {
                match &light_engine.sky_light[sec_idx] {
                    LightContainer::Full(data) => {
                        sky_light_mask |= 1 << bit_index;
                        sky_light_arrays.push(data.to_vec());
                    }
                    LightContainer::Empty(val) if *val > 0 => {
                        sky_light_mask |= 1 << bit_index;
                        sky_light_arrays.push(vec![*val << 4 | *val; 2048]);
                    }
                    LightContainer::Empty(_) => {
                        sky_light_empty_mask |= 1 << bit_index;
                    }
                }
            } else {
                sky_light_empty_mask |= 1 << bit_index;
            }

            if sec_idx < light_engine.block_light.len() {
                match &light_engine.block_light[sec_idx] {
                    LightContainer::Full(data) => {
                        block_light_mask |= 1 << bit_index;
                        block_light_arrays.push(data.to_vec());
                    }
                    LightContainer::Empty(val) if *val > 0 => {
                        block_light_mask |= 1 << bit_index;
                        block_light_arrays.push(vec![*val << 4 | *val; 2048]);
                    }
                    LightContainer::Empty(_) => {
                        block_light_empty_mask |= 1 << bit_index;
                    }
                }
            } else {
                block_light_empty_mask |= 1 << bit_index;
            }
        }

        // Bit 17: Y = 16 (above world section 15)
        let top_sec = base_section + 16;
        if top_sec < light_engine.sky_light.len() {
            match &light_engine.sky_light[top_sec] {
                LightContainer::Full(data) => {
                    sky_light_mask |= 1 << 17;
                    sky_light_arrays.push(data.to_vec());
                }
                LightContainer::Empty(val) if *val > 0 => {
                    sky_light_mask |= 1 << 17;
                    sky_light_arrays.push(vec![*val << 4 | *val; 2048]);
                }
                LightContainer::Empty(_) => {
                    sky_light_empty_mask |= 1 << 17;
                }
            }
        } else {
            sky_light_empty_mask |= 1 << 17;
        }

        if top_sec < light_engine.block_light.len() {
            match &light_engine.block_light[top_sec] {
                LightContainer::Full(data) => {
                    block_light_mask |= 1 << 17;
                    block_light_arrays.push(data.to_vec());
                }
                LightContainer::Empty(val) if *val > 0 => {
                    block_light_mask |= 1 << 17;
                    block_light_arrays.push(vec![*val << 4 | *val; 2048]);
                }
                LightContainer::Empty(_) => {
                    block_light_empty_mask |= 1 << 17;
                }
            }
        } else {
            block_light_empty_mask |= 1 << 17;
        }

        Ok(LightData {
            trust_edges: true,
            sky_light_mask: BitSet::from_u64(sky_light_mask),
            block_light_mask: BitSet::from_u64(block_light_mask),
            empty_sky_light_mask: BitSet::from_u64(sky_light_empty_mask),
            empty_block_light_mask: BitSet::from_u64(block_light_empty_mask),
            sky_light_arrays,
            block_light_arrays,
        })
    } else {
        let num_sections = light_engine.sky_light.len();
        let mut sky_light_empty_mask = 0u64;
        let mut block_light_empty_mask = 0u64;
        let mut sky_light_mask = 0u64;
        let mut block_light_mask = 0u64;

        let mut sky_light_arrays = Vec::new();
        let mut block_light_arrays = Vec::new();

        sky_light_empty_mask |= 1 << 0;
        block_light_empty_mask |= 1 << 0;

        let sky_sections = num_sections - fully_lit_top_sections(&light_engine.sky_light);

        for section_index in 0..num_sections {
            let bit_index = section_index + 1;

            if section_index >= sky_sections {
                // Neither mask: the client has no layer here, which reads as sky light 15.
            } else if let Some(data) = light_engine.sky_light[section_index].to_array() {
                sky_light_mask |= 1 << bit_index;
                sky_light_arrays.push(data.to_vec());
            } else {
                sky_light_empty_mask |= 1 << bit_index;
            }

            if let Some(data) = light_engine.block_light[section_index].to_array() {
                block_light_mask |= 1 << bit_index;
                block_light_arrays.push(data.to_vec());
            } else {
                block_light_empty_mask |= 1 << bit_index;
            }
        }

        // An empty layer above the world would make the client read sky light 0 for the omitted
        // sections below it.
        if sky_sections == num_sections {
            sky_light_empty_mask |= 1 << (num_sections + 1);
        }
        block_light_empty_mask |= 1 << (num_sections + 1);

        Ok(LightData {
            trust_edges: true,
            sky_light_mask: BitSet::from_u64(sky_light_mask),
            block_light_mask: BitSet::from_u64(block_light_mask),
            empty_sky_light_mask: BitSet::from_u64(sky_light_empty_mask),
            empty_block_light_mask: BitSet::from_u64(block_light_empty_mask),
            sky_light_arrays,
            block_light_arrays,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overworld sections, -64 to 320.
    const SECTIONS: usize = 24;

    /// Sky light from `sky`, one entry per section from the bottom; no block light.
    fn chunk_with_sky(sky: impl Fn(usize) -> LightContainer) -> ChunkData {
        let chunk = ChunkData::empty(0, 0);
        let mut light = chunk.light_engine.lock().unwrap();
        light.sky_light = (0..SECTIONS).map(sky).collect();
        light.block_light = vec![LightContainer::Empty(0); SECTIONS].into_boxed_slice();
        drop(light);
        chunk
    }

    /// The sky light mask, empty sky light mask and number of sky light arrays sent.
    fn sky_masks(chunk: &ChunkData) -> (i64, i64, usize) {
        let data = light_data_from_chunk(chunk, JavaMinecraftVersion::V_26_3).unwrap();
        (
            data.sky_light_mask.0.first().copied().unwrap_or(0),
            data.empty_sky_light_mask.0.first().copied().unwrap_or(0),
            data.sky_light_arrays.len(),
        )
    }

    #[test]
    fn fully_lit_top_sections_send_no_sky_light() {
        let chunk = chunk_with_sky(|index| match index {
            // A fully lit section below a darker one must still be sent.
            1 => LightContainer::Empty(15),
            0..4 => LightContainer::Full(vec![0x7A; LightContainer::ARRAY_SIZE].into_boxed_slice()),
            _ => LightContainer::Full(vec![0xFF; LightContainer::ARRAY_SIZE].into_boxed_slice()),
        });
        let (sent, empty, arrays) = sky_masks(&chunk);
        // Bits 1 to 4 are sections 0 to 3. Nothing above them, not even an empty layer.
        assert_eq!(sent, 0b1_1110);
        assert_eq!(arrays, 4);
        assert_eq!(empty, 1, "only the section below the world is empty");
    }

    #[test]
    fn column_without_fully_lit_top_sends_every_section() {
        let chunk = chunk_with_sky(|_| LightContainer::Empty(14));
        let (sent, empty, arrays) = sky_masks(&chunk);
        assert_eq!(arrays, SECTIONS);
        assert_eq!(sent, ((1 << SECTIONS) - 1) << 1);
        assert_eq!(empty, 1 | (1 << (SECTIONS + 1)));
    }
}
