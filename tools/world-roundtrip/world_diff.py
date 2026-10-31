#!/usr/bin/env python3
"""Compare two world folders file by file and key by key.

    python3 tools/world-roundtrip/world_diff.py <world A> <world B> [--ignore-file FILE]

Chunks in region/, entities/ and poi/ files are matched by chunk position. Inside any
NBT or JSON value, compounds compare key by key and lists of compounds are matched by
what identifies an element: entities and passengers by UUID, block entities and block
ticks by x, y and z, items by Slot (or slot in item components), POI records by pos,
sections by Y, attributes and effects by id. Other lists compare by index. level.dat,
the data/ files, player files, advancements and stats compare this way too; other files
compare by bytes. Region timestamps and how a file lays out its bytes are not compared.

Block states compare by the state each block holds, so a palette entry written as
{Name, Properties} (before data version 5006) matches the bare id or {id, properties}
that 26.3 writes. Bare ids mean the default state, which comes from assets/blocks.json.
A chunk whose palettes use the other form still gets one diff saying so, since 26.3
does not read {Name, Properties} at data version 5023. Biomes compare by the biome each
cell holds.

Each difference prints as `<file> [chunk x,z] <path>: <A> | <B>`, with the path in /data
syntax, for example `block_entities[{x:5,y:200,z:2}].Items[{Slot:0b}].count`. IGNORE
lists values that differ between two saves of the same world, each with a reason;
--ignore-file adds entries, one per line: `<file glob> <path glob> <reason>`, where only
* is special and # starts a comment. `diff_worlds()` returns the differences from Python.

Exit status: 0 when the worlds match outside the ignore list, 1 when they differ, 2 on
usage errors.
"""
import argparse
import collections
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nbt  # noqa: E402

ASSETS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "assets")
MISSING = "<missing>"
SHOW_LIMIT = 200
LONG_ARRAY = 16  # longer int and long arrays (heightmaps) print only their first differing value
IGNORE = [
    ("session.lock", "*", "vanilla writes the lock file again at every start"),
    ("level.dat", "Data.LastPlayed", "wall-clock time of the last save"),
    ("level.dat_old", "*", "level.dat as it was one save earlier, so it depends on how often the server saved"),
    ("players/data/*.dat_old", "*", "the player file as it was one save earlier"),
]
# Fields that identify an element of a list of compounds, tried in order. A field set is
# used when every element on both sides has it and no two elements share its value.
IDENTITY = [("UUID",), ("x", "y", "z"), ("Slot",), ("slot",), ("pos",), ("Y",), ("id",)]
Diff = collections.namedtuple("Diff", "file chunk path a b")


def show(value):
    if value is MISSING:
        return MISSING
    text = nbt.to_snbt(value)
    return text if len(text) <= SHOW_LIMIT else text[:SHOW_LIMIT] + "..."


def unwrap(value):
    # A list mixing tag types stores each non-compound element as {"": value}.
    if isinstance(value, dict) and len(value) == 1 and "" in value:
        return value[""]
    return value


def same(a, b):
    if type(a) is not type(b):
        return False
    return a == b or (isinstance(a, float) and math.isnan(a) and math.isnan(b))


def child_path(path, key):
    key = nbt.snbt_key(key)
    return f"{path}.{key}" if path else key


def glob(pattern):
    return re.compile(".*".join(map(re.escape, pattern.split("*"))))


class BlockStates:
    """Canonical block state text, with defaults from assets/blocks.json for bare ids."""

    def __init__(self, assets):
        self.defaults = {}
        blocks, properties = os.path.join(assets, "blocks.json"), os.path.join(assets, "properties.json")
        if not (os.path.exists(blocks) and os.path.exists(properties)):
            return
        with open(properties) as f:
            props = {p["hash_key"]: p for p in json.load(f)}
        with open(blocks) as f:
            for block in json.load(f)["blocks"]:
                # State ids count through each property's values, the last property fastest
                # (pumpkin-codegen's from_index).
                index, values = block["default_state_id"] - block["states"][0]["id"], {}
                for key in reversed(block.get("properties", [])):
                    prop = props[key]
                    if prop["type"] == "boolean":
                        options = ["true", "false"]
                    elif prop["type"] == "int":
                        options = [str(v) for v in range(prop["min"], prop["max"] + 1)]
                    else:
                        options = prop["values"]
                    values[prop["serialized_name"]] = options[index % len(options)]
                    index //= len(options)
                self.defaults["minecraft:" + block["name"]] = values

    def canonical(self, entry):
        if isinstance(entry, str):
            name, given = entry, {}
        else:
            name = entry.get("id", entry.get("Name", ""))
            given = entry.get("properties", entry.get("Properties", {}))
        name = name if ":" in name else "minecraft:" + name
        props = {**self.defaults.get(name, {}), **given}
        return name + ("[" + ",".join(f"{k}={v}" for k, v in sorted(props.items())) + "]" if props else "")


def palette_form(entry):
    if isinstance(entry, str):
        return "bare id"
    return "{Name, Properties}" if "Name" in entry else "{id, properties}"


