#!/usr/bin/env python3
"""Run a GameTest datapack on vanilla and on PEX and compare the results.

Both servers start in fresh directories under --work with the datapack copied into
world/datapacks. Each runs `test run <selector>` from the console once per top-level
test directory (`pumpkin:redstone/*` and so on), because PEX also has builtin tests
in the `pumpkin` namespace, and reads the summary line after each run. PEX reports
every test on the console. Vanilla only logs failures (LogTestReporter), so on
vanilla a selected test without a failure line counts as passed, and the test count
in each summary line must match the datapack.

    python3 tools/dual-run/dual_run.py \\
        --java <jdk 25>/bin/java --server-jar <vanilla 26.3 server jar> \\
        --pex target/debug/pumpkin \\
        --datapack assets/tests/datapacks/pumpkin-dual-run --work <scratch dir>

--selector runs one selector instead. --reuse-vanilla reads
<work>/vanilla-results.json instead of starting vanilla again when the datapack and
selectors have not changed. --only runs one side.

Exit status: 0 when every selected test has the same result on both servers, 1 when
results differ or a server did not finish, 2 on usage errors.
"""
import argparse
import fnmatch
import hashlib
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import threading
import time

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TEST_ID = r"([a-z0-9_.-]+:[a-z0-9_./-]+)"
SUMMARY = re.compile(r"Game Test complete! (\d+) test\(s\) were run")
SUMMARY_END = re.compile(r"All required tests passed|required test\(s\) failed")
# LogTestReporter.onTestFailed: "{id} failed at {x, y, z}! {error}", logged at ERROR
# (WARN for optional tests).
VANILLA_FAILED = re.compile(r"/(?:ERROR|WARN)\]: (?:\(optional\) )?" + TEST_ID + r" failed at [-0-9, ]+[!.] ?(.*)$")
# ReportGameListener's text, which PEX sends to the command source.
PEX_PASSED = re.compile(TEST_ID + r" passed!")
PEX_FAILED = re.compile(r"(?:\(optional\) )?" + TEST_ID + r" failed! ?(.*)$")
PEX_PACK_ENABLED = re.compile(r"Enabling data pack|is already enabled")
PEX_PACK_UNKNOWN = re.compile(r"Unknown data pack")


def datapack_tests(datapack):
    tests = []
    data = os.path.join(datapack, "data")
    for namespace in sorted(os.listdir(data)):
        root = os.path.join(data, namespace, "test_instance")
        for dirpath, _, files in os.walk(root):
            for name in files:
                if name.endswith(".json"):
                    rel = os.path.relpath(os.path.join(dirpath, name), root)[:-5].replace(os.sep, "/")
                    tests.append(f"{namespace}:{rel}")
    return sorted(tests)


def group_selectors(tests):
    """One selector per top-level test directory, or the id itself for a test at the top."""
    selectors = set()
    for test in tests:
        namespace, path = test.split(":", 1)
        selectors.add(f"{namespace}:{path.split('/', 1)[0]}/*" if "/" in path else test)
    return sorted(selectors)


def datapack_hash(datapack):
    digest = hashlib.sha256()
    for dirpath, dirs, files in os.walk(datapack):
        dirs.sort()
        for name in sorted(files):
            path = os.path.join(dirpath, name)
            digest.update(os.path.relpath(path, datapack).encode())
            with open(path, "rb") as f:
                digest.update(f.read())
    return digest.hexdigest()


class Server:
    """A server process whose console is read line by line and written through stdin."""

    def __init__(self, argv, cwd):
        self.proc = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, bufsize=1, errors="replace")
        self.lines = queue.Queue()
        self.log = open(os.path.join(cwd, "console.log"), "w")
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            line = ANSI.sub("", line.rstrip("\n"))
            self.log.write(line + "\n")
            self.log.flush()
            self.lines.put(line)
        self.lines.put(None)

    def command(self, text):
        self.proc.stdin.write(text + "\n")
        self.proc.stdin.flush()

    def next_line(self, deadline):
        try:
            line = self.lines.get(timeout=max(0.0, deadline - time.time()))
        except queue.Empty:
            raise TimeoutError("timed out") from None
        if line is None:
            raise RuntimeError("server exited")
        return line

    def wait_for(self, regex, timeout):
        deadline = time.time() + timeout
        while True:
            line = self.next_line(deadline)
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


