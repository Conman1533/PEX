# Mob parity

Goal 1 of the 2.0 roadmap (`docs/ROADMAP-2.0.md` on the `roadmap-2.0` branch) is that every mob in 26.3 has its vanilla AI. This table tracks it, one row per entity type with `"mob": true` in `assets/entities.json` (90 rows).

## Columns

The first six columns come from the roadmap's Appendix A, generated on 2026-10-24 from `alpha` at `2ce43ef3d`:

- **Vanilla 26.3 AI**: "Brain" when the mob's own class or a parent below `Mob` defines `brainProvider` or `makeBrain`, or a `<Class>Ai` class exists. The goal number counts `addGoal` call sites in the class chain below `PathfinderMob`, so it is an upper bound, not the exact number of registered goals.
- **PEX AI**: "goals (n)" counts `add_goal` calls in the mob's own file, and names the family file when shared goals live there. "tick only" means no goals or Brain, with the behaviour in the tick.
- **Unit tests** counts `#[test]` functions in the mob's own file. **Audited reach** means `has_audited_tick` returns true.

The other columns are filled by the audit:

- **Vanilla compared**: the classes and methods read in the decompiled 26.3 server (`/Vault-Mk4/vanilla-26.3/src` on pve), for example `AbstractCow.registerGoals`, `Animal.aiStep`, `TemptGoal`, the `*Ai` class for a Brain mob, and any navigation or move control override.
- **Commit audited**: the PEX commit the comparison was made against. A later change to the mob or to a goal it uses means checking again.
- **Status**: `not audited` until someone compares the mob with vanilla. `deviations` when it was audited and the differences are listed in Notes but not fixed or accepted yet. `parity` when it was audited at the named commit and every remaining difference is one the maintainer accepted, listed in Notes.
- **Test**: the GameTest id registered in `crates/pumpkin-core/src/server/builtin_game_tests/`, or the `bot:` test in `tools/pumpkin-tester` when a real client connection is needed.

