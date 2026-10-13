use bytes::{Buf, BufMut, Bytes};
use flate2::read::{GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder};
use lz4_java_wrc::Context;
use pumpkin_config::chunk::AnvilChunkConfig;
use pumpkin_util::math::vector2::Vector2;
use std::{
    io::{Read, SeekFrom, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufWriter},
    sync::Mutex,
};
use tracing::{debug, trace};

use crate::chunk::{
    ChunkParsingError, ChunkReadingError, ChunkSerializingError, ChunkWritingError,
    CompressionError,
    io::{ChunkSerializer, Dirtiable, LoadedData, run_blocking},
};

/// The side size of a region in chunks (one region is 32x32 chunks)
pub const REGION_SIZE: usize = 32;

/// The number of bits that identify two chunks in the same region
pub const SUBREGION_BITS: u8 = pumpkin_util::math::ceil_log2(REGION_SIZE as u32);

pub const SUBREGION_AND: i32 = i32::pow(2, SUBREGION_BITS as u32) - 1;

/// The number of chunks in a region
pub const CHUNK_COUNT: usize = REGION_SIZE * REGION_SIZE;

/// The number of bytes in a sector (4 KiB)
const SECTOR_BYTES: usize = 4096;

// 26.2
pub const WORLD_DATA_VERSION: i32 = 4903;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Compression {
    /// `GZip` Compression
    GZip = Self::GZIP_ID,
    /// `ZLib` Compression
    ZLib = Self::ZLIB_ID,
    /// LZ4 Compression (since 24w04a)
    LZ4 = Self::LZ4_ID,
    /// Custom compression algorithm (since 24w05a)
    Custom = Self::CUSTOM_ID,
}

pub enum CompressionRead<R: Read> {
    GZip(GzDecoder<R>),
    ZLib(ZlibDecoder<R>),
    LZ4(lz4_java_wrc::Lz4BlockInput<R>),
}

impl<R: Read> Read for CompressionRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::GZip(gzip) => gzip.read(buf),
            Self::ZLib(zlib) => zlib.read(buf),
            Self::LZ4(lz4) => lz4.read(buf),
        }
    }
}

#[derive(Clone)]
pub struct AnvilChunkData {
    compression: Option<Compression>,
    // Length is always the length of this + compression byte (1) so we dont need to save a length
    compressed_data: Bytes,
}

enum WriteAction {
    // Don't write anything
    Pass,
    // Write the entire file
    All,
    // Only write certain indices
    Parts(Vec<usize>),
}

impl WriteAction {
    /// If we are currently not writing, sets to new Parts enum,
    /// If we have parts enum, add to it,
    /// If we have All enum, do nothing
    fn maybe_update_chunk_index(&mut self, index: usize) {
        match self {
            Self::Pass => *self = Self::Parts(vec![index]),
            Self::Parts(parts) => {
                if !parts.contains(&index) {
                    parts.push(index);
                }
            }
            Self::All => {}
        }
    }
}

struct AnvilChunkMetadata {
    serialized_data: AnvilChunkData,
    timestamp: u32,
}

/// Tracks which sectors of a region file are in use, like vanilla's `RegionBitmap`.
#[derive(Default, Debug, PartialEq, Eq)]
struct RegionBitmap {
    used: Vec<bool>,
}

impl RegionBitmap {
    /// The first two sectors hold the location and timestamp tables.
    fn with_header() -> Self {
        let mut bitmap = Self::default();
        bitmap.force(0, 2);
        bitmap
    }

    fn force(&mut self, position: u32, size: u32) {
        let end = (position + size) as usize;
        if self.used.len() < end {
            self.used.resize(end, false);
        }
        self.used[position as usize..end].fill(true);
    }

    fn free(&mut self, position: u32, size: u32) {
        let end = ((position + size) as usize).min(self.used.len());
        if let Some(sectors) = self.used.get_mut(position as usize..end) {
            sectors.fill(false);
        }
    }

    /// Takes the first free run of `size` sectors, or the end of the file if none is long enough.
    fn allocate(&mut self, size: u32) -> u32 {
        let mut current = 0;
        loop {
            let free_start = self.used[current..]
                .iter()
                .position(|used| !used)
                .map_or(self.used.len(), |offset| current + offset);
            let free_end = self.used[free_start..]
                .iter()
                .position(|used| *used)
                .map(|offset| free_start + offset);
            match free_end {
                Some(free_end) if free_end - free_start < size as usize => current = free_end,
                _ => {
                    self.force(free_start as u32, size);
                    return free_start as u32;
                }
            }
        }
    }
}

/// What the file on disk holds and what still has to be written to it.
struct RegionLayout {
    write_action: WriteAction,
    used_sectors: RegionBitmap,
    /// The location table as it is on disk: sector offset << 8 | sector count, 0 when absent.
    locations: Box<[u32; CHUNK_COUNT]>,
}

impl RegionLayout {
    fn new() -> Self {
        Self {
            write_action: WriteAction::Pass,
            used_sectors: RegionBitmap::with_header(),
            locations: Box::new([0; CHUNK_COUNT]),
        }
    }
}

pub struct AnvilChunkFile<S: SingleChunkDataSerializer> {
    chunks_data: [Option<AnvilChunkMetadata>; CHUNK_COUNT],
    layout: Mutex<RegionLayout>,

    _dummy: PhantomData<S>,
}

impl Compression {
    const GZIP_ID: u8 = 1;
    const ZLIB_ID: u8 = 2;
    const NO_COMPRESSION_ID: u8 = 3;
    const LZ4_ID: u8 = 4;
    const CUSTOM_ID: u8 = 127;

