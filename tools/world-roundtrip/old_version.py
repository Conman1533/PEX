#!/usr/bin/env python3
"""A world from an older vanilla, through PEX and into vanilla 26.3 (G7-28, roadmap D7-2).

    python3 tools/world-roundtrip/old_version.py --pex <pumpkin binary> \\
        --java <jdk 25>/bin/java --old-jar <older vanilla server jar> \\
        --server-jar <vanilla 26.3 server or bundler jar> --out <empty or missing dir> \\
        [--known-losses FILE] [--port 25990]

1. old: the older jar builds build_fixture.py's lab (blocks, block entities, entities and the
   showcase items it knows; commands naming things it doesn't fail and are listed), plus a
   decorated pot with sherds, a pot item with decorations and a sign, then saves.
2. reference: vanilla 26.3 opens a copy, loads the lab and saves, upgrading it its own way, then
   opens that save once more, so the lab has run as long as on the PEX side.
3. pex: PEX opens another copy, loads the lab and saves; then vanilla 26.3 opens PEX's save,
   loads the lab and saves. Its log is searched for the G7-04 lines.

Step 3's vanilla save is diffed against step 2's, outside RUN_IGNORE and the known-loss list. Chunks outside
the lab are left out: each server loads its own spawn area, and a chunk neither loaded keeps
the old version on both sides. The report goes to <out>/report.md.

Exit status: 0 when the old world built, vanilla logged no G7-04 line on PEX's save and the
two saves match; 1 otherwise; 2 on usage errors.
"""
import argparse
import json
import os
import shutil
import sys

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import build_fixture  # noqa: E402
import roundtrip  # noqa: E402
import world_diff  # noqa: E402

LAB_CHUNKS = roundtrip.LAB_CHUNKS
EXTRAS = [
    # A pot whose sherds and a pot item whose decorations 4996 unflattens, and a sign 5002
    # gives the op flag, east of the lab's block entities.
    f"setblock 92 {build_fixture.Y} 60 decorated_pot"
    '{sherds:["minecraft:angler_pottery_sherd","minecraft:brick","minecraft:skull_pottery_sherd"]}',
    f'setblock 89 {build_fixture.Y} 60 oak_sign{{front_text:{{messages:["\\"old\\"","","",""]}}}}',
    f"setblock 86 {build_fixture.Y} 60 chest",
    f"item replace block 86 {build_fixture.Y} 60 container.0 with decorated_pot"
    '[pot_decorations=["minecraft:heart_pottery_sherd","minecraft:brick","minecraft:brick","minecraft:brick"]]',
    # A treasure map an older jar names the old way.
    f"loot insert 86 {build_fixture.Y} 60 loot minecraft:chests/buried_treasure",
    f"loot insert 86 {build_fixture.Y} 60 loot minecraft:chests/shipwreck_map",
    # Block states in the {Name, Properties} form 5006 renames, which the older jar's summons
    # take (the lab's own summons use 26.3's string form).
    f"summon falling_block 80.5 {build_fixture.Y} 56.5 {{Tags:[\"fixture\"],NoGravity:1b,"
    'Time:-2147483648,BlockState:{Name:"minecraft:anvil",Properties:{facing:"east"}}}',
    f"summon enderman 83.5 {build_fixture.Y} 56.5 {{Tags:[\"fixture\"],NoAI:1b,Silent:1b,"
    'PersistenceRequired:1b,carriedBlockState:{Name:"minecraft:grass_block",Properties:{snowy:"false"}}}',
    f"summon block_display 86.5 {build_fixture.Y} 56.5 {{Tags:[\"fixture\"],"
    'block_state:{Name:"minecraft:oak_stairs",Properties:{facing:"west",half:"top"}}}',
]
# Differences that come from how the two sides run rather than from what PEX reads or writes.
RUN_IGNORE = [
    ("data/minecraft/*.dat", "DataVersion",
     "PEX writes every saved-data file at its own version, vanilla leaves one it didn't change at "
     "the old version; no fixer from 4996 to 5016 reads these files"),
    ("dimensions/*/data/minecraft/*.dat", "DataVersion", "as above, for a dimension's saved data"),
    # Each server runs the lab for its own number of ticks, with its own mobs and weather;
    # roundtrip.py checks these files on a world PEX made itself.
    ("data/minecraft/random_sequences.dat", "data.sequences.*", "random sequences of mobs that ran"),
    ("data/minecraft/wandering_trader.dat", "", "written once a server has run long enough"),
    ("data/minecraft/weather.dat", "data.rain_time", "counts down while a server runs"),
    ("data/minecraft/weather.dat", "data.thunder_time", "counts down while a server runs"),
    ("data/minecraft/world_clocks.dat", "data.*.total_ticks", "ticks each server ran"),
    ("dimensions/*/entities/*.mca", "Entities[*].DeathTime", "a lab mob dying while a server runs"),
] + [("data/minecraft/scoreboard.dat", f"data.{key}",
       "PEX writes the empty lists a save from the older jar leaves out; vanilla reads both the same")
      for key in ("Objectives", "PlayerScores", "Teams", "DisplaySlots")]


