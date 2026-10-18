//! The `/rg` (alias `/region`) command.

use crate::region::{Flag, GLOBAL_REGION, Member, Region, normalize_value};
use crate::{PREFIX, Shared};
use pumpkin::command::argument_builder::{ArgumentBuilder, argument, command};
use pumpkin::command::argument_types::core::string::StringArgumentType;
use pumpkin::command::context::command_context::CommandContext;
use pumpkin::command::context::command_source::CommandSource;
use pumpkin::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin::plugin::Context;

pub const PERM_COMMAND: &str = "pexguard:command.rg";
const USAGE: &str = "Usage: /rg <pos1|pos2|define|redefine|remove|list|info|flag|flags|priority|addowner|removeowner|addmember|removemember> ...";
const MAX_NAME_LEN: usize = 32;
const MAX_MESSAGE_LEN: usize = 256;

pub fn register(context: &Context, shared: &Shared) {
    for name in ["rg", "region"] {
        let node = command(name, "Manages protected regions")
            .executes(RgCommand(shared.clone()))
            .then(
                argument("args", StringArgumentType::GreedyPhrase)
                    .executes(RgCommand(shared.clone())),
            );
        context.register_command(node, PERM_COMMAND);
    }
}

struct RgCommand(Shared);

impl CommandExecutor for RgCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let source = &context.source;
        let _runtime = source.server().runtime.enter();
        let args = StringArgumentType::get(context, "args").unwrap_or_default();
        match run(&self.0, source, args) {
            Ok(message) => {
                source.send_message(PREFIX.info(message));
                Ok(1)
            }
            Err(message) => {
                source.send_message(PREFIX.error(message));
                Ok(0)
            }
        }
    }
}

