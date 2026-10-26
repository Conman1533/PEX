# PEX 2.0 roadmap

Written against `alpha` at `2ce43ef3d` (2026-10-24). Every "current state" line below was checked in that commit, or in the decompiled vanilla 26.3 server where it says so. File paths are relative to the repository root unless they start with `/`. Line numbers are for `2ce43ef3d` and will drift.

This document is organised by the ten 2.0 goals. Each goal has its current state, the work split into pieces of one session each, the dependencies between pieces, the proof that the goal is done, and the decisions only the maintainer can make. Decisions are numbered `Q<goal>.<n>` and collected again at the end.

The unmerged branch `feat/tracker` has an earlier roadmap derived from its feature tracker (719 entries, 50 audits). That roadmap is grouped by tracker area; this one is grouped by goal. Where both cover the same thing, this one was re-checked against the code rather than copied. If `feat/tracker` merges, update its entries in the same branch as each piece.

## How to work a piece

Each piece has an ID (`1.4`), a suggested branch and worktree short name, its dependencies and a "done means" line. A session works exactly one piece, using the step 2 rules (own worktree on branch `<type>/<short-name>` off `alpha`, own `CARGO_TARGET_DIR`, no live servers, no merges, no PRs, a PR draft per branch).

**Where the work happens.** Since 2026-10-24 all PEX work runs in CT 117 `pex-dev` on pve (Ubuntu 26.04, 16 cores, 32 GB RAM, 100 GB disk), reachable over Tailscale as `root@100.109.157.28` (`pex-dev`). The step 2 template's pve paths map like this: worktree `/srv/pex/wt/<short-name>`, `CARGO_TARGET_DIR=/srv/pex/target/<short-name>`, PR draft `/srv/pex/pr-drafts/<branch with / replaced by ->.md`, scratch files `/srv/pex/scratch/<short-name>`. The main checkout is `/srv/pex/repo` (git identity Neon Swan, `gh` logged in as Conman1533). The old pve working area is archived at `/Vault-Mk4/pex-archive/pumpkin-fork` on the pve host, with the old worktrees still in `/Vault-Mk4/pex-wt`.

"Depends on" means the other piece has been merged into `alpha` by the maintainer. Sessions don't merge and don't stack branches on each other, so each merge round unlocks the next pieces. A dependency on a `Q` means the maintainer has answered that question.

**CI while GitHub Actions is off.** GitHub has disabled Actions for the fork (goal 10, Q10.5), and the maintainer asked to work around it for now. `/srv/pex/tools/local-ci.sh <worktree>` stands in for CI. It runs the Linux jobs of `rust.yml` and `typos.yml` (typos, fmt, machete, the WIT check when WIT changed, clippy, nextest, doc tests, `pumpkin-tester`) on the packages the branch affects. It chooses them with the workflow's own `.github/scripts/ci_packages.py`, so commit before running it. When the branch lacks the alpha clippy fix (`ff3ff4f28`), it applies it for the run and removes it afterwards. Logs go to `/srv/pex/ci/<branch>/<time>/`. Wherever this document says "passes in CI" or "in CI", read: local CI passes in the CT, with the summary path in the PR draft. Source `/srv/pex/env.sh` before any cargo command, so builds use CI's `-Dwarnings` and share one cache with local CI. Pieces whose proof needs GitHub-hosted runners (Windows, macOS, ARM64 jobs, tag-triggered releases) prove everything they can locally first. The runner-only part waits until Actions is back.

What is already in the CT for every session:

| What | Where |
| :-- | :-- |
| Decompiled vanilla 26.3 server, whole `net.minecraft` (5,027 files, Vineflower, no errors) | `/srv/pex/vanilla-26.3/src` |
| Vanilla 26.3 server jar (sha256 `a362163e...`), the bundler and Vineflower. Start vanilla with `java -jar bundler-26.3.jar`; `server-26.3.jar` alone doesn't start. Vanilla 26.3 defaults to `white-list=true`, so set it to false for bot runs. | `/srv/pex/vanilla-26.3/server-26.3.jar`, `bundler-26.3.jar`, `vineflower.jar` |
| JDK 25 for running vanilla and Paper | `/srv/pex/jdk-25` |
| Paper 26.3 jar used by the earlier benchmarks | `/srv/pex/bins/paper/paper.jar` |
| Protocol 777 Python bots (`walkbot.py`, `survbot.py`, `botfleet.py`), boot and bench scripts | `/srv/pex/tools` |
| Gameplay benchmark (bots, sampler, report) | `/srv/pex/gameplay-bench` |
| Benchmark world | `/srv/pex/bins/base-world-262` (older worlds and binaries: `/Vault-Mk4/pumpkin-bins` on the pve host) |
| Earlier PR drafts (format to copy) | `/srv/pex/pr-drafts` |
| Dual-run GameTest runner and vanilla structure builder (on `test/redstone-dual-run` until merged) | `tools/dual-run/` in that branch |

The CT shares the pve host with production (Pelican CT 109 with the game servers, hermes-jail, Jellyfin), and the host is short on RAM. Keep at most three sessions compiling at once, build with `-j 8` (`/srv/pex/env.sh` sets it, together with CI's `-Dwarnings` and `CARGO_INCREMENTAL=0`), and don't use fat LTO release builds unless the piece is about release builds. With incremental compilation a debug target dir with tests reached 31 to 41 GB and filled the disk on 2026-10-24, which is why `env.sh` turns it off; without it the 100 GB disk holds three target dirs at a time; delete a piece's target dir once its branch is pushed and handed over. The CT has rustc 1.99.0 and 1.99.1 (current stable, which `rust-toolchain.toml`'s `stable` selects). Cross compilers for release builds (aarch64, mingw) are not installed yet; 10.x pieces add them.

### Baseline on alpha

Run on the pve host with rustc 1.99.0 (logs copied to `/srv/pex/roadmap-baseline/`); rerun in the CT by `/srv/pex/tools/roadmap-baseline.sh` (logs in `/srv/pex/roadmap-baseline/ct-<date>/`).

| Check | Result |
| :-- | :-- |
| `cargo fmt --check` | passes |
| `cargo test --workspace --no-fail-fast` (13 min) | one failure: `pumpkin-world` `tests/world_info_save.rs` `write_world_info_reports_final_rename_failure`. It makes a directory read-only (`0o555`) to force the final rename to fail, and root ignores that, so it fails on pve, where every session runs as root. Everything else passes (616 tests in `pumpkin-core`). |
| `cargo run -p pumpkin-tester` (5 min) | 200 GameTests ran; one required test fails: `pumpkin:scaffolding_distance_and_collapse` ("expected block 'air', found 'scaffolding'" at tick 0). `summon_command_regression` passes. |
| `cargo clippy --workspace --all-targets --all-features` | fails in `pumpkin-core` with 3 errors (`entity/mob/hoglin.rs:273`, `entity/passive/goat.rs:50`, `world/mod.rs:434`) and 1 warning (`entity/passive/ocelot.rs:167`), all from the 2026-10-23 mob AI merges. Because `pumpkin-core` stops there, nothing that depends on it is linted. Fixed on `fix/clippy-alpha` (`ff3ff4f28`); with it the whole workspace passes. Merge it first, or every piece's clippy run stops at the same three errors. |

The rerun in the CT (rustc 1.99.1, `ct-20261024`, with three other builds running) gave the same results, plus two failures that pass when rerun alone: `pumpkin-core` `creaking_heart::tests::creaking_heart_spawns_protector_at_night_spreads_resin_on_hurt_and_despawns_at_dawn` (failed once in two isolated runs too, at `creaking_heart.rs:655`, "comparator analog output should be positive while protector is nearby") and `pumpkin-world` `biome_hashes::nether_biome_hashes_match_vanilla`. Treat both as flaky until someone looks at them.

A session that sees one of these fail on its branch should check this table before blaming its own change.

### Work not on alpha yet

These branches are pushed but not merged. A piece that touches the same area starts by reading them.

| Branch | Ahead of alpha | What it does | Relevant to |
| :-- | --: | :-- | :-- |
| `fix/flush-block-entities-before-unload` | 1 | Writes live block entities into their chunk before it unloads. Without it, chest and hopper changes are lost when players walk away (`pr-drafts/ISSUE-block-entities-lost-on-unload.md`). | 6, 7 |
| `feat/tracker` | 6 | Feature tracker, 50 audits and the earlier roadmap | all |
| `warden-contrib` | 5 | Warden anger management, vibration sharing, collar colour slots | 1.24 |
| `goat-parity` | 1 | Goat spawning, breeding, milking, sounds | 1.21 |
| `wip/fish-movement` | 1 | Vanilla fish swimming | 1.12 |
| `pumpkin-contrib` | 1 | Collar colour data slots (also in `warden-contrib`) | 1.17 |
| `perf/memory` | 3 | Uniform light sections, packed idle proto chunks, two malloc arenas | 9 |
| `perf/pathfinder-heap`, `perf/entity-box-query` | 1 each | Pathfinder and entity query allocations | 9 |
| `claude/gifted-sagan-1yvyp9`, `claude/nifty-curie-occmzq`, `claude/nifty-newton-nx1dgj` | 4, 1, 1 | Worldgen feature placement fixes | 7 |

## Groundwork

