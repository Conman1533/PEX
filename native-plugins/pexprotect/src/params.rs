//! CoreProtect-style parameters: `u:Steve,Alex t:1h30m r:10 a:break c:0,64,0 w:world`.

use crate::db::Filter;

pub const MAX_RADIUS: i32 = 1000;
const MAX_ACTORS: usize = 16;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Params {
    pub actors: Vec<String>,
    /// Seconds back from now.
    pub time: Option<i64>,
    pub radius: Option<Radius>,
    pub action: Option<String>,
    pub center: Option<[i32; 3]>,
    pub world: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Radius {
    Blocks(i32),
    Global,
}

const ACTIONS: &[&str] = &["break", "place", "bucket", "burn", "explode", "change"];

pub fn parse(args: &[&str]) -> Result<Params, String> {
    let mut params = Params::default();
    for arg in args {
        let (key, value) = arg.split_once(':').ok_or(format!(
            "`{arg}` isn't a parameter; use u:, t:, r:, a:, c: or w:."
        ))?;
        match key.to_ascii_lowercase().as_str() {
            "u" | "user" => {
                params.actors = value
                    .split(',')
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect();
                if params.actors.len() > MAX_ACTORS {
                    return Err(format!("At most {MAX_ACTORS} users at once."));
                }
            }
            "t" | "time" => params.time = Some(parse_duration(value)?),
            "r" | "radius" => {
                params.radius = Some(if value.eq_ignore_ascii_case("#global") {
                    Radius::Global
                } else {
                    let radius: i32 = value
                        .parse()
                        .map_err(|_| format!("Radius `{value}` isn't a number."))?;
                    if !(0..=MAX_RADIUS).contains(&radius) {
                        return Err(format!("Radius must be 0 to {MAX_RADIUS}, or #global."));
                    }
                    Radius::Blocks(radius)
                });
            }
            "a" | "action" => {
                let action = value.to_ascii_lowercase();
                if !ACTIONS.contains(&action.as_str()) {
                    return Err(format!(
                        "Unknown action `{value}`. Actions: {}",
                        ACTIONS.join(", ")
                    ));
                }
                params.action = Some(action);
            }
            "c" | "center" => {
                let parts: Vec<i32> = value
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "Center is x,y,z in whole numbers.".to_owned())?;
                let [x, y, z] = parts[..] else {
                    return Err("Center is x,y,z.".to_owned());
                };
                params.center = Some([x, y, z]);
            }
            "w" | "world" => params.world = Some(value.to_owned()),
            _ => return Err(format!("Unknown parameter `{key}:`.")),
        }
    }
    Ok(params)
}

/// Parses `30s`, `5m`, `1h30m`, `2d`, `1w` into seconds.
pub fn parse_duration(input: &str) -> Result<i64, String> {
    let bad = || format!("Time `{input}` should look like 30s, 10m, 1h30m, 2d or 1w.");
    if input.is_empty() {
        return Err(bad());
    }
    let mut total: i64 = 0;
    let mut number = String::new();
    for c in input.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let unit = match c.to_ascii_lowercase() {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            'w' => 604_800,
            _ => return Err(bad()),
        };
        let amount: i64 = number.parse().map_err(|_| bad())?;
        total = amount
            .checked_mul(unit)
            .and_then(|seconds| total.checked_add(seconds))
            .ok_or_else(bad)?;
        number.clear();
    }
    if number.is_empty() {
        Ok(total)
    } else {
        Err(bad())
    }
}

impl Params {
    /// Builds the database filter. `here` is the command source's world and position.
    pub fn filter(&self, now: i64, here: (&str, [i32; 3])) -> Filter {
        let world = self.world.clone().unwrap_or_else(|| here.0.to_owned());
        let area = match self.radius {
            Some(Radius::Blocks(radius)) => Some((self.center.unwrap_or(here.1), radius)),
            Some(Radius::Global) | None => None,
        };
        Filter {
            actors: self.actors.clone(),
            since: self.time.map(|seconds| now - seconds),
            action: self.action.clone(),
            world: if matches!(self.radius, Some(Radius::Global)) && self.world.is_none() {
                None
            } else {
                Some(world)
            },
            area,
            rolled_back: None,
        }
    }
}

/// `90` -> `1m ago`, for lookup output.
pub fn ago(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s ago"),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse_and_combine() {
        assert_eq!(parse_duration("30s"), Ok(30));
        assert_eq!(parse_duration("1h30m"), Ok(5400));
        assert_eq!(parse_duration("2d"), Ok(172_800));
        for bad in ["", "10", "5x", "h", "99999999999999999999w"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parameters_parse() {
        let params = parse(&["u:Steve,Alex", "t:1h", "r:10", "a:break", "c:1,64,-2"]).unwrap();
        assert_eq!(params.actors, ["Steve", "Alex"]);
        assert_eq!(params.time, Some(3600));
        assert_eq!(params.radius, Some(Radius::Blocks(10)));
        assert_eq!(params.action.as_deref(), Some("break"));
        assert_eq!(params.center, Some([1, 64, -2]));
    }

    #[test]
    fn bad_parameters_are_rejected() {
        for bad in [
            &["r:-1"][..],
            &["r:5000"],
            &["a:fly"],
            &["c:1,2"],
            &["q:1"],
            &["steve"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn filter_uses_the_source_position_unless_a_center_is_given() {
        let params = parse(&["r:5", "t:10s"]).unwrap();
        let filter = params.filter(1000, ("world", [10, 64, 10]));
        assert_eq!(filter.area, Some(([10, 64, 10], 5)));
        assert_eq!(filter.since, Some(990));
        assert_eq!(filter.world.as_deref(), Some("world"));

        let global = parse(&["r:#global"])
            .unwrap()
            .filter(1000, ("world", [0, 0, 0]));
        assert_eq!(global.area, None);
        assert_eq!(global.world, None);
    }
}
