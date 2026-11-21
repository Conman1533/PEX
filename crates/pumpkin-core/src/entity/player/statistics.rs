use std::collections::BTreeMap;

use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
pub use pumpkin_data::statistic::{CustomStatistic, StatisticCategory};
use pumpkin_data::{Block, BlockId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_world::chunk::format::anvil::WORLD_DATA_VERSION;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};

#[derive(Default)]
pub struct Statistics {
    /// (Category ID, Statistic ID) -> Value
    pub stats: FxHashMap<(i32, i32), i32>,
    /// Statistics from the file that PEX can't name, by stat type and then name. They are written
    /// back unchanged, like vanilla keeps a datapack's statistics.
    kept: BTreeMap<String, BTreeMap<String, i32>>,
}

impl Statistics {
    pub fn increment(&mut self, category: StatisticCategory, stat: i32, amount: i32) {
        let entry = self.stats.entry((category as i32, stat)).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    pub fn increment_custom(&mut self, stat: CustomStatistic, amount: i32) {
        self.increment(StatisticCategory::Custom, stat as i32, amount);
    }

    pub fn set(&mut self, category: StatisticCategory, stat: i32, value: i32) {
        self.stats.insert((category as i32, stat), value);
    }

    #[must_use]
    pub fn get(&self, category: StatisticCategory, stat: i32) -> i32 {
        *self.stats.get(&(category as i32, stat)).unwrap_or(&0)
    }

    /// The statistics as vanilla `ServerStatsCounter.toJson` writes `players/stats/<uuid>.json`:
    /// `{"stats": {"minecraft:custom": {"minecraft:jump": 3}}, "DataVersion": 5023}`.
    #[must_use]
    pub fn to_vanilla_json(&self) -> Value {
        let mut groups = self.kept.clone();
        for (&(category, stat), &value) in &self.stats {
            if let Some((group, name)) = stat_name(category, stat) {
                groups
                    .entry(group.to_owned())
                    .or_default()
                    .insert(name, value);
            }
        }
        json!({ "stats": groups, "DataVersion": WORLD_DATA_VERSION })
    }

    /// Replaces the statistics with those of a vanilla statistics file, like vanilla
    /// `ServerStatsCounter.parse`.
    pub fn read_vanilla_json(&mut self, json: &Value) {
        self.stats.clear();
        self.kept.clear();
        let Some(groups) = json.get("stats").and_then(Value::as_object) else {
            return;
        };
        for (group, values) in groups {
            let Some(values) = values.as_object() else {
                continue;
            };
            for (name, value) in values {
                let Some(value) = value.as_i64().and_then(|value| i32::try_from(value).ok()) else {
                    continue;
                };
                match stat_id(group, name) {
                    Some(key) => {
                        self.stats.insert(key, value);
                    }
                    None => {
                        self.kept
                            .entry(group.clone())
                            .or_default()
                            .insert(name.clone(), value);
                    }
                }
            }
        }
    }

    /// Reads the `Statistics` compound PEX 1.x kept in the player file, keyed `<category>:<id>`.
    /// Each one replaces the value already held.
    pub fn read_nbt(&mut self, nbt: &NbtCompound) {
        if let Some(stats_compound) = nbt.get_compound(LEGACY_NBT_KEY) {
            for (key, tag) in &stats_compound.child_tags {
                let parts: Vec<&str> = key.split(':').collect();
                if let (NbtTag::Int(value), [cat_str, stat_str]) = (tag, parts.as_slice())
                    && let (Ok(category), Ok(stat)) =
                        (cat_str.parse::<i32>(), stat_str.parse::<i32>())
                {
                    self.stats.insert((category, stat), *value);
                }
            }
        }
    }
}

/// The player file key PEX 1.x kept its statistics under. Vanilla keeps them in their own file.
pub const LEGACY_NBT_KEY: &str = "Statistics";

/// The stat type and value names vanilla's file uses for a statistic, like
/// `("minecraft:mined", "minecraft:stone")`.
fn stat_name(category: i32, stat: i32) -> Option<(&'static str, String)> {
    let category = StatisticCategory::from_i32(category)?;
    let id = u16::try_from(stat).ok();
    let name = match category {
        StatisticCategory::Mined => Block::from_id(BlockId::new(id?)?).name,
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => Item::from_id(id?)?.registry_key,
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            EntityType::from_raw(id?)?.resource_name
        }
        StatisticCategory::Custom => CustomStatistic::from_i32(stat)?
            .registry_key()
            .strip_prefix("minecraft:")?,
    };
    Some((category.registry_key(), format!("minecraft:{name}")))
}

