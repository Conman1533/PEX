# PEX 2.0 roadmap

Assessed at: `300621c3e5b7a9f5eef70af8c6c8ec5dad8b5e96` (`alpha`, committed 2026-10-29, "ci(release): build only the server for musl targets").

This is a fresh skeleton on branch `roadmap-2.0-v2`. The earlier roadmap is still `docs/ROADMAP-2.0.md` on `alpha` and on branch `roadmap-2.0`.

## Order of work

## Decisions for Neon

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
| Benchmark harness against Paper | Outside the repo. CT 117 `/srv/pex/gameplay-bench`, a clone of `github.com/Conman1533/mc-gameplay-bench`: 8 bots on a seeded schedule; samples CPU and threads, memory, disk, network bytes and tick P50/P95/P99 every 10 s; `report.py` writes an HTML report. Its world is `world/base-world-262.tgz` (built on 26.2). Paper: CT 117 `/srv/pex/bins/paper/paper.jar`, Paper `26.3-169-main@46d4834` (from its `out.log`), started by `/srv/pex/tools/paper_run.sh`. Other scripts: `/srv/pex/tools/bench.sh`, `localbench.sh`, `c2bench.sh`. In the repo there are only criterion benches: `pumpkin-world` (chunk, chunk_io, chunk_gen, chunk_gen_concurrent, jigsaw, noise_router, template_pool), `pumpkin-nbt` (nbt), `pumpkin-util` (perlin), `pumpkin-data` (has_random_ticks). None compares against Paper. | Exists outside the repo. No run for the assessed commit: `find /srv/pex -newer /srv/pex/tools/local-ci.sh -name '*.html'` outside `repo/`, `wt/` and `target/` finds no report. See G0-03. |
| CI | `.github/workflows/`: `rust.yml` (push to `master` and `alpha`, PRs, dispatch; jobs format, machete, clippy, clippy_release, check_wit, test, gametest, test_android, build_pull_request, build_android_pull_request, build_release, build_release_musl, build_android_release, draft_canary_release), `nightly.yml` (03:00 cron), `release.yml` (tags), `docker.yml`, `nix.yml`, `typos.yml`, `sync-wit.yml`, `reviewers.yml`. Stand-in: CT 117 `/srv/pex/tools/local-ci.sh`, outside the repo, runs the Linux jobs of `rust.yml` plus `typos` and writes `/srv/pex/ci/<branch>/<time>/summary.txt`. | Not running. `gh api repos/Conman1533/PEX/actions/permissions` says `"enabled": true`, but the newest entry in `gh run list -R Conman1533/PEX` is from 2026-10-14 (a `master` push), and `rust.yml` triggers on `alpha` pushes while `alpha` was last pushed with a 2026-10-29 commit. No Windows, macOS, ARM64 or Android job runs anywhere now. See G0-01. |
| Real-client play-test rig (not asked for, in use) | CT 117 `/srv/pex/playtest/` (`README.md`): vanilla 26.3 client on Xvfb with Mesa software rendering, driven by `xdotool` and console commands, recorded with ffmpeg. | Exists. |

### Pieces

#### G0-01 CI runs on every alpha push again

