# pex-testbot

Scripted Minecraft bots ([Azalea](https://github.com/azalea-rs/azalea)) for testing PEX plugins in-game. It's how WorldPumpkin, PEXGuard and PEXProtect were tested.

## Setup (once)

```bash
sh setup-azalea.sh   # clones and patches Azalea into azalea/ (see the script for why)
cargo build          # debug build; release builds crash the current nightly
```

The crate uses nightly Rust (`rust-toolchain.toml`) and is excluded from the plugin workspace.

## Running

1. Start the test server with `../server_driver.py`. Set `online_mode = false` in `pumpkin.toml` (both places).
2. Start the bots: `target/debug/pex-testbot 127.0.0.1:25565 <commands file> Alice Bob > bots.log`
3. Append lines to the commands file. The format is in `src/main.rs`'s header, for example `Bob mine 106 99 105`.
4. Read `bots.log`: `[Name] text` is chat the bot received, `[Name] > ...` is what it did.

## Lessons from using it (check these before blaming your plugin)

- **Use `127.0.0.1`, not `localhost`.** `localhost` resolves to IPv6 and PEX listens on IPv4.
- **Move the world spawn away from the test area** (`setworldspawn 400 100 400`). Vanilla spawn protection blocks non-op breaking within 16 blocks of spawn.
- **Use creative mode for break tests.** In survival, stone takes 7.5 s by hand and the bot's `mine` gives up after 5 s.
- **Wait about 3 s after `tp`** before acting. The server keeps using the old position until the client confirms the teleport, so reach checks fail ("tried to interact with block out of reach").
- **`op` grants plugin bypass permissions.** De-op a bot when testing a protection rule.
- **Harmless log noise:** Azalea fails to parse some PEX packets (`registry_holder` enchantment errors, `update_advancements`, `swing_animation`). This doesn't affect the tests.
- **A bot that dies and respawns can get kicked with "Wrong teleport id".** That's a PEX/Azalea teleport-confirmation mismatch unrelated to plugins; the bot reconnects on its own. Build a floor so bots don't spawn in water.
