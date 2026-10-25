# AGENTS.md

This folder holds PEX's native plugins. Each plugin is a Rust `cdylib`, built as a member of the main Cargo workspace and loaded by the server from `plugins/`. The repository root `AGENTS.md` still applies.

**Before writing or changing any plugin, read `PORTING_GUIDE.md` in full.** It has the rules, templates, API tables, verified pitfalls and the testing procedure. The short version:

1. Plugins must stay workspace members (listed in `../Cargo.toml`). That's what guarantees they're built from the same source, lockfile and compiler as the server; the server refuses plugins whose build id (commit, compiler, target, lockfile hash, features) differs, but a plugin built outside the workspace can pass that check and then corrupt memory. Never run `cargo update`.
2. Plugin commits don't change `../crates/`. Report missing APIs and events instead.
3. New plugins start as a copy of `template/`. Use `pex_common::export_plugin!`, never `#[plugin_impl]`.
4. Call `context.init_log()` first in `on_load`. Enter `server.runtime` at the top of every handler and executor that calls server code. Don't use Tokio APIs in plugin code.
5. Handlers that cancel or modify events must be registered with `blocking = true`. Check that an event is actually fired before relying on it (guide §3.4, Appendix A).
6. No `unwrap`/`expect`/`panic!`/`println!` outside tests. PEX's clippy `pedantic` and `nursery` lints apply.
7. From the repository root: `cargo build -p <plugin>`, `cargo clippy --no-deps -p <plugin> --all-targets -- -D warnings`, `cargo test -p <plugin>`, `cargo fmt --check`.
8. Boot-test with `tools/server_driver.py` and bot-test anything player-facing with `tools/testbot` (guide §8) before calling anything done. End with the report described in guide §9.

| Folder | What it is |
| :-- | :-- |
| `common/` | Shared helpers: plugin exports, config loading, chat prefixes |
| `template/` | Minimal working plugin to copy (not shipped in releases) |
| `worldpumpkin/` | WorldPumpkin, native port (MIT, credit NicDevTV). Selections, fill, replace, walls, move, undo/redo |
| `pexguard/` | PEXGuard: WorldGuard-style regions and flags (`/rg`) |
| `pexprotect/` | PEXProtect: CoreProtect-style block logging, inspect, lookup, rollback and restore (`/co`), SQLite |
| `tools/server_driver.py` | Runs a test server and forwards lines appended to a file to its console |
| `tools/testbot/` | Scripted Azalea bots for in-game tests (nightly Rust, not a workspace member; see its README) |
