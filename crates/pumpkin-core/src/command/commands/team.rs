use crate::command::argument_builder::{
    ArgumentBuilder, LiteralArgumentBuilder, argument, command, literal,
};
use crate::command::argument_types::core::bool::BoolArgumentType;
use crate::command::argument_types::core::string::StringArgumentType;
use crate::command::argument_types::score_holder::{ResolvedScoreHolder, ScoreHolderArgumentType};
use crate::command::argument_types::team::TeamArgumentType;
use crate::command::argument_types::team_color::TeamColorArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::world::scoreboard::{CollisionRule, NameTagVisibility, Team};
use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::NamedColor;

const DESCRIPTION: &str = "Manages teams.";
const PERMISSION: &str = "minecraft:command.team";

const ARG_TEAM_NAME: &str = "team";
const ARG_DISPLAY_NAME: &str = "displayName";
const ARG_TEAM: &str = "team";
const ARG_MEMBERS: &str = "members";
const ARG_VALUE: &str = "value";
const ARG_PREFIX: &str = "prefix";
const ARG_SUFFIX: &str = "suffix";
const ARG_ALLOWED: &str = "allowed";

const DUPLICATE_TEAM_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_ADD_DUPLICATE,
    translation::java::COMMANDS_TEAM_ADD_DUPLICATE,
);

const TEAM_NOT_FOUND_ERROR: CommandErrorType<1> = CommandErrorType::new(
    translation::java::TEAM_NOTFOUND,
    translation::java::TEAM_NOTFOUND,
);

const EMPTY_UNCHANGED_ERROR: CommandErrorType<1> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_EMPTY_UNCHANGED,
    translation::java::COMMANDS_TEAM_EMPTY_UNCHANGED,
);

const COLOR_UNCHANGED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_COLOR_UNCHANGED,
    translation::java::COMMANDS_TEAM_OPTION_COLOR_UNCHANGED,
);

const NAME_UNCHANGED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_NAME_UNCHANGED,
    translation::java::COMMANDS_TEAM_OPTION_NAME_UNCHANGED,
);

const NAMETAG_VISIBILITY_UNCHANGED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_NAMETAGVISIBILITY_UNCHANGED,
    translation::java::COMMANDS_TEAM_OPTION_NAMETAGVISIBILITY_UNCHANGED,
);

const COLLISION_RULE_UNCHANGED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_COLLISIONRULE_UNCHANGED,
    translation::java::COMMANDS_TEAM_OPTION_COLLISIONRULE_UNCHANGED,
);

const FRIENDLY_FIRE_ALREADY_ENABLED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADYENABLED,
    translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADYENABLED,
);

const FRIENDLY_FIRE_ALREADY_DISABLED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADYDISABLED,
    translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADYDISABLED,
);

const SEE_FRIENDLY_INVISIBLES_ALREADY_ENABLED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_ALREADYENABLED,
    translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_ALREADYENABLED,
);

const SEE_FRIENDLY_INVISIBLES_ALREADY_DISABLED_ERROR: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_ALREADYDISABLED,
    translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_ALREADYDISABLED,
);

struct TeamAddExecutor {
    has_display_name: bool,
}

