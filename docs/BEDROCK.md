# Bedrock Edition

PEX lets Bedrock clients join next to Java ones. For 2.0, Bedrock support is experimental: the current client joins, plays the basics and can't crash the server or grow its memory without bound, and everything else is listed below as a known gap (roadmap decision D8-1).

The marks in this page were read from the code on `alpha` and checked with the Bedrock test bot where a check exists. None of them has been checked with a real Bedrock client yet.

## Version and transport

- Bedrock 1.26.60, protocol 2225 (`CURRENT_BEDROCK_MC_VERSION` and `CURRENT_BEDROCK_MC_PROTOCOL` in `crates/pumpkin-world/src/lib.rs`). A client on any other protocol is refused with `OutdatedClient` or `OutdatedServer`. Hotfix clients that keep protocol 2225 can join.
- New Bedrock releases come in by cherry-picking upstream Pumpkin's Bedrock commits between full upstream merges (D8-2).
- Transport is NetherNet (WebRTC) only. RakNet is gone, its server-list ping included.
- TCP 19132 carries plain HTTP signaling: `/v1/join` to connect and `/v1/ping` for the server list. UDP 19132 carries LAN discovery and the WebRTC traffic. Both come from `networking.bedrock.address`.

## Running a server for Bedrock

Bedrock is on by default. The settings are under `[networking.bedrock]` in the config:

| Key | Default | What it does |
| --- | --- | --- |
| `enabled` | `true` | Turns Bedrock off entirely: no ports, no OIDC key fetch, no skin downloads for Bedrock viewers. |
| `online_mode` | `true` | Checks the Xbox token against Microsoft's OIDC keys. With `false`, any name and XUID is accepted. |
| `max_players` | `1000` | Bedrock logins past this are refused. |
| `address` | `0.0.0.0:19132` | TCP signaling and UDP discovery and WebRTC, on the same port. |
| `username_prefix`, `replace_username_spaces` | | How Bedrock gamertags become player names. |

Open TCP 19132 and UDP 19132. `docker-compose.yml` and the `Dockerfile` already do, and the Pelican eggs map the Bedrock address to an allocation.

Bans, IP bans, the whitelist, duplicate names and UUIDs, and the player limit apply to Bedrock logins as they do to Java ones.

## What works

The Bedrock test bot (below) checks all of these except the HUD line, which is read from the code.

- Joining by IP, reaching spawn, receiving chunks, leaving. Guest and split-screen logins are refused.
- Seeing Java players and being seen by them; movement, sneaking, sprinting, crawling and swimming both ways.
- Chat both ways when `allow_chat_reports = false` (the default), and running commands.
- Breaking and placing blocks in creative and survival, with the right drops; using items on blocks.
- Eating, drinking potions and milk, throwing snowballs, potions and ender pearls, bows, crossbows, tridents, spyglass, goat horn, shield, fishing rod, empty maps, spawn eggs, fish buckets, eyes of ender.
- Containers through item stack requests, including crafting.
- Combat with Java's attack rules and cooldown.
- Riding: boats, minecarts, horses, pigs and striders with their sticks, camels, happy ghasts, leads, rider jumps.
- Nether and end portals, dimension changes, respawning, the end credits.
- HUD: bossbars, titles, scoreboards, time of day, health and hunger, XP, mob effects, attributes, entity metadata.
- Robustness: an out-of-range hotbar slot is refused instead of panicking, NetherNet packets and the chunk blob cache are bounded, closed sessions are released, a malformed packet disconnects only that client, unknown packets are logged once per id.

## Known gaps

