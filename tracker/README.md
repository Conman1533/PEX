# PEX feature tracker

The tracker lists what PEX does compared with vanilla Minecraft and with upstream Pumpkin, one entry per feature. It follows the data model of Pumpkin's own tracker (pumpkinmc.org/tracker): `category:id` keys, a status, a note and a checklist, and separate audit records whose file hashes show when a review has gone stale. The code here is PEX's own.

| File | What it is | Edit by hand? |
| :-- | :-- | :-- |
| `entries.json` | Every tracked feature and its current status | Yes |
| `audits.json` | Recorded reviews, pinned to file hashes | Yes, when you audit |
| `vanilla/registry.json` | Vanilla block, item and command classes, extracted from the decompiled jar | No, regenerate it |
| `TRACKER.md` | The readable table | No, run `check.py --write` |
| `check.py` | Validator and generator (Python 3 standard library, no network) | |
| `extract_vanilla.py` | Builds `vanilla/registry.json` | |

## Commands

```sh
python3 tracker/check.py            # validate, print totals per category and status, list stale audits and gaps
python3 tracker/check.py --write    # also regenerate tracker/TRACKER.md
python3 tracker/check.py --check    # fail if TRACKER.md is out of date
python3 tracker/check.py --strict   # also fail on stale audits or registry ids without an entry
```

Run `--write` after every change to `entries.json` or `audits.json` and commit `TRACKER.md` with it. Errors exit with status 1.

## Entries

```json
"entities:fox": {
  "name": "Fox",
  "group": "Passive",
  "status": "partial",
  "assessment": "audit",
  "origin": "upstream-pr#4093",
  "upstream_status": "missing",
  "note": "One or two sentences on what works and what doesn't.",
  "items": [{"text": "Every Fox.registerGoals goal at its vanilla priority", "done": true}],
  "files": ["crates/pumpkin-core/src/entity/passive/fox/mod.rs"],
  "ids": ["..."],
  "vanilla": "Fox",
  "commits": ["00d2fb7dd upstream-pr#4093", "0da903efb pex"]
}
```

- **key**: `category:id`. Categories are listed in `entries.json` under `categories`. Entities use the registry id (`entities:fox`). Blocks use one entry per vanilla block class in snake case without `Block` (`blocks:stair` for `StairBlock`), with the block ids in `ids`. Items do the same per item class (`items:bow`), plus `items:component-*` entries for behaviour that comes from a component (food, tool, equippable) rather than a class. Commands use the root literal (`commands:teleport`), with aliases in `ids`. The other categories use a short slug.
- **status**: what PEX does now.
  - `done`: everything vanilla does here works, as far as the assessment covered. A `done` entry can't have an unchecked item.
  - `partial`: some of it works. Say what is missing in the note or items.
  - `planned`: not implemented, but someone is working on it or it is a scheduled roadmap item.
  - `missing`: not implemented.
- **assessment**: how the status was decided. Be honest here, it is what people use to judge the status.
  - `audit`: there is a record in `audits.json`. Only this one counts as reviewed.
  - `upstream`: copied from Pumpkin's tracker. The note says whether that was a Pumpkin audit or its baseline claim (file presence and TODO heuristics) and whether PEX has changed the code since.
  - `scan`: an implementation was or wasn't found. Nobody checked the behaviour.
- **origin**: who wrote the current implementation. `pex` for PEX's own work, `upstream-pr#N` for an open upstream pull request merged into PEX, `upstream` for code from Pumpkin master. When PEX only patched upstream code, the origin stays `upstream` and the patches go in `commits`.
- **upstream_status**: only where Pumpkin master differs from PEX. For entries Pumpkin's tracker covers, this is Pumpkin's published status; for features Pumpkin doesn't have, `missing`.
- **items**: the checklist. Write each item as a vanilla behaviour a player or admin could check ("Bred baby trusts the breeding players"), not as a code task.
- **files**: the source that implements the feature, relative to the repo root. `check.py` fails if one is missing.
- **commits**: PEX-only commits that touched the feature, as `<short hash> <origin>`.