impl CommandExecutor for TeamAddExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = StringArgumentType::get(context, ARG_TEAM_NAME)?;
        let display_name = if self.has_display_name {
            TextComponent::text(StringArgumentType::get(context, ARG_DISPLAY_NAME)?.to_string())
        } else {
            TextComponent::text(team_name.to_string())
        };

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();
        let display_name_clone = display_name;

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if scoreboard.get_teams().contains_key(&team_name_owned) {
            return Err(DUPLICATE_TEAM_ERROR.create_without_context());
        }

        let new_team = Team {
            name: team_name_owned,
            display_name: display_name_clone,
            options: 0,
            nametag_visibility: NameTagVisibility::Always,
            collision_rule: CollisionRule::Always,
            color: NamedColor::White,
            player_prefix: TextComponent::empty(),
            player_suffix: TextComponent::empty(),
            players: vec![],
        };

        let formatted_name = new_team.formatted_display_name();
        scoreboard.add_team(&world, new_team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_ADD_SUCCESS,
                translation::java::COMMANDS_TEAM_ADD_SUCCESS,
                [formatted_name],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamRemoveExecutor;

impl CommandExecutor for TeamRemoveExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let team_display_name = team.formatted_display_name();

        scoreboard.remove_team(&world, &team_name_owned);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_REMOVE_SUCCESS,
                translation::java::COMMANDS_TEAM_REMOVE_SUCCESS,
                [team_display_name],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamEmptyExecutor;

impl CommandExecutor for TeamEmptyExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let team_display_name = team.formatted_display_name();
        let players_to_remove = team.players.clone();

        if players_to_remove.is_empty() {
            return Err(EMPTY_UNCHANGED_ERROR.create_without_context(team_display_name));
        }

        for player in &players_to_remove {
            scoreboard.remove_player_from_team(&world, &team_name_owned, player);
        }

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_EMPTY_SUCCESS,
                translation::java::COMMANDS_TEAM_EMPTY_SUCCESS,
                [
                    TextComponent::text(players_to_remove.len().to_string()),
                    team_display_name,
                ],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamJoinExecutor {
    has_members: bool,
}

impl CommandExecutor for TeamJoinExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?.to_string();

        // Vanilla `TeamCommand.joinTeam`: the members are score holders (names, selectors or `*`),
        // and without members the source itself joins, which needs an entity.
        let members = if self.has_members {
            ScoreHolderArgumentType::get_score_holders(context, ARG_MEMBERS)?
        } else {
            vec![ResolvedScoreHolder::entity(
                context.source.entity_or_err()?.as_ref(),
            )]
        };

        let world = context.world().clone();
        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name) else {
            return Err(TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name)));
        };
        let team_display_name = team.formatted_display_name();

        for member in &members {
            scoreboard.add_player_to_team(&world, &team_name, member.name.clone());
        }

        // Every member counts: vanilla `addPlayerToTeam` answers true even for a member already
        // on the team.
        let msg = if let [member] = members.as_slice() {
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_JOIN_SUCCESS_SINGLE,
                translation::java::COMMANDS_TEAM_JOIN_SUCCESS_SINGLE,
                [member.display_name.clone(), team_display_name],
            )
        } else {
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_JOIN_SUCCESS_MULTIPLE,
                translation::java::COMMANDS_TEAM_JOIN_SUCCESS_MULTIPLE,
                [
                    TextComponent::text(members.len().to_string()),
                    team_display_name,
                ],
            )
        };

        context.source.send_feedback(msg, true);

        Ok(members.len() as i32)
    }
}

struct TeamLeaveExecutor;

impl CommandExecutor for TeamLeaveExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let members = ScoreHolderArgumentType::get_score_holders(context, ARG_MEMBERS)?;
        let world = context.world().clone();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Vanilla `TeamCommand.leaveTeam` counts only the members that were on a team.
        let mut removed = Vec::new();
        for member in &members {
            let found_team = scoreboard
                .get_teams()
                .values()
                .find(|team| team.players.contains(&member.name))
                .map(|team| team.name.clone());
            if let Some(team_name) = found_team {
                scoreboard.remove_player_from_team(&world, &team_name, &member.name);
                removed.push(member);
            }
        }

        let msg = if let [member] = removed.as_slice() {
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_LEAVE_SUCCESS_SINGLE,
                translation::java::COMMANDS_TEAM_LEAVE_SUCCESS_SINGLE,
                [member.display_name.clone()],
            )
        } else {
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_LEAVE_SUCCESS_MULTIPLE,
                translation::java::COMMANDS_TEAM_LEAVE_SUCCESS_MULTIPLE,
                [TextComponent::text(removed.len().to_string())],
            )
        };

        context.source.send_feedback(msg, true);

        Ok(removed.len() as i32)
    }
}

