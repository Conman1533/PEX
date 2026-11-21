//! Vanilla's 26.x `DataFixers` from data version 4996 to 5016 (roadmap D7-2).
//!
//! PEX writes everything back at [`CURRENT_DATA_VERSION`], so saved data from an older vanilla,
//! including what PEX keeps without modelling (D7-1), has to be in that version's shape first,
//! or vanilla reads it as already fixed.
//!
//! Vanilla's fixers rewrite typed data (`References.ITEM_STACK`, `BLOCK_STATE`, ...) wherever the
//! schema puts it. PEX has no schema, so [`upgrade_tree`] finds those types by their shape: an item
//! stack has a string `id` and a numeric `count`, a block entity has a string `id` and no `count`,
//! a component map holds one of the component keys a fix touches, and a block state is exactly
//! `{Name}` or `{Name, Properties}`. Plugin and custom data is left alone, as vanilla leaves it.

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

/// The data version PEX writes (vanilla 26.3).
pub const CURRENT_DATA_VERSION: i32 = 5023;

/// Keys whose contents belong to plugins or players, which no vanilla fixer looks inside.
const OPAQUE_KEYS: [&str; 4] = [
    "minecraft:custom_data",
    "custom_data",
    "PumpkinCustomData",
    "BukkitValues",
];

/// Vanilla `ExplorerMapItemFix` (5008): the map item an explorer decoration stands for, and the
/// item name vanilla used to give it.
const EXPLORER_MAPS: [(&str, &str, &str); 11] = [
    (
        "minecraft:mansion",
        "minecraft:woodland_explorer_map",
        "filled_map.mansion",
    ),
    (
        "minecraft:monument",
        "minecraft:ocean_explorer_map",
        "filled_map.monument",
    ),
    (
        "minecraft:trial_chambers",
        "minecraft:trial_explorer_map",
        "filled_map.trial_chambers",
    ),
    (
        "minecraft:jungle_temple",
        "minecraft:jungle_explorer_map",
        "filled_map.explorer_jungle",
    ),
    (
        "minecraft:swamp_hut",
        "minecraft:swamp_explorer_map",
        "filled_map.explorer_swamp",
    ),
    (
        "minecraft:village_desert",
        "minecraft:desert_village_map",
        "filled_map.village_desert",
    ),
    (
        "minecraft:village_plains",
        "minecraft:plains_village_map",
        "filled_map.village_plains",
    ),
    (
        "minecraft:village_savanna",
        "minecraft:savanna_village_map",
        "filled_map.village_savanna",
    ),
    (
        "minecraft:village_snowy",
        "minecraft:snowy_village_map",
        "filled_map.village_snowy",
    ),
    (
        "minecraft:village_taiga",
        "minecraft:taiga_village_map",
        "filled_map.village_taiga",
    ),
    (
        "minecraft:red_x",
        "minecraft:buried_treasure_map",
        "filled_map.buried_treasure",
    ),
];

/// Vanilla's `ItemRenameFix` at 5012.
const RENAMED_MAPS: [(&str, &str); 5] = [
    (
        "minecraft:ocean_explorer_map",
        "minecraft:ocean_monument_map",
    ),
    ("minecraft:swamp_explorer_map", "minecraft:swamp_hut_map"),
    (
        "minecraft:trial_explorer_map",
        "minecraft:buried_trial_chambers_map",
    ),
    (
        "minecraft:woodland_explorer_map",
        "minecraft:woodland_mansion_map",
    ),
    (
        "minecraft:jungle_explorer_map",
        "minecraft:jungle_pyramid_map",
    ),
];

/// Vanilla `BlendingDataFix`: statuses that leave a chunk out of blending.
const STATUSES_TO_SKIP_BLENDING: [&str; 4] = [
    "minecraft:empty",
    "minecraft:structure_starts",
    "minecraft:structure_references",
    "minecraft:biomes",
];

