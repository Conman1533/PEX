#!/usr/bin/env python3
"""Round-trip G7-01's fixture world through PEX and vanilla 26.3 (G7-03 and G7-04).

    python3 tools/world-roundtrip/roundtrip.py --pex <pumpkin binary> \\
        --java <jdk 25>/bin/java --server-jar <vanilla 26.3 server or bundler jar> \\
        --fixture <build_fixture.py --out dir> --out <empty or missing dir> \\
        [--known-losses FILE] [--port 25960]

Steps, each in its own folder under --out:

1. pex: copies the fixture world, starts PEX on it with `chunk = { type = "anvil" }`,
   force-loads the lab, waits, runs `save-all` and `stop`. The save is diffed against the
   fixture with world_diff.py (G7-03).
2. vanilla: vanilla 26.3 opens step 1's save, force-loads the lab, saves and stops. Its log is
   searched for the lines G7-04 names, and its save is diffed against step 1's.
3. pex-again: PEX opens vanilla's save and saves it; the result is diffed against step 1's.
4. fresh: PEX makes a new world, sets time, weather, a scoreboard objective, score and team, a
   moved border and forced chunks, saves, and restarts. After the restart it checks they came
   back, then vanilla opens the same save and checks it sees them too.

Every step's saved data files (`scoreboard.dat`, `world_clocks.dat`, `weather.dat`, each
dimension's `world_border.dat`, `chunk_tickets.dat` and `ender_dragon_fight.dat`) also get
their own section in the report, because Goal 7's saved data pieces prove themselves there.

--known-losses lists differences that are known and not yet fixed, in world_diff's ignore
file form (`<file glob> <path glob> <reason>`). An entry whose file glob is `vanilla.log`
names a known vanilla log line instead: its path glob is searched for in each problem line,
with `*` for any run of characters (spaces included). The report goes to <out>/report.md.

Exit status: 0 when every step ran, no difference falls outside the ignore and known-loss
lists, vanilla logged none of the G7-04 lines and every check in step 4 passed; 1 otherwise;
2 on usage errors.
"""
import argparse
import os
import re
import shutil
import sys
import time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
sys.path.insert(0, os.path.join(TOOLS, "..", "dual-run"))
import nbt  # noqa: E402
import world_diff  # noqa: E402
from dual_run import Server  # noqa: E402

LAB_CHUNKS = (0, 0, 5, 3)  # the fixture lab is x 0..95, z 0..63
VANILLA_PROBLEMS = re.compile(
    r"Failed to parse saved data|Skipping block entity with invalid type|reportChunkLoadFailure"
    r"|Failed to load chunk|Couldn't load chunk|Failed to read"
    # Vanilla falls back to air for a section whose palette it can't read (G7-05).
    r"|Recoverable errors when loading section")
PEX_READY = re.compile(r"Started server|Done \(")
VANILLA_READY = re.compile(r"Done \(")
SAVED = re.compile(r"[Ss]aved the game|Saved the world|saved", re.I)
DATA_FILES = ("data/minecraft/scoreboard.dat", "data/minecraft/world_clocks.dat", "data/minecraft/weather.dat")
DIMENSION_DATA = ("world_border.dat", "chunk_tickets.dat", "ender_dragon_fight.dat")
DIMENSIONS = ("overworld", "the_nether", "the_end")


def data_file(rel):
    parts = rel.split("/")
    return rel in DATA_FILES or (len(parts) == 6 and parts[0] == "dimensions" and parts[5] in DIMENSION_DATA)


class Console:
    """Sends commands and collects what each one printed, up to a marker line.

    The marker is an unknown command: both servers echo it back as `<command><--[HERE]`
    (PEX doesn't log `say`). Vanilla shows at most 10 characters before the cursor, so the
    token stays short.
    """

    def __init__(self, server, kind):
        self.server, self.kind, self.count = server, kind, 0

    def output(self, command, timeout=120):
        self.count += 1
        token = f"rts{self.count}"
        self.server.command(command)
        self.server.command(token)
        marker, lines, deadline = f"{token}<--[HERE]", [], time.time() + timeout
        while True:
            line = self.server.next_line(deadline)
            if marker in line:
                return lines
            lines.append(line)


