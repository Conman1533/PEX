#!/usr/bin/env python3
"""Build the vanilla 26.3 world that Goal 7's round-trip tests start from.

The world stays out of the repository, so this script builds it from an empty folder.
Vanilla starts with a fixed seed and the game frozen (`tick freeze`), so nothing placed
moves or ticks before the save. In a lab at x 0..95, z 0..63 on a glass floor at y 199
it places a block of every block entity type with contents, containers of item stacks
carrying many components, every summonable entity (named, with passengers, leashes and
changed attributes; mobs have NoAI), frogspawn and sniffer eggs whose block ticks are
thousands of ticks away, a block of every POI type including a lit nether portal, and
water and lava with pending fluid ticks. It writes scoreboard objectives, scores and
teams, a boss bar, a scheduled function, command storage, a random sequence, a
stopwatch, a treasure map, moved world borders, force-loaded chunks in all three
dimensions and non-default game rules. A bot (survbot.py from pex-dev's tools) fills
players/ with data, advancements and stats. Before the freeze, outside the lab, a
villager claims a bell (vanilla only counts claimed POIs as a village) and the bot dies
once, is hit by a wind charge's blast and gets two tamed parrots on its shoulders. Once
frozen it gets items, effects, recipes and advancements, starts a raid with Bad Omen at
the bell, takes Bad Omen again and ends in the Nether. A second world is created with
hardcore=true.

    python3 tools/world-roundtrip/build_fixture.py \\
        --java <jdk 25>/bin/java --server-jar <vanilla 26.3 server jar> \\
        --bot <survbot.py> --out <empty or missing dir>

--out gets world/, world-hardcore/, manifest.json and server/ (server files and console
logs). The manifest is read back from the saved files: for every row of the Goal 7 file
table it names what the world holds, and under "missing" it lists anything placed above
that the save does not hold. `build()` does the same from Python and returns the
manifest.

Exit status: 0 when every row has an entry and nothing is missing, 1 otherwise, 2 on
usage errors.
"""
import argparse
import hashlib
import json
import math
import os
import re
import shutil
import subprocess
import sys
import time
import uuid

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
sys.path.insert(0, os.path.join(TOOLS, "..", "dual-run"))
import nbt  # noqa: E402
from dual_run import Server  # noqa: E402
from fixture_content import (BLOCK_ATTACHED, BLOCK_ENTITIES, ENTITY_EXTRA, GAME_RULES, HANGING,  # noqa: E402
                             NEEDS_SUPPORT_ABOVE, NOT_SAVED, POI_BLOCKS, POI_TYPES, SHOWCASE, WALL_Z, WATER_CATEGORIES,
                             WATER_MOBS, item)

ENTITIES_JSON = os.path.join(TOOLS, "..", "..", "assets", "entities.json")
SEED = 2603
BOT_NAME = "fixturebot"
Y = 200  # the lab floor is glass at Y - 1; blocks and entities stand at Y
LAB = (0, 0, 95, 63)  # min x, min z, max x, max z
LAB_TOP = Y + 12
WORLD_SPAWN = (41, Y, 7)
VILLAGE = (104, Y, 4)  # the claimed bell, in chunk 6, 0 east of the lab
BOT_SPOT = (104.5, Y, 1.5)  # where the bot stands, in the village
NETHER_STATION = (0.5, 101, 0.5)
MAX_FILL = 32768  # the max_block_modifications game rule's default
FAILED = re.compile(r"<--\[HERE\]|Unknown or incomplete|Incorrect argument|Expected |Invalid |Unknown |"
                    r"not loaded|No (?:player|entity|entities|targets?) (?:was|were) found|Found no elements|"
                    r"Can't (?!keep up)|Cannot |Could not|Unable to|Failed|Test failed|Nothing changed|/ERROR\]|"
                    r"/WARN\](?!: Can't keep up)")