/// Vanilla `MergeTerrainChunkStatusFix`: statuses merged into `minecraft:biomes`.
const OLD_INTERMEDIATE_TERRAIN_STATUSES: [&str; 2] = ["minecraft:noise", "minecraft:surface"];

/// The dimension id a level folder holds: `<world>/dimensions/<namespace>/<path>`, or the
/// overworld for the world folder itself.
#[must_use]
pub fn dimension_of_folder(folder: &crate::level::LevelFolder) -> String {
    let dim = &folder.dim_folder;
    if dim == &folder.root_folder {
        return "minecraft:overworld".to_owned();
    }
    let path = dim.file_name().and_then(|name| name.to_str());
    let parent = dim.parent();
    let namespace = parent
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str());
    let in_dimensions = parent
        .and_then(std::path::Path::parent)
        .and_then(|folder| folder.file_name())
        .is_some_and(|name| name == "dimensions");
    match (namespace, path) {
        (Some(namespace), Some(path)) if in_dimensions => format!("{namespace}:{path}"),
        _ => "minecraft:overworld".to_owned(),
    }
}

/// The `DataVersion` a saved root was written at, or `None` when it has none.
#[must_use]
pub fn data_version(root: &NbtCompound) -> Option<i32> {
    root.get_int("DataVersion")
}

/// Upgrades a region file chunk saved at `version` in `dimension` (vanilla's `__context`).
/// Returns whether anything older than [`CURRENT_DATA_VERSION`] was upgraded.
pub fn upgrade_chunk(root: &mut NbtCompound, version: i32, dimension: Option<&str>) -> bool {
    if version >= CURRENT_DATA_VERSION {
        return false;
    }
    if version < 4997
        && let Some(dimension) = dimension
    {
        blending_data_fix(root, dimension);
    }
    if version < 5009 {
        blending_data_no_value_height_fix(root);
    }
    if version < 5013 {
        merge_terrain_chunk_status_fix(root);
    }
    upgrade_compound(root, version);
    true
}

/// Upgrades any other saved root (entity chunks, player data, saved data) written at `version`.
/// Returns whether anything older than [`CURRENT_DATA_VERSION`] was upgraded.
pub fn upgrade_tree(root: &mut NbtCompound, version: i32) -> bool {
    if version >= CURRENT_DATA_VERSION {
        return false;
    }
    upgrade_compound(root, version);
    true
}

fn upgrade_tag(tag: &mut NbtTag, version: i32) {
    match tag {
        NbtTag::Compound(compound) => upgrade_compound(compound, version),
        NbtTag::List(list) => {
            for element in list {
                upgrade_tag(element, version);
            }
        }
        _ => {}
    }
}

fn upgrade_compound(compound: &mut NbtCompound, version: i32) {
    // Children first, so a fix that changes an id sees fixed contents, like vanilla's bottom-up
    // rewrite of the typed tree.
    for (key, child) in &mut compound.child_tags {
        if !OPAQUE_KEYS.contains(&key.as_ref()) {
            upgrade_tag(child, version);
        }
    }
    fix_components(compound, version);
    let has_count = compound
        .get("count")
        .is_some_and(|count| count.extract_int().is_some() || count.extract_byte().is_some());
    if let Some(id) = compound.get_string("id").map(str::to_owned) {
        if has_count {
            fix_item_stack(compound, &id, version);
        } else {
            fix_block_entity(compound, &id, version);
        }
    }
    if version < 5006 {
        block_state_field_names_fix(compound);
    }
}

/// The component fixes, for a compound that holds a component they touch.
fn fix_components(components: &mut NbtCompound, version: i32) {
    if version < 4996
        && let Some(NbtTag::List(ids)) = components.get("minecraft:pot_decorations")
        && let Some(unpacked) = unpack_pot_decorations(ids)
    {
        components.put_compound("minecraft:pot_decorations", unpacked);
    }
    if version < 5007
        && let Some(animation) = components.child_tags.remove("minecraft:swing_animation")
    {
        // Vanilla `SwingAnimationComponentSplitFix`.
        components.put("minecraft:attack_animation", animation.clone());
        components.put("minecraft:interact_animation", animation);
    }
    if version < 5008 {
        // Vanilla `MapColorComponentRemovalFix`.
        components.child_tags.remove("minecraft:map_color");
    }
    if version < 5014 {
        // Vanilla `RemoveBlockTransformerComponentFix`.
        components.child_tags.remove("minecraft:block_transformer");
    }
}

