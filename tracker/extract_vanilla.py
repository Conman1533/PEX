#!/usr/bin/env python3
"""Build tracker/vanilla/registry.json from a decompiled vanilla server.

The tracker has to know which vanilla blocks and items carry behaviour so it can
list the ones nobody has written an entry for. Vanilla says that through the
class it registers each block or item with (a plain `Block` or `Item` has none).
This script reads those classes out of `Blocks.java` and `Items.java` and joins
them with the ids in `assets/`, so `check.py` can stay offline.

Run it once per Minecraft version, after decompiling the server jar as
described in the root AGENTS.md:

    python3 tracker/extract_vanilla.py <decompiled source root> [version]

where the source root is the folder holding `net/minecraft/...`.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tracker" / "vanilla" / "registry.json"

COLORS = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray",
    "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black",
]
COPPER_STATES = ["", "exposed_", "weathered_", "oxidized_"]

# Item components that give an item behaviour without a dedicated class.
BEHAVIOUR_COMPONENTS = {
    "minecraft:food": "food",
    "minecraft:consumable": "consumable",
    "minecraft:tool": "tool",
    "minecraft:weapon": "weapon",
    "minecraft:equippable": "equippable",
    "minecraft:jukebox_playable": "jukebox_playable",
    "minecraft:kinetic_weapon": "kinetic_weapon",
    "minecraft:piercing_weapon": "piercing_weapon",
    "minecraft:blocks_attacks": "blocks_attacks",
    "minecraft:death_protection": "death_protection",
    "minecraft:glider": "glider",
    "minecraft:use_cooldown": "use_cooldown",
    "minecraft:use_remainder": "use_remainder",
    "minecraft:repairable": None,
}

FIELD = re.compile(r"public static final ([A-Za-z<>]+) ([A-Z0-9_]+) = ")
CLASS_REF = re.compile(r"\b([A-Z][A-Za-z0-9]*(?:Block|Item))(?:::new|\s*\()")


def statements(text):
    """Yield (type, name, expression) for every `public static final` field."""
    for m in FIELD.finditer(text):
        depth, i = 0, m.end()
        while i < len(text):
            c = text[i]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
            elif c == ";" and depth == 0:
                break
            i += 1
        yield m.group(1), m.group(2), text[m.end():i]


def id_constants(refs):
    """Map `BlockItemIds.STONE` style references to their string id."""
    ids = {}
    for f in ("BlockItemIds", "BlockIds", "ItemIds"):
        path = refs / f"{f}.java"
        if not path.exists():
            continue
        for _, name, expr in statements(path.read_text()):
            if "SPECIAL_NAMES" in expr:
                # copper_block, exposed_copper, ...: the names vanilla kept from before the collections.
                ids[f"{f}.{name}"] = ("copper", "copper_block")
                continue
            m = re.search(r'\(\s*"([a-z0-9_/]+)"', expr)
            if m:
                arg = m.group(1)
                if "createSpawnEgg" in expr:
                    arg = f"{arg}_spawn_egg"
                elif "createPotterySherd" in expr:
                    arg = f"{arg}_pottery_sherd"
                elif "createMusicDisc" in expr:
                    arg = f"music_disc_{arg}"
                elif "createArmorTrimSmithingTemplate" in expr:
                    arg = f"{arg}_armor_trim_smithing_template"
                ids[f"{f}.{name}"] = ("single", arg)
                if "Colored" in expr:
                    ids[f"{f}.{name}"] = ("color", arg)
                elif "Copper" in expr or "prefixWithState" in expr:
                    ids[f"{f}.{name}"] = ("copper", arg)
    return ids


def top_level_args(expr):
    """Split the outermost call's arguments at depth-one commas."""
    start = expr.find("(")
    args, depth, cur = [], 0, ""
    for c in expr[start + 1:]:
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                break
            depth -= 1
        if c == "," and depth == 0:
            args.append(cur)
            cur = ""
        else:
            cur += c
    args.append(cur)
    return args


HELPER = re.compile(r"private static (?:Block|Item) (register[A-Za-z]*)\(([^)]*)\) \{(.*?)\n    \}", re.S)


def helper_classes(text):
    """Map helpers like registerLegacyStair to the class their body builds."""
    found = {}
    for name, _, body in HELPER.findall(text):
        cls = [c for c in CLASS_REF.findall(body) if c not in ("Block", "Item") and not c.endswith("Properties")]
        if cls:
            found.setdefault(name, cls[0])
    return found


def classes_in(expr, base):
    found = [c for c in CLASS_REF.findall(expr) if c not in ("Block", "Item")]
    # Properties builders and helpers are not behaviour classes.
    found = [c for c in found if not c.endswith("Properties") and c not in ("BlockItemId",)]
    return found or [base]


def expand(kind, base, known):
    if kind == "single":
        return [base]
    if kind == "color":
        return [f"{c}_{base}" for c in COLORS]
    states = "|".join(s for s in COPPER_STATES if s)
    if base == "copper_block":
        # Vanilla names the weathered full blocks without the "_block" suffix.
        return sorted(i for i in known if re.fullmatch(rf"(waxed_)?(copper_block|({states})copper)", i))
    return sorted(i for i in known if re.fullmatch(rf"(waxed_)?({states})?{re.escape(base)}", i))