def write_pex_config(folder, port):
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "pumpkin.toml"), "w") as f:
        f.write("\n".join([
            "[networking.java]", f'address = "127.0.0.1:{port}"', "online_mode = false", "",
            "[networking.bedrock]", "enabled = false", "",
            "[plugins]", "enabled = false", "",
            "[telemetry]", "enabled = false", "",
            "[world]", 'chunk = { type = "anvil" }', "",
        ]))


def write_vanilla_config(folder, port):
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "eula.txt"), "w") as f:
        f.write("eula=true\n")
    with open(os.path.join(folder, "server.properties"), "w") as f:
        f.write("\n".join([
            "online-mode=false", f"server-port={port}", "server-ip=127.0.0.1", "level-name=world",
            "view-distance=4", "simulation-distance=4", "spawn-protection=0", "white-list=false",
        ]) + "\n")


def start(args, kind, folder, port):
    if kind == "pex":
        write_pex_config(folder, port)
        server = Server([os.path.abspath(args.pex)], folder)
        server.wait_for(PEX_READY, 900)
    else:
        write_vanilla_config(folder, port)
        server = Server([args.java, "-Xmx3G", "-jar", os.path.abspath(args.server_jar), "nogui"], folder)
        server.wait_for(VANILLA_READY, 900)
    return server, Console(server, kind)


def save_and_stop(server, console):
    console.output("save-all flush", timeout=600)
    server.stop()


def load_lab(console):
    # The fixture already force-loads the lab, so this changes nothing in chunk_tickets.dat;
    # it makes sure the chunks are loaded on a server that didn't restore the tickets.
    # Never `forceload remove` here: that would delete the fixture's own tickets.
    x0, z0, x1, z1 = LAB_CHUNKS
    console.output(f"forceload add {x0 * 16} {z0 * 16} {x1 * 16 + 15} {z1 * 16 + 15}")
    # Let block entities wake and entity chunks go live before saving.
    time.sleep(10)


def copy_world(src, dst):
    if os.path.exists(dst):
        shutil.rmtree(dst)
    shutil.copytree(src, dst, ignore=shutil.ignore_patterns("session.lock"))


def file_glob(rel):
    rel = re.sub(r"r\.-?\d+\.-?\d+\.mca$", "*.mca", rel)
    return re.sub(r"c\.-?\d+\.-?\d+\.mcc$", "*.mcc", rel)


def path_glob(path):
    """`block_entities[{x:1,y:2,z:3}].Items[{Slot:0b}]` -> `block_entities[*].Items[*]`."""
    while True:
        new = re.sub(r"\[[^\[\]]*\]", "\0", path)
        if new == path:
            return path.replace("\0", "[*]")
        path = new


def change_kind(d):
    if d.a == world_diff.MISSING:
        return "added"
    if d.b == world_diff.MISSING:
        return "lost"
    return "changed"


def groups(diffs):
    counts = {}
    for d in diffs:
        key = (file_glob(d.file), path_glob(d.path), change_kind(d))
        counts[key] = counts.get(key, 0) + 1
    return sorted(counts.items(), key=lambda item: (-item[1], item[0]))


def diff(report, title, world_a, world_b, known, all_groups):
    kept, ignored = world_diff.diff_worlds(world_a, world_b, ignore=world_diff.IGNORE + known)
    new = [d for d in kept]
    known_hits = [d for d, entry in ignored if entry in known]
    report.append(f"## {title}\n")
    report.append(f"{len(new)} differences outside the ignore and known-loss lists, "
                  f"{len(known_hits)} known losses.\n")
    data_new = [d for d in new if data_file(d.file)]
    report.append(f"### Saved data files: {len(data_new)} differences\n")
    report.extend(f"- `{world_diff.format_diff(d)}`" for d in data_new)
    report.append("")
    by_file = {}
    for d in new:
        by_file.setdefault(d.file, []).append(d)
    grouped = groups(new)
    all_groups.extend((title, f, p, k, n) for (f, p, k), n in grouped)
    report.append(f"### By kind: {len(grouped)} groups\n")
    report.append("| Count | Change | File | Path |")
    report.append("| --: | :-- | :-- | :-- |")
    report.extend(f"| {n} | {k} | `{f}` | `{p}` |" for (f, p, k), n in grouped)
    report.append("")
    report.append("### All differences by file\n")
    for rel, diffs in sorted(by_file.items(), key=lambda item: -len(item[1])):
        report.append(f"- `{rel}`: {len(diffs)}")
        report.extend(f"  - `{world_diff.format_diff(d)}`" for d in diffs[:20])
        if len(diffs) > 20:
            report.append(f"  - ... {len(diffs) - 20} more")
    report.append("")
    return len(new)


