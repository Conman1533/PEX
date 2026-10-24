#!/usr/bin/env python3
"""Validate the PEX feature tracker and generate tracker/TRACKER.md.

    python3 tracker/check.py            validate, print totals, stale audits and gaps
    python3 tracker/check.py --write    also regenerate tracker/TRACKER.md
    python3 tracker/check.py --check    fail if TRACKER.md is out of date (for CI)
    python3 tracker/check.py --strict   also fail on stale audits and uncovered registry ids

Runs offline with the Python standard library only. Read tracker/README.md before
editing entries.json or audits.json.
"""

import argparse
import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRACKER = ROOT / "tracker"
ENTRIES = TRACKER / "entries.json"
AUDITS = TRACKER / "audits.json"
REGISTRY = TRACKER / "vanilla" / "registry.json"
OUTPUT = TRACKER / "TRACKER.md"

STATUSES = ("done", "partial", "planned", "missing")
ASSESSMENTS = ("audit", "upstream", "scan")
CHECKS = ("source-review", "unit-test", "gametest", "bot", "benchmark", "in-game", "release-test")
ORIGIN = re.compile(r"^(pex|upstream|upstream-pr#\d+)$")
KEY = re.compile(r"^[a-z][a-z-]*:[a-z0-9_.\-]+$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")

# Items registered with these classes and no behaviour component do nothing special.
PLAIN_ITEM_CLASSES = {"Item", "BlockItem"}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe_path(source):
    p = Path(source)
    return not p.is_absolute() and ".." not in p.parts


class Report:
    def __init__(self):
        self.errors = []
        self.warnings = []

    def error(self, msg):
        self.errors.append(msg)

    def warn(self, msg):
        self.warnings.append(msg)


def load(path, report):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        report.error(f"{path.relative_to(ROOT)}: {e}")
        return None


def validate_entries(data, report):
    cats = data.get("categories", {})
    if not cats:
        report.error("entries.json: no categories")
    for key, e in data.get("entries", {}).items():
        where = f"entries.json {key}"
        if not KEY.match(key):
            report.error(f"{where}: key must look like 'category:id'")
            continue
        cat = key.split(":", 1)[0]
        if cat not in cats:
            report.error(f"{where}: unknown category '{cat}'")
        if not e.get("name"):
            report.error(f"{where}: missing name")
        if e.get("status") not in STATUSES:
            report.error(f"{where}: status must be one of {STATUSES}")
        if e.get("assessment") not in ASSESSMENTS:
            report.error(f"{where}: assessment must be one of {ASSESSMENTS}")
        if not ORIGIN.match(str(e.get("origin", ""))):
            report.error(f"{where}: origin must be pex, upstream or upstream-pr#N")
        if "upstream_status" in e and e["upstream_status"] not in STATUSES:
            report.error(f"{where}: upstream_status must be one of {STATUSES}")
        for item in e.get("items", []):
            if not isinstance(item.get("text"), str) or not isinstance(item.get("done"), bool):
                report.error(f"{where}: checklist items need text and a boolean done")
        if e.get("status") == "done" and any(not i["done"] for i in e.get("items", []) if isinstance(i.get("done"), bool)):
            report.error(f"{where}: status is done but a checklist item is not")
        for f in e.get("files", []):
            if not safe_path(f):
                report.error(f"{where}: file path must be relative to the repo: {f}")
            elif not (ROOT / f).exists():
                report.error(f"{where}: file does not exist: {f}")
        if e.get("assessment") == "audit" and key not in AUDIT_KEYS:
            report.error(f"{where}: assessment is audit but audits.json has no entry")


def check_audits(entries, audits, report):
    """Return {key: 'current' | 'stale'}; a stale audit lists its changed files."""
    freshness = {}
    stale = {}
    for key, a in audits.get("entries", {}).items():
        where = f"audits.json {key}"
        if key not in entries:
            report.error(f"{where}: no matching entry in entries.json")
            continue
        for field in ("status", "commit", "date", "check", "scope", "note", "files", "evidence"):
            if not a.get(field):
                report.error(f"{where}: missing {field}")
        if a.get("status") not in STATUSES:
            report.error(f"{where}: status must be one of {STATUSES}")
        if not COMMIT.match(str(a.get("commit", ""))):
            report.error(f"{where}: commit must be a full 40-character hash")
        checks = a.get("check") if isinstance(a.get("check"), list) else [a.get("check")]
        for c in checks:
            if c not in CHECKS:
                report.error(f"{where}: check must be one of {CHECKS}")
        if "source-review" in checks and not a.get("vanilla") and not a.get("reference"):
            report.error(f"{where}: a source review names the vanilla class (vanilla) or what it was compared against (reference)")
        changed = []
        lines = {}
        for f in a.get("files", []):
            src = f.get("source", "")
            if not safe_path(src) or not SHA256.match(str(f.get("sha256", ""))):
                report.error(f"{where}: bad file record {f}")
                continue
            path = ROOT / src
            if not path.exists():
                changed.append(f"{src} (deleted)")
                continue
            if sha256(path) != f["sha256"]:
                changed.append(src)
            lines[src] = path.read_text(errors="replace").count("\n") + 1
        for ev in a.get("evidence", []):
            src = ev.get("source")
            if src not in {f.get("source") for f in a.get("files", [])}:
                report.error(f"{where}: evidence {src} is not in the audited files")
            elif not isinstance(ev.get("line"), int) or ev["line"] < 1:
                report.error(f"{where}: evidence line must be a positive integer")
            elif src in lines and ev["line"] > lines[src]:
                report.error(f"{where}: evidence line {ev['line']} is past the end of {src}")
            if not ev.get("label"):
                report.error(f"{where}: evidence needs a label")
        e = entries[key]
        if not changed and a.get("status") != e.get("status"):
            report.error(f"{where}: audit says {a.get('status')} but the entry says {e.get('status')}")
        if changed:
            stale[key] = changed
            freshness[key] = "stale"
        else:
            freshness[key] = "current"
    return freshness, stale


