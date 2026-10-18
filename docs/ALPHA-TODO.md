# alpha: work status (2026-10-16)

## World generation (done on alpha)
- `sequence` features, `volume_match` predicates, template placement (fluid fill and shape updates for
  speleothems and water), desert well now data-driven (old `DesertWellFeature` removed).
- Template feature still places only the first palette (all current templates have one).
- Shape updates after template placement only cover blocks the current templates use (speleothems,
  water, waterlogged blocks). Other blocks keep their state; a general hook into the server's
  `get_state_for_neighbor_update` would need block behaviours that can read a generation cache.
- Worldgen features that place block entities (`creaking_heart`, `sculk_sensor`, `sculk_shrieker`,
  `sculk_catalyst`) now register pending block entities on `ProtoChunk` so they instantiate when
  chunks load.
- Verified with unit tests (`template::tests`):
  - `sulfur_spring_places_tuff_patch_and_spring_template_with_spikes_and_water`
  - `desert_well_volume_match_rejects_air_pockets_and_places_on_solid_sand`

## Sculk / vibrations (done)
- Game events reach sculk sensors, calibrated sensors, shriekers, wardens, and allays
  (`world/vibration.rs`, a port of `VibrationSystem`, `VibrationSelector`, occlusion by
  `#minecraft:dampens_vibrations` blocks).
- Emitted vanilla game events: step/swim (players and mobs), hit_ground, block_place, block_destroy,
  eat/drink, projectile_shoot/projectile_land, container_open/container_close, block_open/block_close,
  entity_place, equip, splash, flap, entity_damage, entity_die, explode, fluid_place/fluid_pickup,
  note_block_play, jukebox_play/jukebox_stop_play, shear, instrument_play, elytra_glide.
- Entity listeners dispatched in `World::game_event`:
  - `WardenEntity`: listens to `#minecraft:warden_can_listen` (radius 16), tracks anger via
    `AngerManagement` (+35 anger or +10 for repeated projectiles within 20t), plays tendril
    shivering sound/status, investigates disturbances, applies darkness aura, and saves/loads
    `anger` and `listener` NBT.
  - `AllayEntity`: listens to `#minecraft:allay_can_listen` (radius 16), binds to `NoteBlockPlay`
    for 600 ticks, dances near `JukeboxPlay`, and saves/loads `listener` NBT.
- Vibration listener state (`VibrationData`) is persisted to/from the vanilla `listener` NBT compound
  on `SculkSensorBlockEntity`, `CalibratedSculkSensorBlockEntity`, `SculkShriekerBlockEntity`,
  `WardenEntity`, and `AllayEntity` (re-emitting in-flight vibration particles on reload).
- `dampens_vibrations` implemented for `ItemEntity` carrying `#minecraft:dampens_vibrations` items
  and `WardenEntity`; `SculkShriekerBlock::try_get_player` resolves direct players, passengers,
  projectile owners, and item throwers (`Thrower` NBT).
- End-to-end Ancient City verification test (`sculk_shrieker::tests::ancient_city_sensor_and_shrieker_four_warnings_summon_warden_and_entities_listen`):
  verifies natural ancient city generation, sensor-to-shrieker propagation through 4 warnings to
  warden summoning, warden projectile anger rules, and allay note-block listening.

## Creaking (done)
- Ported `CreakingHeartBlockEntity.serverTick` (`block/entities/creaking_heart.rs` and
  `block/blocks/creaking_heart.rs`):
  - Comparator analog output signal based on protector distance.
  - Particle trail emission and sound playback on `creaking_hurt`, plus BFS resin clump spreading
    across adjacent `#minecraft:pale_oak_logs`.
  - Automatic state updates (`Uprooted`, `Dormant`, `Awake`), night-time protector spawning within
    32 blocks of survival/adventure players, dawn/distance/stuck-player despawning, and 20..=24 XP
    drop when a natural heart is broken without Silk Touch.
- Verified with end-to-end test (`creaking_heart::tests::creaking_heart_spawns_protector_at_night_spreads_resin_on_hurt_and_despawns_at_dawn`).

## Other (done)
- `net/java/play/player_position.rs`: Fixed `handle_position_rotation` jump check to use
  `(packet.collision & FLAG_ON_GROUND) == 0`, matching `handle_position` and vanilla.