class OldConsole(build_fixture.Console):
    """build_fixture's console for a jar whose command feedback isn't logged as `System chat:`."""

    @staticmethod
    def ok(lines):
        return bool(lines) and not any(build_fixture.FAILED.search(line) or "Test failed" in line
                                       for line in lines)

    def run(self, command):
        lines = self.output(command)
        if not self.ok(lines):
            self.failures.append((command, lines))
            print(f"FAILED: {command}\n    " + "\n    ".join(lines), flush=True)

    def test(self, command):
        return self.ok(self.output(command))


def build_old_world(args, out):
    server_dir = os.path.join(out, "server")
    with open(build_fixture.ENTITIES_JSON) as f:
        entity_types = {name: info for name, info in json.load(f).items()
                        if info["summonable"] and name not in build_fixture.NOT_SAVED}
    build_fixture.write_server_files(server_dir, args.port, hardcore=False)
    server = build_fixture.start(args.java, args.old_jar, server_dir, out, "world")
    console = OldConsole(server)
    try:
        console.run("tick freeze")
        build_fixture.build_lab(console, entity_types)
        for command in EXTRAS:
            console.run(command)
        console.run("save-all flush")
    finally:
        server.stop()
    for name in ("libraries", "versions"):
        shutil.rmtree(os.path.join(server_dir, name), ignore_errors=True)
    return [command for command, _ in console.failures]


def open_and_save(args, kind, src_world, folder, port):
    roundtrip.copy_world(src_world, os.path.join(folder, "world"))
    server, console = roundtrip.start(args, kind, folder, port)
    try:
        roundtrip.load_lab(console)
        roundtrip.save_and_stop(server, console)
    finally:
        server.stop()
    return os.path.join(folder, "world")


def in_lab(d):
    if d.chunk is None:
        return True
    x, z = d.chunk
    x0, z0, x1, z1 = LAB_CHUNKS
    return x0 <= x <= x1 and z0 <= z <= z1


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--pex", required=True)
    parser.add_argument("--java", required=True)
    parser.add_argument("--old-jar", required=True)
    parser.add_argument("--server-jar", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--known-losses")
    parser.add_argument("--port", type=int, default=25990)
    args = parser.parse_args()
    if os.path.exists(args.out) and os.listdir(args.out):
        parser.error(f"{args.out} is not empty")
    known = world_diff.read_ignore_file(args.known_losses) if args.known_losses else []
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    report, problems = ["# An older vanilla's world through PEX\n"], 0

    old_dir = os.path.join(out, "old")
    failed = build_old_world(args, old_dir)
    old_world = os.path.join(old_dir, "world")
    report.append(f"## 1. Built with {os.path.basename(args.old_jar)}\n")
    report.append(f"{len(failed)} commands failed (things the older jar doesn't have).\n")
    report.extend(f"- `{command}`" for command in failed[:40])
    report.append("")

    upgraded = open_and_save(args, "vanilla", old_world, os.path.join(out, "upgraded"), args.port + 1)
    reference = open_and_save(args, "vanilla", upgraded, os.path.join(out, "reference"), args.port + 4)
    pex_world = open_and_save(args, "pex", old_world, os.path.join(out, "pex"), args.port + 2)
    vanilla_dir = os.path.join(out, "pex-vanilla")
    through = open_and_save(args, "vanilla", pex_world, vanilla_dir, args.port + 3)

    with open(os.path.join(vanilla_dir, "console.log")) as f:
        bad = [line.rstrip() for line in f if roundtrip.VANILLA_PROBLEMS.search(line)]
    report.append("## 2. Vanilla 26.3 log on PEX's save\n")
    report.append(f"{len(bad)} lines naming a load problem.\n")
    report.extend(f"- `{line}`" for line in bad[:50])
    report.append("")
    problems += len(bad)

    kept, ignored = world_diff.diff_worlds(reference, through, ignore=world_diff.IGNORE + RUN_IGNORE + known)
    kept = [d for d in kept if in_lab(d)]
    known_hits = [d for d, entry in ignored if entry in known and in_lab(d)]
    report.append("## 3. Vanilla's own upgrade -> PEX, then vanilla\n")
    report.append(f"{len(kept)} differences outside the ignore and known-loss lists, "
                  f"{len(known_hits)} known losses.\n")
    grouped = roundtrip.groups(kept)
    report.append("| Count | Change | File | Path |")
    report.append("| --: | :-- | :-- | :-- |")
    report.extend(f"| {n} | {k} | `{f}` | `{p}` |" for (f, p, k), n in grouped)
    report.append("")
    report.extend(f"- `{world_diff.format_diff(d)}`" for d in kept[:200])
    report.append("")
    problems += len(kept)

    with open(os.path.join(out, "report.md"), "w") as f:
        f.write("\n".join(report) + "\n")
    print("\n".join(report))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