fn fix_item_stack(item: &mut NbtCompound, id: &str, version: i32) {
    let mut id = ensure_namespaced(id);
    if version < 5008 && id == "minecraft:filled_map" {
        // Vanilla `ExplorerMapItemFix`.
        let explorer = item
            .get_compound("components")
            .and_then(|components| components.get_compound("minecraft:map_decorations"))
            .and_then(|decorations| decorations.get_compound("+"))
            .and_then(|decoration| decoration.get_string("type"))
            .map(ensure_namespaced)
            .and_then(|kind| {
                EXPLORER_MAPS
                    .iter()
                    .find(|(decoration, ..)| *decoration == kind)
            });
        if let Some((_, new_id, legacy_name)) = explorer {
            (*new_id).clone_into(&mut id);
            item.put_string("id", id.clone());
            if let Some(components) = compound_mut(item, "components")
                && components
                    .get("minecraft:item_name")
                    .and_then(plain_translation_key)
                    == Some(*legacy_name)
            {
                components.child_tags.remove("minecraft:item_name");
            }
        }
    }
    if version < 5012
        && let Some((_, new_id)) = RENAMED_MAPS.iter().find(|(old, _)| *old == id)
    {
        item.put_string("id", (*new_id).to_owned());
        (*new_id).clone_into(&mut id);
    }
    if version < 5016
        && id == "minecraft:map"
        && let Some(components) = compound_mut(item, "components")
        && components
            .get("minecraft:item_name")
            .and_then(plain_translation_key)
            == Some("filled_map.buried_treasure")
        && let Some(NbtTag::Compound(name)) = components.child_tags.get_mut("minecraft:item_name")
    {
        // Vanilla `UnfilledBuriedTreasureMapFix`.
        name.put_string("translate", "item.minecraft.buried_treasure_map".to_owned());
    }
}

fn fix_block_entity(block_entity: &mut NbtCompound, id: &str, version: i32) {
    let id = ensure_namespaced(id);
    // `PotDecorationsBlockEntityUnflatteningFix` is 4996.1, so a 4996 save still needs it.
    if version <= 4996 && id == "minecraft:decorated_pot" {
        let sherds = match block_entity.get("sherds") {
            Some(NbtTag::List(ids)) => unpack_pot_decorations(ids),
            Some(_) => None,
            None => unpack_pot_decorations(&[]),
        };
        if let Some(sherds) = sherds {
            block_entity.put_compound("sherds", sherds);
        }
    }
    if version < 5002 && (id == "minecraft:sign" || id == "minecraft:hanging_sign") {
        // Vanilla `SignsOpFlagFix`.
        block_entity.put_bool("allow_op_features", true);
    }
}

/// Vanilla `PotDecorationsComponentUnflatteningFix.unpackList`: the four sides as `{id}` items,
/// bricks for missing ones. `None` keeps the original (an empty id, as vanilla).
fn unpack_pot_decorations(ids: &[NbtTag]) -> Option<NbtCompound> {
    let mut sides = NbtCompound::new();
    for (index, side) in ["back", "left", "right", "front"].into_iter().enumerate() {
        let id = ids.get(index).map_or("minecraft:brick", |tag| {
            tag.extract_string().unwrap_or("minecraft:brick")
        });
        if id.is_empty() {
            return None;
        }
        let mut item = NbtCompound::new();
        item.put_string("id", id.to_owned());
        sides.put_compound(side, item);
    }
    Some(sides)
}

