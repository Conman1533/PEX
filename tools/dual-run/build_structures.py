#!/usr/bin/env python3
"""Build GameTest structures in vanilla and save them into a datapack.

Each structure has a build file, `<structures dir>/<path>.build`, holding its size and
the vanilla commands that build it. This script starts vanilla in a scratch directory,
runs the commands at a fixed origin, waits for redstone to settle and saves the area
the way the structure block screen does (a bot sends ServerboundSetStructureBlockPacket
with SAVE_AREA). The saved file is copied to
`<datapack>/data/<namespace>/structure/<path>.nbt`. Block states and block entities
are vanilla's own settled ones: test structures are placed without block updates, so
the file must already hold the state vanilla would reach.

    python3 tools/dual-run/build_structures.py \\
        --java <jdk 25>/bin/java --server-jar <vanilla 26.3 server jar> \\
        --structures tools/dual-run/structures \\
        --datapack assets/tests/datapacks/pumpkin-dual-run --work <scratch dir> [path ...]

Build files: `# size: X Y Z` is required, other lines starting with `#` are comments,
and every other line is a command run with `execute positioned <origin> run`, so
`~ ~ ~` is the structure's lowest north-west corner. Keep X and Z at 14 or less so the
test fits pumpkin-tester's 16 block grid with its barrier shell. The test instance
JSON in `<datapack>/data/<namespace>/test_instance/` is written by hand.
"""
import argparse
import gzip
import io
import os
import queue
import re
import shutil
import socket
import struct
import subprocess
import sys
import threading
import time
import uuid
import zlib

PROTOCOL = 777
BOT_NAME = "dualrun_builder"
ORIGIN = (0, -60, 0)
MAX_SIZE = 48
MAX_HORIZONTAL_SIZE = 14
SETTLE_SECONDS = 1.5

# Protocol 777 packet ids.
SB_INTENTION, SB_LOGIN_HELLO, SB_LOGIN_ACK = 0, 0, 3
SB_CFG_CLIENT_INFO, SB_CFG_FINISH, SB_CFG_KEEP_ALIVE, SB_CFG_PONG, SB_CFG_KNOWN_PACKS = 0, 3, 4, 5, 7
SB_PLAY_ACCEPT_TP, SB_PLAY_CHUNK_BATCH, SB_PLAY_CFG_ACK, SB_PLAY_KEEP_ALIVE, SB_PLAY_LOADED = 0, 11, 16, 28, 44
SB_PLAY_SET_STRUCTURE_BLOCK = 60
CB_LOGIN_DISCONNECT, CB_LOGIN_FINISHED, CB_LOGIN_COMPRESSION = 0, 2, 3
CB_CFG_DISCONNECT, CB_CFG_FINISH, CB_CFG_KEEP_ALIVE, CB_CFG_PING, CB_CFG_KNOWN_PACKS = 2, 3, 4, 5, 15
CB_PLAY_CHUNK_BATCH_FINISHED, CB_PLAY_KEEP_ALIVE, CB_PLAY_PLAYER_POSITION, CB_PLAY_START_CFG = 11, 45, 73, 120

# StructureBlockEntity.UpdateType.SAVE_AREA and StructureMode.SAVE ordinals.
UPDATE_SAVE_AREA, MODE_SAVE = 1, 0


def varint(n, bits=32):
    n &= (1 << bits) - 1
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            return out + bytes([b])


def read_varint(stream):
    num = shift = 0
    while True:
        raw = stream.read(1)
        if not raw:
            raise EOFError
        num |= (raw[0] & 0x7F) << shift
        if not raw[0] & 0x80:
            break
        shift += 7
    return num - (1 << 32) if num & 0x80000000 else num


def string(s):
    data = s.encode()
    return varint(len(data)) + data


def read_string(buf):
    return buf.read(read_varint(buf)).decode()


def block_pos(x, y, z):
    return struct.pack(">q", ((x & 0x3FFFFFF) << 38) | ((z & 0x3FFFFFF) << 12) | (y & 0xFFF))


