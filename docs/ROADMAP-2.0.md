# PEX 2.0 roadmap

Assessed at: `300621c3e5b7a9f5eef70af8c6c8ec5dad8b5e96` (`alpha`, committed 2026-10-29, "ci(release): build only the server for musl targets").

This is a fresh skeleton on branch `roadmap-2.0-v2`. The earlier roadmap is still `docs/ROADMAP-2.0.md` on `alpha` and on branch `roadmap-2.0`.

## Order of work

A piece can start once every piece in its "Depends on" line is on `alpha` and its decisions are answered. Among the pieces that can start, take them in this order.

1. Answer the open decisions that hold the most work. The table below has all of them.
   - Neon's GitHub support ticket (D0-1): G0-01 waits on Actions running again, and through it the dual-run job in CI, the hosted Windows and ARM64 runs, and D10-5.
   - D7-4 (6 pieces) and D7-1 (5).
   - D5-1, D6-4, D8-3 and the open part of D10-2 (2 pieces each).
2. Fix what hurts running servers now. None of these waits on another piece.
   - G8-04: an out-of-range hotbar slot from a Bedrock client can panic the server (read from the code, not reproduced).
   - G3-01: one bad value in `pumpkin.toml` resets the whole config to defaults, which can start a new world. D3-1 is answered except its two-key sub-question.
   - G7-10: block entity changes are lost when their chunk unloads. D7-6 picked the existing flush branch.
   - G8-02, then G8-03: alpha accepts only Bedrock protocol 2193, so current Bedrock clients most likely can't join.
   - G6-04: `fix/attack-packet-parity` is finished and only needs landing.
3. Groundwork other goals build on. These can run in parallel.
   - G0-04, then G0-03: the benchmark baseline.
   - G0-05 now, for Linux CI in CT 117. G0-01 once GitHub runs Actions again, then G0-02: hosted CI and the dual-run job.
   - G1-01: the mob GameTest helpers most of Goal 1 uses.
   - G3-02, then G3-03: the first `Step::Config` migration. After it, each migration step takes the next free `config_version` when it lands, never a number another open branch already uses: G2-10, G7-22, G7-26, G9-08, and G3-08, G5-14 and G8-19 if they need one. G4-01 lands before or after G3-03, not alongside it.
   - G7-01, G7-02, G7-03, G7-04: the fixture world, the tree diff and the two round trips that every Goal 7 field piece proves itself with.
   - G6-01, G6-02, G6-03: the tester server and the bot actions the survival stages need.
   - G9-02, and G9-01 after G0-03: measuring before Goal 9's fixes.
   - G10-01, G10-02: the pinned toolchain and the checked vanilla jar.
4. Then each goal in its own order, taking first the pieces another goal waits on:
   - for Goal 6's stages: G1-10, G1-12, G1-27 and G1-30 from Goal 1, G7-07 and G7-10 from Goal 7;
   - G1-42 before G7-24, G5-04 before G7-09, G6-19 and G6-22 before G7-31;
   - Goal 9's fixes after G9-01 and G9-03 have measured, and Goal 10's release pieces after G0-01.
   - Goal 8 stops at G8-12 for 2.0 (D8-1); G8-13 to G8-19 and G8-22 come after the release.
   - Goal 2 has no Wasm pieces of its own (D2-2): G2-07 to G2-10 wait on upstream, and G2-06 is dropped (D2-1).
5. Last, the end-to-end proofs: G6-22 (the full chained run), G7-31 (the long-play round trip), G1-44 (villager life), G9-14 (published benchmarks), G10-09 and G10-10 (real hardware).

## Decisions for Neon

Every decision, one row each; the options, their costs and the details are in the goal's own Decisions list. On 2026-10-31 Neon accepted every answer the earlier roadmap gave on 2026-10-29 (`git show origin/roadmap-2.0:docs/ROADMAP-2.0.md`, section "All decisions"; those were Claude's recommendations at Neon's request). Where an earlier answer settled only part of a question, the row says what is still open. Neon answered D0-1, D1-2, D1-3, D3-2, D6-1, D6-3 and D7-6 the same day. That leaves 18 answered, 2 answered except a sub-question, 4 partly answered and 21 open.

| ID | Question | Status | Pieces waiting |
| :-- | :-- | :-- | :-- |
| D0-1 | How does CI come back? | Answered: a GitHub support ticket, and G0-05's timer in CT 117 until Actions is back | G0-01, G10-08 |
| D1-1 | Must the 21 vanilla Brain mobs run on the PEX Brain, or may they stay goal ports? | Answered: b) goal ports with Brain-layout NBT, the Brain where it already covers a mob (Q1.1) | G1-04, G1-05, G1-28, G1-29, G1-30, G1-31, G1-32, G1-33, G1-34, G1-35, G1-36, G1-37, G1-38, G1-39, G1-40, G1-41, G1-42, G1-43, G1-44 |
| D1-2 | What form must each mob's test take? | Answered: b) function GameTest plus a dual-run copy when no player is needed | G1-03 |
| D1-3 | How are the open upstream mob PRs (#3630, #4393, #4392, #4389, #4387, #4368, #4304, #4287, #4115) handled? | Answered: a) per piece, cherry-picking each PR; #3630 as a reference only | G1-08, G1-09, G1-11, G1-13, G1-18, G1-25, G1-29, G1-30, G1-31, G1-38 |
| D2-1 | Should native plugins get a versioned ABI that keeps them loading across releases, or stay explicitly unstable, loading only into the build they came from? | Answered: explicitly unstable (Q2.1). G2-06 dropped | G2-05, G2-06 |
| D2-2 | When v0.1 goes, what do Wasm plugin authors get? | Answered: follow upstream, v0.1 stays until upstream drops it (Q2.2). G2-07 to G2-10 not started | G2-07, G2-08, G2-09, G2-10 |
| D3-1 | What happens when a config value is bad? | Answered: per-key default and a warning, a syntax error refuses to start (Q3.1). Open: the two-key sub-question | G3-01, G3-09 |
| D3-2 | Remove or wire the keys that nothing reads? | Answered: remove them all in G3-03 | G3-03 |
| D3-3 | What should `default_difficulty` do? | Open | G3-07 |
| D3-4 | Should the server write changes made at runtime back to `pumpkin.toml`, as vanilla does with `server.properties`? | Open | G3-06 |
| D3-5 | What should happen to the defaults that point at upstream Pumpkin? | Partly (Q3.6): telemetry off by default. Open: (a) or (b), and `bug_report` | G3-08 |
| D3-6 | How should the egg set the port? | Open | G3-05 |
| D4-1 | In offline mode, which UUID should `op`, `whitelist add`, `ban`, `pardon` and `fetchprofile` use for a name the server hasn't seen? | Open | G4-03 |
| D4-2 | Should the player lists and `usercache.json` live in the server root like vanilla instead of `data/`? | Open | G4-06 |
| D4-3 | Should plugins get the old-to-new UUID mapping through the plugin API? | Open | G4-04 |
| D5-1 | How do scheduled block and fluid ticks and block entities run? | Open | G5-05, G5-15 |
| D5-2 | Is the chained neighbour update limit configurable? | Open | G5-14 |
| D5-3 | Is experimental redstone (`minecraft:redstone_experiments`) part of 2.0? | Answered: out, default redstone only for 2.0 (Q5.1) | none (scope only) |
| D5-4 | What does the proof compare? | Partly (Q5.2): position-dependent behaviour not required. Open: pass or fail, or a per-tick trace | G5-16 |
| D6-1 | What may a stage set up with commands? | Answered: `give` and `locate` per stage; only G6-22 chains without `give` | G6-06, G6-07, G6-08, G6-11, G6-13, G6-15, G6-20, G6-22 |
| D6-2 | Where do the stages that need the Nether, the End and the fixed seed run? | Answered: the shared tester server, fixed seed, both dimensions on (Q6.4) | G6-01 |
| D6-3 | Which stages run in CI's `gametest` job? | Answered: short stages in CI, the long ones nightly and before releases | G6-06, G6-07, G6-08, G6-11, G6-13, G6-15, G6-20, G6-22 |
| D6-4 | Where do the 12 pearls come from? | Open | G6-12, G6-13 |
| D6-5 | Does the dragon stage wait for Goal 1's dragon phases? | Open | G6-20 |
| D6-6 | Does anything off the start-to-dragon path belong in Goal 6 (enchanting, brewing, anvils, trading, respawn anchors)? | Open | none (scope only) |
| D7-1 | Should PEX keep NBT it doesn't model and write it back unchanged, or model each field? | Open | G7-06, G7-21, G7-23, G7-24, G7-25 |
| D7-2 | What should PEX do with worlds saved before 5023 (it accepts 4435 and up)? | Open | G7-28 |
| D7-3 | How much of a structure start should PEX save for chunks it generates? | Open | G7-07 |
| D7-4 | Which `data/` files must round-trip for 2.0? | Open | G7-15, G7-16, G7-17, G7-18, G7-19, G7-20 |
| D7-5 | What happens to plugin data (`PumpkinCustomData`) when a world goes through vanilla? | Open | G7-29 |
| D7-6 | How should block entity changes reach their chunk before it unloads (`/srv/pex/pr-drafts/ISSUE-block-entities-lost-on-unload.md`)? | Answered: flush before unload (`fix/flush-block-entities-before-unload`) | G7-10 |
| D8-1 | What does "Bedrock polish" mean for 2.0? | Answered: (c) experimental with honest gaps (Q8.1). G8-13 to G8-19 and G8-22 after 2.0 | G8-13, G8-14, G8-15, G8-16, G8-17, G8-18, G8-19, G8-22 |
| D8-2 | Which Bedrock versions does PEX support, and how does it keep up? | Open | G8-21 |
| D8-3 | Which client does the Bedrock test bot use? | Open | G8-11, G8-12 |
| D8-4 | Which UUID should a Bedrock player get? | Open | G8-20 |
| D9-1 | What is the bar against Paper, and at which sizes? | Answered: less RAM than Paper, P95 within 10% at 10, 25 and 50 bots (Q9.2) | G9-12, G9-14 |
| D9-2 | Where does the benchmark live? | Answered: move it to `tools/gameplay-bench` (Q9.3) | G9-11 |
| D9-3 | Where are benchmark results published? | Open | G9-14 |
| D9-4 | Which default Java compression level? | Open | G9-08 |
| D9-5 | Which hardware produces the published numbers? | Answered: a dedicated 4-core cloud VM, pve for development (Q9.1) | G9-14 |
| D10-1 | Which ARM64 and Windows machines run G10-09 and G10-10? | Answered: hosted runners plus one real run each (Q10.1) | G10-09, G10-10 |
| D10-2 | Which release assets must be bit-for-bit reproducible, and where can a rebuild happen? | Partly (Q10.3): x86_64 Linux only. Open: a pinned container or one host | G10-03, G10-04 |
| D10-3 | Are Windows releases signed? | Answered: unsigned for 2.0, SignPath after (Q10.2) | G10-05 |
| D10-4 | What does CI publish on the fork besides `v*` releases? | Answered: a nightly pre-release, never Latest, and Docker images (Q10.6). Open: `master` or `alpha` | G10-06 |
| D10-5 | Which targets does a 2.0 release ship, and what builds them? | Partly (Q10.4): keep musl, server only. Open: the target list, after D0-1 | G10-05 |

## Repo map

Paths are relative to the repository root. `crates/pumpkin-core/src/` is shortened to `core/` in the second table.

### Crates and top-level directories

| Path | What it owns |
| :-- | :-- |
| `crates/pumpkin` | The server binary (`src/main.rs` only). Wires the crates together. Feature `plugin-v0_2` turns on the v0.2 Wasm host. |
| `crates/pumpkin-core` | The server itself: blocks, items, entities and mob AI, commands, the ticking world, Java and Bedrock connection handling, the plugin loader and events, server tick loop, ops/whitelist/bans. See the table below. |
| `crates/pumpkin-world` | Chunk storage and region IO (`chunk/format`, `chunk/io`), the chunk system (`chunk_system/`), world generation (`generation/`), lighting, POI (`poi/`), scheduled block and fluid ticks (`tick/scheduler.rs`, `tick/order.rs`), `level.dat` and other world files (`world_info/`), GameTest instance blocks (`test_instance/`). Has its own `AGENTS.md` and benches. |
| `crates/pumpkin-protocol` | Java (`java/`) and Bedrock (`bedrock/`) packet definitions, codecs and serialization. Has its own `AGENTS.md` and fuzz targets. |
| `crates/pumpkin-data` | Vanilla data generated from `assets/` into `src/generated/` (never edited by hand), plus hand-written wrappers (`block_state.rs`, `item_stack/`, `data_component_impl/`). |
| `crates/pumpkin-config` | `pumpkin.toml` and feature config structs, one file per area (`chunk.rs`, `world.rs`, `networking/`, `player_data.rs`, ...), and `migration.rs`. |
| `crates/pumpkin-inventory` | Menus and screen handlers (crafting, furnace-like, anvil, brewing, enchanting, merchant, ...), the player inventory, click/drag handling and slot sync. |
| `crates/pumpkin-nbt` | NBT reading and writing, compression, serde support. Benches and fuzz targets. |
| `crates/pumpkin-util` | Shared types: math, noise, random, text and translation, identifiers, loot tables, permissions, `uuid.rs`, `version.rs`. |
| `crates/pumpkin-codecs` | DataFixerUpper-style codecs (`DynamicOps`, `JsonOps`, `DataResult`). Used by `pumpkin-command`, `pumpkin-nbt` and `pumpkin-util`. |
| `crates/pumpkin-command` | Brigadier-style dispatcher, parser, argument types, SNBT, suggestions. |
| `crates/pumpkin-auth` | Mojang authentication, HTTP client, JWT. |
| `crates/pumpkin-macros` | Proc macros: `#[derive(Event)]`, `cancellable`/`send_cancellable`, `#[packet]`/`#[java_packet]`, `#[pumpkin_block]`, NBT serde derives, `translate_*`. |
| `crates/pumpkin-api-macros` | `#[plugin_method]`, `#[plugin_impl]`, `#[with_runtime]`. |
| `crates/pumpkin-scheduler` | Domain-serialized async task scheduler. No other crate depends on it: `git grep pumpkin[-_]scheduler` outside the crate finds only the workspace member list in `Cargo.toml`. |
| `crates/pumpkin-gametest` | GameTest framework: `GameTestRunner`, block-based tests, structures, helpers. |
| `crates/pumpkin-plugin-wit` | WIT definitions for the Wasm API, `v0.1/` and `v0.2/`. No `Cargo.toml`. Mirrored to its own repository by `sync-wit.yml`. MIT OR Apache-2.0. |
| `crates/pumpkin-plugin-api` | SDK for Wasm plugins. MIT OR Apache-2.0. |
| `crates/pumpkin-plugin-utils` | Plugin helpers: licensing, lease caching, update checks. MIT OR Apache-2.0. |
| `crates/pumpkin-plugin-runtime` | Wasm store executor and lifecycle, used by the Wasm host crates. |
| `crates/pumpkin-wasm-host` | Wasm plugin host entry point (runtime, scheduler). |
| `crates/pumpkin-wasm-host-common` | Host plumbing shared by both API versions: plugin state, signatures, scheduler, logging. |
| `crates/pumpkin-wasm-host-v0_1`, `-v0_2` | Host bindings for each Wasm API version. |
| `native-plugins/` | PEX native plugins, Rust `cdylib`s built as workspace members: `common/`, `template/`, `worldpumpkin/`, `pexguard/`, `pexprotect/`. Also `PORTING_GUIDE.md`, `tools/server_driver.py`, `tools/testbot/` (Azalea bot, not a workspace member). Has its own `AGENTS.md`. |
| `tools/pumpkin-codegen` | Generates `pumpkin-data` and WIT data from `assets/`. Has its own `AGENTS.md` and `Cargo.lock`. |
| `tools/pumpkin-tester` | In-process server with headless Rust bots (`bot.rs`, `bot/`), bot tests (`bot_tests/`), survival stages (`survival/`) and GameTests (`gametests.rs`). Run by CI's `gametest` job. |
| `tools/pumpkin-fuzzer` | Network packet fuzzer and stress tool (modes all, raw, framed, stateful, corrupt-varint). |
| `tools/dual-run` | `dual_run.py` runs a GameTest datapack on vanilla and PEX and compares results; `build_structures.py` and `structures/redstone/*.build` (5 structures). |
| `assets/` | Vanilla data JSON from the Extractor, `bedrock/`, `translations/`, and `tests/` (worldgen reference data, test datapacks `pumpkin-dual-run`, `pumpkin-unit-test`, `pumpkin-unit-test-example`). |
| `docs/` | `ALPHA-TODO.md` (alpha status, 2026-10-16), `MOB-PARITY.md` (mob table built from the earlier roadmap's Appendix A), this file. |
| `.github/` | Workflows (see CI under G0), `scripts/ci_packages.py` (affected-package selection), issue and PR templates, `dependabot.yml`. |
| Root files | `Cargo.toml`/`Cargo.lock` (workspace, lints), `rust-toolchain.toml` (`channel = "stable"`, not pinned to a version), `Dockerfile`, `docker-compose.yml`, Nix (`flake.nix`, `flake.lock`, `default.nix`, `shell.nix`), `egg-pex.json`/`egg-pumpkin.json` (Pterodactyl/Pelican eggs), `typos.toml`, `rustfmt.toml`, `AGENTS.md`, `CONTRIBUTING.md`. |

### Inside `crates/pumpkin-core/src/`

| Path | What it owns |
| :-- | :-- |
| `core/block/` | Block behaviour (`blocks/`, registered in `blocks/mod.rs` and `registry.rs`), block entities (`entities/`), fluids (`fluid/`). |
| `core/entity/` | Entity types (`type.rs`, `from_type()`), living entities, players (`player.rs`, `player/`), mobs (`mob/`), passive mobs, bosses (`boss/`), projectiles, vehicles, decorations. |
| `core/entity/ai/` | Mob AI: `goal/` (GoalSelector goals), `brain/` and `behavior/` (Brain, sensors, memories, behaviours), `pathfinder/`, `control/`, `sensing.rs`, `target_predicate.rs`. |
| `core/item/` | Item behaviour (`items/`, `registry.rs`), potions. |
| `core/command/` | Command implementations (`commands/`, `default_dispatcher()`), argument types, client suggestions. |
| `core/world/` | The ticking world: neighbour updates (`neighbor_updater.rs`), natural spawning (`natural_spawner.rs`), entity index, tracker and islands, explosions, portals, raids, weather, time, the dragon fight (`dragon_fight.rs`, `end_podium.rs`), loot, maps, scoreboard, vibrations, villager POI. |
| `core/net/` | Java connections (`java/`), Bedrock (`bedrock/`, including NetherNet), proxies (`proxy/`), query, LAN broadcast, the JSON-RPC management API (`management/`), chunk sending, packet limiter, authentication. |
| `core/plugin/` | Native plugin loader (`loader/native.rs`), plugin API (`api/`, events in `api/events/`), permissions, `PLUGIN_API_VERSION` in `mod.rs`. |
| `core/server/` | Tick loop (`ticker.rs`, `tick_rate_manager.rs`), builtin GameTests (`builtin_game_tests/`, `server_test_manager.rs`), metrics, debug profiler, memory, recipes, key store. |
| `core/data/` | Ops, whitelist, ban lists, user cache, `offline_uuid_migration`, datapacks, advancement data. |
| `core/enchantment/` | Enchantment effects and helpers. |

## Shared tooling (G0)

| Tool | Where it lives today | State |
| :-- | :-- | :-- |
| Decompiled vanilla 26.3 server (Vineflower) | Outside the repo, as `AGENTS.md` "Getting the source" asks. CT 117 `/srv/pex/vanilla-26.3/src` (5,027 `.java` files), with `server-26.3.jar`, `bundler-26.3.jar`, `vineflower.jar` and `vf-full.log` beside it. Second copy on pve at `/Vault-Mk4/vanilla-26.3/src` (5,027 files). | Exists. No script rebuilds it; the recipe is the "Getting the source" section of `AGENTS.md`. |
| GameTest harness | In the repo. Framework `crates/pumpkin-gametest` (`GameTestRunner`). Builtin tests in `crates/pumpkin-core/src/server/builtin_game_tests/` (33 test files plus `mod.rs`) and `server_test_manager.rs`. Datapacks in `assets/tests/datapacks/`. Runner `tools/pumpkin-tester/src/gametests.rs`, run by `cargo run -p pumpkin-tester` in the `gametest` job of `.github/workflows/rust.yml`. Vanilla comparison: `tools/dual-run/dual_run.py` (5 redstone structures). | Exists. Dual-run is manual: no workflow runs it (`grep -rn dual .github/` finds nothing). See G0-02. |
| survbot | Python: CT 117 `/srv/pex/tools/survbot.py`, outside the repo (protocol 777, roles builder, miner, afk, explorer; built on `walkbot.py`, fleets via `botfleet.py`). Rust, in the repo: `HeadlessBot` in `tools/pumpkin-tester/src/bot.rs` (`connect_and_join`, `move_to`, `send_command`, `use_block`, `interact_entity`, `attack_entity`, `start_digging`/`finish_digging`, `click_slot`, `close_container`) and the one survival stage `tools/pumpkin-tester/src/survival/wood_to_stone_pickaxe.rs`, run in CI with the GameTests. Azalea: `native-plugins/tools/testbot` (nightly Rust, protocol patch). | Exists. Serverbound play packets used anywhere in `tools/pumpkin-tester/src/`: `SPlayerPosition`, `SPlayerPositionRotation`, `SConfirmTeleport`, `SChatCommand`, `SPlayerCommand`, `SSetHeldItem`, `SUseItemOn`, `SInteract`, `SAttack`, `SPlayerAction`, `SClickSlot`, `SCloseContainer`, `SChunkBatch`, `SPlayerLoaded`, `SKeepAlive`, `SPlayPingRequest`. No `SUseItem` (use item in air) and no arm swing. What survival needs beyond that belongs to Goal 6. |
| Benchmark harness against Paper | Outside the repo. CT 117 `/srv/pex/gameplay-bench`, a clone of `github.com/Conman1533/mc-gameplay-bench`: 8 bots on a seeded schedule; samples CPU and threads, memory, disk, network bytes and tick P50/P95/P99 every 10 s; `report.py` writes an HTML report. Its world is `world/base-world-262.tgz` (built on 26.2). Paper: CT 117 `/srv/pex/bins/paper/paper.jar`, Paper `26.3-169-main@46d4834` (from its `out.log`), started by `/srv/pex/tools/paper_run.sh`. Other scripts: `/srv/pex/tools/bench.sh`, `localbench.sh`, `c2bench.sh`. In the repo there are only criterion benches: `pumpkin-world` (chunk, chunk_io, chunk_gen, chunk_gen_concurrent, jigsaw, noise_router, template_pool), `pumpkin-nbt` (nbt), `pumpkin-util` (perlin), `pumpkin-data` (has_random_ticks). None compares against Paper. | Exists outside the repo. No run for the assessed commit: `find /srv/pex -newer /srv/pex/tools/local-ci.sh -name '*.html'` outside `repo/`, `wt/` and `target/` finds no report. It can't run in CT 117 as it is: `sampler.py` `cgroup()` finds each server through `docker inspect` and `/sys/fs/cgroup/system.slice/docker-<id>.scope`, and CT 117 has neither Docker nor Podman (`command -v docker podman`). `whitelist.py` `offline_uuid` kind `pumpkin` is the SHA-256 scheme PEX dropped in Goal 4's work, so PEX now needs kind `vanilla`. See G0-04 and G0-03. |
| CI | `.github/workflows/`: `rust.yml` (push to `master` and `alpha`, PRs, dispatch; jobs format, machete, clippy, clippy_release, check_wit, test, gametest, test_android, build_pull_request, build_android_pull_request, build_release, build_release_musl, build_android_release, draft_canary_release), `nightly.yml` (03:00 cron), `release.yml` (tags), `docker.yml`, `nix.yml`, `typos.yml`, `sync-wit.yml`, `reviewers.yml`. Stand-in: CT 117 `/srv/pex/tools/local-ci.sh`, outside the repo, runs the Linux jobs of `rust.yml` plus `typos` and writes `/srv/pex/ci/<branch>/<time>/summary.txt`. | Not running. `gh api repos/Conman1533/PEX/actions/permissions` says `"enabled": true`, but the newest entry in `gh run list -R Conman1533/PEX` is from 2026-10-14 (a `master` push), and `rust.yml` triggers on `alpha` pushes while `alpha` was last pushed with a 2026-10-29 commit. Checked again on 2026-10-31: all eight workflows report `active` (`gh api repos/Conman1533/PEX/actions/workflows`), and the repository is a public fork with 45 runs in total. No Windows, macOS, ARM64 or Android job runs anywhere now. See G0-01. |
| Real-client play-test rig (not asked for, in use) | CT 117 `/srv/pex/playtest/` (`README.md`): vanilla 26.3 client on Xvfb with Mesa software rendering, driven by `xdotool` and console commands, recorded with ffmpeg. | Exists. |

### Pieces

#### G0-01 CI runs on every alpha push again

- Scope: in: `rust.yml` and `typos.yml` start on pushes to `alpha` (hosted Actions, once GitHub runs them again for this repository; D0-1), and the format, machete, clippy, test and gametest jobs pass on the first such run. In: `reviewers.yml` gets a repository guard so the upstream reviewer bot doesn't run on the fork (neither it nor `sync-wit.yml` checks `github.repository`; `sync-wit.yml` goes in G2-04). Out: release reproducibility and the Windows/ARM64 hardware runs (Goal 10), new jobs (G0-02), fixing failures in tests that a goal piece owns.
- Depends on: none
- Branch: ci/run-on-alpha
- Needs decision: D0-1
- Done when: a push to `alpha` shows up in `gh run list -R Conman1533/PEX -b alpha` for `rust.yml` and `typos.yml`, and the format, machete, clippy, test and gametest jobs of that run pass.
- Needed by: G0-02, G2-02, G2-03, G7-03, G8-12, G9-13, G10-04, G10-05, G10-08, and the Proofs of Goals 1, 3, 4, 6 and 7.

#### G0-02 Dual-run job in CI

- Scope: in: a job that fetches the vanilla 26.3 server jar from piston-meta, checks its SHA-1, sets up a JDK and runs `tools/dual-run/dual_run.py` on `assets/tests/datapacks/pumpkin-dual-run`. Out: new test structures (each goal adds its own), running bot scenarios or mobs on vanilla.
- Depends on: G0-01
- Branch: ci/dual-run
- Needs decision: none
- Done when: the job runs on a push to `alpha` and passes with the 5 existing redstone structures, and a scratch branch that changes one structure's expected outcome makes it exit 1.
- Needed by: G1-03, G6-21, G7-04, and the Proofs of Goals 1, 5 and 7.

#### G0-03 Benchmark baseline: alpha against Paper

- Scope: in: gameplay-bench runs of PEX at the assessed commit and of Paper `26.3-169`, same CPU set, memory limit, seed, 8 bots and view distance 10, recorded in this file: tick P50/P95/P99, CPU, peak memory, network bytes sent per bot-hour, plus the report paths. Out: fixing anything the runs show (Goal 9), per-packet counts (Goal 9), replacing the 26.2 benchmark world.
- Depends on: G0-04
- Branch: docs/bench-baseline
- Needs decision: none
- Done when: this section has a PEX row and a Paper row, each from at least 2 runs of 60 minutes, with run-to-run spread on tick P95 under 10% (more runs if not).
- Needed by: G1-44, G5-05, G5-15, G6-10, G9-01, G9-03, G9-14, G10-09, G10-10, and the Proofs of Goals 1, 5, 9 and 10.
- Result (2026-11-03): branch `docs/bench-baseline` at `d1cedafd8` adds `docs/benchmarks/baseline-26.3.md`, which has every run, the setup and how each number is made. Seven 60-minute runs in CT 117, one server at a time on CPUs 22, 23, 24 and 27 (four physical cores) with `MemoryMax=8G`, 8 bots on the fleet's seeded schedule, view and simulation distance 10, a fresh copy of `/srv/pex/bins/base-world-262` per run, and the other two build slots free throughout. Reports: `/srv/pex/scratch/bench-baseline/<run>/report.html` in CT 117. Each value is the mean over the runs, except peak memory (the highest); P95 spread is the highest run P95 minus the lowest, over the mean.

  | Server | Runs | Tick P50 | Tick P95 | Tick P99 | CPU (cores) | Peak memory | Sent per bot-hour | P95 spread |
  | :-- | --: | --: | --: | --: | --: | --: | --: | --: |
  | PEX `300621c3e` | 3 | 12.05 ms | 22.95 ms | 39.54 ms | 0.40 | 361 MiB | 72.4 MiB | 5.7% |
  | Paper 26.3-169 (runs 1, 3, 4) | 3 | 11.64 ms | 16.31 ms | 23.68 ms | 0.38 | 1,721 MiB | 52.4 MiB | 1.6% |
  | Paper 26.3-169 (all 4 runs) | 4 | 11.45 ms | 15.88 ms | 22.90 ms | 0.38 | 1,721 MiB | 52.1 MiB | 11.8% |

  - Open for Neon: the three-run Paper row leaves out Paper run 2, whose P95 (14.57 ms) was about 10% below the other three, while the machine was as quiet as in the other runs. With it the spread is 11.8%, over this piece's 10% bar. The likely cause is the explorer bot: `survbot.py` picks its heading with an unseeded `random`, so it walks into different, newly generated terrain each run (in Paper run 2 it went 644 blocks toward +Z, where no other run went). Either keep the three-run row, or seed the explorer in mc-gameplay-bench and rerun Paper.
  - What stands out, for Goal 9: PEX's P95 is about 23 ms in every 100-tick window against Paper's 16.3 ms while the P50s match, so the tail is steady rather than a few spikes. PEX has ticks of 100 to 300 ms exactly every 6,000 ticks (the 5-minute autosave and player save), with almost none of that time inside the world tick. PEX sends about 1.4 times Paper's bytes per bot-hour, and peaks at about a fifth of Paper's memory. Network bytes vary between runs (PEX 409 to 494 MiB, Paper 309 to 346 MiB), for the same explorer reason.

#### G0-04 Gameplay benchmark runs in CT 117

- Scope: in: `sampler.py` finds a server's cgroup without Docker (for example a server started with `systemd-run --scope`, read from `/sys/fs/cgroup/system.slice/<unit>.scope`), `report.py` reads the `tick query` answers from the server's log file instead of `docker logs`, and `SERVERS.md` says PEX at `300621c3e` or later whitelists with kind `vanilla`. Lands in github.com/Conman1533/mc-gameplay-bench. Out: installing Docker in CT 117, moving the benchmark into this repository (D9-2), a 26.3 benchmark world, per-packet counts (G9-01).
- Depends on: none
- Branch: fix/no-docker (in mc-gameplay-bench)
- Needs decision: none
- Done when: a 5-minute run with 8 bots against PEX and Paper in CT 117 writes a report with CPU, memory, network and tick P50/P95/P99 for both servers, and every builder and miner bot has confirmed placements or breaks above zero.
- Needed by: G0-03.

#### G0-05 Interim CI timer in CT 117

- Scope: in: a systemd timer in CT 117 that checks `origin/alpha` every 15 minutes and runs `/srv/pex/tools/local-ci.sh --full` on each new commit in a worktree of its own, keeps each `summary.txt` under `/srv/pex/ci/alpha/`, and has a nightly slot for the long survival stages D6-3 keeps out of CI. Out: posting results to GitHub (commit statuses need a token that can write to the repository, which Neon decides on separately), Windows and ARM64 jobs, replacing G0-01.
- Depends on: none
- Branch: none (CT 117 tooling under `/srv/pex/tools`, outside the repository)
- Needs decision: none
- Done when: a push to `alpha` has a finished `summary.txt` for that commit within 60 minutes (a run took about 17 minutes on 2026-10-27), a second timer run on the same commit starts nothing, and the timer survives a reboot of CT 117.
- Needed by: the Linux CI jobs until G0-01 lands, and the nightly survival stages (D6-3).

### Decisions

- D0-1: How does CI come back? Options:
  - Ask GitHub support why Actions does not start on Conman1533/PEX. Costs nothing but the owner's time and an unknown wait. Keeps hosted Linux, Windows, macOS and ARM64 runners.
  - Self-hosted runner in CT 117. Dropped on 2026-10-31: a runner only gets the jobs GitHub schedules, so it can't help while GitHub has Actions disabled for the repository.
  - A timer in CT 117 that runs `local-ci.sh --full` on each new `alpha` commit (G0-05). No wait on GitHub and no cost, Linux x86_64 only, and the results stay in CT 117.
  - Keep `local-ci.sh` run by hand before each merge. No cost, no automatic runs, and no Windows, ARM64 or Android coverage.
  - The earlier roadmap asked the same question as Q10.5, and it was never answered.
  - Waits on it: G0-01 and G10-08, and through G0-01 everything listed under it.
  - Answered 2026-10-31 (Neon): both. Neon checks the repository's Actions settings and the account's billing page, then opens a GitHub support ticket; hosted Actions are the only route to the Windows, ARM64 and macOS runners that D10-1, D10-5 and G10-08 need. Until Actions runs again, G0-05 runs the Linux jobs in CT 117 on every new `alpha` commit.

## How to fill in a goal

- Current state: what exists on alpha today. Every claim cites a file and symbol.
  Where something is missing, say what you searched for. Nothing from memory or
  docs alone.
- Pieces: one heading per piece, "#### G<goal>-<nn> <title>", with:
  - Scope: what's in, and what's explicitly out
  - Depends on: piece IDs, or none. For a dependency on another goal not yet
    written, write "G<goal>: <what's needed>"; it gets resolved to an ID later.
  - Branch: <type>/<short-name> (type: feat, fix, refactor, perf, test, ci, docs)
  - Needs decision: D-IDs, or none
  - Done when: test names, bot scenario and its pass condition, benchmark and its
    target number
  A piece fits one session: one subsystem, one reviewable PR that lands on its own
  with its own test. Split anything bigger.
- Proof: how the whole goal is shown done, end to end, with numbers where they apply.
- Decisions for Neon: "D<goal>-<n>: question?", the realistic options, what each
  costs, and which pieces wait on it.

## Goal 1: Every 26.3 mob has its vanilla AI

Every 26.3 mob has its vanilla AI, with a parity table and a test per mob.

Start from: vanilla's EntityType list vs what PEX implements; whether PEX has both vanilla AI systems (GoalSelector goals, and Brain with sensors/memories/behaviours) and pathfinding. A first-pass parity table goes in Current state.

### Current state

Checked at alpha 300621c3e. `core/` is `crates/pumpkin-core/src/`. Vanilla paths are under `/srv/pex/vanilla-26.3/src/net/minecraft/world/entity/`.

Mob types
- `assets/entities.json` marks 90 types `"mob": true`. `core/entity/type.rs` `from_type` gives 88 of them a struct. `camel_husk` and `zombie_nautilus` fall to the `_ =>` arm, a bare `LivingEntity` with no AI. Elsewhere they appear only in `check_spawn_rules` (CAMEL_HUSK) and `core/entity/player.rs` (ZOMBIE_NAUTILUS, near line 3688).
- `docs/MOB-PARITY.md` (2ce43ef3d, 2026-10-24) has 90 rows, all "not audited". The table below is the first real pass.
- No test checks that every mob type gets a struct. `type.rs` test `mobs_are_freed_when_dropped` spawns 19 of them.

Goals
- `core/entity/ai/goal/mod.rs` `Goal` and `GoalReach`, `goal_selector.rs` `GoalSelector` (10 tests on a zombie fixture). `core/entity/mob/mod.rs` `MobEntity` holds `goals_selector`, `target_selector`, `navigator` and `brain`.
- 61 goal files. Vanilla names that PEX renamed: FloatGoal is `SwimGoal`, PanicGoal is `EscapeDangerGoal`, SwellGoal is `CreeperIgniteGoal`, NearestAttackableTargetGoal is `ActiveTargetGoal`, HurtByTargetGoal is `RevengeGoal`, LookAtPlayerGoal is `LookAtEntityGoal`, RemoveBlockGoal is `StepAndDestroyBlockGoal`, EatBlockGoal is `EatGrassGoal`, ZombieAttackTurtleEggGoal is `DestroyEggGoal`, TryFindLiquidGoal is `TryFindWaterGoal`, RandomStandGoal is `AmbientStandGoal`.
- Vanilla goals with no PEX struct (searched `struct <Name>` under `core/entity/`): BreathAirGoal, CatLieOnBlockGoal, CatSitOnBlockGoal, DolphinJumpGoal, FollowPlayerRiddenEntityGoal, GolemRandomStrollInVillageGoal, LandOnOwnersShoulderGoal, LlamaFollowCaravanGoal, MoveBackToVillageGoal, MoveThroughVillageGoal, SpearUseGoal, DefendVillageTargetGoal, NearestAttackableWitchTargetGoal, NearestHealableRaiderTargetGoal. `AmbientStandGoal` and `RunAroundLikeCrazyGoal` exist but no mob registers them.
- Entity islands: only breed, eat_grass, escape_danger, follow_parent, look_around, look_at_entity, swim, tempt and wander_around implement `Goal::tick_reach`. `MobEntity::has_audited_tick` is true only for chicken, cow, pig and sheep.
- Spears: `core/item/items/spear.rs` `SpearItem::kinetic_attack` is player-only. Vanilla zombies, husks, zombified piglins and piglins use spears (SpearUseGoal, piglin spear behaviours).

Brain
- `core/entity/ai/brain/mod.rs` `Brain`, `BrainTick`, `BrainProvider`. `activity.rs` `ALL_ACTIVITIES` has 26 activities. `memory/types.rs` `memory_module_types!` has all 116 vanilla memory types (test `registry_has_every_vanilla_type_in_declaration_order`). Memories with a codec are saved under "Brain" by `MobEntity::write_mob_nbt` and rebuilt in `read_custom_nbt`.
- Sensors: `sensing/mod.rs` `SensorType` has 5 (Dummy, NearestLivingEntities, NearestPlayers, HurtBy, NearestItems) of the 24 in vanilla `ai/sensing/SensorType.java`.
- Behaviours: 16 files in `core/entity/ai/brain/behavior/`. Vanilla has 104 classes in `ai/behavior/`, 9 in `ai/behavior/warden/`, and species classes beside each `*Ai`.
- Vanilla runs a Brain for 21 mobs: allay, armadillo, axolotl, breeze, camel, camel_husk, copper_golem, creaking, frog, goat, happy_ghast, hoglin, nautilus, piglin, piglin_brute, sniffer, tadpole, villager, warden, zoglin, zombie_nautilus. PEX runs one only for the allay (`core/entity/passive/allay.rs` `make_brain`, `allay/ai.rs` `make_brain`, `tick_brain` in its tick). The other 19 it implements are goal ports or tick code.
- The shared behaviours PEX lacks most, counted by how many vanilla Brain mobs use them: FollowTemptation 10, StartAttacking 10, StopAttackingIfTargetInvalid 9, AnimalMakeLove 8, SetWalkTargetFromAttackTargetIfTargetOutOfReach 7, MeleeAttack 7, BabyFollowAdult 6, SetEntityLookTarget 5, InteractWithDoor 4. Shared sensors missing: NEAREST_ADULT (7 mobs), FOOD_TEMPTATIONS (6), FROG_TEMPTATIONS (2), NAUTILUS_TEMPTATIONS (2).
- Saved state: `core/entity/passive/villager/mod.rs` `mob_write_nbt` writes `JobSiteX/Y/Z`, `JobSitePending` and `HomeX/Y/Z`. Vanilla keeps these as Brain memories `minecraft:job_site` and `minecraft:home`. `core/entity/mob/piglin.rs` writes its own "Brain" compound (test `piglin_memories_round_trip_through_the_vanilla_layout`).

Pathfinding and controls
- `core/entity/ai/pathfinder/mod.rs` has `PathFinder`, `GroundPathNavigation`, `FlyingPathNavigation`, `WaterBoundPathNavigation`, `WallClimberNavigation`, `AmphibiousPathNavigation`, with walk, fly, swim and amphibious node evaluators and 16 tests. `Navigator::ground` is the default.
- Callers of the other navigators (`grep -rn "Navigator::" core/entity/`): flying for allay and bee, water_bound for cod, salmon, tropical fish, pufferfish and dolphin, amphibious for drowned. `Navigator::wall_climber` has no caller, so spiders do not climb.
- Vanilla `createNavigation` also gives climbing to spider and cave spider, amphibious to axolotl, frog and turtle, water to tadpole, nautilus and guardian, and flying to parrot, wither and happy ghast. FrogPathNavigation, TurtlePathNavigation, StriderPathNavigation, CreakingPathNavigation and BabyFlyingPathNavigation have no PEX equivalent (searched the names).
- Custom move controls exist only in cube_mob, drowned, allay, bee, abstract_fish and fish, and dolphin (`grep -rln move_control`). Missing: the rabbit, axolotl, panda, turtle, fox, camel, ghast, vex, guardian, sulfur cube, creaking and phantom controls, FlyingMoveControl (parrot, wither, happy ghast) and SmoothSwimmingMoveControl (frog, tadpole, nautilus). The vex has no no-clip.

Ender dragon
- `core/entity/boss/ender_dragon/phase/mod.rs` `EnderDragonPhase` numbers its phases differently from vanilla `EnderDragonPhase`, has no SittingScanning and adds FlyToPortal. PEX neither syncs nor saves the phase. Vanilla syncs `DATA_PHASE` and saves "DragonPhase" (`boss/enderdragon/EnderDragon.java` `addAdditionalSaveData`).
- `phase/strafing.rs` puts an area effect cloud on the target instead of firing a dragon fireball. `from_type` has no DRAGON_FIREBALL.

Tests and harness
- Function GameTests are in `core/server/builtin_game_tests/`. The mob tests are `entities_and_ai.rs` `pumpkin:zombie_pathfinds_to_villager` (it spawns a husk) and `farm_animals.rs` `pumpkin:cow_follows_wheat_and_panics_when_hit`, plus pattern-spawn tests in `golems.rs` and `commands_and_entities_extended.rs`. `mod.rs` `MOCK_PLAYER_TEST_IDS` makes mock-player tests run alone.
- One block-based mob test, `pumpkin:creeper_should_run_from_cat`, lives in `assets/tests/datapacks/pumpkin-unit-test-example` and is run by `tools/pumpkin-tester/src/gametests.rs`.
- `core/server/server_test_manager.rs` creates mock players in creative mode, so hostile targeting of a player cannot be tested yet. `crates/pumpkin-gametest/src/helper.rs` `GameTestHelper` has spawn, entity-in-box, item and container asserts. It has no target, held-item or NBT assert.
- `tools/dual-run/dual_run.py` and `assets/tests/datapacks/pumpkin-dual-run` hold 5 redstone structures. No mob test runs on vanilla.
- Unit tests that touch mob AI: `cube_mob.rs`, `hoglin.rs` (2), `warden.rs`, `zombie_villager.rs`, `abstract_schooling_fish.rs`, allay (7 over `allay.rs`, `allay/ai.rs`, `allay/vibration.rs`), `goat.rs`, `parrot.rs` (3), `wandering_trader.rs` (4), villager (9), `equipment.rs` (2). Bot tests: `bot:allay_hand_transfer_and_interaction_order`, `bot:shearing_tool_loot_and_item_events`.

Upstream and branches
- Open Pumpkin-MC/Pumpkin PRs not on alpha: #3630 piglin Brain (+5113/-1277, conflicting, last updated 2026-10-06), #4393 witch, #4392 slime and magma cube, #4389 creeper, #4387 pig, #4368 spider, #4304 bat, #4287 goat, #4115 copper golem, #3955 pathfinder performance. Tracking issue #3468 targets 26.2.
- Fork branches with mob commits not on alpha (`git cherry 300621c3e`): `goat-parity` (1), `warden-contrib` (4), `wip/fish-movement` (1).

#### Parity table

V is the number of goal and target registrations in the vanilla class chain (`registerGoals`, `addBehaviourGoals`). P is the number of `add_goal` and `add_target_goal` calls in the PEX mob's files. Both are regex counts, so read them as a first-pass signal. "Brain" means vanilla drives the mob from its `*Ai` class. Vanilla nav is ground and PEX nav is ground unless the row says otherwise. PEX paths are under `core/entity/`. "freed" is `type.rs` `mobs_are_freed_when_dropped`, which only checks the mob is freed after it is dropped.

| Mob | Vanilla AI, nav | PEX type (file) | PEX AI, gaps | Tests at 300621c3e | Piece |
|---|---|---|---|---|---|
| allay | Brain (AllayAi), flying | AllayEntity (passive/allay.rs) | Brain from allay/ai.rs, flying nav, own move control; sensors and behaviours match | 7 unit; bot:allay_hand_transfer_and_interaction_order | G1-37 |
| armadillo | Brain (ArmadilloAi), ARMADILLO_SCARE_DETECTED | ArmadilloEntity (passive/armadillo.rs) | P8 generic goals; roll-up and scare in custom_server_ai_step | none | G1-04 |
| axolotl | Brain (AxolotlAi), AXOLOTL_ATTACKABLES, amphibious, own move control | AxolotlEntity (passive/axolotl.rs) | P19, ground nav; set_playing_dead(true) never called | none | G1-34 |
| bat | no goals, customServerAiStep | BatEntity (mob/bat.rs) | custom_server_ai_step, tick_flying, tick_roosting | none | G1-25 |
| bee | V17, flying | BeeEntity (passive/bee.rs) | P11, flying nav; no hive, pollinate, flower or grow-crop goals | none | G1-23 |
| blaze | V7 | BlazeEntity (mob/blaze.rs) | P6; extra SwimGoal, no MoveTowardsRestriction or HurtByTarget | freed | G1-12 |
| bogged | V12 (AbstractSkeleton), poison arrows | BoggedSkeletonEntity (mob/skeleton/bogged.rs) | P12 via SkeletonEntityBase::with_attack_intervals | freed | G1-07 |
| breeze | Brain (BreezeAi), BREEZE_ATTACK_ENTITY_SENSOR | BreezeEntity (mob/breeze.rs) | P5; no shoot, long jump or slide | none | G1-41 |
| camel | Brain (CamelAi), no goals, own move control | CamelEntity (passive/camel.rs) | P8 goals; no sit or dash | none | G1-32 |
| camel_husk | Brain (CamelAi through Camel), isMobControlled, cannot breed | none (`from_type` fallback) | no AI | none | G1-32 |
| cat | V16 | CatEntity (passive/cat.rs) | P13; no CatLieOnBlock, CatSitOnBlock or CatRelaxOnOwner | freed | G1-21 |
| cave_spider | V10 (Spider), climbs | CaveSpiderEntity (mob/cave_spider.rs) | P8, ground nav; no LeapAtTarget, no AvoidEntity(Armadillo), attack at 3 not 4, no daylight check on targets | none | G1-08 |
| chicken | V8 | ChickenEntity (passive/chicken.rs) | P8, names match, audited tick | none | G1-18 |
| cod | V4 (schooling fish), water | CodEntity (passive/cod.rs) | P4 via abstract_fish.rs, water nav, fish move control | school_limit_precedes_self_filter_and_stop_releases_a_slot; freed | G1-16 |
| copper_golem | Brain (CopperGolemAi), TransportItemsBetweenContainers, InteractWithDoor | CopperGolemEntity (passive/copper_golem.rs) | P4; no item transport | none | G1-38 |
| cow | V8 | CowEntity (passive/cow.rs) | P8, names match, audited tick | pumpkin:cow_follows_wheat_and_panics_when_hit | G1-18 |
| creaking | Brain (CreakingAi), CreakingPathNavigation, own move control | CreakingEntity (mob/creaking.rs) | P6; freeze logic in mob_tick | none | G1-40 |
| creeper | V10 | CreeperEntity (mob/creeper.rs) | P10, names match | pumpkin:creeper_should_run_from_cat (block-based); freed | G1-09 |
| dolphin | V15, water | DolphinEntity (passive/dolphin.rs) | P7, water nav, own move control; no BreathAir or DolphinJump | none | G1-17 |
| donkey | V10 (AbstractHorse) | DonkeyEntity (passive/donkey.rs) | P8; no RunAroundLikeCrazy, MountPanic or RandomStand | none | G1-19 |
| drowned | V15, amphibious, own move control | DrownedEntity (mob/zombie/drowned.rs) | P15, names match, amphibious nav, own move control | freed | G1-06 |
| elder_guardian | V6 (Guardian), water, own move control | ElderGuardianEntity (mob/elder_guardian.rs) | P5, ground nav; no laser attack; mining fatigue in custom_server_ai_step | none | G1-15 |
| ender_dragon | phase manager, no goals | EnderDragonEntity (boss/ender_dragon.rs) | phases in boss/ender_dragon/phase/; different ids, no SittingScanning, not saved or synced, no dragon fireball | none | G1-27 |
| enderman | V12 | EndermanEntity (mob/enderman.rs) | P12 under PEX names (ChasePlayer, TeleportTowardsPlayer, pick and place block) | freed | G1-10 |
| endermite | V8 | EndermiteEntity (mob/endermite.rs) | P7; priorities differ, no HurtByTarget | none | G1-09 |
| evoker | V19 (spellcaster, raider, patrol) | EvokerEntity (mob/evoker.rs) | P15; no AvoidEntity(Player, Creaking), no LongDistancePatrol | none | G1-14 |
| fox | V up to 28, own move control | FoxEntity (passive/fox/mod.rs, fox/goals.rs) | P25 including set_target_goals | freed | G1-22 |
| frog | Brain (FrogAi), FrogPathNavigation, SmoothSwimming control | FrogEntity (passive/frog.rs) | P5, ground nav; no tongue, croak, long jump or spawn laying | none | G1-35 |
| ghast | V4, own move control | GhastEntity (mob/ghast.rs) | P4 | none | G1-12 |
| giant | V0 (no registerGoals) | GiantEntity (mob/giant.rs) | P5 goals that vanilla does not have | none | G1-06 |
| glow_squid | V2 (Squid) | GlowSquidEntity (passive/glow_squid.rs) | no goals; swim and flee in squid.rs mob_tick | none | G1-16 |
| goat | Brain (GoatAi), ram, long jump | GoatEntity (passive/goat.rs) | P8; no ram or long jump | attack_damage_follows_the_age_boundary | G1-31 |
| guardian | V6, water, own move control | GuardianEntity (mob/guardian.rs) | P8, ground nav; no laser attack | none | G1-15 |
| happy_ghast | Brain (HappyGhastAi) plus 3 goals, BabyFlyingPathNavigation, FlyingMoveControl | HappyGhastEntity (passive/happy_ghast.rs) | P5, ground nav | none | G1-37 |
| hoglin | Brain (HoglinAi), HOGLIN_SPECIFIC_SENSOR | HoglinEntity (mob/hoglin.rs) | P7; no repellent avoidance, no breeding | fully_resistant_target_gets_no_throw, throw_scales_with_resistance; freed | G1-28 |
| horse | V10 (AbstractHorse) | HorseEntity (passive/horse.rs) | P8; no RunAroundLikeCrazy, MountPanic or RandomStand | none | G1-19 |
| husk | V13 (Zombie) | HuskEntity (mob/zombie/husk.rs) | P10 via zombie/mod.rs ZombieEntityBase; no SpearUse or MoveThroughVillage, ZombieAttack at 2 not 3, targets villagers with mustSee | pumpkin:zombie_pathfinds_to_villager | G1-06 |
| illusioner | V18 | IllusionerEntity (mob/illusioner.rs) | P13; no spells, no bow, HoldGroundAttack instead | none | G1-14 |
| iron_golem | V12 | IronGolemEntity (passive/iron_golem.rs) | P8; no MoveBackToVillage, GolemRandomStrollInVillage or DefendVillageTarget | pattern spawn in golems.rs | G1-24 |
| llama | V13 | LlamaEntity (passive/llama.rs) | P11; no LlamaFollowCaravan or RunAroundLikeCrazy | none | G1-20 |
| magma_cube | V6 | CubeMobEntity (mob/cube_mob.rs) | P6, names match, own move control | freed | G1-11 |
| mooshroom | V8 | MooshroomEntity (passive/mooshroom.rs) | P8 | none | G1-18 |
| mule | V10 (AbstractHorse) | MuleEntity (passive/mule.rs) | P7; no RunAroundLikeCrazy, MountPanic, RandomStand or Breed | none | G1-19 |
| nautilus | Brain (NautilusAi), NAUTILUS_TEMPTATIONS, NEAREST_ADULT, ChargeAttack, water, SmoothSwimming control | NautilusEntity (passive/nautilus.rs) | no goals, mob_tick only | none | G1-36 |
| ocelot | V10 | OcelotEntity (passive/ocelot.rs) | P10 | freed | G1-21 |
| panda | V16, own move control | PandaEntity (passive/panda.rs) | P8 | none | G1-22 |
| parched | V12 (AbstractSkeleton), weakness arrows | ParchedSkeletonEntity (mob/skeleton/parched.rs) | P12 via SkeletonEntityBase::with_attack_intervals | none | G1-07 |
| parrot | V8, flying, FlyingMoveControl | ParrotEntity (passive/parrot.rs) | P5, ground nav; no SitWhenOrdered, FollowOwner or LandOnOwnersShoulder | 3 unit (food, poison) | G1-21 |
| phantom | V4, own move control | PhantomEntity (mob/phantom.rs) | P6: the 4 vanilla goals plus 2 look goals vanilla lacks | freed | G1-25 |
| pig | V9 | PigEntity (passive/pig.rs) | P9, names match, audited tick | none | G1-18 |
| piglin | Brain (PiglinAi), PIGLIN_SPECIFIC_SENSOR, about 30 behaviours | PiglinEntity (mob/piglin.rs) | P13 plus piglin_ai.rs PiglinAi helpers (admire, barter); saves a Brain compound | piglin_memories_round_trip_through_the_vanilla_layout; freed | G1-30 |
| piglin_brute | Brain (PiglinBruteAi), PIGLIN_BRUTE_SPECIFIC_SENSOR | PiglinBruteEntity (mob/piglin_brute.rs) | P10 | none | G1-29 |
| pillager | V16 | PillagerEntity (mob/pillager.rs) | P14; no AvoidEntity(Creaking) | pillager_crossbow_enchantments | G1-13 |
| polar_bear | V12 | PolarBearEntity (passive/polar_bear.rs) | P9 | none | G1-22 |
| pufferfish | V4, water | PufferfishEntity (passive/pufferfish.rs) | P4, water nav | freed | G1-16 |
| rabbit | V15, own move control | RabbitEntity (passive/rabbit.rs) | P14; no RaidGarden | none | G1-22 |
| ravager | V14 | RavagerEntity (mob/ravager.rs) | P13; extra HoldGroundAttack, no ObtainRaidLeaderBanner | none | G1-13 |
| salmon | V4 (schooling fish), water | SalmonEntity (passive/salmon.rs) | P3 via fish.rs init, water nav, no schooling | none | G1-16 |
| sheep | V9 | SheepEntity (passive/sheep.rs) | P9, names match, audited tick | bot:shearing_tool_loot_and_item_events | G1-18 |
| shulker | V7 | ShulkerEntity (mob/shulker.rs) | P6; no ShulkerDefenseAttackGoal | freed | G1-10 |
| silverfish | V7 | SilverfishEntity (mob/silverfish.rs) | P7, names match | freed | G1-09 |
| skeleton | V12 (AbstractSkeleton) | SkeletonEntity (mob/skeleton/skeleton.rs) | P12 via SkeletonEntityBase::new and reassess_weapon_goal | none | G1-07 |
| skeleton_horse | V11 | SkeletonHorseEntity (passive/skeleton_horse.rs) | P4 | none | G1-19 |
| slime | V6 | CubeMobEntity (mob/cube_mob.rs) | P6, names match, own move control | zero_speed_requests_are_consumed_as_movement_operations; freed | G1-11 |
| sniffer | Brain (SnifferAi), FOOD_TEMPTATIONS | SnifferEntity (passive/sniffer.rs) | P7 plus a transition_to state machine | none | G1-33 |
| snow_golem | V5 | SnowGolemEntity (passive/snow_golem.rs) | P5 | pattern spawn in golems.rs | G1-24 |
| spider | V10, climbs | SpiderEntity (mob/spider.rs) | P8, ground nav; no LeapAtTarget, no AvoidEntity(Armadillo), attack at 3 not 4, no daylight check on targets | none | G1-08 |
| squid | V2 | SquidEntity (passive/squid.rs) | no goals; mob_tick and flee | none | G1-16 |
| stray | V12 (AbstractSkeleton), slowness arrows | StraySkeletonEntity (mob/skeleton/stray.rs) | P12 via SkeletonEntityBase::new | none | G1-07 |
| strider | V9, StriderPathNavigation | StriderEntity (passive/strider.rs) | P9; extra SwimGoal(0), no StriderGoToLavaGoal | none | G1-12 |
| sulfur_cube | V5 (SulfurCubeTemptGoal, SulfurCubeSearchForItemsGoal, Float, RandomDirection, KeepOnJumping), no attack, own move control | CubeMobEntity (mob/cube_mob.rs) | P6 copied from the slime, with attack and target goals vanilla lacks | freed | G1-11 |
| tadpole | Brain (TadpoleAi) plus 3 AbstractFish goals, water, SmoothSwimming control | TadpoleEntity (passive/tadpole.rs) | P7, ground nav | none | G1-35 |
| trader_llama | V17 | TraderLlamaEntity (passive/trader_llama.rs) | P12; no TraderLlamaDefendWanderingTraderGoal | none | G1-20 |
| tropical_fish | V4 (schooling fish), water | TropicalFishEntity (passive/tropical_fish.rs) | P3 via fish.rs init, water nav, no schooling | none | G1-16 |
| turtle | V9, TurtlePathNavigation, own move control | TurtleEntity (passive/turtle.rs) | P7, ground nav | none | G1-17 |
| vex | V8, own move control, no-clip | VexEntity (mob/vex.rs) | P6 ground melee and wander, no no-clip | none | G1-14 |
| villager | Brain (VillagerGoalPackages), about 50 behaviours, 5 villager sensors | VillagerEntity (passive/villager/mod.rs) | P17; no sleep, schedule, POI, farming, breeding, golem or raid behaviour; job and home in non-vanilla NBT keys | 8 unit in villager/mod.rs, 1 in villager/data.rs | G1-42, G1-43, G1-44 |
| vindicator | V19 | VindicatorEntity (mob/vindicator.rs) | P15; no VindicatorBreakDoor, Johnny or AvoidEntity(Creaking) | vindicator_weapon_enchantments | G1-13 |
| wandering_trader | V18 | WanderingTraderEntity (passive/wandering_trader.rs) | P20; avoid list splits Zombie into 4 types and misses zombified_piglin | 4 unit | G1-20 |
| warden | Brain (WardenAi), WARDEN_ENTITY_SENSOR, warden behaviours | WardenEntity (mob/warden.rs) | P7 (EmergeGoal, InvestigateGoal, melee) plus anger and vibrations; no sniff, roar, dig or sonic boom | anger_management_sorts_angry_and_players_first_and_round_trips_nbt | G1-39 |
| witch | V13 | WitchEntity (mob/witch.rs) | P10; no heal-raider or attack-player target goals, no LongDistancePatrol or banner goal | none | G1-13 |
| wither | V7, flying, FlyingMoveControl | WitherEntity (boss/wither.rs) | P3, ground nav; heads in custom_server_ai_step | pattern spawn in commands_and_entities_extended.rs | G1-26 |
| wither_skeleton | V13 | WitherSkeletonEntity (mob/skeleton/wither.rs) | P13 via SkeletonEntityBase::build with PIGLIN and PIGLIN_BRUTE targets | none | G1-07 |
| wolf | V20 | WolfEntity (passive/wolf.rs) | P20; no ResetUniversalAnger, target priorities differ | none | G1-21 |
| zoglin | Brain (Zoglin.makeBrain) | ZoglinEntity (mob/zoglin.rs) | P7 | none | G1-05 |
| zombie | V13 | ZombieEntity (mob/zombie/zombie.rs) | P10 via ZombieEntityBase; same gaps as husk | freed; goal_selector.rs fixture | G1-06 |
| zombie_horse | V9 | ZombieHorseEntity (passive/zombie_horse.rs) | P6 | none | G1-19 |
| zombie_nautilus | Brain (ZombieNautilusAi) | none (`from_type` fallback) | no AI | none | G1-36 |
| zombie_villager | V13 (Zombie) | ZombieVillagerEntity (mob/zombie/zombie_villager.rs) | P10 via ZombieEntityBase | only_iron_bars_and_beds_speed_up_the_cure | G1-06 |
| zombified_piglin | V9 (Zombie) | ZombifiedPiglinEntity (mob/zombified_piglin.rs) | P6; MeleeAttack instead of ZombieAttack, extra SwimGoal, no SpearUse, turtle egg or anger target goals | none | G1-06 |

### Pieces

Rules for every mob piece (G1-04 to G1-44):
- Check the open upstream PR for the mob first (D1-3).
- Audit against the vanilla class chain: `registerGoals` and `addBehaviourGoals`, `customServerAiStep`, `aiStep`, the `*Ai` class, `createNavigation`, and the move and look controls.
- Fill its rows in `docs/MOB-PARITY.md`, naming the test.
- Add the named function GameTests to `builtin_game_tests/<piece>.rs` and register them in `mod.rs`. Add a block-based copy where D1-2 asks for one.
- Implement `tick_reach` for every goal it writes or rewrites, or return None.
- A test for a gap must fail on 300621c3e. A test for behaviour that already exists must fail with the named goal or behaviour removed.
- Brain pieces call `tick_brain` the way `allay.rs` does.

#### G1-01 Mob GameTest helpers
- Scope: survival mock players; `GameTestHelper` asserts for a mob's target, a held item and an NBT path; mock-player tests share a batch instead of running alone; wider test grid spacing for mob tests. Out: any mob behaviour.
- Depends on: none.
- Branch: `test/mob-test-helpers`
- Needs decision: none.
- Done when: `pumpkin:zombie_targets_survival_mock_player` passes and fails with the zombie's player target goal removed; both entries of `MOCK_PLAYER_TEST_IDS` pass inside the normal batch.

#### G1-02 Parity table refresh and wiring test
- Scope: replace the 90 "not audited" rows of `docs/MOB-PARITY.md` with the table above; add a unit test that every mob type in `entities.json` gets a `MobEntity` from `from_type`. Out: fixing the two missing types.
- Depends on: none.
- Branch: `docs/mob-parity-refresh`
- Needs decision: none.
- Done when: `every_mob_type_has_an_entity_implementation` in `type.rs` passes with exceptions `camel_husk` and `zombie_nautilus` (removed by G1-32 and G1-36), and fails when either exception is dropped.

#### G1-03 Mob tests on vanilla through dual-run
- Scope: let `tools/dual-run` build and run a block-based mob test (structure with entities, test function, `/tick freeze` while building) on vanilla 26.3 and PEX; port the creeper and cat test into `pumpkin-dual-run`. Out: the other mobs' copies.
- Depends on: G0-02.
- Branch: `test/mob-dual-run`
- Needs decision: D1-2.
- Done when: `pumpkin:mob/creeper_flees_cat` passes on both servers in `dual_run.py`, and `dual_run.py` exits 1 against a scratch build with the creeper's cat AvoidEntityGoal removed.

#### G1-04 Brain animal behaviours and armadillo
- Scope: FollowTemptation, AnimalMakeLove, BabyFollowAdult, RandomLookAround, SetEntityLookTarget; sensors for food, frog and nautilus temptations, NEAREST_ADULT, NEAREST_ADULT_ANY_TYPE and ARMADILLO_SCARE_DETECTED; the armadillo on ArmadilloAi. Out: other mobs.
- Depends on: none.
- Branch: `feat/brain-animal-armadillo`
- Needs decision: D1-1.
- Done when: `pumpkin:armadillo_rolls_up_near_zombie` passes, plus unit tests for each new behaviour beside `hurt_by_remembers_the_attacker_until_it_is_gone`.

#### G1-05 Brain combat and door behaviours and zoglin
- Scope: StartAttacking, StopAttackingIfTargetInvalid, SetWalkTargetFromAttackTargetIfTargetOutOfReach, MeleeAttack, EraseMemoryIf, SetWalkTargetAwayFrom, StopBeingAngryIfTargetDead, BecomePassiveIfMemoryPresent, CopyMemoryWithExpiry, InteractWithDoor; the zoglin on its Brain. Out: piglin and hoglin sensors.
- Depends on: none.
- Branch: `feat/brain-combat-zoglin`
- Needs decision: D1-1.
- Done when: `pumpkin:zoglin_attacks_any_nearby_mob` and unit `stop_attacking_if_target_invalid_forgets_a_dead_target` pass.

#### G1-06 Zombies
- Scope: zombie, husk, zombie_villager, drowned, zombified_piglin, giant. Mob spear use (SpearUseGoal), MoveThroughVillageGoal, priorities and villager targeting in `ZombieEntityBase`, zombified piglin anger and alerting; remove the giant's goals. Out: drowned trident items and spear items.
- Depends on: G1-01.
- Branch: `fix/mobai-zombies`
- Needs decision: none.
- Done when: `pumpkin:zombie_tramples_turtle_egg`, `pumpkin:zombie_pathfinds_to_villager` (husk), `pumpkin:zombie_villager_chases_wandering_trader`, `pumpkin:zombified_piglin_alerts_pack_when_hit`, `pumpkin:drowned_throws_trident_at_player` and `pumpkin:giant_ignores_survival_player` pass.

#### G1-07 Skeletons
- Scope: skeleton, stray, bogged, parched, wither_skeleton: weapon switching, tipped arrows per variant, target lists. Out: skeleton horse traps (G1-19).
- Depends on: G1-01.
- Branch: `fix/mobai-skeletons`
- Needs decision: none.
- Done when: `pumpkin:skeleton_switches_to_melee_without_bow`, `pumpkin:stray_arrow_applies_slowness`, `pumpkin:bogged_arrow_applies_poison`, `pumpkin:parched_arrow_applies_weakness` and `pumpkin:wither_skeleton_targets_piglin` pass.

#### G1-08 Spiders
- Scope: spider and cave_spider on `Navigator::wall_climber`, LeapAtTarget, AvoidEntity(Armadillo), SpiderAttack at 4, daylight check on SpiderTargetGoal. Out: jockeys.
- Depends on: G1-01.
- Branch: `fix/mobai-spiders`
- Needs decision: D1-3 (#4368).
- Done when: `pumpkin:spider_climbs_wall_to_target` and `pumpkin:cave_spider_stops_targeting_in_daylight` pass.

#### G1-09 Creeper, silverfish, endermite
- Scope: audit creeper and silverfish; endermite priorities and HurtByTarget. Out: explosion damage.
- Depends on: none.
- Branch: `fix/mobai-small-hostiles`
- Needs decision: D1-3 (#4389).
- Done when: `pumpkin:creeper_should_run_from_cat`, `pumpkin:silverfish_wakes_friends_from_infested_stone` and `pumpkin:endermite_climbs_out_of_powder_snow` pass.

#### G1-10 Enderman and shulker
- Scope: enderman stare, teleport and block goals against vanilla; ShulkerDefenseAttackGoal and bullet attack. Out: shulker box drops.
- Depends on: G1-01.
- Branch: `fix/mobai-enderman-shulker`
- Needs decision: none.
- Done when: `pumpkin:enderman_angers_when_stared_at` and `pumpkin:shulker_shoots_bullet_at_survival_player` pass.
- Needed by: G6-13 (endermen that hunt players and drop ender pearls).

#### G1-11 Cube mobs
- Scope: slime and magma cube audit; sulfur cube rebuilt on its own vanilla goals with no attack. Out: splitting on death.
- Depends on: G1-01.
- Branch: `fix/mobai-cube-mobs`
- Needs decision: D1-3 (#4392).
- Done when: `pumpkin:slime_hops_toward_survival_player`, `pumpkin:magma_cube_targets_iron_golem` and `pumpkin:sulfur_cube_picks_up_dropped_item` pass.

#### G1-12 Blaze, ghast, strider
- Scope: blaze goal list and alerting; ghast fireball and move control; StriderGoToLavaGoal and StriderPathNavigation. Out: strider riding.
- Depends on: G1-01.
- Branch: `fix/mobai-nether`
- Needs decision: none.
- Done when: `pumpkin:blaze_alerts_nearby_blazes_when_hit`, `pumpkin:ghast_shoots_fireball_at_survival_player` and `pumpkin:strider_walks_back_to_lava` pass.
- Needed by: G6-11 (blazes that fight and drop blaze rods).

#### G1-13 Raiders
- Scope: pillager, vindicator, ravager, witch: AvoidEntity(Creaking), VindicatorBreakDoor, Johnny, ObtainRaidLeaderBanner, witch raider-heal and player targets, LongDistancePatrol. Out: raid waves.
- Depends on: G1-01.
- Branch: `fix/mobai-raiders`
- Needs decision: D1-3 (#4393).
- Done when: `pumpkin:pillager_shoots_crossbow_at_survival_player`, `pumpkin:vindicator_named_johnny_attacks_any_mob`, `pumpkin:ravager_breaks_leaves_in_its_path` and `pumpkin:witch_heals_hurt_raider` pass.

#### G1-14 Spellcasters
- Scope: evoker avoid and patrol goals; illusioner spells and bow; vex flying move control and no-clip. Out: raid spawning.
- Depends on: G1-01.
- Branch: `fix/mobai-spellcasters`
- Needs decision: none.
- Done when: `pumpkin:evoker_summons_vexes_at_survival_player`, `pumpkin:illusioner_shoots_bow_at_survival_player` and `pumpkin:vex_flies_through_wall_to_target` pass.

#### G1-15 Guardians
- Scope: guardian and elder guardian on water navigation, guardian move control, laser attack goal. Out: monument generation.
- Depends on: G1-01.
- Branch: `fix/mobai-guardians`
- Needs decision: none.
- Done when: `pumpkin:guardian_laser_kills_squid` and `pumpkin:elder_guardian_gives_mining_fatigue` pass.

#### G1-16 Fish and squid
- Scope: merge `abstract_fish.rs` and `fish.rs` so salmon and tropical fish school like cod (start from `wip/fish-movement`); pufferfish; squid and glow squid goals. Out: buckets.
- Depends on: G1-01.
- Branch: `fix/mobai-fish`
- Needs decision: none.
- Done when: `pumpkin:cod_school_follows_leader`, `pumpkin:salmon_school_follows_leader`, `pumpkin:tropical_fish_school_follows_leader` (one shared function), `pumpkin:pufferfish_puffs_up_near_survival_player`, `pumpkin:squid_flees_when_hit` and `pumpkin:glow_squid_flees_when_hit` pass.

#### G1-17 Dolphin and turtle
- Scope: dolphin BreathAir, DolphinJump and the rest of its 15 goals; turtle amphibious TurtlePathNavigation, move control, home and egg goals. Out: treasure maps.
- Depends on: none.
- Branch: `fix/mobai-dolphin-turtle`
- Needs decision: none.
- Done when: `pumpkin:dolphin_swims_up_to_breathe` and `pumpkin:turtle_returns_to_home_beach_to_lay_egg` pass.

#### G1-18 Farm animals
- Scope: chicken, cow, mooshroom, pig, sheep audit. Out: their loot.
- Depends on: G1-01.
- Branch: `fix/mobai-farm`
- Needs decision: D1-3 (#4387).
- Done when: `pumpkin:cow_follows_wheat_and_panics_when_hit`, `pumpkin:chicken_follows_seeds`, `pumpkin:mooshrooms_breed_when_fed_wheat`, `pumpkin:pig_follows_carrot_on_a_stick` and `pumpkin:sheep_eats_grass_and_regrows_wool` pass.

#### G1-19 Horses
- Scope: horse, donkey, mule, skeleton_horse, zombie_horse: RunAroundLikeCrazy, MountPanic, RandomStand, skeleton trap. Out: horse inventory.
- Depends on: G1-01.
- Branch: `fix/mobai-horses`
- Needs decision: none.
- Done when: `pumpkin:horse_throws_untamed_rider`, `pumpkin:donkey_and_horse_breed_a_mule`, `pumpkin:mule_panics_when_hit`, `pumpkin:skeleton_trap_horse_spawns_riders` and `pumpkin:zombie_horse_is_tempted_by_its_food` pass.

#### G1-20 Llamas and wandering trader
- Scope: LlamaFollowCaravanGoal, TraderLlamaDefendWanderingTraderGoal, the trader's avoid list. Out: trades.
- Depends on: none.
- Branch: `fix/mobai-traders`
- Needs decision: none.
- Done when: `pumpkin:llamas_form_a_caravan_behind_a_leashed_llama`, `pumpkin:trader_llama_defends_wandering_trader` and `pumpkin:wandering_trader_drinks_invisibility_at_night` pass.

#### G1-21 Tamables
- Scope: wolf, cat, ocelot, parrot: cat block goals, parrot flying navigation and FlyingMoveControl, SitWhenOrdered, FollowOwner, LandOnOwnersShoulder, wolf anger reset and target priorities. Out: taming items.
- Depends on: G1-01.
- Branch: `fix/mobai-tamables`
- Needs decision: none.
- Done when: `pumpkin:tamed_wolf_teleports_to_far_owner`, `pumpkin:cat_sits_on_chest`, `pumpkin:ocelot_hunts_chicken` and `pumpkin:tamed_parrot_lands_on_owner_shoulder` pass.

#### G1-22 Wild animals
- Scope: rabbit, fox, panda, polar_bear, with their move controls and missing goals. Out: fox item pickup tags.
- Depends on: G1-01.
- Branch: `fix/mobai-wild`
- Needs decision: none.
- Done when: `pumpkin:rabbit_eats_grown_carrots`, `pumpkin:fox_sleeps_during_the_day`, `pumpkin:panda_attacks_back_when_hit` and `pumpkin:polar_bear_attacks_player_near_cub` pass.

#### G1-23 Bee
- Scope: hive, pollinate, flower and grow-crop goals. Out: hive block entity changes beyond what the goals need.
- Depends on: none.
- Branch: `fix/mobai-bees`
- Needs decision: none.
- Done when: `pumpkin:bee_pollinates_flower_and_returns_to_hive` passes.

#### G1-24 Golems
- Scope: iron golem village goals and DefendVillageTargetGoal; snow golem audit. Out: golem spawning by villagers (G1-44).
- Depends on: none.
- Branch: `fix/mobai-golems`
- Needs decision: none.
- Done when: `pumpkin:iron_golem_attacks_zombie` and `pumpkin:snow_golem_throws_snowballs_at_zombie` pass.

#### G1-25 Bat and phantom
- Scope: bat roosting audit; phantom goal list (drop the 2 extra look goals) and move control. Out: phantom spawning.
- Depends on: G1-01.
- Branch: `fix/mobai-flyers`
- Needs decision: D1-3 (#4304).
- Done when: `pumpkin:bat_roosts_under_a_solid_ceiling` and `pumpkin:phantom_swoops_at_survival_player` pass.

#### G1-26 Wither
- Scope: flying navigation, FlyingMoveControl, the 7 vanilla goals, head targeting. Out: spawn pattern (already tested).
- Depends on: none.
- Branch: `fix/mobai-wither`
- Needs decision: none.
- Done when: `pumpkin:wither_targets_non_undead_mob` passes.

#### G1-27 Ender dragon phases
- Scope: vanilla phase ids and the SittingScanning phase, phase sync and the "DragonPhase" NBT key, DRAGON_FIREBALL entity for the strafe phase. Out: End generation and the fight's portal and exit logic.
- Depends on: none.
- Branch: `fix/ender-dragon-phases`
- Needs decision: none.
- Done when: unit tests `dragon_phase_round_trips_through_nbt` and `strafe_phase_shoots_dragon_fireball` and GameTest `pumpkin:dragon_fireball_leaves_breath_cloud` pass.
- Needed by: G6-19 and G6-20 (an End fight that plays like vanilla).

#### G1-28 Hoglin
- Scope: HoglinAi, HOGLIN_SPECIFIC_SENSOR, repellent avoidance, breeding. Out: zoglin conversion changes.
- Depends on: G1-04, G1-05.
- Branch: `feat/brain-hoglin`
- Needs decision: D1-1.
- Done when: `pumpkin:hoglin_avoids_warped_fungus` passes.

#### G1-29 Piglin brute and piglin sensors
- Scope: PIGLIN_SPECIFIC_SENSOR, PIGLIN_BRUTE_SPECIFIC_SENSOR, PiglinBruteAi. Out: bartering.
- Depends on: G1-01, G1-05.
- Branch: `feat/brain-piglin-brute`
- Needs decision: D1-1, D1-3 (#3630).
- Done when: `pumpkin:piglin_brute_attacks_player_in_gold_armor` passes.

#### G1-30 Piglin
- Scope: PiglinAi with admiring, bartering, hunting hoglins, spear and crossbow behaviours, Mount and Dismount; fold `piglin_ai.rs` into it. Out: barter loot table contents.
- Depends on: G1-29.
- Branch: `feat/brain-piglin`
- Needs decision: D1-1, D1-3 (#3630).
- Done when: `pumpkin:piglin_barters_gold_ingot_for_loot` passes.
- Needed by: G6-13 (piglin admiring and bartering).

#### G1-31 Goat and long jump
- Scope: GoatAi, PrepareRamNearestTarget, RamTarget, LongJumpToRandomPos, LongJumpMidJump. Out: goat horn items.
- Depends on: G1-01, G1-04.
- Branch: `feat/brain-goat`
- Needs decision: D1-1, D1-3 (#4287).
- Done when: `pumpkin:goat_rams_survival_player` passes.

#### G1-32 Camel and camel husk
- Scope: CamelAi, camel move control, sit and dash; a CamelHusk struct in `from_type` with `isMobControlled` and no breeding; remove its G1-02 exception. Out: player dash input.
- Depends on: G1-01, G1-04.
- Branch: `feat/brain-camels`
- Needs decision: D1-1.
- Done when: `pumpkin:camel_follows_cactus` and `pumpkin:husk_rider_steers_camel_husk_to_target` pass.

#### G1-33 Sniffer
- Scope: SnifferAi in place of the `transition_to` state machine. Out: sniffer egg hatching.
- Depends on: G1-04.
- Branch: `feat/brain-sniffer`
- Needs decision: D1-1.
- Done when: `pumpkin:sniffer_digs_up_seed_on_grass` passes.

#### G1-34 Axolotl
- Scope: AxolotlAi, AXOLOTL_ATTACKABLES, PlayDead and ValidatePlayDead, TryFindLiquid, amphibious navigation, move control. Out: bucket variants.
- Depends on: G1-04, G1-05.
- Branch: `feat/brain-axolotl`
- Needs decision: D1-1.
- Done when: `pumpkin:axolotl_plays_dead_when_hurt` passes.

#### G1-35 Frog and tadpole
- Scope: FrogAi (ShootTongue, Croak, LongJumpToPreferredBlock, TryFindLand, TryLaySpawnOnFluidNearLand, FROG_ATTACKABLES, IS_IN_WATER), FrogPathNavigation, TadpoleAi on water navigation. Out: frogspawn block.
- Depends on: G1-01, G1-31.
- Branch: `feat/brain-frogs`
- Needs decision: D1-1.
- Done when: `pumpkin:frog_eats_small_slime_with_tongue` and `pumpkin:tadpole_follows_slimeball` pass.

#### G1-36 Nautilus and zombie nautilus
- Scope: NautilusAi and ZombieNautilusAi, ChargeAttack, water navigation, SmoothSwimmingMoveControl; a ZombieNautilus struct in `from_type`; remove the last G1-02 exception. Out: nautilus armor.
- Depends on: G1-01, G1-04, G1-05.
- Branch: `feat/brain-nautilus`
- Needs decision: D1-1.
- Done when: `pumpkin:nautilus_follows_tempting_player` and `pumpkin:zombie_nautilus_charges_survival_player` pass.

#### G1-37 Allay and happy ghast
- Scope: allay audit and GameTest; HappyGhastAi with its 3 goals, BabyFlyingPathNavigation and FlyingMoveControl. Out: harness riding.
- Depends on: G1-01, G1-04.
- Branch: `feat/brain-allay-happy-ghast`
- Needs decision: D1-1.
- Done when: `pumpkin:allay_delivers_matching_item_to_player` and `pumpkin:happy_ghast_flies_to_tempting_player` pass.

#### G1-38 Copper golem
- Scope: CopperGolemAi, TransportItemsBetweenContainers, InteractWithDoor use. Out: oxidation.
- Depends on: G1-05.
- Branch: `feat/brain-copper-golem`
- Needs decision: D1-1, D1-3 (#4115).
- Done when: `pumpkin:copper_golem_moves_items_from_copper_chest_to_chest` passes.

#### G1-39 Warden
- Scope: WardenAi, WARDEN_ENTITY_SENSOR, the 9 `ai/behavior/warden/` behaviours, warden ground navigation; review `warden-contrib` first. Out: sculk shrieker spawning.
- Depends on: G1-01, G1-05.
- Branch: `feat/brain-warden`
- Needs decision: D1-1.
- Done when: `pumpkin:warden_sonic_boom_hits_survival_player` passes.

#### G1-40 Creaking
- Scope: CreakingAi, CreakingPathNavigation, creaking move control; move the freeze logic out of `mob_tick` where vanilla has it in the Brain. Out: creaking heart block.
- Depends on: G1-01, G1-05.
- Branch: `feat/brain-creaking`
- Needs decision: D1-1.
- Done when: `pumpkin:creaking_freezes_while_watched` passes.

#### G1-41 Breeze
- Scope: BreezeAi, BREEZE_ATTACK_ENTITY_SENSOR, LongJump, Shoot, ShootWhenStuck, Slide. Out: wind charge physics beyond the shot.
- Depends on: G1-01, G1-05.
- Branch: `feat/brain-breeze`
- Needs decision: D1-1.
- Done when: `pumpkin:breeze_shoots_wind_charge_at_survival_player` passes.

#### G1-42 Villager POI, schedule and sleep
- Scope: VillagerGoalPackages core, idle, rest and meet activities; GOLEM_DETECTED, NEAREST_BED, SECONDARY_POIS, VILLAGER_BABIES, VILLAGER_HOSTILES sensors; AcquirePoi, sleep; job site and home as Brain memories instead of `JobSiteX/Y/Z` and `HomeX/Y/Z`. Out: work, breeding, raids.
- Depends on: G1-04, G1-05.
- Branch: `feat/villager-poi-schedule`
- Needs decision: D1-1.
- Done when: `pumpkin:villager_claims_job_site_and_bed` passes and a villager saved and loaded keeps `minecraft:job_site` and `minecraft:home`.
- Needed by: G7-24 (villager job site and home round-trip in vanilla's Brain memory layout).

#### G1-43 Villager work
- Scope: work activity, per-profession work behaviours, HarvestFarmland, trade restock at the job site. Out: trade tables.
- Depends on: G1-42.
- Branch: `feat/villager-work`
- Needs decision: D1-1.
- Done when: `pumpkin:farmer_villager_harvests_and_replants_wheat` passes.

#### G1-44 Villager life
- Scope: panic, golem summoning, breeding, gossip spread, raid and hero behaviours (RingBell, GiveGiftToHero); measure villager tick cost. Out: raid waves.
- Depends on: G1-43, G0-03.
- Branch: `feat/villager-life`
- Needs decision: D1-1.
- Done when: `pumpkin:panicking_villagers_summon_iron_golem` passes, and the G0-03 benchmark with 50 villagers shows tick P95 within 10% of the G0-03 PEX baseline.

### Proof

1. `docs/MOB-PARITY.md` has 90 of 90 rows marked at parity, each naming its test.
2. `every_mob_type_has_an_entity_implementation` passes with an empty exception list.
3. The 92 per-mob GameTests named in G1-04 to G1-44 (89 new, 3 existing) pass in the gametest job of `rust.yml` on 3 consecutive alpha pushes (G0-01), with 0 retries.
4. Every block-based mob copy that D1-2 asks for passes on vanilla 26.3 and on PEX in the dual-run job (G0-02). The job summary prints the count.
5. A G0-03 gameplay benchmark rerun on the alpha that closes this goal shows tick P95 within 10% of the G0-03 PEX baseline.

### Decisions

D1-1: Must the 21 vanilla Brain mobs run on the PEX Brain, or may they stay goal ports?
- The 2026-10-29 answer to the earlier roadmap accepted goal ports that match behaviour and pass a GameTest. It is asked again because covering the 21 mobs takes 19 more sensors and about 90 behaviours, and vanilla saves their state as Brain memories that G7 needs to round-trip.
- a) Brain for all 21. Cost: G1-04 and G1-05 are large and each Brain piece replaces an existing goal port. Memories and their NBT come out matching vanilla.
- b) Goal ports for all, with saved state written by hand in vanilla's "Brain" layout as `piglin.rs` does. Cost: hand-written NBT per mob, timing drift from vanilla, and schedules (villager) are awkward as goals.
- c) Brain for villager, piglin, piglin brute, warden and the shared animal behaviours; goal ports with Brain-layout NBT for the rest. Cost: two code shapes to maintain.
- Waiting: G1-04, G1-05, G1-28 to G1-44.
- Answered 2026-10-31 (Neon accepted Q1.1): b) goal ports are fine when the observable behaviour matches vanilla and a GameTest proves it, with saved state in vanilla's Brain layout so G7-24 round-trips. A mob the PEX Brain already covers stays on the Brain.

D1-2: What form must each mob's test take?
- a) Function GameTest only. Cost: cheapest; checks PEX against our reading of vanilla, never against vanilla.
- b) Function GameTest, plus a block-based copy run on vanilla through dual-run when the scenario needs no player. Cost: each such scenario is written twice; player-facing behaviour stays function-only.
- c) Dual-run for every mob test, with a bot on vanilla standing in for the player. Cost: largest; every mob piece waits on G1-03 plus a vanilla bot harness.
- Waiting: G1-03 and the Done when of G1-04 to G1-44.
- Partly answered 2026-10-31 (Neon accepted Q1.4): mock players with a closed client are fine in function GameTests. Still open: a, b or c.
- Answered 2026-10-31 (Neon): b) a function GameTest for every mob, plus a block-based copy run on vanilla through dual-run when the scenario needs no player. G1-03 first shows that mob structures behave under dual-run (freezing ticks while they are built is untested), and until G0-02 runs the job in CI, the vanilla copies run by hand with `tools/dual-run/dual_run.py`.

D1-3: How are the open upstream mob PRs (#3630, #4393, #4392, #4389, #4387, #4368, #4304, #4287, #4115) handled?
- a) Merge them onto alpha as one batch, as was done for the ten PRs merged on 2026-10-23, then audit. Cost: #3630 conflicts and is +5113/-1277; merge work up front, less code written.
- b) Use them as references and write each piece fresh. Cost: duplicated work and drift from upstream.
- c) Wait for upstream to merge them and take them in the next sync. Cost: pieces wait on upstream; #3630 has been idle since 2026-10-06.
- Waiting: G1-08, G1-09, G1-11, G1-13, G1-18, G1-25, G1-29, G1-30, G1-31, G1-38.
- Partly answered 2026-10-31 (Neon accepted Q1.2): PEX doesn't upstream, so nothing from these pieces is offered back. Still open: a, b or c.
- Answered 2026-10-31 (Neon): a), per piece instead of one batch. Each piece starts by cherry-picking its PR's own commits, resolves the conflicts, audits the result against vanilla and adds the mob's test. On 2026-10-31, `git merge-tree --merge-base` of each PR's own changes onto `300621c3e` merged #4393 and #4389 cleanly, #4387, #4287, #4368, #4392 and #4115 conflicted in 2 to 4 files, and #4304 in 8. #3630 (about 46 conflicting files, older file layout, idle since 2026-10-06) is a reference only, since D1-1 keeps piglins as goal ports.

Carried over from 2026-10-29 and not asked again: villagers are in scope, tests stay in this repo, mock players are acceptable.

## Goal 2: Plugin API decision

Plugin API decision: a versioned native plugin ABI, or an explicitly unstable one; drop WASM v0.1.

Start from: what native-plugins/ exposes and how plugins load; everything WASM v0.1 touches (loader, deps, config, docs, plugins using it).

### Current state

**How native plugins load**

- `PluginManager::new` (`crates/pumpkin-core/src/plugin/mod.rs`) always puts `NativePluginLoader` first, and `crates/pumpkin/src/main.rs` adds `WasmPluginLoader` as the only other loader. `NativePluginLoader::can_load` (`plugin/loader/native.rs`) takes `.so`, `.dll` or `.dylib` by OS. `load` opens the file with `libloading` and reads four symbols: `PUMPKIN_BUILD_ID` (checked first, by `check_build_id`), `PUMPKIN_API_VERSION` (a `u32` that must equal `PLUGIN_API_VERSION`), `METADATA` (`LazyLock<PluginMetadata>`) and `plugin` (`fn() -> Box<dyn Plugin>`). After the build id everything crosses as Rust types, and `on_load` gets an `Arc<Context>` whose `server` is the whole `Arc<Server>` (PORTING_GUIDE §1). There is no plugin boundary narrower than the public items of `pumpkin-core`.
- The build id landed in `4ccd66fca` ("feat(plugin): refuse native plugins from a different build", 2026-10-25). `build_id` in `crates/pumpkin-core/build.rs` joins the git commit, the rustc version and commit, the target, the first 8 bytes of the SHA-256 of `Cargo.lock`, `pumpkin-core`'s own features and debug assertions; `plugin::BUILD_ID` and `BUILD_ID_SYMBOL` (`plugin/mod.rs`) carry it. A different or missing id gives `LoaderError::BuildIdMismatch` or `BuildIdMissing` (`plugin/loader/mod.rs`), which `load_plugins` logs as "Failed to load plugin from ...". Tests: `mismatched_build_id_is_refused`, `plugin_without_build_id_is_refused`, `exported_build_id_matches_this_build` (`native.rs`). Gaps, stated in `build.rs` and PORTING_GUIDE §0 rule 1: without `.git` the commit is `unknown` (the Nix package uses `lib.cleanSource` in `flake.nix`, though it builds only `--package pumpkin`), uncommitted changes don't change the id, and dependency features aren't part of it.
- `PLUGIN_API_VERSION` is 3 (`plugin/mod.rs`), the same as upstream master. It is checked after the build id, and equal build ids mean the same commit, so for committed builds this check can't fail on its own. AGENTS.md "Plugin API" still says breaking changes to the native API bump it.
- The build id shows up only in refusals: `git grep BUILD_ID` finds no log line or command that prints it. The startup line in `crates/pumpkin/src/main.rs` ("Starting Pumpkin <version> ...") has no commit; `/pumpkin` (`command/commands/pumpkin.rs`) shows the short commit (`GIT_HASH`).
- Both `pex_common::export_plugin!` (`native-plugins/common/src/lib.rs`) and `#[plugin_impl]` (`crates/pumpkin-api-macros/src/lib.rs`) export the four symbols. `#[plugin_impl]` also starts a `GLOBAL_RUNTIME` Tokio runtime per plugin, which PORTING_GUIDE §0 rule 6 forbids. No crate uses `pumpkin-api-macros`: `git grep -e plugin_impl -e pumpkin-api-macros` finds only the macro crate, `Cargo.toml`, `Cargo.lock`, `reviewers.yml`, comments and docs.

**What the in-repo plugins use**

- worldpumpkin, pexguard, pexprotect and template (6,451 lines of Rust with `common`) all use `export_plugin!` (each `src/lib.rs`). None is a Wasm plugin: the only crate depending on `pumpkin-plugin-api` is `pumpkin-plugin-utils`, and `git ls-files` lists no `.wasm` or `.wat` file.
- They reach well past `pumpkin::plugin`. Their 52 distinct `use pumpkin...` statements import from `pumpkin::server`, `pumpkin::world`, `pumpkin::entity`, `pumpkin::net`, `pumpkin::command`, `pumpkin::data::offline_uuid_migration`, `pumpkin_world::world` and `pumpkin_config::migration`. They handle 19 event types (pexguard `listeners.rs` `register`, pexprotect `lib.rs` `on_load`, worldpumpkin and template `on_load`), out of 280 `derive(... Event ...)` lines in `plugin/api/events/`.
- `EventPriority` is stored and ignored. `PluginManager::register` (`plugin/mod.rs`) and `Context::register_event` (`plugin/api/context.rs`, which both Wasm hosts call from their `context.rs`) push onto the end of the event's list, and `fire_dyn` runs the blocking handlers and then the others in list order, so the order follows plugin load order. The `EventPriority` doc comment (`plugin/api/events/mod.rs`) says lower priorities run first; PORTING_GUIDE §3.2 says protection plugins use `High` "so they run before gameplay plugins". PEXGuard registers every handler at `High` (`listeners.rs` `register`); PEXProtect registers its inspect handlers at `Highest` and its loggers at `Lowest` (`lib.rs`). Open upstream PR Pumpkin-MC/Pumpkin#4151 sorts handlers Lowest first and keeps blocking handlers first.

**How plugins ship**

- `release.yml` step "Package native plugins" zips `worldpumpkin`, `pexguard` and `pexprotect` as `pumpkin-plugins-${{ runner.arch }}-${{ runner.os }}.zip` on every glibc runner (X64 and ARM64 Linux, X64 and ARM64 Windows, `macos-latest`). The musl builds are server-only ("musl can't build the native plugins' cdylibs, and the musl server can't load them anyway"). `nightly.yml` builds the whole workspace on the glibc runners but uploads only `dist/${{ matrix.asset }}`, and `rust.yml` `build_release` and `draft_canary_release` upload only server binaries. Nightly and canary users get no plugins, while `native-plugins/README.md` says "every PEX build comes with a matching set".
- Published releases (`gh release view -R Conman1533/PEX`): v1.2.1 has only `pumpkin-plugins-X64-Linux.zip`; v1.2.0 has the X64 Linux, ARM64 Linux and X64 Windows zips. Both were built by hand (checksum file `SHA256SUMS`; `release.yml` writes `checksums.sha256`). v1.2.1 is `83642d34b`, which predates `4ccd66fca`, so alpha refuses its plugins with `BuildIdMissing`.
- `egg-pex.json` installs `pumpkin-${PUMPKIN_ARCH}-Linux-musl` unless `DOWNLOAD_URL` is set, and the `Dockerfile` downloads the musl binary, so servers set up either way by default can't load native plugins (`native-plugins/README.md`, "The static `musl` Linux builds can't load native plugins"). The Pelican "PEX Plugins" panel plugin that downloads the zip by name is not in CT 117 (`ls /root/pex-plugins-plugin` fails), so how it uses the name isn't checked here. The name is treated as an interface.
- `native-plugins/README.md` ("Always use plugin files from the same build"), `native-plugins/AGENTS.md` rule 1 and PORTING_GUIDE §0 rule 1 describe the build id check. PORTING_GUIDE §6.3 still says "A mismatched plugin is not rejected", which `check_build_id` contradicts. The top-level `README.md` doesn't mention native plugins (`grep -n -i native README.md` finds nothing).

**WASM v0.1 and what it touches**

- Loader: `crates/pumpkin-wasm-host/Cargo.toml` always depends on `pumpkin-wasm-host-v0_1`; `pumpkin-wasm-host-v0_2` is optional behind the `v0_2` feature, which `plugin-v0_2` in `crates/pumpkin/Cargo.toml` turns on ("Off until the v0.2 ABI is finished"). `plugin_api_version` (`pumpkin-wasm-host/src/runtime.rs`) reads the component's metadata version: `0.1.*` runs on a sequential engine and is refused if it imports WASI 0.3, `0.2.*` needs the feature, anything else is `UnsupportedApiVersion`. v0.1 code in that file: `engine_v0_1`, `linker_v0_1`, `setup_linker_v0_1` and the `PluginApiVersion::V0_1` arms of `PluginRuntime::engine`, `init_plugin`, `on_load`, `on_unload` and `handle_ipc_message`, plus the one in `run_task` (`pumpkin-wasm-host/src/scheduler.rs`). The enum is in `pumpkin-wasm-host-common/src/plugin.rs`. `StoreMode::Sequential` (`pumpkin-plugin-runtime/src/executor.rs`) serves only v0.1 (`init_plugin` maps v0.2 to `Concurrent`); `LegacySyncReentry` is used by both hosts.
- Tests that touch v0.1: in `runtime.rs`, `v0_1_plugin_is_identified_by_its_metadata_export`, `v0_1_plugin_importing_wasi_p3_is_rejected_with_the_import_name`, `components_without_a_usable_metadata_export_are_rejected` and `v0_2_plugins_are_rejected_without_the_feature`; the two in `pumpkin-wasm-host-v0_1` (`server_list_ping_applies_and_consumes_returned_resources`, `wit_particle_ids_match_internal_particle_ids`), which v0.2 has copies of; the 20 tests of `pumpkin-plugin-api`, which builds against v0.1; the `pumpkin-plugin-runtime` tests that loop over both store modes (`lib.rs`, `MODES`).
- SDK: `pumpkin-plugin-api` generates its bindings from `../pumpkin-plugin-wit/v0.1` (`src/lib.rs`, `wit_bindgen::generate!`, `with` keys at `@0.1.0`), so the repo's only Wasm SDK builds v0.1 plugins, and `pumpkin-plugin-utils` depends on it. Upstream's v0.2 SDK is a separate crate, `pumpkin-plugin-api-v0_2`, in open PR Pumpkin-MC/Pumpkin#3962 (+11,341 lines, updated 2026-10-31).
- Size: Rust lines in `pumpkin-wasm-host-v0_1` 34,434 (47 files), `-v0_2` 35,271, `-common` 1,473, `pumpkin-wasm-host` 1,083, `pumpkin-plugin-runtime` 3,662, `pumpkin-plugin-api` 24,072, `pumpkin-plugin-utils` 830. WIT: `v0.1` has 46 files and 16,011 lines, `v0.2` 47 files (it adds `gametest.wit`) and 16,647 lines. The v0.1 host's debug rlib is 432 MB, against 204 MB for `pumpkin-core` (CT 117 `/srv/pex/target/block-entity-bookkeeping/debug/deps`, built 2026-10-27).
- Cargo: `pumpkin-wasm-host-v0_1` and `-v0_2` declare the same dependencies (their `Cargo.toml`), so dropping v0.1 alone removes no external crate. Removing every Wasm crate leaves 75 external crate versions unreachable, among them wasmtime 50, cranelift, wit-bindgen, wasmparser, wasm-encoder, postcard, tracing-serde-structured and zstd (`cargo tree --offline -e normal,build` of the workspace compared with the same for the non-Wasm members).
- Config: no key in `crates/pumpkin-config/src/plugins.rs` is read by the v0.1 host (`git grep` per key; v0.1 reads only gameplay settings, in `server.rs`). Keys for the Wasm host as a whole: `allow_unsigned` (through `PluginLoader::is_signed`, which is `true` for native plugins), `verify_signatures` (`main.rs`), and `inherit_env`, `loopback_only`, `max_memory_mb` and `overrides.<name>.environment` (`pumpkin-wasm-host/src/runtime.rs` `on_load`). Wasm-only in effect: `hot_reload` (`start_watcher` reacts only to `.wasm` files), and `ask_permission_confirmation`, `allowed_permissions` and `blocked_permissions`, which `check_permissions_cached` applies to `metadata.permissions`. `export_plugin!` leaves that list empty, and the names in `plugin/permissions.rs` are read only by the Wasm hosts. `enabled` and `overrides.<name>.enabled` apply to native plugins too (`load_plugins`). The fixtures `crates/pumpkin-config/tests/fixtures/pumpkin-v1.{0,1,2}.0.toml` contain these keys. Core also carries `LoaderError::WasmInitializationError`.
- CI and docs: `rust.yml` `check_wit` runs `for version in v0.1 v0.2`; `.github/scripts/ci_packages.py` `WIT_CONSUMER_PACKAGES` lists `pumpkin-wasm-host-v0_1`; `reviewers.yml` maps its path to upstream reviewers; the root `Cargo.toml` has the member, `[profile.release.package.pumpkin-wasm-host-v0_1]` and the workspace dependency; AGENTS.md "Layout" and "Plugin API" name the crate. `tools/pumpkin-codegen` generates only v0.2 (`src/wit/mod.rs` has only `v0_2`), so the v0.1 WIT is edited by hand. `sync-wit.yml` pushes `crates/pumpkin-plugin-wit` to `Pumpkin-MC/pumpkin-plugin-wit` on pushes to `master`, with a Pumpkin-MC app token from `vars.BOT_APP_CLIENT_ID` and `secrets.BOT_APP_PRIVATE_KEY`; `gh run list -R Conman1533/PEX -w sync-wit.yml` lists no run. AGENTS.md "The WIT is public" describes that mirror.
- v0.2 is unfinished. In `crates/pumpkin-wasm-host-v0_2/src`, `gametest.rs` returns "... not implemented" 143 times and has 41 "TODO: make this async"; `commands/mod.rs` returns "require_with_handler_id not implemented"; `events/server.rs` returns "Modifying packets from WASM is not yet supported" and has a `panic!` for the same case. The v0.2 WIT has 43 fields or cases of type `inexpressible-host-type`, codegen's placeholder for a Rust type it can't map (`common.wit`), in `bedrock-packets.wit` and `java-packets.wit`. Upstream master (`645895098`, 2026-10-31) still keeps `plugin-v0_2` off and the SDK on v0.1, and its draft PR #4405 turns every WIT event into a resource, which breaks the v0.2 ABI.
- v0.1 costs edits whenever core changes: 24 commits in alpha's history since 2026-10-01 touch `crates/pumpkin-wasm-host-v0_1` or `crates/pumpkin-plugin-wit/v0.1` (`git log --since=2026-10-01 300621c3e -- <both paths>`), two of them PEX-only (`463a0f480`, `800b194c8`) that just follow core changes. Upstream patched the v0.1 WIT again on 2026-10-31 (`645895098`).
- CT 117 has no `wasm32-wasip2` target (`rustup target list --installed` shows only the x86_64 gnu and musl targets), so no Wasm plugin can be built there yet.

### Pieces

#### G2-01 Event handlers run in priority order

- Scope: in: `PluginManager::register` and `Context::register_event` insert handlers by `EventPriority`, Lowest first as the enum's doc comment and Pumpkin-MC/Pumpkin#4151 have it, keeping registration order within a priority and all blocking handlers before the others. Rewrite the priority bullet in PORTING_GUIDE §3.2 to match, and check that PEXGuard (`High`) and PEXProtect (`Highest` inspect, `Lowest` loggers) still do what they meant. Out: events that are declared but never fired or that ignore `cancelled()`, the Wasm guest side.
- Depends on: none
- Branch: fix/plugin-event-priority
- Needs decision: none
- Done when: a new unit test in `plugin/mod.rs` (for example `handlers_run_in_priority_order`) registers blocking and non-blocking handlers at all five priorities out of order, through both paths, and sees them called Lowest to Highest with blocking first; a testbot run (PORTING_GUIDE §8.4) with PEXGuard and PEXProtect loaded shows a non-member's block break inside a region refused, and `/co` inspect on that block still shows its history.

#### G2-02 Automated native plugin load test

- Scope: in: a script beside `native-plugins/tools/server_driver.py` that builds `pumpkin` and the three plugins in one invocation, starts the server in a scratch folder and checks three things: a `Loaded <name> (<version>)` line (`plugin/mod.rs`) for each bundled plugin; a copy of `libpextemplate.so` with one byte of its `PUMPKIN_BUILD_ID` changed is refused with "Plugin build mismatch"; the plugins from the v1.2.1 `pumpkin-plugins-X64-Linux.zip`, pinned by SHA-256, are refused with "Plugin has no build id". A CI job runs it. Out: Windows and macOS runs (Goal 10), Wasm plugins.
- Depends on: G0-01
- Branch: test/native-plugin-load
- Needs decision: none
- Done when: the script exits 0 on `alpha` and 1 on a scratch commit that skips `check_build_id`, and its job passes on an `alpha` push.

#### G2-03 Nightly and canary releases ship matching plugin zips

- Scope: in: `nightly.yml` and the `rust.yml` canary path package the three plugins on each glibc runner the way `release.yml` does, with the same `pumpkin-plugins-<arch>-<os>.zip` names, and upload them next to the server. Out: `release.yml` (already does it), musl and Android (they can't load plugins), the musl default in `egg-pex.json` and the `Dockerfile`, signing.
- Depends on: G0-01
- Branch: ci/nightly-plugin-zips
- Needs decision: none
- Done when: the first nightly release after the merge has `pumpkin-plugins-X64-Linux.zip`, `-ARM64-Linux.zip`, `-X64-Windows.zip`, `-ARM64-Windows.zip` and `-ARM64-macOS.zip` next to the matching binaries, and G2-02's script passes with that nightly's X64 Linux binary and zip.

#### G2-04 Remove the WIT mirror job

- Scope: in: delete `.github/workflows/sync-wit.yml` and its entry in `RUST_CI_IGNORED_FILES` (`.github/scripts/ci_packages.py`), and rewrite the AGENTS.md "The WIT is public" bullet, since PEX can't push to Pumpkin-MC's mirror. Out: `check_wit`, which stays while any WIT does.
- Depends on: none
- Branch: ci/drop-wit-mirror
- Needs decision: none
- Done when: `git grep -n -e sync-wit -e pumpkin-plugin-wit.git -- .github AGENTS.md` finds nothing and `/srv/pex/tools/local-ci.sh` passes on the branch.

#### G2-05 State and show the unstable native ABI (if D2-1 is "unstable")

- Scope: in: log `plugin::BUILD_ID` once at startup next to the "Starting Pumpkin" line (`crates/pumpkin/src/main.rs`) and add it to the `/pumpkin` version hover (`command/commands/pumpkin.rs`); remove the `PUMPKIN_API_VERSION` check, `PLUGIN_API_VERSION`, `LoaderError::ApiVersionMissing` and `ApiVersionMismatch`, and their export in `export_plugin!` and `#[plugin_impl]`; rewrite PORTING_GUIDE §6.3 to match §0 rule 1, replace the AGENTS.md sentence about bumping `PLUGIN_API_VERSION` with the policy, and add a short native plugins paragraph to the top-level `README.md` that links `native-plugins/README.md`. Out: build ids for dirty trees or trees without `.git` (a hash of uncommitted changes would go stale, because `build.rs` reruns only on `.git/HEAD`, `.git/refs/heads/` and `Cargo.lock`), third-party native plugins.
- Depends on: none
- Branch: feat/unstable-plugin-abi
- Needs decision: D2-1
- Done when: the three `native.rs` build id tests pass; a boot test with `server_driver.py` logs the build id once and loads the three plugins; `git grep -n PLUGIN_API_VERSION` finds nothing; G2-02's script, if merged, still passes.

#### G2-06 Versioned native ABI design (if D2-1 is "versioned")

- Scope: in: `docs/PLUGIN-ABI.md` with the chosen mechanism (from D2-1), the covered surface starting from what the bundled plugins use (the 19 event types and the 52 `use` statements above), the compatibility promise (which releases load which plugins), how the build id check changes, and the implementation split into G2 pieces added to this section. Out: any implementation.
- Depends on: none
- Branch: docs/plugin-abi-design
- Needs decision: D2-1
- Done when: Neon approves the document and its pieces are listed here with IDs.

#### G2-07 Wasm v0.2 on by default, with a v0.2 SDK (if D2-2 is "v0.2 only")

- Scope: in: take Pumpkin-MC/Pumpkin#3962 (`pumpkin-plugin-api-v0_2`) as merged upstream, or as it stands if it hasn't merged; make `pumpkin-wasm-host-v0_2` a plain dependency of `pumpkin-wasm-host` and remove the `v0_2` and `plugin-v0_2` features and their `cfg` arms in `runtime.rs`; return an error instead of the `panic!` in `pumpkin-wasm-host-v0_2/src/events/server.rs`; add a minimal example plugin built with the v0.2 SDK. Out: the stubbed `gametest.wit` host, upstream's event rework (#4405), removing v0.1 (G2-08).
- Depends on: none
- Branch: feat/wasm-v0-2-default
- Needs decision: D2-2
- Done when: `v0_2_plugins_are_accepted_with_the_feature` runs without a feature flag and passes; a boot test loads the example plugin, built for `wasm32-wasip2`, on a server from a plain `cargo build -p pumpkin`, and it logs a line when a bot joins.

#### G2-08 Remove WASM v0.1

- Scope: in: delete `crates/pumpkin-wasm-host-v0_1`, `crates/pumpkin-plugin-wit/v0.1` and the v0.1 SDK `crates/pumpkin-plugin-api`, and move `pumpkin-plugin-utils` (it uses `Context` and `MarketplaceMetadata`, `src/lib.rs` and `src/models.rs`) to the v0.2 SDK; remove the v0.1 engine, linker and match arms in `pumpkin-wasm-host/src/runtime.rs`, the arm in `scheduler.rs` `run_task` and `PluginApiVersion::V0_1`, and make `plugin_api_version` refuse `0.1.*` with a message that names v0.2; remove `StoreMode::Sequential` from `pumpkin-plugin-runtime` with its tests if nothing else uses it; update the root `Cargo.toml` (member, release profile, workspace dependency), the `check_wit` loop, `WIT_CONSUMER_PACKAGES`, `reviewers.yml`, AGENTS.md "Layout" and "Plugin API", and the crate names in `README.md` and `assets/NOTICE.md`. Out: config keys (none is v0.1-only).
- Depends on: G2-07
- Branch: refactor/drop-wasm-v0-1
- Needs decision: D2-2
- Done when: `git grep -n -E "pumpkin-wasm-host-v0_1|pumpkin-plugin-wit/v0\.1|V0_1|@0\.1\.0" -- crates tools .github AGENTS.md Cargo.toml` finds nothing; `cargo metadata --locked` lists no `pumpkin-wasm-host-v0_1`; a test that feeds `plugin_api_version` a component with metadata `0.1.0` gets the new message; `cargo test --workspace` and clippy pass; G2-07's boot test still passes.

#### G2-09 Remove Wasm plugin support (if D2-2 is "native only")

- Scope: in: delete `pumpkin-wasm-host`, `-common`, `-v0_1`, `-v0_2`, `pumpkin-plugin-runtime`, `pumpkin-plugin-api`, `pumpkin-plugin-utils` and `pumpkin-plugin-wit`; the `WasmPluginLoader` wiring in `crates/pumpkin/src/main.rs` and the `plugin-v0_2` feature; the `wit` and `sdk` outputs of `tools/pumpkin-codegen`; `check_wit`, the WIT handling in `ci_packages.py` and the matching `reviewers.yml` entries; the Wasm-only workspace dependencies and release profiles; the WIT and SDK parts of AGENTS.md "Plugin API", `README.md` and `assets/NOTICE.md`. A `.wasm` file in `plugins/` logs one line saying Wasm plugins aren't supported. Out: config keys and core hooks (G2-10).
- Depends on: none
- Branch: refactor/drop-wasm
- Needs decision: D2-2
- Done when: `cargo tree --workspace -i wasmtime` finds no such package; the 75 Wasm-only crate versions are gone from `Cargo.lock` (same `cargo tree` comparison as above); `cargo test --workspace`, clippy and the gametest run pass; a boot test loads the three native plugins and logs the one line for a dummy `.wasm` file.

#### G2-10 Remove Wasm-only plugin config and core hooks

- Scope: in: drop `allow_unsigned`, `verify_signatures`, `inherit_env`, `loopback_only`, `max_memory_mb`, `hot_reload`, `ask_permission_confirmation`, `allowed_permissions` and `blocked_permissions`, and every `PluginOverride` field but `enabled`, from `crates/pumpkin-config/src/plugins.rs`, through a `Step::Config` migration (`migration.rs`) that removes them from `pumpkin.toml` with a backup and logs one line per key; remove `PluginLoader::is_signed`, `LoaderError::WasmInitializationError`, the hot-reload watcher (`start_watcher`), `check_permissions_cached`, `plugin/cache.rs` and `plugin/permissions.rs`. Out: `enabled` and `overrides.<name>.enabled`.
- Depends on: G2-09, G3-03 (the migration step that removes `world.chunk.write_in_place`; this one takes the next `config_version`)
- Branch: refactor/drop-wasm-config
- Needs decision: D2-2
- Done when: pumpkin-config tests load the v1.0.0, v1.1.0 and v1.2.0 fixtures and migrate them, with a backup, to files without these keys; a test checks one log line per removed key; a boot test with a copy of a 1.2 `pumpkin.toml` starts and loads the three plugins.

### Proof

- Loading: G2-02's script, run on a release build of the finished goal, loads 3 of 3 bundled plugins, refuses the byte-patched plugin with "Plugin build mismatch" naming both ids, and refuses 3 of 3 plugins from the v1.2.1 zip with "Plugin has no build id". With an unstable ABI (G2-05) the startup log shows the build id and `git grep PLUGIN_API_VERSION` finds nothing. With a versioned ABI (G2-06's pieces) a plugin built against the previous ABI version also loads and passes its bot test.
- Shipping: the first nightly after G2-03 has a `pumpkin-plugins-<arch>-<os>.zip` next to each of its 5 glibc binaries, and G2-02's script passes on the X64 Linux pair.
- Events: G2-01's unit test and its PEXGuard plus PEXProtect bot test pass.
- WASM v0.1 is gone: the `git grep` in G2-08's "Done when" finds nothing and `sync-wit.yml` is gone (G2-04). If v0.2 stays, a `0.1.0` component is refused with a message naming v0.2, and the v0.2 example plugin loads on a default build and logs a bot's join. If Wasm goes, `cargo tree --workspace -i wasmtime` finds no package, the 75 Wasm-only crate versions are gone from `Cargo.lock`, and a 1.2 `pumpkin.toml` migrates without the Wasm-only keys.
- PORTING_GUIDE §3.2 and §6.3, AGENTS.md "Plugin API" and `native-plugins/README.md` describe the handler order and ABI policy that these tests check.

### Decisions

- D2-1: Should native plugins get a versioned ABI that keeps them loading across releases, or stay explicitly unstable, loading only into the build they came from? Options:
  - Explicitly unstable, which alpha already enforces through the build id. Costs one session (G2-05); G2-02 and G2-03 are wanted either way. Plugins can only be built inside the PEX workspace (PORTING_GUIDE §0 rule 1), so in practice there are no third-party native plugins; admins replace plugins with every server update, as `native-plugins/README.md` already says; the panel plugin keeps installing per release. The earlier roadmap recorded this answer on 2026-10-29, with 2.0 covering only the three bundled plugins.
  - Versioned C ABI: a separate plugin-facing crate of `#[repr(C)]` types and versioned function tables, and plugins lose direct `Arc<Server>` access. It has to cover at least the 19 event types and the server, world, entity, command and config items the bundled plugins import, and the 6,451 lines of plugin code get rewritten against it. Many sessions, then every change to a covered type or event needs a version bump or a shim, including the ones upstream merges bring in.
  - Versioned through `abi_stable` or `stabby`: the same narrowing with Rust-shaped stable types and less hand-written `unsafe`, plus a new dependency tree. `PluginFuture` (a boxed `dyn Future`) and `Arc<Context>` need stable wrappers. About as many sessions as the C ABI.
  - Native stays unstable, and the Wasm v0.2 API is the stable option for third parties. Needs D2-2 to keep v0.2, and costs G2-05 plus G2-07. The promise is then upstream's, and upstream is still breaking v0.2 (#4405); its draft #2828 explores running WIT plugins as native cdylibs, which could later give native speed on the same API.
  - Waits on it: G2-05 (unstable) or G2-06 (versioned). G2-01 to G2-04 and G2-07 to G2-10 don't wait on it.
  - Answered 2026-10-31 (Neon accepted Q2.1): explicitly unstable. Plugins load only into the build they came from, with no ABI promise across releases. G2-05 goes ahead and G2-06 is dropped.
- D2-2: When v0.1 goes, what do Wasm plugin authors get? Options:
  - v0.2 becomes the only Wasm API now (G2-07, then G2-08). Two sessions, plus taking upstream's open #3962 (+11,341 lines). PEX plugin authors then build against an ABI upstream is still breaking (#4405) and that is stubbed in places (`gametest.rs`), and every upstream commit to the v0.1 host becomes a modify/delete conflict at merge time.
  - Drop Wasm plugins (G2-09, then G2-10). Two sessions, mostly deletion: about 100,800 lines of Rust, 32,700 lines of WIT and 75 external crate versions. No sandboxed plugin option is left, and every upstream merge that touches those crates conflicts. Nothing in the repo uses Wasm today.
  - Keep v0.1 until upstream drops it (the earlier roadmap's 2026-10-29 answer). Nothing to do now, but this goal's "drop WASM v0.1" stays open with no date: upstream master still defaults to v0.1 on 2026-10-31. v0.1 keeps needing edits with core changes (24 commits since 2026-10-01).
  - Waits on it: G2-07 and G2-08 (first option) or G2-09 and G2-10 (second). G2-04 doesn't wait on it.
  - Answered 2026-10-31 (Neon accepted Q2.2): keep v0.1 until upstream drops it, take upstream's v0.2 host as upstream finishes it, and spend no PEX pieces on Wasm. G2-07 to G2-10 are not started. v0.1 leaves with the upstream merge that removes it, so this goal's "drop WASM v0.1" has no date.

## Goal 3: Config cleanup

Config cleanup: config_version + automatic migration, remove dead keys (write_in_place).

Start from: every config key and where it's read; keys nothing reads; any existing versioning or migration code.

### Current state

Paths in this section: `cfg/` is `crates/pumpkin-config/src/`, `core/` is `crates/pumpkin-core/src/`, `pw/` is `crates/pumpkin-world/src/`, `wh/` is `crates/pumpkin-wasm-host/src/`, `bin/` is `crates/pumpkin/src/main.rs`.

**One file.** Every server setting is in `pumpkin.toml` (`PumpkinConfig`, `cfg/lib.rs`, `get_path`). The separate features file is gone: `c8633b78d` ("chore: merge config files", 2026-05-11, an ancestor of 300621c3e) merged it, and `git grep -i "features\.toml\|configuration\.toml"` finds nothing. `bin/` `main` calls `PumpkinConfig::load(&exec_dir)` before `init_logger`, so load results come back as `LoadMessage`s and are logged afterwards. Ops, whitelist and ban lists are JSON files handled by `core/data/mod.rs` (`LoadJSONConfiguration`) and are not part of this goal.

**Versioning and migration already exist.** `feat/config-version` (`8569d579d`) is in alpha (`git merge-base --is-ancestor` succeeds), and so is `feat/write-in-place-default` (`38e1f51bd`). In `cfg/migration.rs`:
- `CONFIG_VERSION_KEY` is `config_version`. A file without it is `UNVERSIONED_CONFIG_VERSION = 1` (the PEX 1.0 to 1.2 layout). `Migration { version, step }` holds a `Step::Config(fn(&mut DocumentMut) -> Vec<String>)` or a `Step::ServerFiles`.
- `PumpkinConfig::MIGRATIONS` (`cfg/lib.rs`) has one entry, `OFFLINE_UUID_VERSION = 2` as `Step::ServerFiles`, so new files get `config_version = 2`. A const assert on `migrations_are_ordered` rejects an unordered list at compile time.
- `load_existing` parses with `toml_edit`. `migrate` runs the newer `Config` steps, then `add_missing_keys` (adds defaults and keeps comments, order and unknown keys), then `set_config_version`. Nothing removes or renames a key: `git grep "Step::Config"` finds only the unit tests in `cfg/migration.rs`.
- Before a changed file is written, `save_migrated` copies it to `pumpkin.toml.v<old>.bak` (`write_backup` never overwrites and adds ` (n)` instead), then writes the new file with plain `fs::write`. While a `ServerFiles` step is due, `load` returns a `PendingSave`. `bin/` `main` saves it only after `migrate_offline_uuids` and `migrate_world_uuids` succeed, and exits 1 otherwise.
- A newer or non-integer `config_version` gives one warning and the file is left as it is. Its keys are still read, and missing keys get defaults in memory.
- If a file is set back to `config_version = 1`, every step after 1 runs again (`migrate` filters on `migration.version > version`). Each future `Config` step therefore has to be safe on a file it already migrated.
- Tests: `crates/pumpkin-config/tests/config_version.rs` has 5 tests (`release_files_migrate_to_the_current_version`, `current_file_is_left_byte_for_byte`, `an_earlier_backup_is_not_overwritten`, `file_from_a_newer_server_is_left_unchanged`, `file_keeps_its_version_until_the_server_files_are_migrated`) on `tests/fixtures/pumpkin-v1.0.0.toml`, `-v1.1.0.toml` and `-v1.2.0.toml`. `cfg/migration.rs` has 2 more. The first test checks that the only keys added to a 1.x file are `[world.entity_islands]` and `networking.java.validate_player_names`, and that every original line survives. `v1.2.1` changed no config code (`git diff --stat v1.2.0 v1.2.1 -- crates/pumpkin-config` is empty).

**One bad value resets every setting, without a message.** `load_existing` ends with `toml::from_str(&document.to_string()).unwrap_or_else(|_| C::default())`. A single value of the wrong type, or one out of range for its type (`view_distance = 0` for a `NonZero<u8>`, `max_players = -1`), makes every setting its default, and no `LoadMessage` is added. The file on disk is not touched.
- A TOML syntax error logs "Couldn't parse TOML ... Using default config." and the server starts on defaults as well.
- On defaults the world folder is `world` and the seed is random. A server with a different `default_level_name` therefore starts a new world (the `WorldInfoError::InfoNotFound` branch of `Server::new`), and `white_list` is off.
- `PumpkinConfig::validate` uses `assert!` for `keep_alive_time > 0`, view distances in 2..=64, `online_mode` without `encryption`, and `allow_chat_reports` without `online_mode`. Those cases panic inside `load`, before the logger exists.
- Vanilla falls back one key at a time (`Settings.get`: `MoreObjects.firstNonNull(deserializer.apply(value), defaultValue)`), and in offline mode it treats secure profiles as off instead of failing (`DedicatedServer.enforceSecureProfile`). The only per-key fallback in PEX is `deserialize_margin` for `world.entity_islands` (`cfg/world.rs`).

**Unknown keys are ignored without a message.** No struct has `deny_unknown_fields` (`git grep deny_unknown_fields crates/pumpkin-config` finds none) and the loader never checks for unknown keys. A typo such as `onlinemode = false` silently keeps the default. The serialized defaults are not a complete list of known keys:
- 13 keys default to `None` and are never written: `favicon_path`, `networking.lan_broadcast.motd` and `.port`, `networking.java.authentication.url`, `.profile_by_name_url`, `.profile_by_uuid_url`, `.prevent_proxy_connection_auth_url` and `.services_url`, `networking.bedrock.authentication.url`, `chat.anti_spam.chat_spam_threshold_seconds` and `.command_spam_threshold_seconds`, `plugins.max_memory_mb`, and `telemetry.server_name`.
- `[world.chunk.compression]` is written only for `type = "anvil"`.
- Three tables are maps: `commands.overrides`, `plugins.overrides` and `server_links.custom`.
- Serde aliases are accepted in `cfg/lib.rs`, `chat.rs`, `networking/java.rs`, `networking/auth.rs` and `networking/packet_limiter.rs` (`accepts-transfers`, `keep-alive-time`, `keep_alive_interval`, `fallback_urls`, `max-packet-rate` and others).

**write_in_place.** PEX's `38e1f51bd` (2026-10-12) made it default to true. Upstream #4062 (`b7174db92`, 2026-10-16, also in alpha) then removed the field from `AnvilChunkConfig` (`cfg/chunk.rs`). At 300621c3e, `git grep write_in_place` finds only line 48 of the 1.0.0 and 1.1.0 fixtures, the `egg-pex.json` description and the roadmap. A migrated 1.0 or 1.1 file still contains `write_in_place = true`, which nothing reads, and `release_files_migrate_to_the_current_version` checks that the line stays. Outside the repo, 41 files in CT 117 `/srv/pex/tools` still set it (`compat.sh`, `localbench.sh`, `longrun.py`, `iso_run.sh`, the `iso-*.toml` and `c2-*.toml` files), and so does `/srv/pex/gameplay-bench/SERVERS.md`.

**Keys nothing reads, or whose value changes nothing.** These come from the inventory below. The searches cover `git ls-files crates tools native-plugins` minus `crates/pumpkin-config` and `crates/pumpkin-data`, also with whitespace removed so that multi-line field chains are found.
- `world.chunk.write_in_place`, covered above.
- `networking.java.authentication.enabled`. `git grep -n -E "(auth|auth_config|authentication)\.enabled"` finds only Bedrock reads (`core/lib.rs` `bind_nethernet`, `core/net/bedrock/nethernet/discovery.rs` `advertise`, `core/net/bedrock/login/login.rs` `try_handle_login`). Java authentication follows `online_mode` alone.
- `networking.java.authentication.textures.enabled` and `textures.types.{skin,cape,elytra}`. `validate_textures` (`core/net/authentication.rs`) checks every texture URL against `allowed_url_schemes` and `allowed_url_domains` whatever these say. `git grep -n -E "textures\.enabled|\.types\b|\.(skin|cape|elytra)\b"` finds no read of the config.
- `networking.bedrock.compression.enabled`. `BedrockClient::handle_request_network_settings` reads only `compression.info`. The only `compression.enabled` reads are Java's: `PendingConnection::handle_login_start`, and `tools/pumpkin-tester/src/harness.rs`.
- `pvp.swing` is read in `Player::attack` as `if config.swing {}`, which has an empty body.
- `default_difficulty` is read only by `ManagementSettings::new` (`core/net/management/hub.rs`), which reports it. New worlds get `DEFAULT_DIFFICULTY` (`pw/world_info/mod.rs`), and nothing applies the key at start. Vanilla does: `MinecraftServer.loadLevel` calls `forceDifficulty`, and `DedicatedServer` overrides that to set the `server.properties` `difficulty`.

**Changes made at runtime are not written back.** No code outside `pumpkin-config` writes TOML (`git grep "toml::\|toml_edit" -- crates ':!crates/pumpkin-config'` finds nothing). `/whitelist on` only flips `Server.white_list` (`core/command/commands/whitelist.rs` `OnExecutor`). After a restart the whitelist follows the file again, which is `white_list = false` by default. The management API setters only store into `ManagementSettings` (`core/net/management/handlers.rs`). `serversettings/difficulty/set` does not even change the world. Vanilla writes 19 settings back to `server.properties` through `DedicatedServerSettings.update` (19 `settings.update` calls in `DedicatedServer.java`), including `setUsingWhitelist` and the management `setDifficulty`. One related bug is in Goal 2's area: the Wasm `has_whitelist` returns `basic_config.white_list`, the value at startup, in both `pumpkin-wasm-host-v0_1/src/server.rs` and `-v0_2/src/server.rs`.

**Defaults that point at upstream Pumpkin.** `TelemetryConfig::default` has `enabled: true`. It sends heartbeats to `https://market.pumpkinmc.org/api/v1/rest/telemetry/heartbeat` with `server_type: "pumpkin"` and the OS, CPU, RAM and plugin list (`core/telemetry.rs` `build_heartbeat_payload`). The earlier roadmap recorded "off by default" as an answer on 2026-10-29, but 300621c3e doesn't do that. `server_links.bug_report` defaults to `https://github.com/Pumpkin-MC/Pumpkin/issues`. `ChunkConfig::default` is `Pump`, which is Goal 7's question. Every generated file writes `[world.chunk] type` explicitly, so a new default would only reach new files.

**Other findings.**
- With `pvp.enabled = false`, `core/net/java/play/attack.rs` `handle_attack` and `interact.rs` `handle_interact` return early for every melee target, mobs included. Vanilla 26.3 has no such property: PvP is the `pvp` game rule (`GameRules.PVP`), which PEX reads only in `core/entity/passive/wolf.rs`. This is gameplay rather than config cleanup, so it is left to Goal 6.
- A read-only `pumpkin.toml` that needs a migration (the commented `:ro` mount in `docker-compose.yml`) gains another backup file on every start. `save_migrated` writes the backup first, the `fs::write` then fails with a warning, and the next start repeats both.

**Config consumers outside the server.**
- `egg-pex.json` and `egg-pumpkin.json` (`config.files`): the `file` parser replaces `address = "0.0.0.0:25565"` with the allocated port.
  - A fresh file has that line twice, under `[networking.query]` and `[networking.java]`, so both get the port.
  - The match needs the default text, so it works until the first rewrite. Whether Wings follows a later change of allocation was not checked.
  - The install script runs `timeout 2 ./pumpkin` to create `pumpkin.toml`. In an empty directory, `load` hands the write to `PendingSave`, and `bin/` `main` writes the file after the UUID migration and before the level loads.
  - `egg-pex.json`'s description tells admins to "set write_in_place = false" before moving a world back to Pumpkin. That key has done nothing since `b7174db92`.
- `tools/pumpkin-tester/src/harness.rs` sets config struct fields directly (`networking.java.encryption`, `compression.enabled`, `bedrock.authentication.enabled` and others). Removing a field breaks it at compile time.
- `tools/dual-run/dual_run.py` `run_pex` writes a partial `pumpkin.toml` without `config_version`, so it goes through the version 1 path (UUID migration, backup, all keys added).
- Native plugins load `<data folder>/config.toml` with `pex_common::load_config` (`native-plugins/common/src/lib.rs`). Users are `template` and `worldpumpkin` (`worldpumpkin/src/config.rs` `Config::load`).
  - There is no version key and no migration, missing keys are never written to the file, and unknown keys are ignored.
  - A file that fails to parse logs an error and runs on all defaults. That is better than the server, which says nothing. `pex-common` has no tests.
  - `pexguard` uses `pumpkin_config::migration::write_backup` and `OFFLINE_UUID_VERSION` for its `regions.toml` (`pexguard/src/uuid_migration.rs`), and it reads `Server.basic_config` directly (`pexguard/src/lib.rs`).
  - The native loader refuses plugins built from other sources (`core/plugin/loader/native.rs` `check_build_id`). A change to the config structs therefore means rebuilding the bundled plugins, which happens anyway.
- Only the Wasm host reads these `[plugins]` keys: `inherit_env`, `loopback_only`, `max_memory_mb` and `overrides.*.{loopback_only,max_memory_mb,environment}` in `wh/runtime.rs` `PluginRuntime::on_load`, and `verify_signatures`, which `bin/` `main` passes to `WasmPluginLoader::new`. They become dead keys if Goal 2 drops Wasm.

**Key inventory.** Each group row names the struct. A key's field is the field of that struct named after the key's last part (`networking.java.online_mode` is `JavaConfig.online_mode`), unless the row says otherwise. Defaults are the `Default` impls; "not written" means the default is `None`, so the key is missing from generated files. "same" means the same place as the row above.

| Key | Default | Read in |
| :-- | :-- | :-- |
| **Root: `BasicConfiguration` (`cfg/lib.rs`), flattened** | | |
| `config_version` (no field, `migration::CONFIG_VERSION_KEY`) | 2 in new files, missing counts as 1 | `cfg/migration.rs` `file_version` |
| `seed` | random (`random::get_seed`) | `core/server/mod.rs` `Server::new`, only when `level.dat` is missing |
| `default_difficulty` | `"Normal"` | only `core/net/management/hub.rs` `ManagementSettings::new`, which reports it; never applied |
| `op_permission_level` | `4` | `core/command/commands/op.rs` `OpCommandExecutor::execute`, `core/command/context/command_source.rs` `send_to_ops` |
| `allow_nether` | `true` | `Server::new` (dimension list), `core/entity/mod.rs` `Entity::try_use_portal`, Wasm hosts' `server.rs` |
| `allow_end` | `true` | same |
| `hardcore` | `false` | `Server::set_difficulty`, `core/world/mod.rs` `World::spawn_java_player` and `spawn_bedrock_player`, `core/net/java/play/client_command.rs` `handle_client_status` |
| `tps` | `20.0` | `Server::new` (`ServerTickRateManager::new`), `core/command/commands/tps.rs`, `core/entity/player.rs` `Player::attack`, `core/item/items/spear.rs` |
| `default_gamemode` | `"Survival"` | `Server::new` (`DefaultGamemode`), Wasm hosts |
| `force_gamemode` | `false` | `core/command/commands/defaultgamemode.rs` `DefaultGamemodeExecutor::execute`, `ManagementSettings::new` |
| `scrub_ips` | `true` | `core/lib.rs` `PumpkinServer::unified_listener_task` |
| `use_favicon` | `true` | `core/server/connection_cache.rs` `CachedStatus::build_response` |
| `favicon_path` | not written | same |
| `default_level_name` | `"world"` | `BasicConfiguration::get_world_path` (used by `Server::new` and `bin/` `main`), `core/net/bedrock/status.rs` `IceSocket::handle_packet`, `nethernet/discovery.rs` `advertise` |
| `allow_chat_reports` | `false` | `PumpkinConfig::validate`, `Server::new` (Mojang keys), `core/net/java/play/chat_message.rs` `handle_chat_message` |
| `white_list` | `false` | `Server::new` (initial `Server.white_list`), Wasm hosts' `has_whitelist` |
| `enforce_whitelist` | `false` | `core/command/commands/whitelist.rs` `kick_non_whitelisted_players`, `ManagementSettings::new` |
| `accepts_transfers` | `true` | `core/net/java/handshake.rs` `handle_handshake` |
| `spawn_protection` | `16` | `core/world/mod.rs` `World::is_in_spawn_protection` |
| **`[logging]`: `LoggingConfig` (`cfg/logging.rs`)** | | |
| `enabled` | `true` | `core/lib.rs` `init_logger` |
| `level` | `"info"` | same |
| `threads` | `false` | same |
| `thread_ids` | `false` | same |
| `target` | `false` | same |
| `color` | `true` | same |
| `timestamp` | `true` | same |
| `timestamp_format` | `"[hour]:[minute]:[second]"` | same |
| `file` | `"latest.log"` | same |
| **`[resource_pack.java]`: `JavaResourcePackConfig` (`cfg/resource_pack.rs`)** | | |
| `enabled` | `false` | `core/net/java/login/login_acknowledged.rs` `handle_login_acknowledged`, `core/net/java/pending.rs` `handle_resource_pack_response` |
| `url` | `""` | `handle_login_acknowledged` |
| `sha1` | `""` | same |
| `prompt_message` | `""` | same |
| `force` | `false` | same, `core/net/java/play/resource_pack_response.rs` `handle_play_resource_pack_response` |
| **`[resource_pack.bedrock]`: `BedrockResourcePackConfig`** | | |
| `enabled` | `false` | `core/net/bedrock/login/login.rs` `try_handle_login`, `core/net/bedrock/login/resource_pack_response.rs` `handle_resource_pack_response` |
| `force` | `false` | same |
| `packs` | `[]` | same |
| `packs[]` entries (`BedrockPack`): `uuid`, `version`, `size`, `download_url` required; `path`, `content_key`, `sub_pack_name`, `content_id`, `has_scripts`, `addon_pack`, `rtx_enabled` optional | empty or `false` | same, `resolve_pack_path` (`path`) |
| **`[world]`: `LevelConfig` (`cfg/world.rs`)** | | |
| `chunk.type` (tag of `ChunkConfig`, `cfg/chunk.rs`) | `"pump"` | `pw/level.rs` `Level::from_root_folder` (picks the saver and converts a world stored in another format), `bin/` `main` (`--convert-world`) |
| `chunk.compression.algorithm` (`AnvilChunkConfig`, only with `type = "anvil"`) | `"LZ4"` | `pw/chunk/format/anvil.rs` `AnvilChunkData::compress` |
| `chunk.compression.level` | `6` | same |
| `chunk.write_in_place` (no field since `b7174db92`) | `true` in 1.0 and 1.1 files | nothing: `git grep write_in_place` |
| `lighting` (`LightingEngineConfig`) | `"default"` | `Level::from_root_folder`, `core/world/mod.rs` `World::get_lighting_config` |
| `autosave_ticks` | `6000` | `Level::from_root_folder`, then `World::tick_environment` |
| `entity_islands.unaudited_reach` | `16.0` | `core/world/entity_islands.rs` `EntityReach::of` (config passed in from `World::tick`) |
| `entity_islands.knockback_reach` | `4.0` | `EntityReach::resolve` |
| **`[networking.query]`: `QueryConfig`** | | |
| `enabled` | `false` | `core/lib.rs` `PumpkinServer::new` |
| `address` | `"0.0.0.0:25565"` | same |
| **`[networking.management]`: `ManagementServerConfig`** | | |
| `enabled` | `false` | `PumpkinServer::new` |
| `address` | `"127.0.0.1:25585"` | `core/net/management/mod.rs` `ManagementServer::run` |
| `secret` | `""` | same |
| `allowed_origins` | `[]` | same |
| `status_heartbeat_interval` | `0` | `ManagementSettings::new`, `ManagementServer::run_heartbeat_loop` |
| **`[networking.proxy]`: `ProxyConfig` (`cfg/networking/proxy.rs`)** | | |
| `enabled` | `false` | `core/net/java/login/login_start.rs` `handle_login_start` |
| `velocity.enabled` | `false` | same, `core/net/java/login/plugin_response.rs` `handle_plugin_response` |
| `velocity.secret` | `""` | `core/net/proxy/velocity.rs` `receive_velocity_plugin_response` |
| `bungeecord.enabled` | `false` | `handle_login_start` |
| `bungeecord.secret` | `""` | same |
| `vine.enabled` | `false` | same, `handle_plugin_response` |
| `vine.public_key` | `""` | `core/net/proxy/vine.rs` `get_verifying_key` |
| `vine.secret` | `""` | same |
| **`[networking.lan_broadcast]`: `LANBroadcastConfig`** | | |
| `enabled` | `false` | `PumpkinServer::new` |
| `motd` | not written | `core/net/lan_broadcast.rs` `LANBroadcast::new` |
| `port` | not written | same |
| **`[networking.java]`: `JavaConfig` (`cfg/networking/java.rs`)** | | |
| `enabled` | `true` | `PumpkinServer::new`, `bin/` `main` (log line) |
| `address` | `"0.0.0.0:25565"` | `PumpkinServer::new` |
| `encryption` | `true` | `handle_login_start`, `PumpkinConfig::validate` |
| `online_mode` | `true` | `handle_login_start`, `core/net/java/login/encryption_response.rs` `handle_encryption_response`, `core/command/commands/fetchprofile.rs`, `validate` |
| `validate_player_names` | `true` | `handle_login_start`, `fetchprofile.rs` `fetch_profile_by_name_helper` |
| `max_players` | `1000` | `handle_login_start`, `Server::new` (`CachedStatus::new`), `core/command/commands/list.rs`, `ManagementSettings::new` |
| `view_distance` | `16` | `core/world/chunker.rs` `get_view_distance`, `validate`, Wasm hosts |
| `simulation_distance` | `10` | `chunker.rs` `update_position`, `World::update_active_chunks` |
| `keep_alive_time` (aliases `keep-alive-time`, `keep_alive_interval`, `keep-alive-interval`) | `15` | `core/net/java/mod.rs` `JavaClient::progress_player_packets`, `validate` |
| `motd` | `"PEX, an experimental fork of Pumpkin"` | `Server::new` (`CachedStatus::new`), `PumpkinServer::new` (LAN), `core/net/query.rs` `handle_packet` |
| `compression.enabled` (`CompressionConfig`) | `true` | `handle_login_start` |
| `compression.threshold` (`CompressionInfo`, flattened) | `256` | `encryption_response.rs` `enable_compression`, `core/net/java/pending.rs` `set_compression` |
| `compression.level` | `4` | same |
| **`[networking.java.authentication]`: `AuthenticationConfig` (`cfg/networking/auth.rs`)** | | |
| `enabled` | `true` | nothing (see above) |
| `url` | not written | `core/net/authentication.rs` `authenticate` |
| `fallbacks` (alias `fallback_urls`) | `[]` | same |
| `profile_by_name_url` | not written | `lookup_profile_by_name` |
| `profile_by_name_fallbacks` | `[]` | same |
| `profile_by_uuid_url` | not written | `fetch_profile_by_uuid` |
| `profile_by_uuid_fallbacks` | `[]` | same |
| `connect_timeout` | `5000` | `create_client` |
| `read_timeout` | `5000` | same |
| `prevent_proxy_connections` | `false` | `authenticate` |
| `prevent_proxy_connection_auth_url` | not written | same |
| `services_url` | not written | `fetch_mojang_public_keys` |
| `player_profile.allow_banned_players` | `false` | `encryption_response.rs` `PendingConnection::authenticate` |
| `player_profile.allowed_actions` | `["FORCED_NAME_CHANGE", "USING_BANNED_SKIN"]` | same |
| `textures.enabled` | `true` | nothing (see above) |
| `textures.allowed_url_schemes` | `["http", "https"]` | `is_texture_url_valid`, called from `PendingConnection::authenticate` |
| `textures.allowed_url_domains` | `[".minecraft.net", ".mojang.com"]` | same |
| `textures.types.skin` | `true` | nothing (see above) |
| `textures.types.cape` | `true` | nothing |
| `textures.types.elytra` | `true` | nothing |
| **`[networking.java.packet_limiter]`: `PacketLimiterConfig`** | | |
| `enabled` | `true` | `PumpkinServer::unified_listener_task` (`PacketRateLimiter::from_config`, `core/net/packet_limiter.rs`) |
| `max_packet_rate` (alias `max-packet-rate`) | `500.0` | same |
| `burst_capacity` (alias `burst-capacity`) | `500.0` | same |
| `kick_message` (alias `kick-message`) | `"Kicked for spamming packets"` | `JavaClient::progress_player_packets`, `pending.rs` |
| **`[networking.bedrock]`: `BedrockConfig` (`cfg/networking/bedrock.rs`)** | | |
| `enabled` | `true` | `PumpkinServer::bind_nethernet`, `bind_bedrock_status`, `bin/` `main` |
| `online_mode` | `true` | `core/net/bedrock/login/login.rs` `try_handle_login`, `bind_nethernet`, `Server::new` (OIDC keys) |
| `max_players` | `1000` | `core/net/bedrock/status.rs` `IceSocket::handle_packet`, `nethernet/discovery.rs` `advertise`, `list.rs` |
| `view_distance` | `16` | `get_view_distance`, `core/net/bedrock/play/request_chunk_radius.rs` `handle_request_chunk_radius`, `validate` |
| `simulation_distance` | `10` | `World::spawn_bedrock_player` |
| `compression.enabled` | `true` | nothing (see above) |
| `compression.threshold` | `256` | `core/net/bedrock/login/request_network_settings.rs` `handle_request_network_settings`, `core/net/bedrock/mod.rs` `BedrockClient::set_compression` |
| `compression.level` | `4` | `BedrockClient::set_compression` |
| `motd` | `"PEX, an experimental fork of Pumpkin"` | `IceSocket::handle_packet`, `advertise`, `nethernet/endpoint.rs` `status` |
| `username_prefix` | `""` | `try_handle_login` |
| `replace_username_spaces` | `true` | same |
| `chunk_caching` | `true` | `core/net/bedrock/mod.rs` `send_chunks` |
| `authentication.enabled` (`BedrockAuthenticationConfig`) | `true` | `try_handle_login`, `bind_nethernet`, `Server::new` |
| `authentication.url` | not written | `Server::new` (`fetch_oidc_jwks`) |
| `authentication.connect_timeout` | `5000` | same |
| `authentication.read_timeout` | `5000` | same |
| `nethernet.enabled` (`NetherNetConfig`) | `true` | `bind_nethernet`, `bind_bedrock_status` |
| `nethernet.address` | `"0.0.0.0:19132"` | `bind_bedrock_status` |
| `nethernet.external_ip` | `""` (detect) | `nethernet/endpoint.rs` `NetherNetListener::bind` |
| `nethernet.identity_key` | `"nethernet-key.der"` | `bind_nethernet` |
| `nethernet.stun_servers` | `[]` | `NetherNetListener::bind` |
| `packet_limiter.enabled`, `.max_packet_rate`, `.burst_capacity` | `true`, `500.0`, `500.0` | `PumpkinServer::unified_listener_task` |
| `packet_limiter.kick_message` | `"Kicked for spamming packets"` | `BedrockClient::process_batch` |
| **`[commands]`: `CommandsConfig` (`cfg/commands.rs`)** | | |
| `use_console` | `true` | `PumpkinServer::start` |
| `use_tty` | `true` | `init_logger`, `PumpkinServer::start` |
| `log_console` | `true` | `handle_chat_command` in `core/net/java/play/chat_command.rs` and `core/net/bedrock/play/chat_command.rs` |
| `broadcast_console_to_ops` | `true` | `Server::new` (`set_broadcast_console_to_ops`) |
| `default_op_level` | `0` | `Player::new` |
| `overrides.<command>.enabled` (`CommandOverride`, map) | `true` | `core/command/commands/mod.rs` `apply_command_overrides` |
| `overrides.<command>.permission_level` | not written | same |
| **`[chat]`: `ChatConfig` (`cfg/chat.rs`)** | | |
| `format` | `"<{DISPLAYNAME}> {MESSAGE}"` | `handle_chat_message` (Java and Bedrock) |
| `anti_spam.enabled` | `true` | `Player::check_chat_spam`, `Player::tick` |
| `anti_spam.spam_threshold` | `200` | `AntiSpamConfig::chat_threshold_ticks` and `command_threshold_ticks`, called by `check_chat_spam` |
| `anti_spam.chat_spam_threshold_seconds` (alias with dashes) | not written | `chat_threshold_ticks` |
| `anti_spam.command_spam_threshold_seconds` (alias with dashes) | not written | `command_threshold_ticks` |
| `anti_spam.message_cost` | `20` | `check_chat_spam` |
| `anti_spam.decay_per_tick` | `1` | `Player::tick` |
| `anti_spam.ops_bypass` | `true` | `check_chat_spam` |
| **`[pvp]`: `PVPConfig` (`cfg/pvp.rs`)** | | |
| `enabled` | `true` | `core/net/java/play/attack.rs` `handle_attack`, `interact.rs` `handle_interact`, `spear.rs` `can_hit` |
| `hurt_animation` | `true` | `core/entity/living.rs` `LivingEntity::damage_with_context` |
| `protect_creative` | `true` | `handle_attack`, `handle_interact`, `can_hit` |
| `knockback` | `true` | `Player::attack`, `spear.rs` `stab_attack` |
| `swing` | `true` | `Player::attack`, as `if config.swing {}`: no effect |
| **`[server_links]`: `ServerLinksConfig` (`cfg/server_links.rs`)** | | |
| `enabled` | `true` | `handle_login_acknowledged` |
| `bug_report` | `"https://github.com/Pumpkin-MC/Pumpkin/issues"` | same |
| `support` | `""` | same |
| `status` | `""` | same |
| `feedback` | `""` | same |
| `community` | `""` | same |
| `website` | `""` | same |
| `forums` | `""` | same |
| `news` | `""` | same |
| `announcements` | `""` | same |
| `custom` (map) | `{}` | same |
| **`[player_data]`: `PlayerDataConfig`** | | |
| `save_player_data` | `true` | `Server::new` (`ServerPlayerData::new`) |
| `save_player_cron_interval` | `300` | same |
| **`[fun]`: `FunConfig`** | | |
| `april_fools` | `true` | `core/server/seasonal_events.rs` `modify_chat_message` |
| **`[recipe]`: `RecipeConfig`** | | |
| `send_recipes` | `true` | `World::spawn_java_player` |
| **`[plugins]`: `PluginsConfig` (`cfg/plugins.rs`)** | | |
| `enabled` | `true` | `PumpkinServer::init_plugins`, `core/plugin/mod.rs` `start_loading_plugin` |
| `hot_reload` | `false` | `init_plugins` |
| `ask_permission_confirmation` | `true` | `PluginManager::check_permissions_cached` |
| `allow_unsigned` | `true` | `PluginManager::load_plugins` |
| `allowed_permissions` | `[]` | `check_permissions_cached` |
| `blocked_permissions` | `[]` | same, `wh/runtime.rs` `PluginRuntime::on_load` |
| `inherit_env` | `false` | `on_load` |
| `loopback_only` | `false` | same |
| `max_memory_mb` | not written | same |
| `overrides.<plugin>.enabled` (`PluginOverride`, map) | `true` | `load_plugins`, `start_loading_plugin` |
| `overrides.<plugin>.allow_unsigned` | not written | `load_plugins` |
| `overrides.<plugin>.allowed_permissions`, `.blocked_permissions` | `[]` | `check_permissions_cached`, `on_load` (blocked) |
| `overrides.<plugin>.loopback_only`, `.max_memory_mb` | not written | `on_load` |
| `overrides.<plugin>.environment` (map) | `{}` | `on_load` |
| `verify_signatures` | `true` | `bin/` `main` (`WasmPluginLoader::new`) |
| **`[advancement]`: `AdvancementConfig`** | | |
| `save_advancements` | `true` | `Server::new` (`AdvancementManager::new`) |
| **`[metrics]`: `MetricsConfig`** | | |
| `enabled` | `true` | `core/server/metrics.rs` `Metrics::new` |
| `slow_tick_ms` | `100` | same |
| **`[telemetry]`: `TelemetryConfig` (`cfg/telemetry.rs`, not flattened)** | | |
| `enabled` | `true` | `core/telemetry.rs` `start_telemetry_with_config` |
| `endpoint` | `"https://market.pumpkinmc.org/api/v1/rest/telemetry/heartbeat"` | same |
| `interval_secs` | `300` (at least 60 is used) | same |
| `public` | `false` | `build_heartbeat_payload` |
| `server_name` | not written | same |

### Pieces

#### G3-01 Bad config values fall back per key

- Scope:
  - In: a value that fails to deserialize, or is out of range, takes that key's default. The server logs one warning naming the key path, the bad value and the default, and every other key keeps its value. Vanilla `Settings.get` falls back per key too, without a message.
  - In: a TOML syntax error stops the server with exit code 1, reporting the line and column, instead of starting on defaults.
  - In: `PumpkinConfig::validate` no longer panics. Single-key ranges (view distances 2..=64, `keep_alive_time` above 0) fall back per key. `allow_chat_reports` without `online_mode` counts as off, as in vanilla `DedicatedServer.enforceSecureProfile`. `online_mode` without `encryption` is handled as D3-1 decides. `bin/` `main` acts on the load result before anything else runs.
  - A possible approach: retry `toml::from_str` after replacing the value at the error's span with the default from the defaults document. Whether `toml` 1.1 errors carry spans was not checked.
  - Out: unknown keys (G3-02), plugin configs (G3-09), the JSON lists in `core/data/`.
- Depends on: none
- Branch: fix/config-invalid-values
- Needs decision: D3-1
- Done when:
  - New tests in `crates/pumpkin-config/tests/invalid_values.rs`, each failing on alpha:
    - `one_bad_value_keeps_every_other_value`: a file with `default_level_name = "survival"`, `white_list = true` and `tps = "fast"` loads with `survival`, the whitelist on and `tps` 20.0. It gives exactly one `LoadMessage::Warn` naming `tps`, the value and the default, and the file stays byte for byte the same.
    - `out_of_range_view_distance_uses_the_default`: 0 and 65 both give 16 with a warning and no panic.
    - `syntax_error_stops_with_line_and_column`.
  - Boot test (debug build, scratch directory): `tps = "fast"` reaches "Server is now running." with one warning. A missing `]` exits 1 with the position and creates no world folder.

#### G3-02 Warn about unknown keys

- Scope:
  - In: after migration, one warning per key the structs don't read, by full path (`pumpkin.toml: unknown key networking.java.onlinemode`), logged once per start.
  - In: these count as known: the serialized defaults, `config_version`, the 13 keys that default to `None`, `world.chunk.compression` for any chunk type, entries of the map tables (`commands.overrides.*`, `plugins.overrides.*` including `environment.*`, `server_links.custom.*`), the keys of `resource_pack.bedrock.packs[]` entries, and the serde aliases.
  - Prefer getting the list from serde over a hand-written list that drifts. `serde_ignored`, for example, is a new dependency; check first how it handles the root `#[serde(flatten)]`.
  - Out: removing keys (G3-03), refusing to start on unknown keys.
- Depends on: none
- Branch: feat/config-unknown-keys
- Needs decision: none
- Done when:
  - `unknown_key_is_reported_by_full_path`: a typo under `[networking.java]` gives one warning with the full path.
  - `optional_and_aliased_keys_are_known`: a file with `favicon_path`, `keep-alive-time`, `[commands.overrides.tp]`, `[plugins.overrides.x.environment]`, `telemetry.server_name` and an Anvil `[world.chunk.compression]` gives no warning.
  - The generated file and the migrated 1.2.0 fixture give no warning. Until G3-03 lands, the 1.0.0 and 1.1.0 fixtures give exactly one, for `world.chunk.write_in_place`.

#### G3-03 First config migration removes dead keys

- Scope:
  - In: `Migration { version: 3, step: Step::Config(..) }` in `PumpkinConfig::MIGRATIONS`. It deletes `world.chunk.write_in_place`, including inside an inline `chunk = { .. }`, plus the keys D3-2 drops, and logs one line per removed key.
  - In: the step must be safe to run again on a file set back to `config_version = 1`.
  - In: the matching struct fields go, and so do their uses in `tools/pumpkin-tester/src/harness.rs`.
  - In: `release_files_migrate_to_the_current_version` changes from "every original line survives" to "every original line survives except the removed keys".
  - Out: `[plugins]` keys that only the Wasm host reads (Goal 2 removes them with this step type), what `pvp.enabled` means (Goal 6), the default chunk format (Goal 7).
- Depends on: G3-02
- Branch: fix/config-dead-keys
- Needs decision: D3-2
- Done when:
  - `release_files_migrate_to_the_current_version` passes with `config_version` 3. The migrated 1.0.0 and 1.1.0 files have no `write_in_place`, and the log has their removal lines.
  - New `dead_key_step_is_safe_to_run_twice`: a migrated file set back to `config_version = 1` loads back to the same bytes.
  - New `inline_chunk_table_loses_write_in_place`.
  - G3-02 reports no unknown key on any fixture.
- Needed by: G3-10, and Goal 2 for removing the Wasm-only `[plugins]` keys.

#### G3-04 Save pumpkin.toml safely

- Scope:
  - In: `save_migrated` and `PendingSave::save` write a temporary file in the same directory and rename it over `pumpkin.toml`.
  - In: if the write fails, the new backup is removed again. A read-only file (the `:ro` mount in `docker-compose.yml`) then gets one warning per start and no new backup.
  - `pumpkin-config` can't call `pw/chunk/io/mod.rs` `write_file_atomically`, because `pumpkin-world` depends on `pumpkin-config`, so this needs a small local helper.
  - Out: writing back changes made at runtime (G3-06).
- Depends on: none
- Branch: fix/config-safe-save
- Needs decision: none
- Done when:
  - New `failed_write_leaves_the_file_and_no_new_backup`: block the write with a directory at the temporary file's path, because root ignores file modes. Two loads leave the original bytes and no `.bak` file.
  - The 5 existing `config_version.rs` tests pass.

#### G3-05 Pelican egg matches the config

- Scope:
  - In: the `egg-pex.json` description drops the `write_in_place` advice. It says instead what is true now about moving a PEX world back to Pumpkin, checked against current upstream first.
  - In: the port rewrite works as D3-6 decides and touches only `networking.java.address`.
  - In: `egg-pumpkin.json` changes only if D3-6 changes the `config.files` block both eggs share.
  - Out: Pelican panel plugins, the free tier.
- Depends on: none
- Branch: fix/egg-config
- Needs decision: D3-6
- Done when:
  - Both eggs parse with `python3 -m json.tool`, and `git grep write_in_place -- egg-pex.json egg-pumpkin.json` finds nothing.
  - On a throwaway Pelican server (not a player's), a fresh install binds the allocated port, and `networking.query.address` keeps its own value.
  - If the D3-6 option supports it, the server binds the new port after the allocation changes and it restarts.

#### G3-06 Write runtime setting changes back to pumpkin.toml

- Scope:
  - In: a `pumpkin-config` function that sets one key in the file with `toml_edit`, keeping comments and order, and saves through G3-04's safe save.
  - In: it is called where vanilla calls `DedicatedServerSettings.update`: `/whitelist on` and `off` (vanilla `DedicatedServer.setUsingWhitelist`), plus the management API setters D3-4 includes that have a PEX key. Writes happen off the tick thread.
  - Out: making management setters change live state they don't change today, such as the world difficulty for `serversettings/difficulty/set`.
- Depends on: G3-04
- Branch: feat/config-write-back
- Needs decision: D3-4
- Done when:
  - Unit test `set_key_changes_only_that_line`: in a file with comments, only the set line differs afterwards.
  - Boot test: console `whitelist on`, `stop`, start again. A second `whitelist on` then fails with "Whitelist is already turned on" (`ERROR_ALREADY_ON`), and `pumpkin.toml` differs from before only in `white_list = true`.

#### G3-07 default_difficulty does something

- Scope:
  - In: D3-3's choice. Option (a): once the worlds have loaded, call `Server::set_difficulty(basic_config.default_difficulty, true)`, as vanilla `MinecraftServer.loadLevel` does through `DedicatedServer.forceDifficulty`. Hardcore still forces Hard, which `Server::set_difficulty` already does. Option (b): remove the key with a step like G3-03's, and have `ManagementSettings::new` report the difficulty from `level.dat`.
  - Out: keeping `/difficulty` across restarts. Vanilla's `/difficulty` doesn't write `server.properties` either.
- Depends on: none (G3-03 for option b)
- Branch: fix/default-difficulty
- Needs decision: D3-3
- Done when, for option (a): boot test with a world whose `level.dat` says Hard and `default_difficulty = "Easy"`. After start, console `difficulty` reports Easy, as vanilla 26.3 does with `difficulty=easy` in `server.properties` (run both). With `hardcore = true` it stays Hard.

#### G3-08 Defaults that point at upstream

- Scope:
  - In: what D3-5 decides. `TelemetryConfig::default` gets `enabled: false` for new files, as the earlier answer said. Optionally a `Step::Config` migration covers existing files. The `server_links.bug_report` default changes as decided.
  - Out: the telemetry protocol and endpoint code.
- Depends on: none (G3-03 if D3-5 adds a migration, which then takes the next version)
- Branch: fix/telemetry-default-off
- Needs decision: D3-5
- Done when:
  - Boot test in an empty directory: the log has no "Anonymous server telemetry is enabled" line (written by `start_telemetry_with_config`), and the generated file has `[telemetry] enabled = false`.
  - `current_file_is_left_byte_for_byte` still passes.
  - With a migration, the release fixtures test expects its log line.

#### G3-09 Versioned plugin configs in pex_common

- Scope:
  - In: `pex_common::load_config` behaves like `pumpkin.toml` loading: `config_version`, ordered migrations per plugin, and missing keys added after a `.v<old>.bak` backup. It reuses `pumpkin_config::migration`, which `pexguard` already uses, plus G3-01's per-key fallback and G3-02's unknown-key warnings.
  - In: `worldpumpkin` and `template` keep calling it. They need no migration step yet: `git log v1.2.0..300621c3e -- native-plugins/worldpumpkin/src/config.rs native-plugins/template/src/lib.rs native-plugins/common/src/lib.rs` shows only `4ccd66fca` (build id), which changed no config keys.
  - Out: plugin data files such as `pexguard`'s `regions.toml`, Wasm plugin configs.
- Depends on: G3-01, G3-02 (D2-1 keeps the native ABI unstable, so plugin config stays in `pex_common`)
- Branch: feat/plugin-config-version
- Needs decision: D3-1
- Done when:
  - First tests in `pex-common` pass under `cargo nextest run -p pex-common`: a missing file is written with `config_version`, an old file gains a new key and gets a backup, one bad value keeps the rest, and an unknown key gives a warning.
  - `worldpumpkin` loads a config written by 1.2.0 unchanged apart from the version line.

#### G3-10 Real 1.x configs migrate and boot

- Scope:
  - In: copies of `pumpkin.toml` from the live PEX servers. The earlier roadmap names Pelican servers 30 and 34. Neon takes the copies read-only, or someone else does with his go-ahead. Secrets, URLs and seeds are replaced with placeholders of the same type, and the copies sit next to the release fixtures.
  - In: a test that each copy migrates with only the expected log lines, and that its effective config matches what alpha 300621c3e loads from the same file, apart from removed keys.
  - In: a debug-build boot on each copy.
  - Out: any change on the live servers.
- Depends on: G3-01, G3-02, G3-03
- Branch: test/config-real-files
- Needs decision: none
- Done when:
  - `real_files_keep_their_settings` passes. It compares `toml::to_string(&PumpkinConfig)` against expected files generated once with alpha 300621c3e, without the removed keys.
  - Each copy boots to "Server is now running." with no unknown-key warning.
  - A second start leaves `pumpkin.toml` byte for byte the same and logs no config line.

### Proof

- `cargo nextest run -p pumpkin-config -p pex-common` passes with the tests named in G3-01 to G3-04, G3-09 and G3-10, and each new test fails on alpha 300621c3e.
- Every release fixture (1.0.0, 1.1.0, 1.2.0, which also covers 1.2.1) and every real copy from G3-10 reaches the latest `config_version` (3, or 4 if G3-08 adds a step). Each has its backup, one log line per change, 0 unknown-key warnings and no `write_in_place`. On a second start: 0 config log lines and a byte-identical file.
- Boot tests (debug build, scratch directories):
  - One bad value gives 1 warning, and every other value takes effect.
  - A syntax error exits 1 with the line and column and creates no world.
  - `whitelist on` survives a restart, if D3-4 includes it.
  - A fresh directory starts with telemetry off.
- `git grep write_in_place` finds only the 1.0.0 and 1.1.0 fixtures and the G3-03 migration step.
- A fresh install with `egg-pex.json` on a throwaway Pelican server binds the allocated port.
- The `rust.yml` run for the `alpha` push that lands the last piece passes (G0-01).

### Decisions

- D3-1: What happens when a config value is bad? Options:
  - Per-key default plus a warning naming the key, the value and the default. A TOML syntax error stops the server, reporting the line and column. The earlier roadmap recorded this answer on 2026-10-29. It is closest to vanilla (`Settings.get`), and a typo can no longer start a server on all defaults (a new world under `world`, whitelist off).
  - Refuse to start on any bad value. Safest, but one typo keeps a server down until someone edits the file, and vanilla never does this.
  - Keep alpha's behaviour: all defaults, no message. No work, and the silent reset stays.
  - Sub-question on rules that involve two keys. `allow_chat_reports` without `online_mode` would count as off, as in vanilla `enforceSecureProfile`. `online_mode` without `encryption` has no vanilla equivalent: refuse to start, or turn encryption on with a warning?
  - Waits on it: G3-01, G3-09.
  - Answered 2026-10-31 (Neon accepted Q3.1): the first option. A bad value gets its key's default and a warning naming the key, the value and the default; a TOML syntax error stops the server with the line and column. Still open: the sub-question on rules that involve two keys.
- D3-2: Remove or wire the keys that nothing reads? `world.chunk.write_in_place` goes either way. The others are `networking.java.authentication.enabled`, `networking.java.authentication.textures.enabled`, `textures.types.skin`, `.cape` and `.elytra`, `networking.bedrock.compression.enabled`, and `pvp.swing`. Options:
  - Remove them all with G3-03's step, one log line each. Vanilla has none of them, and `authentication.enabled` duplicates `online_mode`. Admins lose nothing, since none of them has an effect today.
  - Wire some of them:
    - The texture keys go in `validate_textures` (a small change in `core/net/authentication.rs`).
    - Bedrock `compression.enabled` touches Bedrock network code, which is Goal 8's area, and Bedrock clients expect network settings with a threshold.
    - `pvp.swing` has no vanilla behaviour to copy.
    - Each wired key needs its own test, and each keeps a setting vanilla doesn't have.
  - Keep them and document that they do nothing. No work, and admins go on editing keys that change nothing.
  - Waits on it: G3-03.
  - Answered 2026-10-31 (Neon): remove them all with G3-03's step, one log line per key. G3-05 already drops the egg description's `write_in_place` advice.
- D3-3: What should `default_difficulty` do? Options:
  - Apply it at every start, as vanilla `DedicatedServer.forceDifficulty` does. This matches vanilla. On a server where `/difficulty` changed `level.dat`, the difficulty goes back to the file's value at the next restart. Generated files say `Normal`, so admins will notice this once.
  - Remove the key, keeping `level.dat` as the only source, and have the management API report the world's difficulty. Existing servers see no change, but this differs from vanilla, where `server.properties` wins.
  - Leave it as a value that only the management API reports. No work, and the key keeps looking as if it does something.
  - Waits on it: G3-07.
- D3-4: Should the server write changes made at runtime back to `pumpkin.toml`, as vanilla does with `server.properties`? Options:
  - Yes, for every setting vanilla writes back (19 `settings.update` calls in `DedicatedServer`) that has a PEX key. This matches vanilla. The server then rewrites the admin's file while it runs, and a hand edit made in that time is lost on the next write. Vanilla has the same race.
  - Only `/whitelist on` and `off`. That is the one with a security effect: today a restart turns off a whitelist that was turned on by command. It is also the smallest change.
  - No, and document that changes made by command or through the management API last until the next restart.
  - Waits on it: G3-06.
- D3-5: What should happen to the defaults that point at upstream Pumpkin? The earlier roadmap recorded "telemetry off by default, admins can turn it on" (2026-10-29), but `TelemetryConfig::default` still has `enabled: true`. Open questions:
  - (a) New files only. Existing generated files have `enabled = true` written in them, so current servers keep reporting.
  - (b) Also a migration that turns telemetry off in existing files. A generated `true` can't be told apart from one an admin chose, so the migration logs a line saying how to turn it back on.
  - The same question applies to `server_links.bug_report`, which points at Pumpkin's issue tracker. Keep it, point it at `https://github.com/Conman1533/PEX/issues`, or leave it empty?
  - Waits on it: G3-08.
  - Partly answered 2026-10-31 (Neon accepted Q3.6): telemetry is off by default and admins can turn it on. Still open: (a) or (b) for existing files, and what `server_links.bug_report` points at.
- D3-6: How should the egg set the port? Options:
  - Keep the `file` parser on the default line. No server change. But it also rewrites `networking.query.address`, and it stops matching once the line holds a port. Whether Wings copes with a changed allocation was not checked.
  - A Wings parser that edits `networking.java.address` by key, if Wings has one that writes TOML. That was not checked in this session. Only the egg changes.
  - PEX reads Pelican's `SERVER_PORT` environment variable for `networking.java.address` (Wings sets it; not checked in this session). This works on every start and after allocation changes. But it adds a second config source to the server, which `AGENTS.md` ("Keep it as small as the task") argues against, and upstream's egg wouldn't benefit.
  - Waits on it: G3-05.

## Goal 4: Offline UUIDs match vanilla

Offline UUIDs match vanilla, with a player-data migration.

Start from: how PEX derives offline UUIDs vs vanilla (v3 UUID of "OfflinePlayer:<name>"); everything keyed by player UUID (playerdata, stats, advancements, ops/whitelist/bans, plugin data).

### Current state

Mostly done. The vanilla derivation and the whole PEX 1.x to 2.0 migration reached alpha in merge `24490ad12` (2026-10-27): `fix/offline-uuid-vanilla`, `feat/offline-uuid-migration`, `feat/offline-uuid-world`, `feat/offline-uuid-plugins`, `feat/offline-uuid-login-migration` and `feat/offline-uuid-world-rerun` are all ancestors of `300621c3e` (`git merge-base --is-ancestor`). Since then only the login name check touched these files (`6d58e3a63`, `4ed21e1dc`). What is left: repeatable proof at the release commit, tests for the wiring, upgrade docs, and vanilla differences in how names map to UUIDs and how the server lists are stored.

Derivation (paths under `crates/pumpkin-core/src/` unless given in full):

- `offline_uuid` (`net/mod.rs`) is `uuid::Builder::from_md5_bytes` of `md5("OfflinePlayer:" + name)`, a version 3 UUID, like vanilla `UUIDUtil.createOfflinePlayerUUID` (`/srv/pex/vanilla-26.3/src/net/minecraft/core/UUIDUtil.java`, `UUID.nameUUIDFromBytes` of the UTF-8 bytes). Test `offline_uuid_matches_vanilla` (`net/mod.rs` `tests`) checks six names, including Polish, CJK and emoji ones, against vanilla's values (five of them written by a vanilla 26.3 server).
- `legacy_offline_uuid` (`net/mod.rs`) is the PEX 1.x UUID (first 16 bytes of SHA-256 of the name, no version bits), used only by the migration. Test `legacy_offline_uuid_matches_pex_1_2` checks it against what PEX 1.2.0 wrote.
- `offline_uuid` callers (`git grep -n "offline_uuid("`): offline login `PendingConnection::handle_login_start` (`net/java/login/login_start.rs`), BungeeCord without a forwarded UUID `bungeecord_login` (`net/proxy/bungeecord.rs`), unknown names in offline mode in `GameProfileResult::resolve` (`command/argument_types/game_profile.rs`, used by `op`, `deop`, `whitelist`, `ban`, `pardon`) and `fetch_profile_by_name_helper` (`command/commands/fetchprofile.rs`), `Server::migrate_offline_player` (`server/mod.rs`) and PEXGuard `move_members` (`native-plugins/pexguard/src/uuid_migration.rs`).
- Velocity and Vine take the UUID the proxy forwards (`read_game_profile` in `net/proxy/velocity.rs` and `net/proxy/vine.rs`). Online mode takes Mojang's from `authenticate` (`net/java/login/encryption_response.rs`); `PumpkinConfig::validate` (`crates/pumpkin-config/src/lib.rs`) refuses online mode without encryption, so the client's own UUID that `handle_login_start` stores first is always replaced.
- `SLoginStart::read` (`crates/pumpkin-protocol/src/java/server/login/login_start.rs`) still falls back to `Uuid::new_v3(&Uuid::nil(), "OfflinePlayer:<name>")`, which also hashes the 16 namespace bytes and so differs from vanilla. It only runs for protocols that send no UUID (below 1.20.2), which the handshake refuses (`net/java/handshake.rs`, `LOWEST_SUPPORTED_MC_VERSION` is 26.3), and the value never becomes a player's identity. No piece.
- Bedrock players get the MD5 of `pocket-auth-1-xuid:<xid>` with version 3 bits (`extract_oidc_player_claims`, `xuid_to_uuid` in `crates/pumpkin-auth/src/jwt/mod.rs`), the same code as `v1.2.0` (`git show v1.2.0:crates/pumpkin-auth/src/jwt/mod.rs`), so their files need no migration. The startup pass counts version 3 UUIDs as vanilla's (`is_vanilla_uuid`) and the login move only runs on the Java path (`net/java/pending.rs`). With Bedrock `online_mode = false`, every token with neither `xid` nor `leguuid` gets the same UUID; that is Goal 8's to judge.
- Not vanilla: in offline mode `GameProfileResult::resolve` stores `offline_uuid` of the name as typed and never asks Mojang. Vanilla `CachedUserNameToIdResolver.get` lowercases the name (`toLowerCase(Locale.ROOT)`), `lookupGameProfile` asks Mojang's `GameProfileRepository` even in offline mode (`Services.create` builds it unconditionally, `DedicatedServer` only calls `resolveOfflineUsers(!usesAuthentication())`), and only then uses `NameAndId.createOffline` of the lowercased name. Vanilla `FetchProfileCommand.resolveName` goes through the same cache (`ProfileResolver.Cached`). No test covers PEX's rule (`game_profile.rs` has no `#[test]`). See D4-1.

Migration:

- Trigger: `OFFLINE_UUID_VERSION = 2` with `Step::ServerFiles` in `PumpkinConfig::MIGRATIONS` (`crates/pumpkin-config/src/lib.rs`). `main` (`crates/pumpkin/src/main.rs`) runs `migrate_offline_uuids` then `migrate_world_uuids` while the pending `config_version` is below 2, exits 1 before `PendingSave::save` on failure, and on every start runs `migrate_world_uuids_of_new_players` before `PumpkinServer::new`. A 1.x `pumpkin.toml` loads as version 1 (test `release_files_migrate_to_the_current_version`, `crates/pumpkin-config/tests/config_version.rs`), and a missing one is pending at `UNVERSIONED_CONFIG_VERSION` (1) (`LoadConfiguration::load`), so a 1.x world in a fresh install is migrated too. Only 1.x files copied into a server already at version 2 skip the startup pass. `main.rs` has no tests (`grep -c "#\[test\]"` gives 0), so nothing checks this order.
- Startup pass (`data/offline_uuid_migration.rs`): names come from the lists (`read_player_lists`, `players_by_old_uuid`), and only UUIDs that are `legacy_offline_uuid` of a listed name change, so Mojang and vanilla UUIDs stay. Backups come first, the mapping goes to `world/players/uuid-migration.json` (`MAPPING_FILE`), files no name matches stay and are listed as `unmatched` (except the roughly 1 in 32 legacy hashes that look like version 3 or 4 UUIDs, `is_vanilla_uuid`; the login move still catches those). A list that doesn't parse stops the run before anything changes.
- Login move: in `net/java/pending.rs`, after `can_not_join` and before `ReadyToPlay`, `Server::migrate_offline_player` moves a player's files from `legacy_offline_uuid(name)` when the new UUID has none, and kicks with `multiplayer.disconnect.invalid_player_data` when that fails. Unit tests cover the function; nothing covers the call in `pending.rs`.
- The real-client play test of a PEX 1.2.0 world ran on 2026-10-27 on `integrate/uuid-batch` `246b60af0` (`/srv/pex/playtest/captures/offline-uuid-batch/`, `/srv/pex/scratch/playtest-uuid/BINARIES.txt`), not at `300621c3e`, and can't be rerun from the repo. The bot proof the earlier roadmap planned (`test/offline-uuid-upgrade`) was never made (`git branch -a` has no such branch).

Every store keyed by player UUID:

| Store | Where | Moved by | Test |
| :-- | :-- | :-- | :-- |
| Player data `<uuid>.dat`, `.dat_old` | `world/players/data`, `PlayerDataStorage::get_player_data_path` (`crates/pumpkin-world/src/data/player_data.rs`) | startup pass and login move; the `UUID` tag is rewritten (`migrated_contents`) | `migrates_a_1_2_server_once`, `moves_an_unmatched_player_when_they_log_in_once`, `keeps_files_when_the_new_uuid_has_its_own_and_finishes_a_stopped_move`, `finishes_a_stopped_run_and_keeps_data_already_under_the_new_uuid` |
| Statistics | inside player data as `Statistics` (`Statistics::write_nbt`, `entity/player/statistics.rs`), so they move with the `.dat`; a vanilla `players/stats/<uuid>.json` is renamed too (`PLAYER_FILES`) | same | `migrates_a_1_2_server_once` |
| Advancements | `world/players/advancements/<uuid>.json` (`AdvancementManager::new`, `data/advancement_data.rs`; `PlayerAdvancement::new`, `entity/player/advancement.rs`) | same | same tests |
| `ops.json`, `whitelist.json`, `banned-players.json`, `usercache.json` | `data/` (`DATA_FOLDER`, `data/mod.rs`; `OperatorConfig`, `WhitelistConfig`, `BannedPlayerList`, `USER_CACHE_PATH`). Vanilla keeps them in the server root (`PlayerList.OPLIST_FILE`, `WHITELIST_FILE`, `USERBANLIST_FILE`; `usercache.json` in `Services.create`). Login checks match by UUID (`is_whitelisted`, `BannedPlayerList::get_entry`). | startup pass rewrites `uuid` fields (`update_entries`) after `data/<list>.json.v1.bak` | `migrates_a_1_2_server_once`, `stops_before_changing_anything_on_a_list_it_cannot_read` |
| `banned-ips.json` | `data/` | holds no UUIDs | none needed |
| World references (owners, throwers, anger targets, gossip, trial spawner and vault lists, head profiles, boss bars, raids) | entity and block entity NBT in `region/` and `entities/` of every dimension (Anvil, Linear, Pump through `edit_region`), player data and saved data (`move_references`, `replace_in_tag`, `data/offline_uuid_migration/world.rs`); int-array and dashed-string UUIDs | startup pass, then for players moved at login at the next start (`RECORD_FILE` `world/players/uuid-migration-world.json`) | `moves_references_in_every_format_and_file_once`, `moves_references_to_players_added_to_the_mapping_at_the_next_start` |
| PEXGuard regions | `plugins/data/pexguard/regions.toml`, owner and member `uuid` strings (`native-plugins/pexguard/src/region.rs`) | `uuid_migration::migrate` in `PexGuard::on_load`: `regions.toml.v1.bak`, applied copy in `plugins/data/pexguard/uuid-migration.json`, members the mapping lacks moved by their stored name (`move_members`) | `moves_owners_and_members_once`, `failure_keeps_regions_toml_and_the_backup` |
| PEXProtect history | SQLite `blocks.actor` holds the player's name (`native-plugins/pexprotect/src/db.rs` schema, `lib.rs` uses `gameprofile.name`); the `inspecting` UUID set is memory only | not needed | none needed |
| WorldPumpkin | `grep -rn -i uuid native-plugins/worldpumpkin/src` finds nothing | not needed | none needed |
| Other plugins' data | `plugins/data/<plugin>` (`Context::get_data_folder`, `plugin/api/context.rs`); `native-plugins/PORTING_GUIDE.md` tells authors to key their own files by UUID | nothing. Native plugins can read `MAPPING_FILE` as PEXGuard does; Wasm plugins can only reach their own data folder (`FS_READ_DATA`, `FS_WRITE_DATA` in `plugin/permissions.rs`). See D4-3. | none |

Format and place of the lists, outside the migration:

- `UserCache` (`data/usercache.rs`) writes `expiresOn` as RFC 3339 (`format_cache_date`) and skips entries in any other form (`load_entries`), so a vanilla `usercache.json` (`yyyy-MM-dd HH:mm:ss Z`, `CachedUserNameToIdResolver.createDateFormat`) loads empty, and vanilla's `readGameProfile` drops PEX's entries with "Failed to parse date" (by reading the code, not run). Entries expire after 30 days (`one_month_from_now`) instead of one calendar month (`GAMEPROFILES_EXPIRATION_MONTHS`), and keys are lowercased with `to_ascii_lowercase` instead of `toLowerCase(Locale.ROOT)`. The migration reads only `name`, so it isn't affected.
- Ban lists write dates as `2026-10-31 12:00:00+00:00` (`DATE_FORMAT`, `data/banlist_serializer.rs`); vanilla writes `2026-10-31 12:00:00 +0000` (`BanListEntry.DATE_FORMAT`).
- No doc mentions the migration: `git grep -n -i "uuid-migration"` outside the code finds nothing, and there is no hand-written release notes file (`rust.yml` and `release.yml` generate the notes). Upgrading also adds `validate_player_names = true` to 1.x configs (`release_files_migrate_to_the_current_version`), so 1.x players with non-ASCII names or spaces are refused at login (`is_allowed_login_name`, `net/mod.rs`) although their data is migrated.
- Outside the repo, `/srv/pex/gameplay-bench/whitelist.py` still writes SHA-256 UUIDs for kind `pumpkin`, and its `README.md` tells PEX to use that kind. For alpha, G0-03 needs kind `vanilla`.
- Harmless noise: `<uuid>_corrupted_<date>.dat` copies (`PlayerDataStorage`) end in `.dat`, so each world pass logs "Couldn't read ... as NBT" for them (`list_nbt_files`, `edit_nbt_file`).

### Pieces

#### G4-01 Upgrade test on a PEX 1.2.0 server directory

- Scope: in: move the startup sequence in `crates/pumpkin/src/main.rs` (the `OFFLINE_UUID_VERSION` step with `migrate_offline_uuids` and `migrate_world_uuids`, `PendingSave::save`, then `migrate_world_uuids_of_new_players`) into one function in `pumpkin-core` that `main` calls, keeping `main`'s `exit(1)`; a small committed fixture made by PEX `v1.2.0` in offline mode, not by hand (three players with data and advancements, an op, whitelist entries, a ban, the `pumpkin.toml` it wrote, and the region and entities chunks holding a tamed wolf and a trial spawner the player activated, trimmed from a world like `/srv/pex/scratch/playtest-uuid/base-120`); an integration test that copies the fixture to a temp dir and runs that function. Out: changes to migration behaviour; PEXGuard (its tests already use the 1.2.0 `regions.toml` format); vanilla loading the result (Proof).
- Depends on: G3-03 lands first, or after this one, but not at the same time: both change the `config_version` gate (`PendingSave`, `Step::ServerFiles`).
- Branch: test/offline-uuid-upgrade-fixture
- Needs decision: none
- Done when: `upgrades_a_pex_1_2_0_server` in `crates/pumpkin-core/tests/` (fixture in `tests/fixtures/`, like `crates/pumpkin-config/tests/`) passes and asserts player files under the vanilla UUIDs with their `UUID` tag, lists rewritten with `.v1.bak` copies, the wolf's `Owner` and the trial spawner's player list on the new UUID, and `config_version = 2` saved; a second run changes no byte; an unreadable list leaves `config_version = 1` and every file untouched. The PR says it checked by hand that the test fails when the world step is dropped or the save moves ahead of the migration.

#### G4-02 Tests for the offline UUID at login and the login move

- Scope: in: a pumpkin-tester bot test on its own server (temp dir with `save_player_data = true`, unlike `create_test_server` in `tools/pumpkin-tester/src/harness.rs`): a bot named `Notch` gets the server-side UUID `b50ad385-829d-3141-a216-7e7d7539ba7f` (vanilla's, as in `offline_uuid_matches_vanilla`); with `players/data/fe3625b2-f47a-1462-c456-19d51eedef7d.dat` (PEX 1.2.0's UUID for `Player_123456789`, `XpLevel` 7) in place, a bot of that name joins with XP level 7, and the file ends up under `c0d79c73-221a-32dc-b983-3cfa4a8a971a` and in `uuid-migration.json`. A unit test in `net/proxy/bungeecord.rs`: forwarded data with an IP and no UUID gives `b50ad385-829d-3141-a216-7e7d7539ba7f` for `Notch`. Out: Velocity and Vine (the proxy sends the UUID), Bedrock, the kick on a failed move.
- Depends on: none
- Branch: test/offline-uuid-login
- Needs decision: none
- Done when: `cargo run -p pumpkin-tester` reports `[PASS] bot:offline_uuid_login` and the BungeeCord test passes in `cargo nextest run -p pumpkin-core`; the PR says both failed by hand with the `migrate_offline_player` call removed from `net/java/pending.rs` or `legacy_offline_uuid` swapped in for `offline_uuid`.

#### G4-03 Offline name lookup in `op`, `whitelist`, `ban`, `pardon` and `fetchprofile`

- Scope: in: the offline branch of `GameProfileResult::resolve` and of `fetch_profile_by_name_helper`, and `UserCache::get_by_name` lowercasing, as D4-1 decides; one GameTest next to `pumpkin:quoted_non_ascii_player_name_in_commands` (`server/builtin_game_tests/commands_and_entities_extended.rs`) that runs `whitelist add` and `op` for names the server hasn't seen. Out: online mode, entity selectors, the cache file format (G4-05).
- Depends on: none
- Branch: fix/offline-name-lookup
- Needs decision: D4-1
- Done when: the GameTest passes, with expected UUIDs written as literals taken from a vanilla 26.3 offline server (its `whitelist.json` and `ops.json` after the same commands, or its `usercache.json` after the player logs in, whichever D4-1's rule matches), and fails on alpha if D4-1 changes the rule.

#### G4-04 Upgrade notes for admins and plugin authors

- Scope: in: `docs/UPGRADING-2.0.md`, linked from `README.md`: what the first 2.0 start does to an offline server (files moved, `world/players/uuid-migration.json`, `unmatched`); every backup it leaves (`world/players.v1.bak`, `data/<list>.json.v1.bak`, `<file>.v1.bak` beside region, saved-data and login-moved player files, `plugins/data/pexguard/regions.toml.v1.bak`) and when they can be deleted; that a failed migration exits 1 and retries on the next start; how to migrate 1.x files copied into a server already at `config_version = 2` (set it to 1); that restoring `regions.toml.v1.bak` needs `plugins/data/pexguard/uuid-migration.json` deleted; that pets, vaults and trial spawners of players moved at login follow them after the next restart; that `validate_player_names = true` refuses 1.x players with non-ASCII names or spaces; and whether offline players' default skins change (check that against the vanilla client before writing it). For plugin authors: the `uuid-migration.json` format (`Mapping` in `data/offline_uuid_migration.rs`) and PEXGuard's pattern, linked from the `PersistentDataContainer` row of `native-plugins/PORTING_GUIDE.md`. Out: a plugin API for the mapping (D4-3, Goal 2).
- Depends on: none
- Branch: docs/offline-uuid-upgrade
- Needs decision: D4-3 (only for the Wasm plugin paragraph)
- Done when: every path and log line the doc names appears in a scratch upgrade of a PEX 1.2.0 server directory (for example a copy of `/srv/pex/scratch/playtest-uuid/base-120`), listed in the PR, and `README.md` links the doc.

#### G4-05 `usercache.json` and ban dates in vanilla's format

- Scope: in: `UserCache` (`data/usercache.rs`) reads `expiresOn` as vanilla's `yyyy-MM-dd HH:mm:ss Z` and as RFC 3339 (PEX 1.x and alpha files), writes vanilla's, expires an entry one calendar month after it is added (`CachedUserNameToIdResolver.add`), and lowercases keys like `toLowerCase(Locale.ROOT)`; ban lists (`DATE_FORMAT`, `data/banlist_serializer.rs`) write `created` and `expires` like `BanListEntry.DATE_FORMAT` and read both forms. Out: where the files live (D4-2), name lookup (G4-03).
- Depends on: none
- Branch: fix/usercache-vanilla-format
- Needs decision: none
- Done when: unit tests in `usercache.rs` and `banlist_serializer.rs` load a `usercache.json` and a `banned-players.json` written by a vanilla 26.3 server (bytes in the test) with every entry, write dates byte-identical to vanilla's, and still load the files PEX 1.2.0 wrote; the vanilla-file tests fail on alpha.

#### G4-06 Server lists in vanilla's place

- Scope: in, as D4-2 decides: `ops.json`, `whitelist.json`, `banned-players.json`, `banned-ips.json` and `usercache.json` read from the server root (the `LoadJSONConfiguration::get_path` impls, `DATA_FOLDER` in `data/mod.rs`, `UserCache::path`), once when `data/` has none or always after a one-time move with backups, and the startup pass's `read_player_lists` pointed at the same place. Out: file formats (G4-05).
- Depends on: G4-05
- Branch: feat/server-lists-in-root
- Needs decision: D4-2
- Done when: a test with a vanilla 26.3 server's five files in a temp server root loads every entry (op level, whitelist, ban reason and expiry, cache), and a test with a PEX 1.2.0 `data/` directory keeps every entry (with `.bak` copies if D4-2 picks the move).

### Proof

- Tests: `offline_uuid_matches_vanilla`, the migration tests in the table above, `upgrades_a_pex_1_2_0_server` (G4-01), `bot:offline_uuid_login` (G4-02), G4-03's GameTest and G4-05's tests all pass on one alpha commit, in CI (G0-01) or `/srv/pex/tools/local-ci.sh`.
- Rehearsal on copies of the live worlds. The earlier roadmap records the answer that servers 30 and 34 run offline mode and get the upgrade on a copy first (the free tier runs no PEX); not checked here. Before the upgrade, dump for 3 active players per server their `.dat` (position, inventory, XP), their ops and whitelist entries, one tamed pet's `Owner` and their PEXGuard membership. Start the release build on each copy and record players moved, the `unmatched` count, region files read and the time taken; the second start must log no world pass ("Moving references" absent). Then those players join (bot or client). Pass: the dumps match apart from the UUIDs, the pet obeys, members can build.
- Vanilla direction: the migrated G4-01 fixture on a vanilla 26.3 offline server (`/srv/pex/vanilla-26.3/server-26.3.jar`), with the lists copied into its root (or in place after G4-06). Pass: a bot named `Notch` spawns at the position and with the inventory saved in `b50ad385-829d-3141-a216-7e7d7539ba7f.dat` and is op. Advancements and statistics carrying over need G7-22 (vanilla 26.3 reads PEX's advancement and statistics files).

### Decisions

- D4-1: In offline mode, which UUID should `op`, `whitelist add`, `ban`, `pardon` and `fetchprofile` use for a name the server hasn't seen? Options:
  - Keep PEX's rule: vanilla's offline UUID of the name as typed, no Mojang lookup. Works for a player who joins with exactly that name. Costs one documented difference from vanilla's lists and a test pinning it.
  - Vanilla's rule: ask Mojang first, then the offline UUID of the lowercased name. Lists match vanilla's byte for byte, but on an offline server the stored UUID only matches a player who joins with an all-lowercase name, so `whitelist add Alex` locks `Alex` out as it does on vanilla, and offline servers make blocking Mojang calls (`lookup_profile_by_name_blocking`, `net/authentication.rs`).
  - Lowercase only, no Mojang: matches a vanilla server without internet access. Mixed-case names are still locked out.
  - Waits on it: G4-03.
- D4-2: Should the player lists and `usercache.json` live in the server root like vanilla instead of `data/`? Options:
  - Keep `data/` (upstream Pumpkin's layout). No work; a server moved from or to vanilla loses its ops, whitelist and bans unless the admin moves the files.
  - Read the root files once when `data/` has none, keep writing `data/`. Vanilla to PEX works; PEX to vanilla still needs a manual copy. Small piece.
  - Move them to the root with a one-time move and backups. Works both ways; diverges from upstream Pumpkin's `data/` code, and scripts that write `data/whitelist.json` (the benchmark's `README.md` does) break.
  - Waits on it: G4-06.
- D4-3: Should plugins get the old-to-new UUID mapping through the plugin API? Options:
  - Docs only: native plugins read `world/players/uuid-migration.json` as PEXGuard does. Wasm plugins can't reach it, so their UUID-keyed data stays on old UUIDs unless they stored names and recompute the old hash themselves. No code.
  - An API call returning the mapping, native and WIT. Works for every plugin; adds a public WIT item that outlives the migration, and it's Goal 2's call whether v0.1 survives to carry it (D2-2 decides where a new WIT function goes).
  - Waits on it: the Wasm paragraph of G4-04, and a Goal 2 piece if the API option is picked.

## Goal 5: Redstone parity

Redstone parity (update order, quasi-connectivity, comparator timing).

Start from: how neighbour updates and scheduled ticks are queued and ordered; quasi-connectivity in pistons/dispensers/droppers; comparator and repeater timing; existing redstone tests.

### Current state

`core/` is `crates/pumpkin-core/src/`, `world/` here is `crates/pumpkin-world/src/`, vanilla classes are under `/srv/pex/vanilla-26.3/src/net/minecraft/`. "Derived" marks a consequence read from both sources and not run. No build or server was run for this section.

**Tick phases and parallelism**

- Vanilla `ServerLevel.tick` runs on one thread: time, `blockTicks.tick(tick, 65536, this::tickBlock)`, `fluidTicks.tick(...)`, raids, `getChunkSource().tick` (random ticks, spawning), `runBlockEvents()`, entities, `tickBlockEntities()`. Player packets run serially before the levels tick (`MinecraftServer.processPacketsAndTick`).
- PEX `World::tick` (`core/world/mod.rs`): `flush_synced_block_events`, time and weather, raids, `tick_chunks` (block ticks, fluid ticks, random ticks, spawning), players in `players.par_iter()` (each handles its packets in `Player::process_inbound_packets`), entities (islands in parallel), block entities, `flush_comparator_updates`. Worlds tick in parallel (`worlds.par_iter()` in `core/server/mod.rs`).
- `World::tick_chunks` runs the sorted block ticks as `par_chunks(BATCH_SIZE)` with `BATCH_SIZE = 32`, then fluid ticks the same way. Each Rayon worker drains its own neighbour cascade (`PENDING_UPDATES` is `thread_local!` in `core/world/neighbor_updater.rs`). Derived: once more than 32 block ticks are due in one game tick anywhere in the world, ticks in different batches run in no fixed order and their cascades can interleave on shared blocks. A structure with 32 or fewer due ticks per game tick runs in one batch and cannot show this.
- Block entities tick in `par_chunks(16)` over a list built from `World::block_entities` (`DashMap<chunk, FxHashMap<BlockPos, _>>`), so in hash order and in parallel. Vanilla ticks `blockEntityTickers` in insertion order (`Level.tickBlockEntities`). Hoppers, and comparators reading them, depend on this order.
- Entity contact (pressure plates, tripwire, detector rails: `changes_on_contact` in `core/world/entity_islands.rs`) starts cascades inside parallel islands, which bound same-tick block writes by `BLOCK_UPDATE_REACH = SIGNAL_MAX + 2`. A powered rail line extends a cascade by up to 8 rails past a wire's 15 blocks (`PoweredRailBlock.findPoweredRailSignal`, `searchDepth >= 8`), so the bound looks short. Not tested.

**Neighbour updates**

- Order matches vanilla. `UpdateStack::promote_staged` pushes staged updates reversed, so updates submitted during an update run first, in submission order, before older siblings, as `CollectingNeighborUpdater.runUpdates` does with `addedThisLayer`. PEX flattens a six-neighbour update into six entries where vanilla's `MultiNeighborUpdate` resumes after the new layer; the sequence is the same. `BlockDirection::update_order()` (W, E, D, U, N, S) equals `NeighborUpdater.UPDATE_ORDER`, and `BlockRegistry::update_neighbors` uses `abstract_block_update_order()` (W, E, N, S, D, U) like `BlockBehaviour.UPDATE_SHAPE_ORDER`. Both capture the neighbour state of a shape update when it is submitted (`neighbor_updater::update_shape`, `CollectingNeighborUpdater.ShapeUpdate`).
- Limit: `MAX_CHAINED_UPDATES = 1_000_000` is hardcoded; vanilla reads `max-chained-neighbor-updates` (default 1000000, negative means unlimited, `DedicatedServerProperties`). PEX counts each neighbour (`UpdateStack::stage`), vanilla counts one per `addAndRun` call, so a six-neighbour update costs 6 in PEX and 1 in vanilla. `grep -rni chained crates/pumpkin-config/src` finds no key.
- No shape-update depth limit. Vanilla passes `updateLimit` (512 from `Level.setBlock`) through `NeighborUpdater.executeShapeUpdate` and `Block.updateOrDestroy`; `grep -rn update_limit` over `core/` finds nothing.
- Source block: `World::update_neighbors` uses the block at the updated position as the source. `RedstoneGateBlock::update_target` (`core/block/blocks/redstone/abstract_redstone_gate.rs`) and `update_neighbors` in `redstone_torch.rs` call it where vanilla passes the diode or torch (`DiodeBlock.updateNeighborsInFront`, `RedstoneTorchBlock.notifyNeighbors`). Vanilla `RailBlock.updateState` re-orients a three-way rail only when the source is a signal source. PEX `rails/rail.rs` `on_neighbor_update` only checks support, so redstone rail switching is missing outright.
- `placed` runs only when the block type changes (`World::on_block_state_set`, `is_new_block`). Vanilla calls `onPlace` on every state change (`LevelChunk.setBlockState`, unless flag 512), and `DiodeBlock.onPlace` and `RedstoneTorchBlock.onPlace` update neighbours without a type check. Repeater, comparator and torch ticks make that call themselves. Missing: repeater delay changes (`RepeaterBlock::on_use`), comparator mode changes (`ComparatorBlock::normal_use`), lock changes from shape updates (`RepeaterBlock::get_state_for_neighbor_update`), and the second front update vanilla sends when a comparator's `POWERED` flips (`ComparatorBlock.refreshOutputState`: `setBlock(..., 2)`, then `updateNeighborsInFront`).

**Scheduled ticks**

- `ChunkTickScheduler` (`world/tick/scheduler.rs`): per chunk, a ring of 256 `Vec` slots (`MAX_TICK_DELAY`, delays are `u8`) and a dedupe set `queued_ticks`. `Level::get_tick_data` (`world/level.rs`) appends every active chunk's due slot and `sort_unstable`s by `(priority, sub_tick_order)` (`OrderedTick::cmp`), like `ScheduledTick.INTRA_TICK_DRAIN_ORDER`. `sub_tick_order` comes from `Level::schedule_tick_counts`, with `ORDERS_PER_ENTITY` reserved per entity (`world/tick/order.rs`) so parallel entities keep list order. Delays count like vanilla (test `delay_counts_game_ticks_like_vanilla`).
- Priorities and delays match: repeater `High`, `VeryHigh` or `ExtremelyHigh` with delay * 2, rescheduled `VeryHigh` (`repeater.rs` `update_powered`, `on_scheduled_tick`; `DiodeBlock.checkTickOnNeighbor`, `DiodeBlock.tick`); comparator `Normal` or `High` with 2 (`comparator.rs`; `ComparatorBlock.checkTickOnNeighbor`); torch 2, observer 2, dispenser and dropper 4.
- Tick type is not checked. `tick_chunks` calls `on_scheduled_tick` on whatever block is at the position and ignores `OrderedTick::value`; vanilla `ServerLevel.tickBlock` runs only if `state.is(type)`, and `tickFluid` likewise.
- No `willTickThisTick`. `step_tick` removes collected ticks from `queued_ticks` before they run, and repeater, comparator and torch skip scheduling only on `is_block_tick_scheduled`. Vanilla `DiodeBlock`, `ComparatorBlock` and `RedstoneTorchBlock` ask `LevelTicks.willTickThisTick`, which stays true while a tick is collected and not yet run. Derived: a diode or torch updated earlier in the tick its own tick is due schedules a second tick in PEX. Observer and target block use `hasScheduledTick` in vanilla and match.
- `FromIterator for ChunkTickScheduler` gives every loaded tick order 0, so equal-priority ticks due together after a chunk load run in `sort_unstable` order; vanilla `LevelChunkTicks.unpack` numbers them from `-pendingTicks.size()` in saved order. `step_tick` drains the whole slot (no 65536 cap). `skip_tick` puts overdue ticks first, but the global sort then orders by priority and sub-tick only, where vanilla drains a chunk by trigger tick first (`ScheduledTick.DRAIN_ORDER`).

**Block events and pistons**

- `World::add_synced_block_event` pushes onto a `Vec`; `BlockEvent` (`core/block/mod.rs`) has no block field. `flush_synced_block_events` takes the queue at the start of `World::tick`, so events queued during the flush wait a tick, duplicates are kept and the block type is not checked. Vanilla `ServerLevel.blockEvent` adds to an `ObjectLinkedOpenHashSet`, `runBlockEvents` loops until it is empty after block, fluid and random ticks, and `doBlockEvent` checks `state.is(block)`.
- Derived: a piston fired by a scheduled tick in game tick N starts at the start of N+1 in PEX, so its `PistonBlockEntity` (`core/block/entities/piston.rs`, +0.5 per tick like `PistonMovingBlockEntity`) first ticks in N+1 instead of N and pushed blocks land one tick later than in vanilla. A piston that triggers another through a neighbour update adds a tick per link.
- Sticky retraction in `PistonBlock::handle_synced_block_event` (`core/block/blocks/piston/piston.rs`) tests `data == 2`, and `data` is the direction byte (`BlockDirection::to_index`, 2 is North). Vanilla tests the event type (`b0 != 1` in `PistonBaseBlock.triggerEvent`). Derived: north-facing sticky pistons never pull, and other sticky pistons pull on a fast (type 2) retraction where vanilla leaves the block. The builtin sticky tests face Up, Down and East (`pistons_extended.rs`).
- Also missing against `triggerEvent` and `checkIfExtend`: finishing an extending moving block two blocks out (`pistonPiece`), the `getLastTicked()` and `isHandlingTick()` conditions for event 2 (`try_move` has "TODO: more stuff"), the retract flags (vanilla 276 has no neighbour or client flag; PEX sets `NOTIFY_ALL | FORCE_STATE` and removes the head at once), and the world border and build height checks of `isPushable` (`PistonBlock::is_movable`, "TODO: more checks"). The push limit of 12 and slime and honey exist (`piston/mod.rs` `MAX_MOVABLE_BLOCKS`, `is_adjacent_block_stuck`); `PistonHandler` was not audited line by line.

**Quasi-connectivity**

- Pistons: `should_extend` follows `PistonBaseBlock.getNeighborSignal` (five non-front sides, then the sides of the block above except down) apart from the middle check. Vanilla `level.hasSignal(pos, Direction.DOWN)` reads the piston itself, which is never a conductor (`Blocks.pistonProperties`, `isRedstoneConductor(Blocks::never)`), so it never fires. PEX passes the block below with the piston's position to `is_emitting_redstone_power`; when the block below is a conductor, `get_redstone_power` returns `get_max_strong_power` at the piston's position, the strong power reaching the piston from any side. Derived: a piston on stone extends from a powered repeater pointing into its front face, where vanilla stays retracted.
- Dispensers and droppers match: `on_neighbor_update` checks `block_receives_redstone_power` at the block and at `up()` (`dispenser.rs`, `dropper.rs`), like `DispenserBlock.neighborChanged`. The crafter checks only its own position in both (`crafter.rs`, `CrafterBlock.neighborChanged`).

**Redstone wire**

- `update_power_strength` (`core/block/blocks/redstone/redstone_wire.rs`) updates the wire and its six neighbours from a `Vec` in fixed order (self, D, U, N, S, W, E). Vanilla `DefaultRedstoneWireEvaluator.updatePowerStrength` iterates a `HashSet<BlockPos>` (16 buckets, `Vec3i.hashCode` is `(y + z * 31) * 31 + x`), so the order depends on position. A Python model of `java.util.HashMap`, not vanilla itself, gives self, down, south, east, up, north, west at (0, 64, 0) and up, north, west, self, down, south, east at (1, 64, 0); none of six sampled positions matched PEX.
- Corner updates (`update_neighbors_of_neighboring_wires`) and indirect shape updates (`prepare`) iterate `BlockDirection::horizontal()`, which is N, S, W, E; vanilla `Direction.Plane.HORIZONTAL` is N, E, S, W. `World::update_neighbour_for_output_signal` has the same difference against `Level.updateNeighbourForOutputSignal`.
- Removal: vanilla `affectNeighborsAfterRemoval` runs for any removal with the neighbour flag that a piston didn't cause. It updates the neighbours of all six neighbours, reruns `updatePowerStrength`, then does the corner updates. PEX does the first and last step only, in `broken`, and only player breaking calls `broken` (`grep -rn "\.broken("` finds the Java and Bedrock action handlers and `Player`); the wire has no `on_state_replaced`. Derived: wire removed by water, an explosion, `/setblock` or loss of support never updates the blocks around the block below it, so a lamp or piston there can stay on.
- The wire's own `update_neighbors_at` helper submits six single updates through `World::update_neighbor` (`include_fluid: false`), where vanilla's `Level.updateNeighborsAt` is one `MultiNeighborUpdate`. Power output reads the stored connections (`get_weak_redstone_power`) where vanilla recomputes `getConnectionState`.
- Experimental redstone: `redstone_experiments` is recognised as a datapack (`core/data/datapack/mod.rs`), but there is no `ExperimentalRedstoneWireEvaluator` and no `Orientation` (`grep -rni orientation` over `core/` finds only the jigsaw block).

**Torches, repeaters, comparators**

- No torch burnout. `redstone_torch.rs` has nothing like `RECENT_TOGGLES` (8 toggles in 60 ticks, then a 160-tick restart and level event 1502, `RedstoneTorchBlock.tick`); `grep -i "toggle\|burn\|160\|1502"` on the file finds nothing.
- Comparator input matches `ComparatorBlock.getInputSignal` (analog block, else through a conductor to a block or a single item frame), and `calculate_output_signal` and `has_power` match `calculateOutputSignal` and `shouldTurnOn`. Repeater locking matches `RepeaterBlock.isLocked` (diodes only on the sides).
- Container changes set `comparator_dirty`, and `World::flush_comparator_updates` sends the output-signal updates once after all block entities ticked, in that list's order. Vanilla sends them inside `BlockEntity.setChanged` at the moment of change. Derived: a comparator reading a container schedules its tick later in sub-tick order than in vanilla, and which comparator hears first follows the parallel list.

**Tests**

- 197 builtin function GameTests (`core/server/builtin_game_tests/`, registered in `mod.rs`). Redstone files: `redstone_wire_and_power.rs` 14, `redstone_logic_circuits.rs` 12, `redstone_inputs.rs` 4, `redstone_gates.rs` 3, `redstone_lamps_and_bulbs.rs` 2, `comparators_extended.rs` 14, `note_and_comparators.rs` 2, `pistons_extended.rs` 12, `pistons.rs` 1, dispenser and dropper files 15, rail files 15 (some fluid tests among them), `buttons_levers_plates_variants.rs` 12, `lamps_bulbs_and_noteblocks_extended.rs` 12. They check end states or tick windows inside PEX only (for example `run_repeater_delay_test`).
- Unit tests: `neighbor_updater.rs` 5 (stack order, limit, panic reset), `tick/scheduler.rs` 3, `tick/order.rs` 1, pressure plates 7, daylight detector 2.
- Dual-run: 5 block-based structures in `assets/tests/datapacks/pumpkin-dual-run` (`wire_attenuation`, `torch_double_inverter`, `piston_pushes_redstone_block`, `repeater_lock_holds_off`, `comparator_subtract_clamps`), built on vanilla by `tools/dual-run/build_structures.py`, embedded into PEX by codegen (`crates/pumpkin-data/src/generated/test_instance.rs`), compared on pass or fail only by `dual_run.py`. The last recorded run (2026-10-24, `/srv/pex/scratch/redstone-dual-run/final-run/*-results.json`, on the branch before it merged) passed all 5 on both. Not re-run at `300621c3e`.
- Nothing covers quasi-connectivity, BUDs, update order, locational wire, torch burnout, more than 32 ticks due at once, sticky pull or spit, piston arrival tick or hopper clocks: `grep -rli "quasi\|bud\b\|burnout\|locational\|zero.tick"` over the test directories and `assets/tests` finds nothing.
- Placement: vanilla places a console `/test run` at the respawn point's x, the surface y and z + 3 (`TestCommand.createTestPositionAround`), so absolute coordinates depend on the world. `dual_run.py` does not pin them.

### Pieces

Each fix piece starts by adding its dual-run structure and showing it fails on `300621c3e` and passes on vanilla. If it passes on alpha, the finding above was wrong; the piece then corrects Current state and stops. New structures are embedded with `cargo run -p pumpkin-codegen`. "Passes on both" means `tools/dual-run/dual_run.py` exits 0, run by hand until G0-02 runs it.

#### G5-01 Dual-run pins test placement

- Scope: in: an option in `dual_run.py` that starts both servers on the same superflat world and runs tests from a fixed position (`execute positioned ... run test run`), and a check that both servers put each structure at the same origin. Out: new redstone structures, CI wiring (G0-02).
- Depends on: none
- Branch: test/dual-run-pinned-placement
- Needs decision: none
- Done when: with the option, both servers report the same origin for the 5 existing structures, all 5 still pass on both, and changing the pin moves the origin on both; `dual_run.py` exits 1 when the origins differ.

#### G5-02 Scheduled ticks run only for the block that scheduled them

- Scope: in: `World::tick_chunks` skips a block tick unless the block at the position is the tick's `value`, and a fluid tick unless the fluid matches (`ServerLevel.tickBlock`, `tickFluid`). Out: tick order (G5-05).
- Depends on: none
- Branch: fix/scheduled-tick-block-check
- Needs decision: none
- Done when: `pumpkin:redstone/tick_skips_replaced_block` (for example a lit lamp with a pending turn-off tick pushed away and replaced by an observer, which must not pulse) fails on alpha and passes on both.

#### G5-03 willTickThisTick for diodes and torches

- Scope: in: the scheduler keeps the ticks collected for this game tick until each runs and answers `will_tick_this_tick`; repeater, comparator and torch use it where vanilla calls `willTickThisTick` (`DiodeBlock.checkTickOnNeighbor`, `ComparatorBlock.checkTickOnNeighbor`, `RedstoneTorchBlock.neighborChanged`). Out: observer and target block (they match), test blocks.
- Depends on: none
- Branch: fix/will-tick-this-tick
- Needs decision: none
- Done when: new unit test `collected_tick_will_tick_until_run` in `world/tick/scheduler.rs` passes; `pumpkin:redstone/diode_update_before_own_tick` (a repeater whose input changes earlier in the tick its own tick runs) fails on alpha and passes on both.

#### G5-04 Scheduler order details

- Scope: in: loaded ticks keep their saved order (numbered from `-n` like `LevelChunkTicks.unpack`), overdue ticks from a skipped chunk run before ticks due now, and block and fluid ticks stop at 65536 per tick with the rest carried over. Out: parallel execution (G5-05), the `u8` delay of saved ticks (`ScheduledTick::from_nbt_compound`, a Goal 7 round-trip issue).
- Depends on: none
- Branch: fix/tick-scheduler-order
- Needs decision: none
- Done when: new unit tests `loaded_ticks_keep_saved_order`, `overdue_ticks_run_first` and `ticks_over_cap_run_next_tick` pass, and each fails on alpha.

#### G5-05 Block and fluid ticks run in vanilla order

- Scope: in: `tick_chunks` runs block ticks, then fluid ticks, in sorted order, either serially or in partitions that never split a cascade, as D5-1 decides. Out: random ticks and spawning, block entities (G5-15), players, entity islands.
- Depends on: G0-03
- Branch: fix/ordered-block-ticks
- Needs decision: D5-1
- Done when: `pumpkin:redstone/many_ticks_one_tick` (64 or more diodes due in the same game tick with an outcome that depends on their order) passes on both and PEX gives the same result in 20 runs; the PR records MSPT of a redstone stress world and a gameplay-bench run before and after, with tick P95 within 5% of the G0-03 baseline.

#### G5-06 Block events run like vanilla

- Scope: in: `BlockEvent` carries the block; the queue keeps insertion order without duplicates, runs after the random ticks in `tick_chunks` and before players and entities, drains until empty and skips events whose block changed (`ServerLevel.runBlockEvents`, `doBlockEvent`). Out: what pistons do inside the event (G5-07).
- Depends on: none
- Branch: fix/block-event-phase
- Needs decision: none
- Done when: `pumpkin:redstone/piston_arrival_tick` (a pushed redstone block races a repeater chain and reaches ACCEPT only on vanilla's arrival tick) and `pumpkin:redstone/piston_bud_chain` fail on alpha and pass on both; the builtin piston, note block and chest GameTests still pass.

#### G5-07 Sticky piston retraction matches triggerEvent

- Scope: in: test the event type instead of `data == 2`; finish an extending moving block two blocks out; send event 2 when the moving block was ticked this game tick or the level is in its block phase (`checkIfExtend`); use vanilla's retract flags (276) and update sequence. Out: activation (G5-08), structure resolution in `PistonHandler`.
- Depends on: G5-06
- Branch: fix/sticky-piston-retract
- Needs decision: none
- Done when: `pumpkin:redstone/sticky_piston_pulls_north`, `pumpkin:redstone/sticky_piston_spits_on_short_pulse` and `pumpkin:redstone/sticky_piston_pulls_moving_block` fail on alpha and pass on both.

#### G5-08 Piston activation and push bounds

- Scope: in: `should_extend` drops its middle check, which never fires in vanilla; `is_movable` gains the world border and build height checks of `PistonBaseBlock.isPushable`. Out: dispenser and dropper quasi-connectivity (it matches; regression structures only).
- Depends on: none
- Branch: fix/piston-activation
- Needs decision: none
- Done when: `pumpkin:redstone/piston_ignores_power_into_face` fails on alpha and passes on both; `pumpkin:redstone/piston_qc_needs_update` and `pumpkin:redstone/dropper_qc` pass on both before and after; builtin GameTest `piston_stops_at_build_limit` passes.

#### G5-09 Wire removal updates and horizontal order

- Scope: in: wire removal through `on_state_replaced` (any removal with the neighbour flag, not by piston) runs vanilla's `affectNeighborsAfterRemoval` sequence, `updatePowerStrength` included; N, E, S, W order in `update_neighbors_of_neighboring_wires`, `prepare` and `World::update_neighbour_for_output_signal`; the wire helper's updates go through `World::update_neighbors_at`. The PR lists the other callers of `BlockDirection::horizontal()` and keeps their order unless vanilla uses `Direction.Plane.HORIZONTAL` there. Out: the power update order (G5-10).
- Depends on: none
- Branch: fix/wire-removal-updates
- Needs decision: none
- Done when: `pumpkin:redstone/wire_washed_away_unpowers_lamp` fails on alpha and passes on both; the builtin wire GameTests still pass.

#### G5-10 Wire power updates follow vanilla's HashSet order

- Scope: in: `update_power_strength` visits its seven positions in `java.util.HashSet` iteration order for `Vec3i.hashCode`. Out: the experimental evaluator (D5-3).
- Depends on: G5-01
- Branch: fix/wire-update-order
- Needs decision: none
- Done when: a unit test in `redstone_wire.rs` matches the orders printed by a small Java program using `java.util.HashSet` and the decompiled `Vec3i.hashCode` (kept outside the repo) for at least 8 positions, negative coordinates included; pinned structures `pumpkin:redstone_locational/wire_order_a` and `wire_order_b` (one contraption at two positions with different orders) pass on both, and at least one fails on alpha.

#### G5-11 Diodes and torches update neighbours on every state change

- Scope: in: vanilla's state-change `onPlace` for repeater, comparator and torch: the front update after a delay change, a mode change or a lock change, and the comparator's second front update when `POWERED` flips. Either a state-change hook in `World::on_block_state_set` or explicit calls; the PR picks one and lists the other vanilla blocks whose `onPlace` has no type check. Out: the update source (G5-12).
- Depends on: none
- Branch: fix/diode-state-updates
- Needs decision: none
- Done when: `pumpkin:redstone/repeater_lock_updates_front` (a quasi-powered piston in front of a repeater whose lock flips) fails on alpha and passes on both.

#### G5-12 Update source and rail junction switching

- Scope: in: callers of `World::update_neighbors` that vanilla gives the emitting block (`RedstoneGateBlock::update_target`, `update_neighbors` in `redstone_torch.rs`) pass it; `RailBlock` re-orients a three-way junction when a signal source updates it (`RailBlock.updateState`, `RailState.place`). Out: powered, detector and activator rails.
- Depends on: none
- Branch: feat/rail-junction-switching
- Needs decision: none
- Done when: `pumpkin:redstone/rail_junction_switches` (a torch beside a T-junction turns off) fails on alpha and passes on both.

#### G5-13 Torch burnout

- Scope: in: a per-world list of recent toggles with vanilla's 60-tick window, 8 toggles, 160-tick restart tick and level event 1502 (`RedstoneTorchBlock.tick`, `isToggledTooFrequently`). Out: nothing else.
- Depends on: none
- Branch: feat/torch-burnout
- Needs decision: none
- Done when: `pumpkin:redstone/torch_burnout` (a torch wired to toggle itself, ACCEPT once it stays off past the window) fails on alpha and passes on both.

#### G5-14 Neighbour updater limits

- Scope: in: count one per submission like `CollectingNeighborUpdater.addAndRun`; read the limit per D5-2 (negative means unlimited); carry the 512 shape update depth through shape updates (`Level.setBlock` `updateLimit`, `Block.updateOrDestroy`). Out: neighbour order (it matches).
- Depends on: G3-03, only if D5-2 picks a config key (its step takes a `config_version` after G3-03's)
- Branch: fix/neighbor-update-limits
- Needs decision: D5-2
- Done when: new tests `multi_neighbor_update_counts_once` and `shape_updates_stop_at_depth_512` in `neighbor_updater.rs` pass and fail on alpha.

#### G5-15 Block entities tick in order and comparators hear container changes at once

- Scope: in: block entities tick in vanilla list order (insertion order, pending ones added first, `Level.tickBlockEntities`) as D5-1 decides; a container change sends the output-signal update when it happens (`BlockEntity.setChanged`) instead of through `flush_comparator_updates`. Out: hopper transfer rules (audited only if a structure fails).
- Depends on: G0-03
- Branch: fix/block-entity-order
- Needs decision: D5-1
- Done when: `pumpkin:redstone/hopper_clock_period` and `pumpkin:redstone/item_sorter` pass on both and give the same PEX result in 20 runs; `pumpkin:redstone/comparator_container_race` fails on alpha and passes on both; gameplay-bench tick P95 within 5% of the G0-03 baseline.

#### G5-16 Dual-run versions of the builtin redstone GameTests

- Scope: in: block-based versions of the builtin redstone GameTests that test blocks can express (wire, gates, latches, repeater delays and locking, comparators on containers, pistons, observers, rails, doors, dispensers, droppers, lamps, note blocks) in `pumpkin-dual-run`. Out: tests that need players or mobs; fixes for what they find (new pieces).
- Depends on: none
- Branch: test/redstone-dual-run-coverage
- Needs decision: D5-4
- Done when: at least 40 new structures pass on both, each listed against the builtin test it mirrors; a structure that differs stays out of the datapack and is added to Current state as a finding.

### Proof

- `dual_run.py` on `assets/tests/datapacks/pumpkin-dual-run` passes in the G0-02 job on `alpha` with at least 60 redstone structures (5 today): every structure named in G5-02 to G5-16, at least 2 pinned locational ones (G5-10) and at least 1 with 64 or more ticks due in one game tick (G5-05).
- Every structure a fix piece added failed on `300621c3e`, as recorded in its PR, or the finding was corrected in Current state.
- PEX gives identical results in 20 consecutive dual-run runs (`--reuse-vanilla`), so ordering is deterministic.
- After G5-05 and G5-15, gameplay-bench tick P95 is within 5% of the G0-03 alpha baseline, and the redstone stress world's MSPT before and after is recorded here.

### Decisions

- D5-1: How do scheduled block and fluid ticks and block entities run? Options:
  - Serially, in vanilla order. Exact order; loses the Rayon parallelism of those phases. The cost is unknown: gameplay-bench has no redstone (`grep -ril redstone /srv/pex/gameplay-bench` finds nothing), so G5-05 has to build a stress world to measure it.
  - In parallel over independent regions, serially inside each. Keeps most parallelism on spread-out worlds; needs a reach bound that holds for wire, rails, shape chains and pistons (the same open question as `BLOCK_UPDATE_REACH`), and a wrong bound gives rare non-vanilla order. Roughly two pieces instead of one.
  - Keep them parallel. No work; any build with more than 32 due ticks or 16 block entities in one tick can differ from vanilla and from run to run, and the proof has to leave large builds out.
  - Waits on it: G5-05, G5-15.
- D5-2: Is the chained neighbour update limit configurable? Options:
  - A key matching vanilla's `max-chained-neighbor-updates` (default 1000000, negative means unlimited). A new config key, which should land after Goal 3's versioning.
  - Vanilla's default as a constant. No config work; a server that changes it on vanilla can't on PEX.
  - Waits on it: G5-14.
- D5-3: Is experimental redstone (`minecraft:redstone_experiments`) part of 2.0? The earlier roadmap recorded "default redstone only for 2.0" on 2026-10-29; this restates it so it carries over. Options:
  - Out. No pieces. A world with the pack enabled runs the default evaluator and differs from vanilla with the pack.
  - In. Port `ExperimentalRedstoneWireEvaluator`, `ExperimentalRedstoneUtils` and `Orientation`, which vanilla threads through every neighbour update (`CollectingNeighborUpdater.MultiNeighborUpdate.runNext`). At least three pieces, after G5-10.
  - Waits on it: no piece if out.
  - Answered 2026-10-31 (Neon accepted Q5.1): out. Default redstone only for 2.0; experimental redstone can come later. No pieces.
- D5-4: What does the proof compare? Options:
  - Pass or fail of block-based tests, which `dual_run.py` does today. An order difference that doesn't change ACCEPT or FAIL goes unseen, so every structure has to be built as a race.
  - Also a per-tick trace: both servers record the block states of each test area every tick and the traces must match. Catches order and timing directly; needs a recorder on vanilla (a datapack function or a mod, outside the repo) and one tooling piece.
  - Waits on it: G5-16 and the Proof.
  - Partly answered 2026-10-31 (Neon accepted Q5.2): position-dependent behaviour isn't required for 2.0; contraptions that don't depend on position must give the same results. Still open: pass or fail only, or also a per-tick trace.

## Goal 6: Survival parity end to end

Survival parity end to end (start through dragon kill), proven with bots.

Start from: what survbot can do today; the checkpoints a start-to-dragon run needs (wood, iron, nether, blaze rods, pearls, stronghold, End), and which PEX mechanics each one depends on.

### Current state

Paths: `core/` is `crates/pumpkin-core/src/`, `tester/` is `tools/pumpkin-tester/src/`, `wgen/` is `crates/pumpkin-world/src/generation/`. Vanilla classes are from `/srv/pex/vanilla-26.3/src`.

#### Bots today

- `HeadlessBot` (`tester/bot.rs`) sends the packets listed in the G0 survbot row. It keeps its position, its inventory and open menu (`tester/bot/inventory.rs` `ClientInventory`; container slots are generic, so a furnace menu works), its loaded chunks, and entities of type `ITEM` only (`HeadlessBot::handle_entity_packet`). It has no rotation method (`move_to` sends `SPlayerPosition`), no `SUseItem`, no release of a used item, no drop, no arm swing and no `SClientCommand`, and it ignores `CRespawn`, `CGameEvent`, `CSetHealth` and every entity that isn't an item. `StageContext::walk_to` (`tester/survival/mod.rs`) walks a straight line, one step per tick; movement is client authoritative, so that is enough on flat staged ground.
- One survival stage: `survival:wood_to_stone_pickaxe` (`tester/survival/wood_to_stone_pickaxe.rs`), run first by `run_all_suites` (`tester/main.rs`), so it is part of CI's `gametest` job. It builds its area at y 200 with `fill`, so it doesn't depend on the seed. 116 logged runs under `/srv/pex/ci` and `/srv/pex/scratch` took 9 to 86 s, median about 30 s.
- The tester server (`create_test_server`, `tester/harness.rs`) sets `allow_nether: false` and `allow_end: false` and keeps the default random seed (`BasicConfiguration::default`, `Seed(random::get_seed())`, `crates/pumpkin-config/src/lib.rs`). `Entity::try_use_portal` (`core/entity/mod.rs`) refuses portals into a disabled dimension, so no stage can leave the Overworld.
- `LpVector3d::read` (`crates/pumpkin-protocol/src/codec/lp_vector_3d.rs`) reads two bytes before testing for a zero vector; vanilla `LpVec3.read` reads one. The bot works around it by reading `CSpawnEntity` only up to the position. The server has the same bug in `SInteract::read` (`crates/pumpkin-protocol/src/java/server/play/interact.rs`).
- Outside the repo, `/srv/pex/tools/survbot.py` runs load roles (builder, miner, afk, explorer) on stations a driver sets up with commands, and respawns after a death; it doesn't craft, smelt, fight or use portals. The Azalea bot (`native-plugins/tools/testbot/src/main.rs`) can chat, look, mine, use a block, attack a player and walk. The play-test rig (`/srv/pex/playtest/README.md`) drives a real 26.3 client for what bots can't see.

#### Checkpoints

| Checkpoint | Mechanics it needs | PEX at 300621c3e | Bot coverage |
| :-- | :-- | :-- | :-- |
| Wood to stone tools | dig timing, block loot, pickup, 2x2 and 3x3 crafting | Works. `Player::destroy_block` drops only when `Player::can_harvest` (`core/entity/player.rs`); `calc_block_breaking` (`core/block/mod.rs`); `block::drop_loot`; `match_crafting_recipe` (shaped, shapeless, transmute; `crates/pumpkin-inventory/src/crafting/crafting_screen_handler.rs`). | `survival:wood_to_stone_pickaxe`; `bot_tests/block_breaking.rs`, `bot_tests/loot_tables.rs` |
| Iron, food, bed | tool tier drops, furnace and fuel, buckets, eating, sleeping, respawn point | Present. Tier gate as above; furnace tick in `core/block/entities/furnace_like_block_entity.rs` (GameTest `test_furnace_ignites_and_smelts`); `core/item/items/bucket.rs` fills and empties in `normal_use`, which needs `SUseItem`; eating in `handle_use_item` (`core/net/java/play/use_item.rs`) and `core/entity/hunger.rs`; `BedBlock::use_bed` (`core/block/blocks/bed.rs`) calls `Player::set_respawn_point` and `Player::sleep`; `World::should_skip_night`, `World::respawn_player` (`core/world/mod.rs`). | none |
| Nether portal | obsidian from water on lava, flint and steel, portal shape, travel, linking | Present. `receive_neighbor_fluids` (`core/block/fluid/lava.rs`, GameTest `test_water_meets_lava_source_forms_obsidian`); `FireBlock::placed` (`core/block/blocks/fire/fire.rs`) calls `NetherPortal::get_new_portal`, which tries X then Z like vanilla `PortalShape.findEmptyPortalShape`; `PortalType::get_portal_destination` (`core/world/portal/mod.rs`) scales, clamps to the border, searches `World::portal_poi` (`NetherPortal::search_for_portal`) or builds a frame (`find_safe_location`, `build_portal_frame`, `core/world/portal/nether.rs`). | none; tester has the Nether off |
| Blaze rods | fortress, blaze spawner, fortress spawns, blaze AI, player-kill loot | Fortress pieces in `wgen/structure/structures/nether_fortress/`; `bridge_platform.rs` places the blaze spawner. `MobSpawnerBlockEntity::tick` (`core/block/entities/mob_spawner.rs`) only checks for free space: no Peaceful check, no `check_spawn_rules` (`core/entity/type.rs`) where vanilla `BaseSpawner.serverTick` calls `SpawnPlacements.checkSpawnRules` with `SPAWNER`, and SpawnData is reduced to the entity id (`TODO` at the spawn call). No structure spawns (see below). Blaze AI: goals, "not audited" in `docs/MOB-PARITY.md`. `LivingEntity::on_death` passes `killed_by_player` to the loot table (`core/entity/living.rs`), which `entities/blaze` needs. | none |
| Ender pearls | enderman AI and loot, or piglin bartering | Enderman: 12 goals, not audited (`docs/MOB-PARITY.md`). `PiglinAi::get_barter_response_items` (`core/entity/mob/piglin_ai.rs`) is a hard-coded 459-weight table of plain stacks, so the Soul Speed book and boots and the Fire Resistance potions lose their components. Vanilla `PiglinAi.getBarterResponseItems` rolls `gameplay/piglin_bartering`, which is generated as `GAMEPLAY_PIGLIN_BARTERING` (`crates/pumpkin-data/src/generated/loot_table.rs`) and unused. Piglins run their own `piglin_ai.rs`, not the Brain. | none |
| Eyes and stronghold | crafting, eye throw and flight, stronghold location and generation | `EnderEyeItem::normal_use` (`core/item/items/ender_eye.rs`) asks `find_nearest_structure` for the `strongholds` set and spawns `EyeOfEnder` (`core/entity/projectile/eye_of_ender.rs`). While the rings are still computing (`StructureSearch::Pending`) the eye isn't thrown at all; vanilla `ChunkGeneratorStructureState.getRingPositionsFor` waits (`join()`). Portal room: `wgen/structure/structures/stronghold/portal_room.rs` (12 frames, each with an eye at 10%, silverfish spawner). `locate structure` and `place structure` exist (`core/command/commands/locate.rs`, `place.rs`). | none; bot can't send `SUseItem` |
| End portal and arrival | eye on frame, frame shape, End platform | `EnderEyeItem::use_on_block` fills the frame and calls `EndPortal::get_new_portal` (`core/world/portal/end.rs`), its own shape check instead of vanilla's `EndPortalFrameBlock.getOrCreatePortalShape` pattern. Compared with vanilla `EnderEyeItem.useOn` it skips `Block.pushEntitiesUp`, `updateNeighbourForOutputSignal` and the `destroyBlock` of each portal position. Arrival builds the 5x5 platform at (100, 48, 0) and puts players at y 49, as vanilla `EndPortalBlock.getPortalDestination` does. | none; tester has the End off |
| Dragon fight | dragon spawn, crystals, damage through parts, phases, death, exit portal, egg | `DragonFight` (`core/world/dragon_fight.rs`) spawns the dragon and crystals, finds or builds the exit portal, places gateways and the egg. Gaps below. | none |
| Exit and credits | credits screen, respawn at the bed | Gaps below. | none |

#### Dragon (read in code, not run with a client)

- Parts aren't in the world. `EnderDragonEntity::new` (`core/entity/boss/ender_dragon.rs`) reserves 8 ids and builds `EnderDragonPart`s but never adds them (`// TODO: world.add_entity_silent`), and `World::get_entity_by_id` searches only `entities` and `players`. The client aims at parts (vanilla `EnderDragon.isPickable` returns false), so a melee hit names an id the server can't find and `handle_attack` (`core/net/java/play/attack.rs`) kicks with `multiplayer.disconnect.invalid_entity_attacked`. Vanilla resolves part ids with `ServerLevel.getEntityOrPart`, and `Level.getEntities` returns parts to projectiles.
- `EnderDragonPart::damage` forwards the full amount. Vanilla `EnderDragon.hurt(level, part, source, damage)` uses `damage / 4 + min(damage, 1)` for body parts, ignores sources that are neither a player nor tagged `always_hurts_ender_dragons`, and sends a sitting dragon to `TAKEOFF` after a quarter of its health. `EnderDragonEntity::hurt`, which counts sitting damage, has no caller.
- `DragonFight::on_crystal_destroyed` only recounts crystals. Vanilla `EnderDragon.onCrystalDestroyed` also hits the head for 10 when that crystal was healing the dragon, then tells the phase.
- The `Dying` phase (`phase/dying.rs`) runs the 200-tick sequence and calls `set_dragon_killed`. But `LivingEntity::damage` calls `on_death` when health reaches 0, the living tick removes a non-player 20 ticks after that (`core/entity/living.rs`), and `Dying` holds health at 1 only while the dragon is 10 to 150 blocks from the podium. Vanilla `EnderDragon.handleKillingBlow` sets health 1 and `DYING` instead, and `tickDeath` is overridden. Not verified whether PEX removes the dragon before the exit portal opens.
- Phases: PEX `EnderDragonPhase` numbers differ from vanilla's (`Charging = 2`, vanilla `LANDING_APPROACH` is 2), PEX adds `FlyToPortal` and lacks `SittingScanning`, nothing sets `tracked_data::ender_dragon::DATA_PHASE`, and no `DragonPhase` NBT is written. `dragon_fireball` is missing from `from_type()` (`core/entity/type.rs`); strafing spawns the breath cloud directly (`phase/strafing.rs`). This is mob AI, Goal 1's.
- Saving: `DragonFight` has no save or load. Vanilla stores `EnderDragonFight.CODEC` in `data/minecraft/ender_dragon_fight.dat`; `grep -rn ender_dragon_fight crates/` finds only the generated registry (the dimension type's `has_ender_dragon_fight`). `DragonFight::create_new_dragon` spawns with `spawn_entity_non_save`, so after a restart `scan_state` guesses from the exit portal and a fight in progress gets a new dragon at full health.

#### Credits

- Leaving the End, `get_portal_destination` sends `GameEvent::WinGame` with 1.0 on every exit and teleports the player at once to world spawn. Vanilla `EndPortalBlock.entityInside` calls `ServerPlayer.showEndCredits` only while `seenCredits` is false: the player leaves the level, gets `WIN_GAME` with 0.0, `wonGame` and `seenCredits` are set, and the player comes back only on `PERFORM_RESPAWN` through `PlayerList.respawn(player, true)` at the bed or respawn point. Later exits go to the respawn point (`findRespawnPositionAndUseSpawnBlock`) without credits.
- PEX never sets `Player::seen_credits` (only NBT reads and writes it, `core/entity/player.rs`), has no `wonGame`, and `handle_client_status` (`core/net/java/play/client_command.rs`) ignores `PERFORM_RESPAWN` while health is above 0. What the 26.3 client does with 1.0 against 0.0 isn't in the server jar.

#### Other gaps on the path

- Natural spawning ignores structures. `get_random_spawn_mob_at` and `can_spawn_mob_at` (`core/world/natural_spawner.rs`) read only the biome's spawners; vanilla `NaturalSpawner.mobsAt` uses `NetherFortressStructure.FORTRESS_ENEMIES` over nether bricks in a fortress and `ChunkGenerator.getMobsAt` for structure `spawn_overrides`. The overrides are generated (`spawn_overrides` in `crates/pumpkin-data/src/generated/structures.rs`) but `grep -rn spawn_overrides crates/pumpkin-core/src crates/pumpkin-world/src` finds no reader, and PEX keeps no structure starts or references after generation to look them up (`grep -rn '"structures"\|"References"' crates/pumpkin-world/src` finds nothing; vanilla saves them in the chunk's `structures` tag).
- `fix/attack-packet-parity` (6 commits from 2026-10-27: ignore attacks on entities that are gone, vanilla `handleAttack` checks, `can_hit` overrides, pick-item and spectate range) is not on alpha (`git merge-base --is-ancestor` fails) and merges cleanly onto 300621c3e (`git merge-tree`). Until it lands, hitting a mob in the tick it dies kicks the player.
- `Player::attack` (`core/entity/player.rs`) computes attack speed as 4.0 plus the item's `base_attack_speed` modifier, or minus 2.4 for an empty hand, and times the cooldown with `basic_config.tps`. Vanilla `Player.getCurrentItemAttackStrengthDelay` is `20 / attack_speed` from the attribute, and fists carry no modifier: 5 ticks against PEX's 12.5.
- Block entity changes are lost when the chunk unloads before the world copies them in (`World::add_block_entity_nbt` skips an unloaded chunk; `fix/flush-block-entities-before-unload` is not on alpha; options in `/srv/pex/pr-drafts/ISSUE-block-entities-lost-on-unload.md`). A run that leaves a furnace or chest and comes back can lose it.
- Mob AI the run needs (blaze, enderman, piglin, dragon phases) is Goal 1's.

Neon answered parts of this on 2026-10-29 for the earlier roadmap, and those answers are kept: the staged bot is the proof for each stage, plus one human playthrough with a real client before 2.0; accepted deviations are client-authoritative movement, no anti-cheat and the Paper-style delayed-dig tick count, and any other deviation is listed in the PR that adds it and in a deviations list beside the stages; terrain-dependent stages use a fixed seed on the shared tester server; block entity loss on unload is settled before the survival stages. The decisions below ask only what those answers left open.

### Pieces

#### G6-01 Tester server with the Nether, the End and a fixed seed

- Scope: in: `create_test_server` turns on `allow_nether` and `allow_end` and sets a fixed seed (a named constant in `harness.rs`), or a second server runs the stages that need them, per D6-2; all current suites keep passing. Out: picking structure positions (each stage does that), any stage.
- Depends on: none
- Branch: test/tester-all-dimensions
- Needs decision: D6-2
- Done when: `cargo run -p pumpkin-tester` passes with the new setup, and the PR lists tester wall time and peak RSS before and after, 3 runs each.

#### G6-02 Read a zero LpVec3 like vanilla

- Scope: in: `LpVector3d::read` reads one byte and returns zero when it is 0, like `LpVec3.read`; this fixes `SInteract::read` and the `CSpawnEntity` reader. Out: the writer, which is already right.
- Depends on: none
- Branch: fix/lpvec3-zero-read
- Needs decision: none
- Done when: a test in `codec/lp_vector_3d.rs` decodes vanilla's bytes for a zero vector followed by another field and reads that field (it fails on alpha), and the bot reads `CSpawnEntity` in full.

#### G6-03 Bot actions for the later stages

- Scope: in: `HeadlessBot` gets `look_at` (`SPlayerRotation`), `use_item` (`SUseItem`), `release_use_item` and `drop_item` (`SPlayerAction`), `swing` (`SSwingArm`) and `respawn` (`SClientCommand` `PERFORM_RESPAWN`); it reads `CRespawn` (clears chunks and entities, records the dimension), `CGameEvent` (win game, no respawn block), `CSetHealth`, and tracks every entity by id, type and position. `StageContext` gets helpers for eating, shooting a bow at a point, attacking an entity until it is gone, and waiting for a dimension. The deviations list from the earlier answer starts in the module doc of `survival/mod.rs`. Out: pathfinding, jump and fall physics.
- Depends on: G6-01, G6-02
- Branch: test/survival-bot-actions
- Needs decision: none
- Done when: new `bot_tests/survival_actions.rs` passes in `pumpkin-tester`: `eat_restores_food` (after `effect give <bot> hunger` the food level rises when the bot eats bread), `bow_release_spawns_arrow`, `kill_then_respawn` (health 0, `PERFORM_RESPAWN`, health 20 at spawn), `tracks_summoned_mob` (a summoned zombie shows up with its type and is gone after `kill`), `dimension_change` (`execute in minecraft:the_nether run tp` gives a `CRespawn` and nether chunks).

#### G6-04 Land attack packet parity

- Scope: in: rebase the existing `fix/attack-packet-parity` onto alpha and land it as it is. Out: any new check, the attack cooldown (G6-05).
- Depends on: none
- Branch: fix/attack-packet-parity (exists; no new branch)
- Needs decision: none
- Done when: its bot tests (commit `f85846200`) pass in `pumpkin-tester` on alpha, including an attack on an entity that just died not kicking the bot.

#### G6-05 Attack cooldown from the attack_speed attribute

- Scope: in: `Player::attack` reads the `attack_speed` attribute (base 4, item modifiers applied by the attribute system) and times the cooldown at 20 ticks per second like `Player.getCurrentItemAttackStrengthDelay` and `getAttackStrengthScale`, for the damage scale and the minimum attack charge check. Out: the `Used` statistic, Bedrock's cooldown.
- Depends on: G6-04
- Branch: fix/attack-cooldown-parity
- Needs decision: none
- Done when: new `bot_tests/attack_cooldown.rs` passes: a punch 5 ticks after the last attack deals the full base damage, a punch after 2 ticks deals `getAttackStrengthScale`'s reduced amount, `attribute <bot> minecraft:attack_speed base set 8` halves the delay, and changing `tps` in the config doesn't change it.

#### G6-06 Stage: iron tools

- Scope: in: `survival/iron_tools.rs`, in a staged area like stage 1's: mine coal and iron ore with a stone pickaxe and check a wooden pickaxe gets nothing from iron ore; craft and place a furnace, smelt raw iron with coal through furnace menu clicks and take the XP; craft an iron pickaxe, a bucket, a shield, and flint and steel from flint dug out of a gravel column long enough that missing flint is under 1 in 1,000 (66 blocks at 10%). Starts from stage 1's outputs as D6-1 allows. Fixes for what it finds are their own pieces. Out: food and beds (G6-07).
- Depends on: G6-03, G7-10 (block entity changes survive their chunk unloading; the earlier answer puts this before the stages)
- Branch: test/survival-iron
- Needs decision: D6-1, D6-3
- Done when: `survival:iron_tools` passes 10 of 10 runs and checks server and bot inventories the way stage 1's `check_outcome` does.

#### G6-07 Stage: food, bed and respawn

- Scope: in: `survival/food_bed_respawn.rs`: kill a summoned cow, cook the beef, eat it after `effect give hunger` has lowered the food level; shear a summoned sheep, craft and place a bed, sleep after `time set 13000` until the night skips; die by `kill`, respawn at the bed through `PERFORM_RESPAWN`; break the bed, die again and respawn at world spawn with the no-respawn-block game event. Out: hostile mobs at night.
- Depends on: G6-03
- Branch: test/survival-food-bed
- Needs decision: D6-1, D6-3
- Done when: `survival:food_bed_respawn` passes 10 of 10; food rises by cooked beef's nutrition, the night skips, and both respawn positions match vanilla `ServerPlayer.findRespawnPositionAndUseSpawnBlock`.

#### G6-08 Stage: Nether portal

- Scope: in: `survival/nether_portal.rs` on the fixed seed: fill buckets at staged water and lava sources, cast obsidian, mine it with a diamond pickaxe (from staged diamond ore and the iron pickaxe), build a 4x5 frame, light it with flint and steel, stand in it for `players_nether_portal_default_delay`, arrive in the Nether, check the portal there sits near x/8 and z/8, walk back and arrive in the first portal. Out: ruined portals, portals near the world border.
- Depends on: G6-01, G6-03
- Branch: test/survival-nether-portal
- Needs decision: D6-1, D6-3
- Done when: `survival:nether_portal` passes 10 of 10 in both directions, and a second trip reuses the Nether portal from the first (found through `World::portal_poi`) instead of building another.

#### G6-09 Mob spawner rules and SpawnData

- Scope: in: `MobSpawnerBlockEntity::tick` follows vanilla `BaseSpawner.serverTick`: no hostile spawns on Peaceful, `check_spawn_rules` with the spawner reason unless SpawnData has `custom_spawn_rules`, the SpawnData `entity` NBT applied to the spawned mob, and `Mob.checkSpawnRules` when there are no custom rules. Out: trial spawners.
- Depends on: none
- Branch: fix/spawner-spawn-rules
- Needs decision: none
- Done when: new `bot_tests/spawners.rs` passes: a zombie spawner next to a torch spawns nothing (vanilla `Monster.checkMonsterSpawnRules` only skips light for trial spawners), nothing spawns on Peaceful, a custom name in SpawnData reaches the mob, and a blaze spawner in a lit fortress room still spawns (blazes use `checkAnyLightMonsterSpawnRules`).

#### G6-10 Structure spawn overrides

- Scope: in: natural spawning picks from `FORTRESS_ENEMIES` over nether bricks inside a fortress and from a structure's `spawn_overrides` inside its pieces or box, like `NaturalSpawner.mobsAt` and `ChunkGenerator.getMobsAt`; this needs a lookup of the structure starts covering a position. Out: saving starts to chunk NBT (Goal 7).
- Depends on: G0-03, G7-07 (structure starts and references kept per chunk and saved like vanilla's `structures` tag)
- Branch: feat/structure-spawn-overrides
- Needs decision: none
- Done when: a test shows the monster list at a nether-brick position inside a `place structure minecraft:fortress` is the fortress list and outside it the biome list, a swamp hut yields witches, and G0-03's scenario shows tick P95 within the baseline's run-to-run spread.

#### G6-11 Stage: blaze rods

- Scope: in: `survival/blaze_rods.rs`: reach a fortress (`locate structure minecraft:fortress` and a teleport, if D6-1 allows), find the blaze spawner, fight with an iron sword and food, and pick up 6 blaze rods, enough for 12 eyes. Out: wither skeletons, fire resistance.
- Depends on: G6-03, G6-05, G6-09, G1-12 (blaze AI matching vanilla `Blaze`: hover, fireball volleys, targeting the bot)
- Branch: test/survival-blaze-rods
- Needs decision: D6-1, D6-3
- Done when: `survival:blaze_rods` passes 10 of 10 and every rod comes from a blaze the bot killed (`entities/blaze` drops rods only for player kills).

#### G6-12 Piglin bartering from the loot table

- Scope: in: `PiglinAi::get_barter_response_items` rolls `GAMEPLAY_PIGLIN_BARTERING` through the loot code with the piglin as `this_entity`, like vanilla `PiglinAi.getBarterResponseItems`; the hard-coded table goes. Out: admiring and pickup behaviour (Goal 1).
- Depends on: none
- Branch: fix/piglin-barter-loot-table
- Needs decision: D6-4
- Done when: a test bartering until it gets one shows the enchanted book carries Soul Speed and the potions Fire Resistance (both fail on alpha).

#### G6-13 Stage: ender pearls

- Scope: in: `survival/ender_pearls.rs`: collect 12 pearls by the route D6-4 picks: drop gold ingots near a piglin and pick up what it throws, or kill endermen. Out: Looting.
- Depends on: G6-03, G6-05, G6-12 (if bartering), G1-30 (piglin admiring and bartering, if bartering), G1-10 (enderman AI, if endermen)
- Branch: test/survival-ender-pearls
- Needs decision: D6-1, D6-3, D6-4
- Done when: `survival:ender_pearls` passes 10 of 10 and every pearl came from a barter or a kill.

#### G6-14 Eye of ender and portal frame like vanilla

- Scope: in: `EnderEyeItem::use_on_block` adds `Block.pushEntitiesUp`, the comparator update and the `destroyBlock` of each portal position, and finds the frame with vanilla's pattern from `EndPortalFrameBlock.getOrCreatePortalShape`; `normal_use` waits for the stronghold layout like `getRingPositionsFor` instead of not throwing. Out: eye flight (already ported).
- Depends on: G6-03
- Branch: fix/ender-eye-parity
- Needs decision: none
- Done when: new `bot_tests/end_portal_frame.rs` passes: a comparator next to a filled frame reads 15, a block inside the ring drops as an item when the last eye goes in, a bot standing on the frame gets a teleport up onto the eye (vanilla `Block.pushEntitiesUp`), and an eye thrown right after startup flies instead of doing nothing.

#### G6-15 Stage: stronghold

- Scope: in: `survival/stronghold.rs` on the fixed seed: craft blaze powder and 12 eyes, throw one and check it flies toward the position `locate structure minecraft:stronghold` reports, reach the stronghold (teleport if D6-1 allows), find the portal room, fill the empty frames, check the portal appears with the last eye and the `end_portal_spawn` world event. Out: following eyes across the map, silverfish.
- Depends on: G6-01, G6-03, G6-14
- Branch: test/survival-stronghold
- Needs decision: D6-1, D6-3
- Done when: `survival:stronghold` passes 10 of 10 and the portal is absent until the last frame is filled.

#### G6-16 Dragon damage through its parts

- Scope: in: part ids resolve for attacks, interactions and projectiles like `ServerLevel.getEntityOrPart` and `Level.getEntities`; `EnderDragon.hurt(part, ...)` with the body divisor, the player or `always_hurts_ender_dragons` gate and sitting damage leading to takeoff; 10 head damage when the healing crystal is destroyed. Out: each phase's `onHurt` and its reaction to a destroyed crystal (Goal 1).
- Depends on: G6-01, G6-04
- Branch: fix/dragon-damage-parts
- Needs decision: none
- Done when: new `bot_tests/dragon_damage.rs` passes in the End: a hit on the head id lowers health by the full damage, a body hit by `damage / 4 + min(damage, 1)`, the bot isn't kicked, an arrow hits a part, `damage` without a player source leaves health alone, and destroying the healing crystal costs 10.

#### G6-17 Dragon death sequence

- Scope: in: a lethal hit keeps the dragon alive at 1 health in `DYING` like `handleKillingBlow`; the death runs as vanilla `tickDeath` (200 ticks, 12,000 XP on the first kill and 500 after, the `mobDrops` rule, global event 1028) without the generic 20-tick removal; `kill` on the dragon calls `set_dragon_killed` like `EnderDragon.kill`. Out: phase movement during death (Goal 1).
- Depends on: G6-01
- Branch: fix/dragon-death-sequence
- Needs decision: none
- Done when: new `bot_tests/dragon_death.rs` passes: after `damage <dragon> 1000 minecraft:player_attack by <bot>` the dragon exists for 200 ticks, then the exit portal holds `end_portal` blocks, the egg sits on the podium, one gateway exists and the XP orbs add up to 12,000; the same test with `kill` instead of `damage`, on a fresh world, opens the portal at once.

#### G6-18 Credits and respawn after the End

- Scope: in: the End portal in the End shows credits like `ServerPlayer.showEndCredits` while `seen_credits` is false, keeps the player out of the world until `PERFORM_RESPAWN`, then respawns it like `PlayerList.respawn(player, true)` at the bed or respawn point with its inventory; later exits go to the respawn point without credits; `handle_client_status` handles the won-game case. Check the `WIN_GAME` value against a real 26.3 client with the play-test rig. Out: Bedrock's `CShowCredits` beyond keeping it working.
- Depends on: G6-01, G6-03
- Branch: fix/end-credits-respawn
- Needs decision: none
- Done when: new `bot_tests/end_credits.rs` passes: a bot with a bed spawn walks into an End portal in the End, gets `WinGame` once, comes back at the bed after `PERFORM_RESPAWN` with its items, and a second exit gives no credits; plus a play-test capture of the credits screen.

#### G6-19 Dragon fight saved data

- Scope: in: save and load `DragonFight` as `data/minecraft/ender_dragon_fight.dat` with vanilla's field names (`needs_state_scanning`, `dragon_killed`, `previously_killed`, `respawn_stage`, `respawn_time`, `dragon_uuid`, `exit_portal_location`, `gateways`, `respawn_crystals`); the dragon is saved with its chunk instead of `spawn_entity_non_save`. Out: the general `data/` round trip with vanilla (Goal 7).
- Depends on: G6-17, G1-27 (dragon phases with vanilla ids and `DragonPhase` NBT)
- Branch: fix/dragon-fight-saved-data
- Needs decision: none
- Done when: a test in `core/world/dragon_fight.rs` reads an `ender_dragon_fight.dat` written by vanilla 26.3 and writes the same fields back; a boot test (kill the dragon, `stop`, start) finds no new dragon and the same gateways; a fight stopped mid-way comes back with the same dragon and health.

#### G6-20 Stage: the End and the dragon

- Scope: in: `survival/dragon_fight.rs`: enter the portal, land on the platform facing west, reach the island, craft a bow and arrows, shoot the crystals, hit the dragon's head while it perches, wait out the death, check the exit portal, egg and gateway, walk into the exit portal, take the credits, respawn at the bed with the inventory. Out: respawning the dragon, End cities, elytra.
- Depends on: G6-03, G6-05, G6-16, G6-17, G6-18, G1-27 (dragon phases with vanilla ids, `DATA_PHASE` sync and the `dragon_fireball` entity, per D6-5)
- Branch: test/survival-dragon
- Needs decision: D6-1, D6-3, D6-5
- Done when: `survival:dragon_fight` passes 5 of 5 with ticks per run in the PR, and all dragon damage came from bot attacks, arrows and crystal explosions.

#### G6-21 Survival mechanics on vanilla through dual-run

- Scope: in: structures beside `tools/dual-run/structures/redstone/` for water cast on lava (obsidian), a hopper-fed furnace smelting raw iron with coal, and a dispenser lighting an obsidian frame with flint and steel, each with its expected outcome. Out: anything with mobs or other dimensions.
- Depends on: G0-02
- Branch: test/survival-dual-run
- Needs decision: none
- Done when: `dual_run.py` passes the 3 structures on vanilla 26.3 and on PEX, and a scratch change to one expected outcome makes it exit 1.

#### G6-22 Full chained run

- Scope: in: `survival/full_run.rs` runs the stages in order on one world and seed with no `give` after the start and teleports only where D6-1 allows, logging ticks and wall time per stage; outside CI per D6-3. Out: new mechanics.
- Depends on: G6-06, G6-07, G6-08, G6-11, G6-13, G6-15, G6-20, G7-10 (block entity changes survive their chunk unloading)
- Branch: test/survival-full-run
- Needs decision: D6-1, D6-3
- Done when: three runs in a row reach the credits and respawn at the bed with no kick, panic or new `ERROR` log line, and the PR has the per-stage tick and time table.

### Proof

- Every stage passes in `pumpkin-tester`: `survival:iron_tools`, `food_bed_respawn`, `nether_portal`, `blaze_rods`, `ender_pearls` and `stronghold` 10 of 10, `dragon_fight` 5 of 5, in CI (once G0-01 runs it) or nightly as D6-3 decides.
- `survival:full_run` reaches the credits three times in a row on one seed, with its per-stage table.
- G6-21's survival structures pass on vanilla and PEX.
- The deviations list in `survival/mod.rs` holds only the accepted deviations.
- One human playthrough with a real client before 2.0 (the earlier answer), with the play-test rig's capture: portal lighting and travel, eye flight, crystal beams, the dragon perching and breathing, its death beam, the exit portal, the credits screen and the respawn.

### Decisions

- D6-1: What may a stage set up with commands? The earlier answer made the staged bot the proof but didn't say. Options:
  - Each stage starts from `give` of the earlier stages' outputs and may teleport to a structure `locate` reports; only G6-22 chains without `give`. Stages stay short and independent; only the full run proves the outputs chain in one world.
  - No `give`: each stage replays the earlier ones in its own area. Strongest per stage, but stage N costs stages 1 to N and one failure fails everything after it.
  - `give` and teleports everywhere, the full run included. Cheapest, and nothing proves the run works in one world.
  - Waits on it: G6-06, G6-07, G6-08, G6-11, G6-13, G6-15, G6-20, G6-22.
  - Partly answered 2026-10-31 (Neon accepted Q6.1): the staged bot is the proof for every stage, plus one human playthrough with a real client before the 2.0 release. Still open: which of the three setups a stage may use.
  - Answered 2026-10-31 (Neon): the first option. Each stage starts from `give` of the earlier stages' outputs and may teleport to a structure `locate` reports; only G6-22 chains without `give`.
- D6-2: Where do the stages that need the Nether, the End and the fixed seed run? The earlier answer chose the shared tester server with a fixed seed; it didn't cover turning on both dimensions there. Options:
  - Shared server, both dimensions on: one startup, but every tester run loads three dimensions (G6-01 measures the cost).
  - A second server in the same tester process for those stages: the bot tests and GameTests keep today's server; more harness code and more RAM while it runs.
  - Waits on it: G6-01, and through it every stage from G6-08 on.
  - Answered 2026-10-31 (Neon accepted Q6.4): the shared tester server with a fixed seed and both dimensions on, and a separate server only for a stage that conflicts with the others. G6-01 measures what loading three dimensions costs.
- D6-3: Which stages run in CI's `gametest` job? Stage 1 alone takes 9 to 86 s. Options:
  - All stages: a regression shows on the push that caused it; the job grows by minutes on whatever runner D0-1 picks.
  - The short ones (iron, food, portal) in CI; blaze rods, pearls, stronghold, dragon and the full run in `nightly.yml` or by hand.
  - Only stage 1 in CI; the rest by hand before a release.
  - Waits on it: G6-06, G6-07, G6-08, G6-11, G6-13, G6-15, G6-20, G6-22.
  - Answered 2026-10-31 (Neon): the short stages (G6-06 iron, G6-07 food, G6-08 portal) run in CI's `gametest` job. Blaze rods, pearls, stronghold, the dragon and the full run (G6-11, G6-13, G6-15, G6-20, G6-22) run nightly, in G0-05's slot until Actions is back, and before every release.
- D6-4: Where do the 12 pearls come from? Options:
  - Bartering: needs G6-12 and Goal 1's piglin work; at the table's weight (10 of 459, 2 to 4 pearls) about 180 gold ingots.
  - Endermen: needs Goal 1's enderman work; 0 or 1 pearl per kill without Looting, about 24 kills.
  - Both, as players do: both Goal 1 dependencies, and both mechanics get tested.
  - Waits on it: G6-12, G6-13.
- D6-5: Does the dragon stage wait for Goal 1's dragon phases? Options:
  - Wait: the stage proves the vanilla fight (perching, breath, fireballs), but G6-20 and G6-22 block on Goal 1.
  - Run it on the current phase machine with the phase gaps in the deviations list, and rerun when Goal 1 lands.
  - Waits on it: G6-20, G6-22.
- D6-6: Does anything off the start-to-dragon path belong in Goal 6 (enchanting, brewing, anvils, trading, respawn anchors)? Options:
  - No: Goal 6 is the run; those systems go to a later goal.
  - Yes: one audit piece and one stage per system. More pieces and CI time.
  - Waits on it: nothing now; a yes adds pieces.

## Goal 7: Worlds round-trip with vanilla 26.3

Worlds round-trip with vanilla 26.3.

Start from: what PEX reads and writes (region/chunk NBT, data version, level.dat, playerdata, entities/, poi/, data/), and what's lost going PEX -> vanilla -> PEX.

### Current state

Paths in this section: `pw/` is `crates/pumpkin-world/src/`, `core/` is `crates/pumpkin-core/src/`, `cfg/` is `crates/pumpkin-config/src/`, `pd/` is `crates/pumpkin-data/src/`. Vanilla classes are cited by name from the decompiled 26.3 source in `/srv/pex/vanilla-26.3/src/` (world data version 5023).

PEX opens vanilla 26.3 worlds. The save batch from the earlier roadmap is on alpha: `fix/load-vanilla-26-3-worlds`, `fix/level-dat-and-save-all`, `fix/player-data-atomic-save`, `fix/region-sector-allocator`, `fix/save-block-entities-on-stop` and `fix/json-list-load-safety` are ancestors of `300621c3e` (`git merge-base --is-ancestor`). Not on alpha: `fix/flush-block-entities-before-unload`, `fix/block-entity-bookkeeping`, `perf/region-write-in-place`. Nothing has ever opened a PEX save in vanilla: `grep -rln "server-26.3\|bundler-26.3" tools .github /srv/pex/tools` finds nothing, and `tools/dual-run/dual_run.py` only builds its own structures. So everything below about vanilla reading PEX files is read from code, not run.

Neon answered three questions on 2026-10-29 for the earlier roadmap (`git -C /srv/pex/repo show origin/roadmap-2.0:docs/ROADMAP-2.0.md`, Q7.1 to Q7.3), and those answers are kept: Anvil is the default for new configs, existing configs keep the format they name and are not converted, and Pump stays an opt-in; the fixture world stays out of the repository (a script regenerates it, or it is a release asset); conversion moves source files to a backup folder and never deletes them. The decisions below ask only what those answers left open.

**Two things stop any round trip today.**
- The default format is Pump. `ChunkConfig` (`cfg/chunk.rs`) has `#[default] Pump`. `Level::from_root_folder` (`pw/level.rs`) calls `convert_folder(..., true)` on `region/` and `entities/` at every start, and `convert_into_dst` (`pw/chunk/io/converter.rs`) then deletes the `.mca` files and their external `.mcc` chunks. `--convert-world` (`crates/pumpkin/src/main.rs`) also passes `true`. Release configs name their format: `type = "anvil"` in `crates/pumpkin-config/tests/fixtures/pumpkin-v1.0.0.toml` and `-v1.1.0.toml`, `type = "pump"` in `-v1.2.0.toml`.
- Anvil block palettes use the old field names. `ChunkData::internal_to_bytes` (`pw/chunk/format/mod.rs`) writes each palette entry as `{Name, Properties}` and stamps `DataVersion` 5023 (`WORLD_DATA_VERSION`, `pw/chunk/format/anvil.rs`). Vanilla renamed them at data version 5006 (`BlockStateFieldNamesFix` in `DataFixers`), and 26.3 `BlockState.CODEC` takes a bare id string or `{id, properties}`. With a current `DataVersion` no fixer runs. `PalettedContainer.codec` swaps an entry it can't read for the default (`ExtraCodecs.orElsePartial`), so vanilla should load PEX-written blocks as air, or regenerate the chunk through `ChunkMap.handleChunkLoadFailure` where the section fails as a whole. G7-04 confirms which.

**What each file keeps.**

| Files | PEX reads | PEX writes | Lost or changed |
|---|---|---|---|
| `region/*.mca` chunks | Anvil, Linear or Pump per config. `internal_from_bytes`: positions, sections (light, `block_states` as bare id or `{id or Name, properties or Properties}` via `extract_u16_array`, biomes), heightmaps, `block_ticks`, `fluid_ticks`, `block_entities`, `isLightOn`, `Status`, `InhabitedTime`, `PumpkinCustomData` or `BukkitValues`. `DataVersion` is ignored. | The same keys plus `DataVersion` 5023 (`internal_to_bytes`); `.mca` only with `type = "anvil"`. | Palettes as above. Never read or written (`grep -rn` for each name in `pw/` and `core/` outside `generated/`): `structures` (starts and `References`), `PostProcessing`, `blending_data`, `below_zero_retrogen`, `UpgradeData`, `LastUpdate`, proto-chunk `entities`. Tick `t` is cast `as u8` (`parse_scheduled_tick`; `ScheduledTick::from_nbt_compound` and `ScheduledTick.delay`, `MAX_TICK_DELAY = 1 << 8`, `pw/tick/mod.rs`), so 300 becomes 44 and an overdue `-1` becomes 255; vanilla `SavedTick` keeps an int. Because of this `HATCH_DELAY` is `u8::MAX` (`core/block/blocks/frogspawn.rs`) where vanilla `FrogspawnBlock` uses 3600 to 12000, and `get_hatch_delay` returns `u8` (`sniffer_egg.rs`). A biome name PEX doesn't know reads as id 0 (`extract_u8_array`) and is written back as that biome. |
| `entities/*.mca` | `ChunkEntityData`: `Position` (or `Position-X`), `Entities`. | `DataVersion`, `Position`, `Entities`. | Live entities are rewritten by their own `write_nbt`. Base keys (`Entity::write_nbt`, `core/entity/mod.rs`): `Air`, `CustomName`, `CustomNameVisible`, `Fire`, `HasVisualFire`, `id`, `Invulnerable`, `leash`, `Motion`, `OnGround`, `PortalCooldown`, `Pos`, `PumpkinCustomData`, `Rotation`, `Tags`, `TicksFrozen`, `UUID`. `NoGravity`, `Silent` and `Glowing` appear only in `core/entity/bucketable.rs`; `Passengers` and `attributes` nowhere in `core/entity/`. `DRAGON_FIREBALL`, `FISHING_BOBBER`, `MANNEQUIN`, `OMINOUS_ITEM_SPAWNER` and `ZOMBIE_NAUTILUS` have no arm in `from_type` (`core/entity/type.rs`) and load as a plain `Entity` or `LivingEntity`. In a live chunk the saved list is replaced by the live entities (`merge_entity_records`, `core/world/mod.rs`), so a record that didn't spawn (unknown id: warn "Entity has no valid Entity Type") is dropped. Per-type fields not audited. |
| Block entities (chunk `block_entities`) | `block_entity_from_nbt` (`core/block/entities/mod.rs`) for all 49 vanilla ids plus `minecraft:bed`. Untouched ones stay as loaded NBT (`get_block_entity` clones `pending_block_entities`). | Live ones via `write_internal` plus `PumpkinCustomData` (`save_block_entities`). | A woken block entity keeps only what its type writes; per type not audited. PEX saves beds as `minecraft:bed` (`core/block/entities/bed.rs`, for Bedrock block actors); 26.3 beds have no block entity (`BedBlock`), and vanilla logs "Skipping block entity with invalid type" (`BlockEntity.loadStatic`). Changes are lost when the chunk unloads before the World copies them in (`World::add_block_entity_nbt` skips a chunk that isn't loaded; `/srv/pex/pr-drafts/ISSUE-block-entities-lost-on-unload.md`). |
| Item stacks (containers, entities, players) | `read_item_stack` (`pd/item_stack/mod.rs`) | `write_item_stack` | One component name `DataComponent::try_from_name` doesn't know, or data `read_data` (`pd/data_component_impl/mod.rs`, 122 components) can't parse, drops the whole stack (both behind `?`). |
| `poi/*.mca` | `PoiRegion::load` (`pw/poi/mod.rs`): compression 2 (zlib) only, no `.mcc`; records `x`, `y`, `z` (each `unwrap_or(0)`), `type`, `free_tickets`. | Same keys plus `Valid`, `Records`, `Sections` and `DataVersion` 3955 (`DATA_VERSION`). `PoiRegion::save` rewrites the whole region with `File::create` (not atomic) and deletes an empty one. | Vanilla `PoiRecord.Packed` stores `pos` as an int array, so PEX puts every vanilla record at 0,0,0 and vanilla can't read PEX's. PEX knows 13 job sites (`poi_type_for_block`, `core/world/villager_poi.rs`) and nether portals out of vanilla's 21 (`PoiTypes`). POIs are added only from `set_block_state` (`core/world/mod.rs`) and never rebuilt from blocks (vanilla `PoiManager.checkConsistencyWithBlocks`); nether portal search uses only POIs (`get_in_square`, `core/world/portal/nether.rs`), so portals in a vanilla world aren't found. |
| `level.dat` | `read_level_dat_with_fallback`, `level_data_from_nbt` (`pw/world_info/anvil.rs`), data versions 4435 to 5023 (`pw/world_info/mod.rs`). | `level_data_to_nbt` merged into the existing root (`existing_level_dat_root`), `write_and_replace`. | `difficulty_settings.hardcore` is always `false` (`diff_comp.put_bool("hardcore", false)`) and vanilla `LevelSettings.parse` reads hardcore there, so a hardcore world stops being one. `Time` and `GameType` are not written: a vanilla world keeps its old values through the merge, a world PEX made has none. |
| `data/minecraft/game_rules.dat`, `world_gen_settings.dat`, `world_clocks.dat` | `read_game_rules`, `read_world_gen_settings`, `read_world_clocks` (`pw/world_info/data_files.rs`) | `game_rules_to_nbt`, `write_world_gen_settings`, `write_world_clocks` | `DataVersion` is written inside `data`. Vanilla `SavedDataStorage` reads it from the root, takes a missing one as 1343 (`NbtUtils.getDataVersion(tag, 1343)`) and runs every fixer from there; effect not run. `generate_structures` is hard-coded `true` and `bonus_chest` `false`. Only `total_ticks` of the overworld clock is written; `partial_tick`, `rate`, `paused` (`ClockState`) and other clocks are dropped. |
| `data/minecraft/weather.dat`, `wandering_trader.dat` | `read_weather` | `write_weather`, wandering trader, both with a root `DataVersion` | `write_world_info` writes back the weather it read plus `clear_weather_time`; live weather (`core/world/weather.rs`) is neither saved nor restored. Time likewise: `World::load` builds `LevelTime::new()`, `LevelTime::load_from` (`core/world/time.rs`) has no caller (`grep -rn "\.load_from("` finds only `OperatorConfig` in `core/data/mod.rs`), and `level_info.day_time` is only read in `core/` (`explosion.rs`, `living.rs`), so the clock PEX saves is the one it loaded. |
| `data/minecraft/scoreboard.dat`, `custom_boss_events.dat`, `scheduled_events.dat`, `random_sequences.dat`, `stopwatches.dat` | No | Empty stubs, only when missing (`write_scoreboard_stub` and the other `write_*_stub`, `data_files.rs`) | A vanilla file passes through untouched but PEX ignores it. What `/scoreboard`, `/team`, `/bossbar`, `/schedule`, `/random` and `/stopwatch` change in PEX (`core/command/commands/`) is lost on restart. |
| `data/minecraft/maps/<id>.dat`, `maps/last_id.dat`, `data/<ns>/command_storage.dat` | No | No | `MapManager` (`core/world/map.rs`) is a `DashMap` and `command_storage` a `HashMap` on `Server` (`core/server/mod.rs`). `next_map_id` keeps the counter in `LevelData.map_id`, which vanilla doesn't read. `grep -rn 'last_id\|"maps"\|command_storage' crates/pumpkin-core/src crates/pumpkin-world/src` finds no file I/O. Vanilla's path for command storage: `DimensionStorageFileFix`. |
| `dimensions/<ns>/<dim>/data/minecraft/`: `world_border.dat`, `chunk_tickets.dat`, `raids.dat`, `ender_dragon_fight.dat` (End) | No | No | Each start, `World::load` builds `Worldborder::new(0, 0, 5.9999968E7, 0, 5, 300)`, an empty forced-chunk set, `DragonFight::new()` and no raids; `border.rs`, `raid.rs`, `dragon_fight.rs` in `core/world/` have no file I/O. Vanilla's folder: `ServerChunkCache` data folder, `LevelDatToSavedDataFileFix` (`forDataFileInDimension("the_end", "minecraft/ender_dragon_fight.dat")`). The dragon fight file is Goal 6's G6-19. |
| `players/data/<uuid>.dat` | Player `read_nbt` | `NbtCompound::new()` filled by `Player::write_nbt` (`core/data/player_server.rs`, `core/entity/player.rs`), atomic with `.dat_old` (`pw/data/player_data.rs`) | Keys vanilla `ServerPlayer` saves that PEX neither reads nor writes: `recipeBook`, `ender_pearls`, `LastDeathLocation`, `ShoulderEntityLeft`, `ShoulderEntityRight`, `entered_nether_pos`, `raid_omen_position`, `post_effects`, `last_explosion_impact_pos`. PEX keeps its statistics in this file (`Statistics`, `core/entity/player/statistics.rs`). |
| `players/advancements/<uuid>.json` | `PlayerAdvancement::load` (`core/entity/player/advancement.rs`) | `PlayerAdvancement::save`, in place with `tokio::fs::write` | PEX writes `{advancement: {criterion: SystemTime}}` in serde's `{secs_since_epoch, nanos_since_epoch}` form, done criteria only (`AdvancementProgress` `Serialize`). Vanilla writes `{advancement: {criteria: {criterion: "yyyy-MM-dd HH:mm:ss Z"}, done}, DataVersion}`. PEX fails to parse a vanilla file (warn "Error loading player", `Server::add_player`) and later saves over it. |
| `players/stats/<uuid>.json` | No; only renamed by the offline UUID migration (`PLAYER_FILES`) | No | Vanilla's file stays as it was; PEX's progress never reaches it. |

**Other gaps.**
- Unreadable chunks are regenerated silently. `fetch_chunks` (`pw/chunk/io/file_manager.rs`) turns a decode or parse error into `LoadedData::Error` without a log line, and `io_read_work` (`pw/chunk_system/worker_logic.rs`) answers it with `ProtoChunk::new`, which is later saved over the old data. Vanilla regenerates too, but logs it (`ChunkMap.reportChunkLoadFailure`).
- Older worlds are restamped without fixes. Chunk reading ignores `DataVersion` and every save writes 5023, so a 4435 to 5022 chunk is saved as current without the 26.x fixers (`DataFixers` 4996 to 5016, among them `PotDecorationsComponentUnflatteningFix`, `SignsOpFlagFix`, `BlockStateFieldNamesFix`, `SwingAnimationComponentSplitFix`, `ExplorerMapItemFix`, `MapColorComponentRemovalFix`, `BlendingDataNoValueHeightFix`, `MergeTerrainChunkStatusFix`, `RemoveBlockTransformerComponentFix`, `UnfilledBuriedTreasureMapFix`), and vanilla never applies them afterwards.
- No `session.lock`. `git grep '"session.lock"' crates` finds nothing; vanilla holds one (`LevelResource.LOCK_FILE`, `DirectoryLock` in `LevelStorageSource`). Nothing stops PEX and vanilla writing one folder at once.
- Plugin data. `PumpkinCustomData` on chunks, entities, block entities and items is exposed to plugins (`set-custom-data` in `crates/pumpkin-plugin-wit/v0.2/block-entity.wit`, `item-stack.wit`, `world.wit`). Vanilla's chunk, entity and block entity codecs read named fields only, so these keys should not survive a vanilla save. The world-level file is shared by mistake: `World::load` and `World::save` (`core/world/mod.rs`) use `level_folder.root_folder.join("pumpkin_custom_data.nbt")`, and `root_folder` is the world root for every dimension (`LevelFolder`, `pw/level.rs`), so the last dimension saved wins.
- Tests and tools. In-repo tests build NBT by hand: `extract_u16_array_from_vanilla_compound_palette` and `extract_u16_array_from_26_3_string_palette` (`pw/chunk/format/mod.rs`), `spawn_and_difficulty_format_26_3` and `all_26_3_minecraft_data_files_written_and_read` (`pw/world_info/anvil.rs`). None checks where `DataVersion` sits or that vanilla reads the output. No world is committed (`git ls-files | grep -c '\.mca$'` is 0). Outside the repo: `/srv/pex/tools/mca_check.py` (header and zlib check), `mca_blocks.py`, `mca_blockdiff.py` (palette by `p.get("Name")` only, so it can't read 26.3 palettes), `leveldat_version.py`. Vanilla runs with `/srv/pex/jdk-25/bin/java -jar /srv/pex/vanilla-26.3/bundler-26.3.jar nogui`.

### Pieces

#### G7-01 Vanilla 26.3 fixture world

- Scope: in: a script that builds a world on vanilla 26.3 with a fixed seed from nothing: every block entity type with contents, item stacks carrying many components, every summonable entity with names, passengers, leashes and changed attributes, frogspawn and long block ticks, POIs of all 21 types (village, portals, beehive, lodestone, lightning rod), a hardcore copy, scoreboard objectives and teams, maps, boss bars, `/schedule`, command storage, a moved world border, `/forceload`, a raid; a bot (`/srv/pex/tools/survbot.py`) joins so `players/` has data, advancements and stats. Stored as Q7.2 says. Out: committing the world.
- Depends on: none
- Branch: test/world-fixture-26-3
- Needs decision: none
- Done when: the script rebuilds the world from an empty folder, `mca_check.py` passes on it, and its manifest names at least one entry for every row of the table above.

#### G7-02 World tree diff

- Scope: in: a tool that compares two world folders file by file and key by key: chunks, entity chunks and POI sections matched by position, `level.dat`, every `data/` file, player files and JSON. An ignore list with a reason per entry (timestamps such as `LastPlayed`). Block palettes compared in both forms. Out: fixing what it finds.
- Depends on: none
- Branch: test/nbt-tree-diff
- Needs decision: none
- Done when: two copies of G7-01's world compare equal, and a scratch change to one chest item shows as one diff with its full NBT path.

#### G7-03 PEX round trip of the fixture

- Scope: in: a test that copies G7-01's world, starts PEX with `type = "anvil"`, loads every fixture chunk, wakes every block entity and makes every entity chunk live, runs `save-all` and `stop`, and diffs the result against the fixture with G7-02. It prints a loss report; at first it fails only on diffs not in a known-loss list, and each fix piece shrinks that list. Out: vanilla.
- Depends on: G7-01, G7-02, G0-01
- Branch: test/world-roundtrip-pex
- Needs decision: none
- Done when: it runs in CI and the PR holds its first loss report.

#### G7-04 Vanilla opens a PEX save

- Scope: in: vanilla 26.3 starts on G7-03's output and on a fresh PEX-generated world, force-loads the fixture area, saves and stops; the test greps its log for `Failed to parse saved data`, `Skipping block entity with invalid type` and `ChunkMap.reportChunkLoadFailure` lines, diffs vanilla's save against PEX's, then loads vanilla's save in PEX again and diffs that against PEX's first load. Out: fixes.
- Depends on: G7-03, G0-02
- Branch: test/world-roundtrip-vanilla
- Needs decision: none
- Done when: it runs in CI and its first report says, for palettes, POI and the `data/` files, what vanilla actually did with them.

#### G7-05 Block palettes in 26.3 form

- Scope: in: `internal_to_bytes` writes palette entries the way `BlockState.CODEC` does (bare id for a default state, `{id, properties}` otherwise). Reading keeps both forms. Out: other chunk keys.
- Depends on: none
- Branch: fix/palette-26-3-ids
- Needs decision: none
- Done when: a unit test checks the written tags for a default and a non-default state, and G7-04 shows no block change in chunks PEX wrote.

#### G7-06 Keep chunk keys PEX doesn't model

- Scope: in: as D7-1 says, `structures`, `PostProcessing`, `blending_data`, `below_zero_retrogen`, `UpgradeData`, `LastUpdate`, proto-chunk `entities` and other unknown top-level keys survive load and save. Out: making structure starts (G7-07).
- Depends on: none
- Branch: fix/chunk-passthrough-keys
- Needs decision: D7-1
- Done when: G7-03 shows these keys unchanged in every fixture chunk.

#### G7-07 Structure starts and references for chunks PEX generates

- Scope: in: generation records each structure start and its references, saves them in `structures` like `SerializableChunkData` and `StructureStart`, and keeps them for lookups by position. Out: using them for spawn overrides (G6-10).
- Depends on: G7-06
- Branch: feat/chunk-structure-starts
- Needs decision: D7-3
- Done when: in a world PEX generated, vanilla's `/locate structure minecraft:village_plains` and PEX's agree on an already generated village, and G7-04 logs no structure errors. G6-10 waits on this.

#### G7-08 Log unreadable chunks

- Scope: in: `fetch_chunks` and `io_read_work` log the region file, chunk position and error before regenerating, like `ChunkMap.reportChunkLoadFailure`. Out: changing what happens next.
- Depends on: none
- Branch: fix/log-unreadable-chunks
- Needs decision: none
- Done when: a unit test with a corrupt chunk payload sees one error line naming the chunk.

#### G7-09 Scheduled ticks longer than 255

- Scope: in: saved `t` keeps its int value through `parse_scheduled_tick` and `ScheduledTick::from_nbt_compound`, either with a wider `delay` or an overflow list beside the 256-slot ring; overdue ticks run first. Frogspawn and sniffer eggs use vanilla's delays. Out: tick order inside a tick (G5-04).
- Depends on: G5-04 (it edits `pw/tick/scheduler.rs` too, so it lands first)
- Branch: fix/wide-tick-delays
- Needs decision: none
- Done when: a unit test loads ticks with `t = 300` and `t = -5`, runs the overdue one on the first tick and saves the other with its remaining delay; a frogspawn hatches between 3600 and 12000 ticks.

#### G7-10 Block entity changes survive their chunk unloading

- Scope: in: what D7-6 picks. For the flush option, rebase `fix/flush-block-entities-before-unload` (`870f8ce5c`, a `before_chunk_unload` hook from the chunk scheduler); `git merge-tree` shows conflicts with alpha in `core/world/mod.rs` and `pw/chunk_system/schedule.rs`. Out: per-type NBT keys (G7-25).
- Depends on: none
- Branch: fix/flush-block-entities-before-unload (exists; no new branch)
- Needs decision: D7-6
- Done when: the branch's `unloading_a_chunk_writes_its_live_block_entities_into_it` passes on alpha, and a bot test (hopper line into a chest, bot leaves, chunk unloads, `stop`) finds the items in the saved chest. G6-06 and G6-22 wait on this.

#### G7-11 POI files in vanilla's format

- Scope: in: records read and written with `pos` (old `x`, `y`, `z` still read), every region compression and `.mcc` through the chunk region code, `DataVersion` 5023, atomic writes like `write_file_atomically`. Out: rebuilding (G7-12).
- Depends on: none
- Branch: fix/poi-vanilla-format
- Needs decision: none
- Done when: a unit test reads a POI region from G7-01's world with the right positions, and G7-04 shows vanilla reading PEX's POI unchanged.

#### G7-12 POIs rebuilt from blocks

- Scope: in: all 21 `PoiTypes` mapped from block states (whether `assets/` already has the mapping was not checked), sections rebuilt on chunk load when missing or not `Valid` like `PoiManager.checkConsistencyWithBlocks`, adds and removes on every block change. Out: villager use of the new types (Goal 1).
- Depends on: G7-11
- Branch: feat/poi-from-blocks
- Needs decision: none
- Done when: G7-01's world with `poi/` deleted gets the same records in PEX as in vanilla (G7-02), and PEX's portal search finds a portal built in vanilla.

#### G7-13 level.dat and data files vanilla reads

- Scope: in: `difficulty_settings.hardcore` kept from the world, `Time` and `GameType` written, `DataVersion` at the root of every saved data file, `generate_structures` and `bonus_chest` kept as read, every clock and `ClockState` field kept. Out: live time and weather (G7-14).
- Depends on: none
- Branch: fix/level-data-26-3
- Needs decision: none
- Done when: unit tests per file check the root `DataVersion` and the kept fields; the hardcore fixture is still hardcore in vanilla after a PEX save; G7-04 logs no `Failed to parse saved data`.

#### G7-14 Time and weather saved and restored

- Scope: in: `World::load` restores `LevelTime` from `world_clocks.dat`, saves write the live time, and weather state goes to and from `weather.dat`. Out: clocks other than the overworld's beyond what G7-13 keeps.
- Depends on: G7-13
- Branch: fix/persist-time-weather
- Needs decision: none
- Done when: a boot test runs `time set 6000` and `weather thunder 3000`, restarts and finds both; vanilla on that save shows the same time and weather.

#### G7-15 Scoreboard saved data

- Scope: in: `scoreboard.dat` read and written with `ScoreboardSaveData`'s fields (objectives, scores, teams, display slots). Out: scoreboard behaviour.
- Depends on: none
- Branch: feat/scoreboard-saved-data
- Needs decision: D7-4
- Done when: objectives, scores and teams from G7-01's world survive PEX and come back in vanilla, and ones made in PEX survive a restart.

#### G7-16 Map saved data

- Scope: in: `maps/<id>.dat` and `maps/last_id.dat` read and written; new ids come from `last_id` instead of `LevelData.map_id`. Out: map rendering changes.
- Depends on: none
- Branch: feat/map-saved-data
- Needs decision: D7-4
- Done when: a fixture map shows its picture in PEX, and a map made in PEX shows in vanilla without reusing an existing id.

#### G7-17 World border and forced chunks

- Scope: in: per-dimension `world_border.dat` and `chunk_tickets.dat` read and written; `/forceload` chunks are loaded again on start. Out: border behaviour.
- Depends on: none
- Branch: feat/border-tickets-saved-data
- Needs decision: D7-4
- Done when: a border and forced chunks set in vanilla are in place in PEX and the reverse, each after a restart.

#### G7-18 Raids saved data

- Scope: in: per-dimension `raids.dat` read and written for raids in `core/world/raid.rs`. Out: raid AI.
- Depends on: none
- Branch: feat/raids-saved-data
- Needs decision: D7-4
- Done when: a raid running at `stop` resumes with the same wave and omen level after a restart, in PEX and in vanilla.

#### G7-19 Command storage and scheduled functions

- Scope: in: `data/<ns>/command_storage.dat` and `scheduled_events.dat` read and written. Out: command behaviour.
- Depends on: none
- Branch: feat/command-saved-data
- Needs decision: D7-4
- Done when: `/data modify storage` values and a pending `/schedule function` survive a restart and pass to vanilla and back.

#### G7-20 Boss bars, random sequences and stopwatches

- Scope: in: `custom_boss_events.dat`, `random_sequences.dat` and `stopwatches.dat` read and written instead of stubs. Out: their behaviour.
- Depends on: none
- Branch: feat/bossbar-sequences-saved-data
- Needs decision: D7-4
- Done when: a boss bar, a random sequence's next value and a stopwatch survive a restart and pass to vanilla and back.

#### G7-21 Player file keys

- Scope: in: player files keep the vanilla keys listed in the table (kept or modelled per D7-1); thrown ender pearls and `LastDeathLocation` are modelled. Out: statistics and advancements (G7-22), UUID keys (Goal 4).
- Depends on: none
- Branch: fix/player-data-keys
- Needs decision: D7-1
- Done when: G7-03 shows no diff in the fixture's player file outside the ignore list.

#### G7-22 Advancements and statistics in vanilla's files

- Scope: in: `players/advancements/<uuid>.json` in vanilla's form, read and written; statistics read from and written to `players/stats/<uuid>.json`; a one-time move of PEX's current advancement files and NBT statistics. Out: which UUID names the files (Goal 4).
- Depends on: G3-03 (the move runs once as a `Step::ServerFiles` step, which takes a `config_version` after G3-03's)
- Branch: fix/vanilla-advancements-stats
- Needs decision: none
- Done when: a PEX 1.x player directory moves without losing a criterion or statistic, vanilla reads both files PEX wrote (G7-04), and PEX reads vanilla's. This is what Goal 4's vanilla-direction proof waits on.

#### G7-23 Item stacks keep components PEX can't parse

- Scope: in: an unknown or unparsable component is kept as raw NBT and written back unchanged instead of dropping the stack, with one warning per component name. Out: modelling new components.
- Depends on: none
- Branch: fix/item-unknown-components
- Needs decision: D7-1
- Done when: unit tests round-trip a stack with an unknown component and one with a malformed known component, and G7-03 shows no item loss in fixture containers.

#### G7-24 Entity NBT keys

- Scope: in: `NoGravity`, `Silent`, `Glowing`, `Passengers` and `attributes` for every entity, real types for the five fallback ids, and the per-type keys G7-03 reports, kept or modelled per D7-1. Out: entity behaviour.
- Depends on: G7-03, G1-42 (villager job site and home saved as Brain memories instead of `JobSiteX` and `HomeX`)
- Branch: fix/entity-nbt-keys
- Needs decision: D7-1
- Done when: G7-03 shows no entity diff outside the ignore list.

#### G7-25 Block entity NBT keys

- Scope: in: the per-type keys G7-03 reports, kept or modelled per D7-1; beds no longer saved as `minecraft:bed`. Coordinate with the unmerged `fix/block-entity-bookkeeping`, which changes the same save path. Out: the unload loss (G7-10).
- Depends on: G7-03, G7-10
- Branch: fix/block-entity-nbt-keys
- Needs decision: D7-1
- Done when: G7-03 shows no block entity diff outside the ignore list, and G7-04 logs no `Skipping block entity with invalid type`.

#### G7-26 Anvil default for new configs

- Scope: in: `ChunkConfig` defaults to Anvil (Q7.1). A config file without `[world.chunk]` would otherwise switch format, so existing files get `type = "pump"` written in. Out: converting existing worlds.
- Depends on: G7-27, G3-03 (the step that writes `world.chunk.type = "pump"` into existing files lacking it takes a `config_version` after G3-03's)
- Branch: feat/anvil-default
- Needs decision: none
- Done when: a new server writes `.mca`; a config without the key keeps Pump after migration; the Goal 3 config tests pass.

#### G7-27 Conversion keeps its sources

- Scope: in: `convert_into_dst` moves source region files and `.mcc` files to a backup folder (Q7.3) instead of deleting them, for boot conversion and `--convert-world`, and logs the folder. Out: the default format (G7-26).
- Depends on: none
- Branch: fix/convert-keeps-source
- Needs decision: none
- Done when: a unit test converts a folder and finds every source file byte for byte in the backup.

#### G7-28 Worlds older than 5023

- Scope: in: what D7-2 picks for data versions 4435 to 5022.
- Depends on: G7-03
- Branch: fix/old-data-versions
- Needs decision: D7-2
- Done when: a world made by an older vanilla jar (piston-meta) and saved by PEX opens in vanilla 26.3 with no G7-04 log errors and no block or item diff, or, if D7-2 picks refusing, PEX refuses it with a message naming the version.

#### G7-29 Plugin custom data per dimension and through vanilla

- Scope: in: `pumpkin_custom_data.nbt` per dimension folder, falling back to the root file for the overworld; plus what D7-5 picks for plugin data inside chunks, entities, block entities and items. Out: the WIT API.
- Depends on: none
- Branch: fix/custom-data-per-dimension
- Needs decision: D7-5
- Done when: different data in the overworld and the Nether both survive a restart, and plugin data passes through vanilla as D7-5 says.

#### G7-30 Take session.lock

- Scope: in: create and lock `session.lock` like vanilla's `DirectoryLock`, and refuse to start on a locked world. Out: anything else at startup.
- Depends on: none
- Branch: fix/world-session-lock
- Needs decision: none
- Done when: PEX refuses a world vanilla has open, and vanilla refuses one PEX has open.

#### G7-31 Long-play round trip

- Scope: in: the world from Goal 6's full run opened in vanilla 26.3 and then in PEX again, with checks for the dead dragon (no new one), gateways, exit portal, inventories, ender chest, chests, advancements and statistics. Out: new mechanics.
- Depends on: G7-04, G6-22 (a saved world from the full chained run), G6-19 (the dragon fight saved, which should write under the End's `data/` folder like `LevelDatToSavedDataFileFix`)
- Branch: test/world-roundtrip-survival
- Needs decision: none
- Done when: vanilla shows that state with no new error line, and PEX's reload of vanilla's save compares equal to PEX's own (G7-02).

### Proof

- G7-03 passes in CI (G0-01) on G7-01's world with no diff outside the ignore list, and every ignore entry has a reason.
- G7-04 passes in CI (G0-02): vanilla 26.3 loads PEX's save with no line matching its patterns, its re-save matches PEX's save outside the ignore list, and PEX's second load equals its first. The same holds for a world PEX generated with the default config.
- A new server with the default config writes `.mca` (G7-26), and no conversion deletes a file (G7-27).
- G7-31 runs once before release and its result is in the PR.

### Decisions

- D7-1: Should PEX keep NBT it doesn't model and write it back unchanged, or model each field? Options:
  - Pass through: keep unknown keys of chunks, entities, block entities, players and item components and write them back. One generic mechanism, nothing lost; costs memory per object, and a kept key can contradict a modelled one (for example a vanilla key derived from state PEX changed).
  - Model each field: exact behaviour, but many per-type changes, and every unmodelled key stays lost until its piece lands.
  - Pass through now, model fields as features need them.
  - Waits on it: G7-06, G7-21, G7-23, G7-24, G7-25.
- D7-2: What should PEX do with worlds saved before 5023 (it accepts 4435 and up)? Options:
  - Refuse to start. Safe, but every world from before 26.3 needs vanilla 26.3 to upgrade it first.
  - Keep each untouched chunk's own `DataVersion` so vanilla fixes it later. Small; chunks PEX changed are still restamped without fixes.
  - Port the 26.x fixers (4996 to 5016). Matches vanilla; ten fixers over chunks, block entities and item components to write and keep up.
  - Waits on it: G7-28.
- D7-3: How much of a structure start should PEX save for chunks it generates? Options:
  - Full `StructureStart` NBT with every piece, like vanilla. Every structure piece type needs a serializer.
  - Start id, bounding box and references only. Enough for spawn overrides and `/locate`; what vanilla does with a start without pieces is unknown until G7-04 runs it.
  - Nothing: only vanilla's starts survive (G7-06), and PEX-generated chunks keep losing them.
  - Waits on it: G7-07, and through it G6-10.
- D7-4: Which `data/` files must round-trip for 2.0? Options:
  - All: scoreboard, maps, world border, chunk tickets, raids, command storage, scheduled events, boss bars, random sequences, stopwatches. Six pieces.
  - A subset (for example maps, scoreboard, world border and chunk tickets), with the rest listed as known losses in the release notes.
  - Waits on it: G7-15 to G7-20.
- D7-5: What happens to plugin data (`PumpkinCustomData`) when a world goes through vanilla? Options:
  - Accept the loss and document it. No work.
  - A sidecar file per region beside vanilla's files, keyed by position or UUID. Survives vanilla, but goes stale when vanilla moves or kills things.
  - Store it in slots vanilla keeps, such as the `minecraft:custom_data` component on items. Survives where a slot exists; chunks have none. Changes the plugin API's storage (D2-2 decides whether the WIT custom data calls can change).
  - Waits on it: G7-29.
- D7-6: How should block entity changes reach their chunk before it unloads (`/srv/pex/pr-drafts/ISSUE-block-entities-lost-on-unload.md`)? Options:
  - Flush before unload, as `fix/flush-block-entities-before-unload` does. One cross-crate hook, no per-type change; needs a rebase over two conflicting files.
  - Write on change. Closest to vanilla; the issue counts 18 of 54 block entity types with `is_dirty`, so most need dirty tracking.
  - Hold the chunk with a ticket until the World has saved its block entities. Simple; delays unloading by up to one cleanup cycle.
  - Waits on it: G7-10, and through it G6-06 and G6-22.
  - Partly answered 2026-10-31 (Neon accepted Q6.3): the block entity loss is settled before the survival stages, so G7-10 comes early. Still open: which of the three methods.
  - Answered 2026-10-31 (Neon): flush before unload, by rebasing `fix/flush-block-entities-before-unload` (`870f8ce5c`). The observable result matches vanilla, which saves a block entity's state with its chunk.

## Goal 8: Bedrock polish

Bedrock polish.

Start from: how Bedrock support works today and its known gaps (issues, TODOs, docs). "Polish" is open-ended, so its Decisions must pin down what done means.

### Current state

Paths without a crate prefix are under `crates/pumpkin-core/src/`.

Version and transport

- Alpha targets Bedrock 1.26.51, protocol 2193: `CURRENT_BEDROCK_MC_VERSION` and `CURRENT_BEDROCK_MC_PROTOCOL` in `crates/pumpkin-world/src/lib.rs`. `incompatible_protocol_status` (`net/bedrock/login/request_network_settings.rs`) answers any other protocol with `OutdatedClient` or `OutdatedServer` (test `accepts_only_the_current_bedrock_protocol`). `BedrockMinecraftVersion` (`crates/pumpkin-util/src/version.rs`) knows only `V_1_21` (671) and `V_1_26_51` (2193).
- The current client is newer. Upstream master moved to 1.26.60, protocol 2225, in Pumpkin-MC/Pumpkin#4407 (merged 2026-10-29; upstream's `crates/pumpkin-world/src/lib.rs` now says 2225), and both client libraries under "Tests and tooling" target 2225. So a 1.26.60 client gets `OutdatedServer` from alpha. Not tried with a real client here.
- Packet layouts: 62 files in `crates/pumpkin-protocol/src/bedrock/` say "Last verified for v2169" (26.45, per the comment on Pumpkin-MC/Pumpkin#3340), none says 2193, and 31 have no marker (`git grep -c "Last verified for v" -- crates/pumpkin-protocol/src/bedrock`).
- Transport is NetherNet only. `BedrockClient` (`net/bedrock/mod.rs`) wraps a `NetherNetSession`; RakNet was removed in `ffcf09ece` ("chore: remove bedrock raknet (#2811)"). `StatusResponder::receive` (`net/bedrock/status.rs`) still answers RakNet unconnected pings for the server list on UDP 19132 (IPv6 on the port plus one, `StatusResponder::bind`) and rejects RakNet connection requests with `CIncompatibleProtocolVersion` (`reject_legacy_raknet`).
- Signaling is plain HTTP on TCP 19132: `NetherNetListener::bind` (`net/bedrock/nethernet/endpoint.rs`), axum routes `/v1/join` and `/v1/join/{network_id}`. There is no TLS (`git grep -n -i "rustls\|tls" crates/pumpkin-core/src/net/bedrock` finds only DTLS lines), so direct joins rely on trust on first use of `nethernet.identity_key`. WebRTC is the `webrtc` 0.21.1 crate (root `Cargo.toml`); LAN discovery is `NetherNetDiscovery` (`nethernet/discovery.rs`).
- Upstream Bedrock work alpha lacks. Alpha's last upstream merge is `cf50108ad` (2026-10-23, merge base `8ca5cb443`). Since then upstream master changed `net/bedrock/` in Pumpkin-MC/Pumpkin@8e21ebc7b (whitelist and bans on Bedrock login), Pumpkin-MC/Pumpkin#4397 (workstation item stack requests, combat, movement, falling block and TNT metadata; +1585 -146), Pumpkin-MC/Pumpkin#4407 (1.26.60; +788 -671, 32 files), Pumpkin-MC/Pumpkin#4408 (movement emission), Pumpkin-MC/Pumpkin#4427 (status over NetherNet, RakNet ping removed) and Pumpkin-MC/Pumpkin#4428 (cursor drag, item stack fixes), plus Pumpkin-MC/Pumpkin@645895098, which reverts #4407's change to the v0.1 WIT. Found with `gh api "repos/Pumpkin-MC/Pumpkin/commits?sha=master&path=crates/pumpkin-core/src/net/bedrock&since=2026-10-23T00:00:00Z"`; none of these objects is in `/srv/pex/repo`, so none is in alpha. Upstream's core refactors `b70c7e1e6` and `5e9101f35` (2026-10-26, 2026-10-25) also touch `net/bedrock/` and are not in alpha (`git merge-base --is-ancestor`).
- Alpha's own Bedrock commits that upstream lacks: `e95094043` (Java reach checks on `UseItemOnEntity`) and `92c807b4b`, `4d62dd248`, `256dba417` (motion and pushing sync) (`git log --no-merges upstream/master..300621c3e` over the Bedrock paths, with the local `upstream/master` at `de9538a17`).

Login, authentication, UUIDs

- `BedrockClient::try_handle_login` (`net/bedrock/login/login.rs`): with `online_mode` the token is checked against `Server::bedrock_oidc_keys` (`verify_oidc_token_path` in `login/mod.rs`); `SelfSigned` tokens pass only when `authentication.enabled = false`; guest and split-screen logins are refused (`LoginError::GuestUnimplemented`). With `online_mode = false` the claims are read with no check (`extract_oidc_token_player_claims`), so the client picks its own name and XUID.
- UUIDs: `extract_oidc_player_claims` and `xuid_to_uuid` (`crates/pumpkin-auth/src/jwt/mod.rs`) take the MD5 of `pocket-auth-1-xuid:<xid>` with version 3 bits; without `xid`, the `leguuid` claim; without either, the MD5 of `pocket-auth-1-xuid:` with nothing after it, which is the same UUID for every such player. Floodgate uses `new UUID(0, xuid)` (`Utils.getJavaUuid` in GeyserMC/Floodgate `core/src/main/java/org/geysermc/floodgate/util/Utils.java`), so player files from a Geyser and Floodgate server do not line up with PEX's. Names come from `build_username` (`username_prefix`, empty by default; `replace_username_spaces`).
- OIDC keys are fetched once by a task at startup (`Server` setup in `server/mod.rs`, guarded by `online_mode && authentication.enabled`, not by `bedrock.enabled`). On failure an empty key set goes into the `OnceCell` and is never refetched, so every online Bedrock login fails until a restart. Keys are never refreshed.
- Bedrock logins skip the join checks. `can_not_join` (`net/mod.rs`: player ban, IP ban, whitelist) has one caller, `net/java/pending.rs`. The duplicate UUID and name checks are in `net/java/login/encryption_response.rs` only. `spawn_bedrock_client_task` (`lib.rs`) goes from `handle_login_sequence` straight to `Server::add_player`, which checks neither. Java enforces `java.max_players` in `net/java/login/login_start.rs`; `bedrock.max_players` is only advertised (`nethernet/discovery.rs`, `nethernet/endpoint.rs`). Upstream fixed bans and the whitelist in Pumpkin-MC/Pumpkin@8e21ebc7b.

What a Bedrock player gets in play

- `BedrockClient::handle_play_packet` (`net/bedrock/mod.rs`) dispatches 27 serverbound packet types; anything else logs `Bedrock: Received Unknown Game packet` at WARN. A read error is logged in `Player::process_inbound_packets` (`entity/player.rs`) but, unlike the Java branch next to it, never kicks. `handle_interaction` only opens the inventory, `handle_player_action` ends in a `// TODO` catch-all (`play/player_action.rs`), `SSetPlayerInventoryOptions` and `SLoadingScreen` are ignored, `handle_emote_list` only logs, and `SPlayerHotbar` (id 48) is defined but not dispatched.
- No Bedrock packet struct exists for book editing, map info, sub-chunk requests, lecterns, player skin changes, rider jumps or server settings (`git grep -l -i` for each name in `crates/pumpkin-protocol/src/bedrock` is empty). `CCorrectPlayerMovePrediction` and `CGamerulesChanged` are defined and never used.
- Gameplay code sends Java packets unless someone added a Bedrock path. `Player::send_client_packet`, `Player::try_send_client_packet`, `JavaPlayer::send_packet` (`entity/player.rs`) and `World::broadcast_packet_all`, `broadcast_packet_except`, `broadcast_to_chunk`, `broadcast_to_chunk_except` and `broadcast_java_clients` (`world/mod.rs`) reach Java clients only. They have 126 call sites outside `net/java/`, against 113 `*_editioned(` and 6 `broadcast_to_chunk_bedrock(` calls (`git grep -n -E "<helper>\(" -- crates/pumpkin-core/src ':!crates/pumpkin-core/src/net/java'`, definitions left out). Java only through these: particles (`Player::spawn_particle`, `World::spawn_particles`), statistics (`Player::send_stats`), world border, map data (`CMapItemData`), the advancement screen (`CUpdateAdvancements`), item cooldowns, camera, tab list header.
- Sounds: `World::play_sound_raw` and `World::play_sound_event` broadcast to Java only. There are 363 calls of `play_sound*` and `play_block_sound` against 3 of `World::play_bedrock_level_sound`. `pumpkin-data` has no Java-to-Bedrock sound or particle mapping (`ls crates/pumpkin-data/src/generated | grep -i bedrock` gives only `bedrock_biome.rs` and `bedrock_creative.rs`).
- Text is flattened on the server. `TextComponent::to_bedrock_string` (`crates/pumpkin-util/src/text/mod.rs`) writes `%<key>` and falls back to the Java key when a component has no `bedrock_translate`, so advancement titles reach Bedrock raw (`PlayerAdvancement::finish_award` in `entity/player/advancement.rs`; Pumpkin-MC/Pumpkin#2901). `TextComponent::get_text()` always renders `Locale::EnUs`, whatever `ClientData.language_code` says; vanilla text exists only in English in `assets/` (`en_us_java.json`, `en_us_bedrock.lang`; `assets/translations/` holds Pumpkin's own strings).
- Chat: with `allow_chat_reports = true` a Bedrock player's message is logged and dropped, because the broadcast is commented out under `//TODO Alex help, what is this?` (`BedrockClient::handle_chat_message`, `play/chat_message.rs`). The default is false (`crates/pumpkin-config/src/lib.rs`). Java to Bedrock chat works either way (`OutgoingChatMessage::send_to_player`, `net/chat/mod.rs`).
- Entities: `send_bedrock_spawn_packet` (`entity/mod.rs`) builds `CAddActor`; mob metadata comes from `Mob::mob_metadata`, implemented in 13 mob files. Open TODOs: falling block variant (`entity/falling.rs`), TNT fuse (`entity/tnt.rs`), a non-mob metadata hook and the pig, cow and chicken `climate_variant` (`entity/mod.rs`).
- Skins: `Player::fetch_skin` downloads a Java player's skin with `block_in_place` and `block_on` inside `Player::new` (`entity/player.rs`) for every profile with a `textures` property, even with `networking.bedrock.enabled = false` (upstream fix Pumpkin-MC/Pumpkin#3820 is open). A Bedrock player's own skin (`skin_id`, `skin_data` in `ClientData`, `crates/pumpkin-protocol/src/bedrock/server/login.rs`) is never read (`git grep '\.skin_data' crates/pumpkin-core/src` hits only `fetch_skin`), so other Bedrock players see the Steve fallback and Java players see a default skin.
- Smaller gaps: `World::spawn_bedrock_player` sends `permission_level: 2` for everyone, with a TODO (`world/mod.rs`); `LevelTime::send_time` has a TODO on frozen time (`world/time.rs`); `recipe::build` (`net/bedrock/recipe.rs`) sends shaped and shapeless crafting only, other recipe types fall through `_ => {}`; `Player::open_handled_screen` maps 15 window types to Bedrock container types and the rest to 0; on the Bedrock break path `Player::tick` records `state.id` as the Mined statistic where the other paths use the block id.
- Present in code: resource pack streaming (`login/resource_pack_response.rs`), block actor data and sign editing (`play/block_actor_data.rs`), forms for Wasm plugins (`CModalFormRequest` in `crates/pumpkin-wasm-host-v0_2/src/player.rs`, answers as `BedrockFormResponseEvent`), Bedrock scoreboards (`BedrockPlayer::set_scoreboard`), the command tree (`CAvailableCommands` in `command/client_suggestions.rs`), chunk encoding on Rayon with blob caching (`BedrockClient::send_chunks`, `CLevelChunk::encode_chunk`).

Robustness

- Client-triggerable panic: `map_bedrock_container_slot` (`net/bedrock/play/item_stack_request.rs`) does not range-check `ContainerName::HotBar` slot ids, and `get_slot_stack` and `update_slot_stack` index `slots[screen_slot]`. On the player screen (46 slots) a hotbar slot id of 10 or more indexes past the end, inside `Player::process_inbound_packets`, which runs in the tick with no `catch_unwind`. Read from the code, not reproduced. Upstream draft Pumpkin-MC/Pumpkin#4056 adds bounds.
- `FragmentBuffer::push` (`nethernet/session.rs`) limits a reassembled packet only by the 255-segment counter, not by size (upstream draft Pumpkin-MC/Pumpkin#4001). The batch decoder does cap at `MAX_PACKET_DATA_SIZE` (`crates/pumpkin-protocol/src/bedrock/packet_decoder.rs`).
- `BedrockClient::blob_cache` gets every new chunk blob in `send_chunks` and nothing removes entries (`git grep -n blob_cache` shows one insert and one read), so it grows for the whole session (upstream Pumpkin-MC/Pumpkin#3244).
- Upstream's panic Pumpkin-MC/Pumpkin#2291 (spawn packet from a Rayon thread) no longer matches alpha's code: `BedrockClient::enqueue_spawn_packet` is synchronous and `BedrockClient::spawn_task` enters `rt_handle`.

Config, deployment and docs

- `BedrockConfig::default` (`crates/pumpkin-config/src/networking/bedrock.rs`) has Bedrock on with `online_mode = true`, so every PEX server binds UDP 19132, UDP 19133 and TCP 19132. `docker-compose.yml` publishes only `19132:19132/udp`, so signaling can't be reached there; the `Dockerfile` exposes only 25565; `egg-pex.json` rewrites only the Java address. `online_mode` and `authentication.enabled` overlap (see login).
- `README.md` says PEX "Supports the latest Java & Bedrock Minecraft server version". `assets/bedrock/README.md` lists Geyser and CloudburstMC data for 26.50 and BDS 1.26.40 biome definitions. `docs/` has no Bedrock page (`grep -il bedrock docs/*.md` finds only the roadmap).

TODO markers

`git grep -n -E 'TODO|todo!|unimplemented!|FIXME|XXX|HACK' -- crates/pumpkin-core/src/net/bedrock crates/pumpkin-protocol/src/bedrock 'tools/pumpkin-codegen/src/bedrock_*.rs' tools/pumpkin-codegen/src/wit/v0_2/bedrock_packet.rs crates/pumpkin-config/src/networking/bedrock.rs` finds 24, and `git grep -n -i -E "(TODO|FIXME).*bedrock|bedrock.*(TODO|FIXME)" -- '*.rs'` finds 6 more elsewhere. All 30 are comments; there is no `todo!` or `unimplemented!`.

| Area | Count | What they are |
| :-- | :-- | :-- |
| `net/bedrock/login` | 1 | ignore a pack response after spawn (`resource_pack_response.rs`; upstream draft Pumpkin-MC/Pumpkin#4067) |
| `net/bedrock/play` | 3 | secure chat broadcast, unhandled player actions, `pack_degrees` cleanup |
| `net/bedrock/nethernet`, `level_chunk.rs`, `recipe.rs`, `status.rs`, `mod.rs` | 0 | |
| `crates/pumpkin-protocol/src/bedrock` | 20 | 13 fields that want an enum or a real type, 4 "update inventory" (`add_item_actor`, `add_player`, `creative_content` twice), 3 unverified fields (`start_game` "LE", `update_abilities` "confirm these", `network_chunk_publisher_update`) |
| codegen `bedrock_*.rs`, config `bedrock.rs` | 0 | |
| elsewhere in `pumpkin-core` | 6 | falling block variant, TNT fuse, non-mob metadata hook, climate variant, permission level, frozen time |

By kind: 9 missing behaviour, 13 typing, 8 unverified fields or cleanup.

Tests and tooling

- 36 tests in `crates/pumpkin-core/src/net/bedrock` and 40 in `crates/pumpkin-protocol/src/bedrock` (`#[test]` plus `#[tokio::test]`); fuzz targets `decoder_bedrock.rs` and `encoder_bedrock.rs` in `crates/pumpkin-protocol/fuzz/fuzz_targets/`.
- Nothing runs a Bedrock connection end to end. `create_test_server` (`tools/pumpkin-tester/src/harness.rs`) sets `bedrock.enabled = false`, `tools/dual-run/dual_run.py` writes `enabled = false`, and the play-test rig's `/srv/pex/playtest/server-config.py` turns Bedrock off. CT 117 has neither Go nor Node (`which go node`), and `/srv/pex/tools` has no Bedrock client.
- Client libraries at 2225: gophertunnel (`minecraft/protocol/info.go`: `CurrentProtocol = 2225`; NetherNet through `df-mc/go-nethernet`, whose `endpoint` package has a `/v1/join` client) and PrismarineJS bedrock-protocol 3.61.0 (`CURRENT_VERSION = '1.26.60'` in `src/options.js`, NetherNet in `src/nethernet/`; `/v1/join` shows up only in `tools/vanillaClient.js`). A Rust bot would need the other direction of PEX's codecs: 3 of 62 clientbound packet files have a reader and 9 of 30 serverbound files have a writer.

Issues and PRs

- Conman1533/PEX has issues disabled. Its only PR matching "bedrock" is Conman1533/PEX#2 (mob AI, merged).
- Open upstream issues labelled "Bedrock Edition": 19 (`gh issue list -R Pumpkin-MC/Pumpkin --label "Bedrock Edition" --state open`). `--search bedrock` adds 29 more whose titles are about the block or both editions.
  - Joining: Pumpkin-MC/Pumpkin#3738 (a 1.26.51 client sends TLS to the signaling port and gets plain HTTP 400), Pumpkin-MC/Pumpkin#3487 (outdated client, config parse error), Pumpkin-MC/Pumpkin#2370 (blank name, rubber-banding; a comment says fixed), Pumpkin-MC/Pumpkin#3340 (Geyser-Velocity transfer), Pumpkin-MC/Pumpkin#3735 (several Bedrock versions at once).
  - Movement and physics: Pumpkin-MC/Pumpkin#2104 (floating, swim state, unknown packets 30, 31, 152, 307), Pumpkin-MC/Pumpkin#3064 (damage next to blocks), Pumpkin-MC/Pumpkin#3831 (crawling into one-block gaps), Pumpkin-MC/Pumpkin#4421 (saddled mounts don't move).
  - Inventory and block state: Pumpkin-MC/Pumpkin#4130 (two blocks used per placement, food not consumed, ghost blocks, hotbar out of sync, furnace items vanish), Pumpkin-MC/Pumpkin#3396 (fall damage, commands, difficulty setting, mob hits), Pumpkin-MC/Pumpkin#4321 (block pick ignored), Pumpkin-MC/Pumpkin#3239 (lava and water, both editions).
  - Text and lifecycle: Pumpkin-MC/Pumpkin#2901 (raw advancement keys), Pumpkin-MC/Pumpkin#2981 (not disconnected on stop; not checked at alpha), Pumpkin-MC/Pumpkin#2291 (see Robustness).
  - API and features: Pumpkin-MC/Pumpkin#4006 (`PacketSentEvent`), Pumpkin-MC/Pumpkin#3841 (mutable resource pack config), Pumpkin-MC/Pumpkin#4021 (experimental minecarts).
- Open upstream PRs labelled "Bedrock Edition": 14.
  - Robustness: Pumpkin-MC/Pumpkin#4061 (bans and whitelist; @8e21ebc7b already landed the same fix), Pumpkin-MC/Pumpkin#4056 (hotbar bounds, draft), Pumpkin-MC/Pumpkin#4001 (NetherNet size cap, draft), Pumpkin-MC/Pumpkin#3245 (release closed sessions), Pumpkin-MC/Pumpkin#3244 (retire chunk blobs), Pumpkin-MC/Pumpkin#4067 (spawn once per pack response, draft). Unlabelled but related: Pumpkin-MC/Pumpkin#3996 (validate positions in Bedrock input).
  - Performance: Pumpkin-MC/Pumpkin#3427 (reuse palettes), Pumpkin-MC/Pumpkin#3467 (skip all-air sub-chunks).
  - Features: Pumpkin-MC/Pumpkin#4418 (Bedrock scoreboard with tests), Pumpkin-MC/Pumpkin#4322 (saddle slot), Pumpkin-MC/Pumpkin#3793 and Pumpkin-MC/Pumpkin#3794 (push and velocity sync), Pumpkin-MC/Pumpkin#3642 (gamemode feedback order per edition), Pumpkin-MC/Pumpkin#3820 (skin download off the join).

### Pieces

G8-01 to G8-12 are needed whatever D8-1 says (G8-11 and G8-12 wait on D8-3 only for the choice of tool). G8-13 onward wait on D8-1.

#### G8-01 Bans, whitelist, duplicate logins and player limit for Bedrock

- Scope: in: run `can_not_join` (player ban, IP ban, whitelist), the duplicate UUID and name checks and a `bedrock.max_players` check (as `java.max_players` in `net/java/login/login_start.rs`) on the Bedrock path before `Server::add_player`, starting from Pumpkin-MC/Pumpkin@8e21ebc7b, and kick with the Bedrock keys the Java path already passes through `translate_cross`. Out: the UUID scheme (G8-20).
- Depends on: none
- Branch: fix/bedrock-login-checks
- Needs decision: none
- Done when: tests `bedrock_login_rejects_banned_player`, `bedrock_login_rejects_banned_ip`, `bedrock_login_rejects_unlisted_player`, `bedrock_login_rejects_online_uuid` and `bedrock_login_rejects_when_full` in `net/bedrock/login/` fail on alpha and pass.

#### G8-02 Upstream Bedrock gameplay on alpha

- Scope: in: cherry-pick Pumpkin-MC/Pumpkin#4397 onto alpha, keeping one entity reach check where it overlaps alpha's `e95094043`; a separate commit that makes the Bedrock break path in `Player::tick` record the block id as the Mined statistic, if #4397 still records `state.id`. Out: the protocol bump (G8-03). If #4397 doesn't apply without upstream's core refactors (`b70c7e1e6`, `5e9101f35`), stop and raise it under D8-2.
- Depends on: none
- Branch: feat/bedrock-upstream-gameplay
- Needs decision: none. If D8-2 picks full upstream merges and one lands first, this piece shrinks to its Done when on the merged tree.
- Done when: `cargo nextest run -p pumpkin-core -p pumpkin-inventory -p pumpkin-protocol` passes, including #4397's `net::bedrock` tests; `bot:entity_interaction_range` (`tools/pumpkin-tester/src/bot_tests/entity_interaction_range.rs`) still passes; a boot test with Bedrock on logs no new WARN or ERROR.

#### G8-03 Bedrock 1.26.60 (protocol 2225)

- Scope: in: cherry-pick Pumpkin-MC/Pumpkin#4407 together with Pumpkin-MC/Pumpkin@645895098 (so the public v0.1 WIT stays as it is), then Pumpkin-MC/Pumpkin#4427; if #4427 needs #4408's `net/bedrock` hunk, take only that hunk. Update the versions in `assets/bedrock/README.md`. Out: accepting 2193 as well (D8-2), #4408's movement emission feature.
- Depends on: G8-02
- Branch: feat/bedrock-26-60
- Needs decision: none (same note as G8-02 on D8-2)
- Done when: `accepts_only_the_current_bedrock_protocol` asserts 2225; the status and LAN discovery tests report 1.26.60 and 2225; a 1.26.60 client joins a local debug build by IP and through LAN discovery, reaches spawn and sees terrain (human check with a capture until G8-11 automates it).

#### G8-04 Bounds on Bedrock item stack slots

- Scope: in: cherry-pick Pumpkin-MC/Pumpkin#4428; range-check every `ContainerName` arm of `map_bedrock_container_slot` against the open screen's slot count; make `get_slot_stack` and `update_slot_stack` use `get` instead of indexing; answer an out-of-range slot with an error response. Out: other inventory behaviour (G8-13).
- Depends on: G8-03 (#4428 sits on #4407)
- Branch: fix/bedrock-slot-bounds
- Needs decision: none
- Done when: test `hotbar_slot_out_of_range_maps_to_none` in `net/bedrock/play/item_stack_request.rs` covers hotbar slot ids 9 to 255 on the player screen and on a chest screen, and fails or panics on alpha.

#### G8-05 Bounded NetherNet and chunk cache memory

- Scope: in: cap `FragmentBuffer::push` at `MAX_PACKET_DATA_SIZE` in total and close the session past it (as Pumpkin-MC/Pumpkin#4001); drop `BedrockClient::blob_cache` entries once `SClientCacheBlobStatus` lists them in `hit_hashes` or `miss_hashes` and the miss has been answered (as Pumpkin-MC/Pumpkin#3244); release closed and abandoned NetherNet sessions (as Pumpkin-MC/Pumpkin#3245). Out: chunk encoding speed (Pumpkin-MC/Pumpkin#3427, #3467).
- Depends on: none
- Branch: fix/bedrock-connection-limits
- Needs decision: none
- Done when: tests `fragment_buffer_rejects_oversized_packet`, `blob_cache_drops_acknowledged_blobs` (cache empty after a status that lists every sent hash) and a test that the endpoint keeps no state for a session after it closes; each fails on alpha.

#### G8-06 Cross-play chat and Bedrock text

- Scope: in: broadcast Bedrock chat when `allow_chat_reports = true`, the way the `false` branch of `handle_chat_message` does; `to_bedrock_string` callers (advancement announcements first) send the rendered text for Java-only translation keys instead of `%key`. Out: text in the player's own locale (vanilla text is only in `assets/en_us_java.json` and `assets/en_us_bedrock.lang`, so it needs more Extractor data; G8-22 onward), forms and scoreboards.
- Depends on: none
- Branch: fix/bedrock-chat-text
- Needs decision: none
- Done when: test `bedrock_chat_broadcasts_with_chat_reports` fails on alpha and passes; a `pumpkin-util` test that the Bedrock string of `advancements.adventure.sleep_in_bed.title` is its en_us text, not `%advancements.adventure.sleep_in_bed.title` (fails on alpha).

#### G8-07 Java skin download off the join path

- Scope: in: no skin download when `networking.bedrock.enabled = false`; otherwise fetch on a task after the join, with a size and time limit, and send the converted skin to Bedrock viewers when it arrives (as Pumpkin-MC/Pumpkin#3820). Out: Bedrock players' own skins (G8-18).
- Depends on: none
- Branch: perf/skin-fetch-off-join
- Needs decision: none
- Done when: a test where the `textures` URL points at a local server that waits 5 s: `Player::new` returns in under 100 ms (about 5 s on alpha), and with Bedrock off that server gets no request.

#### G8-08 Bedrock ports in Docker and the eggs

- Scope: in: `docker-compose.yml` publishes TCP 19132 and UDP 19132 to 19133; the `Dockerfile` exposes them; `egg-pex.json` and `egg-pumpkin.json` map the Bedrock address to an allocation. Out: changes on the Pelican fleet.
- Depends on: none
- Branch: fix/bedrock-deploy-ports
- Needs decision: none
- Done when: after `docker compose up` with the repo's file, `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:19132/v1/join` prints 200 (alpha: connection refused); importing the egg shows a Bedrock port.

#### G8-09 OIDC key lifecycle

- Scope: in: fetch keys only when `networking.bedrock.enabled`; retry a failed fetch with backoff instead of keeping the empty set; refetch, rate-limited, when a token names an unknown `kid`. Out: merging `online_mode` and `authentication.enabled` (a config change for Goal 3's migration).
- Depends on: none
- Branch: fix/bedrock-oidc-keys
- Needs decision: none
- Done when: with a local JWKS server, a first fetch that fails is followed by a login that verifies (fails on alpha); a token signed with a key added after startup verifies after one refetch; a boot test with Bedrock off and no network logs no "Failed to fetch Bedrock OIDC keys".

#### G8-10 Bedrock coverage table

- Scope: in: `docs/BEDROCK.md` with one row per Java-only send in `pumpkin-core` (the 126 helper call sites, the sound and particle helpers, direct `ClientPlatform::Java` sends), grouped by feature and marked covered, partial or missing for Bedrock, plus one row per serverbound packet a 1.26.60 client sends that `handle_play_packet` doesn't handle. It is the input to D8-1. Out: fixing rows.
- Depends on: G8-03
- Branch: docs/bedrock-coverage
- Needs decision: none
- Done when: a script kept outside the repo diffs the call-site grep from Current state against the table and prints nothing; the unhandled-packet rows come from the WARN lines of a real 1.26.60 session of at least 15 minutes.

#### G8-11 Bedrock test bot

- Scope: in: a scripted client on the library D8-3 picks, joining a local server with `online_mode = false` over NetherNet by IP, no Xbox account: login, spawn, chunks, walk 50 blocks, break and place a block, chat to and from a Java `HeadlessBot`, open and close a chest, disconnect. First check that the library can send the login payload PEX reads (`AuthPayload` in `net/bedrock/login/mod.rs`) with a self-signed token. Kept outside the repo or in `tools/`, per D8-3. Out: CI (G8-12), Xbox sign-in.
- Depends on: G8-03
- Branch: test/bedrock-bot
- Needs decision: D8-3
- Done when: the scenario passes 10 runs in a row against alpha with G8-03, each under 60 s, and on alpha without G8-06 it fails at the chat step when `allow_chat_reports = true`.

#### G8-12 Bedrock bot in CI

- Scope: in: a job, or a step of `gametest` in `.github/workflows/rust.yml`, that starts the server with Bedrock on and offline and runs G8-11's scenario. Out: Xbox sign-in, Windows and macOS runners.
- Depends on: G8-11, G0-01
- Branch: ci/bedrock-bot
- Needs decision: D8-3
- Done when: the job runs on a push to `alpha` and passes, and a scratch branch that drops `SText` handling makes it fail.

#### G8-13 Survival containers for Bedrock

- Scope: in: the containers on D8-1's list (for option (a): crafting table, furnace, blast furnace, smoker, chest, double chest, shulker box, anvil, enchanting table, brewing stand, villager trading), checked after G8-02 and G8-04, with fixes for what fails, starting with Pumpkin-MC/Pumpkin#4130's furnace items, hotbar sync, uneaten food and double placement. Out: the creative inventory, containers not on the list.
- Depends on: G8-04, G8-11
- Branch: fix/bedrock-containers
- Needs decision: D8-1
- Done when: a G8-11 scenario per container (put in, take out, close, reopen, contents match the server) passes, and the same rows of the human checklist pass on a 1.26.60 client.

#### G8-14 Movement and physics for Bedrock

- Scope: in: fall damage, swimming and floating, damage next to walls, crawling into one-block gaps, ridden mounts (Pumpkin-MC/Pumpkin#2104, #3064, #3831, #3396, #4421), after G8-02's movement changes, compared with what a Java player gets on PEX and on vanilla. Out: anticheat; movement stays client authoritative (`AGENTS.md`).
- Depends on: G8-02, G8-11
- Branch: fix/bedrock-movement
- Needs decision: D8-1
- Done when: G8-11 scenarios pass: a 10-block fall costs the same health as a Java `HeadlessBot`'s fall from the same spot (vanilla's `LivingEntity.calculateFallDamage`: floor of fall distance plus 1e-6 minus the safe fall distance), standing in shallow water doesn't set the swim state, walking 20 blocks along a wall costs no health, a saddled horse moves under the bot.

#### G8-15 Sounds for Bedrock players

- Scope: in: a Java-to-Bedrock sound mapping generated from `assets/` (new data from Geyser's mappings or the Extractor, per `AGENTS.md` "Game data and constants"), and the `World::play_sound*` and `play_block_sound` helpers sending it to Bedrock players in range, minus sounds the Bedrock client plays by itself. Out: particles (G8-16).
- Depends on: G8-10
- Branch: feat/bedrock-sounds
- Needs decision: D8-1
- Done when: a codegen test that every mapped Java sound resolves to a Bedrock one; the G8-11 bot receives a level sound packet when a door next to it opens; the human checklist's sound rows pass.

#### G8-16 Particles for Bedrock players

- Scope: in: a Java-to-Bedrock particle mapping generated from `assets/`, and `Player::spawn_particle` and the `World::spawn_particles*` helpers sending it to Bedrock players. Out: sounds (G8-15).
- Depends on: G8-10
- Branch: feat/bedrock-particles
- Needs decision: D8-1
- Done when: a codegen test that every mapped Java particle resolves; the G8-11 bot receives a particle packet for a broken block; the human checklist's particle rows pass.

#### G8-17 Entity metadata and variants for Bedrock

- Scope: in: the entity rows G8-10 marks missing after G8-02: the non-mob metadata hook (falling block and TNT, if #4397 left them), the pig, cow and chicken `climate_variant`, and other `mob_metadata` gaps among 26.3 mobs. Out: mob AI (Goal 1).
- Depends on: G8-02, G8-10
- Branch: fix/bedrock-entity-metadata
- Needs decision: D8-1
- Done when: unit tests that `CAddActor` for falling sand, primed TNT and a cold pig carries the block variant, fuse and climate property; human checklist: they render right.

#### G8-18 Bedrock players' own skins

- Scope: in: build `Player::bedrock_skin` for Bedrock players from `ClientData` (`skin_id`, `skin_data`, geometry), with size limits, so other Bedrock players see it. Out: showing Bedrock skins to Java players, which needs a signed `textures` property and so an outside upload service.
- Depends on: G8-07
- Branch: feat/bedrock-skins
- Needs decision: D8-1
- Done when: a test that a `ClientData` with a 64x64 skin gives a `Skin` with the same bytes and an oversized one is refused; human: two Bedrock clients see each other's skins.

#### G8-19 TLS on NetherNet signaling

- Scope: in: first check with a 1.26.60 client whether plain HTTP still joins (Pumpkin-MC/Pumpkin#3738), then optional TLS on the signaling listener from a configured certificate and key. Out: getting certificates.
- Depends on: G8-03, and G3-03 if the new `[networking.bedrock.nethernet]` keys need a migration step (it takes a `config_version` after G3-03's)
- Branch: feat/nethernet-tls
- Needs decision: D8-1
- Done when: with a test certificate, `curl --cacert test.pem https://localhost:19132/v1/join` answers 200 and plain HTTP still answers when TLS is off; human: a direct join shows no trust prompt.

#### G8-20 Bedrock UUIDs in offline mode and from Geyser servers

- Scope: in: the scheme D8-4 picks; under every option, tokens with neither `xid` nor `leguuid` stop sharing one UUID; under option (b), existing Bedrock player files move to Floodgate's UUIDs through Goal 4's login-time move. Out: Java UUIDs (Goal 4).
- Depends on: none. Goal 4's offline UUID helper and the login-time file move with `uuid-migration.json` are already on alpha (Goal 4, Current state); G4-02 adds their tests.
- Branch: fix/bedrock-uuids
- Needs decision: D8-4
- Done when: a `pumpkin-auth` test that two tokens without `xid` or `leguuid` and with different names get different UUIDs (the same one on alpha), plus the option's own test (under (b): a file under `new UUID(0, xuid)` is found at login for that XUID).

#### G8-21 Supported Bedrock versions, written down

- Scope: in: D8-2's policy in `docs/BEDROCK.md`, `README.md`'s "latest Java & Bedrock" line changed to match, and a release step in `docs/BEDROCK.md` comparing `CURRENT_BEDROCK_MC_PROTOCOL` with upstream's and gophertunnel's `CurrentProtocol`. Under D8-2 (d), this piece writes the multi-version design instead. Out: multi-version code.
- Depends on: G8-10
- Branch: docs/bedrock-version-policy
- Needs decision: D8-2
- Done when: `docs/BEDROCK.md` and `README.md` state the same policy as D8-2's answer, and the release step names the two sources to compare.

#### G8-22 Remaining coverage rows (one piece per group, numbered on from G8-22)

- Scope: in: one piece per group G8-10 marks missing that D8-1 puts in scope, in D8-1's order: for example forms for native plugins, scoreboards (from Pumpkin-MC/Pumpkin#4418), commands and permission level, recipe types beyond crafting, guest and split-screen logins, maps, books, text in the player's locale. Out: groups D8-1 leaves out, which stay as known gaps in `docs/BEDROCK.md`.
- Depends on: G8-10, G8-11
- Branch: fix/bedrock-<group>
- Needs decision: D8-1
- Done when: the group's rows move to covered, each with a G8-11 scenario or a unit test.

### Proof

1. A 1.26.60 client and G8-11's bot join a release build over NetherNet by IP and through LAN discovery, and G8-12's job passes on the `alpha` push that closes the goal.
2. The regression tests of G8-01, G8-04, G8-05, G8-06, G8-07 and G8-09 fail on `300621c3e` and pass on alpha.
3. Soak: 2 Bedrock bots and 2 Java `HeadlessBot`s walk for 60 minutes on one debug build: no panic, no ERROR lines, no `Unknown Game packet` for packets in D8-1's scope, no `blob_cache` entries left after the last chunk batch is acknowledged, and RSS at 60 minutes within 10% of RSS at 10 minutes.
4. Every row of `docs/BEDROCK.md` is covered or listed as a known gap, and every row in D8-1's scope is covered.
5. Under D8-1 (a) or (b): the human checklist on a phone with a 1.26.60 client passes every row, with captures. Under (b), the same checklist on Paper 26.3 with Geyser and the same client, row by row, matches.

### Decisions

- D8-1: What does "Bedrock polish" mean for 2.0? Options:
  - (a) A checklist a Bedrock player completes on the current client: join by IP and LAN; walk, sprint, swim, fall with damage; break and place with the right drops; hotbar, armor, offhand and the survival containers (G8-13's list); eat, health, XP, death, respawn, beds; fight mobs and players; see mobs, items, falling blocks and TNT render right; chat both ways and run commands; nether and end portals; sounds and particles for all of that. A G8-11 check where the bot can do it, a human check on a phone otherwise. Cost: G8-13 to G8-18 and the matching G8-22 rows, plus a phone tester before each release.
  - (b) Geyser parity: the same list, passing when it behaves as on Paper 26.3 with Geyser for the same client, plus what Geyser offers that PEX lacks (TLS signaling G8-19, Floodgate UUIDs through D8-4, skins both ways, forms for native plugins). Cost: a Geyser reference server in CT 117 (Paper is at `/srv/pex/bins/paper/paper.jar` with only bStats and spark in `plugins/`), the most pieces, and a re-check after each upstream Bedrock change.
  - (c) Experimental with honest gaps: done means the current client joins, there's no client-triggerable crash or unbounded memory, bans and the whitelist apply and chat works both ways (G8-01 to G8-12), and `docs/BEDROCK.md` lists everything else as known gaps. Cost: least; G8-13 to G8-19 and G8-22 onward move after 2.0.
  - The earlier roadmap recorded an answer on 2026-10-29: experimental in 2.0 with the coverage table as the list of gaps; survival basics first, then cross-play chat, then forms; a phone checklist before release that doesn't block. That is (c), with (a)'s list in that order for anything done beyond it.
  - Waits on it: G8-13 to G8-19, G8-22 onward, Proof item 5.
  - Answered 2026-10-31 (Neon accepted Q8.1 to Q8.3): (c), experimental in 2.0 with the coverage table as the list of gaps. Anything done beyond that follows (a)'s list in this order: survival basics, then cross-play chat, then forms. A phone checklist runs before the release but doesn't block it. G8-13 to G8-19 and G8-22 move after 2.0, and Proof item 5 doesn't apply.
- D8-2: Which Bedrock versions does PEX support, and how does it keep up? Options:
  - (a) One version, taken from upstream through regular full upstream merges, as on 2026-10-19 and 2026-10-23. Cost: Bedrock can't be joined from a client release until upstream bumps and PEX merges; each full merge touches every goal's code.
  - (b) One version, with upstream's Bedrock commits cherry-picked between merges, as G8-02 and G8-03 do. Cost: conflicts to resolve on each pick, and the next full merge meets the same changes again.
  - (c) One version, ported by PEX itself. Cost: a protocol port per Bedrock release (#4407 was +788 -671 over 32 files) that duplicates upstream.
  - (d) The current and the previous protocol, as Pumpkin-MC/Pumpkin#3735 asks. Cost: `BClientPacket::write_packet` takes no version (`crates/pumpkin-protocol/src/lib.rs`), unlike Java's `serialize_packet_for_version`; `assets/bedrock/` holds one version's data; every packet test doubles. A goal of its own.
  - Waits on it: G8-21. G8-02 and G8-03 change shape only under (a).
- D8-3: Which client does the Bedrock test bot use? Options:
  - (a) gophertunnel (Go): at 2225 and has a `/v1/join` client in `df-mc/go-nethernet`. Cost: a Go toolchain in CT 117 and in CI; the bot lives outside the Rust workspace.
  - (b) PrismarineJS bedrock-protocol (Node): at 1.26.60 with NetherNet, but direct `/v1/join` joins appear only in a tool script, so they may need work. Cost: Node in CT 117 and in CI, plus that check.
  - (c) A Rust bot in `tools/pumpkin-tester`: runs in the existing `gametest` job next to `HeadlessBot`. Cost: the largest; readers for the clientbound packets it checks (3 of 62 exist), writers for the serverbound ones (9 of 30 exist) and a WebRTC client side with the `webrtc` crate. `AGENTS.md` warns against bots turning into side projects.
  - Waits on it: G8-11, G8-12.
- D8-4: Which UUID should a Bedrock player get? Options:
  - (a) Keep the MD5 of `pocket-auth-1-xuid:<xid>`, and give tokens with neither `xid` nor `leguuid` Goal 4's offline UUID of the prefixed name. Cost: smallest, no migration; player files from Geyser and Floodgate servers still don't carry over.
  - (b) Floodgate's `new UUID(0, xuid)`, so files from a Geyser and Floodgate server carry over. Cost: every existing PEX Bedrock player file moves, through Goal 4's move.
  - (c) In offline mode ignore the unchecked `xid` and use Goal 4's offline UUID of the prefixed name for every Bedrock player; online, keep (a) or (b). Cost: with an empty `username_prefix`, a Bedrock player and a Java player of the same name share one UUID and one player file.
  - Waits on it: G8-20.

## Goal 9: Performance vs Paper

Performance vs Paper: fix extra network traffic and p95 tick spikes, publish benchmarks.

Start from: where the extra traffic comes from (packets/bytes per scenario vs Paper), what causes p95 spikes, what profiling and benchmarks exist.

### Current state

Paths under `crates/pumpkin-core/src/` are written `core/`, as in the repo map. Vanilla paths are under `/srv/pex/vanilla-26.3/src/net/minecraft/`.

**Results on record.** G0-03's baseline (2026-11-03) is the first for `300621c3e`, and its raw samples, bot logs and load logs are kept in CT 117: see G0-03's Result. For the older results below no raw data survives (`find / -xdev -name samples.csv` in CT 117 found nothing before G0-03).

- `/srv/pex/gameplay-bench/README.md`, "Results (2026-10-14, one Pelican node, 4 CPU cores and 8 GB per server)", 8 bots for 2 hours. Network sent: PEX `bbc4cd07e` 959 MiB, Paper 26.3 #186 642 MiB (1.49x); in the second table PEX 923 MiB, Paper 614 MiB (1.50x). Average MSPT: PEX 8.4 and 7.1 ms, Paper 15.9 and 13.7 ms. Neither table has a P95.
- `/srv/pex/pr-drafts/comments-on-alpha-fix-report.md` (2026-10-15), item 15: PEX 1.0 (`74d1a6ad4`) against Paper 26.3, 2 h at 8 bots then 1 h at 24. Network sent 2,831 MiB against 1,959 MiB (1.45x); mean tick 6.3 against 12.4 ms (8 bots) and 12.0 against 20.5 ms (24 bots). Its list of new issues says PEX's 95th-percentile tick is about level with Paper's, with no number. The full report it links (claude.ai artifact `J6pegsAznB8U14Y72SmSpe`) could not be opened from this session. The same item says the 24-bot phase was lighter on PEX, because the harness could not build stations in unloaded chunks there.
- The earlier roadmap's P95 figures (`bench-game3`: PEX 17.7 ms, Paper 17.1 ms) cite reports in the pve archive and were not re-checked.

**No per-packet breakdown exists.** `grep -rIl -iE "per.packet|packet.type|packet_count|bytes_by|by_pid"` over `/srv/pex/tools`, `/srv/pex/gameplay-bench`, `/srv/pex/scratch` and `/srv/pex/pr-drafts` finds nothing relevant. The bots decode every packet ID (`bot/walkbot.py` `Conn.recv`) but count nothing. The sampler keeps only receive and transmit bytes from `/proc/<pid>/net/dev` (`sampler.py` `cg_stats`), not the transmitted packet count beside them.

**How the benchmark measures.**

- P95: `sampler.py` sends `tick query` every 30 s (`next_tick += 30`), and the answer covers the last 100 ticks (`core/command/commands/tick.rs` `send_percentiles` over `Server::tick_times_nanos`, `[i64; 100]`; vanilla also keeps 100). One sample covers 5 s of every 30 s, and `report.py` reports the mean of the samples' P95 (`mspt_p95 = statistics.fmean(allp)`).
- Network: `report.py` reports the run total (`net_tx_mib = tx[-1] - tx[0]`), not bytes per bot-hour, so dropped sessions change it.
- The bots ask for view distance 8 (the `ClientInformation` payload in `walkbot.py` and `survbot.py` `main`, `bytes([8])`) and 25 chunks per tick (`SB_PLAY_CHUNK_BATCH`, `25.0`). Both servers clamp to the client's 8 (`core/world/chunker.rs` `get_view_distance`, vanilla `ChunkMap.getPlayerViewDistance`), so G0-03's view distance 10 is a server cap the bots never reach.
- CT 117 has no Docker (`which docker podman` finds neither), and `sampler.py` `cgroup()` reads only Docker cgroups. `/srv/pex/tools/localbench.sh` runs PEX without Docker but records only `tick query`.
- `whitelist.py`'s `pumpkin` mode (SHA-256 UUIDs), which `SERVERS.md` says to use for PEX, no longer matches alpha, which derives vanilla's version-3 UUID (`core/net/mod.rs` `offline_uuid`).

**Network: code paths that send more than vanilla.** Each is a candidate until G9-01 measures it.

1. Player movement is broadcast per client packet. `core/net/java/play/player_position.rs` sends `CUpdateEntityPos` or `CUpdateEntityPosRot` to every watcher for each accepted movement packet, and a `CHeadRot` after every position-and-rotation packet even when the yaw did not change. `player_rotation.rs` sends `CUpdateEntityRot` and `CHeadRot` per rotation packet. Vanilla `ServerGamePacketListenerImpl.handleMovePlayer` broadcasts nothing: players go through `ServerEntity.sendChanges` like any entity, at `updateInterval(2)` (`world/entity/EntityTypes.java` `PLAYER`), and only send what changed. PEX's tracker skips players (`core/world/entity_tracker.rs` `EntityTracker::update_all`, `get_player().is_none()`). A bot that moves every tick costs each watcher about twice vanilla's movement packets, plus one `CHeadRot` each.
2. Those broadcasts mostly leave as separate TCP segments. Client packets are handled in each connection's task (`core/net/java/mod.rs` `JavaClient::progress_player_packets`), outside the tick, and "off-tick sends flush at once" (`core/net/java/outgoing.rs` `OutgoingPacket::ingest`, `FlushRequest::IfNotSuspended`). Flushing is held only during `Server::tick` (`core/server/mod.rs` `suspend_player_flushes`). With `TCP_NODELAY` (`core/lib.rs`, `set_nodelay(true)`) each small packet carries its own Ethernet, IP and TCP headers, which `/proc/net/dev` counts. Vanilla sends movement from the tick, inside `suspendFlushing`/`resumeFlushing` (`server/MinecraftServer.java`).
3. Mob head rotation every tick. `TrackedEntity::send_changes` (`core/world/entity_tracker.rs`) sends `CHeadRot` on every tick the head yaw changed, outside the update-interval check. Vanilla sends it inside that check (`server/level/ServerEntity.java` `sendChanges`, lines 202 to 206, in the block opened at line 133). Most mobs have interval 3, and the benchmark world has a chicken farm.
4. Compression level. PEX defaults to zlib level 4 at threshold 256 (`crates/pumpkin-config/src/networking/compression.rs` `CompressionInfo::default`), and generated `pumpkin.toml` files write `level = 4` (for example `/srv/pex/scratch/block-entity-bookkeeping/run-branch-bed/pumpkin.toml`). Vanilla uses `new Deflater()`, zlib's default level 6 (`network/CompressionEncoder.java`). The Paper install has `compression-level: default` (`/srv/pex/bins/paper/config/paper-global.yml`); what Paper maps that to was not checked in its code.
5. Sky light arrays. `light_data_from_chunk` (`core/net/java/chunk_data/light.rs`) sends a 2048-byte array for every section that `LightContainer::to_array` (`crates/pumpkin-world/src/chunk/format/mod.rs`) returns, which includes every section uniformly at level 15. Vanilla sends an array only for sections that have a `DataLayer` (`network/protocol/game/ClientboundLightUpdatePacketData.java` `prepareSectionData`). Uniform arrays compress well, so the cost on the wire is unknown, and whether vanilla keeps layers for open-sky sections was not checked.
6. Re-sent chunks. Every player hears about every chunk the scheduler publishes (`ChunkListener::process_new_chunk`), and `Player::tick` re-queues any that is in view (`core/entity/player.rs`, `sender.enqueue_chunk(pos)` in the listener loop). `ChunkSender::enqueue_chunk` (`core/net/chunk_sender.rs`) queues a chunk the client already holds for sending again. The scheduler republishes chunks that never left: an unload that finds a pending ticket (`crates/pumpkin-world/src/chunk_system/schedule.rs`, `has_pending_load`) and a regenerated chunk that "was already public". Vanilla queues a chunk for a player when it becomes ready or enters the view (`ChunkMap.markChunkPendingToSend`). Whether this happens in the benchmark is not known.
7. Less than vanilla: PEX never sends `CLightUpdate` (`grep -rn CLightUpdate crates/pumpkin-core/src` finds only its constructor in `light.rs`, and `World::flush_block_updates` has "TODO: Send light updates"). Whichever goal adds light updates adds bytes, so this goal's numbers must be measured again after it lands.

Already like vanilla: the tracker's update interval, 60-tick position refresh, 400-tick absolute sync and delta encoding (`TrackedEntity::pick_move`), block changes batched per section (`World::flush_block_updates`), and the view shape (`Cylindrical::is_within_distance` matches `ChunkTrackingView.isWithinDistance`).

**Tick: what is measured.**

- `Ticker::run` (`core/server/ticker.rs`) follows vanilla's loop: overload skip, then park until the deadline.
- `core/server/metrics.rs`, on by default with `slow_tick_ms = 100` (`crates/pumpkin-config/src/metrics.rs`): `metrics/summary.csv` per minute with average, P99, maximum and ticks over 50 ms (no P50 or P95), and `metrics/slow-ticks.log` with each world's chunks, players, entities, tracker and block-entity times (`WorldTickReport`). 100 ms is more than 10 times the 7.1 to 8.4 ms average tick in the README results, so ticks in the P95 range are never logged.
- Not timed: in `Server::tick_worlds`, everything outside `World::tick` (the `#minecraft:tick` functions, `plugin_manager.tick_loaders`, `scheduled_functions.tick`, `player_data_storage.tick`); `flush_pending_block_updates`; the game tests and the `ServerTickStartEvent`/`ServerTickEndEvent` `block_on` calls in `Ticker::run`; and in `World::tick`, the work before `t_chunks` (`flush_block_updates`, `update_active_chunks`, `tick_environment`, raids) and after block entities (`send_change`, the dragon fight). Island planning counts as "entities". Example: `/srv/pex/scratch/playtest-uuid/base-120/metrics/slow-ticks.log` (2026-10-27) has `tick=1785 total=152.0ms` with the overworld at 9.4 ms. Every slow-tick log under `/srv/pex/scratch` comes from a debug build (this one per `/srv/pex/scratch/playtest-uuid/BINARIES.txt`), so none of them says anything about release P95.
- Other tools: `/debug start` and `stop` give TPS only (`core/server/debug_profiler.rs`); `[profile.profiling]` in `Cargo.toml` (release with symbols, for `perf`); feature `console-subscriber` on `crates/pumpkin`. There are no tracing spans in tick code (`grep -rn "info_span!\|debug_span!\|#\[instrument"` finds none). Criterion benches exist only in `pumpkin-world` (chunk, chunk_io, chunk_gen, chunk_gen_concurrent, jigsaw, noise_router, template_pool), `pumpkin-nbt` (nbt), `pumpkin-util` (perlin) and `pumpkin-data` (has_random_ticks). None covers packet encoding, compression or a tick phase.

**Tick: spike candidates in the code.** None is measured.

- Chunk encoding in the players phase. `Player::tick` encodes its batch synchronously (`ChunkSender::encode_batch`) into a cache created per call (`per_player_cache`), so players in one area each encode the same chunk. `EncodedChunk::is_fresh_for` compares only the `Arc`, which misses in-place edits, and `ChunkData` has only the save flag `dirty`, so a shared cache needs a change counter. PEX allows 500 chunks per tick (`MAX_CHUNKS_PER_TICK` in `core/net/chunk_sender.rs`), vanilla 64 (`server/network/PlayerChunkSender.java`), and the Paper install caps sends at 75 chunks per second (`player-max-chunk-send-rate` in `paper-global.yml`). The fleet teleports each bot at session start, which sends a whole view at once.
- One Rayon pool for tick and background work. The tick's `par_iter` calls (`World::tick` players, islands and block entities, `World::tick_chunks`, `Server::tick_worlds`) run on the global pool built in `crates/pumpkin/src/main.rs`. The same pool runs region decoding (`crates/pumpkin-world/src/chunk/io/file_manager.rs`, `rayon::spawn` with `into_par_iter`), entity chunk loads (`Level::spawn_entity_generation`), player saves (`ServerPlayerData::tick`) and `JavaClient::send_chunks`. Only generation has its own pool (`ChunkGen-N` in `schedule.rs`, half the CPUs, 2 to 16). The ticker thread is not a pool worker, so its `par_iter` waits behind queued jobs.
- Periodic work. Every 100 ticks `World::tick_environment` runs `Level::clean_memory` on the tick, which can `shrink_to_fit` three DashMaps. Every `save_player_cron_interval` (300 s, `crates/pumpkin-config/src/player_data.rs`) `ServerPlayerData::tick` writes every online player's NBT on the tick thread. Every `autosave_ticks` (6000) the Schedule thread runs `save_all_chunk` (`schedule.rs` `work`). Every 60 s the `pex-malloc-trim` thread calls `malloc_trim(0)` (`core/server/memory.rs`), which locks each glibc arena while it trims.
- Entity islands. `IslandSchedule::should_plan` (`core/world/entity_islands.rs`) plans again whenever the entity count changes and re-probes a serial plan every 20 ticks (`SERIAL_PROBE_INTERVAL`). Eggs and item pickups change the count often.

Unmerged branches on tick cost and memory: `perf/memory` (3 commits: uniform light sections, packed idle proto chunks, two malloc arenas), `perf/pathfinder-heap` (1), `perf/entity-box-query` (1), `perf/region-write-in-place` (2, the second reverts the first). None touches network traffic or tick timing.

### Pieces

#### G9-01 Packet and segment breakdown in the gameplay benchmark

- Scope: in: `Conn.recv` in `bot/walkbot.py` counts packets, wire bytes and decompressed bytes per clientbound play packet ID and writes them per bot every 60 s; it records bytes per `level_chunk_with_light` by chunk position and counts chunk packets for a position the bot already holds (no `forget_level_chunk` in between). `sampler.py` adds transmitted packets from `/proc/<pid>/net/dev` to `samples.csv`. `report.py` adds a table per server: packet type, count and bytes per bot-hour, PEX/Paper ratio, plus segments per bot-hour. One 60-minute 8-bot run of PEX at the G0-03 commit and of Paper 26.3-169, with the top 10 packet types by excess bytes copied into Current state above. Out: fixes, server-side counters in PEX.
- Depends on: G0-03
- Branch: feat/packet-breakdown (in `Conman1533/mc-gameplay-bench`, or in PEX after G9-11)
- Needs decision: none (D9-2 only decides the repository)
- Done when: on both servers, bot-side wire bytes plus 66 bytes per transmitted packet (Ethernet, IPv4, TCP with timestamps) account for the sampler's transmitted bytes within 5%, and the top-10 table is in this section.

#### G9-02 Tick phase attribution in `metrics/`

- Scope: in: time the parts listed as "Not timed" above, as named phases in `WorldTickReport` and in a new server-level report (island planning split from entity ticking); add P50 and P95 over every tick of the minute to `summary.csv`; add an `unattributed=` field to `slow-ticks.log`; write background events with timestamps (chunk autosave start and end, `malloc_trim`, player data snapshot, region read batches) to `metrics/events.log`. Out: moving work out of the tick, tracing spans, new config keys.
- Depends on: none
- Branch: feat/tick-phase-metrics
- Needs decision: none
- Done when: a unit test in `core/server/metrics.rs` checks `summary_line`'s P50, P95 and P99 against a hand-computed window, and in a 30-minute `/srv/pex/tools/localbench.sh` run with `slow_tick_ms = 20`, `unattributed` stays under 10% of `total` on every logged tick.

#### G9-03 Tick spike breakdown against Paper's P95

- Scope: in: with G9-02 on alpha, run the G0-03 scenario twice on a release build with `slow_tick_ms` set to Paper's G0-03 P95, and table the causes (phase, matching background event) by share of the ticks above that line and of their excess time. Add a fix piece to this section for every cause above 10% that G9-09 and G9-10 don't cover. Out: code changes.
- Depends on: G0-03, G9-02
- Branch: docs/tick-spike-breakdown
- Needs decision: none
- Done when: the table is in this section and both runs rank the same top three causes.

#### G9-04 Player movement goes through the entity tracker

- Scope: in: other Java players get a player's movement, rotation and head rotation from `TrackedEntity::send_changes` at the player type's interval of 2, with vanilla's change checks, inside the tick. The per-packet broadcasts in `core/net/java/play/player_position.rs`, `player_rotation.rs` and the Java packets in `core/net/bedrock/play/player_auth_input.rs` go. Corrections back to the moving player (`sync_position`) stay. Out: what Bedrock watchers receive (Goal 8), anti-cheat.
- Depends on: G9-01
- Branch: fix/player-move-tracking
- Needs decision: none
- Done when: a new tester bot test `bot:player_move_broadcast_interval` has bot A send one position packet per server tick for 40 ticks while bot B watches, and B receives 19 to 21 movement packets for A and no `CHeadRot` while A's yaw is fixed (40 and 40 before the fix). A G9-01 rerun shows movement packets per bot-hour within 10% of Paper's. A human checks with two real clients that other players still move smoothly.

#### G9-05 Mob head rotation on the update interval

- Scope: in: `TrackedEntity::send_changes` sends `CHeadRot` only inside the interval, `needs_sync` or dirty-data block, as `ServerEntity.sendChanges` does. Out: Bedrock head yaw (`send_bedrock_move`), other tracker changes.
- Depends on: G9-01
- Branch: fix/head-rot-interval
- Needs decision: none
- Done when: a new tester bot test `bot:mob_head_rot_interval` keeps a pig with AI next to a bot for 300 ticks and the bot receives at most 100 `CHeadRot` for it, plus one per `CSetEntityMetadata` or `CEntityVelocity` it also received (the test fails before the fix), and a G9-01 rerun shows `rotate_head` bytes per bot-hour within 10% of Paper's.

#### G9-06 Held chunks are not sent again on republish

- Scope: in: a chunk the scheduler republishes without unloading it (the `has_pending_load` path and the regenerated-while-public path in `schedule.rs`) is not queued again for players who hold it, following `ChunkMap.markChunkPendingToSend`. Out: other scheduler changes. Drop the piece if G9-01 counts no re-sent chunks on PEX.
- Depends on: G9-01
- Branch: fix/held-chunk-resend
- Needs decision: none
- Done when: a regression test in `crates/pumpkin-world/src/chunk_system/tests.rs` or the tester reproduces the republish and fails before the fix, and a G9-01 rerun counts the same re-sent chunks per bot-hour on PEX as on Paper.

#### G9-07 Chunk packet size matches Paper per chunk

- Scope: in: compare decompressed `level_chunk_with_light` bytes per chunk position from G9-01 (same world and positions on both servers, so compression level drops out) and fix what makes PEX's larger in `core/net/java/chunk_data`, starting with the uniform sky light arrays; add a criterion bench of `CChunkData` encoding (bytes and time) over chunks from the benchmark world. Out: compression level (G9-08), the chunk storage format.
- Depends on: G9-01
- Branch: perf/chunk-packet-size
- Needs decision: none
- Done when: the median decompressed bytes per position are within 5% of Paper's, or the PR explains the rest; the new bench shows encode time no worse; a human checks lighting on those chunks with a real client.

#### G9-08 Default network compression level

- Scope: in: `CompressionInfo::default().level` follows D9-4, and existing `pumpkin.toml` files that still hold the old default move to the new one; the PR quotes compressed bytes and time per chunk packet at levels 4 and 6 from G9-07's bench. Out: changing the compression library.
- Depends on: G9-07, G3-03 (the step that rewrites a key still at its old default takes a `config_version` after G3-03's)
- Branch: perf/compression-level
- Needs decision: D9-4
- Done when: a migration test turns `level = 4` into the new default and leaves a non-default level alone, and a G9-01 rerun shows the change in chunk-packet wire bytes per bot-hour.

#### G9-09 Chunk encoding out of the players phase

- Scope: in: each chunk is encoded once per change and shared between players (a change counter on `ChunkData` that `EncodedChunk::is_fresh_for` checks), and encoding leaves `Player::tick`, so the tick only queues ready payloads while `CChunkBatchStart`, the chunks, `CChunkBatchEnd` and `pair_entities_in_chunks` stay in order. Out: send-rate limits, Bedrock. Drop the piece if G9-03 ranks chunk sending under 10% of the excess time.
- Depends on: G9-03
- Branch: perf/chunk-encode-off-tick
- Needs decision: none
- Done when: a unit test shows that an edit to a cached chunk changes its encoded payload; a G9-03 rerun shows the players phase's share of slow ticks at least halved, with mean tick and P95 no worse over 2 runs; a human joins with a real client and sees no missing chunks or entities.

#### G9-10 Background work off the tick's Rayon pool

- Scope: in: region decoding, entity chunk loads, player saves and `JavaClient::send_chunks` run on a pool of their own, like `ChunkGen`, so the tick's `par_iter` calls don't queue behind them. Out: the generation and lighting pools. Drop the piece if G9-03 doesn't tie slow ticks to these jobs.
- Depends on: G9-03
- Branch: perf/tick-rayon-pool
- Needs decision: none
- Done when: a G9-03 rerun shows the matching cause under 10% of slow ticks, mean tick and P95 are no worse over 2 runs, and the `chunk_io` bench in `pumpkin-world` moves less than 5%.

#### G9-11 Gameplay benchmark in the repository

- Scope: in: if D9-2 says so, `tools/gameplay-bench` with the harness and its MIT `LICENSE`, the world tarball handled as D9-2 says, `whitelist.py` with the vanilla UUID mode for PEX, and `SERVERS.md` updated. Out: new scenarios and fleet sizes (G9-12).
- Depends on: none
- Branch: test/gameplay-bench-in-tree
- Needs decision: D9-2
- Done when: following its README, a 10-minute run against PEX and Paper produces a report with both servers' rows, and the files in `tools/gameplay-bench` match `Conman1533/mc-gameplay-bench` at a named commit except for the listed changes.

#### G9-12 Larger fleets with equal work

- Scope: in: the fleet sizes D9-1 picks beyond 8, with more stations, all built under `forceload` on every server (the 24-bot phase on 2026-10-15 lost stations on PEX, `comments-on-alpha-fix-report.md` item 15), and report rows for placed blocks, chests opened and dropped sessions. Out: new bot roles.
- Depends on: G9-11 (or the bench repository, per D9-2)
- Branch: test/bench-fleet-sizes
- Needs decision: D9-1
- Done when: at each size, a 30-minute run builds every station on both servers, and placed blocks and chests opened are within 5% between PEX and Paper (block breaks are confirmed differently, `README.md` "Things to know").

#### G9-13 Traffic budget test

- Scope: in: a tester bot test that runs a fixed 600-tick scenario (two bots walking and building near a few mobs) and fails when bytes received per packet type exceed a checked-in budget by more than 10%; the budget is recorded after G9-04 and G9-05. Out: tick-time checks, which need release builds.
- Depends on: G9-04, G9-05, G0-01
- Branch: test/traffic-budget
- Needs decision: none
- Done when: the test passes on its branch, fails on a scratch branch that reverts G9-05, and runs in the `gametest` job on an `alpha` push.

#### G9-14 Published benchmarks

- Scope: in: the page D9-3 picks, with method, hardware (D9-5), versions, commands and config differences, PEX and Paper rows in the G0-03 format for every D9-1 size, the G9-01 packet table and the G9-03 spike table, plus the HTML reports and raw CSVs. Out: automatic publishing from CI.
- Depends on: G0-03, G9-01, G9-03, G9-04, G9-05, G9-12 if D9-1 adds sizes, and every fix piece G9-01 and G9-03 keep
- Branch: docs/benchmarks
- Needs decision: D9-1, D9-3, D9-5
- Done when: the page is published with at least 2 runs per server and size on D9-5's hardware, and another session reproduces one PEX row within 10% using only the page.

### Proof

- G9-14's page, run on D9-5's hardware: PEX at the release commit against the Paper 26.3 build it names, G0-03 scenario (60 minutes, 8 bots, at least 2 runs each, run-to-run P95 spread under 10%), plus any sizes D9-1 adds. Targets until D9-1 decides otherwise:
  - network bytes sent per bot-hour at or below Paper's (1.45x to 1.50x in the 2026-10-14 and 2026-10-15 results);
  - tick P95 at most 1.10 times Paper's;
  - mean tick and peak memory no worse than G0-03's PEX row.
- A G9-01 rerun on the same commit: no packet type above 1.2 times Paper's bytes per bot-hour unless the page gives the reason.
- G9-13 passes on `alpha` in CI.

### Decisions

- D9-1: What is the bar against Paper, and at which sizes? Options:
  - 8 bots only, the G0-03 scenario: bytes per bot-hour at or below Paper's and P95 at most 1.10 times Paper's. Cheapest: no fleet work, G9-12 is dropped.
  - 8 and 24 bots with the same bar. Needs G9-12 and about 2 more run-hours per server per comparison.
  - The earlier roadmap's answer (Q9.2, 2026-10-29): less RAM than Paper and within 10% of Paper's P95 at 10, 25 and 50 bots for 30 minutes each. It sets no network bar, and 50 bots needs about six times the stations of today's fleet.
  - Waits on it: G9-12, G9-14, Proof.
  - Answered 2026-10-31 (Neon accepted Q9.2): the third option. PEX uses less RAM than Paper and stays within 10% of Paper's P95 at 10, 25 and 50 bots for 30 minutes each, so G9-12 is needed. The bar sets no network number; the traffic pieces keep their own targets.
- D9-2: Where does the benchmark live? Options:
  - Move it to `tools/gameplay-bench`, which the earlier roadmap's answer chose (Q9.3, 2026-10-29: under `tools/`, if the license can match). MIT code can ship in the GPL-3.0 repository if its notice stays. Costs: the 6 MB world tarball goes into git or becomes a checksummed download, and `AGENTS.md` ("Keep investigation tools out of the repo") needs a line that names it as kept tooling.
  - Keep it in `Conman1533/mc-gameplay-bench` and pin a commit in the benchmark page. Costs: two repositories to update on every protocol bump, and no PR can change server and harness together.
  - Waits on it: G9-11, and which repository G9-01 and G9-12 land in.
  - Answered 2026-10-31 (Neon accepted Q9.3): move it to `tools/gameplay-bench`, keeping its MIT notice. G9-11 goes ahead. G9-01 and G9-12 land in this repository once G9-11 has moved it, and in `mc-gameplay-bench` before that.
- D9-3: Where are benchmark results published? Options:
  - `docs/BENCHMARKS.md` in PEX, with the HTML reports and CSVs attached to the GitHub release they measure. No new infrastructure; assets are uploaded by hand until Goal 10's release jobs run.
  - A GitHub Pages site from the repository. Costs a Pages workflow, which needs Actions running (G0-01).
  - The mc-gameplay-bench README, where the 2026-10-14 results already are. Costs nothing, but nothing in PEX's releases or README points there.
  - Waits on it: G9-14.
- D9-4: Which default Java compression level? Options:
  - Keep 4. No change; more bytes per chunk, less CPU.
  - 6, like vanilla's `new Deflater()`. Fewer bytes, more CPU per compressed packet (G9-07's bench gives both numbers). Existing servers keep 4 unless the G3 migration rewrites untouched defaults.
  - Let G9-08's bench decide: switch to 6 only if it saves at least 5% of chunk-packet bytes for at most 20% more compression time.
  - Waits on it: G9-08.
- D9-5: Which hardware produces the published numbers? Options:
  - A dedicated 4-core cloud VM, the earlier roadmap's answer for release numbers (Q9.1, 2026-10-29; pve was fine for development comparisons run back to back). Costs money per run-hour and a Docker setup per run.
  - CT 117 on pve. Free, but it shares the host with production (Pelican CT 109) and has no Docker, which `sampler.py` needs.
  - A Pelican node like the 2026-10-14 runs. Real hosting conditions, but the runs compete with players' servers.
  - Waits on it: G9-14. G0-03 and the development reruns in G9-01 to G9-10 can run on any host if PEX and Paper run there back to back.
  - Answered 2026-10-31 (Neon accepted Q9.1): a dedicated 4-core cloud VM for the published numbers. CT 117 or pve is fine for development comparisons that run PEX and Paper back to back.

## Goal 10: Reproducible release builds in CI, ARM64/Windows on real hardware

Reproducible release builds in CI, ARM64/Windows run on real hardware.

Start from: CI workflows, toolchain pinning, Cargo.lock, anything that makes builds differ (paths, timestamps, build scripts), and what ARM64/Windows hardware exists (a Decision if unknown).

### Current state

CI and runners:

- No workflow has run since 2026-10-14 (G0 CI row, G0-01). Once `rust.yml` runs again, every event except `pull_request` tests on five hosted runner types: `.github/scripts/ci_packages.py` `plan` picks `FULL_TEST_MATRIX` (`ubuntu-latest`, `ubuntu-26.04-arm`, `windows-latest`, `windows-11-arm`, `macos-latest`), and `rust.yml` `build_release` builds on the same five. `gametest` runs on `ubuntu-latest` only. `test_android` runs `cargo ndk-test -t arm64-v8a` on an emulator with `arch: x86_64`. All of these are GitHub-hosted VMs; nothing runs on owned ARM64 or Windows hardware.

Toolchain, lockfile, profile:

- `rust-toolchain.toml` has `channel = "stable"`. Inside the checkout it wins over `rustup default` (`rustup show active-toolchain` in the worktree prints `stable-x86_64-unknown-linux-gnu (overridden by '.../rust-toolchain.toml')`), so `rust.yml`'s `rust_toolchain_version` input (`CI_RUST_TOOLCHAIN_VER`) and its `rustup default` steps don't change the compiler. Every job builds with the stable of that day, including `release.yml` `build_release` (`toolchain: [stable]`), `nightly.yml` `build` (`rustup update stable`) and `release.yml` `build_android_release` (`dtolnay/rust-toolchain@stable`). CT 117's stable is rustc 1.99.1 (58cf8f9c3, 2026-10-13); `Cargo.toml` has `rust-version = "1.96"`. All three workflows set `RUSTFLAGS: "-Dwarnings"`, so a newer stable that adds a warning makes an old tag fail to build.
- `Cargo.lock` matches the manifests (`cargo metadata --locked --offline` succeeds). `--locked` is only on the Android builds (`grep -n locked .github/workflows/*.yml`). The server builds in `rust.yml` `build_release` and `build_release_musl`, `release.yml` `build_release` and `nightly.yml` `build` would rewrite a stale lockfile instead of failing. `cargo install cargo-ndk` takes the newest version. `tools/pumpkin-codegen/Cargo.lock` is dead: the crate is a workspace member, so cargo uses the root lockfile.
- `Cargo.toml` `[profile.release]`: fat LTO, `codegen-units = 1` (16 for eight large crates), `strip = "debuginfo"`. There is no `.cargo/config.toml`. `nightly.yml` `build` and `build_android` use thin LTO on the ARM64 and Android jobs because fat LTO "runs these runners out of memory at the final link"; `release.yml` has no such override for its ARM64 Linux and Android jobs.

Build scripts (`git ls-files | grep build.rs` finds two):

- `crates/pumpkin-core/build.rs` sets `GIT_HASH` and `GIT_HASH_FULL` from `git rev-parse` ("unknown" without git) and `PUMPKIN_BUILD_ID` from `build_id`: commit, rustc version and hash, target, first 8 bytes of the `Cargo.lock` SHA-256, the crate's features, debug assertions. No timestamp and no dirty-tree flag, so a build from a modified tree carries the clean commit's id. `loader/native.rs` `check_build_id` compares `plugin::BUILD_ID` (`plugin/mod.rs`) with each plugin's exported `PUMPKIN_BUILD_ID`, so a server and plugins rebuilt from the same inputs keep loading together.
- `crates/pumpkin-data/build.rs`: when `assets/datapack/data/minecraft` is missing, `download_server_jar` fetches the 26.3 jar through piston-meta (or `ensure_datapack` reads `PUMPKIN_MINECRAFT_SERVER_JAR`) and `extract_datapack` writes its `data/` into the source tree at `assets/datapack/` (gitignored). No hash is checked; the manifest's `sha1` field is never read. An existing directory is reused without checking which version it holds. The extracted files are compiled into the binary by `include_str!` in `crates/pumpkin-world/src/world_info/mod.rs` (`WorldPreset::NORMAL_RAW` and six more) and in the generated `context_provider.rs` (30 references), `enchantment_provider.rs` (7) and `test_instance.rs` (1). Every CI release build therefore needs piston-meta to be up and trusts an unchecked download.

Paths and timestamps:

- `git grep -w` for `SOURCE_DATE_EPOCH`, `remap-path-prefix`, `trim-paths` and `vergen` finds nothing. A release-profile server binary in CT 117 (`/srv/pex/target/hotfix-34/x86_64-unknown-linux-gnu/release/pumpkin`, commit 69a956092 of 2026-10-27, same `[profile.release]`) holds 1,104 strings under `/root/.cargo/registry/src/` (panic locations in dependencies), 8 under its target dir (generated `OUT_DIR` sources of `cranelift-codegen`, pulled in by wasmtime, and of `rustc_version_runtime`, used in `crash.rs`) and 77 `/rustc/<hash>/` paths, which are fixed per toolchain. Workspace files appear as relative `crates/...` paths and the checkout path does not appear. So a different `CARGO_HOME` or target dir changes the binary; a different checkout path alone should not (not tested).
- Probe: two debug builds of `pumpkin-codegen` (`cargo build --locked -j 4`, rustc 1.99.1, 35 s each) from the same checkout into two target dirs gave the same SHA-256 (`1dd83e96...`). Its crate graph has no `OUT_DIR` source with panic paths, so this does not test the leak above; it shows that its build scripts and proc macros (serde, rayon-core, proc-macro2) give the same output on two runs. That binary also holds 122 `CARGO_HOME` paths and its checkout path (`env!("CARGO_MANIFEST_DIR")` in `tools/pumpkin-codegen/src/main.rs`); it isn't shipped.
- No build script reads the clock. Plugin zips are made with `zip -9` (`release.yml` step `Package native plugins`, `7z a` on Windows; `/srv/pex/tools/build-release.sh` `zip -q -9`), which stores each file's modification time, so the zip hash changes on every build even when the libraries are identical.

Codegen:

- Generated code is committed (88 files in `crates/pumpkin-data/src/generated/`), builds don't run codegen, and no workflow regenerates and diffs it (`grep -in codegen .github/workflows/*.yml` finds only `release_codegen_units`). `fix/codegen-deterministic` (sorted NBT compound keys, rustfmt on the SDK output) is in 300621c3e. Reading the 55 `HashMap`/`HashSet` lines in `tools/pumpkin-codegen/src` found lookups and seen-sets; the one set that becomes output (`tag.rs` `all_registry_keys`) is collected into a `BTreeSet` (`EnumCreator::values`). Codegen was not run twice here: it needs `assets/datapack` and writes into the source tree.
- `crates/pumpkin-api-macros` `plugin_impl` emits methods in `HashMap` order (`PLUGIN_METHODS.values()`), which can change from one compiler run to the next. The bundled plugins don't use it (`native-plugins/AGENTS.md`: use `pex_common::export_plugin!`).

Publishing:

- `release.yml` runs on every tag except `nightly*` and `canary*`. A tag named `alpha` exists (`gh api repos/Conman1533/PEX/git/matching-refs/tags/alpha`), so moving it would publish "Release alpha". `build_release` has no `fail-fast: false`. `publish_release` names the checksum file `checksums.sha256`; hand releases use `SHA256SUMS`. `sign_windows_release` needs four `SIGNPATH_*` secrets; `gh api repos/Conman1533/PEX/actions/secrets` returns an empty list for Neon's token, which has push but not admin, so whether they exist is unknown.
- `nightly.yml` `draft_release` sets `prerelease: false` and `make_latest: true`; its first run would replace v1.2.1 as Latest with server binaries only, no plugin zips or checksums.
- `Dockerfile` downloads `pumpkin-<arch>-Linux-musl` from release `PUMPKIN_TAG` (default `nightly`) with no checksum, and `docker.yml` `docker-build-deploy` passes no build args, so an image tagged with a commit (`type=sha`) holds whatever the nightly was at the time.
- `flake.nix` pins the jar (`fetchurl`; its SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c` and SHA-256 match CT 117's `/srv/pex/vanilla-26.3/bundler-26.3.jar`), builds only `--package pumpkin` with nixpkgs' rustc and thin LTO, and builds from `lib.cleanSource ./.` without git, so its `GIT_HASH` and build-id commit are "unknown".
- Releases (`gh release list`, `gh release view`): v1.2.1 (2026-10-29) has `pumpkin-X64-Linux`, `pumpkin-X64-Linux-musl`, `pumpkin-plugins-X64-Linux.zip` and `SHA256SUMS`. Its notes say it was built outside Actions with rustc 1.99.1 in a Debian 12 container and has no ARM64, Windows or Android build. That container recipe is not in CT 117 (`grep -rl -E "bookworm|debian:12|SHA256SUMS" /srv/pex` outside the repo, worktrees and target dirs finds only PR drafts). v1.2.0 also had `pumpkin-ARM64-Linux`, `pumpkin-X64-Windows.exe` and their plugin zips. The `nightly` pre-release (2026-10-13) has eight server assets, including ARM64 Windows, macOS and Android, and no plugin zips.

Local tooling (outside the repo):

- `/srv/pex/tools/build-release.sh` builds X64 glibc, X64 musl, ARM64 glibc and Windows x64 GNU (plugin zips for all but musl) with fat LTO, without `--locked` or path remapping. It expects `/srv/pex/wt/rel1` (missing), `aarch64-linux-gnu-gcc` and `x86_64-w64-mingw32-gcc` (neither installed, `which`), so it doesn't run in CT 117 as written. CT 117 has only the `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl` targets (`rustup target list --installed`), `musl-gcc`, and `cargo-zigbuild` with `/srv/pex/tools/zig` (zig 0.14.1); no qemu-aarch64, wine, docker or podman.
- `/srv/pex/tools/release-smoke.sh` boots a binary under `unshare -n`, pings it, stops it and prints log lines. It has no pass/fail exit code and runs on Linux only. gameplay-bench's `sampler.py` reads `/proc`, so its CPU and memory numbers are Linux only.

Platform-specific code a hardware run has to reach:

- Windows: region IO through `seek_read`/`seek_write` (`crates/pumpkin-world/src/chunk/io/mod.rs` `read_exact_at`, `write_all_at`), `.dll` plugins with `can_unload` false (`loader/native.rs`), and Ctrl-C as the only shutdown signal off Unix (`crates/pumpkin/src/main.rs` `setup_sighandler`). ARM64 glibc runs the glibc-only memory trimmer (`server/memory.rs` `spawn_trimmer`); musl and Windows don't.

Hardware:

- Nothing in the repo names an ARM64 or Windows machine (`git grep -i -E "raspberry|graviton|self-hosted|snapdragon"` finds only D0-1's text), and `gh api repos/Conman1533/PEX/actions/runners` is 403 for Neon's token. Neon's notes (unverified, machines not contacted) list a Pixel 7a phone (aarch64, Termux and proot), an armv7 OpenWrt router (not ARM64) and no Windows machine. See D10-1.

### Pieces

#### G10-01 Pin the toolchain and lock every build

- Scope: in: `rust-toolchain.toml` pinned to an exact release (1.99.1, the compiler of CT 117 and v1.2.1); `--locked` on every cargo command that resolves dependencies in `rust.yml`, `release.yml` and `nightly.yml`; `cargo install cargo-ndk --locked --version <x>`; drop the `rust_toolchain_version` input and the `rustup update`/`rustup default` steps that the toolchain file overrides anyway; the same `--locked` flags in `/srv/pex/tools/local-ci.sh` (outside the repo). Out: path remapping (G10-03), build containers (D10-2), any toolchain update policy beyond "a bump is its own PR".
- Depends on: none
- Branch: ci/pin-toolchain
- Needs decision: none
- Done when: `rustup show active-toolchain` in the checkout prints `1.99.1-x86_64-unknown-linux-gnu (overridden by ...)`; `grep -nE '\bcargo (build|clippy|nextest|test|run|install|ndk)' .github/workflows/*.yml | grep -v -- --locked` prints nothing; `actionlint` passes; `local-ci.sh --full` on the branch gives the same results as on alpha.

#### G10-02 Check the vanilla jar the build downloads

- Scope: in: `crates/pumpkin-data/build.rs` checks the jar it downloads or reads from `PUMPKIN_MINECRAFT_SERVER_JAR` against a SHA-1 constant next to `MC_VERSION` (`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, the jar `flake.nix` pins) and fails on a mismatch; it writes a marker with version and SHA-1 into `assets/datapack/` and extracts again when the marker is missing or different. Out: moving the extraction into `OUT_DIR` (every `include_str!` path into `assets/datapack` would change), caching the jar in CI.
- Depends on: none
- Branch: fix/datapack-jar-hash
- Needs decision: none
- Done when: with `PUMPKIN_MINECRAFT_SERVER_JAR` pointing at a copy of the jar with one byte changed, `cargo build --locked -p pumpkin-data` fails with an error naming the expected and actual SHA-1; an `assets/datapack/` whose marker names another version is extracted again (build warning line); `nix build .#default` still passes (CI `nix.yml` after G0-01, or any host with Nix).

#### G10-03 Release build script with path-independent output

- Scope: in: one script in the repo, `tools/release/build.sh`, that builds the assets of one target the way `/srv/pex/tools/build-release.sh` does (server and plugins in one cargo run, asset names as in v1.2.0 and v1.2.1), with `--locked`; `--remap-path-prefix` for `CARGO_HOME` and the target dir (the two path sources in today's binary), appended to `RUSTFLAGS` after `-Dwarnings` (a `.cargo/config.toml` `rustflags` entry would be ignored, since CI and `env.sh` set `RUSTFLAGS`); the per-target LTO setting `nightly.yml` uses; plugin zips with sorted entries dated at the commit time, written by one tool on every OS; `SHA256SUMS`; and a refusal to build from a dirty tree (the build id's `commit=` can't show local changes). It runs inside a pinned container image if D10-2 picks that. Out: workflow changes (G10-04 to G10-06), Windows and macOS byte identity (D10-2).
- Depends on: G10-01, G10-02
- Branch: ci/release-build-script
- Needs decision: D10-2
- Done when: two runs in CT 117 at one commit, from two checkout paths with two `CARGO_HOME`s and two target dirs, give identical SHA-256 for `pumpkin-X64-Linux`, `pumpkin-X64-Linux-musl` and `pumpkin-plugins-X64-Linux.zip`; `strings -n 8 pumpkin-X64-Linux | grep -cE '/root/\.cargo|/home/|/srv/pex/'` prints 0; a run on a tree with an uncommitted change exits nonzero before building.

#### G10-04 Reproducibility check in CI

- Scope: in: `.github/workflows/reproducible.yml` (`workflow_dispatch` and `workflow_call`): two parallel jobs run G10-03's script for each D10-2 target in different checkout paths and `CARGO_HOME`s, and a third compares the SHA-256s, fails on any difference and uploads a `diffoscope` report of the differing file. Out: targets D10-2 leaves out, wiring it into the release (G10-05).
- Depends on: G10-03, G0-01
- Branch: ci/reproducibility-check
- Needs decision: D10-2
- Done when: the workflow passes on a dispatch at alpha; on a scratch branch whose `crates/pumpkin-core/build.rs` appends the current time to `GIT_HASH`, it fails and the report names `pumpkin-X64-Linux`.

#### G10-05 Release workflow for the fork

- Scope: in: `release.yml` triggers on `v*` tags only; `fail-fast: false`; every build job runs G10-03's script on its native runner, so each target that loads plugins ships its own plugin zip; `SHA256SUMS` replaces `checksums.sha256`; G10-04 runs as a called workflow and gates publishing; `sign_windows_release` follows D10-3; a `dry_run` dispatch input builds everything and publishes nothing; the target list and runners follow D10-5. Out: nightly and Docker (G10-06), smoke tests (G10-07).
- Depends on: G10-03, G10-04, G0-01, G2-05 (D2-1 keeps plugins tied to one exact build, which is why every target ships its own plugin zip)
- Branch: ci/fork-release-workflow
- Needs decision: D10-3, D10-5
- Done when: a `dry_run` dispatch at alpha builds every asset on the D10-5 list plus `SHA256SUMS` and creates no tag or release (`gh release list` unchanged); with D10-2's container option, its x86_64 Linux lines equal a G10-03 rebuild outside GitHub; `actionlint` passes.

#### G10-06 Nightly and Docker image from checked assets

- Scope: in: `nightly.yml` publishes per D10-4 (a pre-release that is never Latest, or nothing) and builds through G10-03's script, so a nightly has plugin zips and `SHA256SUMS`; `Dockerfile` takes the release tag and the musl asset's SHA-256 as build args and fails on a mismatch; `docker.yml` builds for the release or nightly it belongs to and passes both args. Out: `release.yml` (G10-05), reproducible image layers.
- Depends on: G10-05
- Branch: ci/nightly-docker-from-release
- Needs decision: D10-4
- Done when: a nightly dispatch publishes a pre-release while `gh release list` still shows the newest `v*` release as Latest; `docker build --build-arg PUMPKIN_TAG=v1.2.1 --build-arg PUMPKIN_SHA256=<wrong>` fails, and the right value builds an image whose `/usr/local/bin/pumpkin` SHA-256 equals its `SHA256SUMS` line.

#### G10-07 Release smoke test on every runner

- Scope: in: `tools/release/smoke.py` (Python, so one script runs on Linux, macOS and Windows), replacing `/srv/pex/tools/release-smoke.sh`: start an asset in a scratch dir with its plugin zip unpacked, wait for the startup line, status-ping it, check each bundled plugin logged as loaded, send `stop` on stdin, start it again and check the saved chunks load without errors (on Windows this reaches `seek_read`/`seek_write`), and exit 1 on a panic or ERROR line, a timeout or a missing plugin. A `release.yml` job runs it on each native runner after the build. Out: bot play and benchmarks (G10-09, G10-10).
- Depends on: G10-03, G10-05
- Branch: ci/release-smoke
- Needs decision: none
- Done when: in CT 117 the script passes on `pumpkin-X64-Linux` with its zip and on `pumpkin-X64-Linux-musl` without one, and exits 1 when given a plugin zip from another commit (build id refused); a `dry_run` dispatch shows the smoke job passing on every runner D10-5 keeps (`ubuntu-latest`, `ubuntu-26.04-arm`, `windows-latest`, `windows-11-arm`, `macos-latest` under its first option).

#### G10-08 GameTests on ARM64 and Windows runners

- Scope: in: `rust.yml` `gametest` takes the `test` job's matrix on pushes (`FULL_TEST_MATRIX`), so pushes also run it on Linux ARM64, Windows x64 and Windows ARM64; pull requests keep `ubuntu-latest`. Out: fixing failures it finds (the goal that owns the area takes them).
- Depends on: G0-01
- Branch: ci/gametest-matrix
- Needs decision: D0-1 (only its hosted-runner option has ARM64 and Windows runners)
- Done when: a push to alpha shows `GameTest and Bot Harness` passing on Linux ARM64, Windows x64 and Windows ARM64, with no failure that the Linux x64 job of the same run doesn't also have.

#### G10-09 ARM64 run on real hardware

- Scope: in: on the D10-1 ARM64 machine, the tagged `pumpkin-ARM64-Linux` with its plugin zip (and `pumpkin-aarch64-android` if the machine is the phone): G10-07's smoke script, then G0-03's gameplay-bench scenario (same seed, 8 bots, view distance 10) for 60 minutes, recorded in this file next to G0-03's rows: tick P50/P95/P99, CPU, peak RSS, network bytes sent, panic and ERROR count, and where the bots ran. Out: performance fixes (Goal 9), tuning for the machine.
- Depends on: G10-07, G0-03
- Branch: docs/arm64-hardware-run
- Needs decision: D10-1
- Done when: the row is in this file from one 60-minute run, with the machine named (CPU, RAM, OS, kernel), zero panic or ERROR lines, all three plugins logged as loaded, and a restart that reloads the world.

#### G10-10 Windows run on real hardware

- Scope: in: as G10-09, for `pumpkin-X64-Windows.exe` and its plugin zip on the D10-1 Windows machine (`pumpkin-ARM64-Windows.exe` if that machine is ARM64), with CPU and memory sampled by a Windows tool because `sampler.py` reads `/proc` (named in the row); shut down once with `stop` and once with Ctrl-C, the only signal `setup_sighandler` handles off Unix. Out: performance fixes (Goal 9), an installer or Windows service wrapper.
- Depends on: G10-07, G0-03
- Branch: docs/windows-hardware-run
- Needs decision: D10-1
- Done when: the same as G10-09, plus both shutdown paths leave a world that the next start loads without errors.

### Proof

- A `v*` tag (or, before 2.0, a `dry_run` dispatch) produces every asset on the D10-5 list, its plugin zips and `SHA256SUMS` in `release.yml` with no manual step. The run URL goes here.
- In that run, G10-04 rebuilt the D10-2 assets in a second checkout path and `CARGO_HOME` and got the same SHA-256 for each. With D10-2's container option, a rebuild of the tag outside GitHub also matches `SHA256SUMS`.
- `strings -n 8` on the tag's `pumpkin-X64-Linux` finds 0 build-path strings (`/root/.cargo`, `/home/`, runner or target paths).
- G10-07's smoke job passed on every runner in that run, and G10-08's GameTests passed on Linux ARM64, Windows x64 and Windows ARM64 at the same commit.
- G10-09 and G10-10 rows exist for that tag: 60 minutes of the G0-03 scenario on each D10-1 machine, zero panic or ERROR lines, plugins loaded, the world reloaded after a restart, numbers next to the x86_64 baseline.

### Decisions

- D10-1: Which ARM64 and Windows machines run G10-09 and G10-10? Nothing in the repo or CT 117 names one. Options:
  - GitHub-hosted `ubuntu-26.04-arm`, `windows-latest` and `windows-11-arm` only. GitHub doesn't bill standard hosted runners on public repositories (PEX is public). They are VMs, not owned hardware, and exist only if D0-1 brings hosted Actions back.
  - The Pixel 7a from Neon's notes (unverified). Owned, aarch64, no cost. It runs `pumpkin-aarch64-android` natively in Termux, but the glibc ARM64 asset only inside proot, whose syscall translation makes benchmark numbers not comparable with G0-03; phone thermals limit long runs.
  - A cloud ARM64 VM: Oracle Cloud's Always Free Ampere A1 (no charge, free capacity is often unavailable) or a small paid ARM VM (Hetzner CAX class, a few euros a month). Real Linux on server ARM cores, still a VM.
  - An ARM64 board such as a Raspberry Pi 5 with 8 GB: about $100 to $150 with power supply and SSD. Owned hardware, slower than the x86_64 baseline.
  - Windows on owned x86_64 hardware: Windows on a spare or external drive in an owned PC (Neon's notes list a ThinkPad T495 and a desktop, both on Fedora), or a Windows VM on a Proxmox host. Needs a Windows license or Microsoft's time-limited evaluation image.
  - A volunteer runs G10-07's script and the benchmark on their own Windows or ARM64 machine. No cost; needs a written recipe and trust in the reported numbers. Owned Windows ARM64 hardware otherwise means buying a Snapdragon laptop (several hundred dollars).
  - The earlier roadmap recorded an answer on 2026-10-29 (Claude's recommendation at the maintainer's request): hosted runners plus one real run each, on an ARM64 VM or board and a Windows PC.
  - Waits on it: G10-09, G10-10.
  - Answered 2026-10-31 (Neon accepted Q10.1): GitHub-hosted ARM64 and Windows runners once Actions is back (D0-1), plus one real run each, on an ARM64 VM or board and on a Windows PC. Which VM or board and which PC gets picked when G10-09 and G10-10 start.
- D10-2: Which release assets must be bit-for-bit reproducible, and where can a rebuild happen? Options:
  - `pumpkin-X64-Linux`, `pumpkin-X64-Linux-musl` and `pumpkin-plugins-X64-Linux.zip`, built inside a container image pinned by digest, in CI and for any rebuild. Anyone with Docker or Podman can check a release; CT 117 has neither, so one gets installed there or checks happen in CI only. The image also pins the C start-up objects, linker and musl that end up in the binaries.
  - The same assets, reproducible on one host only (CI twice, or CT 117 twice), no container. Proves there are no timestamps or path leaks, but nobody can check a CI binary against a rebuild elsewhere.
  - Every target. Windows and macOS byte identity was not assessed here; the MSVC linker, signing (D10-3) and Apple's toolchain each add inputs, and the work happens on runners nobody here owns.
  - The earlier roadmap recorded x86_64 Linux only (glibc and musl).
  - Waits on it: G10-03, G10-04, G10-05.
  - Partly answered 2026-10-31 (Neon accepted Q10.3): x86_64 Linux only, glibc and musl. Still open: a container pinned by digest (the first option) or one host without one (the second).
- D10-3: Are Windows releases signed? Options:
  - Ship unsigned, with a SmartScreen note in the release text. No cost; the `.exe` compares with a rebuild as is; `sign_windows_release` is removed.
  - SignPath's free open-source program, which `sign_windows_release` already calls. Costs an application and an approval wait, and the repository owner has to add the four `SIGNPATH_*` secrets. A signed `.exe` differs from a rebuild, so checking it means stripping the signature first.
  - A bought code-signing certificate: a yearly fee of a few hundred dollars, the key on a hardware token or cloud HSM, and the same checking caveat.
  - The earlier roadmap recorded unsigned for 2.0, then an application to SignPath.
  - Waits on it: G10-05.
  - Answered 2026-10-31 (Neon accepted Q10.2): unsigned for 2.0 with a SmartScreen note, then an application to SignPath's free open-source program after 2.0.
- D10-4: What does CI publish on the fork besides `v*` releases? Options:
  - A nightly pre-release that is never Latest, built from `master` (the default branch, where schedules run) or from `alpha` (needs an explicit ref), plus Docker images for tagged releases and the nightly. The earlier roadmap recorded nightly as a pre-release and Docker yes.
  - No nightly: only `v*` releases and their Docker images. Least runner time and nothing stale; testers build `alpha` themselves.
  - `nightly.yml` as it is: its first run makes the nightly Latest over v1.2.1, without plugin zips or checksums.
  - Waits on it: G10-06.
  - Answered 2026-10-31 (Neon accepted Q10.6): the first option, a nightly pre-release that is never Latest, plus Docker images. The upstream reviewer bot and the WIT mirror don't run on the fork (G0-01, G2-04). Still open: whether the nightly builds `master` or `alpha`.
- D10-5: Which targets does a 2.0 release ship, and what builds them? This follows D0-1. Options:
  - Everything `release.yml` lists (X64 and ARM64 Linux, glibc and musl; X64 and ARM64 Windows; ARM64 macOS; Android) on hosted runners. Needs D0-1's hosted option; no runner bill on a public repository.
  - X64 Linux, ARM64 Linux and Windows x64, cross-compiled in CT 117 as v1.2.0 was on pve: aarch64 through `cargo-zigbuild` (installed, target not added yet), Windows through `x86_64-pc-windows-gnu` (mingw not installed). No Windows ARM64, macOS or Android. The GNU Windows build has a different target than the MSVC build `release.yml` makes on `windows-latest`, so their build ids differ and their plugin zips never mix.
  - X64 Linux only, as v1.2.1. Least work, but the ARM64 and Windows half of this goal has nothing to test until hosted runners return.
  - Waits on it: G10-05, G10-07, G10-09, G10-10.
  - Partly answered 2026-10-31 (Neon accepted Q10.4): keep the musl build and build only `-p pumpkin` for it, which alpha `300621c3e` already does. Still open: the target list and how it is built, after D0-1.
