"""Run a PEX test server and feed it console commands from a file.

Usage: python server_driver.py <pumpkin.exe> <server-dir> <cmds.txt> <log.txt>

The commands file is truncated at start. Append lines to it (one command per line) and they are
sent to the server's stdin within half a second. All server output goes to the log file, and
the server's exit code is appended to the log when it stops. Send `stop` to shut it down.
"""
import subprocess, sys, time, os, threading
exe, cwd, cmds, log = sys.argv[1:5]
open(cmds, 'w').close()
with open(log, 'wb') as out:
    p = subprocess.Popen([exe], cwd=cwd, stdin=subprocess.PIPE, stdout=out, stderr=subprocess.STDOUT)
    pos = 0
    while p.poll() is None:
        with open(cmds, 'rb') as f:
            f.seek(pos)
            data = f.read()
            pos += len(data)
        if data:
            p.stdin.write(data); p.stdin.flush()
        time.sleep(0.5)
    out.write(f"\n[driver] server exit code {p.returncode}\n".encode())
