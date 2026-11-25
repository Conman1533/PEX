#!/usr/bin/env python3
"""Bedrock packet coverage: every packet gophertunnel knows at the current protocol, next to what
PEX has for it.

    tools/bedrock-coverage.py --gophertunnel ../gophertunnel [--out docs/BEDROCK-COVERAGE.md]
    tools/bedrock-coverage.py --check docs/BEDROCK.md

gophertunnel (https://github.com/Sandertv/gophertunnel) is the reference: its packet ids and its
lists of server- and client-originating packets. For each packet the table says whether PEX can
encode it (a clientbound struct in pumpkin-protocol), decode it (a serverbound struct), and use it
(the clientbound struct is built somewhere in pumpkin-core, or the serverbound one has a match arm
there). The second table lists every call site in pumpkin-core of a helper that reaches Java clients
only. `--check` compares those call sites with the `path::function` keys in docs/BEDROCK.md and
prints what is missing from the document or no longer in the code; it prints nothing when they
agree. Run it from the repository root. Standard library only.
"""

import argparse
import re
import sys
from pathlib import Path

PROTOCOL_DIRS = ["crates/pumpkin-protocol/src/bedrock", "crates/pumpkin-core/src/net/bedrock"]
CORE_DIR = "crates/pumpkin-core/src"
# Client-only packets that gophertunnel's server-originating list still holds (it has a TODO to
# prune that list).
CLIENT_ONLY = {
    "Login",
    "PlayerAuthInput",
    "ResourcePackClientResponse",
    "SetLocalPlayerAsInitialised",
    "CommandRequest",
    "ModalFormResponse",
    "PacketViolationWarning",
    "SubChunkRequest",
    "ClientCacheStatus",
    "ClientCacheBlobStatus",
    "ResourcePackChunkRequest",
    "BlockPickRequest",
    "ActorPickRequest",
    "ItemStackRequest",
    "LecternUpdate",
    "Interact",
    "PlayerAction",
    "MapInfoRequest",
    "ServerSettingsRequest",
    "PositionTrackingDBClientRequest",
    "PhotoInfoRequest",
    "StructureTemplateDataRequest",
    "PlayerToggleCrafterSlotRequest",
    "SetPlayerInventoryOptions",
    "BookEdit",
    "CommandBlockUpdate",
    "StructureBlockUpdate",
    "AnvilDamage",
    "NPCRequest",
    "EmoteList",
    "SetDefaultGameType",
    "UpdateClientOptions",
    "ClientMovementPredictionSync",
    "SetPlayerFurnaceOptions",
}
PROTOCOL_VERSION_FILE = "crates/pumpkin-world/src/lib.rs"


def gophertunnel_packets(root: Path):
    """(id -> name, server-originating ids, client-originating ids, protocol)."""
    packet_dir = root / "minecraft/protocol/packet"
    names = {}
    # Go constant blocks: iota counts every spec line, `_` included, and a bare name repeats the
    # last `iota + N` with its own iota.
    iota = 0
    offset = None
    in_block = False
    for line in (packet_dir / "id.go").read_text().splitlines():
        line = line.split("//")[0].strip()
        if line.startswith("const ("):
            in_block, iota, offset = True, 0, None
            continue
        if not in_block or not line:
            continue
        if line == ")":
            in_block = False
            continue
        match = re.fullmatch(r"(ID\w+|_)\s*=\s*iota\s*\+\s*(\d+)", line)
        if match:
            offset = int(match.group(2))
            name = match.group(1)
        else:
            name = line
        if offset is not None and name.startswith("ID"):
            names[iota + offset] = name[2:]
        iota += 1
    by_name = {name: packet_id for packet_id, name in names.items()}

    pool = (packet_dir / "pool.go").read_text()
    blocks = re.findall(r"(\w+Originating) := map\[uint32\]func\(\) Packet\{(.*?)\n\t\}", pool, re.S)
    sides = {}
    for side, body in blocks:
        sides[side] = {by_name[name] for name in re.findall(r"\bID(\w+):", body) if name in by_name}

    info = (root / "minecraft/protocol/info.go").read_text()
    protocol = re.search(r"CurrentProtocol\s*=\s*(\d+)", info)
    return (
        names,
        sides.get("serverOriginating", set()),
        sides.get("clientOriginating", set()),
        int(protocol.group(1)) if protocol else None,
    )


