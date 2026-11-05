#!/usr/bin/env python3
"""Tests for world_diff.py on small worlds written with nbt.py.

    python3 tools/world-roundtrip/test_world_diff.py
"""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nbt  # noqa: E402
import world_diff  # noqa: E402
from nbt import Byte, Compound, Int, IntArray, List, LongArray, String  # noqa: E402

REGION = "dimensions/minecraft/overworld/region/r.0.0.mca"
ENTITIES = "dimensions/minecraft/overworld/entities/r.0.0.mca"


def chest(count):
    stack = Compound(id=String("minecraft:diamond"), count=Int(count))
    return Compound(id=String("minecraft:chest"), x=Int(5), y=Int(200), z=Int(2),
                    Items=List([Compound(stack, Slot=Byte(0)), Compound(stack, Slot=Byte(1))]))


def section(palette, data=None):
    states = Compound(palette=List(palette))
    if data is not None:
        states["data"] = LongArray(data)
    return Compound(Y=Byte(12), block_states=states, biomes=Compound(palette=List([String("minecraft:plains")])))


def chunk(sections, block_entities):
    return Compound(DataVersion=Int(5023), xPos=Int(0), zPos=Int(0), Status=String("minecraft:full"),
                    sections=List(sections), block_entities=List(block_entities))


def entity(n):
    return Compound(id=String("minecraft:pig"), UUID=IntArray([n, n, n, n]), Health=nbt.Float(10.0))


# Blocks 1 and 2 of the section use palette entries 1 and 2, every other block entry 0.
# Four bits per block, as vanilla packs a palette of up to 16 entries.
BLOCKS = [0x210] + [0] * 255
STAIRS = {"facing": "east", "half": "bottom", "shape": "straight", "waterlogged": "false"}
LEGACY = [Compound(Name=String("minecraft:stone")),
          Compound(Name=String("minecraft:oak_stairs"), Properties=Compound({k: String(v) for k, v in STAIRS.items()})),
          Compound(Name=String("minecraft:oak_log"), Properties=Compound(axis=String("y")))]
CURRENT = [Compound({"": String("minecraft:stone")}),
           Compound(id=String("minecraft:oak_stairs"), properties=Compound({k: String(v) for k, v in STAIRS.items()})),
           Compound({"": String("minecraft:oak_log")})]


class WorldDiffTest(unittest.TestCase):
    def world(self, sections=None, block_entities=None, entities=None, last_played=1):
        root = tempfile.mkdtemp(dir=self.tmp.name)
        os.makedirs(os.path.join(root, os.path.dirname(REGION)))
        os.makedirs(os.path.join(root, os.path.dirname(ENTITIES)))
        sections = sections if sections is not None else [section(CURRENT, BLOCKS)]
        nbt.write_region(os.path.join(root, REGION), {(0, 0): (7, chunk(sections, block_entities or [chest(1)]))})
        entity_chunk = Compound(DataVersion=Int(5023), Position=IntArray([0, 0]),
                                Entities=List(entities if entities is not None else [entity(1), entity(2)]))
        nbt.write_region(os.path.join(root, ENTITIES), {(0, 0): (7, entity_chunk)})
        nbt.write_file(os.path.join(root, "level.dat"),
                       Compound(Data=Compound(LastPlayed=nbt.Long(last_played), LevelName=String("w"))))
        return root

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self):
        self.tmp.cleanup()

    def diff(self, a, b):
        return world_diff.diff_worlds(a, b)[0]

    def test_copies_match(self):
        self.assertEqual(self.diff(self.world(), self.world()), [])

    def test_one_item_change_is_one_diff_with_its_path(self):
        changed = chest(1)
        changed["Items"][1]["count"] = Int(5)
        diffs = self.diff(self.world(), self.world(block_entities=[changed]))
        self.assertEqual([world_diff.format_diff(d) for d in diffs],
                         [f"{REGION} chunk 0,0 block_entities[{{x:5,y:200,z:2}}].Items[{{Slot:1b}}].count: 1 | 5"])

    def test_entities_match_by_uuid_in_any_order(self):
        self.assertEqual(self.diff(self.world(), self.world(entities=[entity(2), entity(1)])), [])
        diffs = self.diff(self.world(), self.world(entities=[entity(2)]))
        self.assertEqual([(d.file, d.path, d.b) for d in diffs],
                         [(ENTITIES, "Entities[{UUID:[I;1,1,1,1]}]", world_diff.MISSING)])

    def test_legacy_palette_holds_the_same_blocks(self):
        diffs = self.diff(self.world(), self.world(sections=[section(LEGACY, BLOCKS)]))
        self.assertEqual([(d.path, d.a, d.b) for d in diffs],
                         [("sections[].block_states.palette", "bare id, {id, properties}", "{Name, Properties}")])

    def test_changed_block_reports_its_position(self):
        swapped = [CURRENT[1], CURRENT[0], CURRENT[2]]
        diffs = self.diff(self.world(), self.world(sections=[section(swapped, BLOCKS)]))
        self.assertEqual(len(diffs), 1)
        self.assertEqual(diffs[0].path, "sections[{Y:12b}].block_states")
        self.assertTrue(diffs[0].a.startswith("4095 blocks differ, first at 0,192,0: minecraft:stone"), diffs[0].a)

    def test_last_played_is_ignored(self):
        kept, ignored = world_diff.diff_worlds(self.world(), self.world(last_played=2))
        self.assertEqual(kept, [])
        self.assertEqual([d.path for d, _ in ignored], ["Data.LastPlayed"])


if __name__ == "__main__":
    unittest.main()