struct TeamListExecutor {
    has_team: bool,
}

impl CommandExecutor for TeamListExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context.world().clone();
        let has_team = self.has_team;
        let team_name = if has_team {
            Some(TeamArgumentType::get(context, ARG_TEAM)?.to_string())
        } else {
            None
        };

        let scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(team_name) = team_name {
            let Some(team) = scoreboard.get_teams().get(&team_name) else {
                return Err(
                    TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name))
                );
            };

            if team.players.is_empty() {
                context.source.send_feedback(
                    TextComponent::translate_cross(
                        translation::java::COMMANDS_TEAM_LIST_MEMBERS_EMPTY,
                        translation::java::COMMANDS_TEAM_LIST_MEMBERS_EMPTY,
                        [team.formatted_display_name()],
                    ),
                    false,
                );
            } else {
                let mut list_comp = TextComponent::empty();
                for (i, player) in team.players.iter().enumerate() {
                    if i > 0 {
                        list_comp = list_comp.add_child(TextComponent::text(", "));
                    }
                    list_comp = list_comp.add_child(TextComponent::text(player.clone()));
                }

                context.source.send_feedback(
                    TextComponent::translate_cross(
                        translation::java::COMMANDS_TEAM_LIST_MEMBERS_SUCCESS,
                        translation::java::COMMANDS_TEAM_LIST_MEMBERS_SUCCESS,
                        [
                            team.formatted_display_name(),
                            TextComponent::text(team.players.len().to_string()),
                            list_comp,
                        ],
                    ),
                    false,
                );
            }
        } else {
            let teams = scoreboard.get_teams();
            if teams.is_empty() {
                context.source.send_feedback(
                    TextComponent::translate_cross(
                        translation::java::COMMANDS_TEAM_LIST_TEAMS_EMPTY,
                        translation::java::COMMANDS_TEAM_LIST_TEAMS_EMPTY,
                        [],
                    ),
                    false,
                );
            } else {
                let mut list_comp = TextComponent::empty();
                for (i, team) in teams.values().enumerate() {
                    if i > 0 {
                        list_comp = list_comp.add_child(TextComponent::text(", "));
                    }
                    list_comp = list_comp.add_child(team.formatted_display_name());
                }

                context.source.send_feedback(
                    TextComponent::translate_cross(
                        translation::java::COMMANDS_TEAM_LIST_TEAMS_SUCCESS,
                        translation::java::COMMANDS_TEAM_LIST_TEAMS_SUCCESS,
                        [TextComponent::text(teams.len().to_string()), list_comp],
                    ),
                    false,
                );
            }
        }

        Ok(1)
    }
}

struct TeamModifyColorResetExecutor;

impl CommandExecutor for TeamModifyColorResetExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        if team.color == NamedColor::White {
            return Err(COLOR_UNCHANGED_ERROR.create_without_context());
        }

        team.color = NamedColor::White;
        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_COLOR_CLEAR_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_COLOR_CLEAR_SUCCESS,
                [team_display_name],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyColorExecutor;

impl CommandExecutor for TeamModifyColorExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let new_color = TeamColorArgumentType::get(context, ARG_VALUE)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        if team.color == new_color {
            return Err(COLOR_UNCHANGED_ERROR.create_without_context());
        }

        team.color = new_color;
        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_COLOR_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_COLOR_SUCCESS,
                [team_display_name, TextComponent::text(new_color.name())],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyDisplayNameExecutor;

impl CommandExecutor for TeamModifyDisplayNameExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let new_name_str = StringArgumentType::get(context, ARG_DISPLAY_NAME)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();
        let new_name_owned = new_name_str.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let new_display_name = TextComponent::text(new_name_owned);
        if team.display_name == new_display_name {
            return Err(NAME_UNCHANGED_ERROR.create_without_context());
        }

        team.display_name = new_display_name.clone();
        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_NAME_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_NAME_SUCCESS,
                [team_display_name, new_display_name],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyPrefixExecutor;