# The Goal 7 table in docs/ROADMAP-2.0.md, one row per file group.
ROWS = [
    "region/*.mca chunks",
    "entities/*.mca",
    "Block entities (chunk block_entities)",
    "Item stacks (containers, entities, players)",
    "poi/*.mca",
    "level.dat",
    "data/minecraft/game_rules.dat, world_gen_settings.dat, world_clocks.dat",
    "data/minecraft/weather.dat, wandering_trader.dat",
    "data/minecraft/scoreboard.dat, custom_boss_events.dat, scheduled_events.dat, random_sequences.dat, stopwatches.dat",
    "data/minecraft/maps/<id>.dat, maps/last_id.dat, data/<ns>/command_storage.dat",
    "dimensions/<ns>/<dim>/data/minecraft/: world_border.dat, chunk_tickets.dat, raids.dat, ender_dragon_fight.dat (End)",
    "players/data/<uuid>.dat",
    "players/advancements/<uuid>.json",
    "players/stats/<uuid>.json",
]
PLAYER_KEYS = ["recipeBook", "ender_pearls", "LastDeathLocation", "ShoulderEntityLeft", "ShoulderEntityRight",
               "entered_nether_pos", "raid_omen_position", "post_effects", "last_explosion_impact_pos"]

def offline_uuid(name):
    digest = bytearray(hashlib.md5(f"OfflinePlayer:{name}".encode()).digest())
    digest[6] = digest[6] & 0x0F | 0x30
    digest[8] = digest[8] & 0x3F | 0x80
    return uuid.UUID(bytes=bytes(digest))


def uuid_ints(value):
    return [int.from_bytes(value.bytes[i:i + 4], "big", signed=True) for i in range(0, 16, 4)]


class Console:
    """Runs commands on a vanilla console and checks what each one printed."""

    def __init__(self, server):
        self.server = server
        self.count = 0
        self.failures = []

    def output(self, command):
        self.count += 1
        token = f"fixture-sync-{self.count}"
        self.server.command(command)
        self.server.command(f"say {token}")
        lines, deadline = [], time.time() + 120
        while True:
            line = self.server.next_line(deadline)
            if line.endswith(f"[Server] {token}"):
                return lines
            lines.append(line)

    def run(self, command):
        lines = self.output(command)
        if any(FAILED.search(line) for line in lines) or not any("System chat:" in line for line in lines):
            self.failures.append((command, lines))
            print(f"FAILED: {command}\n    " + "\n    ".join(lines), flush=True)

    def test(self, command):
        """True when an `execute if ...` or `data get` command succeeds."""
        lines = self.output(command)
        return any("System chat:" in line for line in lines) and not any(FAILED.search(line) for line in lines)

    def wait_until(self, command, timeout, want=True):
        deadline = time.time() + timeout
        while self.test(command) != want:
            if time.time() > deadline:
                raise TimeoutError(f"`{command}` still {'fails' if want else 'succeeds'} after {timeout} s")
            time.sleep(0.5)