def coverage(entries, registry):
    """Registry ids that need an entry and have none, per category."""
    covered = defaultdict(set)
    for key, e in entries.items():
        cat, ident = key.split(":", 1)
        covered[cat].add(ident)
        covered[cat].update(e.get("ids", []))
    gaps = {}
    assets = json.loads((ROOT / "assets/entities.json").read_text())
    gaps["entities"] = sorted(i for i in assets if i != "player" and i not in covered["entities"])
    gaps["blocks"] = sorted(b for b, cls in registry["blocks"].items() if cls != "Block" and b not in covered["blocks"])
    gaps["items"] = sorted(
        i for i, v in registry["items"].items()
        if (v["class"] not in PLAIN_ITEM_CLASSES or v["components"]) and i not in covered["items"]
    )
    gaps["commands"] = sorted(
        c for c, v in registry["commands"].items()
        if v["when"] in ("always", "dedicated", "jfr") and "alias_of" not in v and c not in covered["commands"]
    )
    return {k: v for k, v in gaps.items() if v}


def totals(entries, categories):
    t = {c: Counter() for c in categories}
    for key, e in entries.items():
        t[key.split(":", 1)[0]][e["status"]] += 1
    return t


def md_escape(s):
    return str(s).replace("|", "\\|").replace("\n", " ")


def render(data, audits, freshness, stale, gaps, report):
    cats = data["categories"]
    entries = data["entries"]
    t = totals(entries, cats)
    out = [
        "# PEX feature tracker",
        "",
        "<!-- Generated by tracker/check.py --write from tracker/entries.json and tracker/audits.json. Do not edit by hand. -->",
        "",
        f"Minecraft {data['minecraft']}. Entries written against PEX `{data['commit'][:10]}`, "
        f"compared with upstream Pumpkin `{data['upstream']['commit'][:10]}`. See [README.md](README.md) for how to update it.",
        "",
        "**Status** is what PEX does today: `done`, `partial`, `planned` (not implemented, work is lined up) or `missing`. "
        "**Assessed by** says how much to trust it: `audit` is a recorded review in `audits.json` (its scope says what was and wasn't checked), "
        "`upstream` is copied from Pumpkin's tracker for code PEX hasn't changed, and `scan` only means an implementation was or wasn't found in the source. "
        "**Upstream** is shown where Pumpkin master differs from PEX.",
        "",
        "## Summary",
        "",
        "| Category | Entries | done | partial | planned | missing | audited (current / stale) |",
        "| :-- | --: | --: | --: | --: | --: | :-- |",
    ]
    by_cat_audit = Counter()
    by_cat_stale = Counter()
    for key, f in freshness.items():
        (by_cat_stale if f == "stale" else by_cat_audit)[key.split(":", 1)[0]] += 1
    grand = Counter()
    for c, meta in cats.items():
        row = t[c]
        grand.update(row)
        out.append(
            f"| [{meta['label']}](#{meta['label'].lower().replace(' ', '-')}) | {sum(row.values())} | "
            + " | ".join(str(row[s]) for s in STATUSES)
            + f" | {by_cat_audit[c]} / {by_cat_stale[c]} |"
        )
    out.append(
        f"| **Total** | **{sum(grand.values())}** | "
        + " | ".join(f"**{grand[s]}**" for s in STATUSES)
        + f" | {sum(by_cat_audit.values())} / {sum(by_cat_stale.values())} |"
    )
    origin = Counter(e["origin"].split("#")[0] for e in entries.values())
    assess = Counter(e["assessment"] for e in entries.values())
    diff = sum(1 for e in entries.values() if "upstream_status" in e)
    out += [
        "",
        f"Origin: {origin['pex']} PEX, {origin['upstream-pr']} from open upstream PRs merged into PEX, {origin['upstream']} upstream. "
        f"Assessed by: {assess['audit']} audit, {assess['upstream']} upstream tracker, {assess['scan']} source scan. "
        f"{diff} entries differ from upstream Pumpkin.",
        "",
    ]
    if stale:
        out += ["## Stale audits", "", "These files changed after the audit. Re-audit before trusting the status.", ""]
        for k, files in sorted(stale.items()):
            out.append(f"- `{k}`: {', '.join(f'`{f}`' for f in files)}")
        out.append("")
    if gaps:
        out += ["## Registry ids without an entry", ""]
        for c, ids in gaps.items():
            out.append(f"- **{c}** ({len(ids)}): " + ", ".join(f"`{i}`" for i in ids))
        out.append("")
    for c, meta in cats.items():
        keys = sorted((k for k in entries if k.startswith(c + ":")), key=lambda k: (entries[k].get("group", ""), entries[k]["name"].lower()))
        out += [f"## {meta['label']}", "", meta.get("description", ""), ""]
        out += ["| Entry | Status | Origin | Upstream | Assessed by | Notes |", "| :-- | :-- | :-- | :-- | :-- | :-- |"]
        group = None
        for k in keys:
            e = entries[k]
            if e.get("group") and e["group"] != group:
                group = e["group"]
                out.append(f"| **{md_escape(group)}** | | | | | |")
            assessed = e["assessment"]
            if k in freshness:
                a = audits["entries"][k]
                checks = a["check"] if isinstance(a["check"], list) else [a["check"]]
                assessed = f"audit ({', '.join(checks)}; {freshness[k]} @{a['commit'][:7]})"
            notes = md_escape(e.get("note", ""))
            items = e.get("items", [])
            if items:
                notes += ("<br>" if notes else "") + "<br>".join(
                    ("☑ " if i["done"] else "☐ ") + md_escape(i["text"]) for i in items
                )
            ids = e.get("ids", [])
            if ids and len(ids) > 1:
                notes += f"<br><sub>{len(ids)} ids: {', '.join(ids[:12])}{' …' if len(ids) > 12 else ''}</sub>"
            out.append(
                f"| {md_escape(e['name'])} | {e['status']} | {e['origin']} | {e.get('upstream_status', '')} | {assessed} | {notes} |"
            )
        out.append("")
    if report.warnings:
        out += ["## Warnings", ""] + [f"- {md_escape(w)}" for w in report.warnings] + [""]
    return "\n".join(out).rstrip() + "\n"