impl CommandExecutor for TeamModifyPrefixExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let new_prefix_str = StringArgumentType::get(context, ARG_PREFIX)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();
        let new_prefix_owned = new_prefix_str.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let new_prefix = TextComponent::text(new_prefix_owned);
        team.player_prefix = new_prefix.clone();
        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_PREFIX_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_PREFIX_SUCCESS,
                [team_display_name, new_prefix],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifySuffixExecutor;

impl CommandExecutor for TeamModifySuffixExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let new_suffix_str = StringArgumentType::get(context, ARG_SUFFIX)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();
        let new_suffix_owned = new_suffix_str.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let new_suffix = TextComponent::text(new_suffix_owned);
        team.player_suffix = new_suffix.clone();
        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_SUFFIX_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_SUFFIX_SUCCESS,
                [team_display_name, new_suffix],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyFriendlyFireExecutor;

impl CommandExecutor for TeamModifyFriendlyFireExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let value = BoolArgumentType::get(context, ARG_ALLOWED)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let is_enabled = (team.options & 0x01) != 0;
        if value == is_enabled {
            if value {
                return Err(FRIENDLY_FIRE_ALREADY_ENABLED_ERROR.create_without_context());
            }
            return Err(FRIENDLY_FIRE_ALREADY_DISABLED_ERROR.create_without_context());
        }

        if value {
            team.options |= 0x01;
        } else {
            team.options &= !0x01;
        }

        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        let key = if value {
            translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ENABLED
        } else {
            translation::java::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_DISABLED
        };

        context.source.send_feedback(
            TextComponent::translate_cross(key, key, [team_display_name]),
            true,
        );

        Ok(1)
    }
}

struct TeamModifySeeFriendlyInvisiblesExecutor;

impl CommandExecutor for TeamModifySeeFriendlyInvisiblesExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let value = BoolArgumentType::get(context, ARG_ALLOWED)?;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let is_enabled = (team.options & 0x02) != 0;
        if value == is_enabled {
            if value {
                return Err(SEE_FRIENDLY_INVISIBLES_ALREADY_ENABLED_ERROR.create_without_context());
            }
            return Err(SEE_FRIENDLY_INVISIBLES_ALREADY_DISABLED_ERROR.create_without_context());
        }

        if value {
            team.options |= 0x02;
        } else {
            team.options &= !0x02;
        }

        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        let key = if value {
            translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_ENABLED
        } else {
            translation::java::COMMANDS_TEAM_OPTION_SEEFRIENDLYINVISIBLES_DISABLED
        };

        context.source.send_feedback(
            TextComponent::translate_cross(key, key, [team_display_name]),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyNametagVisibilityExecutor {
    value: NameTagVisibility,
}

impl CommandExecutor for TeamModifyNametagVisibilityExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let value = self.value;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let is_unchanged = matches!(
            (&team.nametag_visibility, &value),
            (NameTagVisibility::Always, NameTagVisibility::Always)
                | (NameTagVisibility::Never, NameTagVisibility::Never)
                | (
                    NameTagVisibility::HideForOtherTeams,
                    NameTagVisibility::HideForOtherTeams
                )
                | (
                    NameTagVisibility::HideForOwnTeam,
                    NameTagVisibility::HideForOwnTeam
                )
        );

        if is_unchanged {
            return Err(NAMETAG_VISIBILITY_UNCHANGED_ERROR.create_without_context());
        }

        team.nametag_visibility = match value {
            NameTagVisibility::Always => NameTagVisibility::Always,
            NameTagVisibility::Never => NameTagVisibility::Never,
            NameTagVisibility::HideForOtherTeams => NameTagVisibility::HideForOtherTeams,
            NameTagVisibility::HideForOwnTeam => NameTagVisibility::HideForOwnTeam,
        };

        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_NAMETAGVISIBILITY_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_NAMETAGVISIBILITY_SUCCESS,
                [
                    team_display_name,
                    TextComponent::text(value.to_str().to_string()),
                ],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyDeathMessageVisibilityExecutor {
    value: &'static str,
}

impl CommandExecutor for TeamModifyDeathMessageVisibilityExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let world = context.world().clone();
        let team_name_owned = team_name.to_string();
        let value = self.value;

        let scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let team_display_name = team.formatted_display_name();

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_DEATHMESSAGEVISIBILITY_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_DEATHMESSAGEVISIBILITY_SUCCESS,
                [team_display_name, TextComponent::text(value.to_string())],
            ),
            true,
        );

        Ok(1)
    }
}