def commands(mc):
    """Root command literals vanilla registers, with the condition that guards them."""
    text = (mc / "commands/Commands.java").read_text()
    start = text.index("public Commands(")
    body_start = text.index("{", start) + 1
    stack, depth, i, found = [], 1, body_start, []
    pending = None
    while depth and i < len(text):
        c = text[i]
        if text.startswith("if (", i):
            j, d = i + 3, 0
            while True:
                if text[j] == "(":
                    d += 1
                elif text[j] == ")":
                    d -= 1
                    if d == 0:
                        break
                j += 1
            cond = text[i + 4:j]
            pending = ("dev" if "DEBUG" in cond or "IDE" in cond
                       else "dedicated" if "includeDedicated" in cond
                       else "integrated" if "includeIntegrated" in cond
                       else "jfr" if "JvmProfiler" in cond else "other")
            i = j
        elif c == "{":
            depth += 1
            stack.append(pending)
            pending = None
        elif c == "}":
            depth -= 1
            if stack:
                stack.pop()
        else:
            m = re.match(r"([A-Z][A-Za-z]*Commands?)\.register\(", text[i:i + 80])
            if m and (i == 0 or not text[i - 1].isalnum()):
                conds = [x for x in stack if x]
                cond = "dev" if "dev" in conds else (conds[-1] if conds else "always")
                found.append((m.group(1), cond))
                i += len(m.group(0))
                continue
        i += 1
    out = {}
    for cls, cond in found:
        path = next(mc.rglob(f"server/commands/**/{cls}.java"), None) or next(mc.rglob(f"{cls}.java"), None)
        if path is None:
            continue
        t = path.read_text()
        # Root literals are the ones passed straight to dispatcher.register(...).
        names = re.findall(r'dispatcher\.register\([(A-Za-z<>)]*Commands\.literal\("([a-z0-9_-]+)"\)', t)
        derived = cls.removesuffix("Commands").removesuffix("Command").lower()
        if not names and f'Commands.literal("{derived}")' in t:
            names = [derived]
        if not names:
            # ReturnCommand and TestCommand build their literal through a helper.
            names = [cls.removesuffix("Commands").removesuffix("Command").lower()]
        for k, n in enumerate(dict.fromkeys(names)):
            out[n] = {"class": cls, "when": cond, **({"alias_of": names[0]} if k else {})}
    return dict(sorted(out.items()))


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    src = Path(sys.argv[1])
    version = sys.argv[2] if len(sys.argv) > 2 else "26.3"
    mc = src / "net" / "minecraft"
    ids = id_constants(mc / "references")

    assets_blocks = {b["name"] for b in json.loads((ROOT / "assets/blocks.json").read_text())["blocks"]}
    assets_items = json.loads((ROOT / "assets/items.json").read_text())

    blocks = {}
    blocks_java = (mc / "world/level/block/Blocks.java").read_text()
    block_helpers = helper_classes(blocks_java)
    for _, name, expr in statements(blocks_java):
        ref = re.search(r"\b(Block(?:Item)?Ids\.[A-Z0-9_]+)", expr)
        kind, base = ids.get(ref.group(1), ("single", name.lower())) if ref else ("single", name.lower())
        cls = classes_in(expr, "Block")
        helpers = [h for h in re.findall(r"\b(register[A-Za-z]*)\(", expr) if h in block_helpers]
        if cls == ["Block"] and helpers:
            cls = [block_helpers[helpers[0]]]
        if kind == "copper" and "registerBlocks" in expr:
            # registerBlocks(ids, register, waxedFactory, weatheringFactory, properties)
            args = top_level_args(expr)
            waxed, weathering = classes_in(args[2], "Block")[0], classes_in(args[3], "Block")[0]
            for bid in expand(kind, base, assets_blocks):
                blocks[bid] = waxed if bid.startswith("waxed_") else weathering
            continue
        for bid in expand(kind, base, assets_blocks):
            blocks[bid] = cls[0]

    items = {}
    for _, name, expr in statements((mc / "world/item/Items.java").read_text()):
        ref = re.search(r"\b((?:Block)?(?:Item)?Ids\.[A-Z0-9_]+)", expr)
        kind, base = ids.get(ref.group(1), ("single", name.lower())) if ref else ("single", name.lower())
        default = "BlockItem" if "registerBlock" in expr else "Item"
        cls = classes_in(expr, default)
        targets = [i for i in expand(kind, base, set(assets_items)) if i in assets_items]
        if not targets or "createBlockItemWithCustomItemName" in expr:
            # Block items with a custom name (wheat_seeds for BlockItemIds.WHEAT_CROP).
            targets = [name.lower()]
            if "createBlockItemWithCustomItemName" in expr and cls == ["Item"]:
                cls = ["BlockItem"]
        for iid in targets:
            items[iid] = cls[0]

    out_blocks = {}
    for bid in sorted(assets_blocks):
        out_blocks[bid] = blocks.get(bid, "?")
    out_items = {}
    for iid in sorted(assets_items):
        comps = assets_items[iid].get("components", {})
        tags = sorted({v for k, v in BEHAVIOUR_COMPONENTS.items() if v and k in comps})
        out_items[iid] = {"class": items.get(iid, "?"), "components": tags}

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({
        "version": version,
        "source": "Blocks.java and Items.java from the decompiled vanilla server, joined with assets/blocks.json and assets/items.json",
        "blocks": out_blocks,
        "items": out_items,
        "commands": commands(mc),
    }, indent=1, sort_keys=False) + "\n")
    unknown_b = [b for b, c in out_blocks.items() if c == "?"]
    unknown_i = [i for i, v in out_items.items() if v["class"] == "?"]
    print(f"{len(out_blocks)} blocks ({len(unknown_b)} unresolved), {len(out_items)} items ({len(unknown_i)} unresolved)")
    for x in unknown_b[:40] + unknown_i[:40]:
        print("  unresolved:", x)


if __name__ == "__main__":
    main()