Every row starts at `not audited`. Upstream pull requests merged into `alpha` on 2026-10-23 changed some of these mobs, and some of them say they match the vanilla goal list. That is their claim, not an audit here, so those rows are noted but still `not audited`. The mob AI foundation rework (Pumpkin-MC/Pumpkin#4092, merged the same day) changed the goal selector and navigation for every mob.

## Table

| Mob | Vanilla 26.3 AI | PEX file | PEX AI | Unit tests | Audited reach | Vanilla compared | Commit audited | Status | Test | Notes |
| :-- | :-- | :-- | :-- | --: | :-- | :-- | :-- | :-- | :-- | :-- |
| allay | Brain | `entity/passive/allay.rs` | Brain | 3 |  |  |  | not audited |  | only PEX mob on the Brain framework. Bot test `bot:allay_hand_transfer_and_interaction_order` covers item hand-over, not movement. |
| armadillo | Brain | `entity/passive/armadillo.rs` | goals (8) |  |  |  |  | not audited |  |  |
| axolotl | Brain | `entity/passive/axolotl.rs` | goals (19) |  |  |  |  | not audited |  |  |
| bat | goals (0) | `entity/mob/bat.rs` | tick only |  |  |  |  | not audited |  | `custom_server_ai_step`, as vanilla. |
| bee | goals (17) | `entity/passive/bee.rs` | goals (11) |  |  |  |  | not audited |  |  |
| blaze | goals (7) | `entity/mob/blaze.rs` | goals (6) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4113 (follow range read from `FOLLOW_RANGE`) and Pumpkin-MC/Pumpkin#3790 (rises toward higher targets), merged 2026-10-23. |
| bogged | goals (12) | `entity/mob/skeleton/bogged.rs` | goals (skeleton/mod.rs, 13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3988 (poison arrows, shearing, skeleton projectile and sound fixes), merged 2026-10-23. |
| breeze | Brain | `entity/mob/breeze.rs` | goals (5) |  |  |  |  | not audited |  |  |
| camel | Brain + goals (10) | `entity/passive/camel.rs` | goals (8) |  |  |  |  | not audited |  |  |
| camel_husk | Brain + goals (10) | - | none |  |  |  |  | not audited |  | not in `from_type()`: spawns as a plain `LivingEntity` with no AI. |
| cat | goals (16) | `entity/passive/cat.rs` | goals (13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3984 rebuilt the goal list against vanilla 26.2 `registerGoals` (not 26.3), merged 2026-10-23. |
| cave_spider | goals (10) | `entity/mob/cave_spider.rs` | goals (8) |  |  |  |  | not audited |  |  |
| chicken | goals (8) | `entity/passive/chicken.rs` | goals (8) |  | yes |  |  | not audited |  |  |
| cod | goals (4) | `entity/passive/cod.rs` | goals (1 + abstract_fish.rs) |  |  |  |  | not audited |  |  |
| copper_golem | Brain | `entity/passive/copper_golem.rs` | goals (4) |  |  |  |  | not audited |  |  |
| cow | goals (8) | `entity/passive/cow.rs` | goals (8) |  | yes |  |  | not audited | `pumpkin:cow_follows_wheat_and_panics_when_hit` | Goal list matches `AbstractCow.registerGoals` (checked for the 1.1 test, not an audit). |
| creaking | Brain | `entity/mob/creaking.rs` | goals (6) |  |  |  |  | not audited |  |  |
| creeper | goals (10) | `entity/mob/creeper.rs` | goals (10) |  |  |  |  | not audited | `pumpkin:creeper_should_run_from_cat` (datapack) | Pumpkin-MC/Pumpkin#4086 (goals hold a weak self reference, no behaviour change), merged 2026-10-23. |
| dolphin | goals (15) | `entity/passive/dolphin.rs` | goals (7) |  |  |  |  | not audited |  |  |
| donkey | goals (10) | `entity/passive/donkey.rs` | goals (8) |  |  |  |  | not audited |  |  |
| drowned | goals (25) | `entity/mob/zombie/drowned.rs` | goals (12 + zombie/mod.rs) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4039 ported the drowned goals, target goals and swimming, merged 2026-10-23. |
| elder_guardian | goals (7) | `entity/mob/elder_guardian.rs` | goals (5) |  |  |  |  | not audited |  |  |
| ender_dragon | goals (0) | `entity/boss/ender_dragon.rs` | tick only |  |  |  |  | not audited |  | phase machine (11 phases); fireball is an instant cloud, no `dragon_fireball` entity. |
| enderman | goals (12) | `entity/mob/enderman.rs` | goals (12) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3815 (aggression cleared when the target dies) and Pumpkin-MC/Pumpkin#4086 (weak self reference), merged 2026-10-23. |
| endermite | goals (8) | `entity/mob/endermite.rs` | goals (7) |  |  |  |  | not audited |  |  |
| evoker | goals (18) | `entity/mob/evoker.rs` | goals (13) |  |  | `Evoker` (all methods), `SpellcasterIllager` (`SpellcasterCastingSpellGoal`, `SpellcasterUseSpellGoal`, `customServerAiStep`), `AbstractIllager.canAttack`, `Raider.registerGoals`, `PatrollingMonster.registerGoals`, the `EvokerFangs` constructor | `151caf820` | deviations | `pumpkin:evoker_summons_vexes_at_survival_player` | G1-14 (`fix/mobai-spellcasters`): vanilla goal list and priorities (the shared raider goals, look at players 3 blocks always at 9, no random look around); hurt-by target ignoring raiders and alerting others, no baby villager targets, players remembered 300 ticks unseen, villagers and wandering traders without line of sight, iron golems without. Spells follow `SpellcasterUseSpellGoal`: warmup in goal ticks, a live target, `mobGriefing`, line of sight and a random blue sheep for the wololo spell. Fang rings only within 3 blocks (was 9), fangs land on the first sturdy block below like `createSpellEntity`, fang yaw in degrees. Summoned vexes are finalized. The casting goal stops the navigation and faces the target or the wololo sheep. Left: `LookAtPlayerGoal(Mob.class)`, `considersEntityAsAlly` for its vexes, and vexes don't join its scoreboard team. |
| fox | goals (28) | `entity/passive/fox/mod.rs` | goals (25) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4093 ported the fox goals from 26.3, merged 2026-10-23. |
| frog | Brain | `entity/passive/frog.rs` | goals (5) |  |  |  |  | not audited |  |  |
| ghast | goals (4) | `entity/mob/ghast.rs` | goals (4) |  |  |  |  | not audited |  |  |
| giant | goals (0) | `entity/mob/giant.rs` | goals (5) |  |  |  |  | not audited |  |  |
| glow_squid | goals (2) | `entity/passive/glow_squid.rs` | tick only |  |  |  |  | not audited |  | shares the squid tick. |
| goat | Brain | `entity/passive/goat.rs` | goals (8) | 1 |  |  |  | not audited |  |  |
| guardian | goals (7) | `entity/mob/guardian.rs` | goals (8) |  |  |  |  | not audited |  |  |
| happy_ghast | Brain + goals (3) | `entity/passive/happy_ghast.rs` | goals (5) |  |  |  |  | not audited |  |  |
| hoglin | Brain | `entity/mob/hoglin.rs` | goals (7) | 2 |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4095 ported `HoglinBase.throwTarget`, merged 2026-10-23. |
| horse | goals (10) | `entity/passive/horse.rs` | goals (8) |  |  |  |  | not audited |  |  |
| husk | goals (13) | `entity/mob/zombie/husk.rs` | goals (zombie/mod.rs, 12) |  |  |  |  | not audited | `pumpkin:zombie_pathfinds_to_villager` | Pumpkin-MC/Pumpkin#4039 removed `FloatGoal` from the zombie family, merged 2026-10-23. |
| illusioner | goals (17) | `entity/mob/illusioner.rs` | goals (12) |  |  | `Illusioner` (all methods), `SpellcasterIllager` (`SpellcasterCastingSpellGoal`, `SpellcasterUseSpellGoal`, `customServerAiStep`), `RangedBowAttackGoal`, `AbstractIllager.canAttack`, `Raider.registerGoals`, `PatrollingMonster.registerGoals` | `151caf820` | deviations | `pumpkin:illusioner_shoots_bow_at_survival_player` | G1-14 (`fix/mobai-spellcasters`): vanilla goal list and priorities (Creaking avoid 8 blocks 1.0/1.2 at 3, no hold ground goal or random look around, look at players 3 blocks always at 9, the shared raider goals); targets as the evoker, with iron golems also remembered 300 ticks; celebrate sound `ILLUSIONER_AMBIENT`; bow shots play `SKELETON_SHOOT`; the casting goal turns at the mob's head limits. The mirror and blindness spells already matched vanilla. Left: `LookAtPlayerGoal(Mob.class)`; the illusion copies are client-side. |
| iron_golem | goals (12) | `entity/passive/iron_golem.rs` | goals (8) |  |  |  |  | not audited |  | Spawn pattern GameTests only: `pumpkin:iron_golem_pattern_spawns`, `pumpkin:iron_golem_pattern_clears_all_four_blocks`. |
| llama | goals (23) | `entity/passive/llama.rs` | goals (11) |  |  |  |  | not audited |  |  |
| magma_cube | goals (6) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `MagmaCube`, `Slime` (`push`, `playerTouch`, `dealDamage`, `jumpFromGround`, `jumpInLiquid`, `finalizeSpawn`), `Slime.SlimeMoveControl`, `SlimeAttackGoal` | `d6c28a434` | deviations | `pumpkin:magma_cube_targets_iron_golem` | `cube_mob.rs`. Pumpkin-MC/Pumpkin#3795 (cube size sync) and Pumpkin-MC/Pumpkin#4086 (weak self reference), merged 2026-10-23. G1-11 (`fix/mobai-cube-mobs`): attack damage is size plus 2 as vanilla `getAttackDamage`, contact damage to iron golems on push, jump boost of 0.1 per size from the ground and in lava, spawn size uses the regional special multiplier, squish decays by 0.9. Left: the friendly-fire team check in `dealDamage`. |
| mooshroom | goals (8) | `entity/passive/mooshroom.rs` | goals (8) |  |  |  |  | not audited |  |  |
| mule | goals (10) | `entity/passive/mule.rs` | goals (7) |  |  |  |  | not audited |  |  |
| nautilus | Brain | `entity/passive/nautilus.rs` | tick only |  |  |  |  | not audited |  | no goals and no Brain. |
| ocelot | goals (10) | `entity/passive/ocelot.rs` | goals (10) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3984 rebuilt the goal list against vanilla 26.2 `registerGoals` (not 26.3), merged 2026-10-23. |
| panda | goals (16) | `entity/passive/panda.rs` | goals (8) |  |  |  |  | not audited |  |  |
| parched | goals (12) | `entity/mob/skeleton/parched.rs` | goals (skeleton/mod.rs, 13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3988 (skeleton projectile and sound fixes), merged 2026-10-23. |
| parrot | goals (8) | `entity/passive/parrot.rs` | goals (5) | 3 |  |  |  | not audited |  |  |
| phantom | goals (4) | `entity/mob/phantom.rs` | goals (6) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3715 added circle and swoop AI, described as ported from vanilla 1.21.1, merged 2026-10-23. |
| pig | goals (9) | `entity/passive/pig.rs` | goals (9) |  | yes |  |  | not audited |  |  |
| piglin | Brain | `entity/mob/piglin.rs` | Brain |  |  |  |  | not audited |  | own `piglin_ai.rs`, not the Brain framework. Pumpkin-MC/Pumpkin#4086 (weak self reference), merged 2026-10-23. |
| piglin_brute | Brain | `entity/mob/piglin_brute.rs` | goals (10) |  |  |  |  | not audited |  |  |
| pillager | goals (15) | `entity/mob/pillager.rs` | goals (14) |  |  | `Pillager.registerGoals`, `AbstractIllager.canAttack`, `Raider.registerGoals`, `ObtainRaidLeaderBannerGoal`, `pickUpItem`, `PatrollingMonster.registerGoals`, `LongDistancePatrolGoal` | `d6c28a434` | deviations | `pumpkin:pillager_shoots_crossbow_at_survival_player` | G1-13 (`fix/mobai-raiders`): vanilla goal list and priorities (Creaking avoid at 1, stroll 0.6 at 8, look at players 15 blocks at 9, alerting hurt-by target ignoring raiders, villager target without line of sight), no baby villager targets, shared raider goals. Left: `LookAtPlayerGoal(Mob.class)` (no any-mob look goal in PEX), and the PEX approximations of `HoldGroundAttackGoal`, `RaiderMoveThroughVillageGoal` and `RaiderCelebration`; the banner pickup doesn't drop a helmet it replaces. |
| polar_bear | goals (12) | `entity/passive/polar_bear.rs` | goals (9) |  |  |  |  | not audited |  |  |
| pufferfish | goals (4) | `entity/passive/pufferfish.rs` | goals (1 + abstract_fish.rs) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4050 added puffing and stinging (`PufferfishPuffGoal`), merged 2026-10-23. |
| rabbit | goals (15) | `entity/passive/rabbit.rs` | goals (14) |  |  |  |  | not audited |  |  |
| ravager | goals (13) | `entity/mob/ravager.rs` | goals (13) |  |  | `Ravager` (`registerGoals`, constructor, `aiStep`, `isImmobile`), `Raider.registerGoals`, `ObtainRaidLeaderBannerGoal`, `pickUpItem`, `PatrollingMonster.registerGoals`, `LongDistancePatrolGoal` | `d6c28a434` | deviations | `pumpkin:ravager_breaks_leaves_in_its_path` | G1-13 (`fix/mobai-raiders`): vanilla goal list (no hold-ground goal), breaks leaves it runs into while `mobGriefing` is on and jumps otherwise, leaves cost nothing to path through, movement speed eases between 0.3 and 0.35. Left: the stun after a shield block, the roar, and `LookAtPlayerGoal(Mob.class)`. |
| salmon | goals (4) | `entity/passive/salmon.rs` | goals (abstract_fish.rs, 3) |  |  |  |  | not audited |  |  |
| sheep | goals (9) | `entity/passive/sheep.rs` | goals (9) |  | yes |  |  | not audited |  | Bot test `bot:shearing_tool_loot_and_item_events` covers shearing, not AI. |
| shulker | goals (7) | `entity/mob/shulker.rs` | goals (6) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4086 (weak self reference), merged 2026-10-23. |
| silverfish | goals (7) | `entity/mob/silverfish.rs` | goals (7) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3886 matched the goal set to `registerGoals`, merged 2026-10-23. |
| skeleton | goals (12) | `entity/mob/skeleton/skeleton.rs` | goals (skeleton/mod.rs, 13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3988 (skeleton projectile and sound fixes), merged 2026-10-23. |
| skeleton_horse | goals (11) | `entity/passive/skeleton_horse.rs` | goals (4) |  |  |  |  | not audited |  |  |
| slime | goals (6) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `Slime` (`registerGoals`, `push`, `playerTouch`, `dealDamage`, `isDealsDamage`, `finalizeSpawn`, `getSpawnSize`), `Slime.SlimeMoveControl`, `SlimeAttackGoal`, `SlimeRandomDirectionGoal`, `SlimeKeepOnJumpingGoal`, `SlimeFloatGoal` | `d6c28a434` | deviations | `pumpkin:slime_hops_toward_survival_player`, `pumpkin:slime_hurts_iron_golem_it_bumps_into` | `cube_mob.rs` shared with magma cube and sulfur cube. Pumpkin-MC/Pumpkin#3795 (cube size sync) and Pumpkin-MC/Pumpkin#4086 (weak self reference), merged 2026-10-23. G1-11 (`fix/mobai-cube-mobs`): contact damage needs line of sight and plays the attack sound, iron golems it bumps into take damage, spawn size uses the regional special multiplier, new slimes start at size 1 before `finalizeSpawn`. Left: the friendly-fire team check in `dealDamage`. |
| sniffer | Brain | `entity/passive/sniffer.rs` | goals (7) |  |  |  |  | not audited |  |  |
| snow_golem | goals (5) | `entity/passive/snow_golem.rs` | goals (5) |  |  |  |  | not audited |  | Spawn pattern GameTests only: `pumpkin:snow_golem_pattern_spawns`, `pumpkin:snow_golem_horizontal_pattern_spawns`. |
| spider | goals (10) | `entity/mob/spider.rs` | goals (8) |  |  |  |  | not audited |  |  |
| squid | goals (2) | `entity/passive/squid.rs` | tick only |  |  |  |  | not audited |  | vanilla's two goals folded into `mob_tick`. |
| stray | goals (12) | `entity/mob/skeleton/stray.rs` | goals (skeleton/mod.rs, 13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3988 (skeleton projectile and sound fixes), merged 2026-10-23. |
| strider | goals (9) | `entity/passive/strider.rs` | goals (9) |  |  |  |  | not audited |  |  |
| sulfur_cube | goals (5) | `entity/mob/cube_mob.rs` | goals (6) | 1 |  | `SulfurCube` (`registerGoals`, `pickUpItem`, `wantsToPickUp`, `aiStep`, `SulfurCubeSearchForItemsGoal`, `SulfurCubeTemptGoal`), `Slime` shared methods | `d6c28a434` | deviations | `pumpkin:sulfur_cube_picks_up_dropped_item` | `cube_mob.rs`. Pumpkin-MC/Pumpkin#3795 gave sulfur cubes their AI, merged 2026-10-23. G1-11 (`fix/mobai-cube-mobs`): its own goals (tempt by hopping, search for items), swallows a `#sulfur_cube_swallowable` item into its body slot with the pickup timer, stops moving while it holds one, never attacks. Left: babies and `#sulfur_cube_food`, the archetypes, fuse and explosion, shearing and bucketing. |
| tadpole | Brain + goals (3) | `entity/passive/tadpole.rs` | goals (7) |  |  |  |  | not audited |  |  |
| trader_llama | goals (27) | `entity/passive/trader_llama.rs` | goals (12) |  |  |  |  | not audited |  |  |
| tropical_fish | goals (4) | `entity/passive/tropical_fish.rs` | goals (abstract_fish.rs, 3) |  |  |  |  | not audited |  |  |
| turtle | goals (9) | `entity/passive/turtle.rs` | goals (7) |  |  |  |  | not audited |  |  |
| vex | goals (8) | `entity/mob/vex.rs` | goals (7) |  |  | `Vex` (all methods), `VexMoveControl`, `VexChargeAttackGoal`, `VexRandomMoveGoal`, `VexCopyOwnerTargetGoal` | `151caf820` | deviations | `pumpkin:vex_flies_through_wall_to_target` | G1-14 (`fix/mobai-spellcasters`): audited, no code change; goals, flight and no-clip already matched vanilla. A charge aims the vex's feet at the target's eyes, so it can stop just above a player's head without hitting, as in vanilla. Left: `LookAtPlayerGoal(Mob.class)`; `noPhysics` stays on between ticks (vanilla clears it after each tick); vanilla's `isAffectedByBlocks` keeps block effects such as powder snow on a vex, PEX turns them off with no physics. |
| villager | Brain | `entity/passive/villager/mod.rs` | goals (17) | 8 |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3774 (sleeping villagers stop walking), merged 2026-10-23. |
| vindicator | goals (18) | `entity/mob/vindicator.rs` | goals (15) |  |  | `Vindicator.registerGoals`, `VindicatorBreakDoorGoal`, `VindicatorJohnnyAttackGoal`, `customServerAiStep`, `setCustomName`, `AbstractIllager.RaiderOpenDoorGoal`, `canAttack`, `Raider.registerGoals`, `ObtainRaidLeaderBannerGoal`, `pickUpItem`, `PatrollingMonster.registerGoals`, `LongDistancePatrolGoal` | `d6c28a434` | deviations | `pumpkin:vindicator_named_johnny_attacks_any_mob` | G1-13 (`fix/mobai-raiders`): vanilla goal list and priorities, Johnny (name or `Johnny` NBT) targets any attackable mob, breaks wooden doors on Normal and Hard during a raid, opens doors and paths through them only where a raid is on, melee attack no longer follows unseen targets. Left: `LookAtPlayerGoal(Mob.class)` (no any-mob look goal in PEX), and the PEX approximations of `HoldGroundAttackGoal`, `RaiderMoveThroughVillageGoal` and `RaiderCelebration`; the banner pickup doesn't drop a helmet it replaces. |
| wandering_trader | goals (18) | `entity/passive/wandering_trader.rs` | goals (20) | 4 |  |  |  | not audited |  |  |
| warden | Brain | `entity/mob/warden.rs` | goals (7) | 1 |  |  |  | not audited |  |  |
| witch | goals (12) | `entity/mob/witch.rs` | goals (10) |  |  | `Witch` (all methods), `RangedAttackGoal`, `NearestHealableRaiderTargetGoal`, `NearestAttackableWitchTargetGoal`, `Raider.registerGoals`, `ObtainRaidLeaderBannerGoal`, `pickUpItem`, `PatrollingMonster.registerGoals`, `LongDistancePatrolGoal` | `d6c28a434` | deviations | `pumpkin:witch_heals_hurt_raider` | G1-13 (`fix/mobai-raiders`): Pumpkin-MC/Pumpkin#4393 by Tertosie21 (heal raiders, player target cooldown, drinking, hat particles, sounds) plus the shared raider goals. Left: what #4393 lists (`getDamageAfterMagicAbsorb`, natural witches joining raids, lightning conversion, swamp hut spawns). |
| wither | goals (7) | `entity/boss/wither.rs` | goals (3) |  |  |  |  | not audited |  | Spawn pattern GameTest only: `pumpkin:wither_pattern_spawns`. |
| wither_skeleton | goals (13) | `entity/mob/skeleton/wither.rs` | goals (skeleton/mod.rs, 13) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#3988 (skeleton projectile and sound fixes), merged 2026-10-23. |
| wolf | goals (20) | `entity/passive/wolf.rs` | goals (20) |  |  |  |  | not audited |  |  |
| zoglin | Brain | `entity/mob/zoglin.rs` | goals (7) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4095 ported `HoglinBase.throwTarget`, merged 2026-10-23. |
| zombie | goals (13) | `entity/mob/zombie/zombie.rs` | goals (zombie/mod.rs, 12) |  |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4039 removed `FloatGoal` from the zombie family, merged 2026-10-23. |
| zombie_horse | goals (12) | `entity/passive/zombie_horse.rs` | goals (6) |  |  |  |  | not audited |  |  |
| zombie_nautilus | Brain | - | none |  |  |  |  | not audited |  | not in `from_type()`: spawns as a plain `LivingEntity` with no AI. |
| zombie_villager | goals (13) | `entity/mob/zombie/zombie_villager.rs` | goals (zombie/mod.rs, 12) | 1 |  |  |  | not audited |  | Pumpkin-MC/Pumpkin#4039 removed `FloatGoal` from the zombie family, merged 2026-10-23. |
| zombified_piglin | goals (19) | `entity/mob/zombified_piglin.rs` | goals (6) |  |  |  |  | not audited |  |  |

## Writing a mob test

- Put function GameTests in `crates/pumpkin-core/src/server/builtin_game_tests/`, one file per family piece (the cow test is in `farm_animals.rs`), and register them in `mod.rs`.
- `GameTestHelper::spawn_mock_player_at` adds a creative mode player holding an item, the way vanilla `makeMockServerPlayerInLevel` does, and removes it when the attempt ends. Mobs tempt to it and look at it, but hostile mobs ignore it because it is in creative mode. A test that needs a survival target has to add a game mode argument first.
- A test that adds a mock player goes in `MOCK_PLAYER_TEST_IDS`, not `BUILTIN_TEST_IDS`. The tester runs each of these on its own, because a player anywhere in the world makes mobs without persistence despawn more than 128 blocks away, and a tempted or targeting mob walks to a player in the next test 16 blocks away.
- For "in this box after n ticks", use `start_sequence().then_execute_after(n, ...)` with `assert_entity_present_in_box`. `assert_entity_not_present_in_box` is the opposite. `assert_entities_present` counts entities of a type in the structure.
- Hit a mob as a player with `execute_command` and `/damage <mob> <amount> minecraft:player_attack by @p`. The hit knocks the mob about two blocks away, so check something knockback alone can't do.
- Before relying on the test, remove the behaviour it checks (the goal, for example), run `cargo run -p pumpkin-tester`, and make sure the test fails. The cow test was checked this way against `TemptGoal` and against its panic goal.
