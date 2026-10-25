# PEX native plugins

Plugins written in Rust against the PEX server itself. They're built with the server, so every PEX build comes with a matching set.

| Plugin | What it does | Commands |
| :-- | :-- | :-- |
| WorldPumpkin | World editing: selections, fill with block patterns, replace, walls, move, undo and redo. A native port of [WorldPumpkin](https://github.com/NicDevTV/WorldPumpkin) by NicDevTV. | `//pos1` `//pos2` `//hpos1` `//hpos2` `//chunk` `//expand` `//set` `//replace` `//walls` `//move` `//undo` `//redo` `/wp` |
| PEXGuard | Region protection: build, interact, chest access, PvP, mob damage and spawning, explosions, fire, liquid flow, pistons, entry, greetings. | `/rg` (`/region`) |
| PEXProtect | Block logging with inspect, lookup, rollback and restore. | `/co` (`/coreprotect`) |

## Installing

1. Download the server and `pumpkin-plugins-<arch>-<os>.zip` **from the same release**.
2. Unzip the plugins you want into the server's `plugins/` folder (create it if it doesn't exist).
3. Start the server. Each plugin writes its config to `plugins/data/<plugin>/`.

To build them yourself, `cargo build --release` builds the server and the plugins together. The plugin files are in `target/release/`: `worldpumpkin.dll`, `pexguard.dll` and `pexprotect.dll` on Windows, or `libworldpumpkin.so` and so on on Linux and macOS.

**Always use plugin files from the same build as the server.** Native plugins aren't portable between server versions the way Java plugins are: the server refuses to load a plugin file from another build and logs "Plugin build mismatch" (or "Plugin has no build id" for plugins from 1.x releases). Update the plugins every time you update the server.

The static `musl` Linux builds can't load native plugins. Use the regular Linux build if you want plugins.

## Writing a plugin

See `PORTING_GUIDE.md`. It's written for AI coding agents but works as a reference for people too. Start from `template/`.