    fn decompress_data(self, compressed_data: &[u8]) -> Result<Box<[u8]>, CompressionError> {
        fn decode<R: std::io::Read>(mut reader: R, capacity: usize) -> std::io::Result<Box<[u8]>> {
            let mut buf = Vec::with_capacity(capacity);
            reader.read_to_end(&mut buf)?;
            Ok(buf.into_boxed_slice())
        }

        let initial_capacity = compressed_data.len();

        match self {
            Self::GZip => decode(GzDecoder::new(compressed_data), initial_capacity)
                .map_err(CompressionError::GZipError),
            Self::ZLib => decode(ZlibDecoder::new(compressed_data), initial_capacity)
                .map_err(CompressionError::ZlibError),
            Self::LZ4 => decode(
                lz4_java_wrc::Lz4BlockInput::new(compressed_data),
                initial_capacity,
            )
            .map_err(CompressionError::LZ4Error),
            Self::Custom => Err(CompressionError::UnknownCompression),
        }
    }

    const LZ4_COMPRESSION_LEVEL_BASE: u32 = 10;
    fn compress_data(
        self,
        uncompressed_data: &[u8],
        compression_level: u32,
    ) -> Result<Vec<u8>, CompressionError> {
        match self {
            Self::GZip => {
                let mut encoder = GzEncoder::new(
                    uncompressed_data,
                    flate2::Compression::new(compression_level),
                );
                let mut chunk_data = Vec::new();
                encoder
                    .read_to_end(&mut chunk_data)
                    .map_err(CompressionError::GZipError)?;
                Ok(chunk_data)
            }
            Self::ZLib => {
                let mut encoder = ZlibEncoder::new(
                    uncompressed_data,
                    flate2::Compression::new(compression_level),
                );
                let mut chunk_data = Vec::new();
                encoder
                    .read_to_end(&mut chunk_data)
                    .map_err(CompressionError::ZlibError)?;
                Ok(chunk_data)
            }
            Self::LZ4 => {
                let mut compressed_data = Vec::new();
                let block_size = 1 << (Self::LZ4_COMPRESSION_LEVEL_BASE + compression_level);
                let mut encoder = lz4_java_wrc::Lz4BlockOutput::with_context(
                    &mut compressed_data,
                    Context::default(),
                    block_size,
                )
                .map_err(CompressionError::LZ4Error)?;
                encoder
                    .write_all(uncompressed_data)
                    .map_err(CompressionError::LZ4Error)?;
                drop(encoder);
                Ok(compressed_data)
            }
            Self::Custom => Err(CompressionError::UnknownCompression),
        }
    }

    /// Returns Ok when a compression is found otherwise an Err
    #[expect(clippy::result_unit_err)]
    pub const fn from_byte(byte: u8) -> Result<Option<Self>, ()> {
        match byte {
            Self::GZIP_ID => Ok(Some(Self::GZip)),
            Self::ZLIB_ID => Ok(Some(Self::ZLib)),
            // Uncompressed (since a version before 1.15.1)
            Self::NO_COMPRESSION_ID => Ok(None),
            Self::LZ4_ID => Ok(Some(Self::LZ4)),
            Self::CUSTOM_ID => Ok(Some(Self::Custom)),
            // Unknown format
            _ => Err(()),
        }
    }
}

impl From<pumpkin_config::chunk::Compression> for Compression {
    fn from(value: pumpkin_config::chunk::Compression) -> Self {
        // :c
        match value {
            pumpkin_config::chunk::Compression::GZip => Self::GZip,
            pumpkin_config::chunk::Compression::ZLib => Self::ZLib,
            pumpkin_config::chunk::Compression::LZ4 => Self::LZ4,
            pumpkin_config::chunk::Compression::Custom => Self::Custom,
        }
    }
}

impl AnvilChunkData {
    /// Raw size of serialized chunk
    #[inline]
    const fn raw_write_size(&self) -> usize {
        // 4 bytes for the *length* and 1 byte for the *compression* method
        self.compressed_data.len() + 4 + 1
    }

    /// Size of serialized chunk with padding
    #[inline]
    const fn padded_size(&self) -> usize {
        let sector_count = self.sector_count() as usize;
        sector_count * SECTOR_BYTES
    }

    #[inline]
    const fn sector_count(&self) -> u32 {
        let total_size = self.raw_write_size();
        total_size.div_ceil(SECTOR_BYTES) as u32
    }

    fn from_bytes(bytes: Bytes) -> Result<Self, ChunkReadingError> {
        let mut bytes = bytes;
        // Minus one for the compression byte, which the length covers, so
        // anything below one does not describe a chunk at all.
        let declared_length = bytes.get_u32() as usize;
        let Some(length) = declared_length.checked_sub(1) else {
            return Err(ChunkReadingError::ParsingError(
                ChunkParsingError::ErrorDeserializingChunk(
                    "Chunk length does not cover its compression byte".to_string(),
                ),
            ));
        };

        // The compression byte has not been read yet, so the payload can use at
        // most one less than what remains.
        let Some(available) = bytes.len().checked_sub(1) else {
            return Err(ChunkReadingError::ParsingError(
                ChunkParsingError::ErrorDeserializingChunk(
                    "Chunk record is missing its compression byte".to_string(),
                ),
            ));
        };

        if length > available {
            return Err(ChunkReadingError::ParsingError(
                ChunkParsingError::ErrorDeserializingChunk(format!(
                    "Chunk length is greater than available bytes ({length} vs {available})"
                )),
            ));
        }

        let compression_method = bytes.get_u8();
        let compression = Compression::from_byte(compression_method)
            .map_err(|()| ChunkReadingError::Compression(CompressionError::UnknownCompression))?;

        Ok(Self {
            compression,
            // If this has padding, we need to trim it
            compressed_data: bytes.slice(..length),
        })
    }

