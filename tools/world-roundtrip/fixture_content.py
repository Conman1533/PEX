"""What the Goal 7 fixture world holds; build_fixture.py places it.

Coordinates are inside the lab: blocks and entities stand at y 200 on a glass floor and the
lodestone is at 52, 200, 2.
"""

GAME_RULES = {
    "advance_time": "false", "advance_weather": "false", "spawn_mobs": "false", "spawn_monsters": "false",
    "spawn_patrols": "false", "spawn_phantoms": "false", "spawn_wandering_traders": "false",
    "spawn_wardens": "false", "mob_griefing": "false", "tnt_explodes": "false", "random_tick_speed": "0",
    "fire_spread_radius_around_player": "0", "spread_vines": "false", "mob_drops": "false",
    "keep_inventory": "true", "immediate_respawn": "true",
}


class Stack(str):
    """An item stack as SNBT; `argument` is the same stack as `give` and `item replace` take it."""

    def __new__(cls, item_id, count=1, **components):
        parts = [f'id:"minecraft:{item_id}"', f"count:{count}"]
        if components:
            parts.append("components:{" + ",".join(f'"minecraft:{k}":{v}' for k, v in components.items()) + "}")
        stack = super().__new__(cls, "{" + ",".join(parts) + "}")
        stack.item_id = item_id
        stack.argument = f"{item_id}[{','.join(f'{k}={v}' for k, v in components.items())}] {count}"
        return stack


item = Stack


def slots(*stacks):
    return "[" + ",".join(stack[:-1] + f",Slot:{i}b}}" for i, stack in enumerate(stacks) if stack) + "]"


