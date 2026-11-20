//! Structure starts and references in a chunk's `structures` compound, saved like vanilla 26.3.
//!
//! Follows `SerializableChunkData.packStructureData`, `StructureStart.createTag` and
//! `StructurePiece.createTag`. G7-07, D7-3: a start is saved whole, with every piece, and a start
//! with a piece type PEX can't write yet is left out, as before.

use pumpkin_data::structures::StructureKeys;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::block_box::BlockBox;
use rustc_hash::FxHashMap;

use super::structures::{
    StructureInstance, StructurePieceBase,
    jigsaw::{JigsawJunction, JigsawProjection, PoolElementKind, PoolElementStructurePiece},
    jigsaw_placement::LiquidSettings,
};
use crate::generation::structure::template::BlockRotation;

/// `ChunkPos.pack`.
#[must_use]
pub const fn pack_chunk_pos(x: i32, z: i32) -> i64 {
    (x as u32 as i64) | ((z as u32 as i64) << 32)
}

/// The `structures` compound for the chunk at `chunk_x`, `chunk_z`: its starts in `starts` and,
/// for each structure, the start chunks of the starts reaching into it.
#[must_use]
pub fn structures_tag<S: std::hash::BuildHasher>(
    chunk_x: i32,
    chunk_z: i32,
    starts: &std::collections::HashMap<StructureKeys, StructureInstance, S>,
    references: &[(StructureKeys, i32, i32)],
) -> NbtCompound {
    let mut starts_tag = NbtCompound::new();
    let mut keys: Vec<_> = starts.keys().collect();
    keys.sort_by_key(|key| key.to_name());
    for key in keys {
        let StructureInstance::Start(position) = &starts[key] else {
            continue;
        };
        let collector = position
            .collector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let children: Option<Vec<NbtTag>> = collector
            .pieces
            .iter()
            .map(|piece| piece_tag(piece.as_ref()).map(NbtTag::Compound))
            .collect();
        let Some(children) = children else {
            continue;
        };
        if children.is_empty() {
            continue;
        }
        let mut start = NbtCompound::new();
        start.put_string("id", format!("minecraft:{}", key.to_name()));
        start.put_int("ChunkX", chunk_x);
        start.put_int("ChunkZ", chunk_z);
        start.put_int("references", 0);
        start.put_list("Children", children);
        starts_tag.put_compound(&format!("minecraft:{}", key.to_name()), start);
    }

    let mut by_structure: FxHashMap<StructureKeys, Vec<i64>> = FxHashMap::default();
    for &(key, x, z) in references {
        let packed = pack_chunk_pos(x, z);
        let list = by_structure.entry(key).or_default();
        if !list.contains(&packed) {
            list.push(packed);
        }
    }
    let mut references_tag = NbtCompound::new();
    let mut keys: Vec<_> = by_structure.keys().copied().collect();
    keys.sort_by_key(StructureKeys::to_name);
    for key in keys {
        references_tag.put(
            &format!("minecraft:{}", key.to_name()),
            NbtTag::LongArray(by_structure.remove(&key).unwrap_or_default()),
        );
    }

    let mut structures = NbtCompound::new();
    structures.put_compound("starts", starts_tag);
    structures.put_compound("References", references_tag);
    structures
}

/// Adds `ours` to the `structures` a chunk was saved with: a saved start wins over PEX's for the
/// same structure, and the saved references of each structure get PEX's added.
pub fn merge_structures(saved: &mut NbtCompound, mut ours: NbtCompound) {
    if let Some(NbtTag::Compound(our_starts)) = ours.child_tags.remove("starts") {
        let mut starts = match saved.child_tags.remove("starts") {
            Some(NbtTag::Compound(starts)) => starts,
            _ => NbtCompound::new(),
        };
        for (key, start) in our_starts.child_tags {
            if !starts.has(&key) {
                starts.put(&key, start);
            }
        }
        saved.put_compound("starts", starts);
    }
    if let Some(NbtTag::Compound(our_references)) = ours.child_tags.remove("References") {
        let mut references = match saved.child_tags.remove("References") {
            Some(NbtTag::Compound(references)) => references,
            _ => NbtCompound::new(),
        };
        for (key, tag) in our_references.child_tags {
            let NbtTag::LongArray(ours) = tag else {
                continue;
            };
            let mut merged = match references.child_tags.remove(&key) {
                Some(NbtTag::LongArray(saved)) => saved,
                _ => Vec::new(),
            };
            for pos in ours {
                if !merged.contains(&pos) {
                    merged.push(pos);
                }
            }
            references.put(&key, NbtTag::LongArray(merged));
        }
        saved.put_compound("References", references);
    }
}