    async fn write(&self, w: &mut (impl AsyncWrite + Unpin + Send)) -> Result<(), std::io::Error> {
        let padded_size = self.padded_size();

        w.write_u32((self.compressed_data.remaining() + 1) as u32)
            .await?;
        w.write_u8(
            self.compression
                .map_or(Compression::NO_COMPRESSION_ID, |c| c as u8),
        )
        .await?;

        w.write_all(&self.compressed_data).await?;

        let padding_len = padded_size - self.raw_write_size();
        if padding_len > 0 {
            static PADDING: [u8; SECTOR_BYTES] = [0; SECTOR_BYTES];
            w.write_all(&PADDING[..padding_len]).await?;
        }

        Ok(())
    }

    fn to_chunk<S>(&self, pos: Vector2<i32>) -> Result<S, ChunkReadingError>
    where
        S: SingleChunkDataSerializer,
    {
        if let Some(compression) = self.compression {
            let decompress_bytes = compression
                .decompress_data(&self.compressed_data)
                .map_err(ChunkReadingError::Compression)?;

            S::from_bytes(&decompress_bytes.into(), pos)
        } else {
            S::from_bytes(&self.compressed_data, pos)
        }
    }

    fn from_chunk<S>(
        chunk: &S,
        compression: Option<Compression>,
        chunk_config: &AnvilChunkConfig,
    ) -> Result<Self, ChunkWritingError>
    where
        S: SingleChunkDataSerializer,
    {
        let raw_bytes = chunk
            .to_bytes()
            .map_err(|err| ChunkWritingError::ChunkSerializingError(err.to_string()))?;

        let compression = compression.unwrap_or_else(|| chunk_config.compression.algorithm.into());
        let level = chunk_config.compression.level;
        let compressed_data = compression
            .compress_data(&raw_bytes, level)
            .map_err(ChunkWritingError::Compression)?;

        Ok(Self {
            compression: Some(compression),
            compressed_data: compressed_data.into(),
        })
    }
}

impl<S: SingleChunkDataSerializer> AnvilChunkFile<S> {
    #[must_use]
    pub const fn get_region_coords(at: &Vector2<i32>) -> (i32, i32) {
        // Divide by 32 for the region coordinates
        (at.x >> SUBREGION_BITS, at.y >> SUBREGION_BITS)
    }

    #[must_use]
    pub const fn get_chunk_index(x: i32, z: i32) -> usize {
        let local_x = x & SUBREGION_AND;
        let local_z = z & SUBREGION_AND;
        let index = (local_z << SUBREGION_BITS) + local_x;
        index as usize
    }

    /// Vanilla `RegionFile.write`, one chunk at a time: the chunk goes to newly allocated
    /// sectors, its header entry is pointed at them, and only then are the old sectors freed.
    /// An interrupted write leaves the old copy in place, and the file never holds more than
    /// one stale chunk.
    async fn write_indices(
        &self,
        path: &Path,
        layout: &mut RegionLayout,
        indices: &[usize],
    ) -> Result<(), std::io::Error> {
        trace!("Writing in place: {}", path.display());

        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .append(false)
            .open(path)
            .await?;
        let mut write = BufWriter::new(file);

        for &index in indices {
            let Some(chunk) = self.chunks_data[index].as_ref() else {
                continue;
            };
            let sector_count = chunk.serialized_data.sector_count();
            let sector_offset = layout.used_sectors.allocate(sector_count);
            trace!("Writing chunk {index} - {sector_offset}:{sector_count}");

            let _ = write
                .seek(SeekFrom::Start(
                    u64::from(sector_offset) * SECTOR_BYTES as u64,
                ))
                .await?;
            chunk.serialized_data.write(&mut write).await?;

            let location = (sector_offset << 8) | sector_count;
            let _ = write.seek(SeekFrom::Start(index as u64 * 4)).await?;
            write.write_u32(location).await?;
            let _ = write
                .seek(SeekFrom::Start((SECTOR_BYTES + index * 4) as u64))
                .await?;
            write.write_u32(chunk.timestamp).await?;
            write.flush().await?;

            let old_location = std::mem::replace(&mut layout.locations[index], location);
            if old_location != 0 {
                layout
                    .used_sectors
                    .free(old_location >> 8, old_location & 0xFF);
            }
        }

        Ok(())
    }

    /// Write entire file, disregarding saved offsets
    async fn write_all(
        &self,
        path: &Path,
        layout: &mut RegionLayout,
    ) -> Result<(), std::io::Error> {
        let temp_path = path.with_extension("tmp");
        trace!("Writing tmp file to disk: {temp_path:?}");

        let file = tokio::fs::File::create(&temp_path).await?;
        let mut write = BufWriter::new(file);

        // Build the 8 KiB header in memory
        let mut header = Vec::with_capacity(SECTOR_BYTES * 2);
        let mut current_sector: u32 = 2;

        // Location Table
        for metadata in &self.chunks_data {
            if let Some(chunk) = metadata {
                let sector_count = chunk.serialized_data.sector_count();
                header.put_u32((current_sector << 8) | sector_count);
                current_sector += sector_count;
            } else {
                header.put_u32(0);
            }
        }

        // Timestamp Table
        for metadata in &self.chunks_data {
            if let Some(chunk) = metadata {
                header.put_u32(chunk.timestamp);
            } else {
                header.put_u32(0);
            }
        }

        // Write all 8 KiB in a single async call
        write.write_all(&header).await?;

        // Write chunk data
        for chunk in self.chunks_data.iter().flatten() {
            chunk.serialized_data.write(&mut write).await?;
        }

        write.flush().await?;
        tokio::fs::rename(temp_path, path).await?;

        // The chunks are now packed in index order right after the header.
        *layout = RegionLayout::new();
        for (index, chunk) in self.chunks_data.iter().enumerate() {
            if let Some(chunk) = chunk {
                let sector_count = chunk.serialized_data.sector_count();
                let sector_offset = layout.used_sectors.allocate(sector_count);
                layout.locations[index] = (sector_offset << 8) | sector_count;
            }
        }
        Ok(())
    }
}