# One block per block entity type, with contents (BlockEntityTypes in 26.3).
BLOCK_ENTITIES = [
    ("furnace", "furnace[lit=true]{Items:" + slots(item("iron_ore", 3), item("coal", 5), item("iron_ingot", 2))
     + ',cooking_time_spent:40,cooking_total_time:200,lit_time_remaining:500,lit_total_time:1600,'
       'RecipesUsed:{"minecraft:iron_ingot_from_smelting_iron_ore":2}}'),
    ("chest", "chest[facing=south]"),
    ("trapped_chest", "trapped_chest[facing=south]"),
    ("ender_chest", "ender_chest[facing=south]"),
    ("jukebox", "jukebox[has_record=true]{RecordItem:" + item("music_disc_cat") + ",ticks_since_song_started:100L}"),
    ("dispenser", "dispenser[facing=up]{Items:" + slots(item("arrow", 16), item("fire_charge", 2)) + "}"),
    ("dropper", "dropper[facing=up]{Items:" + slots(item("cobblestone", 64)) + "}"),
    ("sign", 'oak_sign[rotation=8]{front_text:{messages:["Fixture","front","",""],color:"blue",'
             'has_glowing_text:1b},back_text:{messages:["back","","",""],color:"red",has_glowing_text:0b},is_waxed:1b}'),
    ("hanging_sign", 'oak_hanging_sign{front_text:{messages:["Hanging","sign","",""],color:"black",'
                     'has_glowing_text:1b}}'),
    ("mob_spawner", 'spawner{SpawnData:{entity:{id:"minecraft:pig",CustomName:"Spawner pig"}},Delay:20s,'
                    "MinSpawnDelay:200s,MaxSpawnDelay:800s,SpawnCount:4s,SpawnRange:4s,RequiredPlayerRange:16s}"),
    ("creaking_heart", "creaking_heart"),
    # A piston block entity only exists while blocks move: build_fixture.py powers this
    # piston and runs one tick, so the save holds the stone and the head halfway.
    ("piston", "piston[facing=up]"),
    ("brewing_stand", "brewing_stand{Items:" + slots(item("potion", potion_contents='{potion:"minecraft:water"}'),
                                                   None, None, item("nether_wart"), item("blaze_powder", 5))
     + ",BrewTime:100,Fuel:10}"),
    ("enchanting_table", 'enchanting_table{CustomName:"Fixture table"}'),
    ("end_portal", "end_portal"),
    ("beacon", 'beacon{primary_effect:"minecraft:speed",secondary_effect:"minecraft:regeneration"}'),
    ("skull", 'player_head{profile:{name:"Fixture",id:[I;1,2,3,4]},custom_name:"Fixture head",'
              'note_block_sound:"minecraft:block.note_block.bell"}'),
    ("daylight_detector", "daylight_detector[inverted=true]"),
    ("hopper", "hopper{Items:" + slots(item("iron_ingot", 10)) + ",TransferCooldown:8}"),
    ("comparator", "comparator{OutputSignal:5}"),
    ("banner", 'white_banner{patterns:[{pattern:"minecraft:stripe_top",color:"red"},'
               '{pattern:"minecraft:creeper",color:"black"}],CustomName:"Fixture banner"}'),
    ("structure_block", 'structure_block[mode=save]{name:"fixture:box",mode:"SAVE",posX:1,posY:0,posZ:1,'
                        'sizeX:3,sizeY:3,sizeZ:3,author:"fixturebot",showboundingbox:1b}'),
    ("end_gateway", "end_gateway{Age:100L,ExactTeleport:1b,exit_portal:[I;41,200,7]}"),
    ("command_block", 'command_block{Command:"say fixture",auto:0b,TrackOutput:1b,CustomName:"Fixture command"}'),
    ("shulker_box", "purple_shulker_box{Items:" + slots(item("diamond", 2), item("emerald", 3))
     + ',CustomName:"Fixture box"}'),
    ("conduit", "conduit"),
    ("barrel", "barrel[facing=up]"),
    ("smoker", "smoker{Items:" + slots(item("beef", 4)) + ",cooking_time_spent:20,cooking_total_time:100}"),
    ("blast_furnace", "blast_furnace{Items:" + slots(item("raw_iron", 4)) + ",cooking_time_spent:20,cooking_total_time:100}"),
    ("lectern", "lectern[has_book=true]{Book:" + item("writable_book", writable_book_content='{pages:["lectern page"]}')
     + ",Page:0}"),
    ("bell", "bell[attachment=floor]"),
    ("jigsaw", 'jigsaw[orientation=north_up]{name:"fixture:start",target:"fixture:end",pool:"minecraft:empty",'
               'joint:"rollable",final_state:"minecraft:stone",placement_priority:1,selection_priority:2}'),
    ("campfire", "campfire[lit=true]{Items:" + slots(item("beef"), None, item("salmon"))
     + ",CookingTimes:[I;100,0,30,0],CookingTotalTimes:[I;600,600,600,600]}"),
    ("beehive", 'beehive[honey_level=3]{bees:[{entity_data:{id:"minecraft:bee",CustomName:"Hive bee"},'
                "ticks_in_hive:100,min_ticks_in_hive:2400}],flower_pos:[I;44,200,2]}"),
    ("sculk_sensor", "sculk_sensor{last_vibration_frequency:4}"),
    ("calibrated_sculk_sensor", "calibrated_sculk_sensor{last_vibration_frequency:7}"),
    ("sculk_catalyst", "sculk_catalyst"),
    ("sculk_shrieker", "sculk_shrieker{warning_level:2}"),
    ("chiseled_bookshelf", "chiseled_bookshelf[slot_0_occupied=true,slot_3_occupied=true]{Items:["
     + item("book")[:-1] + ",Slot:0b}," + item("enchanted_book", stored_enchantments='{"minecraft:mending":1}')[:-1]
     + ",Slot:3b}],last_interacted_slot:3}"),
    ("shelf", "oak_shelf{Items:" + slots(item("apple", 2), None, item("clock")) + ",align_items_to_bottom:1b}"),
    ("brushable_block", "suspicious_sand{item:" + item("diamond") + ",hit_direction:1}"),
    ("decorated_pot", 'decorated_pot{sherds:{back:"minecraft:brick",left:"minecraft:heart_pottery_sherd",'
                      'right:"minecraft:brick",front:"minecraft:skull_pottery_sherd"},item:' + item("emerald", 3) + "}"),
    ("crafter", "crafter{Items:" + slots(item("oak_planks"), None, None, None, item("stick", 2))
     + ",disabled_slots:[I;1,2],crafting_ticks_remaining:0,triggered:0}"),
    ("trial_spawner", "trial_spawner{required_player_range:14,target_cooldown_length:36000}"),
    ("vault", "vault{config:{key_item:" + item("trial_key") + "}}"),
    ("test_block", 'test_block[mode=log]{mode:"log",message:"fixture"}'),
    ("test_instance_block", "test_instance_block"),
    ("copper_golem_statue", "copper_golem_statue"),
    ("potent_sulfur", "potent_sulfur{countdown:100}"),
]
NEEDS_SUPPORT_ABOVE = {"hanging_sign"}

