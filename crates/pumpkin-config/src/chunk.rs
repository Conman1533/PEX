use std::str;

use serde::{Deserialize, Serialize};

/// Configuration for chunk storage format.
///
/// Supports multiple chunk formats, currently `Anvil` and `Linear`.
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ChunkConfig {
    /// Standard Anvil chunk storage.
    #[serde(rename = "anvil")]
    Anvil(AnvilChunkConfig),
    /// Linear chunk storage format.
    #[serde(rename = "linear")]
    Linear,
    /// Pumpkin's own optimized world format.
    #[serde(rename = "pump")]
    Pump,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self::Anvil(AnvilChunkConfig::default())
    }
}

/// Configuration for Anvil chunk storage.
#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct AnvilChunkConfig {
    /// Compression settings for chunk data.
    pub compression: ChunkCompression,
    /// Whether chunks are written into their sectors in place, like vanilla. When false, every
    /// save batch rewrites the whole region file through a temporary file: crash safe, but with
    /// players walking between regions it wrote about 66 times more data than in place.
    pub write_in_place: bool,
}

impl Default for AnvilChunkConfig {
    fn default() -> Self {
        Self {
            compression: ChunkCompression::default(),
            write_in_place: true,
        }
    }
}

/// Compression settings for chunk data.
#[derive(Deserialize, Serialize, Clone)]
pub struct ChunkCompression {
    /// Compression algorithm to use.
    pub algorithm: Compression,
    /// Compression level (algorithm-specific).
    pub level: u32,
}

impl Default for ChunkCompression {
    fn default() -> Self {
        Self {
            algorithm: Compression::LZ4,
            level: 6,
        }
    }
}

/// Compression algorithm used for chunk data storage.
#[derive(Deserialize, Serialize, Clone, Copy)]
pub enum Compression {
    /// `GZip` Compression.
    GZip,
    /// `ZLib` Compression.
    ZLib,
    /// LZ4 Compression (since 24w04a).
    LZ4,
    /// Custom compression algorithm (since 24w05a).
    Custom,
}