def decode_container(container, size, minimum_bits, canonical):
    """The value of every cell of a paletted container (block_states or biomes)."""
    palette = [canonical(unwrap(entry)) for entry in container["palette"]]
    data = container.get("data")
    if len(palette) == 1 or not data:
        return palette[:1] * size
    bits = max(minimum_bits, (len(palette) - 1).bit_length())
    while bits < 64 and -(-size // (64 // bits)) != len(data):
        bits += 1
    per_long, mask, cells = 64 // bits, (1 << bits) - 1, []
    for word in data:
        word &= 0xFFFFFFFFFFFFFFFF
        for _ in range(per_long):
            cells.append(word & mask)
            word >>= bits
    return [palette[i] if i < len(palette) else f"<palette index {i}>" for i in cells[:size]]


class Differ:
    def __init__(self, assets):
        self.states = BlockStates(assets)
        self.diffs = []
        self.file, self.chunk, self.forms = None, None, None

    def add(self, path, a, b):
        self.diffs.append(Diff(self.file, self.chunk, path, a, b))

    def compare(self, a, b, path):
        a, b = unwrap(a), unwrap(b)
        if isinstance(a, dict) and isinstance(b, dict):
            for key in list(a) + [k for k in b if k not in a]:
                child = child_path(path, key)
                if key not in b:
                    self.add(child, show(a[key]), MISSING)
                elif key not in a:
                    self.add(child, MISSING, show(b[key]))
                elif key in ("block_states", "biomes") and all(isinstance(v, dict) and "palette" in v
                                                               for v in (a[key], b[key])):
                    self.compare_container(a[key], b[key], child, key, a.get("Y"))
                else:
                    self.compare(a[key], b[key], child)
        elif isinstance(a, list) and isinstance(b, list) and type(a) is type(b) and type(a) in (list, nbt.List):
            self.compare_list(a, b, path)
        elif type(a) is type(b) and type(a) in (nbt.IntArray, nbt.LongArray) and len(a) == len(b) > LONG_ARRAY \
                and a != b:
            differing = [i for i in range(len(a)) if a[i] != b[i]]
            self.add(path, f"{len(differing)} of {len(a)} values differ, first [{differing[0]}]: {a[differing[0]]}",
                     str(b[differing[0]]))
        elif not same(a, b):
            self.add(path, show(a), show(b))

    def compare_list(self, a, b, path):
        a, b = [unwrap(v) for v in a], [unwrap(v) for v in b]
        fields = identity(a, b)
        if fields is None:
            for i in range(max(len(a), len(b))):
                if i >= len(b):
                    self.add(f"{path}[{i}]", show(a[i]), MISSING)
                elif i >= len(a):
                    self.add(f"{path}[{i}]", MISSING, show(b[i]))
                else:
                    self.compare(a[i], b[i], f"{path}[{i}]")
            return
        keyed_b = {tuple(nbt.to_snbt(e[f]) for f in fields): e for e in b}
        seen = set()
        for element in a:
            key = tuple(nbt.to_snbt(element[f]) for f in fields)
            seen.add(key)
            selector = f"{path}[{{{','.join(f'{nbt.snbt_key(f)}:{v}' for f, v in zip(fields, key))}}}]"
            if key in keyed_b:
                self.compare(element, keyed_b[key], selector)
            else:
                self.add(selector, show(element), MISSING)
        for key, element in keyed_b.items():
            if key not in seen:
                self.add(f"{path}[{{{','.join(f'{nbt.snbt_key(f)}:{v}' for f, v in zip(fields, key))}}}]",
                         MISSING, show(element))

    def compare_container(self, a, b, path, kind, section_y):
        if kind == "block_states" and self.forms is not None:
            for side, container in zip(self.forms, (a, b)):
                side.update(palette_form(unwrap(e)) for e in container["palette"])
        if a == b:
            return
        size, bits, canonical, width = (4096, 4, self.states.canonical, 16) if kind == "block_states" else \
            (64, 1, lambda e: e if ":" in e else "minecraft:" + e, 4)
        cells_a, cells_b = (decode_container(c, size, bits, canonical) for c in (a, b))
        differing = [i for i in range(size) if cells_a[i] != cells_b[i]]
        if not differing:
            return
        i = differing[0]
        scale = 16 // width
        x, z = self.chunk[0] * 16 + (i % width) * scale, self.chunk[1] * 16 + (i // width % width) * scale
        y = (section_y or 0) * 16 + (i // (width * width)) * scale
        unit = "blocks" if kind == "block_states" else "biome cells"
        self.add(path, f"{len(differing)} {unit} differ, first at {x},{y},{z}: {cells_a[i]}", cells_b[i])

    def compare_region(self, rel, path_a, path_b):
        chunks_a = {(cx, cz): raw for cx, cz, _, raw in nbt.region_chunks(path_a)}
        chunks_b = {(cx, cz): raw for cx, cz, _, raw in nbt.region_chunks(path_b)}
        for pos in sorted(set(chunks_a) | set(chunks_b)):
            self.chunk = pos
            if pos not in chunks_b:
                self.add("", "chunk", MISSING)
            elif pos not in chunks_a:
                self.add("", MISSING, "chunk")
            elif chunks_a[pos] != chunks_b[pos]:
                self.forms = (set(), set())
                self.compare(nbt.loads(chunks_a[pos])[1], nbt.loads(chunks_b[pos])[1], "")
                legacy = ["{Name, Properties}" in side for side in self.forms]
                if legacy[0] != legacy[1]:
                    self.add("sections[].block_states.palette", ", ".join(sorted(self.forms[0])) or MISSING,
                             ", ".join(sorted(self.forms[1])) or MISSING)
                self.forms = None
        self.chunk = None

    def compare_file(self, rel, path_a, path_b):
        self.file = rel
        if rel.endswith(".mcc"):
            return  # read with the region file that points to it
        if rel.endswith(".mca"):
            self.compare_region(rel, path_a, path_b)
            return
        loaded = [load(path, rel) for path in (path_a, path_b)]
        if all(value is not None for value in loaded):
            self.compare(loaded[0], loaded[1], "")
        else:
            with open(path_a, "rb") as fa, open(path_b, "rb") as fb:
                if fa.read() != fb.read():
                    self.add("", "bytes", "other bytes")


def identity(a, b):
    elements = a + b
    if not elements or not all(isinstance(e, dict) for e in elements):
        return None
    for fields in IDENTITY:
        if fields == ("id",) and any("count" in e for e in elements):
            continue  # item stacks without slots keep their order
        if all(all(f in e for f in fields) for e in elements):
            keys_a = [tuple(nbt.to_snbt(e[f]) for f in fields) for e in a]
            keys_b = [tuple(nbt.to_snbt(e[f]) for f in fields) for e in b]
            if len(set(keys_a)) == len(keys_a) and len(set(keys_b)) == len(keys_b):
                return fields
    return None


def load(path, rel):
    """The file's NBT or JSON value, or None to compare it by bytes."""
    try:
        if rel.endswith(".json"):
            with open(path) as f:
                return json.load(f)
        if rel.endswith((".dat", ".dat_old", ".nbt")):
            return nbt.read_file(path)[1]
    except (ValueError, OSError, EOFError, IndexError, KeyError):
        pass
    return None


def files(root):
    out = set()
    for dirpath, _, names in os.walk(root):
        for name in names:
            out.add(os.path.relpath(os.path.join(dirpath, name), root).replace(os.sep, "/"))
    return out


def read_ignore_file(path):
    entries = []
    with open(path) as f:
        for number, line in enumerate(f, 1):
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            parts = line.split(None, 2)
            if len(parts) < 3:
                raise ValueError(f"{path}:{number}: needs a file glob, a path glob and a reason")
            entries.append(tuple(parts))
    return entries


def diff_worlds(world_a, world_b, ignore=IGNORE, assets=ASSETS):
    """Returns (differences, [(ignored difference, ignore entry)])."""
    differ = Differ(assets)
    files_a, files_b = files(world_a), files(world_b)
    for rel in sorted(files_a | files_b):
        if rel not in files_b:
            differ.diffs.append(Diff(rel, None, "", "file", MISSING))
        elif rel not in files_a:
            differ.diffs.append(Diff(rel, None, "", MISSING, "file"))
        else:
            differ.compare_file(rel, os.path.join(world_a, rel), os.path.join(world_b, rel))
    rules = [(glob(f), glob(p), (f, p, reason)) for f, p, reason in ignore]
    kept, ignored = [], []
    for diff in differ.diffs:
        entry = next((e for f, p, e in rules if f.fullmatch(diff.file) and p.fullmatch(diff.path)), None)
        if entry:
            ignored.append((diff, entry))
        else:
            kept.append(diff)
    return kept, ignored


def format_diff(diff):
    chunk = f" chunk {diff.chunk[0]},{diff.chunk[1]}" if diff.chunk else ""
    path = f" {diff.path}" if diff.path else ""
    return f"{diff.file}{chunk}{path}: {diff.a} | {diff.b}"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("world_a")
    parser.add_argument("world_b")
    parser.add_argument("--ignore-file", action="append", default=[],
                        help="more ignore entries: <file glob> <path glob> <reason> per line")
    parser.add_argument("--assets", default=ASSETS, help="PEX assets directory, for default block states")
    args = parser.parse_args()
    for world in (args.world_a, args.world_b):
        if not os.path.isdir(world):
            parser.error(f"{world} is not a directory")
    try:
        ignore = IGNORE + [entry for path in args.ignore_file for entry in read_ignore_file(path)]
    except (OSError, ValueError) as error:
        parser.error(str(error))
    diffs, ignored = diff_worlds(args.world_a, args.world_b, ignore, args.assets)
    for diff in diffs:
        print(format_diff(diff))
    counts = collections.Counter(entry for _, entry in ignored)
    for (file_glob, path_glob, reason), count in counts.items():
        print(f"ignored {count}: {file_glob} {path_glob} ({reason})")
    print(f"{len(diffs)} difference(s), {len(ignored)} ignored")
    return 1 if diffs else 0


if __name__ == "__main__":
    sys.exit(main())