class Bot:
    """Just enough of a protocol 777 client to stay online and save structures."""

    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port))
        self.file = self.sock.makefile("rb")
        self.threshold = -1
        self.lock = threading.Lock()
        self.joined = threading.Event()
        self.error = None
        self._send(SB_INTENTION, varint(PROTOCOL) + string("127.0.0.1") + struct.pack(">H", port) + varint(2))
        player_uuid = uuid.uuid3(uuid.NAMESPACE_DNS, "OfflinePlayer:" + BOT_NAME)
        self._send(SB_LOGIN_HELLO, string(BOT_NAME) + player_uuid.bytes)
        threading.Thread(target=self._run, daemon=True).start()

    def _send(self, pid, payload=b""):
        data = varint(pid) + payload
        if self.threshold >= 0:
            data = varint(len(data)) + zlib.compress(data) if len(data) >= self.threshold else varint(0) + data
        with self.lock:
            self.sock.sendall(varint(len(data)) + data)

    def _recv(self):
        data = self.file.read(read_varint(self.file))
        buf = io.BytesIO(data)
        if self.threshold >= 0 and read_varint(buf):
            buf = io.BytesIO(zlib.decompress(buf.read()))
        return read_varint(buf), buf

    def _run(self):
        try:
            phase = "login"
            while True:
                pid, buf = self._recv()
                if phase == "login":
                    phase = self._login(pid, buf)
                elif phase == "config":
                    phase = self._config(pid, buf)
                else:
                    phase = self._play(pid, buf)
        except Exception as error:  # noqa: BLE001
            self.error = error
            self.joined.set()

    def _login(self, pid, buf):
        if pid == CB_LOGIN_COMPRESSION:
            self.threshold = read_varint(buf)
        elif pid == CB_LOGIN_FINISHED:
            self._send(SB_LOGIN_ACK)
            self._send(SB_CFG_CLIENT_INFO, string("en_us") + bytes([4]) + varint(0) + b"\x01"
                       + bytes([0x7F]) + varint(1) + b"\x00\x01" + varint(0))
            return "config"
        elif pid == CB_LOGIN_DISCONNECT:
            raise RuntimeError(f"login disconnect: {buf.read()[:200]!r}")
        return "login"

    def _config(self, pid, buf):
        if pid == CB_CFG_KNOWN_PACKS:
            packs = [(read_string(buf), read_string(buf), read_string(buf)) for _ in range(read_varint(buf))]
            self._send(SB_CFG_KNOWN_PACKS, varint(len(packs)) + b"".join(string(a) + string(b) + string(c) for a, b, c in packs))
        elif pid == CB_CFG_KEEP_ALIVE:
            self._send(SB_CFG_KEEP_ALIVE, buf.read(8))
        elif pid == CB_CFG_PING:
            self._send(SB_CFG_PONG, buf.read(4))
        elif pid == CB_CFG_FINISH:
            self._send(SB_CFG_FINISH)
            return "play"
        elif pid == CB_CFG_DISCONNECT:
            raise RuntimeError(f"config disconnect: {buf.read()[:200]!r}")
        return "config"

    def _play(self, pid, buf):
        if pid == CB_PLAY_KEEP_ALIVE:
            self._send(SB_PLAY_KEEP_ALIVE, buf.read(8))
        elif pid == CB_PLAY_PLAYER_POSITION:
            teleport_id = read_varint(buf)
            position = buf.read(24)
            buf.read(24)
            rotation = buf.read(8)
            self._send(SB_PLAY_ACCEPT_TP, varint(teleport_id) + position + rotation)
            if not self.joined.is_set():
                self._send(SB_PLAY_LOADED)
                self.joined.set()
        elif pid == CB_PLAY_CHUNK_BATCH_FINISHED:
            self._send(SB_PLAY_CHUNK_BATCH, struct.pack(">f", 25.0))
        elif pid == CB_PLAY_START_CFG:
            self._send(SB_PLAY_CFG_ACK)
            return "config"
        return "play"

    def save_structure(self, pos, name, offset, size):
        """Sends what the structure block screen's SAVE button sends."""
        payload = (block_pos(*pos) + varint(UPDATE_SAVE_AREA) + varint(MODE_SAVE) + string(name)
                   + struct.pack(">bbb", *offset) + struct.pack(">bbb", *size)
                   # mirror NONE, rotation NONE, no metadata, integrity 1, seed 0, no flags
                   + varint(0) + varint(0) + string("") + struct.pack(">f", 1.0) + varint(0, 64) + bytes([0]))
        self._send(SB_PLAY_SET_STRUCTURE_BLOCK, payload)


class Server:
    def __init__(self, argv, cwd):
        self.proc = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, bufsize=1, errors="replace")
        self.lines = queue.Queue()
        self.log = open(os.path.join(cwd, "console.log"), "w")
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.log.write(line)
            self.log.flush()
            self.lines.put(line.rstrip("\n"))
        self.lines.put(None)

    def command(self, text):
        self.proc.stdin.write(text + "\n")
        self.proc.stdin.flush()

    def wait_for(self, pattern, timeout):
        deadline = time.time() + timeout
        regex = re.compile(pattern)
        while True:
            try:
                line = self.lines.get(timeout=max(0.0, deadline - time.time()))
            except queue.Empty:
                raise RuntimeError(f"timed out waiting for {pattern!r}") from None
            if line is None:
                raise RuntimeError("server exited")
            if regex.search(line):
                return line

    def stop(self):
        if self.proc.poll() is None:
            try:
                self.command("stop")
                self.proc.wait(120)
            except (OSError, subprocess.TimeoutExpired):
                self.proc.kill()
                self.proc.wait()
        self.log.close()