struct TeamModifyCollisionRuleExecutor {
    value: CollisionRule,
}

impl CommandExecutor for TeamModifyCollisionRuleExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let team_name = TeamArgumentType::get(context, ARG_TEAM)?;
        let value = self.value;

        let world = context.world().clone();
        let team_name_owned = team_name.to_string();

        let mut scoreboard = world
            .scoreboard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(team) = scoreboard.get_teams().get(&team_name_owned) else {
            return Err(
                TEAM_NOT_FOUND_ERROR.create_without_context(TextComponent::text(team_name_owned))
            );
        };

        let mut team = team.clone();
        let is_unchanged = matches!(
            (&team.collision_rule, &value),
            (CollisionRule::Always, CollisionRule::Always)
                | (CollisionRule::Never, CollisionRule::Never)
                | (CollisionRule::PushOtherTeams, CollisionRule::PushOtherTeams)
                | (CollisionRule::PushOwnTeam, CollisionRule::PushOwnTeam)
        );

        if is_unchanged {
            return Err(COLLISION_RULE_UNCHANGED_ERROR.create_without_context());
        }

        team.collision_rule = match value {
            CollisionRule::Always => CollisionRule::Always,
            CollisionRule::Never => CollisionRule::Never,
            CollisionRule::PushOtherTeams => CollisionRule::PushOtherTeams,
            CollisionRule::PushOwnTeam => CollisionRule::PushOwnTeam,
        };

        let team_display_name = team.formatted_display_name();
        scoreboard.update_team(&world, team);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TEAM_OPTION_COLLISIONRULE_SUCCESS,
                translation::java::COMMANDS_TEAM_OPTION_COLLISIONRULE_SUCCESS,
                [
                    team_display_name,
                    TextComponent::text(value.to_str().to_string()),
                ],
            ),
            true,
        );

        Ok(1)
    }
}

fn add_branch() -> LiteralArgumentBuilder {
    literal("add").then(
        argument(ARG_TEAM_NAME, StringArgumentType::SingleWord)
            .executes(TeamAddExecutor {
                has_display_name: false,
            })
            .then(
                argument(ARG_DISPLAY_NAME, StringArgumentType::GreedyPhrase).executes(
                    TeamAddExecutor {
                        has_display_name: true,
                    },
                ),
            ),
    )
}

fn remove_branch() -> LiteralArgumentBuilder {
    literal("remove").then(argument(ARG_TEAM, TeamArgumentType).executes(TeamRemoveExecutor))
}

fn empty_branch() -> LiteralArgumentBuilder {
    literal("empty").then(argument(ARG_TEAM, TeamArgumentType).executes(TeamEmptyExecutor))
}

fn join_branch() -> LiteralArgumentBuilder {
    literal("join").then(
        argument(ARG_TEAM, TeamArgumentType)
            .executes(TeamJoinExecutor { has_members: false })
            .then(
                argument(ARG_MEMBERS, ScoreHolderArgumentType::Multiple)
                    .executes(TeamJoinExecutor { has_members: true }),
            ),
    )
}

fn leave_branch() -> LiteralArgumentBuilder {
    literal("leave")
        .then(argument(ARG_MEMBERS, ScoreHolderArgumentType::Multiple).executes(TeamLeaveExecutor))
}