#[expect(clippy::large_stack_arrays)]
impl<S: SingleChunkDataSerializer> Default for AnvilChunkFile<S> {
    fn default() -> Self {
        Self {
            chunks_data: [const { None }; CHUNK_COUNT],
            layout: Mutex::new(RegionLayout::new()),
            _dummy: PhantomData,
        }
    }
}

pub trait SingleChunkDataSerializer: Send + Sync + Sized + Dirtiable + 'static {
    fn to_bytes(&self) -> Result<Bytes, ChunkSerializingError>;
    fn from_bytes(bytes: &Bytes, pos: Vector2<i32>) -> Result<Self, ChunkReadingError>;
    fn position(&self) -> (i32, i32);
}

impl<S: SingleChunkDataSerializer + 'static> ChunkSerializer for AnvilChunkFile<S> {
    type Data = S;
    type WriteBackend = PathBuf;

    type ChunkConfig = AnvilChunkConfig;

    fn should_write(&self, is_watched: bool) -> bool {
        !is_watched
    }

    fn get_chunk_key(chunk: &Vector2<i32>) -> String {
        let (region_x, region_z) = Self::get_region_coords(chunk);
        format!("./r.{region_x}.{region_z}.mca")
    }

    fn has_unwritten_changes(&self) -> bool {
        // A held lock means a write is in progress.
        self.layout.try_lock().map_or(true, |layout| {
            !matches!(layout.write_action, WriteAction::Pass)
        })
    }

    async fn write(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        let mut layout = self.layout.lock().await;
        let write_action = std::mem::replace(&mut layout.write_action, WriteAction::Pass);
        let result = match &write_action {
            WriteAction::Pass => {
                debug!(
                    "Skipping write for {}, as there were no dirty chunks",
                    path.display()
                );
                Ok(())
            }
            WriteAction::All => self.write_all(path, &mut layout).await,
            WriteAction::Parts(parts) => self.write_indices(path, &mut layout, parts).await,
        };

        // Keep what failed to write so the next flush retries it.
        if result.is_err() {
            layout.write_action = write_action;
        }
        result
    }

    fn read(r: Bytes) -> Result<Self, ChunkReadingError> {
        let mut raw_file_bytes = r;

        if raw_file_bytes.is_empty() {
            return Ok(Self::default());
        }

        if raw_file_bytes.len() < SECTOR_BYTES * 2 {
            return Err(ChunkReadingError::InvalidHeader);
        }

        let headers = raw_file_bytes.split_to(SECTOR_BYTES * 2);
        let (mut location_bytes, mut timestamp_bytes) = headers.split_at(SECTOR_BYTES);

        let mut chunk_file = Self::default();

        for i in 0..CHUNK_COUNT {
            let timestamp = timestamp_bytes.get_u32();
            let location = location_bytes.get_u32();

            let sector_count = (location & 0xFF) as usize;
            let sector_offset = (location >> 8) as usize;

            // If the sector offset or count is 0, the chunk is not present (we should not parse empty chunks).
            // Sector 1 is the timestamp table, so a chunk cannot start there either.
            if sector_offset < 2 || sector_count == 0 {
                continue;
            }

            // We always subtract 2 for the first two sectors for the timestamp and location tables
            // that we walked earlier
            let bytes_offset = (sector_offset - 2) * SECTOR_BYTES;
            let bytes_count = sector_count * SECTOR_BYTES;

            if bytes_offset >= raw_file_bytes.len() {
                return Err(ChunkReadingError::ParsingError(
                    ChunkParsingError::ErrorDeserializingChunk(format!(
                        "Chunk {i} starts past the end of the region file"
                    )),
                ));
            }
            // Since 26.3 vanilla no longer pads the file to a whole sector, so the last chunk can
            // end inside its final sector. Like `RegionFile.getChunkDataInputStream`, read what is
            // there and let the declared length decide whether the chunk is complete.
            let bytes_end = (bytes_offset + bytes_count).min(raw_file_bytes.len());

            let serialized_data =
                AnvilChunkData::from_bytes(raw_file_bytes.slice(bytes_offset..bytes_end))?;

            let layout = chunk_file.layout.get_mut();
            layout
                .used_sectors
                .force(sector_offset as u32, sector_count as u32);
            layout.locations[i] = location;
            chunk_file.chunks_data[i] = Some(AnvilChunkMetadata {
                serialized_data,
                timestamp,
            });
        }

        Ok(chunk_file)
    }

    async fn update_chunk(
        &mut self,
        chunk: Arc<Self::Data>,
        chunk_config: &Self::ChunkConfig,
    ) -> Result<(), ChunkWritingError> {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;

        let index = Self::get_chunk_index(chunk.position().0, chunk.position().1);
        // Default to the compression type read from the file
        let compression_type = self.chunks_data[index]
            .as_ref()
            .and_then(|chunk_data| chunk_data.serialized_data.compression);
        let chunk_config_snapshot = chunk_config.clone();
        let new_chunk_data = run_blocking(move || {
            AnvilChunkData::from_chunk(&*chunk, compression_type, &chunk_config_snapshot)
        })
        .await
        .map_err(|_| {
            ChunkWritingError::IoError(std::io::Error::other("chunk serialization task failed"))
        })??;

        let write_action = &mut self.layout.get_mut().write_action;
        if chunk_config.write_in_place {
            write_action.maybe_update_chunk_index(index);
        } else {
            *write_action = WriteAction::All;
        }
        self.chunks_data[index] = Some(AnvilChunkMetadata {
            serialized_data: new_chunk_data,
            timestamp: epoch,
        });

        Ok(())
    }

    async fn get_chunks(
        &self,
        chunks: Vec<Vector2<i32>>,
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) {
        let chunk_items: Vec<(Vector2<i32>, Option<AnvilChunkData>)> = chunks
            .into_iter()
            .map(|chunk| {
                let index = Self::get_chunk_index(chunk.x, chunk.y);
                let data = self.chunks_data[index]
                    .as_ref()
                    .map(|chunk_metadata| chunk_metadata.serialized_data.clone());
                (chunk, data)
            })
            .collect();

        let (tx, mut rx) = tokio::sync::mpsc::channel(chunk_items.len().max(1));

        rayon::spawn(move || {
            use rayon::prelude::*;
            chunk_items
                .into_par_iter()
                .for_each(|(chunk, serialized_data)| {
                    let result = serialized_data.map_or_else(
                        || LoadedData::Missing(chunk),
                        |data| match data.to_chunk(chunk) {
                            Ok(chunk_res) => LoadedData::Loaded(chunk_res),
                            Err(err) => LoadedData::Error((chunk, err)),
                        },
                    );
                    let _ = tx.blocking_send(result);
                });
        });

        while let Some(item) = rx.recv().await {
            if stream.send(item).await.is_err() {
                return;
            }
        }
    }
}