AUDIT_KEYS = set()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--write", action="store_true", help="regenerate tracker/TRACKER.md")
    ap.add_argument("--check", action="store_true", help="fail if tracker/TRACKER.md is out of date")
    ap.add_argument("--strict", action="store_true", help="fail on stale audits and uncovered registry ids")
    ap.add_argument("--quiet", action="store_true", help="only print problems")
    args = ap.parse_args()

    report = Report()
    data = load(ENTRIES, report)
    audits = load(AUDITS, report) or {"entries": {}}
    registry = load(REGISTRY, report)
    if data is None or registry is None:
        print("\n".join(report.errors), file=sys.stderr)
        return 2
    AUDIT_KEYS.update(audits.get("entries", {}))

    validate_entries(data, report)
    freshness, stale = check_audits(data["entries"], audits, report)
    gaps = coverage(data["entries"], registry)

    if not args.quiet:
        t = totals(data["entries"], data["categories"])
        width = max(len(c) for c in data["categories"])
        print(f"{'category':<{width}}  total  " + "  ".join(f"{s:>7}" for s in STATUSES))
        grand = Counter()
        for c in data["categories"]:
            grand.update(t[c])
            print(f"{c:<{width}}  {sum(t[c].values()):>5}  " + "  ".join(f"{t[c][s]:>7}" for s in STATUSES))
        print(f"{'total':<{width}}  {sum(grand.values()):>5}  " + "  ".join(f"{grand[s]:>7}" for s in STATUSES))
        print(f"\naudits: {sum(1 for f in freshness.values() if f == 'current')} current, {len(stale)} stale")
    for k, files in sorted(stale.items()):
        print(f"stale audit {k}: {', '.join(files)}")
    for c, ids in gaps.items():
        print(f"no entry for {len(ids)} {c} ids: {' '.join(ids)}")

    rendered = render(data, audits, freshness, stale, gaps, report)
    if args.write:
        OUTPUT.write_text(rendered)
        print(f"wrote {OUTPUT.relative_to(ROOT)}")
    if args.check and (not OUTPUT.exists() or OUTPUT.read_text() != rendered):
        report.error("tracker/TRACKER.md is out of date: run python3 tracker/check.py --write")

    for w in report.warnings:
        print(f"warning: {w}", file=sys.stderr)
    for e in report.errors:
        print(f"error: {e}", file=sys.stderr)
    if report.errors:
        return 1
    if args.strict and (stale or gaps):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