fn list_branch() -> LiteralArgumentBuilder {
    literal("list")
        .executes(TeamListExecutor { has_team: false })
        .then(argument(ARG_TEAM, TeamArgumentType).executes(TeamListExecutor { has_team: true }))
}

#[expect(clippy::too_many_lines)]
fn modify_branch() -> LiteralArgumentBuilder {
    literal("modify").then(
        argument(ARG_TEAM, TeamArgumentType)
            .then(
                literal("color")
                    .then(literal("reset").executes(TeamModifyColorResetExecutor))
                    .then(
                        argument(ARG_VALUE, TeamColorArgumentType)
                            .executes(TeamModifyColorExecutor),
                    ),
            )
            .then(
                literal("displayName").then(
                    argument(ARG_DISPLAY_NAME, StringArgumentType::GreedyPhrase)
                        .executes(TeamModifyDisplayNameExecutor),
                ),
            )
            .then(
                literal("prefix").then(
                    argument(ARG_PREFIX, StringArgumentType::GreedyPhrase)
                        .executes(TeamModifyPrefixExecutor),
                ),
            )
            .then(
                literal("suffix").then(
                    argument(ARG_SUFFIX, StringArgumentType::GreedyPhrase)
                        .executes(TeamModifySuffixExecutor),
                ),
            )
            .then(literal("friendlyFire").then(
                argument(ARG_ALLOWED, BoolArgumentType).executes(TeamModifyFriendlyFireExecutor),
            ))
            .then(
                literal("seeFriendlyInvisibles").then(
                    argument(ARG_ALLOWED, BoolArgumentType)
                        .executes(TeamModifySeeFriendlyInvisiblesExecutor),
                ),
            )
            .then(
                literal("nametagVisibility")
                    .then(
                        literal("always").executes(TeamModifyNametagVisibilityExecutor {
                            value: NameTagVisibility::Always,
                        }),
                    )
                    .then(
                        literal("never").executes(TeamModifyNametagVisibilityExecutor {
                            value: NameTagVisibility::Never,
                        }),
                    )
                    .then(literal("hideForOtherTeams").executes(
                        TeamModifyNametagVisibilityExecutor {
                            value: NameTagVisibility::HideForOtherTeams,
                        },
                    ))
                    .then(literal("hideForOwnTeam").executes(
                        TeamModifyNametagVisibilityExecutor {
                            value: NameTagVisibility::HideForOwnTeam,
                        },
                    )),
            )
            .then(
                literal("deathMessageVisibility")
                    .then(
                        literal("always")
                            .executes(TeamModifyDeathMessageVisibilityExecutor { value: "always" }),
                    )
                    .then(
                        literal("never")
                            .executes(TeamModifyDeathMessageVisibilityExecutor { value: "never" }),
                    )
                    .then(literal("hideForOtherTeams").executes(
                        TeamModifyDeathMessageVisibilityExecutor {
                            value: "hideForOtherTeams",
                        },
                    ))
                    .then(literal("hideForOwnTeam").executes(
                        TeamModifyDeathMessageVisibilityExecutor {
                            value: "hideForOwnTeam",
                        },
                    )),
            )
            .then(
                literal("collisionRule")
                    .then(literal("always").executes(TeamModifyCollisionRuleExecutor {
                        value: CollisionRule::Always,
                    }))
                    .then(literal("never").executes(TeamModifyCollisionRuleExecutor {
                        value: CollisionRule::Never,
                    }))
                    .then(
                        literal("pushOtherTeams").executes(TeamModifyCollisionRuleExecutor {
                            value: CollisionRule::PushOtherTeams,
                        }),
                    )
                    .then(
                        literal("pushOwnTeam").executes(TeamModifyCollisionRuleExecutor {
                            value: CollisionRule::PushOwnTeam,
                        }),
                    ),
            ),
    )
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("team", DESCRIPTION)
            .requires(PERMISSION)
            .then(add_branch())
            .then(remove_branch())
            .then(empty_branch())
            .then(join_branch())
            .then(leave_branch())
            .then(list_branch())
            .then(modify_branch()),
    );
}