def pex_packets(repo: Path):
    """(id -> [(struct, direction, path)]) for every Bedrock packet struct PEX defines."""
    found = {}
    for base in PROTOCOL_DIRS:
        for path in sorted((repo / base).rglob("*.rs")):
            text = path.read_text()
            rel = path.relative_to(repo).as_posix()
            if "/server/" in rel:
                direction = "server"
            elif "/client/" in rel:
                direction = "client"
            else:
                direction = None
            for match in re.finditer(r"#\[packet\((\d+)\)\][^{;]*?\b(?:struct|enum)\s+(\w+)", text, re.S):
                found.setdefault(int(match.group(1)), []).append(
                    (match.group(2), direction or guess_direction(match.group(2)), rel)
                )
            for match in re.finditer(
                r"impl\s+\w+\s+for\s+(\w+)\s*\{[^}]*?const PACKET_ID: i32 = (\d+);", text, re.S
            ):
                found.setdefault(int(match.group(2)), []).append(
                    (match.group(1), guess_direction(match.group(1)), rel)
                )
    return found


def guess_direction(struct: str) -> str:
    return "server" if struct.startswith("S") else "client"


def core_sources(repo: Path) -> str:
    return "\n".join(path.read_text() for path in sorted((repo / CORE_DIR).rglob("*.rs")))


def is_used(struct: str, direction: str, core: str) -> bool:
    if direction == "server":
        return re.search(rf"\b{struct}::PACKET_ID\s*=>", core) is not None
    # Built somewhere other than its own definition: a constructor call, a struct literal, an enum
    # variant or a reference to a unit struct, under its own name or a `use ... as` alias.
    names = [struct, *re.findall(rf"\b{struct}\s+as\s+(\w+)", core)]
    return any(
        re.search(rf"(\b{name}\s*(::\w+\s*\(|::[A-Z]\w*\b|\{{)|&\s*{name}\b)", core)
        for name in names
    )


# Helpers that reach Java clients only (Goal 8's Current state in docs/ROADMAP-2.0.md).
JAVA_HELPERS = (
    "send_client_packet",
    "try_send_client_packet",
    "send_packet",
    "broadcast_packet_all",
    "broadcast_packet_except",
    "broadcast_to_chunk",
    "broadcast_to_chunk_except",
    "broadcast_java_clients",
)
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:const\s+)?(?:unsafe\s+)?fn\s+(\w+)")
IMPL_RE = re.compile(r"^impl\b[^{]*?\b(?:for\s+)?(\w+)(?:<[^{]*>)?\s*\{?\s*$")
CALL_RE = re.compile(r"(?:(\w+)\s*)?\.({})\(".format("|".join(JAVA_HELPERS)))
BEDROCK_SELF = {"BedrockClient", "BedrockPlayer"}


def bedrock_imports(text: str) -> set:
    """Names a file imports from pumpkin_protocol::bedrock, aliases included."""
    names = set()
    for stmt in re.findall(r"use\s+pumpkin_protocol::\{?[^;]*?;", text, re.S):
        for part in re.findall(r"bedrock::[^;]*", stmt):
            depth = 0
            end = len(part)
            for i, ch in enumerate(part):
                depth += ch == "{"
                depth -= ch == "}"
                if depth < 0 or (depth == 0 and ch == ","):
                    end = i
                    break
            for name, alias in re.findall(r"\b([A-Z]\w*)(?:\s+as\s+(\w+))?", part[:end]):
                names.add(alias or name)
    return names


def body_end(lines: list, i: int, fns: list) -> int:
    """Where the function holding line i ends: the next fn, less its doc comments and attributes."""
    end = min((s for s, _ in fns if s > i), default=len(lines))
    while end - 1 > i and lines[end - 1].strip().startswith(("///", "#[", "//")):
        end -= 1
    return end


