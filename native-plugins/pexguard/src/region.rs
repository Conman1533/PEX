//! Regions, flags and the rules for deciding what is allowed where. No server access here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const GLOBAL_REGION: &str = "__global__";

/// Every flag a region can set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Build,
    Interact,
    ChestAccess,
    Pvp,
    MobDamage,
    MobSpawning,
    Explosions,
    FireSpread,
    LiquidFlow,
    Pistons,
    Entry,
    Greeting,
    Farewell,
}

impl Flag {
    pub const ALL: [Self; 13] = [
        Self::Build,
        Self::Interact,
        Self::ChestAccess,
        Self::Pvp,
        Self::MobDamage,
        Self::MobSpawning,
        Self::Explosions,
        Self::FireSpread,
        Self::LiquidFlow,
        Self::Pistons,
        Self::Entry,
        Self::Greeting,
        Self::Farewell,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Interact => "interact",
            Self::ChestAccess => "chest-access",
            Self::Pvp => "pvp",
            Self::MobDamage => "mob-damage",
            Self::MobSpawning => "mob-spawning",
            Self::Explosions => "explosions",
            Self::FireSpread => "fire-spread",
            Self::LiquidFlow => "liquid-flow",
            Self::Pistons => "pistons",
            Self::Entry => "entry",
            Self::Greeting => "greeting",
            Self::Farewell => "farewell",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|flag| flag.name().eq_ignore_ascii_case(name))
    }

    /// Message flags hold text; every other flag is `allow` or `deny`.
    pub const fn is_message(self) -> bool {
        matches!(self, Self::Greeting | Self::Farewell)
    }

    /// Owners and members of the top region ignore these flags.
    pub const fn members_bypass(self) -> bool {
        matches!(
            self,
            Self::Build | Self::Interact | Self::ChestAccess | Self::Entry
        )
    }

    /// Inside a region, these are denied to non-members unless a flag says otherwise.
    const fn denied_in_regions_by_default(self) -> bool {
        matches!(self, Self::Build | Self::Interact | Self::ChestAccess)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub uuid: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub world: String,
    pub min: [i32; 3],
    pub max: [i32; 3],
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub owners: Vec<Member>,
    #[serde(default)]
    pub members: Vec<Member>,
    /// Flag name to value: `allow`, `deny`, or the text of a message flag.
    #[serde(default)]
    pub flags: BTreeMap<String, String>,
}