def read_dat(world, rel):
    path = os.path.join(world, rel)
    return nbt.read_file(path)[1] if os.path.exists(path) else None


def fresh_checks(console, kind):
    """Reads back what fresh_setup set. Returns failed checks."""
    failures = []
    # 26.3 queries the overworld clock with `time query time`; `daytime` is a timeline id there.
    lines = " ".join(console.output("time query time"))
    match = re.search(r"(?:time is|is at) (\d+)", lines, re.I)
    if not match or not 6000 <= int(match.group(1)) < 7200:
        failures.append(f"{kind}: time query time printed {lines!r}, expected about 6000")
    # The score check below also proves the objective exists (PEX has no `objectives list`).
    lines = " ".join(console.output("scoreboard players get #roundtrip roundtrip_obj"))
    if not re.search(r"\b31\b", lines):
        failures.append(f"{kind}: score of #roundtrip is not 31: {lines!r}")
    lines = " ".join(console.output("team list"))
    if "roundtrip_team" not in lines:
        failures.append(f"{kind}: team roundtrip_team missing: {lines!r}")
    lines = " ".join(console.output("worldborder get"))
    if not re.search(r"\b1234\b", lines):
        failures.append(f"{kind}: world border is not 1234 wide: {lines!r}")
    lines = " ".join(console.output("forceload query 200 200"))
    if not re.search(r"is marked for force loading|is force loaded", lines, re.I):
        failures.append(f"{kind}: chunk at 200 200 is not force-loaded: {lines!r}")
    return failures


def fresh_setup(console):
    for command in [
        "time set 6000", "weather thunder 3000", "scoreboard objectives add roundtrip_obj dummy",
        "scoreboard players set #roundtrip roundtrip_obj 31", "team add roundtrip_team",
        "worldborder set 1234", "forceload add 200 200",
    ]:
        console.output(command)