def java_sends(repo: Path):
    """Call sites in pumpkin-core, outside net/java, of the helpers that reach Java clients only.

    Returns (path, function, helper, packet, bedrock_in_function) rows. Calls whose receiver is a
    Bedrock client, or whose packet comes from pumpkin_protocol::bedrock, are left out.
    `bedrock_in_function` says the same function also has a Bedrock path (an `*_editioned` call
    or anything naming Bedrock); it is a lead, not proof of coverage.
    """
    rows = []
    for path in sorted((repo / CORE_DIR).rglob("*.rs")):
        rel = path.relative_to(repo).as_posix()
        if "/net/java/" in rel:
            continue
        text = path.read_text()
        lines = text.splitlines()
        imported = bedrock_imports(text)
        fns = [(i, m.group(1)) for i, line in enumerate(lines) if (m := FN_RE.match(line))]
        impls = [(i, m.group(1)) for i, line in enumerate(lines) if (m := IMPL_RE.match(line))]
        test_from = next(
            (i for i, line in enumerate(lines) if line.strip() == "#[cfg(test)]"), len(lines)
        )
        for i, line in enumerate(lines):
            if i >= test_from or line.lstrip().startswith("//") or FN_RE.match(line):
                continue
            call = CALL_RE.search(line)
            if not call:
                continue
            receiver = call.group(1)
            if receiver is None:
                back = i - 1
                while back >= 0 and not lines[back].strip():
                    back -= 1
                found = re.findall(r"(\w+)\s*$", lines[back]) if back >= 0 else []
                receiver = found[0] if found else ""
            start, function = max(((s, n) for s, n in fns if s <= i), default=(0, "?"))
            end = body_end(lines, i, fns)
            body = "\n".join(lines[start:end])
            impl = max(((s, n) for s, n in impls if s <= i), default=(0, ""))[1]
            if receiver == "self" and impl in BEDROCK_SELF:
                continue
            bound = re.escape(receiver)
            if re.search(rf"ClientPlatform::Bedrock\(\s*{bound}\s*\)", body) or re.search(
                rf"Some\(\s*{bound}\s*\)\s*=\s*[^;{{]*\.bedrock\(\)", body
            ):
                continue
            args = " ".join([line[call.end() :], *(part.strip() for part in lines[i + 1 : i + 4])])
            args = args.split(";")[0]
            packet = re.findall(r"\b(C[A-Z]\w+)\b", args)
            if not packet:
                earlier = " ".join(lines[max(start, i - 12) : i])
                packet = re.findall(r"(?:\b(C[A-Z]\w+)\s*(?:::|\{)|&(C[A-Z]\w+)\b)", earlier)[-1:]
                packet = [a or b for a, b in packet]
            packet = packet[0] if packet else ""
            if packet in imported:
                continue
            bedrock = re.search(r"_editioned\(|[Bb]edrock", body) is not None
            rows.append((rel, function, call.group(2), packet, bedrock))
        # Direct sends on a client matched as `ClientPlatform::Java(client)`.
        for i, line in enumerate(lines):
            bind = re.search(r"ClientPlatform::Java\(\s*([a-z]\w*)\s*\)", line)
            if i >= test_from or not bind:
                continue
            start, function = max(((s, n) for s, n in fns if s <= i), default=(0, "?"))
            end = body_end(lines, i, fns)
            near = "\n".join(lines[i : min(end, i + 15)])
            send = re.search(
                rf"\b{bind.group(1)}\s*\.\s*(?:send_packet_now|enqueue_packet|send_packet)\w*\(", near
            )
            if not send or function in JAVA_HELPERS:
                continue
            packet = re.findall(r"\b(C[A-Z]\w+)\b", near[send.end() :][:300])
            body = "\n".join(lines[start:end])
            bedrock = re.search(r"_editioned\(|[Bb]edrock", body) is not None
            rows.append((rel, function, "ClientPlatform::Java", packet[0] if packet else "", bedrock))
    return rows


def short(path: str) -> str:
    return path.removeprefix(CORE_DIR + "/")


def java_sends_table(repo: Path) -> list:
    rows = java_sends(repo)
    functions = {(path, function) for path, function, *_ in rows}
    out = [
        "## Java-only sends",
        "",
        "Every call in `{}` outside `net/java/` of a helper that reaches Java clients only ({}), "
        "plus direct sends on a client matched as `ClientPlatform::Java`. {} call sites in {} "
        "functions. Calls on a Bedrock client and Bedrock packets are left out. \"Bedrock in "
        "function\" means the same function also has a Bedrock path (an `*_editioned` call or "
        "anything naming Bedrock): a lead, not proof. [BEDROCK.md](BEDROCK.md) groups these by "
        "feature and says what a Bedrock player gets.".format(
            CORE_DIR,
            ", ".join(f"`{name}`" for name in JAVA_HELPERS),
            len(rows),
            len(functions),
        ),
        "",
        "| Function | Helper | Java packet | Bedrock in function |",
        "| --- | --- | --- | --- |",
    ]
    for path, function, helper, packet, bedrock in rows:
        out.append(
            "| `{}::{}` | `{}` | {} | {} |".format(
                short(path), function, helper, f"`{packet}`" if packet else "", mark(bedrock)
            )
        )
    out.append("")
    return out