## When you change code

Every change that adds, fixes or removes behaviour updates the tracker in the same branch:

1. Find the entry. `grep -n '"<id>"' tracker/entries.json`, or search by file path in `files`. If there isn't one, add it in the right category. Registry-backed categories (entities, blocks, items, commands) already have an entry for every vanilla id; `check.py` lists any that don't.
2. Update `status`, `note` and `items` to what the code does now. Set `origin` if the feature is new, and add your commit to `commits` once it exists.
3. If the entry has an audit, its files changed, so the audit is now stale. Either re-audit (below) or leave it stale: `check.py` reports it and `TRACKER.md` marks it, which is the honest state. Don't update the hashes without reviewing.
4. Run `python3 tracker/check.py --write` and commit the JSON and `TRACKER.md` together.

## Auditing

An audit is a review you can repeat. Record what you compared and against what, and nothing more.

1. Get the vanilla source as the root `AGENTS.md` describes (Vineflower on the 26.3 server jar) and read the class and methods the feature mirrors. For behaviour that only shows in a running world, use a GameTest, a unit test, `native-plugins/tools/testbot` or an in-game check instead, or as well.
2. Add or replace the record in `audits.json`:

   ```json
   "entities:fox": {
     "status": "partial",
     "commit": "<full 40-character hash of the commit you reviewed>",
     "date": "2026-10-24",
     "check": ["source-review", "unit-test"],
     "scope": "What you compared and what you didn't. Say 'not tested in game' when that's the case.",
     "note": "The finding, as it should read in the tracker.",
     "files": [{"source": "crates/.../fox/mod.rs", "sha256": "<sha256 of the file at that commit>"}],
     "evidence": [{"label": "Goal list mirroring Fox.registerGoals", "source": "crates/.../fox/mod.rs", "line": 233}],
     "items": [{"text": "...", "done": true}],
     "vanilla": {"version": "26.3 (server jar decompiled with Vineflower)", "class": "net.minecraft.world.entity.animal.fox.Fox", "methods": ["registerGoals"]},
     "tests": ["test path or scenario name"]
   }
   ```

   `check` is one or more of `source-review`, `unit-test`, `gametest`, `bot`, `benchmark`, `in-game`, `release-test`. A `source-review` needs `vanilla` (or `reference` when there's no vanilla class, such as region IO or plugins). Every evidence file must be in `files`, and its line must exist.
3. Copy `status`, `note` and `items` into the entry and set `"assessment": "audit"`. `check.py` fails if the audit and the entry disagree while the audit is current.
4. Hash with `sha256sum <file>`. Only hash files you actually read.

Scope matters more than status. "Goal list compared with registerGoals; goal internals not compared; not tested in game" is a useful audit. "Looks complete" is not.

## Updating for a new Minecraft version

1. Decompile the new server jar.
2. `python3 tracker/extract_vanilla.py <decompiled root> <version>` rewrites `vanilla/registry.json`. It reads `Blocks.java`, `Items.java`, `Commands.java` and the id constants in `net/minecraft/references`, and joins them with `assets/blocks.json` and `assets/items.json`, so update `assets/` first.
3. Run `check.py`. New vanilla classes and commands show up as registry ids without an entry; add them. Set `minecraft` in `entries.json`.

## Comparing with Pumpkin's tracker

Pumpkin publishes its data in Pumpkin-MC/Pumpkin-Website (`scripts/tracker-baseline.json` and `scripts/tracker-audits.json`). That repository has no license, so don't copy its files or notes into PEX. Reading its statuses to fill `upstream_status` is fine. Its baseline entries are inherited claims rather than reviews, and most of its audits are against older Pumpkin commits, so treat `upstream` assessments here as weaker than an audit.