- Scope: in: `rust.yml` and `typos.yml` start on pushes to `alpha` (hosted Actions or a runner, per D0-1), and the format, machete, clippy, test and gametest jobs pass on the first such run. Out: release reproducibility and the Windows/ARM64 hardware runs (Goal 10), new jobs (G0-02), fixing failures in tests that a goal piece owns.
- Depends on: none
- Branch: ci/run-on-alpha
- Needs decision: D0-1
- Done when: a push to `alpha` shows up in `gh run list -R Conman1533/PEX -b alpha` for `rust.yml` and `typos.yml`, and the format, machete, clippy, test and gametest jobs of that run pass.
- Needed by: every goal (each piece's "Done when" tests have to stay green on `alpha`), Goal 10 directly.

#### G0-02 Dual-run job in CI

- Scope: in: a job that fetches the vanilla 26.3 server jar from piston-meta, checks its SHA-1, sets up a JDK and runs `tools/dual-run/dual_run.py` on `assets/tests/datapacks/pumpkin-dual-run`. Out: new test structures (each goal adds its own), running bot scenarios or mobs on vanilla.
- Depends on: G0-01
- Branch: ci/dual-run
- Needs decision: none
- Done when: the job runs on a push to `alpha` and passes with the 5 existing redstone structures, and a scratch branch that changes one structure's expected outcome makes it exit 1.
- Needed by: Goal 1 (mob behaviour checked on vanilla), Goal 5 (redstone), Goal 6 (survival mechanics checked on vanilla).

#### G0-03 Benchmark baseline: alpha against Paper

- Scope: in: gameplay-bench runs of PEX at the assessed commit and of Paper `26.3-169`, same CPU set, memory limit, seed, 8 bots and view distance 10, recorded in this file: tick P50/P95/P99, CPU, peak memory, network bytes sent per bot-hour, plus the report paths. Out: fixing anything the runs show (Goal 9), per-packet counts (Goal 9), replacing the 26.2 benchmark world.
- Depends on: none
- Branch: docs/bench-baseline
- Needs decision: none
- Done when: this section has a PEX row and a Paper row, each from at least 2 runs of 60 minutes, with run-to-run spread on tick P95 under 10% (more runs if not).
- Needed by: Goal 9 (the Paper comparison), Goal 10 (the same scenario on ARM64 and Windows hardware), and any Goal 1, 5 or 6 piece that changes tick cost (`AGENTS.md` "Load and memory").

### Decisions

- D0-1: How does CI come back? Options:
  - Ask GitHub support why Actions does not start on Conman1533/PEX. Costs nothing but the owner's time and an unknown wait. Keeps hosted Linux, Windows, macOS and ARM64 runners.
  - Self-hosted runner in CT 117. No wait on GitHub, Linux x86_64 only, and runs PR code on the home network unless restricted to pushes from the repo. Windows and ARM64 jobs still have no runner.
  - Keep `local-ci.sh` run by hand before each merge. No cost, no automatic runs, and no Windows, ARM64 or Android coverage.
  - Waits on it: G0-01, and through it G0-02 and Goal 10.

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

### Pieces

### Proof

### Decisions

## Goal 2: Plugin API decision

Plugin API decision: a versioned native plugin ABI, or an explicitly unstable one; drop WASM v0.1.

Start from: what native-plugins/ exposes and how plugins load; everything WASM v0.1 touches (loader, deps, config, docs, plugins using it).

### Current state

### Pieces

### Proof

### Decisions

## Goal 3: Config cleanup

Config cleanup: config_version + automatic migration, remove dead keys (write_in_place).

Start from: every config key and where it's read; keys nothing reads; any existing versioning or migration code.

### Current state

### Pieces

### Proof

### Decisions

## Goal 4: Offline UUIDs match vanilla

Offline UUIDs match vanilla, with a player-data migration.

Start from: how PEX derives offline UUIDs vs vanilla (v3 UUID of "OfflinePlayer:<name>"); everything keyed by player UUID (playerdata, stats, advancements, ops/whitelist/bans, plugin data).

### Current state

### Pieces

### Proof

### Decisions

## Goal 5: Redstone parity

Redstone parity (update order, quasi-connectivity, comparator timing).

Start from: how neighbour updates and scheduled ticks are queued and ordered; quasi-connectivity in pistons/dispensers/droppers; comparator and repeater timing; existing redstone tests.

### Current state

### Pieces

### Proof

### Decisions

## Goal 6: Survival parity end to end

Survival parity end to end (start through dragon kill), proven with bots.

Start from: what survbot can do today; the checkpoints a start-to-dragon run needs (wood, iron, nether, blaze rods, pearls, stronghold, End), and which PEX mechanics each one depends on.

### Current state

### Pieces

### Proof

### Decisions

## Goal 7: Worlds round-trip with vanilla 26.3

Worlds round-trip with vanilla 26.3.

Start from: what PEX reads and writes (region/chunk NBT, data version, level.dat, playerdata, entities/, poi/, data/), and what's lost going PEX -> vanilla -> PEX.

### Current state

### Pieces

### Proof

### Decisions

## Goal 8: Bedrock polish

Bedrock polish.

Start from: how Bedrock support works today and its known gaps (issues, TODOs, docs). "Polish" is open-ended, so its Decisions must pin down what done means.

### Current state

### Pieces

### Proof

### Decisions

## Goal 9: Performance vs Paper

Performance vs Paper: fix extra network traffic and p95 tick spikes, publish benchmarks.

Start from: where the extra traffic comes from (packets/bytes per scenario vs Paper), what causes p95 spikes, what profiling and benchmarks exist.

### Current state

### Pieces

### Proof

### Decisions

## Goal 10: Reproducible release builds in CI, ARM64/Windows on real hardware

Reproducible release builds in CI, ARM64/Windows run on real hardware.

Start from: CI workflows, toolchain pinning, Cargo.lock, anything that makes builds differ (paths, timestamps, build scripts), and what ARM64/Windows hardware exists (a Decision if unknown).

### Current state

### Pieces

### Proof

### Decisions