def prepare(work, datapack):
    if os.path.exists(work):
        shutil.rmtree(work)
    pack_name = os.path.basename(os.path.normpath(datapack))
    shutil.copytree(datapack, os.path.join(work, "world", "datapacks", pack_name))
    return pack_name


def collect_results(server, kind, selectors, timeout):
    """Runs the selectors one after another. Returns ({test id: (passed, message)}, test count)."""
    results, total = {}, 0
    for selector in selectors:
        total += run_selector(server, kind, selector, timeout, results)
    return results, total


def run_selector(server, kind, selector, timeout, results):
    count = None
    server.command(f"test run {selector}")
    deadline = time.time() + timeout
    while True:
        line = server.next_line(deadline)
        if count is None:
            match = SUMMARY.search(line)
            if match:
                count = int(match.group(1))
                continue
        elif SUMMARY_END.search(line):
            return count
        if kind == "vanilla":
            match = VANILLA_FAILED.search(line)
            if match:
                results[match.group(1)] = (False, match.group(2).strip())
        else:
            match = PEX_FAILED.search(line)
            if match:
                results[match.group(1)] = (False, match.group(2).strip())
                continue
            match = PEX_PASSED.search(line)
            if match:
                results[match.group(1)] = (True, "")


def run_vanilla(args, work, port):
    prepare(work, args.datapack)
    with open(os.path.join(work, "eula.txt"), "w") as f:
        f.write("eula=true\n")
    with open(os.path.join(work, "server.properties"), "w") as f:
        f.write("\n".join([
            "online-mode=false", f"server-port={port}", "server-ip=127.0.0.1", "level-name=world",
            "level-type=minecraft\\:flat", "generate-structures=false", "white-list=false",
            "spawn-monsters=false", "view-distance=4", "simulation-distance=4",
        ]) + "\n")
    server = Server([args.java, "-Xmx2G", "-jar", os.path.abspath(args.server_jar), "nogui"], work)
    try:
        server.wait_for(re.compile(r"Done \("), 600)
        return collect_results(server, "vanilla", args.selectors, args.timeout)
    finally:
        server.stop()


def run_pex(args, work, port):
    pack_name = prepare(work, args.datapack)
    with open(os.path.join(work, "pumpkin.toml"), "w") as f:
        f.write("\n".join([
            "[networking.java]", f'address = "127.0.0.1:{port}"', "online_mode = false", "",
            "[networking.bedrock]", "enabled = false", "",
            "[plugins]", "enabled = false", "",
            "[telemetry]", "enabled = false", "",
        ]))
    server = Server([os.path.abspath(args.pex)], work)
    try:
        server.wait_for(re.compile(r"Started server"), 600)
        # Vanilla enables new packs in world/datapacks when the server starts. PEX only
        # loads packs that level.dat lists as enabled (or embedded pack names).
        server.command(f'datapack enable "file/{pack_name}"')
        line = server.wait_for(re.compile(f"{PEX_PACK_ENABLED.pattern}|{PEX_PACK_UNKNOWN.pattern}"), 120)
        if PEX_PACK_UNKNOWN.search(line):
            raise RuntimeError(f"PEX does not see the datapack: {line}")
        return collect_results(server, "pex", args.selectors, args.timeout)
    finally:
        server.stop()