/*
#[cfg(test)]
mod tests {

    use pumpkin_config::{AdvancedConfiguration, advanced_config, override_config_for_testing};
    use pumpkin_data::BlockDirection;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector2::Vector2;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use temp_dir::TempDir;
    use tokio::sync::RwLock;

    use crate::chunk::ChunkData;
    use crate::chunk::format::anvil::{AnvilChunkFile, SingleChunkDataSerializer};
    use crate::chunk::io::file_manager::{ChunkFileManager, PathFromLevelFolder};
    use crate::chunk::io::{FileIO, LoadedData};
    use crate::dimension::Dimension;
    use crate::generation::{Seed, get_world_gen};
    use crate::level::{Level, LevelFolder, SyncChunk};
    use crate::world::{BlockAccessor, BlockRegistryExt};

    struct BlockRegistry;

    impl BlockRegistryExt for BlockRegistry {
        fn can_place_at(
            &self,
            _block: &pumpkin_data::Block,
            _block_accessor: &dyn BlockAccessor,
            _block_pos: &BlockPos,
            _face: BlockDirection,
        ) -> bool {
            true
        }
    }

    async fn get_chunks<S>(
        saver: &ChunkFileManager<AnvilChunkFile<S>>,
        folder: &LevelFolder,
        chunks: &[(Vector2<i32>, SyncChunk)],
    ) -> Box<[Arc<RwLock<S>>]>
    where
        S: SingleChunkDataSerializer + PathFromLevelFolder + 'static,
    {
        let mut read_chunks = Vec::new();
        let (send, mut recv) = tokio::sync::mpsc::channel(1);

        let chunk_pos = chunks.iter().map(|(at, _)| *at).collect::<Vec<_>>();
        let spawn = saver.fetch_chunks(folder, &chunk_pos, send);
        let collect = async {
            while let Some(data) = recv.recv().await {
                read_chunks.push(data);
            }
        };

        tokio::join!(spawn, collect);

        let read_chunks = read_chunks
            .into_iter()
            .map(|chunk| match chunk {
                LoadedData::Loaded(chunk) => chunk,
                LoadedData::Missing(_) => panic!("Missing chunk"),
                LoadedData::Error((position, error)) => {
                    panic!("Error reading chunk at {position:?} | Error: {error:?}")
                }
            })
            .collect::<Vec<_>>();

        read_chunks.into_boxed_slice()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_existing() {
        let region_path = PathBuf::from("not_existing");
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();

        let mut chunks = Vec::new();
        let (send, mut recv) = tokio::sync::mpsc::channel(1);

        chunk_saver
            .fetch_chunks(
                &LevelFolder {
                    root_folder: PathBuf::from(""),
                    region_folder: region_path,
                    entities_folder: PathBuf::from(""),
                },
                &[Vector2::new(0, 0)],
                send,
            )
            .await;

        while let Some(data) = recv.recv().await {
            chunks.push(data);
        }

        assert!(chunks.len() == 1 && matches!(chunks[0], LoadedData::Missing(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn write_in_place() {
        let mut config = AdvancedConfiguration::default();
        config.chunk.write_in_place = true;
        override_config_for_testing(config);
        assert!(advanced_config().chunk.write_in_place);

        let _ = env_logger::try_init();

        let generator = get_world_gen(Seed(0), Dimension::Overworld, false, Vec::new(), String::new());

        let temp_dir = TempDir::new().unwrap();
        let level_folder = LevelFolder {
            root_folder: temp_dir.path().to_path_buf(),
            region_folder: temp_dir.path().join("region"),
            entities_folder: PathBuf::from("entities"),
        };
        fs::create_dir(&level_folder.region_folder).expect("couldn't create region folder");
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let block_registry = Arc::new(BlockRegistry);

        // Generate chunks
        let mut chunks = vec![];
        let level = Arc::new(Level::from_root_folder(
            temp_dir.path().to_path_buf(),
            block_registry.clone(),
            0,
            Dimension::Overworld,
        ));
        for x in -5..5 {
            for y in -5..5 {
                let position = Vector2::new(x, y);
                let chunk = generator.generate_chunk(&level, block_registry.as_ref(), &position);
                chunks.push((position, Arc::new(RwLock::new(chunk))));
            }
        }

        // TEST APPEND TO END

        chunk_saver
            .save_chunks(&level_folder, chunks.clone())
            .await
            .expect("Failed to write chunk");

        // Create a new manager to ensure nothing is cached
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let read_chunks = get_chunks(&chunk_saver, &level_folder, &chunks).await;

        for (_, chunk) in &chunks {
            let chunk = chunk.read().await;
            for read_chunk in read_chunks.iter() {
                let read_chunk = read_chunk.read().await;
                if read_chunk.position == chunk.position {
                    let original = chunk.section.dump_blocks();
                    let read = read_chunk.section.dump_blocks();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    let original = chunk.section.dump_biomes();
                    let read = read_chunk.section.dump_biomes();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });
                    break;
                }
            }
        }

        // TEST WRITE IN PLACE

        // Idk what blocks these are, they just have to be different
        let mut chunk = chunks.first().unwrap().1.write().await;
        chunk.section.set_relative_block(0, 0, 0, 1000);
        // Mark dirty so we actually write it
        chunk.dirty = true;
        drop(chunk);
        let mut chunk = chunks.last().unwrap().1.write().await;
        chunk.section.set_relative_block(0, 0, 0, 1000);
        // Mark dirty so we actually write it
        chunk.dirty = true;
        drop(chunk);

        chunk_saver
            .save_chunks(&level_folder, chunks.clone())
            .await
            .expect("Failed to write chunk");

        // Create a new manager to ensure nothing is cached
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let read_chunks = get_chunks(&chunk_saver, &level_folder, &chunks).await;

        for (_, chunk) in &chunks {
            let chunk = chunk.read().await;
            for read_chunk in read_chunks.iter() {
                let read_chunk = read_chunk.read().await;
                if read_chunk.position == chunk.position {
                    let original = chunk.section.dump_blocks();
                    let read = read_chunk.section.dump_blocks();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    let original = chunk.section.dump_biomes();
                    let read = read_chunk.section.dump_biomes();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    break;
                }
            }
        }

        // TEST SWAP SHIFT

        // Make a big chunk
        let mut chunk = chunks.first().unwrap().1.write().await;
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..4 {
                    let block_id = 16 * 16 * y + 16 * z + x;
                    chunk.section.set_relative_block(x, y, z, block_id as u16);
                }
            }
        }
        // Mark dirty so we actually write it
        chunk.dirty = true;
        drop(chunk);
        let mut chunk = chunks[2].1.write().await;
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..4 {
                    let block_id = 16 * 16 * y + 16 * z + x;
                    chunk.section.set_relative_block(x, y, z, block_id as u16);
                }
            }
        }
        // Mark dirty so we actually write it
        chunk.dirty = true;
        drop(chunk);

        chunk_saver
            .save_chunks(&level_folder, chunks.clone())
            .await
            .expect("Failed to write chunk");

        // Create a new manager to ensure nothing is cached
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let read_chunks = get_chunks(&chunk_saver, &level_folder, &chunks).await;

        for (_, chunk) in &chunks {
            let chunk = chunk.read().await;
            for read_chunk in read_chunks.iter() {
                let read_chunk = read_chunk.read().await;
                if read_chunk.position == chunk.position {
                    let original = chunk.section.dump_blocks();
                    let read = read_chunk.section.dump_blocks();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    let original = chunk.section.dump_biomes();
                    let read = read_chunk.section.dump_biomes();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    break;
                }
            }
        }

        // TEST DEFAULT TO WRITE ALL

        // Make an even bigger chunk
        let mut chunk = chunks.last().unwrap().1.write().await;
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..16 {
                    let block_id = 16 * 16 * y + 16 * z + x;
                    chunk.section.set_relative_block(x, y, z, block_id as u16);
                }
            }
        }
        // Mark dirty so we actually write it
        chunk.dirty = true;
        drop(chunk);

        chunk_saver
            .save_chunks(&level_folder, chunks.clone())
            .await
            .expect("Failed to write chunk");

        // Create a new manager to ensure nothing is cached
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let read_chunks = get_chunks(&chunk_saver, &level_folder, &chunks).await;

        for (_, chunk) in &chunks {
            let chunk = chunk.read().await;
            for read_chunk in read_chunks.iter() {
                let read_chunk = read_chunk.read().await;
                if read_chunk.position == chunk.position {
                    let original = chunk.section.dump_blocks();
                    let read = read_chunk.section.dump_blocks();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });

                    let original = chunk.section.dump_biomes();
                    let read = read_chunk.section.dump_biomes();

                    original
                        .into_iter()
                        .zip(read)
                        .enumerate()
                        .for_each(|(i, (o, r))| {
                            if o != r {
                                panic!("Data miss-match expected {o}, got {r} ({i})");
                            }
                        });
                    break;
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn write_bulk() {
        let mut config = AdvancedConfiguration::default();
        config.chunk.write_in_place = false;
        override_config_for_testing(config);
        assert!(!advanced_config().chunk.write_in_place);

        let _ = env_logger::try_init();

        let generator = get_world_gen(Seed(0), Dimension::Overworld, false, Vec::new(), String::new());

        let temp_dir = TempDir::new().unwrap();
        let level_folder = LevelFolder {
            root_folder: temp_dir.path().to_path_buf(),
            region_folder: temp_dir.path().join("region"),
            entities_folder: PathBuf::from("entities"),
        };
        fs::create_dir(&level_folder.region_folder).expect("couldn't create region folder");
        let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
        let block_registry = Arc::new(BlockRegistry);

        // Generate chunks
        let mut chunks = vec![];
        let level = Arc::new(Level::from_root_folder(
            temp_dir.path().to_path_buf(),
            block_registry.clone(),
            0,
            Dimension::Overworld,
        ));
        for x in -5..5 {
            for y in -5..5 {
                let position = Vector2::new(x, y);
                let chunk = generator.generate_chunk(&level, block_registry.as_ref(), &position);
                chunks.push((position, Arc::new(RwLock::new(chunk))));
            }
        }

        for _ in 0..5 {
            // Mark the chunks as dirty so we save them again
            for (_, chunk) in &chunks {
                let mut chunk = chunk.write().await;
                chunk.dirty = true;
            }

            chunk_saver
                .save_chunks(&level_folder, chunks.clone())
                .await
                .expect("Failed to write chunk");

            // Create a new manager to ensure nothing is cached
            let chunk_saver = ChunkFileManager::<AnvilChunkFile<ChunkData>>::default();
            let read_chunks = get_chunks(&chunk_saver, &level_folder, &chunks).await;

            for (_, chunk) in &chunks {
                let chunk = chunk.read().await;
                for read_chunk in read_chunks.iter() {
                    let read_chunk = read_chunk.read().await;
                    if read_chunk.position == chunk.position {
                        let original = chunk.section.dump_blocks();
                        let read = read_chunk.section.dump_blocks();

                        original
                            .into_iter()
                            .zip(read)
                            .enumerate()
                            .for_each(|(i, (o, r))| {
                                if o != r {
                                    panic!("Data miss-match expected {o}, got {r} ({i})");
                                }
                            });

                        let original = chunk.section.dump_biomes();
                        let read = read_chunk.section.dump_biomes();

                        original
                            .into_iter()
                            .zip(read)
                            .enumerate()
                            .for_each(|(i, (o, r))| {
                                if o != r {
                                    panic!("Data miss-match expected {o}, got {r} ({i})");
                                }
                            });
                        break;
                    }
                }
            }
        }
    }

    // TODO
    /*
    #[test]
    fn load_java_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let level_folder = LevelFolder {
            root_folder: temp_dir.path().to_path_buf(),
            region_folder: temp_dir.path().join("region"),
        };

        fs::create_dir(&level_folder.region_folder).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(file!())
                .parent()
                .unwrap()
                .join("../../assets/r.0.0.mca"),
            level_folder.region_folder.join("r.0.0.mca"),
        )
        .unwrap();

        let mut actually_tested = false;
        for x in 0..(1 << 5) {
            for z in 0..(1 << 5) {
                let result = AnvilChunkFormat {}.read_chunk(&level_folder, &Vector2 { x, z });

                match result {
                    Ok(_) => actually_tested = true,
                    Err(ChunkReadingError::ParsingError(ChunkParsingError::ChunkNotGenerated)) => {}
                    Err(ChunkReadingError::ChunkNotExist) => {}
                    Err(e) => panic!("{:?}", e),
                }

                println!("=========== OK ===========");
            }
        }

        assert!(actually_tested);
    }
    */
}
 */