def check_doc(repo: Path, doc: Path) -> int:
    """Print call sites the document doesn't name and names the code no longer has."""
    in_code = {f"{short(path)}::{function}" for path, function, *_ in java_sends(repo)}
    in_doc = set(re.findall(r"`([\w/]+\.rs::\w+)`", doc.read_text()))
    missing = sorted(in_code - in_doc)
    stale = sorted(in_doc - in_code)
    for key in missing:
        print(f"not in {doc}: {key}")
    for key in stale:
        print(f"in {doc} but no Java-only send there: {key}")
    return 1 if missing or stale else 0


def pex_protocol(repo: Path):
    path = repo / PROTOCOL_VERSION_FILE
    if not path.exists():
        return None
    match = re.search(r"CURRENT_BEDROCK_MC_PROTOCOL\s*:\s*u32\s*=\s*(\d+)", path.read_text())
    return int(match.group(1)) if match else None


def mark(value: bool) -> str:
    return "yes" if value else ""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--gophertunnel", type=Path, help="gophertunnel checkout")
    parser.add_argument("--repo", default=Path("."), type=Path, help="PEX checkout (default: .)")
    parser.add_argument("--out", type=Path, help="write the Markdown here instead of stdout")
    parser.add_argument(
        "--check", type=Path, help="compare the Java-only call sites with this document's keys"
    )
    args = parser.parse_args()

    if args.check:
        return check_doc(args.repo, args.check)
    if not args.gophertunnel:
        parser.error("--gophertunnel is required unless --check is given")

    names, from_server, from_client, gt_protocol = gophertunnel_packets(args.gophertunnel)
    if not names:
        print("no packet ids found in gophertunnel's id.go", file=sys.stderr)
        return 1
    pex = pex_packets(args.repo)
    core = core_sources(args.repo)

    rows = []
    totals = {"clientbound": [0, 0, 0], "serverbound": [0, 0, 0]}
    for packet_id in sorted(set(names) | set(pex)):
        name = names.get(packet_id, "(not in gophertunnel)")
        serverbound = packet_id in from_client
        # gophertunnel's server list still holds some client-only packets; a packet only the client
        # sends is serverbound only.
        clientbound = packet_id in from_server and not (
            serverbound
            and (
                name in CLIENT_ONLY
                or name.startswith(("Serverbound", "ServerBound", "Request", "ClientToServer"))
            )
        )
        structs = pex.get(packet_id, [])
        encodes = [s for s in structs if s[1] == "client"]
        decodes = [s for s in structs if s[1] == "server"]
        sends = any(is_used(s[0], "client", core) for s in encodes)
        handles = any(is_used(s[0], "server", core) for s in decodes)
        direction = "/".join(
            label for label, on in (("S->C", clientbound), ("C->S", serverbound)) if on
        ) or "?"
        if clientbound:
            totals["clientbound"][0] += 1
            totals["clientbound"][1] += bool(encodes)
            totals["clientbound"][2] += sends
        if serverbound:
            totals["serverbound"][0] += 1
            totals["serverbound"][1] += bool(decodes)
            totals["serverbound"][2] += handles
        rows.append(
            "| {} | {} | {} | {} | {} | {} | {} | {} |".format(
                packet_id,
                name,
                direction,
                mark(bool(encodes)) if clientbound or encodes else "-",
                mark(sends) if clientbound or encodes else "-",
                mark(bool(decodes)) if serverbound or decodes else "-",
                mark(handles) if serverbound or decodes else "-",
                ", ".join(f"`{s[0]}`" for s in structs),
            )
        )

    out = [
        "# Bedrock packet coverage",
        "",
        "Generated by `tools/bedrock-coverage.py` from gophertunnel (protocol {}) and this tree "
        "(protocol {}). Regenerate it rather than editing it.".format(
            gt_protocol, pex_protocol(args.repo)
        ),
        "",
        "Encodes and decodes mean pumpkin-protocol has a struct for the packet. Sends means "
        "pumpkin-core builds the clientbound struct somewhere; handles means pumpkin-core has a "
        "match arm for the serverbound one. Neither says the behaviour behind it matches vanilla. "
        "Directions come from gophertunnel's packet pools, less the client-only packets listed in "
        "the script.",
        "",
        "| Direction | Packets | PEX has a struct | PEX uses it |",
        "| --- | --- | --- | --- |",
    ]
    for label, (total, has, used) in totals.items():
        out.append(f"| {label} | {total} | {has} | {used} |")
    out += [
        "",
        "| Id | Packet | Direction | Encodes | Sends | Decodes | Handles | PEX structs |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
        *rows,
        "",
        *java_sends_table(args.repo),
    ]
    text = "\n".join(out)
    if args.out:
        args.out.write_text(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