#[expect(clippy::too_many_lines, reason = "one match arm per /rg subcommand")]
fn run(shared: &Shared, source: &CommandSource, args: &str) -> Result<String, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let Some((sub, rest)) = parts.split_first() else {
        return Err(USAGE.to_owned());
    };
    let world = source.world().get_world_name().to_owned();
    match sub.to_ascii_lowercase().as_str() {
        "pos1" | "pos2" => {
            let pos = position(source, rest)?;
            let mut selections = shared.selections();
            let entry = selections.entry(source.name.clone()).or_default();
            if *sub == "pos1" {
                entry.0 = Some(pos);
            } else {
                entry.1 = Some(pos);
            }
            Ok(format!("{sub} set to {} {} {}.", pos[0], pos[1], pos[2]))
        }
        "define" | "redefine" => {
            let name = rest
                .first()
                .ok_or("Usage: /rg define <name> [x1 y1 z1 x2 y2 z2]")?;
            validate_name(name)?;
            let (a, b) = if rest.len() == 7 {
                (coords(&rest[1..4])?, coords(&rest[4..7])?)
            } else {
                let selections = shared.selections();
                let selection = selections.get(&source.name);
                match selection {
                    Some((Some(a), Some(b))) => (*a, *b),
                    _ => return Err("Set /rg pos1 and /rg pos2 first.".to_owned()),
                }
            };
            let mut regions = shared.regions();
            if *sub == "define" {
                if regions.get(name).is_some() {
                    return Err(format!("Region {name} already exists."));
                }
                let mut region = Region::new((*name).to_owned(), world, a, b);
                if let Some(player) = source.as_player() {
                    region.owners.push(Member {
                        uuid: player.gameprofile.id.to_string(),
                        name: player.gameprofile.name.clone(),
                    });
                }
                let volume = region.volume();
                regions.regions.push(region);
                shared.save(&regions)?;
                Ok(format!("Defined {name} ({volume} blocks)."))
            } else {
                let region = regions
                    .get_mut(name)
                    .ok_or(format!("No region named {name}."))?;
                let resized = Region::new(region.name.clone(), world, a, b);
                region.world = resized.world;
                region.min = resized.min;
                region.max = resized.max;
                shared.save(&regions)?;
                Ok(format!("Redefined {name}."))
            }
        }
        "remove" => {
            let name = rest.first().ok_or("Usage: /rg remove <name>")?;
            let mut regions = shared.regions();
            let before = regions.regions.len();
            regions
                .regions
                .retain(|region| !region.name.eq_ignore_ascii_case(name));
            if regions.regions.len() == before {
                return Err(format!("No region named {name}."));
            }
            shared.save(&regions)?;
            Ok(format!("Removed {name}."))
        }
        "list" => {
            let regions = shared.regions();
            let names: Vec<_> = regions
                .regions
                .iter()
                .filter(|region| region.world == world)
                .map(|region| region.name.as_str())
                .collect();
            Ok(if names.is_empty() {
                "No regions in this world.".to_owned()
            } else {
                format!("Regions: {}", names.join(", "))
            })
        }
        "info" => {
            let regions = shared.regions();
            let region = if let Some(name) = rest.first() {
                regions
                    .get(name)
                    .ok_or(format!("No region named {name}."))?
            } else {
                let pos = position(source, &[])?;
                *regions
                    .at(&world, pos)
                    .first()
                    .ok_or("You're not in a region.")?
            };
            let names = |members: &[Member]| {
                members
                    .iter()
                    .map(|member| member.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let flags: Vec<_> = region
                .flags
                .iter()
                .map(|(flag, value)| format!("{flag}={value}"))
                .collect();
            Ok(format!(
                "{} in {}: {:?} to {:?}, priority {}. Owners: [{}] Members: [{}] Flags: [{}]",
                region.name,
                region.world,
                region.min,
                region.max,
                region.priority,
                names(&region.owners),
                names(&region.members),
                flags.join(", ")
            ))
        }
        "flags" => Ok(format!("Flags: {}", Flag::ALL.map(Flag::name).join(", "))),
        "flag" => {
            let (name, flag_name) = match rest {
                [name, flag, ..] => (*name, *flag),
                _ => return Err("Usage: /rg flag <region|__global__> <flag> [value]".to_owned()),
            };
            let flag = Flag::parse(flag_name)
                .ok_or(format!("Unknown flag {flag_name}. See /rg flags."))?;
            let value = rest[2..].join(" ");
            if value.len() > MAX_MESSAGE_LEN {
                return Err(format!(
                    "Values are limited to {MAX_MESSAGE_LEN} characters."
                ));
            }
            let value = (!value.is_empty())
                .then(|| normalize_value(flag, &value))
                .transpose()?;
            let mut regions = shared.regions();
            let flags = if name.eq_ignore_ascii_case(GLOBAL_REGION) {
                regions.globals.entry(world).or_default()
            } else {
                &mut regions
                    .get_mut(name)
                    .ok_or(format!("No region named {name}."))?
                    .flags
            };
            let message = if let Some(value) = value {
                let message = format!("Set {} to {value} in {name}.", flag.name());
                flags.insert(flag.name().to_owned(), value);
                message
            } else {
                flags.remove(flag.name());
                format!("Cleared {} in {name}.", flag.name())
            };
            shared.save(&regions)?;
            Ok(message)
        }
        "priority" => {
            let (name, priority) = match rest {
                [name, priority] => (
                    *name,
                    priority
                        .parse::<i32>()
                        .map_err(|_| "Priority must be a whole number.")?,
                ),
                _ => return Err("Usage: /rg priority <name> <number>".to_owned()),
            };
            let mut regions = shared.regions();
            regions
                .get_mut(name)
                .ok_or(format!("No region named {name}."))?
                .priority = priority;
            shared.save(&regions)?;
            Ok(format!("Priority of {name} is now {priority}."))
        }
        sub @ ("addowner" | "removeowner" | "addmember" | "removemember") => {
            let (name, player_name) = match rest {
                [name, player] => (*name, *player),
                _ => return Err(format!("Usage: /rg {sub} <region> <player>")),
            };
            let mut regions = shared.regions();
            let region = regions
                .get_mut(name)
                .ok_or(format!("No region named {name}."))?;
            let list = if sub.ends_with("owner") {
                &mut region.owners
            } else {
                &mut region.members
            };
            if sub.starts_with("add") {
                let player = source
                    .server()
                    .get_player_by_name(player_name)
                    .ok_or(format!("{player_name} must be online to be added."))?;
                let uuid = player.gameprofile.id.to_string();
                if !list.iter().any(|member| member.uuid == uuid) {
                    list.push(Member {
                        uuid,
                        name: player.gameprofile.name.clone(),
                    });
                }
            } else {
                list.retain(|member| !member.name.eq_ignore_ascii_case(player_name));
            }
            shared.save(&regions)?;
            Ok(format!("Updated {sub} for {name}."))
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !name.eq_ignore_ascii_case(GLOBAL_REGION);
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Region names use letters, digits, - and _, up to {MAX_NAME_LEN} characters."
        ))
    }
}

/// The given `x y z`, or the source's block position.
fn position(source: &CommandSource, rest: &[&str]) -> Result<[i32; 3], String> {
    match rest.len() {
        0 => {
            let pos = source.position;
            Ok([
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ])
        }
        3 => coords(rest),
        _ => Err("Give a position as x y z, or none to use yours.".to_owned()),
    }
}

fn coords(parts: &[&str]) -> Result<[i32; 3], String> {
    let mut out = [0; 3];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part
            .parse()
            .map_err(|_| format!("`{part}` is not a whole number."))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_names_are_validated() {
        assert!(validate_name("spawn_1").is_ok());
        assert!(validate_name("__global__").is_err());
        assert!(validate_name("bad name").is_err());
        assert!(validate_name(&"x".repeat(33)).is_err());
    }

    #[test]
    fn coordinates_must_be_integers() {
        assert_eq!(coords(&["1", "-2", "3"]), Ok([1, -2, 3]));
        assert!(coords(&["1", "two", "3"]).is_err());
    }
}