impl Region {
    pub fn new(name: String, world: String, a: [i32; 3], b: [i32; 3]) -> Self {
        Self {
            name,
            world,
            min: [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
            max: [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
            priority: 0,
            owners: Vec::new(),
            members: Vec::new(),
            flags: BTreeMap::new(),
        }
    }

    pub fn contains(&self, world: &str, pos: [i32; 3]) -> bool {
        self.world == world && (0..3).all(|i| self.min[i] <= pos[i] && pos[i] <= self.max[i])
    }

    /// Whether this region overlaps the box from `min` to `max` (inclusive).
    pub fn intersects(&self, world: &str, min: [i32; 3], max: [i32; 3]) -> bool {
        self.world == world && (0..3).all(|i| self.min[i] <= max[i] && min[i] <= self.max[i])
    }

    pub fn is_member(&self, uuid: &str) -> bool {
        self.owners
            .iter()
            .chain(&self.members)
            .any(|member| member.uuid == uuid)
    }

    pub fn volume(&self) -> u64 {
        (0..3)
            .map(|i| u64::from(self.max[i].abs_diff(self.min[i])) + 1)
            .fold(1u64, u64::saturating_mul)
    }

    fn state(&self, flag: Flag) -> Option<bool> {
        parse_state(self.flags.get(flag.name())?)
    }
}

fn parse_state(value: &str) -> Option<bool> {
    match value {
        "allow" => Some(true),
        "deny" => Some(false),
        _ => None,
    }
}

/// Validates a flag value from a command, returning the normalized value to store.
pub fn normalize_value(flag: Flag, value: &str) -> Result<String, String> {
    if flag.is_message() {
        return Ok(value.to_owned());
    }
    match value.to_ascii_lowercase().as_str() {
        "allow" | "true" | "yes" => Ok("allow".to_owned()),
        "deny" | "false" | "no" => Ok("deny".to_owned()),
        _ => Err(format!("`{}` takes allow or deny.", flag.name())),
    }
}

/// Everything saved in `regions.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RegionSet {
    pub regions: Vec<Region>,
    /// World name to its `__global__` flags.
    pub globals: BTreeMap<String, BTreeMap<String, String>>,
}

impl RegionSet {
    pub fn get(&self, name: &str) -> Option<&Region> {
        self.regions
            .iter()
            .find(|region| region.name.eq_ignore_ascii_case(name))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Region> {
        self.regions
            .iter_mut()
            .find(|region| region.name.eq_ignore_ascii_case(name))
    }

    /// Regions containing `pos`, highest priority first; smaller regions win ties.
    pub fn at(&self, world: &str, pos: [i32; 3]) -> Vec<&Region> {
        let mut regions: Vec<_> = self
            .regions
            .iter()
            .filter(|region| region.contains(world, pos))
            .collect();
        regions.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.volume().cmp(&b.volume()))
                .then(a.name.cmp(&b.name))
        });
        regions
    }

    /// Decides whether `flag` allows the action at `pos` for `actor` (a player UUID, or `None`
    /// for things like fire and pistons that nobody is directly responsible for).
    pub fn allows(&self, world: &str, pos: [i32; 3], flag: Flag, actor: Option<&str>) -> bool {
        let regions = self.at(world, pos);
        if flag.members_bypass()
            && let (Some(top), Some(actor)) = (regions.first(), actor)
            && top.is_member(actor)
        {
            return true;
        }
        if let Some(state) = regions.iter().find_map(|region| region.state(flag)) {
            return state;
        }
        if let Some(state) = self
            .globals
            .get(world)
            .and_then(|flags| flags.get(flag.name()))
            .and_then(|value| parse_state(value))
        {
            return state;
        }
        !flag.denied_in_regions_by_default() || regions.is_empty()
    }

    /// Whether any region or the global flags deny `flag` somewhere inside the box.
    ///
    /// Used for explosions, where the event gives no list of the blocks that will be destroyed.
    pub fn denies_in_box(&self, world: &str, min: [i32; 3], max: [i32; 3], flag: Flag) -> bool {
        let global_deny = self
            .globals
            .get(world)
            .and_then(|flags| flags.get(flag.name()))
            .and_then(|value| parse_state(value))
            == Some(false);
        global_deny
            || self.regions.iter().any(|region| {
                region.intersects(world, min, max) && region.state(flag) == Some(false)
            })
    }
}

