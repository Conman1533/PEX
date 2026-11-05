use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::{env, fs};

use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime};
use tracing::warn;
use uuid::Uuid;

use super::banlist_serializer::{format_date, parse_date};

pub(super) const USER_CACHE_PATH: &str = "usercache.json";
const USER_CACHE_MRU_LIMIT: usize = 1000;

#[derive(Clone, Debug)]
pub struct UserCacheEntry {
    pub uuid: Uuid,
    pub name: String,
    expiration_date: OffsetDateTime,
    last_access: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserCacheEntryDisk {
    uuid: Uuid,
    name: String,
    expires_on: String,
}

#[derive(Default)]
pub struct UserCache {
    profiles_by_name: FxHashMap<String, UserCacheEntry>,
    profiles_by_uuid: FxHashMap<Uuid, UserCacheEntry>,
    operation_count: u64,
}

impl UserCache {
    fn path() -> std::path::PathBuf {
        env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .join(super::DATA_FOLDER)
            .join(USER_CACHE_PATH)
    }

    #[must_use]
    pub fn load() -> Self {
        let mut cache = Self::default();
        let mut loaded = Self::load_entries();
        loaded.reverse();
        for entry in loaded {
            cache.safe_add(entry);
        }
        cache
    }

    pub fn save(&self) {
        let path = Self::path();
        if !super::may_save(&path) {
            return;
        }
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            warn!("Failed to create user cache directory: {error}");
            return;
        }

        let to_save: Vec<UserCacheEntryDisk> = self
            .top_mru_profiles(USER_CACHE_MRU_LIMIT)
            .into_iter()
            .map(|entry| UserCacheEntryDisk {
                uuid: entry.uuid,
                name: entry.name,
                expires_on: format_date(entry.expiration_date),
            })
            .collect();

        let Ok(content) = serde_json::to_string(&to_save) else {
            return;
        };

        if let Err(error) = fs::write(path, content) {
            warn!("Failed to save user cache: {error}");
        }
    }

    pub fn upsert(&mut self, uuid: Uuid, name: String) {
        self.add_internal(uuid, name);
    }

    pub fn get_by_name(&mut self, name: &str) -> Option<UserCacheEntry> {
        let lowercase_name = name.to_ascii_lowercase();
        let mut profile = self.profiles_by_name.get(&lowercase_name).cloned();
        let mut needs_save = false;

        if let Some(entry) = &profile
            && is_expired(entry.expiration_date)
        {
            self.profiles_by_uuid.remove(&entry.uuid);
            self.profiles_by_name
                .remove(&entry.name.to_ascii_lowercase());
            needs_save = true;
            profile = None;
        }

        if let Some(mut entry) = profile {
            entry.last_access = self.next_operation();
            self.profiles_by_name
                .insert(entry.name.to_ascii_lowercase(), entry.clone());
            self.profiles_by_uuid.insert(entry.uuid, entry.clone());
            return Some(entry);
        }

        if needs_save {
            self.save();
        }

        None
    }

    pub fn get_by_uuid(&mut self, uuid: Uuid) -> Option<UserCacheEntry> {
        let mut entry = self.profiles_by_uuid.get(&uuid).cloned()?;
        entry.last_access = self.next_operation();
        self.profiles_by_name
            .insert(entry.name.to_ascii_lowercase(), entry.clone());
        self.profiles_by_uuid.insert(entry.uuid, entry.clone());
        Some(entry)
    }

    fn add_internal(&mut self, uuid: Uuid, name: String) -> UserCacheEntry {
        let expiration_date = one_month_from_now();
        let entry = UserCacheEntry {
            uuid,
            name,
            expiration_date,
            last_access: 0,
        };

        self.safe_add(entry.clone());
        self.save();
        entry
    }

    fn safe_add(&mut self, mut entry: UserCacheEntry) {
        entry.last_access = self.next_operation();
        self.profiles_by_name
            .insert(entry.name.to_ascii_lowercase(), entry.clone());
        self.profiles_by_uuid.insert(entry.uuid, entry);
    }

    #[allow(clippy::missing_const_for_fn)]
    fn next_operation(&mut self) -> u64 {
        self.operation_count += 1;
        self.operation_count
    }

    fn top_mru_profiles(&self, limit: usize) -> Vec<UserCacheEntry> {
        let mut entries: Vec<UserCacheEntry> = self.profiles_by_uuid.values().cloned().collect();
        entries.sort_by_key(|entry| Reverse(entry.last_access));
        entries.truncate(limit);
        entries
    }

    fn load_entries() -> Vec<UserCacheEntry> {
        let Some(array) = super::load_list::<Vec<serde_json::Value>>(&Self::path()) else {
            return Vec::new();
        };

        let mut entries = Vec::new();
        for element in array {
            let Some(object) = element.as_object() else {
                continue;
            };

            let Some(name) = object.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(uuid_raw) = object.get("uuid").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(expires_on) = object.get("expiresOn").and_then(serde_json::Value::as_str)
            else {
                continue;
            };

            let Ok(uuid) = Uuid::parse_str(uuid_raw) else {
                continue;
            };

            let Ok(expiration_date) = parse_date(expires_on) else {
                continue;
            };

            entries.push(UserCacheEntry {
                uuid,
                name: name.to_string(),
                expiration_date,
                last_access: 0,
            });
        }

        entries
    }
}

fn is_expired(expiration_date: OffsetDateTime) -> bool {
    OffsetDateTime::now_utc() >= expiration_date
}

fn one_month_from_now() -> OffsetDateTime {
    one_month_after(OffsetDateTime::now_utc())
}

/// Vanilla `ZonedDateTime.plusMonths(1)`: the same day next month, or that month's last day.
fn one_month_after(date: OffsetDateTime) -> OffsetDateTime {
    let (year, month) = match date.month() {
        Month::December => (date.year() + 1, Month::January),
        month => (date.year(), month.next()),
    };
    let day = date.day().min(month.length(year));
    Date::from_calendar_date(year, month, day).map_or(date, |next| date.replace_date(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: Month, day: u8) -> OffsetDateTime {
        Date::from_calendar_date(year, month, day)
            .unwrap()
            .midnight()
            .assume_utc()
    }

    #[test]
    fn entries_expire_one_calendar_month_later() {
        assert_eq!(
            one_month_after(at(2026, Month::November, 4)),
            at(2026, Month::December, 4)
        );
        assert_eq!(
            one_month_after(at(2026, Month::December, 15)),
            at(2027, Month::January, 15)
        );
        assert_eq!(
            one_month_after(at(2027, Month::January, 31)),
            at(2027, Month::February, 28)
        );
        assert_eq!(
            one_month_after(at(2028, Month::January, 31)),
            at(2028, Month::February, 29)
        );
    }

    #[test]
    fn the_disk_entry_uses_vanilla_keys_and_date_format() {
        let entry = UserCacheEntryDisk {
            uuid: Uuid::nil(),
            name: "Notch".to_string(),
            expires_on: format_date(at(2026, Month::December, 4)),
        };
        assert_eq!(
            serde_json::to_string(&entry).unwrap(),
            r#"{"uuid":"00000000-0000-0000-0000-000000000000","name":"Notch","expiresOn":"2026-12-04 00:00:00 +0000"}"#
        );
    }
}
