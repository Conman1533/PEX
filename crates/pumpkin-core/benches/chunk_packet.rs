#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `CChunkData` encoding: time per chunk, and the bytes each chunk packet takes.
//!
//! Reads every chunk of the Anvil region file named by `PEX_BENCH_REGION`. Use one PEX saved
//! after lighting its chunks, such as a region the gameplay benchmark's bots explored: chunks
//! saved by vanilla or Paper leave the fully lit sections out, so they don't show sky light as
//! PEX sends it.

use std::{hint::black_box, io::Read};

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use flate2::read::ZlibDecoder;
use pumpkin_core::net::java::chunk_data::CChunkData;
use pumpkin_protocol::ClientPacket;
use pumpkin_util::{math::vector2::Vector2, version::JavaMinecraftVersion};
use pumpkin_world::chunk::{ChunkData, format::anvil::SingleChunkDataSerializer};

const SECTOR: usize = 4096;
/// Anvil compression type 2.
const ZLIB: u8 = 2;

/// Every zlib-compressed chunk in the region file, with its position.
fn read_region(path: &str) -> Vec<ChunkData> {
    let file = std::fs::read(path).expect("failed to read the region file");
    let (region_x, region_z) = {
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let mut parts = name.split('.').skip(1);
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        (x, z)
    };
    let mut chunks = Vec::new();
    for index in 0..1024 {
        let entry = &file[index * 4..index * 4 + 4];
        let offset =
            (usize::from(entry[0]) << 16 | usize::from(entry[1]) << 8 | usize::from(entry[2]))
                * SECTOR;
        if offset == 0 {
            continue;
        }
        let length = u32::from_be_bytes(file[offset..offset + 4].try_into().unwrap()) as usize;
        if file[offset + 4] != ZLIB {
            continue;
        }
        let mut nbt = Vec::new();
        ZlibDecoder::new(&file[offset + 5..offset + 4 + length])
            .read_to_end(&mut nbt)
            .expect("failed to decompress a chunk");
        let position = Vector2::new(
            region_x * 32 + (index % 32) as i32,
            region_z * 32 + (index / 32) as i32,
        );
        if let Ok(chunk) = ChunkData::from_bytes(&Bytes::from(nbt), position) {
            chunks.push(chunk);
        }
    }
    chunks
}

fn encode(chunk: &ChunkData, version: JavaMinecraftVersion) -> Vec<u8> {
    let mut buf = Vec::new();
    CChunkData(chunk)
        .write_packet_data(&mut buf, &version)
        .expect("failed to encode a chunk packet");
    buf
}

#[expect(clippy::print_stderr)]
fn bench_chunk_packet(c: &mut Criterion) {
    let Ok(path) = std::env::var("PEX_BENCH_REGION") else {
        eprintln!("chunk_packet: set PEX_BENCH_REGION to an Anvil .mca file; skipping");
        return;
    };
    let chunks = read_region(&path);
    assert!(!chunks.is_empty(), "no chunks in {path}");
    let version = JavaMinecraftVersion::V_26_3;

    let mut sizes: Vec<usize> = chunks
        .iter()
        .map(|chunk| encode(chunk, version).len())
        .collect();
    sizes.sort_unstable();
    let total: usize = sizes.iter().sum();
    eprintln!(
        "chunk_packet: {} chunks, {} bytes uncompressed, mean {} and median {} bytes per chunk",
        chunks.len(),
        total,
        total / chunks.len(),
        sizes[sizes.len() / 2]
    );

    c.bench_function("chunk_packet_encode_region", |b| {
        b.iter(|| {
            for chunk in &chunks {
                black_box(encode(black_box(chunk), version));
            }
        });
    });
}

criterion_group!(benches, bench_chunk_packet);
criterion_main!(benches);