/// Names of regions at `to` that aren't at `from`, for greetings and entry checks.
pub fn entered<'a>(from: &[&'a Region], to: &[&'a Region]) -> Vec<&'a Region> {
    to.iter()
        .filter(|region| !from.iter().any(|old| old.name == region.name))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "alice-uuid";
    const BOB: &str = "bob-uuid";

    fn set_with(region: Region) -> RegionSet {
        RegionSet {
            regions: vec![region],
            globals: BTreeMap::new(),
        }
    }

    fn spawn() -> Region {
        let mut region = Region::new(
            "spawn".into(),
            "overworld".into(),
            [10, 0, 10],
            [-10, 100, -10],
        );
        region.owners.push(Member {
            uuid: ALICE.into(),
            name: "Alice".into(),
        });
        region
    }

    #[test]
    fn new_region_normalizes_corners() {
        let region = spawn();
        assert_eq!(region.min, [-10, 0, -10]);
        assert_eq!(region.max, [10, 100, 10]);
        assert!(region.contains("overworld", [10, 100, -10]));
        assert!(!region.contains("overworld", [11, 50, 0]));
        assert!(!region.contains("the_nether", [0, 50, 0]));
    }

    #[test]
    fn non_members_cannot_build_inside_but_can_outside() {
        let set = set_with(spawn());
        assert!(!set.allows("overworld", [0, 64, 0], Flag::Build, Some(BOB)));
        assert!(set.allows("overworld", [0, 64, 0], Flag::Build, Some(ALICE)));
        assert!(set.allows("overworld", [50, 64, 0], Flag::Build, Some(BOB)));
    }

    #[test]
    fn explicit_allow_opens_building_to_everyone() {
        let mut region = spawn();
        region.flags.insert("build".into(), "allow".into());
        let set = set_with(region);
        assert!(set.allows("overworld", [0, 64, 0], Flag::Build, Some(BOB)));
    }

    #[test]
    fn non_member_flags_ignore_membership() {
        let mut region = spawn();
        region.flags.insert("pvp".into(), "deny".into());
        let set = set_with(region);
        assert!(!set.allows("overworld", [0, 64, 0], Flag::Pvp, Some(ALICE)));
        assert!(set.allows("overworld", [50, 64, 0], Flag::Pvp, Some(ALICE)));
    }

    #[test]
    fn higher_priority_region_decides_and_unset_flags_fall_through() {
        let mut outer = spawn();
        outer.flags.insert("pvp".into(), "deny".into());
        outer.flags.insert("fire-spread".into(), "deny".into());
        let mut arena = Region::new("arena".into(), "overworld".into(), [0, 0, 0], [5, 100, 5]);
        arena.priority = 10;
        arena.flags.insert("pvp".into(), "allow".into());
        let set = RegionSet {
            regions: vec![outer, arena],
            globals: BTreeMap::new(),
        };
        assert!(set.allows("overworld", [2, 64, 2], Flag::Pvp, None));
        assert!(!set.allows("overworld", [-5, 64, -5], Flag::Pvp, None));
        // The arena doesn't set fire-spread, so the outer region's deny applies.
        assert!(!set.allows("overworld", [2, 64, 2], Flag::FireSpread, None));
    }

    #[test]
    fn membership_counts_only_for_the_top_region() {
        let outer = spawn();
        let mut shop = Region::new("shop".into(), "overworld".into(), [0, 0, 0], [5, 100, 5]);
        shop.priority = 5;
        let set = RegionSet {
            regions: vec![outer, shop],
            globals: BTreeMap::new(),
        };
        // Alice owns spawn but not the higher-priority shop inside it.
        assert!(!set.allows("overworld", [2, 64, 2], Flag::Build, Some(ALICE)));
    }

    #[test]
    fn global_flags_apply_outside_regions() {
        let mut set = set_with(spawn());
        set.globals
            .entry("overworld".into())
            .or_default()
            .insert("pvp".into(), "deny".into());
        assert!(!set.allows("overworld", [500, 64, 500], Flag::Pvp, None));
        assert!(set.allows("the_nether", [500, 64, 500], Flag::Pvp, None));
    }

    #[test]
    fn explosion_box_check_sees_nearby_regions() {
        let mut region = spawn();
        region.flags.insert("explosions".into(), "deny".into());
        let set = set_with(region);
        assert!(set.denies_in_box("overworld", [7, 60, 0], [23, 70, 8], Flag::Explosions));
        assert!(!set.denies_in_box("overworld", [7, 60, 0], [23, 70, 8], Flag::Pvp));
        assert!(!set.denies_in_box("overworld", [30, 60, 0], [40, 70, 8], Flag::Explosions));
    }

    #[test]
    fn flag_values_are_validated() {
        assert_eq!(normalize_value(Flag::Pvp, "DENY").as_deref(), Ok("deny"));
        assert!(normalize_value(Flag::Pvp, "maybe").is_err());
        assert_eq!(
            normalize_value(Flag::Greeting, "Welcome!").as_deref(),
            Ok("Welcome!")
        );
    }

    #[test]
    fn entered_lists_only_new_regions() {
        let a = spawn();
        let b = Region::new("arena".into(), "overworld".into(), [0, 0, 0], [5, 100, 5]);
        let names: Vec<_> = entered(&[&a], &[&a, &b])
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(names, ["arena"]);
    }

    #[test]
    fn region_file_round_trips() {
        let set = set_with(spawn());
        let text = toml::to_string_pretty(&set).unwrap();
        let back: RegionSet = toml::from_str(&text).unwrap();
        assert_eq!(back.regions.len(), 1);
        assert_eq!(back.regions[0].owners[0].uuid, ALICE);
    }
}
