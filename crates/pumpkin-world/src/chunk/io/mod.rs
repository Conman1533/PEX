use std::{
    error, fmt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::Bytes;
use pumpkin_util::math::vector2::Vector2;

use super::{ChunkReadingError, ChunkWritingError};
use crate::level::LevelFolder;

pub mod file_manager;

pub(crate) async fn run_blocking<T, F>(task: F) -> Result<T, tokio::task::JoinError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(task).await
}

/// The result of loading a chunk data.
///
/// It can be the data loaded successfully, the data not found or an error
/// with the chunk coordinates and the error that occurred.
pub enum LoadedData<D: Send, Err: error::Error> {
    /// The chunk data was loaded successfully
    Loaded(D),

    /// The chunk data was not found
    Missing(Vector2<i32>),

    /// An error occurred while loading the chunk data
    Error((Vector2<i32>, Err)),
}

impl<D: Send, E: error::Error> LoadedData<D, E> {
    pub fn map_loaded<D2: Send>(self, map: impl FnOnce(D) -> D2) -> LoadedData<D2, E> {
        match self {
            Self::Loaded(data) => LoadedData::Loaded(map(data)),
            Self::Missing(pos) => LoadedData::Missing(pos),
            Self::Error(err) => LoadedData::Error(err),
        }
    }
}

pub trait Dirtiable {
    fn is_dirty(&self) -> bool;
    fn mark_dirty(&self, flag: bool);
}

const SAVE_FAILURE_LOG_INTERVAL: Duration = Duration::from_secs(60);

/// Logs a failed save at most once per minute, with a count of the ones in between. When the
/// disk is full every save fails, many times a second.
pub fn log_save_failure(message: fmt::Arguments<'_>) {
    static LAST_LOGGED: Mutex<Option<Instant>> = Mutex::new(None);
    static HELD_BACK: AtomicU64 = AtomicU64::new(0);

    let mut last_logged = LAST_LOGGED.lock().unwrap_or_else(PoisonError::into_inner);
    if last_logged.is_some_and(|at| at.elapsed() < SAVE_FAILURE_LOG_INTERVAL) {
        HELD_BACK.fetch_add(1, Ordering::Relaxed);
        return;
    }
    *last_logged = Some(Instant::now());
    drop(last_logged);

    match HELD_BACK.swap(0, Ordering::Relaxed) {
        0 => tracing::error!("{message}"),
        held_back => tracing::error!("{message} ({held_back} more failed saves since)"),
    }
}

/// Trait to handle the IO of chunks
/// for loading and saving chunks data
/// can be implemented for different types of IO
/// or with different optimizations
///
/// The `R` type is the type of the data that will be loaded/saved
/// like `ChunkData` or `EntityData`
pub trait FileIO
where
    Self: Send + Sync,
{
    type Data: Send + Sync + Sized;

    /// Load the chunks data
    fn fetch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunk_coords: &'a [Vector2<i32>],
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Persist the chunks data
    fn save_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks_data: Vec<(Vector2<i32>, Self::Data)>,
    ) -> impl Future<Output = Result<(), ChunkWritingError>> + Send + 'a;

    /// Tells the `ChunkIO` that these chunks are currently loaded in memory
    fn watch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks: &'a [Vector2<i32>],
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Tells the `ChunkIO` that these chunks are no longer loaded in memory
    fn unwatch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks: &'a [Vector2<i32>],
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Tells the `ChunkIO` that no more chunks are loaded in memory
    fn clear_watched_chunks(&self) -> impl Future<Output = ()> + Send + '_;

    /// Ensure that all ongoing operations are finished
    fn block_and_await_ongoing_tasks(&self) -> impl Future<Output = ()> + Send + '_;
}

/// Trait to serialize and deserialize the chunk data to and from bytes.
///
/// The `Data` type is the type of the data that will be updated or serialized/deserialized
/// like `ChunkData` or `EntityData`
pub trait ChunkSerializer: Send + Sync + Default + 'static {
    type Data: Send + Sync + Sized + Dirtiable;
    type WriteBackend;

    type ChunkConfig;

    /// Get the key for the chunk (like the file name)
    fn get_chunk_key(chunk: &Vector2<i32>) -> String;

    fn should_write(&self, is_watched: bool) -> bool;

    /// Whether changes from [`Self::update_chunk`] haven't reached the backend yet, for
    /// example because the last [`Self::write`] failed.
    fn has_unwritten_changes(&self) -> bool {
        false
    }

    /// Serialize the data to bytes.
    fn write(
        &self,
        backend: &Self::WriteBackend,
    ) -> impl Future<Output = Result<(), std::io::Error>> + Send;

    /// Create a new instance from bytes
    fn read(r: Bytes) -> Result<Self, ChunkReadingError>;

    /// Add the chunk data to the serializer
    fn update_chunk(
        &mut self,
        chunk_data: Arc<Self::Data>,
        chunk_config: &Self::ChunkConfig,
    ) -> impl Future<Output = Result<(), ChunkWritingError>> + Send;

    /// Get the chunks data from the serializer
    fn get_chunks(
        &self,
        chunks: Vec<Vector2<i32>>,
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) -> impl Future<Output = ()> + Send;
}