# Item stacks with many components, put one per slot into the chest and trapped chest.
SHOWCASE = [
    item("diamond_sword", custom_name='{text:"Fixture sword",color:"gold",italic:0b}',
         lore='["first line",{text:"second",color:"red"}]', enchantments='{"minecraft:sharpness":5,"minecraft:unbreaking":3}',
         damage="10", repair_cost="3", rarity='"epic"',
         attribute_modifiers='[{type:"minecraft:attack_damage",id:"fixture:sword",amount:4.0d,operation:"add_value",'
                             'slot:"mainhand"}]',
         custom_data="{fixture:1b,nested:{a:[1,2,3]}}", tooltip_display='{hidden_components:["minecraft:repair_cost"]}'),
    item("leather_chestplate", dyed_color="16711680", trim='{material:"minecraft:gold",pattern:"minecraft:coast"}',
         unbreakable="{}", enchantment_glint_override="1b"),
    item("firework_rocket", 3, fireworks='{flight_duration:2b,explosions:[{shape:"star",colors:[I;16711680],'
                                         'fade_colors:[I;255],has_trail:1b,has_twinkle:1b}]}'),
    item("firework_star", firework_explosion='{shape:"burst",colors:[I;65280]}'),
    item("potion", potion_contents='{potion:"minecraft:strong_healing",custom_color:16711935,'
                                   'custom_effects:[{id:"minecraft:speed",amplifier:1b,duration:600}]}'),
    item("written_book", written_book_content='{title:"Fixture",author:"fixturebot",generation:1,'
                                              'pages:["page one",{text:"page two",bold:1b}]}'),
    item("writable_book", writable_book_content='{pages:["draft one","draft two"]}'),
    item("shield", base_color='"blue"', banner_patterns='[{pattern:"minecraft:cross",color:"red"}]'),
    item("shulker_box", container='[{slot:0,item:' + item("diamond", 2) + "},{slot:5,item:"
         + item("stick", custom_name='"inner stick"') + "}]"),
    item("bundle", bundle_contents="[" + item("apple", 3) + "," + item("arrow", 8) + "]"),
    item("crossbow", charged_projectiles="[" + item("arrow") + "]"),
    item("player_head", profile='{name:"Fixture",id:[I;1,2,3,4]}'),
    item("suspicious_stew", suspicious_stew_effects='[{id:"minecraft:night_vision",duration:100}]'),
    item("goat_horn", instrument='"minecraft:ponder_goat_horn"'),
    item("compass", lodestone_tracker='{target:{pos:[I;52,200,2],dimension:"minecraft:overworld"},tracked:1b}'),
    item("beehive", bees='[{entity_data:{id:"minecraft:bee"},ticks_in_hive:10,min_ticks_in_hive:600}]',
         block_state='{honey_level:"3"}'),
    item("knowledge_book", recipes='["minecraft:crafting_table"]'),
    item("enchanted_book", stored_enchantments='{"minecraft:mending":1}'),
    item("apple", 5, food="{nutrition:10,saturation:2.0f,can_always_eat:1b}", consumable="{consume_seconds:0.5f}",
         max_stack_size="16", custom_model_data="{floats:[1.5f]}", item_model='"minecraft:golden_apple"',
         use_cooldown="{seconds:2.0f}"),
    item("stick", tool="{rules:[],default_mining_speed:2.0f}", weapon="{}", max_stack_size="1", max_damage="100",
         damage="1", use_remainder=item("bowl")),
    item("decorated_pot", pot_decorations='{back:"minecraft:heart_pottery_sherd",'
                                          'front:"minecraft:skull_pottery_sherd"}',
         container="[{slot:0,item:" + item("emerald") + "}]"),
    item("chest", container_loot='{loot_table:"minecraft:chests/simple_dungeon",seed:42L}'),
    item("pig_spawn_egg", entity_data='{id:"minecraft:pig",CustomName:"egg pig"}'),
    item("axolotl_bucket", bucket_entity_data="{Health:5.0f}", **{"axolotl/variant": '"blue"'}),
    item("ominous_bottle", ominous_bottle_amplifier="2"),
    item("debug_stick", debug_stick_state='{"minecraft:oak_stairs":"facing"}'),
    item("elytra", damage_resistant='{types:"#minecraft:is_fire"}', repairable='{items:"minecraft:phantom_membrane"}'),
    item("totem_of_undying", tooltip_style='"minecraft:fixture"', item_name='"Fixture totem"'),
    item("iron_horse_armor", equippable='{slot:"body",allowed_entities:"minecraft:horse"}'),
    item("cod_bucket", bucket_entity_data="{Health:3.0f}"),
    item("filled_map", map_id="0", map_decorations='{fixture:{type:"minecraft:red_x",x:52.0d,z:2.0d,'
                                                     'rotation:180.0f}}'),
    item("trident", enchantments='{"minecraft:loyalty":3}', can_break='{blocks:"minecraft:stone"}',
         can_place_on='{blocks:"minecraft:dirt"}'),
]