/// `StructurePiece.createTag`, or `None` for a piece type PEX doesn't write yet.
fn piece_tag(piece: &dyn StructurePieceBase) -> Option<NbtCompound> {
    let jigsaw = piece.as_any().downcast_ref::<PoolElementStructurePiece>()?;
    let mut tag = NbtCompound::new();
    tag.put_string("id", "minecraft:jigsaw".to_string());
    tag.put("BB", bounding_box_tag(&piece.bounding_box()));
    // `PoolElementStructurePiece` has no orientation and a generation depth of 0.
    tag.put_int("O", -1);
    tag.put_int("GD", 0);
    tag.put_int("PosX", jigsaw.pos.0.x);
    tag.put_int("PosY", jigsaw.pos.0.y);
    tag.put_int("PosZ", jigsaw.pos.0.z);
    tag.put_int("ground_level_delta", jigsaw.ground_level_delta);
    tag.put_compound(
        "pool_element",
        pool_element_tag(&jigsaw.element.kind, jigsaw.element.projection),
    );
    tag.put_string("rotation", rotation_name(jigsaw.rotation).to_string());
    tag.put_list(
        "junctions",
        jigsaw
            .junctions
            .iter()
            .map(|junction| NbtTag::Compound(junction_tag(junction)))
            .collect(),
    );
    // Written only when it isn't `JigsawStructure.DEFAULT_LIQUID_SETTINGS`.
    if jigsaw.liquid_settings == LiquidSettings::IgnoreWaterlogDone {
        tag.put_string("liquid_settings", "ignore_waterlogging".to_string());
    }
    Some(tag)
}

/// `BoundingBox.CODEC`.
fn bounding_box_tag(bounding_box: &BlockBox) -> NbtTag {
    NbtTag::IntArray(vec![
        bounding_box.min.x,
        bounding_box.min.y,
        bounding_box.min.z,
        bounding_box.max.x,
        bounding_box.max.y,
        bounding_box.max.z,
    ])
}

/// `StructurePoolElement.CODEC`.
fn pool_element_tag(kind: &PoolElementKind, projection: JigsawProjection) -> NbtCompound {
    let mut tag = NbtCompound::new();
    match kind {
        PoolElementKind::Empty => {
            tag.put_string("element_type", "minecraft:empty_pool_element".to_string());
            return tag;
        }
        PoolElementKind::Single {
            template,
            processors,
            legacy,
            ..
        } => {
            let element_type = if *legacy {
                "minecraft:legacy_single_pool_element"
            } else {
                "minecraft:single_pool_element"
            };
            tag.put_string("element_type", element_type.to_string());
            tag.put_string("location", namespaced(template));
            let processors = match processors {
                super::structures::jigsaw::ProcessorListRef::Named(name) => namespaced(name),
                super::structures::jigsaw::ProcessorListRef::Empty => "minecraft:empty".to_string(),
            };
            tag.put_string("processors", processors);
        }
        PoolElementKind::List(elements) => {
            tag.put_string("element_type", "minecraft:list_pool_element".to_string());
            // `ListPoolElement` gives its own projection to every element.
            tag.put_list(
                "elements",
                elements
                    .iter()
                    .map(|element| NbtTag::Compound(pool_element_tag(element, projection)))
                    .collect(),
            );
        }
        PoolElementKind::Feature(feature) => {
            tag.put_string("element_type", "minecraft:feature_pool_element".to_string());
            tag.put_string("feature", format!("minecraft:{}", feature.to_name()));
        }
    }
    tag.put_string("projection", projection_name(projection).to_string());
    tag
}