/// The (category, id) PEX keeps a statistic under, from vanilla's names for it.
fn stat_id(group: &str, name: &str) -> Option<(i32, i32)> {
    if !group.starts_with("minecraft:") || !name.starts_with("minecraft:") {
        return None;
    }
    let category = StatisticCategory::from_registry_key(group)?;
    let stat = match category {
        StatisticCategory::Mined => i32::from(Block::from_name(name)?.id.as_u16()),
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => i32::from(Item::from_registry_key(name)?.id),
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            i32::from(EntityType::from_name(name)?.id)
        }
        StatisticCategory::Custom => CustomStatistic::from_registry_key(name)? as i32,
    };
    Some((category as i32, stat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_write_and_read_vanillas_file() {
        let mut stats = Statistics::default();
        stats.increment_custom(CustomStatistic::Jump, 3);
        stats.increment(
            StatisticCategory::Mined,
            i32::from(Block::STONE.id.as_u16()),
            5,
        );
        stats.increment(
            StatisticCategory::Used,
            i32::from(Item::DIAMOND_PICKAXE.id),
            2,
        );
        stats.increment(
            StatisticCategory::KilledBy,
            i32::from(EntityType::ZOMBIE.id),
            1,
        );

        let json = stats.to_vanilla_json();
        assert_eq!(
            json,
            json!({
                "stats": {
                    "minecraft:custom": { "minecraft:jump": 3 },
                    "minecraft:mined": { "minecraft:stone": 5 },
                    "minecraft:used": { "minecraft:diamond_pickaxe": 2 },
                    "minecraft:killed_by": { "minecraft:zombie": 1 },
                },
                "DataVersion": WORLD_DATA_VERSION,
            })
        );

        let mut read = Statistics::default();
        read.read_vanilla_json(&json);
        assert_eq!(read.stats, stats.stats);
    }

    #[test]
    fn statistics_pex_cant_name_are_written_back() {
        let file = json!({
            "stats": {
                "minecraft:custom": { "minecraft:jump": 4, "minecraft:not_a_stat": 9 },
                "mydatapack:things": { "mydatapack:thing": 2 },
            },
            "DataVersion": 4671,
        });
        let mut stats = Statistics::default();
        stats.read_vanilla_json(&file);
        assert_eq!(stats.get_custom(CustomStatistic::Jump), 4);
        stats.increment_custom(CustomStatistic::Jump, 1);

        assert_eq!(
            stats.to_vanilla_json()["stats"],
            json!({
                "minecraft:custom": { "minecraft:jump": 5, "minecraft:not_a_stat": 9 },
                "mydatapack:things": { "mydatapack:thing": 2 },
            })
        );
    }

    #[test]
    fn statistics_read_the_compound_pex_1_kept_in_the_player_file() {
        let mut legacy = NbtCompound::new();
        legacy.put_int(&format!("8:{}", CustomStatistic::Jump as i32), 7);
        let mut nbt = NbtCompound::new();
        nbt.put_compound(LEGACY_NBT_KEY, legacy);

        let mut stats = Statistics::default();
        stats.read_nbt(&nbt);
        assert_eq!(stats.get_custom(CustomStatistic::Jump), 7);
    }

    impl Statistics {
        fn get_custom(&self, stat: CustomStatistic) -> i32 {
            self.get(StatisticCategory::Custom, stat as i32)
        }
    }
}