The rows below are every function in `pumpkin-core` that sends a packet only Java clients receive. The generated list of call sites is in [BEDROCK-COVERAGE.md](BEDROCK-COVERAGE.md#java-only-sends), and `tools/bedrock-coverage.py --check docs/BEDROCK.md` prints any function missing from this page, or named here but no longer sending Java-only. It prints nothing when they agree.

Marks: **covered** means a Bedrock player gets the same thing (from the same function or the path named). **Partial** means some of it. **Missing** means a Bedrock player gets nothing for it. **Java only** means Bedrock has no counterpart, or gets it another way by design.

### Joining and the world

| Function | Mark | Notes |
| --- | --- | --- |
| `world/mod.rs::send_java_login` | Java only | Bedrock joins through `spawn_bedrock_player` (`StartGame`). |
| `world/mod.rs::spawn_java_player` | covered | Tells Bedrock viewers about the new Java player (`PlayerList`, `AddPlayer`). The recipe book unlock in it is Java only. |
| `world/mod.rs::spawn_bedrock_player` | covered | The Java side of a Bedrock join: the `CSpawnEntity` Java viewers need. |
| `world/mod.rs::send_world_info` | Java only | Bedrock gets this in `StartGame`. |
| `world/mod.rs::respawn_player` | covered | Bedrock gets `ChangeDimension` and `SetSpawnPosition` in the same function. |
| `entity/player.rs::unload_watched_chunks` | Java only | Bedrock drops chunks outside the radius from `NetworkChunkPublisherUpdate`. |
| `command/commands/fillbiome.rs::execute` | missing | Bedrock players keep the old biomes until the chunk is sent again. |
| `server/tick_rate_manager.rs::update_state_to_clients` | Java only | No Bedrock packet for the tick rate. |
| `server/tick_rate_manager.rs::update_step_ticks` | Java only | |
| `server/tick_rate_manager.rs::update_joining_player` | Java only | |
| `server/tick_rate_manager.rs::finish_tick_sprint` | missing | The "sprint finished" chat line doesn't reach Bedrock. |
| `world/border.rs::set_center` | missing | Bedrock has no world border packet; the border is enforced on the server only. |
| `world/border.rs::set_diameter` | missing | |
| `world/border.rs::set_warning_delay` | missing | |
| `world/border.rs::set_warning_distance` | missing | |
| `world/border.rs::reset` | missing | |
| `world/weather.rs::set_weather_parameters` | missing | Bedrock players never see rain or thunder (`LevelEvent` start/stop rain isn't sent). |
| `world/weather.rs::tick_weather` | missing | |

### Chat, text and commands

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/player.rs::send_message` | missing | `/msg`, `/say`, `/me` and `/teammsg` lines don't reach Bedrock players. |
| `command/commands/title.rs::execute` | partial | Titles, subtitles and the action bar reach Bedrock through `Player::show_title`; `title clear` and `title reset` don't. |
| `command/client_suggestions.rs::send_c_commands_packet` | covered | Bedrock gets `AvailableCommands` from the same file. |
| `command/commands/dialog.rs::execute` | Java only | Dialogs. Bedrock's counterpart is forms, a post-2.0 group. |
| `entity/player.rs::set_server_links` | Java only | |
| `server/mod.rs::broadcast_server_links` | Java only | |
| `entity/player.rs::send_custom_payload` | Java only | Plugin channels. |

Chat from a Bedrock player is dropped when `allow_chat_reports = true`, and advancement announcements reach Bedrock as raw `%key` text. Both are G8-06.

### Player list

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/player.rs::set_display_name` | missing | Bedrock gets a player's name once, at join. No caller in `pumpkin-core`; plugins can reach it. |
| `entity/player.rs::set_tab_list_name` | missing | |
| `entity/player.rs::set_tab_list_order` | missing | |
| `entity/player.rs::set_tab_list_latency` | missing | |
| `entity/player.rs::set_tab_list_listed` | missing | |
| `entity/player.rs::set_tab_list_header_footer` | Java only | Bedrock's player list has no header or footer. |
| `server/mod.rs::broadcast_tab_list_header_footer` | Java only | |
| `entity/player.rs::set_gamemode` | covered | Bedrock gets `SetPlayerGameType` in the same function. |

### Sounds

Bedrock players hear only the sounds their own client plays and the few level sounds sent through `World::play_bedrock_level_sound` (5 calls). There is no Java-to-Bedrock sound mapping yet.

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/player.rs::play_sound` | missing | |
| `entity/player.rs::play_sound_event` | missing | |
| `entity/player.rs::stop_sound` | missing | |
| `world/mod.rs::play_sound_event` | missing | The helper most block and entity sounds go through. |
| `world/mod.rs::play_sound_event_expect` | missing | |
| `world/mod.rs::play_custom_sound` | missing | |
| `command/commands/playsound.rs::play_sound` | missing | |
| `entity/projectile/arrow.rs::on_hit` | missing | |
| `entity/projectile/trident.rs::on_hit` | missing | |
| `item/items/bow.rs::shoot` | missing | |

### Particles and world events

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/player.rs::spawn_particles` | missing | No Java-to-Bedrock particle mapping yet. |
| `world/mod.rs::spawn_particles` | missing | |
| `world/mod.rs::spawn_particles_with_data` | missing | |
| `command/commands/particle.rs::send_particles` | missing | |
| `entity/passive/allay/vibration.rs::send_vibration_particle` | missing | |
| `world/mod.rs::sync_world_event` | missing | Door, dispenser, fire and similar effects. Bedrock gets `LevelEvent` only for block cracking and breaking. |
| `world/mod.rs::sync_global_world_event` | missing | |
| `entity/ai/goal/blaze_attack.rs::shoot_fireball` | missing | |
| `entity/projectile/splash_potion.rs::on_hit` | missing | |
| `entity/projectile/lingering_potion.rs::broadcast_impact` | missing | |
| `world/mod.rs::run_explosion` | missing | Bedrock sees the blocks go but gets no explosion effect or knockback. |

### Player state and HUD

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/player.rs::set_experience` | covered | Bedrock gets XP as attributes from `Player::tick_experience`. |
| `entity/player.rs::send_effect` | partial | `LivingEntity::add_effect` sends Bedrock `MobEffect`; this direct path (the warden's darkness, `Player::send_active_effects`) doesn't. |
| `entity/player.rs::remove_effect` | partial | `LivingEntity` removal reaches Bedrock through `World::send_remove_mob_effect`; this direct path doesn't. |
| `entity/player.rs::remove_all_effects` | partial | |
| `entity/player.rs::start_cooldown` | missing | The cooldown applies on the server, but the Bedrock client shows no cooldown overlay. |
| `entity/player.rs::set_compass_target` | missing | No caller in `pumpkin-core`. |
| `entity/player.rs::send_game_event` | missing | Its one caller is the pufferfish sting sound. |
| `entity/mob/elder_guardian.rs::custom_server_ai_step` | missing | The elder guardian curse. |
| `entity/player.rs::enqueue_property_packet` | missing | Furnace, brewing stand and similar progress bars (`ContainerSetData` isn't sent). |
| `entity/player.rs::add_post_effect` | Java only | Post-processing shaders. |
| `entity/player.rs::remove_post_effect` | Java only | |
| `entity/player.rs::clear_post_effects` | Java only | |
| `entity/player.rs::send_post_effects` | Java only | |
| `entity/player/advancement.rs::flush_dirty` | Java only | Bedrock has no advancement screen. |
| `entity/player/advancement.rs::set_selected_tab` | Java only | |
| `command/commands/recipe.rs::execute` | Java only | Bedrock gets every crafting recipe in `CraftingData`. |
| `item/items/knowledge_book.rs::normal_use_with_hand` | Java only | |
| `command/commands/waypoint.rs::execute` | missing | Not mapped to Bedrock's locator bar. |

### Camera

| Function | Mark | Notes |
| --- | --- | --- |
| `command/commands/spectate.rs::execute` | missing | A Bedrock spectator can't take another entity's view. |
| `entity/player.rs::set_camera_entity_id` | missing | |
| `entity/player.rs::reset_camera` | missing | |
| `entity/player.rs::tick` | missing | Resets the camera when the spectated entity is gone. |

### Maps and books

| Function | Mark | Notes |
| --- | --- | --- |
| `block/entities/map.rs::broadcast_map_data` | missing | A Bedrock player can fill a map but sees it blank. |
| `entity/player.rs::tick_maps` | missing | |
| `entity/player.rs::open_book` | missing | |

### Entities

| Function | Mark | Notes |
| --- | --- | --- |
| `entity/mod.rs::teleport` | partial | Bedrock catches up through the entity tracker's next move (`MoveActorDelta`), not at once. |
| `entity/player.rs::teleport` | partial | |
| `entity/mob/shulker.rs::teleport_somewhere` | partial | |
| `entity/projectile/arrow.rs::tick` | partial | Bedrock gets projectile motion from the entity tracker (`SetActorMotion`). |
| `entity/projectile/trident.rs::tick` | partial | |
| `entity/projectile/mod.rs::process_tick` | partial | |
| `entity/mod.rs::add_passenger` | covered | `SetActorLink` in the same function. |
| `entity/mod.rs::remove_passenger_internal` | covered | |
| `entity/mod.rs::remove_passenger_on_disconnect` | covered | |
| `entity/mod.rs::leash_to` | covered | |
| `entity/mod.rs::unleash` | covered | |
| `entity/mob/metadata.rs::send_to_viewers` | covered | `SetActorData` in the same function. |
| `entity/attributes.rs::send_attribute_updates_for_living` | covered | `UpdateAttributes` in the same function. |
| `entity/mob/creaking.rs::pre_damage` | missing | The creaking's hurt animation. |
| `entity/player.rs::wake_up` | missing | The leave-bed animation. |
| `world/entity_tracker.rs::send_to_tracking_players_and_self` | partial | A Java-side helper; the tracker sends Bedrock its own packets elsewhere. |
| `server/mod.rs::broadcast_packet_all` | partial | Forwards to every world's Java-only helper; callers decide about Bedrock. |

### Blocks

| Function | Mark | Notes |
| --- | --- | --- |
| `world/mod.rs::flush_block_updates` | covered | `UpdateBlock` and `BlockActorData` in the same function. |
| `world/mod.rs::add_block_entity` | covered | |
| `world/mod.rs::update_block_entity` | covered | |
| `world/mod.rs::set_block_breaking` | covered | `LevelEvent` cracking in the same function. |
| `world/mod.rs::set_block_destroy_stage` | missing | Other players' cracking from this path. |
| `entity/player.rs::send_block_change` | missing | Resyncing one block after a refused action; a Bedrock client can keep a ghost block. |
| `entity/player.rs::reset_block_change` | missing | |

### Scoreboards

| Function | Mark | Notes |
| --- | --- | --- |
| `world/scoreboard.rs::send_je` | covered | The Java half; the Bedrock half is in the same file (`SetScore`, `SetDisplayObjective`, `RemoveObjective`). |
| `world/scoreboard.rs::send_to_player` | covered | |

### Packets from the client that PEX ignores

`BedrockClient::handle_play_packet` handles 31 of the 91 packets a client can send; the rest are logged once per id per client as `Bedrock client <address> sent unknown packet <id>` and dropped. The full list is in [BEDROCK-COVERAGE.md](BEDROCK-COVERAGE.md).

Which of those a 1.26.60 client actually sends in normal play needs a capture from a real client (at least 15 minutes, collecting those log lines), which hasn't been done yet. From the code and gophertunnel, the ones most likely to matter are `MapInfoRequest` (68), `BookEdit` (97), `LecternUpdate` (125), `SubChunkRequest` (175), `ServerSettingsRequest` (102), `AnvilDamage` (141), `SetPlayerFurnaceOptions` (351), `ServerboundStonecutterSetRecipe` (354), `PlayerToggleCrafterSlotRequest` (306), `PlayerSkin` (93) and `UpdateClientOptions` (323).

## Tests

- Unit tests: `cargo test -p pumpkin-protocol bedrock` and `cargo test -p pumpkin-core bedrock`.
- The Bedrock test bot in `tools/pumpkin-tester/src/bedrock/` is a NetherNet client written in Rust that joins a test server with Bedrock on and `online_mode = false`, next to a Java bot. Its scenarios are join and spawn, bad packets, login checks, mixed play (seeing each other, chat, movement, blocks, riding, combat), dimensions and held items. Run only the Bedrock part with:

  ```
  cargo run --locked -p pumpkin-tester -- --bedrock-only
  ```

  The full `cargo run -p pumpkin-tester` runs it after the GameTests. Set `PEX_TESTER_STALL_EXIT_SECS` to end a run that stops making progress.
- Wire check: `PUMPKIN_TESTER_BEDROCK_DUMP=<dir>` makes the bot write every packet it receives; `go run ./tools/bedrock-wirecheck <dir>/*.hex` decodes them with gophertunnel and fails on any that don't decode cleanly.
- Coverage: `tools/bedrock-coverage.py --gophertunnel <gophertunnel checkout> --out docs/BEDROCK-COVERAGE.md` regenerates the packet table and the Java-only call sites; `tools/bedrock-coverage.py --check docs/BEDROCK.md` checks this page against the call sites.