#[cfg(test)]
mod tests {
    use super::{
        AnvilChunkFile, Compression, CompressionError, SECTOR_BYTES, SingleChunkDataSerializer,
        WriteAction,
    };
    use crate::chunk::io::{ChunkSerializer, Dirtiable};
    use crate::chunk::{ChunkData, ChunkReadingError, ChunkSerializingError};
    use bytes::{BufMut, Bytes, BytesMut};
    use pumpkin_config::chunk::{AnvilChunkConfig, ChunkCompression};
    use pumpkin_util::math::vector2::Vector2;
    use std::sync::Arc;

    /// A chunk that serializes to `sectors` sectors of `fill` bytes.
    struct SizedChunk {
        x: i32,
        sectors: usize,
        fill: u8,
    }

    impl Dirtiable for SizedChunk {
        fn is_dirty(&self) -> bool {
            true
        }

        fn mark_dirty(&self, _flag: bool) {}
    }

    impl SingleChunkDataSerializer for SizedChunk {
        fn to_bytes(&self) -> Result<Bytes, ChunkSerializingError> {
            // Leaves room for the record header and the stored deflate framing.
            Ok(Bytes::from(vec![
                self.fill;
                self.sectors * SECTOR_BYTES
                    - SECTOR_BYTES / 2
            ]))
        }

