# alpha: open work (2026-10-16)

## World generation (done on alpha)
- `sequence` features, `volume_match` predicates, template placement (fluid fill and shape updates for
  speleothems and water), desert well now data-driven (old `DesertWellFeature` removed).
- Template feature still places only the first palette (all current templates have one).
- Shape updates after template placement only cover blocks the current templates use (speleothems,
  water, waterlogged blocks). Other blocks keep their state; a general hook into the server's
  `get_state_for_neighbor_update` would need block behaviours that can read a generation cache.
- Not yet built or tested in a running world: check sulfur springs in sulfur caves and desert wells
  (no wells over air pockets).

## Sculk / vibrations (new, partly done)
Working and tested with bots (`tools/sculktest.sh`): game events reach sculk sensors, calibrated
sensors and shriekers (`world/vibration.rs`, a port of VibrationSystem, VibrationSelector, occlusion
by wool). Sensors activate with distance-based power, resonate amethyst and click; shriekers hear the
clicking and shriek for the player. Emitted so far: step/swim (players and mobs), hit_ground,
block_place, block_destroy, plus the events that existed before.

Still to do:
- Emit the other vanilla game events: eat/drink, projectile shoot/land, container and door
  open/close, item pickup/drop, equip, splash, flap, entity damage/die, explode, fluid place/pickup,
  note block, elytra glide.
- Entity listeners: the warden (anger and tracking by vibration) and the allay. Only block listeners
  are dispatched today.
- Save the vibration listener state to NBT (vanilla `listener` tag); in-flight vibrations are lost on
  unload.
- `dampensVibrations` for item entities carrying wool, and the projectile owner as the shrieker's
  player.
- Verify a natural ancient city end to end: four separate warnings, then a warden.

## Creaking (not started)
- The creaking heart block entity has no tick, so hearts never spawn their creaking at night or
  despawn it at dawn. Port `CreakingHeartBlockEntity.serverTick` (vanilla 26.3 decompiled in
  /root/pumpkin-fork/vanilla/decomp10).

## Other
- `net/java/play/player_position.rs`: `handle_position` and `handle_position_rotation` detect a jump
  with opposite on-ground conditions; one of them is wrong.