Small fixes that every other piece trips over. They don't belong to a goal, and they should be merged first.

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 0.1 | Clear the clippy errors the 2026-10-23 mob AI merges left in `pumpkin-core` | `fix/clippy-alpha` | | done (see Piece status) |
| 0.2 | Make `tools/pumpkin-codegen` deterministic: the `structure_template.rs` generator writes block entity NBT keys in a different order on every run (about 1,400 lines of churn), and a plain `cargo run -p pumpkin-codegen` on alpha rewrites six generated files unrelated to the input (`item.rs`, `registry.rs`, `structure_template.rs`, plugin-api `block.rs` and `item.rs`, `java-packets.wit`). Find out whether alpha's generated files are stale or the generator is nondeterministic, fix it, and regenerate once. | `fix/codegen-deterministic` | | Two codegen runs in a row produce no diff, and a run on the fixed branch changes only what its input changed |
| 0.3 | `pumpkin-world` `write_world_info_reports_final_rename_failure` fails under root (a `0o555` directory doesn't stop root); make the failure independent of the user | `test/world-info-rename-root` | | Test passes as root and as a normal user, and still fails when the rename failure isn't reported |
| 0.4 | The `commands_and_gamemode` bot test in `pumpkin-tester` is flaky on a busy host ("Bot `/setblock` did not place emerald_block") and stops the tester before any GameTest runs. Cause found: a chunk unload race in `pumpkin-world` (see Piece status) | `fix/tester-setblock-flake` | | 30 runs in a row pass on a busy CT |

## Goal 1: every 26.3 mob has its vanilla AI

### Current state

- `assets/entities.json` has 90 entity types with `"mob": true`. 88 are wired in `from_type()` (`crates/pumpkin-core/src/entity/type.rs`). `camel_husk` and `zombie_nautilus` fall through to the fallback arm (`type.rs:352`) and spawn as a plain `LivingEntity` with no AI at all.
- In vanilla 26.3, 21 of the 90 mobs run on the Brain (sensors, memories, behaviours): allay, armadillo, axolotl, breeze, camel, camel_husk, copper_golem, creaking, frog, goat, happy_ghast, hoglin, nautilus, piglin, piglin_brute, sniffer, tadpole, villager, warden, zoglin, zombie_nautilus. The other 69 use goal selectors.
- PEX has a Brain framework (`entity/ai/brain/`, 16 behaviours, 5 sensors) but only the allay uses it. The other 20 Brain mobs are hand-written goal sets or tick code that approximate the Brain (the goat has 8 goals, the villager 17, the warden 7). Piglins use their own `mob/piglin_ai.rs`.
- The goal library has 61 goals in `entity/ai/goal/`. 10 implement `tick_reach`, and 4 mobs (chicken, cow, pig, sheep) return true from `has_audited_tick`. Every other mob uses the conservative island reach.
- Known gaps checked in code:
  - The ender dragon's strafing phase spawns the dragon's breath cloud directly (`entity/boss/ender_dragon/phase/strafing.rs:178`). There is no `dragon_fireball` entity in `from_type()`.
  - The nautilus has no goals and no Brain; vanilla drives it with `NautilusAi`.
  - The squid folds vanilla's two goals into `mob_tick`, which is allowed if the behaviour matches but hasn't been audited.
- Tests: 220 builtin GameTests exist (`crates/pumpkin-core/src/server/builtin_game_tests/`), but only one is about mob AI (`zombie_pathfinds_to_villager`). `tools/pumpkin-tester` has bot tests for the allay and sheep shearing. Nine mob files have unit tests.
- There is no parity table on alpha. The upstream mob PRs merged on 2026-10-23 matched their goal lists against vanilla: fox, drowned, bogged, cats and ocelots, silverfish, weak self references in goals, Pumpkin-MC/Pumpkin#4050, #3795, #3774, #3815, #4113, #3790, #4095 and #3715, on top of the #4092 AI foundation. Not all of them against 26.3: #3984 (cats and ocelots) compared with 26.2, and #3715 says its phantom AI came from 1.21.1.

Appendix A has the starting table for all 90 mobs.

### Pieces

Every family piece does the same thing: audit each mob against its vanilla class chain (`registerGoals`, `customServerAiStep`, `aiStep`, the `*Ai` class for Brain mobs, navigation and move control overrides), fix what differs, fill its rows in `docs/MOB-PARITY.md`, and add one GameTest per mob that exercises behaviour a player would notice. Where vanilla can run the same test (a block-based test in a datapack, see 5.1), prefer that form, because passing on both servers is the strongest proof. Implement `tick_reach` for goals the piece writes or rewrites (root `AGENTS.md`, Mob AI section). Start each piece by searching open upstream Pumpkin PRs for the same mobs.

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 1.1 | Parity table and mob test helpers. Create `docs/MOB-PARITY.md` from Appendix A with columns for the vanilla classes and methods compared, the commit audited, status and test name. Add the `GameTestHelper` assertions the family pieces will need (entity in box after N ticks, entity count by type, mob target, held item), only as many as the first test uses. Write that first test for the cow (tempted by wheat, panics when hit). | `test/mob-parity-table` | | Table has 90 rows; the cow test passes in `cargo run -p pumpkin-tester` and fails if `TemptGoal` is removed from the cow |
| 1.1b | Shared mob test helpers before the families run in parallel: a survival-mode mock player (1.1's mock players are creative, so hostiles ignore them), the mob target and held item assertions, and a wider tester grid for mob tests (`TEST_GRID_SPACING` 16 is less than tempt and follow ranges), so mock-player tests can run in one batch instead of one at a time. | `test/mob-test-helpers` | 1.1 | Two existing mob tests run in the same batch without interfering; a hostile mob targets a survival mock player in a test |
| 1.2 | Zombie family: zombie, husk, drowned, zombie_villager, zombified_piglin, giant | `fix/mobai-zombies` | 1.1 | 6 rows filled; a test per mob (zombie breaks a door on hard, husk does not burn, drowned throws a trident, zombified piglin pack anger) |
| 1.3 | Skeleton family: skeleton, stray, bogged, parched, wither_skeleton | `fix/mobai-skeletons` | 1.1 | 5 rows; a test per mob (strafing bow attack, stray slowness arrow, wither skeleton melee with wither effect) |
| 1.4 | Arthropods and small hostiles: spider, cave_spider, creeper, silverfish, endermite | `fix/mobai-arthropods` | 1.1 | 5 rows; tests include creeper fleeing a cat and spider climbing |
| 1.5 | Enderman and shulker | `fix/mobai-endermen` | 1.1 | 2 rows; enderman stare aggression, block carrying and water avoidance; shulker peek and bullet |
| 1.6 | Cube mobs and nether goal mobs: slime, magma_cube, sulfur_cube, blaze, ghast, strider | `fix/mobai-nether-goals` | 1.1 | 6 rows; blaze keeps near its spawner (`MoveTowardsRestrictionGoal`), ghast fireball, strider lava walking |
| 1.7 | Piglin and piglin brute (vanilla `PiglinAi`, `PiglinBruteAi`): bartering, gold admiration, guarding bastions, hunting hoglins | `fix/mobai-piglins` | 1.1, Q1.1 | 2 rows; bartering test gives a loot-table item for a gold ingot; piglins attack a player without gold armour |
| 1.8 | Hoglin and zoglin (vanilla `HoglinAi`, `ZoglinAi`) | `fix/mobai-hoglins` | 1.1, Q1.1 | 2 rows; hoglin avoids warped fungus, zoglin attacks any mob |
| 1.9 | Raiders: pillager, vindicator, ravager, witch | `fix/mobai-raiders` | 1.1 | 4 rows; pillager crossbow, vindicator door breaking in raids, witch drinks potions |
| 1.10 | Spellcasters: evoker, illusioner, vex | `fix/mobai-spellcasters` | 1.1 | 3 rows; evoker fangs and vex summoning, illusioner mirror images |
| 1.11 | Guardian and elder guardian | `fix/mobai-guardians` | 1.1 | 2 rows; laser charge time, elder guardian mining fatigue |
| 1.12 | Fish and squid: cod, salmon, tropical_fish, pufferfish, squid, glow_squid. Start from `wip/fish-movement`. | `fix/mobai-fish` | 1.1 | 6 rows; schooling, pufferfish puff, squid flee |
| 1.13 | Dolphin and turtle | `fix/mobai-dolphin-turtle` | 1.1 | 2 rows; dolphin leads to treasure and gives Dolphin's Grace; turtle returns home to lay eggs |
| 1.14 | Farm animals: chicken, cow, mooshroom, pig, sheep | `fix/mobai-farm` | 1.1 | 5 rows; breeding, tempting, egg laying, eating grass |
| 1.15 | Horses: horse, donkey, mule, skeleton_horse, zombie_horse | `fix/mobai-horses` | 1.1 | 5 rows; taming, skeleton trap horse |
| 1.16 | Llamas and the wandering trader: llama, trader_llama, wandering_trader | `fix/mobai-traders` | 1.1 | 3 rows; caravans, trader llamas defend the trader, trader drinks invisibility at night |
| 1.17 | Tamables: wolf, cat, ocelot, parrot. Start from `pumpkin-contrib`. | `fix/mobai-tamables` | 1.1 | 4 rows; follow owner, sit, cats on beds and chests, parrots on shoulders |
| 1.18 | Wild passives: rabbit, fox, panda, polar_bear | `fix/mobai-wild` | 1.1 | 4 rows; rabbits raid carrots, panda personalities, polar bear protects cubs, bred foxes trust breeders |
| 1.19 | Bee: hives, flowers, pollination, crop growth, anger, stinging | `fix/mobai-bees` | 1.1 | 1 row; bee leaves and re-enters its hive and pollinates crops |
| 1.20 | Brain passives: axolotl, frog, tadpole (vanilla `AxolotlAi`, `FrogAi`, `TadpoleAi`) | `fix/mobai-amphibians` | 1.1, Q1.1 | 3 rows; axolotl play dead, frog eats slimes, tadpole grows |
| 1.21 | Brain passives: goat, armadillo, sniffer. Start from `goat-parity`. | `fix/mobai-goats` | 1.1, Q1.1 | 3 rows; goat ram and long jump, armadillo rolls up, sniffer digs seeds |
| 1.22 | Camels, nautiluses and their zombie variants: camel, camel_husk, nautilus, zombie_nautilus. The two variants need entity implementations first. | `feat/mobai-mounts` | 1.1, Q1.1 | 4 rows; camel_husk and zombie_nautilus spawn as mobs; camel dash |
| 1.23 | Golems and helpers: iron_golem, snow_golem, copper_golem, allay, happy_ghast | `fix/mobai-golems` | 1.1, Q1.1 | 5 rows; iron golem defends villagers, copper golem sorts items, allay delivers to a note block |
| 1.24 | Warden: emerge, sniff, dig, sonic boom, anger. Start from `warden-contrib`. | `fix/mobai-warden` | 1.1, Q1.1 | 1 row; sonic boom test in an ancient city structure |
| 1.25 | Creaking and breeze (vanilla `CreakingAi`, `BreezeAi`) | `fix/mobai-creaking-breeze` | 1.1, Q1.1 | 2 rows; creaking freezes when watched, breeze wind charge and jump |
| 1.26 | Villager, part 1: sensors, memories, POI claiming (job site, bed, meeting point), daily schedule | `fix/mobai-villager-poi` | 1.1, Q1.1 | Villager claims a job site and bed and follows the schedule in a test village |
| 1.27 | Villager, part 2: work, farming, restocking, gossip and reputation | `fix/mobai-villager-work` | 1.26 | Farmer villager harvests and replants; restock happens at the workstation |
| 1.28 | Villager, part 3: breeding, panic, golem summoning, zombie conversion and curing | `fix/mobai-villager-life` | 1.27 | 2 rows (villager, zombie_villager curing); golem summoned on panic |
| 1.29 | Bat and phantom | `fix/mobai-flyers` | 1.1 | 2 rows; phantom swoop and cat avoidance |
| 1.30 | Wither | `fix/mobai-wither` | 1.1 | 1 row; skull targeting and block breaking. The dragon is 6.6. |

1.2 to 1.30 can run in parallel once 1.1 and 1.1b are merged, three at a time in the CT. The Brain pieces (1.7, 1.8, 1.20 to 1.28) wait for Q1.1.

### Proof

- `docs/MOB-PARITY.md` has all 90 rows at status `parity`, each naming the vanilla classes and methods compared and the commit audited, plus any accepted deviation.
- Every mob has a test registered in `builtin_game_tests` (or `pumpkin-tester` where a player is needed) that passes in CI.
- The tests that are block-based datapack tests also pass on vanilla 26.3 through `/test run <namespace>:<dir>/*` (26.3 has no `runall`). Behaviour that needs a player (tempting, targeting players) can't be a block-based test, so those mobs rely on the audit and the function test.

### Decisions

- **Q1.1** Do the 21 Brain mobs have to run on PEX's Brain framework, or is a goal-based port acceptable when the observable behaviour matches vanilla? Root `AGENTS.md` allows a different design with the same behaviour, but upstream Pumpkin has a planned Brain design and won't take workarounds. A full Brain port is roughly twice the work per mob.
- **Q1.2** Should the per-mob tests stay in this repository's GameTests, or go to upstream Pumpkin as well?
- **Q1.3** Is 1.26 to 1.28 (villagers) in scope for 2.0? It's three sessions, and villagers matter for trading but not for the dragon.
- **Q1.4** Tests that need a player use an in-world mock player with a closed client (1.1, after vanilla's `makeMockServerPlayerInLevel`, which vanilla marks deprecated for removal). Acceptable for PEX, and for upstream if the tests go there?

## Goal 2: plugin API decision

### Current state

- **Native plugins** are Rust `cdylib`s loaded with `libloading`. The loader (`crates/pumpkin-core/src/plugin/loader/native.rs`) reads three symbols: `PUMPKIN_API_VERSION` (a `u32`), `METADATA` (a `LazyLock<PluginMetadata>`) and `plugin` (a Rust `fn() -> Box<dyn Plugin>`). Everything crosses the boundary as Rust types, so the ABI is only valid for the same compiler, source and lockfile.
- The only compatibility check is `plugin_api_version != PLUGIN_API_VERSION` (`native.rs:35`), and `PLUGIN_API_VERSION` is 3 (`plugin/mod.rs:37`). A plugin built from a different commit with the same number loads and then corrupts memory, which `native-plugins/AGENTS.md` and the 1.2 release notes both warn about.
- The server already embeds its commit (`crates/pumpkin-core/build.rs` sets `GIT_HASH` and `GIT_HASH_FULL`), so a build identity is cheap to add.
- Event handlers are not ordered by `EventPriority`. `register` pushes them in registration order (`plugin/mod.rs:1187`) and `fire` runs blocking handlers first, then the rest.
- The 1.2 release ships native plugins as `pumpkin-plugins-<arch>-<os>.zip` for X64 Linux, ARM64 Linux and X64 Windows only. The Pelican "PEX Plugins" panel plugin (`/root/pex-plugins-plugin`) installs them by downloading `pumpkin-plugins-X64-Linux.zip` from the GitHub release named in its config.
- **WASM**: `pumpkin-wasm-host` always builds the v0.1 host. The v0.2 host is behind the `plugin-v0_2` feature, off "until the v0.2 ABI is finished" (`crates/pumpkin/Cargo.toml`). The plugin SDK (`pumpkin-plugin-api`) generates its bindings from `pumpkin-plugin-wit/v0.1` (`lib.rs:207`). Dropping v0.1 therefore means either finishing v0.2 and moving the SDK to it, or removing WASM plugins entirely. `grep` finds 44 TODO, `unimplemented!` or "unsupported" markers in the v0.2 host.
- The WASM crates, WIT and SDK are upstream Pumpkin's. `pumpkin-plugin-wit` is mirrored to its own repository on every push to `master` (`.github/workflows/sync-wit.yml`).

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 2.1 | Build identity: the server exports a build id (full commit, rustc version, target triple, `Cargo.lock` hash, enabled features), `pex_common::export_plugin!` and `#[plugin_impl]` export the same, and the loader refuses a mismatch with a one-line message naming both ids. Useful under either answer to Q2.1, as the check for "same build". | `feat/plugin-build-id` | | Unit test: mismatched id is refused. Boot test: the three bundled plugins from the same build load; plugins from the v1.2.0 release zip are refused with the message instead of loading |
| 2.2 | Order handlers by `EventPriority` (keeping blocking before non-blocking if that's the documented contract), and re-check which cancellable events are fired without honouring `cancelled()` (pexport found 41 at `e3b19661`; recount on alpha). Fix or document each. | `fix/plugin-event-order` | | Unit test for ordering; a table of cancellable events and their status in the PR |
| 2.3 | If Q2.1 is "unstable": document the policy in `native-plugins/PORTING_GUIDE.md`, `README.md` and the release notes template; add the build id to `/version` or the startup log so admins can match plugin zips to servers. | `docs/plugin-abi-unstable` | 2.1, Q2.1 | Docs merged; startup log shows the build id |
| 2.4 | If Q2.1 is "versioned": write `docs/PLUGIN-ABI.md` choosing the mechanism (a C ABI with versioned function tables, `abi_stable` or `stabby`, or WASM v0.2 as the only stable API with native plugins kept unstable), what is covered, and the compatibility promise. Split the implementation into pieces in that document. | `docs/plugin-abi-design` | Q2.1 | Design approved by the maintainer before any implementation piece starts |
| 2.5 | Finish WASM v0.2 (only if Q2.2 keeps WASM): go through the 44 markers, make `plugin-v0_2` the default, move `pumpkin-plugin-api` to the v0.2 WIT. | `feat/wasm-v0-2-default` | Q2.2 | An example plugin built with the SDK against v0.2 loads and handles an event in a boot test |
| 2.6 | Remove WASM v0.1: the `pumpkin-wasm-host-v0_1` crate, `pumpkin-plugin-wit/v0.1`, the v0.1 engine and linker in `pumpkin-wasm-host/src/runtime.rs`, the v0.1 branch of the CI WIT check and the WIT mirror job. If Q2.2 drops WASM altogether, remove all the WASM crates, the SDK and runtime instead. | `refactor/drop-wasm-v0-1` | 2.5 or Q2.2 | Workspace builds and tests pass without the crates; a v0.1 plugin is refused with a clear message |
| 2.7 | Plugin zips for every release target, named so the panel plugin keeps working (see 10.3) | part of 10.3 | 2.1 | Release has a plugin zip next to every glibc binary |

### Proof

- A plugin from another build is refused at load with a message naming both build ids (test plus boot test), or, for a versioned ABI, a plugin built against the previous 2.x minor loads and works.
- No v0.1 code is left in the workspace, and the release notes say what replaced it.
- The policy is written down in the porting guide and the release notes.

### Decisions

- **Q2.1** Versioned native ABI (a promise that plugins keep loading across 2.x, several sessions plus ongoing care) or explicitly unstable (plugins only from the same release, enforced by 2.1, one session)? Releases already ship plugins per build, and the panel plugin already installs per release.
- **Q2.2** When v0.1 goes: finish v0.2 and keep WASM, or drop WASM plugins entirely? Either way PEX diverges from upstream in crates upstream keeps changing, so every upstream merge will conflict there.
- **Q2.3** Are third-party native plugins a 2.0 goal, or only the three bundled ones?
- **Q2.4** 2.1's build id can't see dependency features (`--features console-subscriber` adds tokio's `tracing`, and plugins built without it still pass). Normal builds resolve the same features, and releases build everything in one invocation. Acceptable, or is a runtime fingerprint worth calling into the plugin before the check?

## Goal 3: config cleanup

### Current state

- All server settings are in one file, `pumpkin.toml` (`crates/pumpkin-config/src/lib.rs:94`). There is no version key in it.
- Loading (`LoadConfiguration::load`, `lib.rs:288`) merges the file over the defaults. When keys are missing it fills them in and rewrites the whole file with `toml::to_string`, which drops the admin's comments and ordering.
- **An invalid value resets the whole file to defaults without saying which key.** `merge_with_default_toml` does `merged_value.try_into().unwrap_or_else(|_| Self::default())` (`lib.rs:397`), and the error is discarded. A TOML syntax error also falls back to all defaults (`lib.rs:315-318`). Neither is visible: `load` runs before `init_logger` (`crates/pumpkin/src/main.rs:73-77`), so every `tracing` line it emits is dropped, and v1.2.0 boots on all defaults with a broken file and prints nothing (checked by 3.1).
- Unknown keys are kept in the file and silently ignored, because serde ignores unknown fields and the rewrite only happens when keys are missing.
- Dead keys: comparing the config structs at `v1.0.0`, `v1.1.0`, `v1.2.0` and alpha, the only field removed is `AnvilChunkConfig.write_in_place` (`[world.chunk]`). Upstream #4062 (`b7174db92`) replaced it; the 1.2 release notes tell admins it is ignored. Added since 1.2.0: `LevelConfig.entity_islands` (with `knockback_reach` and `unaudited_reach`) and `BedrockPack.path`.
- The default chunk format changed from Anvil to Pump in 1.2; existing files keep their `type`.
- Plugin configs go through `pex_common::load_config` (`native-plugins/common/src/lib.rs`), which logs a parse error and uses defaults without rewriting the file. They have no version either.
- `toml_edit` is not in `Cargo.lock` (`toml` 1.1.8 is), so a comment-preserving rewrite needs a new dependency.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 3.1 | `config_version` in `pumpkin.toml` and an ordered list of migrations. A file without the key is treated as 1.2. Before changing anything, copy the file to `pumpkin.toml.v<old>.bak`. Log one line per change. Decide in the PR whether to add `toml_edit` to keep comments. | `feat/config-version` | | Unit tests: default files generated from the `v1.0.0`, `v1.1.0` and `v1.2.0` tags migrate to the current schema; a current file is left byte for byte unchanged |
| 3.2 | First config migration after 4.2 (4.2 takes `config_version = 2`, so this is 3 if 4.2 merges first): remove `world.chunk.write_in_place`. Warn once at startup about any remaining unknown key, by its full path (`config_version` is not a struct field and must count as known). The 1.0 and 1.1 fixtures from 3.1 (`crates/pumpkin-config/tests/fixtures/`) contain `write_in_place = true`; the migration changes the expected log lines in `tests/config_version.rs`. | `fix/config-dead-keys` | 3.1 | Unit test with a 1.0 file containing the key; boot test shows no warning on a migrated file |
| 3.3 | Invalid values (read upstream Pumpkin-MC/Pumpkin#2985 and its tests first): report the key, the bad value and the expected type, and then follow Q3.1 (refuse to start, or use that key's default and keep the rest). Same for TOML syntax errors. | `fix/config-invalid-values` | 3.1, Q3.1 | Unit test: a file with one bad value keeps every other value; the log line names the key |
| 3.4 | The same versioning and error handling for plugin configs in `pex_common::load_config`, and a first migration for each bundled plugin if any of their keys changed since 1.2. | `feat/plugin-config-version` | 3.1 | Unit tests in `native-plugins/common` |
| 3.5 | Boot test on copies of real configs (check first how the PEX egg on Pelican edits `pumpkin.toml`; upstream #3410 shows the Pterodactyl egg's line-prefix parser only matches on first boot): take read-only copies of `pumpkin.toml` from servers 30 and 34 and a free-tier server (copy via the panel host without changing anything), run the migration, and check the result boots with the same effective settings. | `test/config-migration-real` | 3.2, 3.3 | Before and after files and the boot log in the PR |

### Proof

- Unit tests for every migration step and for invalid values.
- Real 1.x config files from the live servers (copies) migrate and boot with no warnings, and the backups are written.
- `write_in_place` no longer appears in a migrated file.

### Decisions

- **Q3.1** On an invalid value: refuse to start and name the key, or start with that key's default and warn? The current behaviour (reset everything quietly) is the worst of both.
- **Q3.2** Should migrations rewrite the admin's file in place (with a backup), or leave it and write the result next to it for the admin to swap in?
- **Q3.3** Are comments in `pumpkin.toml` worth a new dependency (`toml_edit`)? 3.1 added it (one new crate in `Cargo.lock`; it shares the parser crates with `toml`), and switching back only affects the migration code.
- **Q3.4** A `pumpkin.toml` with a newer `config_version` than the server knows: warn and start (3.1's choice), or refuse to start like vanilla does for a world from an incompatible version?
- **Q3.5** Filling a minimal hand-written file logs one line per missing key (about 30 for a four-line file). Acceptable, or one summary line?

## Goal 4: offline UUIDs match vanilla

### Current state

- `offline_uuid` (`crates/pumpkin-core/src/net/mod.rs:74`) is the first 16 bytes of SHA-256 of the name, with no version bits. Vanilla's `UUIDUtil.createOfflinePlayerUUID` (`/srv/pex/vanilla-26.3/src/net/minecraft/core/UUIDUtil.java:87`) is `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`, a version 3 MD5 UUID. The gameplay benchmark's `whitelist.py` has to generate both kinds for this reason.
- Callers: offline login (`net/java/login/login_start.rs:69`, which also falls back to the nil UUID on error), BungeeCord forwarding without a UUID (`net/proxy/bungeecord.rs:71`), `/fetchprofile` (`command/commands/fetchprofile.rs:199`) and game profile arguments (`command/argument_types/game_profile.rs:99`).
- Data keyed by UUID: `players/data/<uuid>.dat`, `players/advancements/<uuid>.json` (both under the world, `server/mod.rs:244`), stats, `ops.json`, `whitelist.json`, `banned-players.json`, `usercache.json`. Inside the world, entity NBT refers to players by UUID (tamed animals' owner, item thrower and owner, projectile owner, anger targets, villager gossip, trial spawner and vault player lists). PEXGuard stores region members as UUID strings and PEXProtect logs UUIDs in SQLite.
- Upstream Pumpkin-MC/Pumpkin#4172 (open, approved 2026-10-24) makes `offline_uuid` vanilla's version 3 UUID; 4.1 merged it.
- Vanilla's own lists differ from PEX's in ways 4.2 must handle: `CachedUserNameToIdResolver.get` lowercases the name before `NameAndId.createOffline`, so vanilla's offline `whitelist add Notch` stores `notch` with notch's UUID; PEX hashes the name as typed (`command/argument_types/game_profile.rs:100`) and lowercases cache keys with `to_ascii_lowercase` instead of `toLowerCase(Locale.ROOT)`. PEX's `usercache.json` reader (`data/usercache.rs:196`) skips entries whose `expiresOn` isn't RFC 3339, so a vanilla `usercache.json` (`yyyy-MM-dd HH:mm:ss Z`) loads empty, entries expire after 30 days instead of one calendar month (`:221`), and PEX keeps the JSON lists in `data/` (`data/mod.rs:6`) where vanilla keeps them in the server root.
- The old UUID is a hash, so it can't be turned back into a name. A migration has to get names from `usercache.json` and the JSON lists, compute both UUIDs, and report player files it can't match.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 4.1 | Vanilla offline UUID in `offline_uuid`, plus a function that still computes the old one for the migration. Check every caller. | `fix/offline-uuid-vanilla` | | Unit test against UUIDs produced by the vanilla 26.3 server for a few names, including non-ASCII (get them from a vanilla `usercache.json` or `whitelist add`, not from a reimplementation) |
| 4.2 | Server file migration: map old to new UUIDs from `usercache.json`, ops, whitelist and bans; rename player data, advancements and stats; rewrite the JSON lists; back up the old files first; write the mapping to `players/uuid-migration.json`; record that it ran (where depends on Q4.3); report unmatched files. Runs per Q4.1. | `feat/offline-uuid-migration` | 4.1, Q4.1, Q4.3 | Unit test on a fixture directory; running it twice changes nothing the second time |
| 4.3 | World reference migration: rewrite player UUIDs in entity and block entity NBT using the mapping (owners, throwers, anger targets, gossip, trial spawner and vault lists). Generic walk over UUID tags, limited to UUIDs in the mapping. | `feat/offline-uuid-world` | 4.2 | Fixture world: a tamed wolf and a trial spawner keep their player references |
| 4.4 | Plugin data: PEXGuard members and PEXProtect history read `players/uuid-migration.json` and rewrite their UUIDs once. Plugin-only change. | `feat/offline-uuid-plugins` | 4.2 | Plugin unit tests; region membership survives the upgrade in the bot test below |
| 4.6 | Non-ASCII player names in command arguments: entity selectors reject names like `玩家一号` (`give 玩家一号 torch 21` gives "Invalid name or UUID", `tp 玩家一号 ...` gives "Expected double") while login, `op` and `whitelist add` accept them. Match vanilla `EntitySelectorParser` and `GameProfileArgument` (and decide with `is_valid_player_name`, 4.1's finding, what PEX accepts at login). | `fix/selector-player-names` | | Command tests for a non-ASCII name in `give`, `tp` and `@a[name=...]` |
| 4.5 | End-to-end proof with bots: a PEX 1.2.0 server in offline mode, bots build up inventory, advancements, op and whitelist status, a tamed wolf and a PEXGuard region; stop; start the 2.0 branch; the same bots log in and keep all of it. Then copy the world to a vanilla 26.3 offline server and check the player data loads there. | `test/offline-uuid-upgrade` | 4.3, 4.4 | Bot log and before/after NBT dumps in the PR |

### Proof

- Offline UUIDs equal vanilla's for the same names (unit test against vanilla output).
- 4.5's bot scenario: players keep inventory, position, advancements, op, whitelist, pets and region membership across the upgrade, and the world's player data loads in vanilla.

### Decisions

- **Q4.1** (answered 2026-10-25: automatic, with a backup first.) Migrate automatically on the first 2.0 start, or only when the admin passes a flag? Automatic is safer for players; a flag is safer for hosts who edit player files themselves.
- **Q4.2** (proceeding with vanilla UUIDs unless the maintainer objects, 2026-10-25) Should BungeeCord or Velocity setups that forward no UUID keep the old UUIDs for one release?
- **Q4.3** (answered 2026-10-25: in `pumpkin.toml`, through 3.1's `config_version`.) Where does "migration done" live: in `pumpkin.toml` (via `config_version`, which ties this to 3.1) or in the world (`data/pex/migrations.dat`), so a world copied to another server isn't migrated twice?
- **Q4.5** 4.1 merges the open upstream PR #4172 into its branch, as alpha has done with other upstream PRs. Acceptable, or a cherry-pick or an independent commit?
- **Q4.6** (proceeding with 4.2's choice unless the maintainer objects, 2026-10-25) 4.2 refuses to start the server when the migration fails, so players don't create new-UUID files that would collide with their old data later. Agree?
- **Q4.7** (proceeding with 4.2's choice unless the maintainer objects, 2026-10-25) Rerunning the migration on purpose (a 1.x world copied into a migrated server) means setting `config_version = 1`, which also reruns any later config steps. Acceptable while those steps are safe to run twice?
- **Q4.4** Which live servers run in offline mode? Servers 30 and 34 and the free tier will all go through this.

## Goal 5: redstone parity

### Current state

- The block update machinery is ported closely. `world/neighbor_updater.rs` is a stack-based updater with vanilla's 1,000,000 chained update limit (`neighbor_updater.rs:12`, hardcoded where vanilla reads `max-chained-neighbor-updates` from `server.properties`). `BlockDirection::update_order()` is vanilla's west, east, down, up, north, south.
- **Wire update order differs from vanilla.** Vanilla's `DefaultRedstoneWireEvaluator.updatePowerStrength` collects the wire and its six neighbours in a `HashSet<BlockPos>` and updates them in the set's iteration order, which depends on position (this is what makes vanilla redstone locational). PEX pushes them into a `Vec` in direction order (`block/blocks/redstone/redstone_wire.rs:294`).
- Quasi-connectivity is implemented: pistons check the block above (`block/blocks/piston/piston.rs:299`), and dispensers and droppers check `position.up()` (`redstone/dispenser.rs:228`, `redstone/dropper.rs:129`). Not yet audited against `PistonBaseBlock.getNeighborSignal` and `DispenserBlock.neighborChanged`.
- Tick priorities match vanilla where checked: repeaters use extremely high, very high and high as in `DiodeBlock` (lines 89 to 93), comparators use high or normal as in `ComparatorBlock.java:146`.
- 26.x experimental redstone (`redstone_experiments`) is recognised as a datapack feature (`data/datapack/mod.rs:621`), but there is no port of `ExperimentalRedstoneWireEvaluator`.
- Tests: about 145 of the 220 builtin GameTests cover redstone components and the blocks they drive (wire attenuation, corners, gates, latches, repeater delays and locking, comparators reading containers, pistons, observers, rails, doors, dispensers, hoppers). They check end states inside PEX only; none has been run on vanilla.
- Vanilla 26.3 registers `/test` on ordinary servers (`commands/Commands.java:258`), and PEX loads `test_instance` datapacks (`assets/tests/datapacks/`). The same block-based test can therefore run on both servers.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 5.1 | Dual-run harness: a datapack of block-based tests (structure plus `test_instance` JSON, using test blocks) that runs on vanilla 26.3 with `/test run` and on PEX, and a script outside the repo that runs both and compares results. Commit the datapack as test assets; keep the runner script outside the repo per `AGENTS.md`, or in `tools/` if it's worth keeping. Start with 5 existing redstone tests converted to block-based form. | `test/redstone-dual-run` | | The 5 tests pass on both servers; one deliberately broken PEX build fails exactly the affected test |
| 5.2 | Wire update order: iterate the wire and its neighbours in the order Java's `HashSet<BlockPos>` would (`Vec3i.hashCode`, `HashMap` hash spreading, 16 buckets for 7 entries). | `fix/redstone-wire-order` | 5.1 | Unit test against orders recorded from vanilla for several positions; a locational contraption test that behaves differently at two positions passes on both servers |
| 5.3 | Neighbour updater and BUD behaviour: audit against `CollectingNeighborUpdater` (chained update order, the limit, shape updates) and make the limit a config value like vanilla's | `fix/redstone-neighbor-updater` | 5.1 | Audit table in the PR; BUD switch and update-chain tests pass on both servers |
| 5.4 | Quasi-connectivity: pistons, sticky pistons, dispensers, droppers | `fix/redstone-qc` | 5.1 | QC piston, QC dispenser and BUD-powered piston tests pass on both |
| 5.5 | Pistons: push limit, immovable and block-entity blocks, slime and honey, moving piston edge cases, 0-tick behaviour vanilla allows | `fix/redstone-pistons` | 5.4 | Flying machine, piston door and slime pull tests pass on both |
| 5.6 | Comparators and repeaters: container reading and when it updates, subtract mode, item frames, pulse lengths, locking | `fix/redstone-diodes` | 5.1 | Comparator clock, pulse extender and hopper clock tests pass on both |
| 5.7 | Observers, torches (burnout), lamps, copper bulbs, rails, target blocks, daylight detectors | `fix/redstone-components` | 5.1 | One dual-run test per component |
| 5.8 | Hoppers, droppers and the crafter: transfer cooldowns, order, locking; item sorter and hopper clock | `fix/redstone-item-transport` | 5.6 | Item sorter test passes on both |
| 5.9 | Convert the remaining builtin redstone GameTests that can be block-based to the dual-run datapack | `test/redstone-dual-run-more` | 5.1 | Count of dual-run redstone tests in the PR, all passing on both |

5.2 to 5.7 can run in parallel after 5.1.

### Proof

- The dual-run redstone datapack (target: at least 60 tests, covering every component in 5.2 to 5.8, including at least one locational contraption) passes on vanilla 26.3 and on PEX with identical results.
- `cargo run -p pumpkin-tester` passes.

### Decisions

- **Q5.1** Default (non-experimental) redstone only, or also the 26.x experimental wire behind its datapack feature?
- **Q5.2** Is position-dependent (locational) behaviour required, or is "same result for contraptions that don't depend on it" enough? 5.2 matters only if it's required.

## Goal 6: survival parity from start to dragon kill

### Current state

The pieces of a playthrough exist in code; none has been proven end to end.

- Nether portals: `block/blocks/nether_portal.rs`, `world/portal/nether.rs` (portal search and creation), `world/portal/poi.rs`. End portals and gateways: `world/portal/end.rs`, `block/blocks/end_gateway.rs`, `block/entities/end_gateway.rs`. Credits: `world/portal/mod.rs:145` sends `GameEvent::WinGame`, and `seenCredits` is saved (`entity/player.rs:7025`).
- Structures in `crates/pumpkin-world/src/generation/structure/structures/`: stronghold, nether fortress, end city, mansion, ocean monument, mineshaft, ruined portal, temples and more. Bastions and other jigsaw structures generate through the generic jigsaw code (`Structure::BASTION_REMNANT` in `pumpkin-data`); not checked in game.
- The dragon fight manager is ported (`world/dragon_fight.rs`, 1,140 lines: spikes, crystals, respawn stages, exit portal, gateways, boss bar) and so are 11 dragon phases (`entity/boss/ender_dragon/phase/`). The dragon's fireball is a shortcut (see goal 1), and the `dragon_fireball` entity is missing.
- **The dragon's phase never reaches the client or the save.** Vanilla syncs it in `DATA_PHASE` and saves it as `DragonPhase` (`EnderDragon.java:57`, `:740`). PEX has `EnderDragonPhase::network_id()` but nothing calls it, `set_phase` (`entity/boss/ender_dragon.rs:286`) sends nothing, and nothing writes the phase to NBT. The enum's numbers also differ from vanilla's (PEX `Charging = 2`, vanilla `LANDING_APPROACH` is 2), PEX has a `FlyToPortal` phase vanilla doesn't, and vanilla's `SittingScanning` has no PEX counterpart.
- Eyes of ender: `EyeOfEnder` entity is wired (`entity/type.rs:325`).
- Block entity loss on unload (`fix/flush-block-entities-before-unload`, unmerged) breaks any playthrough that stores items in chests away from players.
- Bots: `tools/pumpkin-tester` runs an in-process server with Rust bots built on `pumpkin-protocol` (login, move, commands, use block, dig) and Mojang-style GameTests; CI runs it. `native-plugins/tools/testbot` is an Azalea bot (nightly Rust, protocol patches). `/srv/pex/tools/survbot.py` has builder, miner, AFK and explorer roles. None of them crafts, fights or uses portals.
- Earlier survival bot runs (`survtest*` in the pve archive) only covered building and mining.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 6.1 | Survival stage harness in `pumpkin-tester`: each stage starts from a seeded world plus setup commands, drives one bot through real client actions (container clicks, digging, attacking, using items) and checks the outcome. Add the bot actions the first stage needs. Stage 1: wood to crafting table to wooden and stone pickaxe. | `test/survival-harness` | | Stage 1 passes in `pumpkin-tester`; it fails if the crafting table recipe is removed |
| 6.2 | Stage 2: iron. Mine ore, smelt in a furnace, craft a bucket, shield and iron tools; eat food when hungry; sleep in a bed to skip the night and set spawn; die and respawn at the bed. | `test/survival-iron` | 6.1 | Stage passes; fixes for anything it finds land in their own commits |
| 6.3 | Stage 3: nether portal. Make obsidian from water and lava, build and light a portal, travel, check the portal linked or created on the other side, return. | `test/survival-portal` | 6.1 | Stage passes on a fixed seed in both directions |
| 6.4 | Stage 4: nether. Locate a fortress and a bastion, kill blazes for rods, barter with piglins for pearls (or kill endermen in a warped forest). | `test/survival-nether` | 6.3, 1.6, 1.7 | Bot ends the stage with blaze rods and ender pearls obtained through game mechanics, not `/give` |
| 6.5 | Stage 5: stronghold. Craft eyes of ender, throw them and follow them, find the portal room, fill the frame, activate the portal. | `test/survival-stronghold` | 6.4 | Eyes fly toward the stronghold `/locate` reports; portal activates with the last eye |
| 6.6 | Dragon phases, fireball and fight audit: map the phases 1:1 to vanilla's `EnderDragonPhase` ids (add `SittingScanning`, fold `FlyToPortal` into what vanilla does), sync `DATA_PHASE` and save `DragonPhase`; add the `dragon_fireball` entity (a projectile that bursts into the breath cloud); audit crystal healing, perching and the death sequence against `EnderDragon`, the phase classes and `EndDragonFight`. Could be two sessions (phases, then fireball and fight). | `fix/dragon-fight` | 1.1 | Rows for ender_dragon in `docs/MOB-PARITY.md`; the client sees the sitting animations; a dragon saved mid-fight reloads in the same phase; GameTests for crystal healing and the fireball |
| 6.7 | Stage 6: the End. Arrive on the obsidian platform, destroy the crystals, kill the dragon with bow and melee, get the exit portal, egg and gateway, enter the exit portal, see the credits, respawn at the bed. | `test/survival-end` | 6.5, 6.6 | Stage passes: dragon dies from bot damage, `WinGame` is received, player respawns |
| 6.8 | Audit the survival systems the stages lean on: crafting and smelting recipes, loot tables for the drops used, enchanting, anvils, brewing (fire resistance), beds and respawn anchors, hunger. Fix or file each gap. | `fix/survival-systems-audit` | 6.1 | Audit table in the PR with the vanilla class and method for each |
| 6.10 | Block breaking: `FinishedDigging` breaks the block without checking how long the player dug (`net/java/play/player_action.rs`), so a client can break any block instantly in survival; vanilla `ServerPlayerGameMode.handleBlockBreakAction` needs the same position and `progress * (ticks + 1) >= 0.7`. Mining fatigue in `entity/player.rs:5079` still uses the old 0.0027 and 8.1E-4 factors; 26.3 `Player.getDestroySpeed` uses `0.3^(amplifier + 1)` and the `block_break_speed` attribute. | `fix/block-break-timing` | | Unit tests on the progress check and fatigue; a `pumpkin-tester` bot that sends an early finish gets the block back instead of a break |
| 6.11 | Teleport confirmation: `net/java/play/confirm_teleport.rs:35` kicks with "Wrong teleport id" when a confirm for an older teleport arrives after a newer one; vanilla `handleAcceptTeleportPacket` ignores ids it isn't waiting for. Kicks bots and laggy players after `/tp`. | `fix/teleport-confirm-id` | | Bot test: two teleports in a row with the first confirm sent late, no kick |
| 6.12 | Daylight detector tick panic: the block entity's tick panics (`block/entities/daylight_detector.rs:103`, `to_state_id` on air) when its block is gone; vanilla `LevelChunk.BoundTickingBlockEntity.tick` checks `isValid` first. Find what leaves the block entity behind and add the check. | `fix/daylight-detector-orphan` | | Regression test that ticks a detector block entity whose block was removed |
| 6.13 | Spawn chunk preload: `spawn_java_player` (`crates/pumpkin-core/src/world/mod.rs:3417`, `:3442`) loads only the spawn chunk. Vanilla `PrepareSpawnTask` keeps the player in configuration until chunks within radius 3 are loaded (`addTicketAndLoadWithRadius(PLAYER_SPAWN, spawnChunk, 3)`) and `waitForEntities` has their entities. Players can step into chunks still generating. | `fix/prepare-spawn-preload` | 0.4 | A bot that walks a chunk away right after joining finds the chunk loaded; join time measured before and after |
| 6.14 | Teleport follow-ups from 6.11: `handle_move_vehicle` ignores a pending teleport (vanilla `handleMoveVehicle` returns early, `ServerGamePacketListenerImpl.java:462`); block use isn't refused while a teleport is pending (`:1435`); the join teleport in `spawn_java_player` (`world/mod.rs:3462`) is sent after the spawn chunk loads and undoes a `/tp` in that window; `Player::wake_up` (`entity/player.rs:2408`) sends no teleport where vanilla `stopSleepInBed` does; `/tp` of a riding player doesn't dismount it (vanilla `ServerPlayer.teleport` calls `removeVehicle()`). | `fix/teleport-follow-ups` | 6.11 | A bot or GameTest per case |
| 6.9 | Full chained run: all stages back to back on one fixed seed and one world, without resetting between stages, run in the CT with the release build | `test/survival-full-run` | 6.2 to 6.7 | Log and timings of a full run in the PR; run three times without a failure |

### Proof

- Every stage passes in `pumpkin-tester` (in CI for the quick stages; the full run in 6.9 may be too slow for CI).
- 6.9's chained run on one seed reaches the credits three times in a row.
- A human play-test checklist covering what bots can't see (rendering, sounds, particles of portals and the dragon).

### Decisions

- **Q6.1** What counts as proof: a bot run with staged setup commands (teleports and `/locate` allowed, items only from gameplay), a fully unassisted bot, or a human playthrough as well?
- **Q6.2** Which known deviations are accepted (client-authoritative movement, anti-cheat differences)?
- **Q6.5** Vanilla 26.3 `ServerPlayerGameMode.tick` passes `delayedTickStart` itself as the tick count of a delayed destroy (bytecode offset 51), so for anyone online about 150 ticks a delayed destroy finishes on the next tick. Paper passes `gameTicks - delayedTickStart`. 6.10 follows Paper. Keep that, or copy vanilla's behaviour?
- **Q6.6** PEX counts player ticks like vanilla, without Paper's lag compensation, so when TPS drops a correctly timed dig takes the delayed path: the block flickers back and breaks a moment later. Acceptable?
- **Q6.7** 6.11 keeps vanilla's teleport resend, so a client with a round trip over about 1 s stays frozen after a teleport while it keeps moving, exactly like vanilla 26.3 (alpha let it through after one late confirm). Keep vanilla's behaviour?
- **Q6.8** 6.11 changed `Player::awaiting_teleport`'s type and removed `teleport_id_count`, both public to native plugins. No bundled plugin uses them. Bump `PLUGIN_API_VERSION`?
- **Q6.4** The `pumpkin-tester` world uses a random seed. Stages that depend on terrain (6.3 onward) need a fixed one: on the shared tester server, or on a separate server per stage?
- **Q6.3** Should `fix/flush-block-entities-before-unload` (or the design decision in `ISSUE-block-entities-lost-on-unload.md`) be settled before 6.2? Survival without reliable chests isn't survival parity.

## Goal 7: worlds round-trip with vanilla 26.3

### Current state

- Reading vanilla 26.3 worlds works since `fix/load-vanilla-26-3-worlds` (merged): data version 5023, bare-id palettes from `BlockStateFieldNamesFix`, unpadded region files. Its PR draft lists what isn't covered: the item and block entity format changes in data versions 4996 to 5016 (pot decorations, sign op flag, swing animation, explorer maps, map colour and block transformer components, the unfilled buried treasure map).
- PEX uses the 26.x folder layout (`dimensions/<namespace>/<name>/region|entities|poi`, falling back to `DIM-1` and `DIM1`, `crates/pumpkin-world/src/level.rs:156`) and `players/data` for player files, like vanilla's `DimensionStorageFileFix` and `PlayerStorageFileFix`.
- **Default chunk format is Pump** (`crates/pumpkin-config/src/chunk.rs:20`). Vanilla can't read `.pump` files, so a default PEX world never round-trips. Anvil's default compression is LZ4 (`chunk.rs:43`), which vanilla reads.
- Chunks save with `DataVersion` 5023 (`chunk/format/anvil.rs:31`). POI data defaults to `DataVersion` 3955 (`poi/mod.rs:26`) and structure templates write 4189.
- Conversion deletes the source region files: both the automatic conversion when a world's files don't match the configured format (`crates/pumpkin-world/src/level.rs:250`) and `--convert-world` (`crates/pumpkin/src/main.rs:100`) pass `remove_source = true`.
- Nothing tests a round trip. The only vanilla fixtures are worldgen ones (`assets/tests/`).

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 7.1 | Vanilla fixture world: build a small world with the vanilla 26.3 server (jar in `/srv/pex/vanilla-26.3`) containing chests with enchanted and named items, signs, banners, pots, spawners, a lectern, item frames, maps, villagers with trades, tamed and leashed animals, passengers, a player with inventory and advancements, data in `data/` (scoreboard, map, raid, world border). Script the build with server commands so it can be regenerated for the next version. | `test/vanilla-fixture-world` | Q7.2 | Fixture committed (or stored where Q7.2 says) with the script that made it |
| 7.2 | PEX side round trip: load the fixture, save it as Anvil, and compare every chunk, entity chunk, POI chunk, player file and saved data file tag by tag with the original, with an explicit list of fields vanilla itself rewrites. Integration test in `pumpkin-world` or `pumpkin-tester`. | `test/world-roundtrip` | 7.1 | Test passes; each ignored field has a reason in the list |
| 7.3 | Item and block entity formats from data versions 4996 to 5016 that 7.2 shows are lost or changed | `fix/world-format-5016` | 7.2 | 7.2 passes with those fields no longer ignored |
| 7.4 | Version constants and saved data: POI `DataVersion`, structure template version, `level.dat` fields, everything under `data/` | `fix/world-saved-data` | 7.2 | 7.2 covers `level.dat` and `data/` with no unexplained differences |
| 7.5 | Vanilla side: start vanilla 26.3 on a world saved by PEX, let it load every chunk of the fixture area, stop, and check the log for data fixer warnings, chunk errors and lost block entities, then compare vanilla's re-save with PEX's save | `test/world-roundtrip-vanilla` | 7.2 | Script and log in the PR; no errors in vanilla's log |
| 7.8 | Player data saves: `crates/pumpkin-world/src/data/player_data.rs:124` writes `<uuid>.dat` with `File::create`, so a crash mid-save truncates it; vanilla `PlayerDataStorage.save` writes a temp file and swaps it in with `Util.safeReplaceFile` and a `.dat_old`. | `fix/player-data-atomic-save` | | Test that a write interrupted after the temp file leaves the old data loadable |
| 7.9 | Advancements and statistics in vanilla's formats: PEX writes `{"adv": {"criterion": {"secs_since_epoch", ...}}}` (`entity/player/advancement.rs`) instead of vanilla `PlayerAdvancements` (`criteria`, `done`, `DataVersion`), and keeps statistics in the player NBT instead of `players/stats/<uuid>.json`. A world moved to vanilla loses both. Read and write vanilla's formats, migrating PEX's once. | `fix/vanilla-advancement-stats-format` | 3.1 | 7.2's round trip covers `players/advancements` and `players/stats` with no differences |
| 7.10 | Ops, whitelist and ban lists: `LoadJSONConfiguration::load` (`crates/pumpkin-core/src/data/mod.rs`) falls back to an empty list when the file doesn't parse, and the next save overwrites the admin's file. Refuse to overwrite a file that failed to load (keep it, log the error by name), like vanilla's `StoredUserList.load` failure handling. | `fix/json-list-load-safety` | | Test: a broken `ops.json` is left byte for byte after a save |
| 7.6 | Conversion safety: keep or back up source files during conversion according to Q7.3 | `fix/convert-keep-source` | Q7.3 | Unit test: the converter leaves the sources (or a backup) |
| 7.7 | Long-play round trip: run the survival stages from goal 6 on PEX, then open the world in vanilla and check inventories, chests and the End state (dragon killed, gateways) | `test/world-roundtrip-play` | 7.5, 6.7 | Vanilla loads it with the dragon fight marked done and every chest intact |

### Proof

- 7.2's test passes in CI: vanilla world to PEX and back to disk with no unexplained NBT differences.
- 7.5: vanilla 26.3 opens a PEX-saved world with no errors and the same content.
- 7.7: a world played on PEX opens in vanilla with progress intact.

### Decisions

- **Q7.1** Default chunk format for 2.0: Pump (upstream's default, faster) or Anvil (round-trips with vanilla)? If Pump stays the default, round-trip parity only applies after `--convert-world anvil`.
- **Q7.2** Can a fixture world of a few MB be committed to the repository, or should it live in a release asset or be regenerated by the script in CI?
- **Q7.3** On automatic conversion: keep the source files, move them to a backup folder, or keep deleting them as now?

## Goal 8: Bedrock polish

### Current state

- Bedrock targets 1.26.51, protocol 2193 (`crates/pumpkin-world/src/lib.rs:26`). It has RakNet and NetherNet transports (`net/bedrock/nethernet/`), Xbox authentication, and mapping data from Geyser and CloudburstMC (`assets/bedrock/`).
- 64 Bedrock clientbound packets are implemented (`crates/pumpkin-protocol/src/bedrock/client/`) against 146 Java play packets.
- Gameplay code sends Java packets. `JavaPlayer::send_packet`, `Player::send_client_packet` and `try_send_client_packet` (`entity/player.rs:78`, `:3786`, `:3778`) do nothing for Bedrock clients, and there are 21 `if let ClientPlatform::Java` branches in `pumpkin-core`. A Bedrock player gets a feature only where someone added a Bedrock path, so any feature without one is silently missing for them. Particles (`spawn_particle`, `player.rs:2496`), stats (`send_stats`, `:3393`) and permission level updates (`:3595`) are Java only.
- Known TODOs: falling block and TNT metadata (`entity/falling.rs:148`, `entity/tnt.rs:169`), temperature variants of pigs, cows and chickens (`entity/mod.rs:525`), permission levels (`world/mod.rs:2905`), frozen time (`world/time.rs:131`).
- No Bedrock test exists. The `bedrock` tracker category in `feat/tracker` has entries but no audits.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 8.1 | Coverage report: list every gameplay call site that sends a Java packet, and for each, whether a Bedrock player gets an equivalent. Put the table in `docs/BEDROCK.md`, grouped by feature. No code changes. | `docs/bedrock-coverage` | Q8.1 | Table covers every Java packet type sent from `pumpkin-core`; each row says covered, partial or missing |
| 8.2 | Bedrock test client: pick a client library that speaks 1.26.50 (check gophertunnel and PrismarineJS `bedrock-protocol` versions first), script login, chunk receipt, movement, block break and place, chat and a container | `test/bedrock-bot` | Q8.1 | Script outside the repo (or in `tools/` if kept), passing against alpha, in the PR |
| 8.3 | Entity metadata batch: falling block variant, TNT fuse, temperature variants, the other `mob_bedrock_spawn_metadata` gaps from 8.1 | `fix/bedrock-entity-metadata` | 8.1 | Bot or capture shows the metadata; human checklist for rendering |
| 8.4 | Permissions, time, particles and stats for Bedrock players | `fix/bedrock-player-state` | 8.1 | Each covered in `docs/BEDROCK.md`; bot checks where possible |
| 8.5 | Containers and inventories: every screen a survival player opens (crafting table, furnace, anvil, enchanting, brewing, chests, shulker boxes, villager trading) | `fix/bedrock-containers` | 8.2 | Bot opens and uses each; human checklist |
| 8.6 onward | One piece per remaining "missing" group from 8.1, in the order of Q8.2 | `fix/bedrock-<group>` | 8.1 | Row moves to covered |

### Proof

- `docs/BEDROCK.md` shows every feature as covered or as an accepted gap.
- 8.2's bot scenario passes against the release build.
- A human with a real Bedrock client (phone or console) runs a checklist: join, play survival for 15 minutes, use the containers, see mobs and particles.

### Decisions

- **Q8.1** Is Bedrock a 2.0 requirement, or does 2.0 ship it as experimental with the coverage table as the honest list of gaps?
- **Q8.2** Which Bedrock features matter most (survival basics, plugins' forms, cross-play chat)?
- **Q8.3** Who has a Bedrock client for the human checklist, and on what device?

## Goal 9: performance against Paper

### Current state

Measured with the gameplay benchmark (`/srv/pex/gameplay-bench`; reports in `bench-game3` and `bench-game5` of the pve archive; 4 cores each, 26.3, identical worlds and bot schedules):

| Run | PEX tick (mean) | Paper tick (mean) | PEX p95 | Paper p95 | PEX network sent | Paper network sent |
| :-- | --: | --: | --: | --: | --: | --: |
| `bench-game3`, 8 bots, 2 h (PEX 1.0 era) | 7.1 ms | 13.7 ms | 17.7 ms | 17.1 ms | 923 MiB | 614 MiB |
| `bench-game5`, 8 bots (min 90 to 120), PEX `74d1a6ad4` | 6.3 ms | 12.4 ms | | | | |
| `bench-game5`, 24 bots (min 130 to 180) | 12.0 ms | 20.5 ms | 20.4 ms | 25.0 ms | 2,831 MiB (whole run) | 1,959 MiB |

- PEX's mean tick is about half of Paper's, but in `bench-game3` its p95 was no better than Paper's, so its slow ticks are spiky. The 24-bot numbers in `bench-game5` flatter PEX: the harness couldn't force-load chunks on PEX, so four miners dug through air and six builders had no chest (its report says so).
- PEX sends 45 to 50% more data than Paper. Nobody has broken that down by packet type yet.
- These numbers predate the upstream merges in 1.2 (region IO, entity index) and the mob AI merges on alpha. Nothing has been measured on alpha.
- Entity tracking uses vanilla's per-type update interval and delta flags (`world/entity_tracker.rs:195`, `:216`). Compression threshold defaults to 256 like vanilla.
- The benchmark lives outside the repository. Its whitelist helper has to know PEX's non-vanilla offline UUIDs (goal 4).

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 9.1 | Re-baseline: run the gameplay benchmark on alpha, PEX 1.2.0 and Paper 26.3 on the hardware Q9.1 picks, fixing the force-load problem first so the 24-bot phase is comparable. No code changes in PEX. | `perf/rebaseline` (worktree only, results in the PR draft) | Q9.1 | Report with mean, p50, p95, p99 tick, memory, CPU and bytes sent for all three |
| 9.2 | Network breakdown: count bytes per packet type on both servers during the same bot session (bot-side counting works for both, since they speak the same protocol), and list the packet types where PEX sends more. A one-hour 8-bot session on alpha and Paper is enough; it doesn't need 9.1's full run. | `perf/network-breakdown` | | Table of the top 10 packet types by excess bytes, with counts and bytes per player per minute |
| 9.3 | Fix the largest network excess from 9.2 (one cause per piece; repeat as 9.3a, 9.3b...) | `perf/net-<cause>` | 9.2 | Rerun shows the excess for that packet type gone or explained, and no tick regression |
| 9.4 | Tick spike breakdown: temporary tracing spans around the tick phases, record the slowest 5% of ticks over a benchmark run and attribute them (autosave, chunk loading or generation sync points, entity islands merging, region IO, allocator) | `perf/tick-spikes` | 9.1 | Table of causes with their share of slow ticks |
| 9.5 | Fix the largest spike cause from 9.4 (repeat per cause) | `perf/spike-<cause>` | 9.4 | p95 drops on a rerun, same method |
| 9.6 | Publish: `docs/BENCHMARKS.md` with method, hardware, versions, commands and results, and the HTML report as a release asset or page. Move the benchmark into the repository if Q9.3 says so. | `docs/benchmarks` | 9.3, 9.5 | Document merged; numbers reproducible from it |
| 9.7 | A short benchmark in CI (minutes) that fails on a large tick or network regression | `ci/perf-smoke` | 9.6 | Local CI step (and a workflow job for later) with a recorded baseline |

### Proof

- Published benchmark (9.6) on the agreed hardware where PEX's network bytes per player and p95 tick are at or below Paper's at 8 and 24 players, with mean tick and memory not worse than the 1.0 numbers.
- Raw data and the exact commands are published with it.

### Decisions

- **Q9.1** Which hardware? pve runs production next to the benchmark, which adds noise; a dedicated box or a cloud VM is cleaner. Earlier runs used a Pelican node with 4 cores per server.
- **Q9.2** What is the bar: beat Paper on p95 and network, or only get within some margin? Which player counts and duration?
- **Q9.3** Should the gameplay benchmark move into this repository (it has its own license file today)?

## Goal 10: reproducible release builds in CI, ARM64 and Windows on real hardware

### Current state

- **CI has not run since 2026-10-14, because GitHub disabled Actions for the repository.** The repository setting says enabled and all eight workflows are active, but `gh workflow run` returns `HTTP 422: Actions has been disabled for this repository`. The last run finished at 19:43 UTC on 2026-10-14; the first push without a run was at 22:01 UTC. Nothing has run since: pushes to `alpha` and `master`, PRs #1 and #2, the `v1.0.0` to `v1.2.0` tags and the nightly cron (piece 10.1, `/srv/pex/pr-drafts/ci-restore-actions.md`). Before the stop the fork had used about 2,500 job minutes in two days (1,171 Linux, 858 Windows, 470 macOS).
- `rust.yml` has never run on `alpha`: `alpha` was added to its push branches on 2026-10-15, after the stop.
- Several workflows would do unwanted things on the fork as soon as Actions comes back: `nightly.yml:166` publishes the nightly with `make_latest: true`, which would replace v1.2.0 as the Latest release (without plugin zips or `SHA256SUMS`); `reviewers.yml:117` comments on every PEX PR mentioning upstream Pumpkin maintainers; `docker.yml` would publish `ghcr.io/conman1533/pex` for the first time; `sync-wit.yml` has no repository guard and would try to push to `Pumpkin-MC/pumpkin-plugin-wit`; `release.yml` also runs on the hand-moved `alpha` tag and needs SignPath secrets the fork doesn't have.
- **Releases were built by hand on pve.** `tools/build-release.sh` (now in `/srv/pex/tools`) cross-compiled from `/Vault-Mk4/pex-wt/rel1` with fat LTO (aarch64 with `aarch64-linux-gnu-gcc`, Windows with mingw `x86_64-pc-windows-gnu`, musl with `musl-gcc`). The v1.2.0 release has X64 Linux (glibc and musl), ARM64 Linux, X64 Windows (GNU toolchain, unsigned) and plugin zips for those three glibc builds, plus `SHA256SUMS`. There is no Windows ARM64 or macOS binary.
- `release.yml` would build on native runners for X64 and ARM64 Linux, X64 and ARM64 Windows and macOS, sign Windows with SignPath (secrets the fork probably doesn't have), and publish. It builds without `--locked` (`release.yml:55`), and `rust-toolchain.toml` says `channel = "stable"`, so two builds of the same tag a month apart use different compilers.
- Nothing makes builds reproducible: no pinned toolchain, no `--remap-path-prefix` (panic messages embed the build path), no check that two builds match. `crates/pumpkin-core/build.rs` embeds the commit hash, which is fine because it's an input.
- Release smoke tests: none in CI. `/srv/pex/tools/release-smoke.sh` exists.
- The Pelican "PEX Plugins" panel plugin downloads release assets by name (`pumpkin-plugins-X64-Linux.zip`), so asset names are an interface.
- ARM64 hardware: the Orbi router PEX build is armv7, not ARM64. No ARM64 or Windows machine for PEX testing is recorded.

### Pieces

| ID | Piece | Branch / short name | Depends on | Done means |
| :-- | :-- | :-- | :-- | :-- |
| 10.1 | Find out why Actions stopped after 2026-10-14 and get CI running on `alpha` again (read-only investigation first: workflow syntax at each commit, repository and account Actions settings, run history through the API). Anything that needs a settings change goes to the maintainer as a question. | `ci/restore-actions` | | The cause is identified; if it's fixable in the repo, `gh workflow run rust.yml --ref ci/restore-actions` (the workflow has `workflow_dispatch`) produces a run and its result is in the PR draft |
| 10.7 | Fork-safe workflows: before Actions is re-enabled, guard or change the workflows listed above according to Q10.6 (nightly not marked Latest, no upstream reviewer pings, no Docker publish or WIT sync unless wanted, release not triggered by the `alpha` tag) | `ci/fork-safe-workflows` | Q10.6 | Each workflow's trigger and guard reviewed in the PR draft; `actionlint` passes |
| 10.2 | Pin the toolchain (`rust-toolchain.toml` to an exact version), `--locked` on every build, `--remap-path-prefix` for the workspace and cargo home, and a script that builds the release binary twice in different directories and compares sha256 | `ci/reproducible-builds` | | Two builds of the same commit in different paths in the CT produce identical `pumpkin` binaries for x86_64 Linux glibc |
| 10.3 | Release workflow for PEX: tag-triggered, all targets on native runners, plugin zips for every glibc target, `SHA256SUMS`, asset names the panel plugin expects, SignPath step removed or made optional per Q10.2, build provenance attestation | `ci/pex-release` | 10.2, 2.1 | While Actions is off: the release steps also exist as a script that runs in the CT and builds every asset it can there (x86_64 glibc and musl, aarch64 and Windows x64 cross-compiled), with plugin zips, `SHA256SUMS` and the asset names the panel plugin expects; `actionlint` passes on the workflow. Later: a `workflow_dispatch` dry run builds every asset on native runners without creating a tag or release |
| 10.4 | Release smoke tests in CI: each binary boots to "Done" in a scratch directory, loads its plugin zip (glibc builds), runs a few console commands, stops cleanly; on the native ARM64 Linux and ARM64 Windows runners | `ci/release-smoke` | 10.3 | The smoke script passes in the CT for x86_64 natively, aarch64 under `qemu-user` and Windows x64 under `wine`; the native-runner job waits for Actions |
| 10.5 | Reproducibility check in CI: a second job rebuilds the x86_64 Linux binary in a different path and fails if the hash differs; document how anyone can verify a release in `docs/RELEASING.md` | `ci/reproducibility-check` | 10.2, 10.3 | The check passes in the CT and is part of `local-ci.sh --full`; the workflow job waits for Actions; the doc explains the check |
| 10.6 | Real hardware: run the release binary on an ARM64 Linux machine and a Windows machine (Q10.1), each with the survival stage 1 bot and a 30 minute 8-bot benchmark session, and record the results | `test/real-hardware` | 10.4, Q10.1 | Logs and numbers per machine in the PR |

### Proof

- A tag produces the full release in CI with no manual steps.
- Rebuilding a release from its tag gives the same sha256 for the x86_64 Linux binary (and for the others if Q10.3 asks for it).
- The ARM64 and Windows binaries boot, play a bot session and pass the smoke tests on real hardware, not only on CI runners.

### Decisions

- **Q10.1** Which ARM64 and Windows machines can be used for the real hardware runs?
- **Q10.2** Windows signing: set up SignPath (or another certificate) for the fork, or ship unsigned Windows builds?
- **Q10.3** Which targets must be bit-for-bit reproducible: only x86_64 Linux, or every target? Windows and macOS are much harder.
- **Q10.4** Should releases keep the musl build that can't load plugins (see goal 2)?
- **Q10.5** Actions is disabled by GitHub, not by a setting in the repository. The owner (Conman1533) has to look at the Actions tab and settings while logged in, check for a GitHub email from around 2026-10-14 and the billing page, and open a support ticket if needed. Nothing in the repository can fix it.
- **Q10.6** Which workflows should run on the fork at all: nightly releases (and should they ever be Latest), the upstream reviewer bot, the Docker image, the WIT mirror?

## Order across goals

Pieces with no decision attached can start now. Merge `fix/clippy-alpha` before anything else. Wave 1 (three at a time in the CT):

1. 1.1, 2.1, 2.2, 3.1, 4.1, 5.1, 6.1, 9.2, 10.1, 10.2. 7.1 joins as soon as Q7.2 is answered.

Then, as decisions arrive and wave 1 merges:

2. Goal 1 families (non-Brain ones right after 1.1); 3.2 and 3.3; 4.2 to 4.4; 5.2 to 5.8; 6.2 and 6.3; 7.3 and 7.4; 8.1 and 8.2; 9.1 and 9.4; 10.3.
3. 6.4 to 6.7 (need goal 1 nether mobs); 7.5 to 7.7; 9.3 and 9.5; 10.4 and 10.5; Bedrock fix pieces.
4. 6.9, 9.6, 9.7, 10.6, and goal 2's removal pieces once Q2.1 and Q2.2 are answered.

Cross-goal dependencies to keep in mind:

- 4.2 needs 3.1 if Q4.3 puts the migration marker in `pumpkin.toml`.
- 6.4 needs 1.6 and 1.7; 6.6 is the dragon's row in goal 1.
- 7.7 needs 6.7.
- 9.1 should wait for 10.1 only if Q9.3 moves the benchmark into CI.
- 10.3 needs 2.1 so plugin zips carry the build id.
- The block entity unload fix (Q6.3) affects 6, 7 and the benchmark's chest counts.

## All decisions

| ID | Question |
| :-- | :-- |
| Q1.1 | Must the 21 vanilla Brain mobs run on PEX's Brain framework, or are goal-based ports with matching behaviour acceptable? |
| Q1.2 | Per-mob tests here only, or also offered upstream? |
| Q1.3 | Are villagers (three sessions) in scope for 2.0? |
| Q1.4 | Are mock players with a closed client acceptable in GameTests? |
| Q2.1 | Versioned native plugin ABI, or explicitly unstable (same release only)? |
| Q2.2 | When v0.1 goes: finish WASM v0.2, or drop WASM plugins entirely? |
| Q2.3 | Are third-party native plugins a 2.0 goal? |
| Q2.4 | Is a build id that can't see dependency features acceptable? |
| Q3.1 | Invalid config value: refuse to start, or use that key's default and warn? |
| Q3.2 | Migrate `pumpkin.toml` in place with a backup, or write the result beside it? |
| Q3.3 | Keep comments in `pumpkin.toml` at the cost of a `toml_edit` dependency? |
| Q3.4 | Newer `config_version` than the server knows: warn and start, or refuse? |
| Q3.5 | One log line per filled-in key, or one summary line? |
| Q4.1 | Answered: automatic on first start, with a backup |
| Q4.2 | Keep old UUIDs for one release behind proxies that forward none? |
| Q4.3 | Answered: marker in `pumpkin.toml` (3.1's `config_version`) |
| Q4.4 | Which live servers run in offline mode? |
| Q4.6 | Refuse to start when the UUID migration fails? |
| Q4.7 | Rerun the migration by setting `config_version = 1`? |
| Q4.5 | Merge open upstream PR #4172 for 4.1, or cherry-pick or rewrite? |
| Q5.1 | Default redstone only, or experimental redstone too? |
| Q5.2 | Is locational (position-dependent) behaviour required? |
| Q6.1 | Proof of survival parity: staged bot, unassisted bot, human, or a mix? |
| Q6.2 | Which deviations are accepted? |
| Q6.3 | Settle block entity loss on unload before the survival stages? |
| Q6.4 | Fixed seed for terrain-dependent survival stages: shared tester server or one server per stage? |
| Q6.7 | Keep vanilla's teleport resend for very slow clients? |
| Q6.8 | Bump `PLUGIN_API_VERSION` for 6.11's public field change? |
| Q6.5 | Delayed destroy timing: Paper's fix (6.10's choice) or vanilla 26.3's bug? |
| Q6.6 | No lag compensation for dig timing: acceptable? |
| Q7.1 | Default chunk format for 2.0: Pump or Anvil? |
| Q7.2 | Commit a fixture world of a few MB, or keep it outside the repo? |
| Q7.3 | On conversion: keep, back up, or delete source region files? |
| Q8.1 | Bedrock: 2.0 requirement or experimental? |
| Q8.2 | Which Bedrock features first? |
| Q8.3 | Who runs the real Bedrock client checklist, on what device? |
| Q9.1 | Benchmark hardware? |
| Q9.2 | The performance bar against Paper: player counts, duration, which numbers? |
| Q9.3 | Move the gameplay benchmark into this repository? |
| Q10.1 | Which ARM64 and Windows machines for real hardware runs? |
| Q10.2 | Sign Windows builds, or ship unsigned? |
| Q10.3 | Which targets must be bit-for-bit reproducible? |
| Q10.4 | Keep the musl build that can't load plugins? |
| Q10.5 | Actions is disabled by GitHub: will the owner check the account and contact support? |
| Q10.6 | Which workflows should run on the fork (nightly as Latest, reviewer bot, Docker image, WIT mirror)? |

## Piece status

Alpha `d30c773bb` (2026-10-25) holds 0.1, 0.2, 1.1, 3.1, 5.1, 6.1 and 6.10, and the roadmap itself; alpha `84e5d54f9` (2026-10-26) adds 2.1, 0.4 and 6.11, after a full local CI with only the known failures. Its full local CI showed only the known failures. The offline UUID work (4.1 to 4.4) goes in together, once 4.3 and 4.4 are done, so the migration ships complete.

| Piece | Branch | State | PR draft |
| :-- | :-- | :-- | :-- |
| 0.1 (alpha clippy errors, found by the baseline) | `fix/clippy-alpha` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `ff3ff4f28`, local CI green except the known scaffolding GameTest | `pr-drafts/fix-clippy-alpha.md` |
| 7.8 | `fix/player-data-atomic-save` | in progress | |
| 4.3 | `feat/offline-uuid-world` | in progress (base: alpha plus 4.2) | |
| 4.2 | `feat/offline-uuid-migration` | done, pushed `cffeb7e41` (contains 4.1): on the first start, offline player files (`players/data`, `advancements`, `stats`, the `.dat` `UUID` tag) and the ops, whitelist, ban and user cache lists move to vanilla UUIDs, after backups (`world/players.v1.bak`, `data/<list>.json.v1.bak`), with `world/players/uuid-migration.json` for 4.3 and 4.4. `config_version = 2` is stamped only after success; a failure exits with code 1 and leaves `pumpkin.toml` at 1, so the next start retries. Boot test 1.2.0 to branch: same op, positions, inventories and advancements; restart and resume are byte-identical. Local CI green apart from the known and flaky failures. Needs a real-client play test and a recording. Goes into alpha with 4.1, 4.3 and 4.4. | `pr-drafts/feat-offline-uuid-migration.md` |
| 0.4 | `fix/tester-setblock-flake` | **merged into alpha `84e5d54f9` on 2026-10-26**, pushed `de124041a` (3 commits): the flake was mostly a server bug. The chunk unload pass could unload a chunk whose new ticket was still in the `LevelChannel`, so a player joining into a chunk someone had just left could lose blocks placed in it. Now the pass puts such a chunk back, like vanilla `ChunkMap.updateChunkScheduling`, with a regression test that fails on alpha. The tester also waits for the chunks around the bot and keeps silent bots alive. 40 runs in a row pass under load, against 18 failures in 50 on alpha. Local CI green apart from the known failures. Needs a real-client check (join-and-place right after another player leaves) and a capture. | `pr-drafts/fix-tester-setblock-flake.md` |
| 2.1 | `feat/plugin-build-id` | **merged into alpha `84e5d54f9` on 2026-10-26**, pushed `4ccd66fca` (on alpha `d30c773bb`): `pumpkin::plugin::BUILD_ID` (commit, rustc, target, `Cargo.lock` hash, features, debug assertions) exported by `export_plugin!` and `#[plugin_impl]`; the loader refuses a different or missing id with one line naming both. Unit tests fail without the check; boot tests: same-build plugins load, v1.2.0 plugins and a plugin from another commit are refused, musl still builds. Local CI green apart from the known and flaky failures. | `pr-drafts/feat-plugin-build-id.md` |
| 6.11 | `fix/teleport-confirm-id` | **merged into alpha `84e5d54f9` on 2026-10-26**, pushed `b6e9fceb0` (2 commits): vanilla's teleport bookkeeping (`awaitingTeleport`, `awaitingPositionFromClient`, `awaitingTeleportTime`), confirms of older ids ignored, the resend after 20 ticks, one `teleport` path for every `CPlayerPosition`; fixes a cancelled `PlayerMoveEvent` moving the player anyway. Bot test `bot:teleport_confirm_late_and_resend` fails on alpha; socket boot tests with 0, 100 and 600 ms delay match vanilla. Needs a real-client play test with latency and a recording. | `pr-drafts/fix-teleport-confirm-id.md` |
| 4.1 | `fix/offline-uuid-vanilla` | done, pushed `bc4461e09`: merges upstream #4172 (`offline_uuid` is vanilla's MD5 version 3 UUID, returns `Uuid`, all four callers updated, nil fallback removed), adds `legacy_offline_uuid` (the PEX 1.x SHA-256 UUID) for 4.2, and tests against values from the vanilla 26.3 server and its `UUIDUtil` (CJK and 4-byte UTF-8 names included) and from the PEX 1.2.0 release. Boot tests in offline and BungeeCord mode match vanilla's files. **Must not ship without 4.2**: on its own a 1.2 player loses their data, op and whitelist entries (shown in the boot test). Local CI green apart from the known failures. Needs a real-client check and a capture. | `pr-drafts/fix-offline-uuid-vanilla.md` |
| 0.2 | `fix/codegen-deterministic` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `62fbd4945` (3 commits): NBT literals written with sorted compound keys (`NbtCompound` is a `HashMap` with a random seed, so `item.rs`, `registry.rs` and `structure_template.rs` changed every run), the plugin SDK output now goes through rustfmt like the other generators, and `java-packets.wit` regenerated (stale since `ace306d39`, adds the `trail-particle-option` record to the public WIT). Three runs in a row give identical sha256 over all 190 generated files; all 8,398 literals match `pumpkin-nbt`'s own bytes apart from key order. Local CI green apart from the known failures. | `pr-drafts/fix-codegen-deterministic.md` |
| 6.10 | `fix/block-break-timing` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `42d078520` (3 commits): a finished dig must match the dig position and reach 0.7 progress like vanilla `handleBlockBreakAction`, early finishes become a delayed destroy and the client gets its block back, one shared `destroy_block`, mining fatigue `0.3^(amplifier+1)` plus `block_break_speed` and `submerged_mining_speed`. Unit tests and bot test `bot:survival_dig_timing` fail without the fix. Local CI green apart from the known failures (`/srv/pex/ci/fix-block-break-timing/20261025-023754/`). Needs a real-client play test with a recording before the PR. Rebase on 6.1 is trivial (both add the same `finish_digging`). | `pr-drafts/fix-block-break-timing.md` |
| 3.1 | `feat/config-version` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `8569d579d`: integer `config_version` (missing means 1), ordered migrations like vanilla `FileFixerUpper`, `pumpkin.toml.v<old>.bak` backups that never overwrite, edits with `toml_edit` so comments and unknown keys survive, and load messages now reach the log. Fixtures are the files v1.0.0, v1.1.0 and v1.2.0 actually wrote. Local CI green apart from the known failures (`/srv/pex/ci/feat-config-version/20261025-023248/`). | `pr-drafts/feat-config-version.md` |
| 1.1 | `test/mob-parity-table` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `a37d261e9`: `docs/MOB-PARITY.md` (90 rows, all `not audited`), mock players and `assert_entity_not_present_in_box` in the GameTest helper, `pumpkin:cow_follows_wheat_and_panics_when_hit` (fails without `TemptGoal` 11 of 11 runs, without the panic goal 5 of 5). Local CI green except the known scaffolding GameTest (`/srv/pex/ci/test-mob-parity-table/20261024-221123/`). | `pr-drafts/test-mob-parity-table.md` |
| 5.1 | `test/redstone-dual-run` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `bf917f65f`: datapack `assets/tests/datapacks/pumpkin-dual-run/` with 5 block-based redstone tests built and saved by vanilla, `tools/dual-run/` (runner and structure builder), START test blocks now stay powered like vanilla `TestBlockEntity.trigger`, and `pumpkin-tester` runs every embedded datapack test. All 5 pass on vanilla and PEX; each deliberately broken build fails only its test. Local CI green apart from the known failures (`/srv/pex/ci/test-redstone-dual-run/20261024-225651/`). | `pr-drafts/test-redstone-dual-run.md` |
| 6.1 | `test/survival-harness` | **merged into alpha `d30c773bb` on 2026-10-25**, pushed `c3266dbda`: stage harness in `pumpkin-tester` (client-packet bot with its own inventory model, digging, pickups, menu clicks) and stage 1, wood to stone pickaxe; 10 of 10 runs pass; fails without the crafting table recipe. Local CI green apart from the known scaffolding GameTest and the doc-test job, which fails for any tester-only change (`rust.yml:184`, see below). | `pr-drafts/test-survival-harness.md` |
| 10.1 | none (no commits) | done: cause is outside the repository, see Q10.5 | `pr-drafts/ci-restore-actions.md` |

## Found while working

Sessions add things here that are outside their piece: bugs, missing features, wrong statements in this document. One line each, with the piece ID that found it and a file or commit reference.

- (10.1) Goal 10 originally said the fork's Actions are enabled; only the repository setting is. Corrected above.
- (10.1) `.github/workflows/nightly.yml:166` `make_latest: true` would make the next nightly the Latest release and break the panel plugin's download if it ever follows Latest. See 10.7.
- (6.11) Roadmap 6.11 said vanilla resends teleports from `tick`; it's from `updateAwaitingTeleport`, called by `handleMovePlayer` and `handleMoveVehicle`.
- (6.11) `.github/scripts/ci_packages.py` doesn't map `native-plugins/PORTING_GUIDE.md`, so a doc edit there runs CI on all 31 packages.
- (6.11) `/srv/pex/tools/walkbot.py` doesn't set `TCP_NODELAY` like the vanilla client, so under added latency Nagle delays its confirms and moves.
- (6.11) Upstream Pumpkin-MC/Pumpkin#3466 (open) fixes the same kick with a queue of 32 pending ids, which isn't vanilla; 6.11 follows vanilla.
- (0.4) `crates/pumpkin-world/src/level.rs:669` `get_or_fetch_chunk` inserts into `loaded_chunks` after `fetch_chunk` drops its ticket; an unload pass in between could leave a chunk in the map the scheduler no longer tracks (read, not reproduced).
- (0.4) `bot:survival_dig_timing` failed once in 50 alpha runs ("A dig finished right after it started broke the obsidian") and never in 55 runs with 0.4's scheduler fix; possibly the same race.
- (0.4) `pumpkin-tester` has no switch to run only the bot tests, so looping them needs a local patch.
- (4.2) References inside player data (a shoulder parrot's `Owner`, ender pearl owners) aren't migrated by 4.2; 4.3's UUID walk must include `world/players/data/*.dat`, not only region and entity files.
- (4.2) `LoadJSONConfiguration::load` (`data/mod.rs`) falls back to an empty list when ops, whitelist or bans fail to parse, and the next save overwrites the admin's file. 4.2 made the error visible but didn't change the fallback.
- (4.2) Entity-selector arguments reject non-ASCII player names (`give 玩家一号 torch 21` gives "Invalid name or UUID"), while login, `op` and `whitelist add` accept them. Seen on 1.2.0 and alpha.
- (2.1) The v1.2.0 release plugins export `PUMPKIN_API_VERSION` 2 while alpha is 3, so alpha already refused them; the build id is what catches plugins from other commits with the same API version.
- (2.1) A plugin built outside the workspace that depends on `pumpkin-core` by path or git gets the server's id (`crates/pumpkin-core/build.rs` hashes the server's `Cargo.lock`); documented in `PORTING_GUIDE.md` hard rule 1.
- (2.1) Nothing in the workspace expands `#[plugin_impl]` (`crates/pumpkin-api-macros/src/lib.rs`), so CI never compiles its output.
- (2.1) `crates/pumpkin-core/build.rs` reruns on `../../.git/HEAD`, which doesn't exist in a git worktree (`.git` is a file there), so the embedded commit can go stale in worktree builds.
- (4.1) `crates/pumpkin-protocol/src/java/server/login/login_start.rs:45`: the `SLoginStart` fallback uses `Uuid::new_v3(&Uuid::nil(), "OfflinePlayer:"+name)`, which also hashes the 16 namespace bytes (Notch gives `0a6bfc7c-...`, vanilla `b50ad385-...`). Only reached with `online_mode = true` and `encryption = false` (`net/java/login/login_start.rs:66`).
- (4.1) `crates/pumpkin-core/src/net/mod.rs:446` `is_valid_player_name` accepts non-ASCII names and counts 16 UTF-8 bytes; vanilla `StringUtil.isValidPlayerName` rejects characters at or below 32 and at or above 127 and counts UTF-16 units, so such PEX players can't join vanilla (matters for 4.5).
- (4.1) `/srv/pex/gameplay-bench/whitelist.py` and `/srv/pex/tools/whitelist-pumpkin-bots.json` generate SHA-256 UUIDs for PEX; switch them to version 3 once 4.1 and 4.2 merge.
- (4.1) Editing `local-ci.sh` in place while a run was executing it crashed that run at its last line; script edits now go through `/srv/pex/tools/edit-script.sh` (copy, patch, rename).
- (0.2) `tools/pumpkin-codegen/src/sdk/mod.rs:19` and `wit/v0_2/mod.rs:67` write files even when unchanged, so every codegen run rebuilds the plugin API and v0.2 host crates.
- (0.2) `format_code` in `tools/pumpkin-codegen/src/main.rs` silently falls back to unformatted output when rustfmt is missing; `wit/v0_2/mod.rs:95` duplicates it.
- (0.2) `tools/pumpkin-codegen/src/bedrock_creative.rs:109-115` reads its own previous output back in when `assets/bedrock/creative_items.nbt` is missing, instead of failing.
- (0.2) The WIT generator turns every pub struct in protocol packet files into a public WIT record, including helpers like `TrailParticleOption` (question for the maintainer: wanted for the mirrored public WIT?).
- (6.10) PEX never sends the level events 2019 and 2020 (`PARTICLES_DESTROY_PROGRESS`, `PARTICLES_AND_SOUND_DESTROY_PROGRESS`) that vanilla `ServerPlayerGameMode.tick` sends while a block is being dug, and ignores `ChangeDestroyDirection` (`net/java/play/player_action.rs:225`). Other players may not see or hear digging; needs a real-client check.
- (6.10) Aqua Affinity has no effect: vanilla applies its `submerged_mining_speed` modifier server side and syncs it; PEX only syncs Efficiency by hand (`entity/player.rs` `sync_mining_efficiency`).
- (6.10) `Entity::is_submerged_in_water` (`entity/mod.rs:2697`) compares the eyes with `getHeight`; 26.3 `EntityFluidInteraction` uses `getHeightForCamera`, so water under a solid block counts as full.
- (6.10) The Bedrock break path in `Player::tick` (`entity/player.rs:3000`) records the block state id as the mined statistic; every other path uses the block id.
- (6.10) `CancelledDigging` from out of reach is ignored (`net/java/play/player_action.rs:141`); vanilla still aborts the dig, so the crack can stay visible to others.
- (6.10) Not ported from `handleBlockBreakAction`: the block update for positions above max y, aborts from out of reach, the extra crack reset on a mismatched abort, a start on air.
- (6.10) A full disk at about 23:25 on 2026-10-24 made a `git checkout` truncate a source file in one worktree; restored from git, `git fsck` clean.
- (3.1) Upstream Pumpkin-MC/Pumpkin#3420 (open, agent-opened) makes the same config load failures visible with `eprintln!`; it overlaps 3.1's `LoadMessage` change.
- (3.1) `unshare -n` works in CT 117, so a release binary can boot on its default ports without clashing with other sessions.
- (5.1) `crates/pumpkin-core/src/data/datapack/mod.rs` `scan_datapacks_dir` (around line 1281) only loads world datapacks that `level.dat` lists as enabled; vanilla `MinecraftServer.configurePackRepository` (around line 1593) enables new world packs automatically.
- (5.1) `crates/pumpkin-core/src/block/blocks/test_block.rs:76` returns `emits_redstone_power` true; vanilla `TestBlock` doesn't override `isSignalSource`, so wire connects to test blocks only in PEX.
- (5.1) `crates/pumpkin-gametest/src/runner/mod.rs` `finish_failure` reports a failure at the tick it's detected; vanilla `GameTestInfo.fail` stores it and reports it a tick later, and the timeout text differs from vanilla's `test.error.timeout.no_result`. Pass and fail results agree.
- (5.1) `assets/tests/datapacks/pumpkin-unit-test*/pack.mcmeta` declare pack format 107; vanilla 26.3's data format is 121.
- (5.1) Roadmap said `/test runall`; 26.3's `TestCommand` has no `runall`. Corrected.
- (6.1) `.github/workflows/rust.yml:184` runs `cargo test --doc` on every affected package, so a change that only touches `pumpkin-tester` (no library target) fails CI with "no library targets found". `local-ci.sh` now skips packages without a library; the workflow needs the same fix (10.7).
- (6.1) `crates/pumpkin-protocol/src/codec/lp_vector_3d.rs`: `LpVector3d::read` reads 2 bytes for a zero vector where vanilla `LpVec3` writes 1 (used by `SInteract::read` and `CSpawnEntity::read_packet_data`). `update_entity_pos.rs` and `update_entity_pos_rot.rs` readers still use the pre-26.3 layout without the var int; the writers are right. Matters for the in-repo bot, which reads with these.
- (6.1) `crates/pumpkin-inventory/src/screen_handler.rs:998` repeats a shift-click while `are_items_and_components_equal`; vanilla `AbstractContainerMenu.doClick` uses `ItemStack.isSameItem`.
- (6.1) `cargo run -p pumpkin-codegen` on alpha 2ce43ef3d rewrites six generated files unrelated to the input (`item.rs`, `registry.rs`, `structure_template.rs`, plugin-api `block.rs` and `item.rs`, `java-packets.wit`), so generated files on alpha are stale or the generator isn't deterministic.
- (6.1) The bot's dig timing and position-after-teleport follow the vanilla client from memory, since client classes aren't in the server jar.
- (1.1) The repository copied into the CT had a repo-local git identity (endercreepern64) that overrode Neon Swan; removed on 2026-10-24, and the affected commits were re-authored.
- (1.1) `tools/pumpkin-tester/src/bot_tests/commands_and_gamemode.rs` is flaky on a busy host ("Bot `/setblock` did not place emerald_block"): 3 of 27 runs on 1.1's branch and 1 of 7 runs of alpha's binary. When it fails, the tester stops before any GameTest runs.
- (1.1) Any player in the world makes non-persistent mobs more than 128 blocks away despawn (`crates/pumpkin-core/src/entity/mob/mod.rs:935`), which breaks command-summoned datapack tests such as `creeper_should_run_from_cat` while a player is present. 1.1 runs mock-player tests in their own batch for that reason.
- (1.1) Vanilla `LivingEntity.knockback` throws a hit mob about two blocks, so "moved away after a hit" doesn't prove panic in a test.
- (baseline) `crates/pumpkin-core/src/block/entities/creaking_heart.rs:655`: the creaking heart end-to-end test is flaky (failed 2 of 3 runs in the CT, passed on pve).
- (baseline) `pumpkin-world` `nether_biome_hashes_match_vanilla` failed once under load with no panic message and passed alone.
- (baseline) Running `pumpkin-tester` from the repository root leaves an untracked `metrics/` directory (`slow-ticks.log`, `summary.csv`); it isn't in `.gitignore`. Run it from a scratch directory with `--manifest-path`.
- (baseline) `pumpkin:scaffolding_distance_and_collapse` fails in `pumpkin-tester` on alpha. Not investigated yet.
- (baseline) `crates/pumpkin-world/tests/world_info_save.rs:42` `write_world_info_reports_final_rename_failure` fails when tests run as root, because a `0o555` directory doesn't stop root. Skip it under root or force the rename failure another way.

## Appendix A: mob parity starting table

Generated on 2026-10-24 from `assets/entities.json`, `entity/type.rs` and the mob source files on alpha, and from `EntityTypes.java` and the entity class chains in the decompiled 26.3 server. Piece 1.1 turns this into `docs/MOB-PARITY.md`.

- **Vanilla 26.3 AI**: "Brain" when the mob's own class or a parent below `Mob` defines `brainProvider` or `makeBrain`, or a `<Class>Ai` class exists. The goal number counts `addGoal` call sites in the class chain below `PathfinderMob`, so it's an upper bound (subclasses that override a parent's goal helper are counted twice), not the exact number of registered goals.
- **PEX AI**: "goals (n)" counts `add_goal` calls in the mob's own file; family files that register shared goals are named. "tick only" means no goals or Brain, with behaviour in the tick.
- **Unit tests** counts `#[test]` functions in the mob's own file. **Audited reach** means `has_audited_tick` returns true.

| Mob | Vanilla 26.3 AI | PEX file | PEX AI | Unit tests | Audited reach | Notes |
| :-- | :-- | :-- | :-- | --: | :-- | :-- |
| allay | Brain | `entity/passive/allay.rs` | Brain | 3 |  | only PEX mob on the Brain framework |
| armadillo | Brain | `entity/passive/armadillo.rs` | goals (8) |  |  |  |
| axolotl | Brain | `entity/passive/axolotl.rs` | goals (19) |  |  |  |
| bat | goals (0) | `entity/mob/bat.rs` | tick only |  |  | `custom_server_ai_step`, as vanilla |
| bee | goals (17) | `entity/passive/bee.rs` | goals (11) |  |  |  |
| blaze | goals (7) | `entity/mob/blaze.rs` | goals (6) |  |  |  |
| bogged | goals (12) | `entity/mob/skeleton/bogged.rs` | goals (skeleton/mod.rs, 13) |  |  |  |
| breeze | Brain | `entity/mob/breeze.rs` | goals (5) |  |  |  |
| camel | Brain + goals (10) | `entity/passive/camel.rs` | goals (8) |  |  |  |
| camel_husk | Brain + goals (10) | - | none |  |  | not in `from_type()`: spawns as a plain `LivingEntity` with no AI |
| cat | goals (16) | `entity/passive/cat.rs` | goals (13) |  |  |  |
| cave_spider | goals (10) | `entity/mob/cave_spider.rs` | goals (8) |  |  |  |
| chicken | goals (8) | `entity/passive/chicken.rs` | goals (8) |  | yes |  |
| cod | goals (4) | `entity/passive/cod.rs` | goals (1 + abstract_fish.rs) |  |  |  |
| copper_golem | Brain | `entity/passive/copper_golem.rs` | goals (4) |  |  |  |
| cow | goals (8) | `entity/passive/cow.rs` | goals (8) |  | yes |  |
| creaking | Brain | `entity/mob/creaking.rs` | goals (6) |  |  |  |
| creeper | goals (10) | `entity/mob/creeper.rs` | goals (10) |  |  |  |
| dolphin | goals (15) | `entity/passive/dolphin.rs` | goals (7) |  |  |  |
| donkey | goals (10) | `entity/passive/donkey.rs` | goals (8) |  |  |  |
| drowned | goals (25) | `entity/mob/zombie/drowned.rs` | goals (12 + zombie/mod.rs) |  |  |  |
| elder_guardian | goals (7) | `entity/mob/elder_guardian.rs` | goals (5) |  |  |  |
| ender_dragon | goals (0) | `entity/boss/ender_dragon.rs` | tick only |  |  | phase machine (11 phases); fireball is an instant cloud, no `dragon_fireball` entity |
| enderman | goals (12) | `entity/mob/enderman.rs` | goals (12) |  |  |  |
| endermite | goals (8) | `entity/mob/endermite.rs` | goals (7) |  |  |  |
| evoker | goals (18) | `entity/mob/evoker.rs` | goals (15) |  |  |  |
| fox | goals (28) | `entity/passive/fox/mod.rs` | goals (25) |  |  |  |
| frog | Brain | `entity/passive/frog.rs` | goals (5) |  |  |  |
| ghast | goals (4) | `entity/mob/ghast.rs` | goals (4) |  |  |  |
| giant | goals (0) | `entity/mob/giant.rs` | goals (5) |  |  |  |
| glow_squid | goals (2) | `entity/passive/glow_squid.rs` | tick only |  |  | shares the squid tick |
| goat | Brain | `entity/passive/goat.rs` | goals (8) | 1 |  |  |
| guardian | goals (7) | `entity/mob/guardian.rs` | goals (8) |  |  |  |
| happy_ghast | Brain + goals (3) | `entity/passive/happy_ghast.rs` | goals (5) |  |  |  |
| hoglin | Brain | `entity/mob/hoglin.rs` | goals (7) | 2 |  |  |
| horse | goals (10) | `entity/passive/horse.rs` | goals (8) |  |  |  |
| husk | goals (13) | `entity/mob/zombie/husk.rs` | goals (zombie/mod.rs, 12) |  |  |  |
| illusioner | goals (17) | `entity/mob/illusioner.rs` | goals (13) |  |  |  |
| iron_golem | goals (12) | `entity/passive/iron_golem.rs` | goals (8) |  |  |  |
| llama | goals (23) | `entity/passive/llama.rs` | goals (11) |  |  |  |
| magma_cube | goals (6) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `cube_mob.rs` |
| mooshroom | goals (8) | `entity/passive/mooshroom.rs` | goals (8) |  |  |  |
| mule | goals (10) | `entity/passive/mule.rs` | goals (7) |  |  |  |
| nautilus | Brain | `entity/passive/nautilus.rs` | tick only |  |  | no goals and no Brain |
| ocelot | goals (10) | `entity/passive/ocelot.rs` | goals (10) |  |  |  |
| panda | goals (16) | `entity/passive/panda.rs` | goals (8) |  |  |  |
| parched | goals (12) | `entity/mob/skeleton/parched.rs` | goals (skeleton/mod.rs, 13) |  |  |  |
| parrot | goals (8) | `entity/passive/parrot.rs` | goals (5) | 3 |  |  |
| phantom | goals (4) | `entity/mob/phantom.rs` | goals (6) |  |  |  |
| pig | goals (9) | `entity/passive/pig.rs` | goals (9) |  | yes |  |
| piglin | Brain | `entity/mob/piglin.rs` | Brain |  |  | own `piglin_ai.rs`, not the Brain framework |
| piglin_brute | Brain | `entity/mob/piglin_brute.rs` | goals (10) |  |  |  |
| pillager | goals (15) | `entity/mob/pillager.rs` | goals (14) |  |  |  |
| polar_bear | goals (12) | `entity/passive/polar_bear.rs` | goals (9) |  |  |  |
| pufferfish | goals (4) | `entity/passive/pufferfish.rs` | goals (1 + abstract_fish.rs) |  |  |  |
| rabbit | goals (15) | `entity/passive/rabbit.rs` | goals (14) |  |  |  |
| ravager | goals (13) | `entity/mob/ravager.rs` | goals (13) |  |  |  |
| salmon | goals (4) | `entity/passive/salmon.rs` | goals (abstract_fish.rs, 3) |  |  |  |
| sheep | goals (9) | `entity/passive/sheep.rs` | goals (9) |  | yes |  |
| shulker | goals (7) | `entity/mob/shulker.rs` | goals (6) |  |  |  |
| silverfish | goals (7) | `entity/mob/silverfish.rs` | goals (7) |  |  |  |
| skeleton | goals (12) | `entity/mob/skeleton/skeleton.rs` | goals (skeleton/mod.rs, 13) |  |  |  |
| skeleton_horse | goals (11) | `entity/passive/skeleton_horse.rs` | goals (4) |  |  |  |
| slime | goals (6) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `cube_mob.rs` shared with magma cube and sulfur cube |
| sniffer | Brain | `entity/passive/sniffer.rs` | goals (7) |  |  |  |
| snow_golem | goals (5) | `entity/passive/snow_golem.rs` | goals (5) |  |  |  |
| spider | goals (10) | `entity/mob/spider.rs` | goals (8) |  |  |  |
| squid | goals (2) | `entity/passive/squid.rs` | tick only |  |  | vanilla's two goals folded into `mob_tick` |
| stray | goals (12) | `entity/mob/skeleton/stray.rs` | goals (skeleton/mod.rs, 13) |  |  |  |
| strider | goals (9) | `entity/passive/strider.rs` | goals (9) |  |  |  |
| sulfur_cube | goals (5) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `cube_mob.rs` |
| tadpole | Brain + goals (3) | `entity/passive/tadpole.rs` | goals (7) |  |  |  |
| trader_llama | goals (27) | `entity/passive/trader_llama.rs` | goals (12) |  |  |  |
| tropical_fish | goals (4) | `entity/passive/tropical_fish.rs` | goals (abstract_fish.rs, 3) |  |  |  |
| turtle | goals (9) | `entity/passive/turtle.rs` | goals (7) |  |  |  |
| vex | goals (8) | `entity/mob/vex.rs` | goals (6) |  |  |  |
| villager | Brain | `entity/passive/villager/mod.rs` | goals (17) | 8 |  |  |
| vindicator | goals (18) | `entity/mob/vindicator.rs` | goals (15) |  |  |  |
| wandering_trader | goals (18) | `entity/passive/wandering_trader.rs` | goals (20) | 4 |  |  |
| warden | Brain | `entity/mob/warden.rs` | goals (7) | 1 |  |  |
| witch | goals (12) | `entity/mob/witch.rs` | goals (10) |  |  |  |
| wither | goals (7) | `entity/boss/wither.rs` | goals (3) |  |  |  |
| wither_skeleton | goals (13) | `entity/mob/skeleton/wither.rs` | goals (skeleton/mod.rs, 13) |  |  |  |
| wolf | goals (20) | `entity/passive/wolf.rs` | goals (20) |  |  |  |
| zoglin | Brain | `entity/mob/zoglin.rs` | goals (7) |  |  |  |
| zombie | goals (13) | `entity/mob/zombie/zombie.rs` | goals (zombie/mod.rs, 12) |  |  |  |
| zombie_horse | goals (12) | `entity/passive/zombie_horse.rs` | goals (6) |  |  |  |
| zombie_nautilus | Brain | - | none |  |  | not in `from_type()`: spawns as a plain `LivingEntity` with no AI |
| zombie_villager | goals (13) | `entity/mob/zombie/zombie_villager.rs` | goals (zombie/mod.rs, 12) | 1 |  |  |
| zombified_piglin | goals (19) | `entity/mob/zombified_piglin.rs` | goals (6) |  |  |  |
