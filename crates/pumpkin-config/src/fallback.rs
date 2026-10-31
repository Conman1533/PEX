use serde::{Serialize, de::DeserializeOwned};
use toml_edit::{DocumentMut, Item, TableLike, Value};

use crate::{LoadMessage, default_document};

/// Deserializes `document`. A value that doesn't deserialize, or is out of range, gets its key's
/// default and one warning naming the key, the value and the default, while every other key keeps
/// its value. Vanilla `Settings.get` also falls back one key at a time.
///
/// `defaults` must be `default_config` serialized.
pub fn deserialize_per_key<C: Serialize + DeserializeOwned>(
    document: &DocumentMut,
    default_config: C,
    defaults: &DocumentMut,
    file_name: &str,
    messages: &mut Vec<LoadMessage>,
) -> C {
    if let Ok(config) = toml::from_str(&document.to_string()) {
        return config;
    }
    // The flattened root hides which key an error came from, so the file's values are put on top
    // of the defaults one at a time, and a value that breaks the result is left out.
    let mut units = Vec::new();
    collect_units(
        document.as_table(),
        defaults.as_table(),
        &mut Vec::new(),
        &mut units,
    );
    let mut merged = defaults.clone();
    let mut config = default_config;
    let mut rejected = Vec::new();
    for (path, item) in units {
        let previous = replace(&mut merged, &path, Some(item.clone()));
        match toml::from_str(&merged.to_string()) {
            Ok(merged_config) => config = merged_config,
            Err(err) => {
                replace(&mut merged, &path, previous);
                rejected.push((path, item, err.message().to_owned()));
            }
        }
    }

    let used = default_document(&config).ok();
    for (path, item, reason) in rejected {
        let fallback = used
            .as_ref()
            .and_then(|used| get(used.as_table(), &path))
            .map_or_else(
                || "Ignoring it.".to_owned(),
                |default| format!("Using the default {} instead.", display(default)),
            );
        messages.push(LoadMessage::Warn(format!(
            "{file_name}: {} = {} isn't valid ({reason}). {fallback}",
            path.join("."),
            display(&item)
        )));
    }
    config
}

/// Splits `table` into the values to try one at a time: every value, plus each table that
/// `defaults` doesn't have as a table, whole.
fn collect_units(
    table: &dyn TableLike,
    defaults: &dyn TableLike,
    path: &mut Vec<String>,
    units: &mut Vec<(Vec<String>, Item)>,
) {
    // Plain values go first, so a tag such as `world.chunk.type` is in place before the table it
    // selects is read.
    let (tables, values): (Vec<_>, Vec<_>) =
        table.iter().partition(|(_, item)| item.is_table_like());
    for (key, item) in values.into_iter().chain(tables) {
        path.push(key.to_owned());
        match (
            item.as_table_like(),
            defaults.get(key).and_then(Item::as_table_like),
        ) {
            (Some(table), Some(defaults)) => collect_units(table, defaults, path, units),
            _ => units.push((path.clone(), item.clone())),
        }
        path.pop();
    }
}

/// Puts `item` at `path` in `document`, or removes what is there if it is `None`. Returns the
/// item that was there.
fn replace(document: &mut DocumentMut, path: &[String], item: Option<Item>) -> Option<Item> {
    let (key, parents) = path.split_last()?;
    let mut table: &mut dyn TableLike = document.as_table_mut();
    for parent in parents {
        table = table.get_mut(parent)?.as_table_like_mut()?;
    }
    match item {
        Some(item) => table.insert(key, item),
        None => table.remove(key),
    }
}

fn get<'a>(table: &'a dyn TableLike, path: &[String]) -> Option<&'a Item> {
    let (key, parents) = path.split_last()?;
    let mut table = table;
    for parent in parents {
        table = table.get(parent)?.as_table_like()?;
    }
    table.get(key)
}

/// The item as one line of TOML, without its comments.
fn display(item: &Item) -> String {
    let Ok(mut value) = item.clone().into_value() else {
        return String::new();
    };
    match &mut value {
        Value::InlineTable(table) => table.fmt(),
        Value::Array(array) => array.fmt(),
        _ => {}
    }
    value.decor_mut().clear();
    value.to_string()
}