def fill(console, x0, y0, z0, x1, y1, z1, block, dimension=None):
    layer = (abs(x1 - x0) + 1) * (abs(z1 - z0) + 1)
    step = max(1, MAX_FILL // layer)
    prefix = f"execute in {dimension} run " if dimension else ""
    for y in range(min(y0, y1), max(y0, y1) + 1, step):
        console.run(f"{prefix}fill {x0} {y} {z0} {x1} {min(y + step - 1, max(y0, y1))} {z1} {block}")


def block_pos(x, y, z):
    return "block_pos:[I;" + ",".join(str(math.floor(float(v))) for v in (x, y, z)) + "]"


def summon_nbt(name, info, extra=""):
    parts = [f'CustomName:"Fixture {name}"', 'Tags:["fixture"]', "Silent:1b"]
    parts += ["NoAI:1b", "PersistenceRequired:1b"] if info["mob"] else ["NoGravity:1b"]
    return "{" + ",".join(parts + ([extra] if extra else [])) + "}"


def write_server_files(server_dir, port, hardcore):
    os.makedirs(server_dir, exist_ok=True)
    with open(os.path.join(server_dir, "eula.txt"), "w") as f:
        f.write("eula=true\n")
    with open(os.path.join(server_dir, "server.properties"), "w") as f:
        f.write("\n".join([
            "online-mode=false", f"server-port={port}", "server-ip=127.0.0.1", f"level-seed={SEED}",
            "difficulty=normal", f"hardcore={str(hardcore).lower()}", "spawn-protection=0", "view-distance=4",
            "simulation-distance=4", "white-list=false", "region-file-compression=deflate",
        ]) + "\n")


def write_datapack(world):
    pack = os.path.join(world, "datapacks", "fixture")
    os.makedirs(os.path.join(pack, "data", "fixture", "function"))
    with open(os.path.join(pack, "pack.mcmeta"), "w") as f:
        json.dump({"pack": {"description": "PEX world fixture", "min_format": 121, "max_format": 121}}, f)
    with open(os.path.join(pack, "data", "fixture", "function", "scheduled.mcfunction"), "w") as f:
        f.write("say fixture scheduled function ran\n")


def start(java, server_jar, server_dir, out, world_name):
    server = Server([java, "-Xmx3G", "-jar", os.path.abspath(server_jar), "--universe", out, "--world", world_name,
                     "nogui"], server_dir)
    server.wait_for(re.compile(r"Done \("), 600)
    return server


def block_entity_position(kind):
    i = [be for be, _ in BLOCK_ENTITIES].index(kind)
    return 2 + 3 * (i % 13), 2 + 3 * (i // 13)


def build_lab(console, entity_types):
    x0, z0, x1, z1 = LAB
    console.run(f"forceload add {x0} {z0} {x1} {z1}")
    for cx in range(x0 // 16, x1 // 16 + 1):
        for cz in range(z0 // 16, z1 // 16 + 1):
            console.wait_until(f"execute if loaded {cx * 16} {Y} {cz * 16}", 120)
    fill(console, x0, Y - 1, z0, x1, LAB_TOP, z1, "air")
    fill(console, x0, Y - 1, z0, x1, Y - 1, z1, "glass")

    for kind, block in BLOCK_ENTITIES:
        x, z = block_entity_position(kind)
        if kind in NEEDS_SUPPORT_ABOVE:
            console.run(f"setblock {x} {Y + 1} {z} glass")
        console.run(f"setblock {x} {Y} {z} {block}")
    # Block events and block entities wait while the game is frozen; one tick moves the
    # powered piston halfway.
    x, z = block_entity_position("piston")
    console.run(f"setblock {x} {Y + 1} {z} stone")
    console.run(f"setblock {x + 1} {Y} {z} redstone_block")
    console.run("tick step 1")
    console.wait_until(f"execute if block {x} {Y + 2} {z} moving_piston", 10)
    for i, stack in enumerate(SHOWCASE):
        x, z = block_entity_position("chest" if i < 27 else "trapped_chest")
        console.run(f"item replace block {x} {Y} {z} container.{i % 27} with {stack.argument}")
    x, z = block_entity_position("barrel")
    console.run(f"loot insert {x} {Y} {z} loot minecraft:chests/shipwreck_map")

    for poi in POI_BLOCKS:
        for x, z, block in zip(poi[1::3], poi[2::3], poi[3::3]):
            console.run(f"setblock {x} {Y} {z} {block}")
    # A nether portal: obsidian frame at x 62..65, z 10, lit with fire.
    fill(console, 62, Y, 10, 65, Y + 4, 10, "obsidian")
    fill(console, 63, Y + 1, 10, 64, Y + 3, 10, "air")
    console.run(f"setblock 63 {Y + 1} 10 fire")
    console.wait_until(f"execute if block 63 {Y + 1} 10 nether_portal", 10)

    # Long block ticks: frogspawn and a sniffer egg, each on its own. Water and lava cups
    # leave fluid ticks.
    for x, liquid in ((68, "water"), (74, "lava")):
        fill(console, x - 1, Y, 1, x + 1, Y, 3, "glass")
        console.run(f"setblock {x} {Y} 2 {liquid}")
    console.run(f"setblock 68 {Y + 1} 2 frogspawn")
    console.run(f"setblock 71 {Y} 2 sniffer_egg")
    console.run(f"setblock 77 {Y} 2 turtle_egg[eggs=3]")

    # Water mobs live in a glass tank at x 66..90, z 6..12.
    fill(console, 66, Y, 6, 90, Y + 3, 12, "glass")
    fill(console, 67, Y, 7, 89, Y + 2, 11, "water")
    # A wall for paintings and item frames, and a fence for a leash.
    fill(console, 2, Y, WALL_Z, 20, Y + 3, WALL_Z, "stone")
    console.run(f"setblock 30 {Y} 58 oak_fence")

    def summon(name, x, y, z, extra=None, info=None, label=None):
        info = info or entity_types[name]
        extra = ENTITY_EXTRA.get(name, "") if extra is None else extra
        if name in BLOCK_ATTACHED:
            pos = f"Pos:[{float(x)}d,{float(y)}d,{float(z)}d]"
            extra = ",".join(filter(None, [extra, block_pos(x, y, z), pos]))
        console.run(f"summon minecraft:{name} {x} {y} {z} {summon_nbt(label or name, info, extra)}")

    tank = [t for t, info in entity_types.items() if info["category"] in WATER_CATEGORIES or t in WATER_MOBS]
    land = [t for t in entity_types if t not in HANGING and t not in tank]
    for i, name in enumerate(land):
        summon(name, f"{2 + 4 * (i % 23)}.5", Y, f"{20 + 4 * (i // 23)}.5")
    for i, name in enumerate(tank):
        summon(name, f"{68 + 1.6 * i:.1f}", Y + 1, 9.5)
    for name, x in HANGING.items():
        summon(name, x, Y + 1, WALL_Z - 1)

    mob, thing = {"mob": True}, {"mob": False}
    rides = [
        ("spider", ["skeleton"]), ("chicken", ["zombie"]), ("strider", ["zombified_piglin"]),
        ("camel", ["villager", "pig"]), ("minecart", ["villager"]), ("oak_boat", ["cat"]),
    ]
    for i, (vehicle, riders) in enumerate(rides):
        extra = {"zombie": "IsBaby:1b"}
        passengers = ",".join(f'{{id:"minecraft:{r}",{summon_nbt(f"{r} rider", mob, extra.get(r, ""))[1:]}'
                              for r in riders)
        summon(vehicle, f"{4 + 6 * i}.5", Y, 50.5, f"Passengers:[{passengers}]",
               thing if vehicle in ("minecart", "oak_boat") else mob, f"{vehicle} vehicle")
    summon("cow", 31.5, Y, 56.5, f"leash:[I;30,{Y},58]", mob, "leashed cow")
    summon("birch_boat", 40.5, Y, 56.5, "UUID:[I;7,7,7,7]", thing, "leash holder boat")
    summon("mule", 43.5, Y, 56.5, "leash:{UUID:[I;7,7,7,7]}", mob, "leashed mule")


def build_village(console):
    """A bell east of the lab that a villager claims as its meeting point.

    Vanilla counts a village POI only once a villager claims it (PoiManager.isVillageCenter),
    and Bad Omen only turns into Raid Omen in a village, so this runs before the freeze.
    The keeper then gets NoAI like the other mobs, which keeps the bell claimed.
    """
    x, y, z = VILLAGE
    console.run(f"forceload add {x} {z}")
    console.wait_until(f"execute if loaded {x} {y} {z}", 120)
    fill(console, x - 6, y - 1, z - 4, x + 6, y + 4, z + 6, "air")
    fill(console, x - 6, y - 1, z - 4, x + 6, y - 1, z + 6, "glass")
    console.run(f"setblock {x} {y} {z} bell[attachment=floor]")
    console.run(f'summon minecraft:villager {x}.5 {y} {z + 3}.5 {{CustomName:"Fixture village keeper",'
                'Tags:["fixture","keeper"],Silent:1b,PersistenceRequired:1b}')
    selector = "@e[type=minecraft:villager,tag=keeper,limit=1]"
    console.wait_until(f'execute if data entity {selector} Brain.memories."minecraft:meeting_point"', 60)
    console.run(f"data merge entity {selector} {{NoAI:1b}}")


def world_data(console):
    console.run("scoreboard objectives add fixture_dummy dummy {text:\"Fixture\",color:\"gold\"}")
    console.run("scoreboard objectives add fixture_deaths deathCount")
    console.run("scoreboard objectives setdisplay sidebar fixture_dummy")
    console.run(f"scoreboard players set {BOT_NAME} fixture_dummy 42")
    console.run("scoreboard players set #global fixture_dummy 7")
    console.run("team add fixture_red {text:\"Red team\"}")
    console.run("team modify fixture_red color red")
    console.run("team modify fixture_red friendlyFire false")
    console.run(f"team join fixture_red {BOT_NAME}")
    console.run("team join fixture_red @e[type=minecraft:zombie,tag=fixture]")
    console.run("bossbar add fixture:bar {text:\"Fixture bar\"}")
    console.run("bossbar set fixture:bar color red")
    console.run("bossbar set fixture:bar max 200")
    console.run("bossbar set fixture:bar value 50")
    console.run("bossbar set fixture:bar style notched_10")
    console.run("schedule function fixture:scheduled 1000000t append")
    console.run("data modify storage fixture:main payload set value {flag:1b,list:[1,2,3],text:\"hello\",nested:{d:1.5d}}")
    console.run("random value 1..100 fixture:sequence")
    console.run("stopwatch create fixture:watch")
    console.run("worldborder center 41 7")
    console.run("worldborder set 4000")
    console.run("worldborder warning distance 12")
    console.run("worldborder damage amount 0.5")
    console.run("setworldspawn " + " ".join(map(str, WORLD_SPAWN)))
    for dimension in ("minecraft:the_nether", "minecraft:the_end"):
        console.run(f"execute in {dimension} run forceload add 0 0")
    console.run("execute in minecraft:the_nether run worldborder set 2000")
    fill(console, -2, 100, -2, 2, 100, 2, "obsidian", "minecraft:the_nether")
    fill(console, -2, 101, -2, 2, 104, 2, "air", "minecraft:the_nether")


def bot_before_freeze(console, bot_uuid):
    """What needs the game running: a death, a blast and the shoulder parrots."""
    x, y, z = BOT_SPOT
    console.run(f"spawnpoint {BOT_NAME} {math.floor(x)} {y} {math.floor(z)}")
    # A player can't be hurt until its client has loaded, so kill until it dies. The bot
    # respawns by itself, and the new player keeps LastDeathLocation.
    deadline = time.time() + 30
    while not console.test(f"execute if data entity {BOT_NAME} LastDeathLocation"):
        if time.time() > deadline:
            raise TimeoutError(f"{BOT_NAME} did not die from /kill within 30 s")
        console.output(f"kill {BOT_NAME}")
        time.sleep(1)
    console.wait_until(f"execute if entity @a[name={BOT_NAME},nbt={{Health:20.0f}}]", 30)
    console.run(f"tp {BOT_NAME} {x} {y} {z}")
    # A wind charge bursting on the floor next to the bot sets last_explosion_impact_pos,
    # which vanilla clears once the player falls; the bot stands still.
    wind = summon_nbt("wind charge", {"mob": False}, "Motion:[0.0d,-1.0d,0.0d]")
    console.run(f"summon minecraft:wind_charge {x + 1} {y + 1} {z} {wind}")
    console.wait_until(f"data get entity {BOT_NAME} last_explosion_impact_pos", 10)
    # A tamed parrot lands on its owner's shoulder after 100 ticks if it touches the owner,
    # so each one goes into a glass box with the bot. Damage, a fall or water would drop
    # it, and none follows.
    bx, bz = math.floor(x), math.floor(z)
    console.run(f"fill {bx - 1} {y - 1} {bz - 1} {bx + 1} {y + 2} {bz + 1} glass hollow")
    owner = "Owner:[I;" + ",".join(map(str, uuid_ints(bot_uuid))) + "]"
    for variant, shoulder in ((1, "ShoulderEntityLeft"), (2, "ShoulderEntityRight")):
        console.run(f'summon minecraft:parrot {x} {y} {z} {{CustomName:"Fixture shoulder parrot {variant}",'
                    f'Tags:["fixture"],Silent:1b,PersistenceRequired:1b,{owner},Variant:{variant}}}')
        console.wait_until(f"data get entity {BOT_NAME} {shoulder}", 60)


def bot_steps(console, bot_uuid):
    x, y, z = BOT_SPOT
    showcase = {stack.item_id: stack for stack in SHOWCASE}
    helmet = item("diamond_helmet", enchantments='{"minecraft:protection":4}')
    for target, stack in (("weapon.mainhand", showcase["diamond_sword"]), ("hotbar.1", showcase["bundle"]),
                          ("armor.head", helmet), ("weapon.offhand", showcase["shield"]),
                          ("enderchest.0", showcase["potion"]), ("enderchest.13", showcase["shulker_box"])):
        console.run(f"item replace entity {BOT_NAME} {target} with {stack.argument}")
    console.run(f"effect give {BOT_NAME} minecraft:speed 100000 1")
    console.run(f"xp add {BOT_NAME} 30 levels")
    console.run(f"recipe give {BOT_NAME} *")
    console.run(f"advancement grant {BOT_NAME} only minecraft:story/root")
    console.run(f"advancement grant {BOT_NAME} only minecraft:adventure/kill_all_mobs minecraft:zombie")
    console.run(f"posteffect add {BOT_NAME} minecraft:invert")
    console.run(f"bossbar set fixture:bar players {BOT_NAME}")
    pearl = summon_nbt("owned pearl", {"mob": False}, "Owner:[I;" + ",".join(map(str, uuid_ints(bot_uuid))) + "]")
    console.run(f"summon minecraft:ender_pearl {x} {y + 3} {z} {pearl}")
    # Bad Omen in a village turns into Raid Omen with raid_omen_position; 30 s later the raid
    # starts and the position is cleared. A second Bad Omen sets it again for the save.
    console.run(f"effect give {BOT_NAME} minecraft:bad_omen 60 0")
    console.wait_until(f"data get entity {BOT_NAME} raid_omen_position", 10)
    console.wait_until(f"data get entity {BOT_NAME} raid_omen_position", 45, want=False)
    console.run(f"effect give {BOT_NAME} minecraft:bad_omen 60 0")
    console.wait_until(f"data get entity {BOT_NAME} raid_omen_position", 10)
    nx, ny, nz = NETHER_STATION
    console.run(f"execute in minecraft:the_nether run tp {BOT_NAME} {nx} {ny} {nz}")
    console.wait_until(f"data get entity {BOT_NAME} entered_nether_pos", 10)


def build_hardcore(java, server_jar, server_dir, out, port):
    write_server_files(server_dir, port, hardcore=True)
    server = start(java, server_jar, server_dir, out, "world-hardcore")
    try:
        Console(server).run("save-all flush")
    finally:
        server.stop()
    os.replace(os.path.join(server_dir, "console.log"), os.path.join(server_dir, "console-hardcore.log"))


def build(java, server_jar, bot, out, port=25880):
    """Builds the fixture in out (must be empty or missing) and returns the manifest."""
    if os.path.exists(out) and os.listdir(out):
        raise RuntimeError(f"{out} is not empty")
    out = os.path.abspath(out)
    server_dir, world = os.path.join(out, "server"), os.path.join(out, "world")
    with open(ENTITIES_JSON) as f:
        entity_types = {name: info for name, info in json.load(f).items()
                        if info["summonable"] and name not in NOT_SAVED}
    write_server_files(server_dir, port, hardcore=False)
    write_datapack(world)
    bot_uuid = offline_uuid(BOT_NAME)
    server = start(java, server_jar, server_dir, out, "world")
    console = Console(server)
    bot_proc = None
    try:
        # wandering_trader.dat is first written after 1200 ticks of the trader spawner.
        if not any("Sprint completed" in line for line in console.output("tick sprint 1300")):
            server.wait_for(re.compile(r"Sprint completed"), 120)
        for rule, value in GAME_RULES.items():
            console.run(f"gamerule {rule} {value}")
        console.run("time set 18000")
        console.run("weather clear 1000000")
        build_village(console)
        with open(os.path.join(server_dir, "bot.log"), "w") as bot_log:
            bot_proc = subprocess.Popen([sys.executable, bot, "127.0.0.1", str(port), BOT_NAME, "900", "afk",
                                         *map(str, BOT_SPOT)], stdout=bot_log, stderr=subprocess.STDOUT)
        server.wait_for(re.compile(re.escape(BOT_NAME) + r" joined the game"), 120)
        bot_before_freeze(console, bot_uuid)
        console.run("tick freeze")
        build_lab(console, entity_types)
        world_data(console)
        bot_steps(console, bot_uuid)
        console.run("save-all flush")
    finally:
        if bot_proc:
            bot_proc.terminate()
            bot_proc.wait()
            time.sleep(2)
        server.stop()
    os.replace(os.path.join(server_dir, "console.log"), os.path.join(server_dir, "console-world.log"))
    build_hardcore(java, server_jar, server_dir, out, port)
    for name in ("libraries", "versions"):
        shutil.rmtree(os.path.join(server_dir, name), ignore_errors=True)
    manifest = make_manifest(out, entity_types, bot_uuid)
    manifest["command_failures"] = [{"command": c, "output": lines} for c, lines in console.failures]
    if console.failures:
        manifest["missing"]["commands"] = [c for c, _ in console.failures]
    with open(os.path.join(out, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    return manifest


# ---- manifest -----------------------------------------------------------------------------

def in_lab(x, z):
    return LAB[0] <= x <= LAB[2] and LAB[1] <= z <= LAB[3]


def item_stacks(value, path):
    """Yields (path, stack) for every item stack inside an NBT value."""
    if isinstance(value, dict):
        if isinstance(value.get("id"), str) and "count" in value:
            yield path, value
        for key, child in value.items():
            yield from item_stacks(child, f"{path}.{nbt.snbt_key(key)}")
    elif isinstance(value, list):
        for i, child in enumerate(value):
            yield from item_stacks(child, f"{path}[{i}]")


def describe_stack(where, path, stack):
    components = sorted(stack.get("components", {}))
    return f"{where} {path}: {stack['id']} x{stack['count']}" + (f" [{', '.join(components)}]" if components else "")


def entity_tree(entity):
    yield entity
    for rider in entity.get("Passengers", []):
        yield from entity_tree(rider)


def short(value, limit=120):
    text = nbt.to_snbt(value)
    return text if len(text) <= limit else text[:limit] + "..."


def data_entries(world, rel):
    path = os.path.join(world, rel)
    if not os.path.exists(path):
        return []
    root = nbt.read_file(path)[1]
    data = root.get("data", root)
    return [f"{rel}: {nbt.snbt_key(k)} = {short(v)}" for k, v in data.items()]


def make_manifest(out, entity_types, bot_uuid):
    world = os.path.join(out, "world")
    rows = {row: [] for row in ROWS}
    found_be, found_entities, found_poi, chunk_keys = set(), set(), set(), {}
    for dimension in ("overworld", "the_nether", "the_end"):
        base = os.path.join(world, "dimensions", "minecraft", dimension)
        for kind in ("region", "entities", "poi"):
            folder = os.path.join(base, kind)
            files = sorted(f for f in os.listdir(folder) if f.endswith(".mca")) if os.path.isdir(folder) else []
            for name in files:
                rel = os.path.relpath(os.path.join(folder, name), world)
                for (cx, cz), (_, chunk) in nbt.read_region(os.path.join(folder, name)).items():
                    where = f"{rel} chunk {cx},{cz}"
                    if kind == "region":
                        for key in chunk:
                            chunk_keys[key] = chunk_keys.get(key, 0) + 1
                        for start in chunk.get("structures", {}).get("starts", {}):
                            rows[ROWS[0]].append(f"{where}: structures.starts.{nbt.snbt_key(start)}")
                        if chunk.get("structures", {}).get("References"):
                            refs = ", ".join(chunk["structures"]["References"])
                            rows[ROWS[0]].append(f"{where}: structures.References {refs}")
                        lab = in_lab(cx * 16, cz * 16)
                        for tick in chunk.get("block_ticks", []) if lab else []:
                            rows[ROWS[0]].append(f"{where}: block_ticks {tick['i']} at {tick['x']},{tick['y']},{tick['z']} "
                                                 f"t={tick['t']}")
                        if lab and chunk.get("fluid_ticks"):
                            rows[ROWS[0]].append(f"{where}: fluid_ticks x{len(chunk['fluid_ticks'])}")
                        for be in chunk.get("block_entities", []) if lab else []:
                            found_be.add(be["id"].split(":", 1)[1])
                            keys = sorted(k for k in be if k not in ("id", "x", "y", "z"))
                            rows[ROWS[2]].append(f"{where}: {be['id']} at {be['x']},{be['y']},{be['z']} [{', '.join(keys)}]")
                            for path, stack in item_stacks(be, "block_entities[{x:%d,y:%d,z:%d}]" % (be["x"], be["y"], be["z"])):
                                rows[ROWS[3]].append(describe_stack(where, path, stack))
                    elif kind == "entities":
                        for entity in chunk["Entities"]:
                            for e in entity_tree(entity):
                                if "fixture" not in e.get("Tags", []):
                                    continue
                                found_entities.add(e["id"].split(":", 1)[1])
                                extras = [k for k in ("Passengers", "leash", "attributes") if k in e]
                                pos = ",".join(f"{v:.1f}" for v in e["Pos"])
                                rows[ROWS[1]].append(f"{where}: {e['id']} {short(e.get('CustomName', ''))} at {pos}"
                                                     + (f" [{', '.join(extras)}]" if extras else ""))
                            uuid_path = "Entities[{UUID:%s}]" % nbt.to_snbt(entity["UUID"])
                            for path, stack in item_stacks(entity, uuid_path):
                                rows[ROWS[3]].append(describe_stack(where, path, stack))
                    else:
                        for section_y, section in chunk.get("Sections", {}).items():
                            for record in section.get("Records", []):
                                found_poi.add(record["type"].split(":", 1)[1])
                                rows[ROWS[4]].append(f"{where}: Sections.{section_y} {record['type']} at {short(record['pos'])}")
    rows[ROWS[0]].insert(0, "chunk keys (chunks holding each): " + ", ".join(f"{k} {n}" for k, n in sorted(chunk_keys.items())))

    level = nbt.read_file(os.path.join(world, "level.dat"))[1]["Data"]
    rows[ROWS[5]] += [f"level.dat: Data.{nbt.snbt_key(k)} = {short(v)}" for k, v in level.items()]
    hardcore = nbt.read_file(os.path.join(out, "world-hardcore", "level.dat"))[1]["Data"]
    rows[ROWS[5]].append("world-hardcore/level.dat: Data.difficulty_settings.hardcore = "
                         + short(hardcore.get("difficulty_settings", {}).get("hardcore")))
    for row, names in ((ROWS[6], ("game_rules", "world_gen_settings", "world_clocks")),
                       (ROWS[7], ("weather", "wandering_trader")),
                       (ROWS[8], ("scoreboard", "custom_boss_events", "scheduled_events", "random_sequences", "stopwatches"))):
        for name in names:
            rows[row] += data_entries(world, f"data/minecraft/{name}.dat")
    data_dir = os.path.join(world, "data")
    for dirpath, _, files in os.walk(data_dir):
        for name in sorted(files):
            rel = os.path.relpath(os.path.join(dirpath, name), world)
            if "/maps/" in rel or name.startswith("command_storage"):
                rows[ROWS[9]] += data_entries(world, rel)
    for dimension in ("overworld", "the_nether", "the_end"):
        for name in ("world_border", "chunk_tickets", "raids", "ender_dragon_fight"):
            rows[ROWS[10]] += data_entries(world, f"dimensions/minecraft/{dimension}/data/minecraft/{name}.dat")

    player_rel = f"players/data/{bot_uuid}.dat"
    missing_player_keys = []
    if os.path.exists(os.path.join(world, player_rel)):
        player = nbt.read_file(os.path.join(world, player_rel))[1]
        rows[ROWS[11]] += [f"{player_rel}: {nbt.snbt_key(k)} = {short(v, 80)}" for k, v in player.items()]
        missing_player_keys = [k for k in PLAYER_KEYS if k not in player]
        for path, stack in item_stacks(player, ""):
            rows[ROWS[3]].append(describe_stack(player_rel, path.lstrip("."), stack))
    for row, rel in ((ROWS[12], f"players/advancements/{bot_uuid}.json"), (ROWS[13], f"players/stats/{bot_uuid}.json")):
        if os.path.exists(os.path.join(world, rel)):
            with open(os.path.join(world, rel)) as f:
                data = json.load(f)
            for key, value in (data.get("stats", data)).items():
                rows[row].append(f"{rel}: {key} = {json.dumps(value)[:100]}")

    missing = {
        "rows without entries": [row for row in ROWS if not rows[row]],
        "block entity types": sorted({be for be, _ in BLOCK_ENTITIES} - found_be),
        "entity types": sorted(set(entity_types) - found_entities),
        "POI types": sorted(set(POI_TYPES) - found_poi),
    }
    return {
        "seed": SEED, "world": "world", "hardcore_world": "world-hardcore", "lab": {"min": [LAB[0], Y - 1, LAB[1]],
                                                                                  "max": [LAB[2], LAB_TOP, LAB[3]]},
        "player": {"name": BOT_NAME, "uuid": str(bot_uuid), "keys vanilla saves that are absent": missing_player_keys},
        "rows": rows, "missing": {k: v for k, v in missing.items() if v},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--java", required=True, help="java 25 for the vanilla server")
    parser.add_argument("--server-jar", required=True, help="vanilla 26.3 server jar (the bundler from Mojang)")
    parser.add_argument("--bot", required=True, help="survbot.py, the protocol 777 bot from pex-dev's tools")
    parser.add_argument("--out", required=True, help="empty or missing directory for the world and manifest")
    parser.add_argument("--port", type=int, default=25880)
    args = parser.parse_args()
    if os.path.exists(args.out) and os.listdir(args.out):
        parser.error(f"{args.out} is not empty")
    try:
        manifest = build(args.java, args.server_jar, args.bot, args.out, args.port)
    except (RuntimeError, TimeoutError) as error:
        print(f"fixture not built: {error}; see {os.path.join(args.out, 'server')}")
        return 1
    for row in ROWS:
        print(f"{len(manifest['rows'][row]):5}  {row}")
    for kind, names in manifest["missing"].items():
        print(f"missing {kind}: {', '.join(names)}")
    print(f"manifest: {os.path.join(args.out, 'manifest.json')}")
    return 1 if manifest["missing"] else 0


if __name__ == "__main__":
    sys.exit(main())