# Extra NBT per entity type: what the entity needs to exist (Item, BlockState) or stay put
# when the world is next loaded unfrozen, plus a few keys worth round-tripping.
ENTITY_EXTRA = {
    "item": "Item:" + item("diamond", 3, custom_name='"dropped diamond"') + ",Age:-32768s,PickupDelay:32767s",
    "falling_block": 'BlockState:"minecraft:anvil",Time:-2147483648',
    "tnt": "fuse:32767s",
    "experience_orb": "Value:7s,Count:1",
    "area_effect_cloud": 'Duration:2147483647,Radius:2.0f,potion_contents:{potion:"minecraft:poison"}',
    "firework_rocket": "LifeTime:2147483647,FireworksItem:" + item("firework_rocket"),
    "ominous_item_spawner": "spawn_item_after_ticks:2147483647L,item:" + item("diamond"),
    "eye_of_ender": "Item:" + item("ender_eye"),
    "evoker_fangs": "Warmup:2147483647",
    "splash_potion": "Item:" + item("splash_potion", potion_contents='{potion:"minecraft:healing"}'),
    "lingering_potion": "Item:" + item("lingering_potion", potion_contents='{potion:"minecraft:harming"}'),
    "trident": "item:" + item("trident"),
    "arrow": "pickup:1b,damage:3.0d",
    "spectral_arrow": "pickup:1b",
    "fireball": "ExplosionPower:1b",
    "marker": "data:{fixture:1b}",
    "interaction": "width:1.0f,height:1.0f,response:1b",
    "block_display": 'block_state:"minecraft:stone"',
    "item_display": "item:" + item("apple"),
    "text_display": 'text:"fixture text"',
    "painting": 'variant:"minecraft:kebab",facing:2b',
    "cushion": 'color:"red"',
    "item_frame": "Facing:2b,Item:" + item("clock") + ",ItemRotation:1b,Fixed:1b",
    "glow_item_frame": "Facing:2b,Item:" + item("compass"),
    "armor_stand": "ShowArms:1b,equipment:{head:" + item("diamond_helmet") + ",mainhand:" + item("iron_sword")
                   + "},Pose:{Head:[10.0f,0.0f,0.0f]}",
    "end_crystal": "ShowBottom:0b,beam_target:[I;41,210,7]",
    "chest_minecart": "Items:" + slots(item("rail", 16)),
    "hopper_minecart": "Items:" + slots(item("redstone", 8)),
    "furnace_minecart": "Fuel:100s",
    "spawner_minecart": 'SpawnData:{entity:{id:"minecraft:zombie"}}',
    "command_block_minecart": 'Command:"say minecart"',
    "oak_chest_boat": "Items:" + slots(item("oak_log", 8)),
    "villager": 'VillagerData:{profession:"minecraft:librarian",level:3,type:"minecraft:plains"},Xp:20,'
                "Offers:{Recipes:[{buy:" + item("emerald", 5) + ",sell:" + item("book")
                + ",maxUses:12,uses:2,xp:3,priceMultiplier:0.05f}]},Inventory:[" + item("bread", 2) + "]",
    "zombie_villager": 'VillagerData:{profession:"minecraft:farmer",level:1,type:"minecraft:desert"}',
    "wandering_trader": "DespawnDelay:48000",
    "horse": "Tame:1b,equipment:{saddle:" + item("saddle") + ",body:" + item("iron_horse_armor") + "}",
    "donkey": "Tame:1b,ChestedHorse:1b,Items:" + slots(None, None, item("apple")),
    "llama": "Tame:1b,Strength:3,equipment:{body:" + item("red_carpet") + "}",
    "pig": "equipment:{saddle:" + item("saddle") + "}",
    "sheep": "Color:5b",
    "zombie": "Health:40.0f,equipment:{mainhand:" + item("iron_shovel") + "},drop_chances:{mainhand:0.5f},"
              'attributes:[{id:"minecraft:max_health",base:40.0d,modifiers:[{id:"fixture:boost",amount:2.0d,'
              'operation:"add_value"}]},{id:"minecraft:movement_speed",base:0.3d}]',
    "slime": "Size:2",
    "magma_cube": "Size:1",
    "creeper": "powered:1b,Fuse:30s,ExplosionRadius:3b",
    "enderman": 'carriedBlockState:"minecraft:grass_block"',
    "bee": "HasNectar:1b,hive_pos:[I;8,200,8]",
    "fox": 'Type:"snow",equipment:{mainhand:' + item("sweet_berries") + "}",
    "allay": "equipment:{mainhand:" + item("cookie") + "}",
    "iron_golem": "PlayerCreated:1b",
    "snow_golem": "Pumpkin:0b",
    "piglin": "Inventory:[" + item("gold_ingot", 3) + "]",
    "phantom": "size:3",
    "shulker": "Color:5b",
    "goat": "HasLeftHorn:1b,HasRightHorn:0b",
    "panda": 'MainGene:"lazy",HiddenGene:"playful"',
    "mooshroom": 'Type:"brown"',
    "happy_ghast": "equipment:{body:" + item("red_harness") + "}",
}
WATER_CATEGORIES = {"WATER_AMBIENT", "WATER_CREATURE", "UNDERGROUND_WATER_CREATURE", "AXOLOTLS"}
WATER_MOBS = {"tadpole", "guardian", "elder_guardian", "zombie_nautilus"}
HANGING = {"painting": 4, "item_frame": 8, "glow_item_frame": 12}  # x along the wall at z 62
WALL_Z = 62
# Entities that save the block they hang on or sit on as block_pos. Vanilla reads it before
# summon moves the entity and drops it if it is 16 blocks away, so these get Pos as well.
BLOCK_ATTACHED = {"painting", "item_frame", "glow_item_frame", "cushion"}
# Entity types vanilla does not save: a leash knot is rebuilt from the mob's leash key.
NOT_SAVED = {"leash_knot", "lightning_bolt"}

# POI blocks besides the job sites already in BLOCK_ENTITIES (barrel, blast furnace, smoker,
# brewing stand, lectern, bell, beehive, test instance block).
POI_BLOCKS = [
    ("home", 44, 2, "red_bed[part=foot,facing=south]", 44, 3, "red_bed[part=head,facing=south]"),
    ("cartographer", 46, 5, "cartography_table"), ("farmer", 48, 5, "composter"),
    ("fletcher", 50, 5, "fletching_table"), ("leatherworker", 52, 5, "water_cauldron[level=2]"),
    ("mason", 54, 5, "stonecutter"), ("shepherd", 56, 5, "loom"), ("toolsmith", 58, 5, "smithing_table"),
    ("weaponsmith", 60, 5, "grindstone[face=floor]"), ("bee_nest", 46, 2, "bee_nest"),
    ("lodestone", 52, 2, "lodestone"), ("lightning_rod", 54, 2, "lightning_rod"),
]
POI_TYPES = ["armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker",
             "librarian", "mason", "shepherd", "toolsmith", "weaponsmith", "home", "meeting", "beehive", "bee_nest",
             "nether_portal", "lodestone", "test_instance", "lightning_rod"]
