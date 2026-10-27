use pumpkin_data::{Block, dimension::Dimension};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::world_seed::Seed;

use crate::generation::{get_world_gen, proto_chunk::ProtoChunk};

/// Sculk patches place catalysts over sensors; a catalyst left with the sensor's block entity data
/// never got its own block entity.
#[test]
fn a_block_entity_block_replaces_another_kinds_data() {
    let world_gen = get_world_gen(
        Seed(0),
        Dimension::OVERWORLD,
        false,
        Vec::new(),
        String::new(),
    );
    let mut proto = ProtoChunk::new(0, 0, &world_gen);

    proto.set_block_state(1, 10, 1, Block::SCULK_SENSOR.default_state);
    proto.set_block_state(1, 10, 1, Block::SCULK_CATALYST.default_state);

    let ids: Vec<_> = proto
        .pending_block_entities
        .iter()
        .filter_map(|nbt| nbt.get_string("id"))
        .collect();
    assert_eq!(ids, ["minecraft:sculk_catalyst"]);

    // Data a structure gave a chest stays when the chest is set again.
    proto.set_block_state(2, 10, 1, Block::CHEST.default_state);
    let mut chest = NbtCompound::new();
    chest.put_string("id", "minecraft:chest".to_string());
    chest.put_int("x", 2);
    chest.put_int("y", 10);
    chest.put_int("z", 1);
    chest.put_string("LootTable", "minecraft:chests/simple_dungeon".to_string());
    proto.add_block_entity(chest);
    proto.set_block_state(2, 10, 1, Block::CHEST.default_state);
    assert!(
        proto
            .pending_block_entities
            .iter()
            .any(|nbt| nbt.get_string("LootTable").is_some())
    );
}
