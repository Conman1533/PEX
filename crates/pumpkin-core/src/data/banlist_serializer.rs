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
}
