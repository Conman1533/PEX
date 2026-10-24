# PEX 2.0 roadmap

Derived from `tracker/` at alpha `2ce43ef3d` (2026-10-24). Each work item names the tracker entries it closes, what it depends on, and the proof needed before those entries can move to `done`. Items are sized for one agent session each; split any that turn out bigger rather than stretching a session.

## Where PEX stands

| | Entries | done | partial | planned | missing |
| :-- | --: | --: | --: | --: | --: |
| Vanilla registries (entities, blocks, items, commands) | 601 | 416 | 168 | 2 | 15 |
| Mechanics (redstone, combat, world) | 73 | 46 | 25 | 0 | 2 |
| PEX-only (plugins, performance, storage, Bedrock, ops) | 45 | 16 | 24 | 0 | 5 |
| **Total** | **719** | **478** | **217** | **2** | **22** |

Read the `done` column with care. 50 entries have a PEX audit. Most of the rest carry Pumpkin's published status (556 entries), and most of those are Pumpkin's baseline claims, which come from file presence and TODO heuristics, not reviews. 2.0 should not ship claims it hasn't checked for anything a survival player depends on, which is why several items below are audits rather than code.

What the audits found, in short:

- Mobs merged from open upstream PRs (fox, bogged, drowned, phantom, cat, pufferfish, silverfish) have their vanilla goal lists. The gaps are in species behaviour that isn't a goal: bred foxes don't trust their breeders, dispensers only shear sheep.
- Many passive mobs run generic goals and are missing their own: bees never use hives or flowers, turtles never go home to lay eggs, cats never sleep on beds or sit on chests, parrots never follow or perch on their owner, pandas have no personalities, dolphins don't lead players to treasure, goats don't ram or long-jump, wardens have no sonic boom.
- Several PEX 1.0 features were replaced by upstream code during merges: the region IO fixes (by upstream #4062 in 1.2) and the entity section index (by upstream's `entity_index.rs` in the #4092 merge). The disk-full behaviour has not been re-tested since.
- Native plugins are accepted from any build with the same `PLUGIN_API_VERSION`, offline UUIDs don't match vanilla, an invalid config value resets the whole file to defaults, and nothing versions `pumpkin.toml`.

## Work items

Proof types: **unit** (a `cargo test`), **gametest** (`pumpkin-gametest`), **bot** (`native-plugins/tools/testbot` scenario), **bench** (benchmark run, numbers in the PR), **in-game** (a human with a real client), **audit** (an `audits.json` record with its scope).

### A. Foundations

These unblock or de-risk everything else. Do A1 to A3 first.

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| A1 | `config_version` in `pumpkin.toml` and `features.toml`, with ordered migrations that back up the old file. Report an invalid value by key and keep the rest of the file, instead of resetting everything to defaults. | `ops:config-version`, `ops:config-merge` | | unit: each migration step, a file with one bad value keeps its other values |
| A2 | Vanilla offline UUIDs (v3 of `"OfflinePlayer:" + name`), and a one-time migration of `playerdata`, `ops.json`, `whitelist.json` and bans keyed by the old SHA-256 UUIDs. | `ops:offline-uuid` | A1 (the migration runs once, keyed on `config_version`) | unit: UUIDs for known names match vanilla; migration test on a fixture world; in-game: offline player keeps inventory after upgrade |
| A3 | Native plugin ABI policy: embed a build identity (rustc version, `Cargo.lock` hash, commit) in the server and in `pex_common::export_plugin!`, and refuse a plugin whose identity differs. Or, if Q1 picks stable, design that instead (several sessions). | `pex-plugins:native-abi` | Q1 | unit: loader rejects a mismatched identity; release-test: plugin zip from the same release loads, one from the previous release is refused with a clear message |
| A4 | Release builds entirely in CI: tag-triggered, every target built, each binary booted to "Done" and stopped (QEMU for ARM64, Wine or a Windows runner for Windows), `SHA256SUMS` and plugin zips attached. | `ops:release-builds` | | release-test: a dry-run tag on a fork produces a draft release with all artefacts and passing smoke tests |
| A5 | Run `python3 tracker/check.py --check` in CI so TRACKER.md and the JSON stay valid. | `ops:ci` | | CI job fails on a deliberately broken entry |
| A6 | Upstream merge checklist: a script that, for a merge commit, lists PEX-only files the merge deleted or rewrote, using `tracker/entries.json` `files` and `origin`. | `ops:upstream-sync` | | unit or script test against `fca7ec321`, which must flag `world/entity_sections.rs` |
| A7 | Fix or quarantine the GameTest fixture `pumpkin:summon_command_regression` ("Palette entry 0 is missing 'Name'") and run GameTests in CI. | `ops:gametests` | | gametest: suite passes in CI |

### B. World storage and vanilla round trip

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| B1 | Round-trip harness: a small world generated by vanilla 26.3 (committed fixture, a few regions with chests, signs, banners, spawners, entities and a player) is loaded by PEX, saved, and compared tag by tag with the original, ignoring fields vanilla itself rewrites. | `storage:vanilla-26-3-load` | Q4 | unit: round trip test passes in CI |
| B2 | Reverse direction: a world saved by PEX opens in vanilla 26.3 without data fixer warnings, chunk errors or lost block entities. | `storage:vanilla-26-3-load`, `storage:entity-and-player-data` | B1 | release-test: scripted vanilla server load of a PEX-saved world, log checked for errors |
| B3 | Re-test disk-full behaviour on alpha (loop device or tmpfs filled during play) and restore whatever the upstream region IO lost: chunks stay dirty after a failed write and are saved once space returns. | `storage:disk-full` | | unit where possible; release-test on a full filesystem, no lost chunks after freeing space |
| B4 | Conversion safety: `--convert-world` and automatic conversion keep or back up the source region files by default. | `storage:conversion` | Q5 | unit: converter leaves the source files (or a backup) in place |
| B5 | Audit Pump and Linear formats against their specs and decide which formats 2.0 supports. | `storage:pump-format`, `storage:linear-format` | Q4 | audit |

### C. Redstone parity

Vanilla's default (non-experimental) redstone. Every item adds GameTests built from structures that behave the same on vanilla, so a test that passes on both servers is the proof.

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| C1 | Update order: compare the neighbour updater, wire power propagation and block update ordering with vanilla's `CollectingNeighborUpdater` and `RedStoneWireBlock`; quasi-connectivity and BUD behaviour. | `redstone:redstone-update-order`, `redstone:redstone-wire-signal-propagation`, `redstone:hard-vs-soft-powering` | A7 | audit + gametest: wire line, BUD switch, quasi-connected piston |
| C2 | Pistons: push limits, moving pistons, block entities that can't move, slime and honey, zero-tick and headless cases vanilla allows. | `blocks:piston_base`, `blocks:moving_piston`, `blocks:piston_head`, `redstone:pistons-and-block-swapping` | C1 | gametest: piston door, flying machine, slime block pull |
| C3 | Timing components: repeater, comparator (subtract mode, container reading), observer (re-audit #3863), redstone torch burnout. | `redstone:repeater`, `redstone:comparator`, `redstone:observer`, `redstone:redstone-torch-levers-and-buttons` | C1 | gametest: observer clock, comparator clock, torch burnout |
| C4 | Item transport: hopper timing and locking, droppers, dispensers (including shearing every shearable entity, not only sheep), crafter. | `redstone:hopper-item-transfer`, `redstone:dropper`, `redstone:dispenser`, `blocks:dispenser`, `redstone:crafter`, `entities:bogged` | C1 | gametest: hopper clock, item sorter, dispenser shearing bogged and snow golem |
| C5 | Rails, tripwire, bells, copper bulbs, daylight detectors, target blocks, sculk sensor outputs. | `redstone:rails`, `blocks:powered_rail`, `blocks:trip_wire`, `blocks:trip_wire_hook`, `blocks:bell`, `redstone:copper-bulb`, `redstone:minecart-powering-and-activation` | C1 | gametest per component |

### D. Survival parity to the dragon

The goal: a survival player can go from spawn to killing the ender dragon with nothing broken on the server side. D1 to D5 are the blockers on that path; D6 onwards are the mobs and blocks a normal playthrough meets.

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| D1 | Ender dragon: phases, perching, breath and fireballs (the `dragon_fireball` entity is missing), crystals and healing, the exit portal and egg. | `entities:ender_dragon`, `entities:dragon_fireball`, `entities:end_crystal`, `world:dragon-fight` | | bot: dragon fight scenario in creative-assisted survival; in-game kill |
| D2 | Nether: blazes hover and keep to their fortress (`MoveTowardsRestrictionGoal`, revenge targeting), fortress and bastion spawns, piglin bartering and bastion guarding. | `entities:blaze`, `entities:piglin`, `entities:piglin_brute`, `world:structures` | | audit + bot: obtain blaze rods and pearls via barter |
| D3 | Endermen and pearls, eyes of ender, strongholds and end portal frames. | `entities:enderman`, `entities:eye_of_ender`, `blocks:end_portal_frame`, `world:portals` | | bot: throw eyes to a stronghold, fill the frame, enter the End |
| D4 | Survival bot scenario: scripted stages (gather, craft, smelt, nether, stronghold, End) that each start from a saved world, so a stage can fail on its own. | `world:survival-to-the-dragon`, `ops:bot-tests` | D1 to D3 for the later stages | bot: all stages pass on a fixed seed |
| D5 | Audit the survival-critical upstream claims: crafting, furnaces, brewing, enchanting, anvils, beds and respawn anchors, villagers and trading, loot tables, advancements. Replace `upstream` assessments with audits or fixes. | `world:recipes`, `world:loot-tables`, `world:advancements`, `blocks:bed`, `entities:villager` and the workstation blocks | | audit per entry |
| D6 | Hostile mob batch: re-audit the hostile mobs PEX changed (zombies, skeletons, spiders, creepers, witches, illagers, raids) against vanilla goal lists and attack code; fix what the audits find. One session per family. | `entities:*` group Hostile, `world:raids` | | audit + gametest or bot per family |
| D7 | Passive mob goals, one session per group: bees (hives, flowers, pollination); turtles (home, eggs, travel); cats and parrots (owner behaviour); pandas; dolphins (air, treasure, Dolphin's Grace); goats (ram, long jump); wolves (anger targets); llamas (caravans, taming); polar bears; rabbits (crop raiding); foxes (breeding trust). | `entities:bee`, `entities:turtle`, `entities:cat`, `entities:parrot`, `entities:panda`, `entities:dolphin`, `entities:goat`, `entities:wolf`, `entities:llama`, `entities:polar_bear`, `entities:rabbit`, `entities:fox` | | audit (goal list matches `registerGoals` or the Brain) + bot scenario for the visible behaviour |
| D8 | Warden combat: sonic boom, sniffing and digging. | `entities:warden` | | audit + gametest in an ancient city fixture |
| D9 | Missing entities: `ominous_item_spawner` (trial chambers), `mannequin`, `camel_husk`, `zombie_nautilus`. | those entities | | audit each |
| D10 | Missing block behaviour: sculk catalyst spreading, moss bone meal, amethyst chime, wall skull placement, explorer and filled maps. | `blocks:sculk`, `blocks:sculk_catalyst`, `blocks:bonemealable_feature_placer`, `blocks:amethyst`, `blocks:wall_skull`, `items:map` | | unit or gametest per block |
| D11 | Worldgen leftovers: templates with more than one palette, shape updates for every block placed by a template. | `world:features-and-placement` | | unit: template tests with a two-palette template |

### E. Performance against Paper

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| E1 | Put the gameplay benchmark where it can be rerun: pin `mc-gameplay-bench` by commit, script one run on fixed hardware, and re-measure current alpha against Paper 26.3. | `performance:benchmark-vs-paper` | Q8 | bench: numbers for alpha in the PR, same method as 1.0 |
| E2 | Tail latency: profile the 95th percentile ticks and fix the largest cause. | `performance:p95-ticks` | E1 | bench: p95 below Paper's at 24 players |
| E3 | Network volume: find where PEX sends about 45% more bytes than Paper (chunk data, entity updates, light) and cut it. | `performance:network-volume` | E1 | bench: bytes per player at or below Paper's |
| E4 | Re-measure the entity index after upstream replaced PEX's section storage; port PEX's version back only if it is faster. | `performance:entity-section-index` | E1 | bench: both versions on the same run |
| E5 | A short benchmark in CI (minutes, not hours) that fails on a large regression. | `performance:benchmark-vs-paper` | E1 | CI job with a recorded baseline |

### F. Plugins and Bedrock

| ID | Work item | Tracker entries | Depends on | Proof |
| :-- | :-- | :-- | :-- | :-- |
| F1 | Check that every event PEXGuard and PEXProtect rely on is fired where they expect (PORTING_GUIDE Appendix A), and add bot scenarios for region protection and rollback. | `pex-plugins:pexguard`, `pex-plugins:pexprotect` | A3 | bot: protected region blocks a guest, rollback restores a griefed area |
| F2 | WorldPumpkin copy, paste and rotate; schematics if Q9 says so. | `pex-plugins:worldpumpkin` | A3, Q9 | unit + bot |
| F3 | Bedrock smoke test: a scripted Bedrock client logs in, receives chunks and moves. | `bedrock:parity-tests` | Q10 | bot or scripted client in CI |

## Order

1. A1, A3, A4, A5, A7 in parallel (no shared files), then A2 and A6.
2. B1 and C1 next; they set up the harnesses the rest of B and C reuse.
3. D1 to D3 and E1 can start any time after A; D4 needs D1 to D3.
4. Everything else in any order. Update the tracker entries in the same branch as each item (root `AGENTS.md`).

2.0 is ready when A, B1 to B3, C1 to C4, D1 to D5 and E1 to E3 are done, and no survival-critical entry is assessed only by `scan` or a Pumpkin baseline claim.

## Decisions for the maintainer

1. **Plugin ABI.** Stable (a C ABI or WIT-style interface for native plugins, several sessions and a compatibility promise) or explicitly unstable (refuse plugins from another build, one session)? The second matches how releases already ship plugins.
2. **Offline UUID migration.** Migrate existing offline-mode servers automatically on the first 2.0 start, or only with a flag? Automatic is safer for players and riskier for hosts who edit `playerdata` themselves. Should BungeeCord/Velocity forwarding keep the old UUIDs for one release?
3. **Invalid config values.** Refuse to start and name the key, or start with that key's default and warn?
4. **Default chunk format for 2.0.** Upstream made Pump the default. The vanilla round-trip goal argues for Anvil as the default. Which one, and should Linear stay supported?
5. **Converter.** Keep the source region files after an automatic conversion (needs disk space), move them to a backup folder, or keep deleting them?
6. **Redstone target.** Vanilla default redstone only, or also the 26.x experimental redstone? Are any Pumpkin deviations acceptable (for example update order that differs but gives the same result in common contraptions)?
7. **What counts as survival parity.** A bot run, a human playthrough, or both? Which known deviations are accepted (client-authoritative movement, for one)?
8. **Performance bar.** Which hardware, player counts and duration, and which numbers have to beat Paper: mean tick, p95, memory, network? Should the benchmark move into this repo?
9. **Plugins in 2.0.** Are WorldPumpkin schematics and copy/paste, PEXGuard polygon regions and PEXProtect container logging 2.0 requirements or later?
10. **Bedrock.** In scope for 2.0 or explicitly experimental?
11. **Open upstream PRs.** Keep merging open upstream PRs into alpha, or only take what upstream merged? Open PRs are where most mob work comes from (37 merged so far), and also where merges replaced PEX work.
12. **Upstream claims.** Is an audit required for every survival-critical entry before 2.0, or is Pumpkin's published status acceptable for code PEX hasn't touched?
13. **musl and plugins.** Keep the static musl build without plugin support, or ship a dynamically linked musl build that can load them?