        fn from_bytes(_bytes: &Bytes, _pos: Vector2<i32>) -> Result<Self, ChunkReadingError> {
            Err(ChunkReadingError::ChunkNotExist)
        }

        fn position(&self) -> (i32, i32) {
            (self.x, 0)
        }
    }

    fn in_place_config() -> AnvilChunkConfig {
        AnvilChunkConfig {
            compression: ChunkCompression {
                algorithm: pumpkin_config::chunk::Compression::ZLib,
                level: 0,
            },
            write_in_place: true,
        }
    }

    async fn save(file: &mut AnvilChunkFile<SizedChunk>, x: i32, sectors: usize, fill: u8) {
        file.update_chunk(
            Arc::new(SizedChunk { x, sectors, fill }),
            &in_place_config(),
        )
        .await
        .expect("chunk serializes");
    }

    /// Reads the region back and checks every chunk on disk holds what was saved last.
    fn read_back(
        file: &AnvilChunkFile<SizedChunk>,
        path: &std::path::Path,
    ) -> AnvilChunkFile<SizedChunk> {
        let on_disk = AnvilChunkFile::<SizedChunk>::read(Bytes::from(
            std::fs::read(path).expect("region on disk"),
        ))
        .expect("valid region");
        for (index, chunk) in file.chunks_data.iter().enumerate() {
            assert_eq!(
                chunk.as_ref().map(|c| &c.serialized_data.compressed_data),
                on_disk.chunks_data[index]
                    .as_ref()
                    .map(|c| &c.serialized_data.compressed_data),
                "chunk {index}"
            );
        }
        on_disk
    }

    fn offset_of(file: &mut AnvilChunkFile<SizedChunk>, index: usize) -> u32 {
        file.layout.get_mut().locations[index] >> 8
    }

    #[tokio::test]
    async fn a_grown_chunk_leaves_the_other_chunks_in_place() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("r.0.0.mca");
        let mut file = AnvilChunkFile::<SizedChunk>::default();
        save(&mut file, 0, 1, 1).await;
        save(&mut file, 1, 2, 2).await;
        file.write(&path).await.expect("first flush");