def run_side(args, kind, selected, pack_hash):
    cache = os.path.join(args.work, f"{kind}-results.json")
    if kind == "vanilla" and args.reuse_vanilla and os.path.exists(cache):
        with open(cache) as f:
            cached = json.load(f)
        if cached["datapack"] == pack_hash and cached["selectors"] == args.selectors:
            print(f"vanilla: reusing {cache}", flush=True)
            return cached
    if kind == "vanilla" and not (args.java and args.server_jar):
        raise RuntimeError(f"{cache} is for another datapack or selector, pass --java and --server-jar")
    print(f"{kind}: running {len(selected)} test(s)", flush=True)
    runner, port = (run_vanilla, args.port) if kind == "vanilla" else (run_pex, args.port + 1)
    results, count = runner(args, os.path.join(args.work, kind), port)
    side = {"datapack": pack_hash, "selectors": args.selectors, "count": count, "results": {}}
    for test in selected:
        if test in results:
            passed, message = results[test]
        elif kind == "vanilla":
            passed, message = True, ""
        else:
            passed, message = False, "no result reported"
        side["results"][test] = {"passed": passed, "message": message}
    with open(cache, "w") as f:
        json.dump(side, f, indent=2)
    return side


def report(sides, selected):
    ok = True
    for kind, side in sides.items():
        if side["count"] != len(selected):
            print(f"{kind}: ran {side['count']} test(s), the datapack has {len(selected)}")
            ok = False
    width = max(len(test) for test in selected)
    print(f"{'test':<{width}}  " + "  ".join(f"{kind:<7}" for kind in sides))
    for test in selected:
        cells = ["pass" if side["results"][test]["passed"] else "FAIL" for side in sides.values()]
        same = len(set(cells)) == 1
        ok &= same
        print(f"{test:<{width}}  " + "  ".join(f"{cell:<7}" for cell in cells) + ("" if same else "  <- differs"))
        for kind, side in sides.items():
            result = side["results"][test]
            if not result["passed"] and result["message"]:
                print(f"    {kind}: {result['message']}")
    if len(sides) == 2:
        print("results match" if ok else "results differ")
    return ok


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--pex", help="PEX (pumpkin) binary")
    parser.add_argument("--java", help="java 25 for the vanilla server")
    parser.add_argument("--server-jar", help="vanilla 26.3 server jar (the bundler from Mojang)")
    parser.add_argument("--datapack", required=True, help="datapack directory")
    parser.add_argument("--work", required=True, help="scratch directory for both servers")
    parser.add_argument("--selector", help="run this test selector instead of one per directory")
    parser.add_argument("--timeout", type=float, default=300, help="seconds to wait for the test summary")
    parser.add_argument("--port", type=int, default=25873, help="vanilla port; PEX uses the next one")
    parser.add_argument("--only", choices=["vanilla", "pex"])
    parser.add_argument("--reuse-vanilla", action="store_true")
    args = parser.parse_args()

    if args.only != "pex" and not (args.reuse_vanilla or (args.java and args.server_jar)):
        parser.error("--java and --server-jar are needed to run vanilla")
    if args.only != "vanilla" and not args.pex:
        parser.error("--pex is needed to run PEX")
    tests = datapack_tests(args.datapack)
    selected = [test for test in tests if not args.selector or fnmatch.fnmatchcase(test, args.selector)]
    if not selected:
        parser.error(f"no test instance in {args.datapack} matches {args.selector}")
    args.selectors = [args.selector] if args.selector else group_selectors(selected)
    pack_hash = datapack_hash(args.datapack)
    os.makedirs(args.work, exist_ok=True)

    sides = {}
    for kind in ("vanilla", "pex"):
        if args.only and args.only != kind:
            continue
        try:
            sides[kind] = run_side(args, kind, selected, pack_hash)
        except (RuntimeError, TimeoutError) as error:
            print(f"{kind}: did not finish ({error}); see {os.path.join(args.work, kind, 'console.log')}")
            return 1
    return 0 if report(sides, selected) else 1


if __name__ == "__main__":
    sys.exit(main())