/// Vanilla `BlockStateFieldNamesFix` (5006), for a compound shaped like a block state.
fn block_state_field_names_fix(compound: &mut NbtCompound) {
    let is_block_state = compound.get_string("Name").is_some()
        && compound.child_tags.len() <= 2
        && compound
            .child_tags
            .keys()
            .all(|key| matches!(key.as_ref(), "Name" | "Properties"))
        && compound
            .get("Properties")
            .is_none_or(|properties| matches!(properties, NbtTag::Compound(_)));
    if !is_block_state {
        return;
    }
    if let Some(name) = compound.child_tags.remove("Name") {
        compound.put("id", name);
    }
    if let Some(properties) = compound.child_tags.remove("Properties") {
        compound.put("properties", properties);
    }
}

/// Vanilla `BlendingDataFix` (4997): old overworld chunks blend with the new terrain.
fn blending_data_fix(chunk: &mut NbtCompound, dimension: &str) {
    chunk.child_tags.remove("blending_data");
    if ensure_namespaced(dimension) != "minecraft:overworld" {
        return;
    }
    let Some(status) = chunk.get("Status") else {
        return;
    };
    let status = ensure_namespaced(status.extract_string().unwrap_or("empty"));
    let sections = if !STATUSES_TO_SKIP_BLENDING.contains(&status.as_str()) {
        Some((-64, 384))
    } else if let Some(retrogen) = chunk.get_compound("below_zero_retrogen") {
        let target = ensure_namespaced(retrogen.get_string("target_status").unwrap_or("empty"));
        (!STATUSES_TO_SKIP_BLENDING.contains(&target.as_str())).then_some((0, 256))
    } else {
        None
    };
    if let Some((min_y, height)) = sections {
        let mut blending = NbtCompound::new();
        blending.put_int("min_section", min_y >> 4);
        blending.put_int("max_section", (min_y + height) >> 4);
        chunk.put_compound("blending_data", blending);
    }
}

/// Vanilla `BlendingDataNoValueHeightFix` (5009): blending heights become floats, with
/// `Float.MAX_VALUE` for "no value".
fn blending_data_no_value_height_fix(chunk: &mut NbtCompound) {
    let Some(blending) = compound_mut(chunk, "blending_data") else {
        return;
    };
    let Some(NbtTag::List(heights)) = blending.child_tags.get_mut("heights") else {
        return;
    };
    for height in heights {
        let value = height.extract_double().unwrap_or(f64::MAX);
        #[expect(clippy::cast_possible_truncation, reason = "vanilla casts to float")]
        let fixed = if value == f64::MAX {
            f32::MAX
        } else {
            value as f32
        };
        *height = NbtTag::Float(fixed);
    }
}

/// Vanilla `MergeTerrainChunkStatusFix` (5013): `noise` and `surface` become `biomes` and lose
/// their blocks, `carvers` becomes `terrain`.
fn merge_terrain_chunk_status_fix(chunk: &mut NbtCompound) {
    let fix_status = |status: &str| match status {
        "minecraft:carvers" => "minecraft:terrain".to_owned(),
        status if OLD_INTERMEDIATE_TERRAIN_STATUSES.contains(&status) => {
            "minecraft:biomes".to_owned()
        }
        status => status.to_owned(),
    };
    let unwrap = |tag: Option<&NbtTag>| {
        ensure_namespaced(
            tag.and_then(NbtTag::extract_string)
                .unwrap_or("minecraft:empty"),
        )
    };
    let old_status = unwrap(chunk.get("Status"));
    chunk.put_string("Status", fix_status(&old_status));
    let old_target = compound_mut(chunk, "below_zero_retrogen").map(|retrogen| {
        let old_target = unwrap(retrogen.get("target_status"));
        retrogen.put_string("target_status", fix_status(&old_target));
        old_target
    });
    let status_intermediate = OLD_INTERMEDIATE_TERRAIN_STATUSES.contains(&old_status.as_str());
    let target_intermediate = old_target
        .as_deref()
        .is_some_and(|target| OLD_INTERMEDIATE_TERRAIN_STATUSES.contains(&target));
    if !status_intermediate && !target_intermediate {
        return;
    }
    let only_below_zero = old_target.is_some() && !target_intermediate;
    chunk.child_tags.remove("Heightmaps");
    chunk.child_tags.remove("blending_data");
    if let Some(NbtTag::List(sections)) = chunk.child_tags.get_mut("sections") {
        for section in sections {
            let NbtTag::Compound(section) = section else {
                continue;
            };
            let y = section.get("Y").and_then(NbtTag::extract_byte).unwrap_or(0);
            if !(y >= 0 && only_below_zero) {
                section.child_tags.remove("block_states");
            }
        }
    }
}

