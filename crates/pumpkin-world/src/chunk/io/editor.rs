//! Changes the NBT stored in region files in place, in whichever format each file is.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use bytes::Bytes;
use pumpkin_config::chunk::{AnvilChunkConfig, ChunkConfig};
use pumpkin_nbt::{Nbt, compound::NbtCompound, deserializer::NbtReadHelperJava};
use pumpkin_util::math::vector2::Vector2;
use tracing::warn;

use super::converter::{ChunkConversionError, RegionFormat};
use super::{RegionFile, chunk_in_region};
use crate::chunk::{
    ChunkData, ChunkParsingError, ChunkReadingError, ChunkWritingError,
    format::{
        anvil::{AnvilRegion, CHUNK_COUNT},
        has_named_root,
        linear::LinearV2File,
        pump::{PUMP_CHUNK_MAGIC, PumpFile},
    },
};

/// What [`edit_region`] went through.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RegionEdit {
    /// Chunks read from the file.
    pub chunks: usize,
    /// Chunks that were changed and written back.
    pub changed: usize,
}

/// Calls `edit` on the NBT of every chunk in the region file at `path`, and writes back the chunks
/// it changed. `edit` returns whether it changed anything.
///
/// A chunk stored as NBT gets its whole compound. A chunk in Pump's own binary format keeps its
/// blocks, light and ticks in that format, so `edit` gets each of its block entities and its custom
/// data instead. Region files and chunks that are damaged are skipped with a warning, and other
/// I/O errors are returned. Before anything is
/// written, `before_write` gets the files the write changes: the region file, and for Anvil the
/// external chunk files next to it. An Anvil file is written back with the compression `config`
/// has, or the default one when the server uses another format.
pub fn edit_region(
    path: &Path,
    region: Vector2<i32>,
    format: RegionFormat,
    config: &ChunkConfig,
    edit: impl Fn(&mut NbtCompound) -> bool,
    before_write: impl FnOnce(&[PathBuf]) -> io::Result<()>,
) -> Result<RegionEdit, ChunkConversionError> {
    let edit = RegionEditor { path, region, edit };
    match format {
        RegionFormat::Anvil => {
            let Some(file) = edit.open::<AnvilRegion>()? else {
                return Ok(RegionEdit::default());
            };
            let mut files = vec![path.to_owned()];
            files.extend(file.external_chunk_paths());
            let anvil_config = match config {
                ChunkConfig::Anvil(anvil_config) => anvil_config.clone(),
                ChunkConfig::Linear | ChunkConfig::Pump => AnvilChunkConfig::default(),
            };
            edit.run(file, &anvil_config, &files, before_write)
        }
        RegionFormat::Linear => {
            let Some(file) = edit.open::<LinearV2File>()? else {
                return Ok(RegionEdit::default());
            };
            edit.run(file, &(), &[path.to_owned()], before_write)
        }
        RegionFormat::Pump => {
            let Some(file) = edit.open::<PumpFile>()? else {
                return Ok(RegionEdit::default());
            };
            edit.run(file, &(), &[path.to_owned()], before_write)
        }
    }
}

/// Whether a read failed because the file is damaged, which reading it again won't change. The
/// formats report truncated records and missing external chunk files as I/O errors.
fn is_damaged(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData | io::ErrorKind::NotFound
    )
}

struct RegionEditor<'a, E> {
    path: &'a Path,
    region: Vector2<i32>,
    edit: E,
}