        // No other chunk has chunk 0's old size, which used to force a full rewrite.
        save(&mut file, 0, 3, 3).await;
        file.write(&path).await.expect("second flush");

        let mut on_disk = read_back(&file, &path);
        assert_eq!(offset_of(&mut on_disk, 1), 3);
    }

    #[tokio::test]
    async fn rewriting_a_whole_region_reuses_the_sectors_it_frees() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("r.0.0.mca");
        let mut file = AnvilChunkFile::<SizedChunk>::default();
        for x in 0..4 {
            save(&mut file, x, 1, 1).await;
        }
        file.write(&path).await.expect("first flush");

        for x in 0..4 {
            save(&mut file, x, 1, 2).await;
        }
        file.write(&path).await.expect("second flush");

        read_back(&file, &path);
        // The header, the four chunks and one sector for the first chunk's new copy.
        let file_sectors = std::fs::metadata(&path).expect("region on disk").len() / 4096;
        assert_eq!(file_sectors, 2 + 4 + 1);
    }

    /// A region file whose first location entry is `location`, whose other
    /// 1023 entries are absent, and whose single payload sector starts with
    /// `declared_len` followed by the "no compression" marker.
    fn region_with_first_location(location: u32, declared_len: u32) -> Bytes {
        let mut buf = BytesMut::with_capacity(SECTOR_BYTES * 3);
        buf.put_u32(location);
        buf.put_bytes(0, SECTOR_BYTES - 4);
        buf.put_bytes(0, SECTOR_BYTES);

        buf.put_u32(declared_len);
        buf.put_u8(3);
        buf.put_bytes(0, SECTOR_BYTES - 5);

        buf.freeze()
    }

    #[test]
    fn a_chunk_pointing_into_the_header_is_skipped() {
        // Sectors 0 and 1 hold the location and timestamp tables, so a chunk
        // cannot start before sector 2. Offset 0 is already skipped; offset 1
        // is just as impossible and used to be subtracted from anyway.
        let file = AnvilChunkFile::<ChunkData>::read(region_with_first_location((1 << 8) | 1, 1))
            .expect("one bad location entry should not fail the whole region");

        assert!(file.chunks_data[0].is_none());
    }

    #[test]
    fn a_chunk_at_the_first_free_sector_is_read() {
        // The control for the test above: sector 2 is the first legal one, and
        // a chunk there must not be skipped.
        let file = AnvilChunkFile::<ChunkData>::read(region_with_first_location((2 << 8) | 1, 1))
            .expect("a chunk at the first free sector is well formed");

        assert!(file.chunks_data[0].is_some());
    }

    #[test]
    fn a_last_chunk_that_ends_inside_its_sector_is_read() {
        // 26.3 stops padding region files, so the last chunk's final sector can be cut short.
        let mut region = BytesMut::from(&region_with_first_location((2 << 8) | 1, 1)[..]);
        region.truncate(SECTOR_BYTES * 2 + 5);
        let file = AnvilChunkFile::<ChunkData>::read(region.freeze())
            .expect("a chunk whose declared length fits is complete");

        assert!(file.chunks_data[0].is_some());
    }

    #[test]
    fn a_chunk_declaring_no_length_is_an_error_not_a_panic() {
        // The length covers the compression byte, so the smallest legal value
        // is 1 and the byte count is that minus one. Zero came from a file, so
        // it has to be rejected rather than subtracted from.
        let file = AnvilChunkFile::<ChunkData>::read(region_with_first_location((2 << 8) | 1, 0));

        assert!(file.is_err());
    }

    #[test]
    fn a_chunk_declaring_one_byte_too_many_is_an_error_not_a_panic() {
        // The length is checked while the compression byte is still unread, so
        // a chunk claiming exactly the bytes that remain used to pass the check
        // and then slice one byte past the end.
        let file = AnvilChunkFile::<ChunkData>::read(region_with_first_location(
            (2 << 8) | 1,
            SECTOR_BYTES as u32 - 3,
        ));

        assert!(file.is_err());
    }

    #[test]
    fn custom_compression_returns_unknown_compression_error() {
        assert!(matches!(
            Compression::Custom.compress_data(b"chunk data", 6),
            Err(CompressionError::UnknownCompression)
        ));
    }

    #[test]
    fn custom_decompression_returns_unknown_compression_error() {
        assert!(matches!(
            Compression::Custom.decompress_data(b"chunk data"),
            Err(CompressionError::UnknownCompression)
        ));
    }

    #[tokio::test]
    async fn offsets_match_the_file_after_a_full_rewrite() {
        // Chunk 0 is stored after chunk 1; the full rewrite packs them in index order.
        let mut buf = BytesMut::with_capacity(SECTOR_BYTES * 4);
        buf.put_u32((3 << 8) | 1);
        buf.put_u32((2 << 8) | 1);
        buf.put_bytes(0, SECTOR_BYTES - 8);
        buf.put_bytes(0, SECTOR_BYTES);
        for _ in 0..2 {
            buf.put_u32(2);
            buf.put_u8(3);
            buf.put_u8(0);
            buf.put_bytes(0, SECTOR_BYTES - 6);
        }
        let mut file = AnvilChunkFile::<ChunkData>::read(buf.freeze()).expect("valid region");
        file.layout.get_mut().write_action = WriteAction::All;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("r.0.0.mca");
        file.write(&path).await.expect("full rewrite");

        let written = std::fs::read(&path).expect("region on disk");
        let mut on_disk =
            AnvilChunkFile::<ChunkData>::read(Bytes::from(written)).expect("valid region");
        let (cached, stored) = (file.layout.get_mut(), on_disk.layout.get_mut());
        assert_eq!(cached.locations, stored.locations);
        assert_eq!(cached.used_sectors, stored.used_sectors);
    }
}