def weather_check(world, kind):
    data = read_dat(world, "data/minecraft/weather.dat")
    if data is None:
        return [f"{kind}: no weather.dat"]
    data = data.get("data", data)
    if not int(data.get("thundering", 0)) or not int(data.get("raining", 0)):
        return [f"{kind}: weather.dat is not thundering: {nbt.to_snbt(data)}"]
    if not 0 < int(data.get("thunder_time", 0)) <= 3000:
        return [f"{kind}: weather.dat thunder_time is {data.get('thunder_time')}, expected 1 to 3000"]
    return []


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--pex", required=True)
    parser.add_argument("--java", required=True)
    parser.add_argument("--server-jar", required=True)
    parser.add_argument("--fixture", required=True, help="build_fixture.py's --out folder")
    parser.add_argument("--out", required=True)
    parser.add_argument("--known-losses")
    parser.add_argument("--write-known-losses", metavar="FILE",
                        help="write every difference group found as a known-loss list, to start one")
    parser.add_argument("--port", type=int, default=25960)
    args = parser.parse_args()
    fixture_world = os.path.join(args.fixture, "world")
    if not os.path.isfile(os.path.join(fixture_world, "level.dat")):
        parser.error(f"{fixture_world} has no level.dat")
    known = world_diff.read_ignore_file(args.known_losses) if args.known_losses else []
    os.makedirs(args.out, exist_ok=True)
    report, problems, all_groups = ["# World round trip\n"], 0, []

    pex_dir = os.path.join(args.out, "pex")
    copy_world(fixture_world, os.path.join(pex_dir, "world"))
    server, console = start(args, "pex", pex_dir, args.port)
    try:
        load_lab(console)
        save_and_stop(server, console)
    finally:
        server.stop()
    problems += diff(report, "1. Fixture -> PEX", fixture_world, os.path.join(pex_dir, "world"), known, all_groups)

    vanilla_dir = os.path.join(args.out, "vanilla")
    copy_world(os.path.join(pex_dir, "world"), os.path.join(vanilla_dir, "world"))
    server, console = start(args, "vanilla", vanilla_dir, args.port + 1)
    try:
        load_lab(console)
        save_and_stop(server, console)
    finally:
        server.stop()
    with open(os.path.join(vanilla_dir, "console.log")) as f:
        bad = [line.rstrip() for line in f if VANILLA_PROBLEMS.search(line)]
    log_known = [(world_diff.glob(p), reason) for f, p, reason in known if f == "vanilla.log"]
    known_lines = [line for line in bad if any(g.search(line) for g, _ in log_known)]
    bad = [line for line in bad if line not in known_lines]
    report.append("## 2. Vanilla log on PEX's save\n")
    report.append(f"{len(bad)} lines naming a load problem, {len(known_lines)} known.\n")
    report.extend(f"- `{line}`" for line in bad[:50])
    if known_lines:
        report.append(f"\nKnown, first of {len(known_lines)}: `{known_lines[0]}`")
    report.append("")
    problems += len(bad)
    problems += diff(report, "2. PEX -> vanilla", os.path.join(pex_dir, "world"),
                     os.path.join(vanilla_dir, "world"), known, all_groups)

    again_dir = os.path.join(args.out, "pex-again")
    copy_world(os.path.join(vanilla_dir, "world"), os.path.join(again_dir, "world"))
    server, console = start(args, "pex", again_dir, args.port + 2)
    try:
        load_lab(console)
        save_and_stop(server, console)
    finally:
        server.stop()
    problems += diff(report, "3. Vanilla -> PEX again", os.path.join(pex_dir, "world"),
                     os.path.join(again_dir, "world"), known, all_groups)

    failures = []
    fresh_dir = os.path.join(args.out, "fresh")
    if os.path.exists(fresh_dir):
        shutil.rmtree(fresh_dir)
    server, console = start(args, "pex", fresh_dir, args.port + 3)
    try:
        fresh_setup(console)
        save_and_stop(server, console)
    finally:
        server.stop()
    fresh_world = os.path.join(fresh_dir, "world")
    failures += weather_check(fresh_world, "PEX save")
    server, console = start(args, "pex", fresh_dir, args.port + 3)
    try:
        failures += fresh_checks(console, "PEX after a restart")
        save_and_stop(server, console)
    finally:
        server.stop()
    failures += weather_check(fresh_world, "PEX save after a restart")
    vanilla_fresh = os.path.join(args.out, "fresh-vanilla")
    copy_world(fresh_world, os.path.join(vanilla_fresh, "world"))
    server, console = start(args, "vanilla", vanilla_fresh, args.port + 4)
    try:
        failures += fresh_checks(console, "vanilla on PEX's save")
        save_and_stop(server, console)
    finally:
        server.stop()
    failures += weather_check(os.path.join(vanilla_fresh, "world"), "vanilla's save of it")
    report.append("## 4. Set in PEX, after a restart, then in vanilla\n")
    report.append("All checks passed.\n" if not failures else "\n".join(f"- {f}" for f in failures) + "\n")
    problems += len(failures)

    if args.write_known_losses:
        seen = set()
        with open(args.write_known_losses, "w") as f:
            f.write("# <file glob> <path glob> <reason>; written by roundtrip.py --write-known-losses\n")
            for title, file, path, kind, count in all_groups:
                if (file, path) in seen:
                    continue
                seen.add((file, path))
                f.write(f"{file} {path} step {title.split('.')[0]}: {count} {kind}\n")

    with open(os.path.join(args.out, "report.md"), "w") as f:
        f.write("\n".join(report) + "\n")
    print("\n".join(report))
    return 0 if problems == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
