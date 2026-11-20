#!/usr/bin/env python3
"""Bedrock packet coverage: every packet gophertunnel knows at the current protocol, next to what
PEX has for it.

    tools/bedrock-coverage.py --gophertunnel ../gophertunnel [--out docs/BEDROCK-COVERAGE.md]

gophertunnel (https://github.com/Sandertv/gophertunnel) is the reference: its packet ids and its
lists of server- and client-originating packets. For each packet the table says whether PEX can
encode it (a clientbound struct in pumpkin-protocol), decode it (a serverbound struct), and use it
(the clientbound struct is built somewhere in pumpkin-core, or the serverbound one has a match arm
there). Run it from the repository root. Standard library only.
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
    "PlayerSkin",
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
            for match in re.finditer(r"#\[packet\((\d+)\)\][^{;]*?\bstruct\s+(\w+)", text, re.S):
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
    # Built somewhere other than its own definition: a constructor call or a struct literal.
    return re.search(rf"\b{struct}\s*(::\w+\s*\(|\{{)", core) is not None


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
    parser.add_argument("--gophertunnel", required=True, type=Path, help="gophertunnel checkout")
    parser.add_argument("--repo", default=Path("."), type=Path, help="PEX checkout (default: .)")
    parser.add_argument("--out", type=Path, help="write the Markdown here instead of stdout")
    args = parser.parse_args()

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
    ]
    text = "\n".join(out)
    if args.out:
        args.out.write_text(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