impl<E: Fn(&mut NbtCompound) -> bool> RegionEditor<'_, E> {
    fn open<R: RegionFile>(&self) -> Result<Option<R>, ChunkConversionError> {
        match R::open(self.path, self.region) {
            Ok(file) => Ok(Some(file)),
            Err(ChunkReadingError::IoError(err)) if !is_damaged(&err) => {
                Err(ChunkConversionError::Io(err))
            }
            Err(err) => {
                warn!(
                    "Skipped {}, which can't be read: {err}",
                    self.path.display()
                );
                Ok(None)
            }
        }
    }

    fn run<R: RegionFile>(
        &self,
        mut file: R,
        config: &R::Config,
        files: &[PathBuf],
        before_write: impl FnOnce(&[PathBuf]) -> io::Result<()>,
    ) -> Result<RegionEdit, ChunkConversionError> {
        let mut result = RegionEdit::default();
        let mut changed = Vec::new();
        for index in 0..CHUNK_COUNT {
            let chunk = chunk_in_region(self.region, index);
            let raw = match file.read_chunk(index) {
                Ok(Some(raw)) => raw,
                Ok(None) => continue,
                Err(ChunkReadingError::IoError(err)) if !is_damaged(&err) => {
                    return Err(ChunkConversionError::Io(err));
                }
                Err(err) => {
                    self.skip(chunk, &err);
                    continue;
                }
            };
            result.chunks += 1;
            // Decoding only works on the bytes already read, so any error means they're damaged.
            let payload = match R::decode_chunk(raw) {
                Ok(payload) => payload,
                Err(err) => {
                    self.skip(chunk, &err);
                    continue;
                }
            };
            match self.edit_chunk(&payload, chunk) {
                Ok(Some(payload)) => changed.push((index, R::encode_chunk(payload, config)?)),
                Ok(None) => {}
                Err(err) => self.skip(chunk, &err),
            }
        }
        if changed.is_empty() {
            return Ok(result);
        }
        before_write(files)?;
        result.changed = changed.len();
        for (index, chunk) in changed {
            file.stage_chunk(index, chunk);
        }
        file.flush(true).map_err(ChunkWritingError::IoError)?;
        Ok(result)
    }

    /// Returns the chunk to write back if `edit` changed it.
    fn edit_chunk(
        &self,
        payload: &[u8],
        chunk: Vector2<i32>,
    ) -> Result<Option<Bytes>, ChunkParsingError> {
        if payload.starts_with(&PUMP_CHUNK_MAGIC) {
            let mut data = ChunkData::internal_from_pump_bytes(payload, chunk)?;
            let mut changed = false;
            for nbt in data
                .pending_block_entities
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner)
                .values_mut()
            {
                changed |= (self.edit)(nbt);
            }
            changed |= (self.edit)(
                data.custom_data
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner),
            );
            return Ok(changed.then(|| data.internal_to_pump_bytes()));
        }

        let named = has_named_root(payload);
        let mut cursor = io::Cursor::new(payload);
        let mut reader = NbtReadHelperJava::new(&mut cursor);
        let mut nbt = if named {
            Nbt::read(&mut reader)
        } else {
            Nbt::read_unnamed(&mut reader)
        }
        .map_err(|err| ChunkParsingError::ErrorDeserializingChunk(err.to_string()))?;
        if !(self.edit)(&mut nbt.root_tag) {
            return Ok(None);
        }
        Ok(Some(if named {
            nbt.write()
        } else {
            nbt.write_unnamed()
        }))
    }

    fn skip(&self, chunk: Vector2<i32>, err: &dyn std::error::Error) {
        warn!(
            "Skipped chunk {}, {} in {}, which can't be read: {err}",
            chunk.x,
            chunk.y,
            self.path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use bytes::Bytes;
    use pumpkin_config::chunk::ChunkConfig;
    use pumpkin_data::{Block, chunk::ChunkStatus};
    use pumpkin_nbt::{Nbt, compound::NbtCompound};
    use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
    use rustc_hash::FxHashMap;
    use tempfile::TempDir;

    use super::{RegionEdit, edit_region};
    use crate::chunk::{
        ChunkData, ChunkHeightmaps, ChunkLight, ChunkSections,
        format::pump::PumpFile,
        io::{RegionFile, converter::RegionFormat},
    };
    use crate::tick::scheduler::ChunkTickScheduler;

    fn owned_by(owner: &str) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("owner", owner.to_owned());
        nbt
    }

    /// Changes a top-level `owner` of "old" to "new".
    fn rename_owner(nbt: &mut NbtCompound) -> bool {
        if nbt.get_string("owner") != Some("old") {
            return false;
        }
        nbt.put_string("owner", "new".to_owned());
        true
    }

    fn pump_chunk(position: Vector2<i32>, mut block_entity: NbtCompound) -> Bytes {
        let sections = ChunkSections::new(24, -64);
        sections.set_block_absolute_y(2, -60, 4, Block::DIAMOND_ORE.default_state.id);
        let pos = BlockPos::new(position.x * 16 + 1, 64, position.y * 16 + 1);
        block_entity.put_int("x", pos.0.x);
        block_entity.put_int("y", pos.0.y);
        block_entity.put_int("z", pos.0.z);
        let chunk = ChunkData {
            section: sections,
            heightmap: Mutex::new(ChunkHeightmaps::default()),
            x: position.x,
            z: position.y,
            block_ticks: ChunkTickScheduler::default(),
            fluid_ticks: ChunkTickScheduler::default(),
            pending_block_entities: Mutex::new(FxHashMap::from_iter([(pos, block_entity)])),
            light_engine: Mutex::new(ChunkLight::default()),
            light_populated: AtomicBool::new(true),
            status: ChunkStatus::Full,
            dirty: AtomicBool::new(false),
            blending_data: None,
            inhabited_time: AtomicU64::new(12345),
            custom_data: Mutex::new(owned_by("old")),
        };
        chunk.internal_to_pump_bytes()
    }

    fn read(path: &std::path::Path, region: Vector2<i32>, index: usize) -> Bytes {
        let file = PumpFile::open(path, region).unwrap();
        PumpFile::decode_chunk(file.read_chunk(index).unwrap().unwrap()).unwrap()
    }

    #[test]
    fn edits_block_entities_of_pump_chunks_and_whole_nbt_chunks() {
        let dir = TempDir::new().unwrap();
        let region = Vector2::new(-1, 2);
        let path = dir.path().join(PumpFile::file_name(region));
        let mut block_entity = owned_by("old");
        block_entity.put_string("id", "minecraft:trial_spawner".to_owned());
        let chunks = [
            pump_chunk(Vector2::new(-32, 64), block_entity),
            Nbt::from(owned_by("old")).write(),
            Nbt::from(owned_by("someone else")).write(),
        ];
        let mut file = PumpFile::open(&path, region).unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            file.stage_chunk(index, PumpFile::encode_chunk(chunk.clone(), &()).unwrap());
        }
        file.flush(true).unwrap();
        drop(file);

        let mut backed_up = Vec::new();
        let result = edit_region(
            &path,
            region,
            RegionFormat::Pump,
            &ChunkConfig::Pump,
            rename_owner,
            |files| {
                backed_up.extend_from_slice(files);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            result,
            RegionEdit {
                chunks: 3,
                changed: 2
            }
        );
        assert_eq!(backed_up, std::slice::from_ref(&path));
        let chunk =
            ChunkData::internal_from_pump_bytes(&read(&path, region, 0), Vector2::new(-32, 64))
                .unwrap();
        let block_entities = chunk.pending_block_entities.lock().unwrap();
        let block_entity = block_entities.values().next().unwrap();
        assert_eq!(block_entity.get_string("owner"), Some("new"));
        assert_eq!(
            block_entity.get_string("id"),
            Some("minecraft:trial_spawner")
        );
        assert_eq!(
            chunk.custom_data.lock().unwrap().get_string("owner"),
            Some("new")
        );
        assert_eq!(
            chunk.section.get_block_absolute_y(2, -60, 4),
            Some(Block::DIAMOND_ORE.default_state.id)
        );
        assert_eq!(read(&path, region, 1), Nbt::from(owned_by("new")).write());
        assert_eq!(read(&path, region, 2), chunks[2]);
    }
}