/// `JigsawJunction.serialize`.
fn junction_tag(junction: &JigsawJunction) -> NbtCompound {
    let mut tag = NbtCompound::new();
    tag.put_int("source_x", junction.source_x);
    tag.put_int("source_ground_y", junction.source_ground_y);
    tag.put_int("source_z", junction.source_z);
    tag.put_int("delta_y", junction.delta_y);
    tag.put_string(
        "dest_proj",
        projection_name(junction.projection).to_string(),
    );
    tag
}

const fn projection_name(projection: JigsawProjection) -> &'static str {
    match projection {
        JigsawProjection::Rigid => "rigid",
        JigsawProjection::TerrainMatching => "terrain_matching",
    }
}

/// `Rotation.LEGACY_CODEC`: the enum constant's name.
const fn rotation_name(rotation: BlockRotation) -> &'static str {
    match rotation {
        BlockRotation::None => "NONE",
        BlockRotation::Clockwise90 => "CLOCKWISE_90",
        BlockRotation::Rotate180 => "CLOCKWISE_180",
        BlockRotation::CounterClockwise90 => "COUNTERCLOCKWISE_90",
    }
}

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_string()
    } else {
        format!("minecraft:{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_pos_packs_like_vanilla() {
        assert_eq!(pack_chunk_pos(0, 0), 0);
        assert_eq!(pack_chunk_pos(1, 0), 1);
        assert_eq!(pack_chunk_pos(0, 1), 1 << 32);
        assert_eq!(pack_chunk_pos(-1, -1), -1);
        assert_eq!(pack_chunk_pos(-2, 3), (3 << 32) | 0xFFFF_FFFE);
    }

    #[test]
    fn saved_structures_win_and_references_are_merged() {
        let mut saved_start = NbtCompound::new();
        saved_start.put_string("id", "minecraft:mineshaft".to_string());
        let mut saved_starts = NbtCompound::new();
        saved_starts.put_compound("minecraft:mineshaft", saved_start.clone());
        let mut saved_references = NbtCompound::new();
        saved_references.put("minecraft:mineshaft", NbtTag::LongArray(vec![1, 2]));
        let mut saved = NbtCompound::new();
        saved.put_compound("starts", saved_starts);
        saved.put_compound("References", saved_references);

        let mut our_starts = NbtCompound::new();
        our_starts.put_compound("minecraft:mineshaft", NbtCompound::new());
        our_starts.put_compound("minecraft:village_plains", NbtCompound::new());
        let mut our_references = NbtCompound::new();
        our_references.put("minecraft:mineshaft", NbtTag::LongArray(vec![2, 3]));
        our_references.put("minecraft:village_plains", NbtTag::LongArray(vec![4]));
        let mut ours = NbtCompound::new();
        ours.put_compound("starts", our_starts);
        ours.put_compound("References", our_references);

        merge_structures(&mut saved, ours);
        let starts = saved.get_compound("starts").expect("starts");
        assert_eq!(
            starts.get_compound("minecraft:mineshaft"),
            Some(&saved_start)
        );
        assert!(starts.has("minecraft:village_plains"));
        let references = saved.get_compound("References").expect("References");
        assert_eq!(
            references.get("minecraft:mineshaft"),
            Some(&NbtTag::LongArray(vec![1, 2, 3]))
        );
        assert_eq!(
            references.get("minecraft:village_plains"),
            Some(&NbtTag::LongArray(vec![4]))
        );
    }

    #[test]
    fn references_are_grouped_and_deduplicated() {
        let starts = FxHashMap::default();
        let tag = structures_tag(
            0,
            0,
            &starts,
            &[
                (StructureKeys::VillagePlains, 1, 2),
                (StructureKeys::VillagePlains, 1, 2),
                (StructureKeys::VillagePlains, -1, 0),
            ],
        );
        assert!(
            tag.get_compound("starts")
                .is_some_and(|s| s.child_tags.is_empty())
        );
        let references = tag.get_compound("References").expect("References");
        assert_eq!(
            references.get("minecraft:village_plains"),
            Some(&NbtTag::LongArray(vec![
                pack_chunk_pos(1, 2),
                pack_chunk_pos(-1, 0)
            ]))
        );
    }
}