def parse_build(path):
    size, commands = None, []
    with open(path) as f:
        for raw in f:
            line = raw.strip()
            if not line:
                continue
            if line.startswith("#"):
                match = re.match(r"#\s*size:\s*(\d+)\s+(\d+)\s+(\d+)\s*$", line)
                if match:
                    size = tuple(int(v) for v in match.groups())
                continue
            commands.append(line)
    if size is None:
        raise ValueError(f"{path}: missing '# size: X Y Z'")
    if max(size) > MAX_SIZE or min(size) < 1:
        raise ValueError(f"{path}: each size must be 1 to {MAX_SIZE}")
    if max(size[0], size[2]) > MAX_HORIZONTAL_SIZE:
        raise ValueError(f"{path}: keep X and Z at {MAX_HORIZONTAL_SIZE} or less")
    return size, commands


def find_builds(structures, selected):
    builds = {}
    for dirpath, _, files in os.walk(structures):
        for name in files:
            if name.endswith(".build"):
                path = os.path.join(dirpath, name)
                builds[os.path.relpath(path, structures)[:-6].replace(os.sep, "/")] = path
    if selected:
        missing = [name for name in selected if name not in builds]
        if missing:
            raise ValueError(f"no build file for {', '.join(missing)}")
        builds = {name: builds[name] for name in selected}
    return {name: parse_build(path) for name, path in sorted(builds.items())}


def write_server_files(work, port):
    if os.path.exists(work):
        shutil.rmtree(work)
    os.makedirs(work)
    with open(os.path.join(work, "eula.txt"), "w") as f:
        f.write("eula=true\n")
    with open(os.path.join(work, "server.properties"), "w") as f:
        f.write("\n".join([
            "online-mode=false", f"server-port={port}", "server-ip=127.0.0.1", "level-name=world",
            "level-type=minecraft\\:flat", "generate-structures=false", "spawn-protection=0",
            "spawn-monsters=false", "white-list=false", "view-distance=4", "simulation-distance=4",
        ]) + "\n")


def build_all(server, bot, builds, work, datapack, namespace):
    ox, oy, oz = ORIGIN
    extent = [max(size[axis] for size, _ in builds.values()) + 2 for axis in range(3)]
    # The structure block sits under the build, outside the saved box.
    block = (ox, oy - 2, oz)
    for name, (size, commands) in builds.items():
        # One layer per fill stays under the fill command's block limit.
        for dy in range(extent[1]):
            server.command(f"fill {ox - 1} {oy + dy} {oz - 1} {ox + extent[0]} {oy + dy} {oz + extent[2]} air")
        server.command("kill @e[type=!player]")
        time.sleep(1.0)
        for command in commands:
            server.command(f"execute positioned {ox} {oy} {oz} run {command}")
        time.sleep(SETTLE_SECONDS)
        server.command(f"setblock {block[0]} {block[1]} {block[2]} structure_block[mode=save]")
        time.sleep(0.5)
        saved = os.path.join(work, "world", "generated", namespace, "structure", name + ".nbt")
        if os.path.exists(saved):
            os.remove(saved)
        bot.save_structure(block, f"{namespace}:{name}", (0, oy - block[1], 0), size)
        deadline = time.time() + 10
        while not os.path.exists(saved) and time.time() < deadline:
            time.sleep(0.2)
        time.sleep(0.5)
        if not os.path.exists(saved):
            raise RuntimeError(f"vanilla did not save {namespace}:{name}, see {work}/console.log")
        with gzip.open(saved) as f:
            f.read()
        target = os.path.join(datapack, "data", namespace, "structure", name + ".nbt")
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copyfile(saved, target)
        print(f"saved {namespace}:{name}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--java", required=True, help="java 25")
    parser.add_argument("--server-jar", required=True, help="vanilla 26.3 server jar (the bundler from Mojang)")
    parser.add_argument("--structures", required=True, help="directory with .build files")
    parser.add_argument("--datapack", required=True, help="datapack directory to write structures into")
    parser.add_argument("--work", required=True, help="scratch directory for the vanilla server")
    parser.add_argument("--namespace", default="pumpkin")
    parser.add_argument("--port", type=int, default=25871)
    parser.add_argument("paths", nargs="*", help="structure paths such as redstone/wire_attenuation (default: all)")
    args = parser.parse_args()

    try:
        builds = find_builds(args.structures, args.paths)
    except ValueError as error:
        parser.error(str(error))
    if not builds:
        parser.error(f"no .build files in {args.structures}")

    work = os.path.abspath(args.work)
    write_server_files(work, args.port)
    server = Server([args.java, "-Xmx2G", "-jar", os.path.abspath(args.server_jar), "nogui"], work)
    try:
        server.wait_for(r"Done \(", 600)
        bot = Bot(args.port)
        if not bot.joined.wait(60) or bot.error:
            raise RuntimeError(f"bot did not join: {bot.error}")
        # Saving from the structure block screen needs an operator in creative mode.
        server.command(f"op {BOT_NAME}")
        server.command(f"gamemode creative {BOT_NAME}")
        server.command(f"forceload add {ORIGIN[0] - 16} {ORIGIN[2] - 16} {ORIGIN[0] + 63} {ORIGIN[2] + 63}")
        time.sleep(2)
        build_all(server, bot, builds, work, args.datapack, args.namespace)
    finally:
        server.stop()


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
