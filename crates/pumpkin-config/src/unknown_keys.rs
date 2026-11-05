use toml_edit::{DocumentMut, Item, TableLike};

use crate::migration::{CONFIG_VERSION_KEY, is_alias};

/// The keys of `document` that none of the `known` documents has, by full path. Keys inside an
/// unknown table aren't listed again. `config_version` and the `aliases` count as known.
pub fn unknown_keys(
    document: &DocumentMut,
    known: &[DocumentMut],
    aliases: &[(&str, &str)],
) -> Vec<String> {
    let known: Vec<&dyn TableLike> = known
        .iter()
        .map(|known| known.as_table() as &dyn TableLike)
        .collect();
    let mut unknown = Vec::new();
    add_unknown_keys(document.as_table(), &known, "", aliases, &mut unknown);
    unknown
}

fn add_unknown_keys(
    table: &dyn TableLike,
    known: &[&dyn TableLike],
    path: &str,
    aliases: &[(&str, &str)],
    unknown: &mut Vec<String>,
) {
    for (key, item) in table.iter() {
        let key_path = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        let known_items: Vec<&Item> = known.iter().filter_map(|known| known.get(key)).collect();
        if known_items.is_empty() {
            if key_path != CONFIG_VERSION_KEY && !is_alias(aliases, path, key) {
                unknown.push(key_path);
            }
        } else if let Some(table) = item.as_table_like() {
            let known_tables: Vec<&dyn TableLike> = known_items
                .into_iter()
                .filter_map(Item::as_table_like)
                .collect();
            add_unknown_keys(table, &known_tables, &key_path, aliases, unknown);
        }
    }
}
