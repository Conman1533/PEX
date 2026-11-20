use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::net::GameProfile;

#[derive(Debug, Serialize, Deserialize)]
pub struct BannedPlayerEntry {
    pub uuid: Uuid,
    pub name: String,
    #[serde(with = "format::date")]
    pub created: time::OffsetDateTime,
    pub source: String,
    #[serde(with = "format::option_date")]
    pub expires: Option<time::OffsetDateTime>,
    pub reason: String,
}

impl BannedPlayerEntry {
    #[must_use]
    pub fn new(
        profile: &GameProfile,
        source: String,
        expires: Option<time::OffsetDateTime>,
        reason: String,
    ) -> Self {
        Self {
            uuid: profile.id,
            name: profile.name.clone(),
            created: OffsetDateTime::now_utc(),
            source,
            expires,
            reason,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BannedIpEntry {
    pub ip: IpAddr,
    #[serde(with = "format::date")]
    pub created: time::OffsetDateTime,
    pub source: String,
    #[serde(with = "format::option_date")]
    pub expires: Option<time::OffsetDateTime>,
    pub reason: String,
}

impl BannedIpEntry {
    #[must_use]
    pub fn new(
        ip: IpAddr,
        source: String,
        expires: Option<time::OffsetDateTime>,
        reason: String,
    ) -> Self {
        Self {
            ip,
            created: OffsetDateTime::now_utc(),
            source,
            expires,
            reason,
        }
    }
}

/// Vanilla's `yyyy-MM-dd HH:mm:ss Z`, which ban lists and the user cache both use.
const DATE_FORMAT: &[time::format_description::FormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
);
/// The form older Pumpkin builds wrote, kept readable.
const PUMPKIN_DATE_FORMAT: &[time::format_description::FormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
);

/// Formats a date the way vanilla writes it in ban lists and `usercache.json`.
#[must_use]
pub fn format_date(date: OffsetDateTime) -> String {
    date.format(DATE_FORMAT).unwrap_or_default()
}

/// Reads a date in vanilla's form, or the RFC 3339 and older forms Pumpkin wrote.
pub fn parse_date(date: &str) -> Result<OffsetDateTime, time::error::Parse> {
    OffsetDateTime::parse(date, DATE_FORMAT)
        .or_else(|_| OffsetDateTime::parse(date, PUMPKIN_DATE_FORMAT))
        .or_else(|_| OffsetDateTime::parse(date, &Rfc3339))
}

mod format {
    pub mod date {
        use serde::{self, Deserialize, Deserializer, Serializer};
        use time::OffsetDateTime;

        pub fn serialize<S: Serializer>(
            date: &OffsetDateTime,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&super::super::format_date(*date))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<OffsetDateTime, D::Error> {
            let s = String::deserialize(deserializer)?;
            super::super::parse_date(&s).map_err(serde::de::Error::custom)
        }
    }

    pub mod option_date {
        use serde::{self, Deserialize, Deserializer, Serializer};
        use time::OffsetDateTime;

        #[expect(clippy::ref_option)]
        pub fn serialize<S: Serializer>(
            date: &Option<OffsetDateTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match date {
                Some(date) => serializer.serialize_str(&super::super::format_date(*date)),
                None => serializer.serialize_str("forever"),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<OffsetDateTime>, D::Error> {
            let s = String::deserialize(deserializer)?;
            if s == "forever" {
                Ok(None)
            } else {
                super::super::parse_date(&s)
                    .map(Some)
                    .map_err(serde::de::Error::custom)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_written_like_vanilla_and_old_forms_still_read() {
        let date = OffsetDateTime::from_unix_timestamp(1_793_834_412).unwrap();
        let written = format_date(date);
        assert_eq!(written, "2026-11-04 23:20:12 +0000");
        assert_eq!(parse_date(&written).unwrap(), date);
        assert_eq!(parse_date("2026-11-04 23:20:12+00:00").unwrap(), date);
        assert_eq!(parse_date("2026-11-04T23:20:12Z").unwrap(), date);
        assert_eq!(parse_date("2026-11-04 18:20:12 -0500").unwrap(), date);
    }

    #[test]
    fn a_ban_entry_round_trips_through_json() {
        let json = r#"{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"Notch","created":"2026-11-04 23:20:12 +0000","source":"Server","expires":"forever","reason":"Banned by an operator."}"#;
        let entry: BannedPlayerEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.expires, None);
        assert_eq!(serde_json::to_string(&entry).unwrap(), json);
    }

    /// Written by vanilla 26.3 (offline, `ban fixturebot Testing bans`,
    /// `ban-ip 203.0.113.7 Testing ip bans`) on 2026-11-20.
    const VANILLA_BANNED_PLAYERS: &str = "[\n  {\n    \"uuid\": \"2f1e2305-25df-3dac-bfa2-b5baaab4b251\",\n    \"name\": \"fixturebot\",\n    \"created\": \"2026-11-20 02:47:33 +0000\",\n    \"source\": \"Server\",\n    \"expires\": \"forever\",\n    \"reason\": \"Testing bans\"\n  }\n]";
    const VANILLA_BANNED_IPS: &str = "[\n  {\n    \"ip\": \"203.0.113.7\",\n    \"created\": \"2026-11-20 02:47:33 +0000\",\n    \"source\": \"Server\",\n    \"expires\": \"forever\",\n    \"reason\": \"Testing ip bans\"\n  }\n]";
    /// Written by PEX 1.2.0 (the v1.2.0 release binary) for the same commands.
    const PEX_1_2_0_BANNED_PLAYERS: &str = "[\n  {\n    \"uuid\": \"c49a0338-03fb-89e2-e6ce-7a3721039f79\",\n    \"name\": \"fixturebot\",\n    \"created\": \"2026-11-20 02:47:38+00:00\",\n    \"source\": \"Server\",\n    \"expires\": \"forever\",\n    \"reason\": \"Testing bans\"\n  }\n]";
    const PEX_1_2_0_BANNED_IPS: &str = "[\n  {\n    \"ip\": \"203.0.113.7\",\n    \"created\": \"2026-11-20 02:47:39+00:00\",\n    \"source\": \"Server\",\n    \"expires\": \"forever\",\n    \"reason\": \"Testing ip bans\"\n  }\n]";

    /// Loads `contents` as the list's file from a fresh data folder, saves it there again and
    /// returns what was written.
    fn through_the_list<T>(name: &str, contents: &str) -> (T, String)
    where
        T: crate::data::SaveJSONConfiguration + Default + Serialize + for<'de> Deserialize<'de>,
    {
        let dir = std::env::temp_dir().join(format!("pex-banlist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(T::get_path());
        std::fs::write(&path, contents).unwrap();
        let list = T::load_from(&dir);
        std::fs::remove_file(&path).unwrap();
        list.save_to(&dir);
        let written = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        (list, written)
    }

    #[test]
    fn vanilla_ban_lists_load_and_save_byte_for_byte() {
        let (players, written) = through_the_list::<crate::data::banned_player::BannedPlayerList>(
            "vanilla-players",
            VANILLA_BANNED_PLAYERS,
        );
        assert_eq!(players.banned_players.len(), 1);
        assert_eq!(players.banned_players[0].name, "fixturebot");
        assert_eq!(written, VANILLA_BANNED_PLAYERS);
        let (ips, written) = through_the_list::<crate::data::banned_ip::BannedIpList>(
            "vanilla-ips",
            VANILLA_BANNED_IPS,
        );
        assert_eq!(ips.banned_ips.len(), 1);
        assert_eq!(written, VANILLA_BANNED_IPS);
    }

    #[test]
    fn pex_1_2_0_ban_lists_still_load() {
        let (players, written) = through_the_list::<crate::data::banned_player::BannedPlayerList>(
            "pex120-players",
            PEX_1_2_0_BANNED_PLAYERS,
        );
        assert_eq!(players.banned_players.len(), 1);
        assert_eq!(
            written,
            PEX_1_2_0_BANNED_PLAYERS.replace("02:47:38+00:00", "02:47:38 +0000")
        );
        let (ips, written) = through_the_list::<crate::data::banned_ip::BannedIpList>(
            "pex120-ips",
            PEX_1_2_0_BANNED_IPS,
        );
        assert_eq!(ips.banned_ips.len(), 1);
        assert_eq!(
            written,
            PEX_1_2_0_BANNED_IPS.replace("02:47:39+00:00", "02:47:39 +0000")
        );
    }
}