fn compound_mut<'a>(compound: &'a mut NbtCompound, key: &str) -> Option<&'a mut NbtCompound> {
    match compound.child_tags.get_mut(key) {
        Some(NbtTag::Compound(child)) => Some(child),
        _ => None,
    }
}

/// Vanilla `ExtraDataFixUtils.getPlainTranslationKey`: the key of a text component that is
/// nothing but `{translate}`.
fn plain_translation_key(text: &NbtTag) -> Option<&str> {
    match text {
        NbtTag::Compound(fields) if fields.child_tags.len() == 1 => fields.get_string("translate"),
        _ => None,
    }
}

/// Vanilla `NamespacedSchema.ensureNamespaced` for the ids these fixes compare.
fn ensure_namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_list(ids: &[&str]) -> NbtTag {
        NbtTag::List(ids.iter().map(|id| NbtTag::String((*id).into())).collect())
    }

    fn item(id: &str) -> NbtCompound {
        let mut item = NbtCompound::new();
        item.put_string("id", id.to_owned());
        item.put_int("count", 1);
        item
    }

    fn translate(key: &str) -> NbtCompound {
        let mut text = NbtCompound::new();
        text.put_string("translate", key.to_owned());
        text
    }

    #[test]
    fn current_data_is_left_alone() {
        let mut root = NbtCompound::new();
        root.put_string("Status", "minecraft:carvers".to_owned());
        assert!(!upgrade_chunk(
            &mut root,
            CURRENT_DATA_VERSION,
            Some("minecraft:overworld")
        ));
        assert_eq!(root.get_string("Status"), Some("minecraft:carvers"));
    }

    #[test]
    fn pot_decorations_unflatten_in_items_and_block_entities() {
        let mut components = NbtCompound::new();
        components.put(
            "minecraft:pot_decorations",
            string_list(&["minecraft:angler_pottery_sherd", "minecraft:brick"]),
        );
        let mut stack = item("minecraft:decorated_pot");
        stack.put_compound("components", components);
        let mut pot = NbtCompound::new();
        pot.put_string("id", "minecraft:decorated_pot".to_owned());
        pot.put_int("x", 0);
        let mut root = NbtCompound::new();
        root.put_list("Items", vec![NbtTag::Compound(stack)]);
        root.put_list("block_entities", vec![NbtTag::Compound(pot)]);

        upgrade_tree(&mut root, 4995);

        let stack = root.get_list("Items").unwrap()[0]
            .extract_compound()
            .unwrap();
        let sides = stack
            .get_compound("components")
            .and_then(|components| components.get_compound("minecraft:pot_decorations"))
            .unwrap();
        let side = |name: &str| {
            sides
                .get_compound(name)
                .and_then(|side| side.get_string("id"))
        };
        assert_eq!(side("back"), Some("minecraft:angler_pottery_sherd"));
        assert_eq!(side("left"), Some("minecraft:brick"));
        assert_eq!(side("front"), Some("minecraft:brick"));
        let pot = root.get_list("block_entities").unwrap()[0]
            .extract_compound()
            .unwrap();
        let sherds = pot.get_compound("sherds").unwrap();
        assert_eq!(
            sherds
                .get_compound("right")
                .and_then(|side| side.get_string("id")),
            Some("minecraft:brick")
        );
    }

    #[test]
    fn a_4996_pot_gets_only_the_block_entity_fix() {
        let mut pot = NbtCompound::new();
        pot.put_string("id", "minecraft:decorated_pot".to_owned());
        pot.put("sherds", string_list(&["minecraft:skull_pottery_sherd"]));
        let mut components = NbtCompound::new();
        components.put(
            "minecraft:pot_decorations",
            string_list(&["minecraft:brick"]),
        );
        pot.put_compound("components", components);

        upgrade_tree(&mut pot, 4996);

        assert!(pot.get_compound("sherds").is_some());
        assert!(
            pot.get_compound("components")
                .unwrap()
                .get_list("minecraft:pot_decorations")
                .is_some()
        );
    }

    #[test]
    fn signs_get_the_op_flag() {
        let mut sign = NbtCompound::new();
        sign.put_string("id", "minecraft:hanging_sign".to_owned());
        upgrade_tree(&mut sign, 5001);
        assert_eq!(sign.get_bool("allow_op_features"), Some(true));
    }

    #[test]
    fn block_states_rename_their_fields() {
        let mut properties = NbtCompound::new();
        properties.put_string("axis", "y".to_owned());
        let mut state = NbtCompound::new();
        state.put_string("Name", "minecraft:oak_log".to_owned());
        state.put_compound("Properties", properties);
        let mut falling = NbtCompound::new();
        falling.put_string("id", "minecraft:falling_block".to_owned());
        falling.put_compound("BlockState", state);
        // An entity's own `CustomName`-style keys are no block state.
        let mut other = NbtCompound::new();
        other.put_string("Name", "x".to_owned());
        other.put_int("Value", 1);
        falling.put_compound("other", other);

        upgrade_tree(&mut falling, 5005);

        let state = falling.get_compound("BlockState").unwrap();
        assert_eq!(state.get_string("id"), Some("minecraft:oak_log"));
        assert!(state.get_compound("properties").is_some());
        assert!(state.get("Name").is_none());
        assert_eq!(
            falling.get_compound("other").unwrap().get_string("Name"),
            Some("x")
        );
    }

    #[test]
    fn components_split_and_drop() {
        let mut components = NbtCompound::new();
        components.put_string("minecraft:swing_animation", "whack".to_owned());
        components.put_int("minecraft:map_color", 3);
        components.put_compound("minecraft:block_transformer", NbtCompound::new());
        let mut stack = item("minecraft:stick");
        stack.put_compound("components", components);

        upgrade_tree(&mut stack, 5006);

        let components = stack.get_compound("components").unwrap();
        assert_eq!(
            components.get_string("minecraft:attack_animation"),
            Some("whack")
        );
        assert_eq!(
            components.get_string("minecraft:interact_animation"),
            Some("whack")
        );
        assert!(components.get("minecraft:swing_animation").is_none());
        assert!(components.get("minecraft:map_color").is_none());
        assert!(components.get("minecraft:block_transformer").is_none());
    }

    #[test]
    fn explorer_maps_become_their_own_items() {
        let mut decoration = NbtCompound::new();
        decoration.put_string("type", "monument".to_owned());
        let mut decorations = NbtCompound::new();
        decorations.put_compound("+", decoration);
        let mut components = NbtCompound::new();
        components.put_compound("minecraft:map_decorations", decorations);
        components.put_compound("minecraft:item_name", translate("filled_map.monument"));
        let mut stack = item("minecraft:filled_map");
        stack.put_compound("components", components);

        upgrade_tree(&mut stack, 5007);

        assert_eq!(stack.get_string("id"), Some("minecraft:ocean_monument_map"));
        assert!(
            stack
                .get_compound("components")
                .unwrap()
                .get("minecraft:item_name")
                .is_none()
        );
    }

    #[test]
    fn an_unfilled_treasure_map_gets_its_new_name() {
        let mut components = NbtCompound::new();
        components.put_compound(
            "minecraft:item_name",
            translate("filled_map.buried_treasure"),
        );
        let mut stack = item("minecraft:map");
        stack.put_compound("components", components);

        upgrade_tree(&mut stack, 5015);

        let name = stack
            .get_compound("components")
            .and_then(|components| components.get_compound("minecraft:item_name"))
            .and_then(|name| name.get_string("translate"));
        assert_eq!(name, Some("item.minecraft.buried_treasure_map"));
    }

    #[test]
    fn custom_data_is_never_touched() {
        let mut state = NbtCompound::new();
        state.put_string("Name", "minecraft:stone".to_owned());
        let mut custom = NbtCompound::new();
        custom.put_compound("state", state);
        let mut components = NbtCompound::new();
        components.put_compound("minecraft:custom_data", custom);
        let mut stack = item("minecraft:stick");
        stack.put_compound("components", components);

        upgrade_tree(&mut stack, 4500);

        let state = stack
            .get_compound("components")
            .and_then(|components| components.get_compound("minecraft:custom_data"))
            .and_then(|custom| custom.get_compound("state"))
            .unwrap();
        assert_eq!(state.get_string("Name"), Some("minecraft:stone"));
    }

    #[test]
    fn old_overworld_chunks_blend_and_statuses_merge() {
        let mut chunk = NbtCompound::new();
        chunk.put_string("Status", "minecraft:full".to_owned());
        upgrade_chunk(&mut chunk, 4900, Some("minecraft:overworld"));
        let blending = chunk.get_compound("blending_data").unwrap();
        assert_eq!(blending.get_int("min_section"), Some(-4));
        assert_eq!(blending.get_int("max_section"), Some(20));

        let mut nether = NbtCompound::new();
        nether.put_string("Status", "minecraft:full".to_owned());
        nether.put_compound("blending_data", NbtCompound::new());
        upgrade_chunk(&mut nether, 4900, Some("minecraft:the_nether"));
        assert!(nether.get("blending_data").is_none());

        let mut section_below = NbtCompound::new();
        section_below.put_byte("Y", -1);
        section_below.put_compound("block_states", NbtCompound::new());
        let mut section_above = NbtCompound::new();
        section_above.put_byte("Y", 2);
        section_above.put_compound("block_states", NbtCompound::new());
        let mut proto = NbtCompound::new();
        proto.put_string("Status", "noise".to_owned());
        proto.put_compound("Heightmaps", NbtCompound::new());
        proto.put_list(
            "sections",
            vec![
                NbtTag::Compound(section_below),
                NbtTag::Compound(section_above),
            ],
        );
        upgrade_chunk(&mut proto, 5012, Some("minecraft:overworld"));
        assert_eq!(proto.get_string("Status"), Some("minecraft:biomes"));
        assert!(proto.get("Heightmaps").is_none());
        for section in proto.get_list("sections").unwrap() {
            assert!(
                section
                    .extract_compound()
                    .unwrap()
                    .get("block_states")
                    .is_none()
            );
        }

        let mut carved = NbtCompound::new();
        carved.put_string("Status", "minecraft:carvers".to_owned());
        upgrade_chunk(&mut carved, 5012, Some("minecraft:overworld"));
        assert_eq!(carved.get_string("Status"), Some("minecraft:terrain"));
    }

    #[test]
    fn blending_heights_become_floats() {
        let mut blending = NbtCompound::new();
        blending.put_list(
            "heights",
            vec![NbtTag::Double(f64::MAX), NbtTag::Double(70.5)],
        );
        let mut chunk = NbtCompound::new();
        chunk.put_string("Status", "minecraft:full".to_owned());
        chunk.put_compound("blending_data", blending);
        upgrade_chunk(&mut chunk, 5008, Some("minecraft:overworld"));
        let heights = chunk
            .get_compound("blending_data")
            .and_then(|blending| blending.get_list("heights"))
            .unwrap();
        assert_eq!(heights[0].extract_float(), Some(f32::MAX));
        assert_eq!(heights[1].extract_float(), Some(70.5));
    }
}
