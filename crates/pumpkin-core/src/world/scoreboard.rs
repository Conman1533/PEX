use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Weak};

use rustc_hash::FxHashMap;

use pumpkin_data::scoreboard::ScoreboardDisplaySlot;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::{
    BClientPacket, ClientPacket, NumberFormat,
    bedrock::client::{
        remove_objective::CRemoveObjective as BRemoveObjective,
        set_display_objective::CSetDisplayObjective as BSetDisplayObjective,
        set_score::{CSetScore as BSetScore, ScoreEntry as BScoreEntry},
    },
    codec::var_int::VarInt,
    java::client::play::{
        CDisplayObjective, CResetScore, CSetPlayerTeam, CUpdateObjectives, CUpdateScore, Mode,
        RenderType, TeamMethod, TeamParameters,
    },
};
use pumpkin_util::text::{TextComponent, color::NamedColor};
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::world_info::{
    WorldInfoError,
    data_files::{read_data_file, write_data_file},
};
use tracing::warn;

use super::World;
use crate::entity::player::{DATA_VERSION, Player};

pub trait ScoreboardTarget: Send + Sync {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    );
    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J);
}

impl ScoreboardTarget for World {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    ) {
        self.broadcast_editioned(je_packet, be_packet);
    }

    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J) {
        self.broadcast_packet_all(je_packet);
    }
}

impl<T: ScoreboardTarget + ?Sized> ScoreboardTarget for &T {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    ) {
        (*self).send_editioned(je_packet, be_packet);
    }

    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J) {
        (*self).send_je(je_packet);
    }
}

impl ScoreboardTarget for std::sync::Arc<World> {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    ) {
        self.broadcast_editioned(je_packet, be_packet);
    }

    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J) {
        self.broadcast_packet_all(je_packet);
    }
}

impl ScoreboardTarget for Player {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    ) {
        self.try_enqueue_packet_editioned(je_packet, be_packet);
    }

    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J) {
        self.try_send_client_packet(je_packet);
    }
}

impl ScoreboardTarget for std::sync::Arc<Player> {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        je_packet: &J,
        be_packet: &B,
    ) {
        self.try_enqueue_packet_editioned(je_packet, be_packet);
    }

    fn send_je<J: ClientPacket + Sync>(&self, je_packet: &J) {
        self.try_send_client_packet(je_packet);
    }
}

pub struct NoTarget;

impl ScoreboardTarget for NoTarget {
    fn send_editioned<J: ClientPacket + Sync, B: BClientPacket + Sync>(
        &self,
        _je_packet: &J,
        _be_packet: &B,
    ) {
    }

    fn send_je<J: ClientPacket + Sync>(&self, _je_packet: &J) {}
}

#[derive(Clone, Debug, Default)]
pub struct Scoreboard {
    objectives: FxHashMap<String, ScoreboardObjective>,
    display_slots: FxHashMap<ScoreboardDisplaySlot, String>,
    scores: FxHashMap<String, FxHashMap<String, ScoreboardScore>>,
    teams: FxHashMap<String, Team>,
}

impl Scoreboard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn get_objectives(&self) -> &FxHashMap<String, ScoreboardObjective> {
        &self.objectives
    }

    #[must_use]
    pub fn get_objective(&self, name: &str) -> Option<&ScoreboardObjective> {
        self.objectives.get(name)
    }

    pub fn get_objective_mut(&mut self, name: &str) -> Option<&mut ScoreboardObjective> {
        self.objectives.get_mut(name)
    }

    #[must_use]
    pub const fn get_display_slots(&self) -> &FxHashMap<ScoreboardDisplaySlot, String> {
        &self.display_slots
    }

    #[must_use]
    pub fn get_display_objective(&self, slot: ScoreboardDisplaySlot) -> Option<&str> {
        self.display_slots.get(&slot).map(String::as_str)
    }

    #[must_use]
    pub const fn get_scores(&self) -> &FxHashMap<String, FxHashMap<String, ScoreboardScore>> {
        &self.scores
    }

    #[must_use]
    pub fn get_score(&self, entity_name: &str, objective_name: &str) -> Option<&ScoreboardScore> {
        self.scores.get(objective_name)?.get(entity_name)
    }

    #[must_use]
    pub fn get_score_value(&self, entity_name: &str, objective_name: &str) -> Option<i32> {
        self.get_score(entity_name, objective_name)
            .map(|s| s.value.0)
    }

    #[must_use]
    pub fn get_scores_for_objective(
        &self,
        objective_name: &str,
    ) -> Option<&FxHashMap<String, ScoreboardScore>> {
        self.scores.get(objective_name)
    }

    #[must_use]
    pub fn get_scores_for_entity(&self, entity_name: &str) -> FxHashMap<String, &ScoreboardScore> {
        let mut entity_scores = FxHashMap::default();
        for (obj_name, obj_scores) in &self.scores {
            if let Some(score) = obj_scores.get(entity_name) {
                entity_scores.insert(obj_name.clone(), score);
            }
        }
        entity_scores
    }

    #[must_use]
    pub const fn get_teams(&self) -> &FxHashMap<String, Team> {
        &self.teams
    }

    #[must_use]
    pub fn get_team(&self, name: &str) -> Option<&Team> {
        self.teams.get(name)
    }

    pub fn get_team_mut(&mut self, name: &str) -> Option<&mut Team> {
        self.teams.get_mut(name)
    }

    #[must_use]
    pub fn get_entity_team(&self, entity_name: &str) -> Option<&Team> {
        self.teams
            .values()
            .find(|team| team.players.iter().any(|p| p == entity_name))
    }

    pub fn add_objective(
        &mut self,
        target: &impl ScoreboardTarget,
        objective: ScoreboardObjective,
    ) {
        if self.objectives.contains_key(&objective.name) {
            warn!(
                "Tried to create an objective which already exists: {}",
                &objective.name
            );
            return;
        }

        let je_update = CUpdateObjectives::new(
            objective.name.clone(),
            Mode::Add,
            objective.display_name.clone(),
            objective.render_type,
            objective.number_format.clone(),
        );

        let be_update = BSetDisplayObjective {
            display_slot_name: "sidebar".to_string(), // Default to sidebar
            objective_name: objective.name.clone(),
            objective_display_name: objective.display_name.clone().get_text(),
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(0),
        };

        target.send_editioned(&je_update, &be_update);

        self.objectives.insert(objective.name.clone(), objective);
    }

    pub fn update_objective(
        &mut self,
        target: &impl ScoreboardTarget,
        objective: ScoreboardObjective,
    ) {
        if !self.objectives.contains_key(&objective.name) {
            warn!(
                "Tried to update an objective which does not exist: {}",
                &objective.name
            );
            return;
        }

        let je_update = CUpdateObjectives::new(
            objective.name.clone(),
            Mode::Update,
            objective.display_name.clone(),
            objective.render_type,
            objective.number_format.clone(),
        );

        let be_update = BSetDisplayObjective {
            display_slot_name: "sidebar".to_string(),
            objective_name: objective.name.clone(),
            objective_display_name: objective.display_name.clone().get_text(),
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(0),
        };

        target.send_editioned(&je_update, &be_update);

        self.objectives.insert(objective.name.clone(), objective);
    }

    pub fn set_display_objective(
        &mut self,
        target: &impl ScoreboardTarget,
        slot: ScoreboardDisplaySlot,
        objective_name: Option<&str>,
    ) {
        let slot_str = match slot {
            ScoreboardDisplaySlot::List => "list",
            ScoreboardDisplaySlot::BelowName => "belowname",
            _ => "sidebar",
        };

        let obj_name_str = objective_name.unwrap_or("");

        let display_name = objective_name
            .and_then(|name| self.objectives.get(name))
            .map_or_else(
                || obj_name_str.to_string(),
                |o| o.display_name.clone().get_text(),
            );

        let je_display = CDisplayObjective::new(slot, obj_name_str.to_string());
        let be_display = BSetDisplayObjective {
            display_slot_name: slot_str.to_string(),
            objective_name: obj_name_str.to_string(),
            objective_display_name: display_name,
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(0),
        };

        target.send_editioned(&je_display, &be_display);

        if let Some(name) = objective_name {
            self.display_slots.insert(slot, name.to_string());
        } else {
            self.display_slots.remove(&slot);
        }
    }

    pub fn clear_display_objective(
        &mut self,
        target: &impl ScoreboardTarget,
        slot: ScoreboardDisplaySlot,
    ) {
        self.set_display_objective(target, slot, None);
    }

    pub fn remove_objective(&mut self, target: &impl ScoreboardTarget, name: &str) {
        if !self.objectives.contains_key(name) {
            warn!(
                "Tried to remove an objective which does not exist: {}",
                name
            );
            return;
        }

        let je_packet = CUpdateObjectives::new(
            name.to_string(),
            Mode::Remove,
            TextComponent::empty(),
            RenderType::Integer,
            None,
        );

        let be_packet = BRemoveObjective {
            objective_name: name.to_string(),
        };

        target.send_editioned(&je_packet, &be_packet);

        self.objectives.remove(name);
        self.scores.remove(name);
        self.display_slots.retain(|_, obj| obj != name);
    }

    pub fn update_score(&mut self, target: &impl ScoreboardTarget, score: ScoreboardScore) {
        if !self.objectives.contains_key(&score.objective_name) {
            warn!(
                "Tried to place a score into an objective which does not exist: {}",
                &score.objective_name
            );
            return;
        }

        let je_packet = CUpdateScore::new(
            score.entity_name.clone(),
            score.objective_name.clone(),
            score.value,
            score.display_name.clone(),
            score.number_format.clone(),
        );

        let be_packet = BSetScore {
            action: VarInt(0), // Change
            entries: vec![BScoreEntry {
                scoreboard_id: score.entity_name.as_ptr() as i64, // Internal ID
                objective_name: score.objective_name.clone(),
                score: score.value,
                entry_type: VarInt(3), // Fake player / Literal
                entity_unique_id: 0,
                custom_name: score.entity_name.clone(),
            }],
        };

        target.send_editioned(&je_packet, &be_packet);

        self.scores
            .entry(score.objective_name.clone())
            .or_default()
            .insert(score.entity_name.clone(), score);
    }

    pub fn set_score_value(
        &mut self,
        target: &impl ScoreboardTarget,
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        value: i32,
    ) {
        let entity_s = entity_name.into();
        let obj_s = objective_name.into();
        let existing = self.get_score(&entity_s, &obj_s).cloned();
        let score = ScoreboardScore {
            entity_name: entity_s,
            objective_name: obj_s,
            value: VarInt(value),
            display_name: existing.as_ref().and_then(|s| s.display_name.clone()),
            number_format: existing.as_ref().and_then(|s| s.number_format.clone()),
            locked: existing.as_ref().is_none_or(|s| s.locked),
        };
        self.update_score(target, score);
    }

    pub fn add_score(
        &mut self,
        target: &impl ScoreboardTarget,
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        delta: i32,
    ) -> i32 {
        let entity_s = entity_name.into();
        let obj_s = objective_name.into();
        let current_val = self.get_score_value(&entity_s, &obj_s).unwrap_or(0);
        let new_val = current_val.wrapping_add(delta);
        self.set_score_value(target, entity_s, obj_s, new_val);
        new_val
    }

    pub fn remove_score(
        &mut self,
        target: &impl ScoreboardTarget,
        entity_name: &str,
        objective_name: &str,
    ) {
        let je_packet = CResetScore::new(entity_name.to_string(), Some(objective_name.to_string()));

        let be_packet = BSetScore {
            action: VarInt(1), // Remove
            entries: vec![BScoreEntry {
                scoreboard_id: entity_name.as_ptr() as i64,
                objective_name: objective_name.to_string(),
                score: VarInt(0),
                entry_type: VarInt(3),
                entity_unique_id: 0,
                custom_name: entity_name.to_string(),
            }],
        };

        target.send_editioned(&je_packet, &be_packet);

        if let Some(objective_scores) = self.scores.get_mut(objective_name) {
            objective_scores.remove(entity_name);
        }
    }

    pub fn reset_scores_for_entity(&mut self, target: &impl ScoreboardTarget, entity_name: &str) {
        let je_packet = CResetScore::new(entity_name.to_string(), None);

        let mut be_entries = Vec::new();
        for (obj_name, obj_scores) in &self.scores {
            if obj_scores.contains_key(entity_name) {
                be_entries.push(BScoreEntry {
                    scoreboard_id: entity_name.as_ptr() as i64,
                    objective_name: obj_name.clone(),
                    score: VarInt(0),
                    entry_type: VarInt(3),
                    entity_unique_id: 0,
                    custom_name: entity_name.to_string(),
                });
            }
        }

        let be_packet = BSetScore {
            action: VarInt(1), // Remove
            entries: be_entries,
        };

        target.send_editioned(&je_packet, &be_packet);

        for obj_scores in self.scores.values_mut() {
            obj_scores.remove(entity_name);
        }
    }

    pub fn add_team(&mut self, target: &impl ScoreboardTarget, team: Team) {
        if self.teams.contains_key(&team.name) {
            warn!("Tried to create Team which already exists, {}", team.name);
            return;
        }

        let parameters = TeamParameters {
            display_name: &team.display_name,
            options: team.options,
            nametag_visibility: team.nametag_visibility.to_str(),
            collision_rule: team.collision_rule.to_str(),
            color: team.color as i32,
            player_prefix: &team.player_prefix,
            player_suffix: &team.player_suffix,
        };

        target.send_je(&CSetPlayerTeam {
            team_name: team.name.clone(),
            method: TeamMethod::Create,
            parameters: Some(parameters),
            players: team.players.clone().into(),
        });

        self.teams.insert(team.name.clone(), team);
    }

    pub fn create_team(&mut self, target: &impl ScoreboardTarget, team: Team) {
        self.add_team(target, team);
    }

    pub fn update_team(&mut self, target: &impl ScoreboardTarget, team: Team) {
        if !self.teams.contains_key(&team.name) {
            warn!("Tried to update Team which does not exist, {}", team.name);
            return;
        }

        let parameters = TeamParameters {
            display_name: &team.display_name,
            options: team.options,
            nametag_visibility: team.nametag_visibility.to_str(),
            collision_rule: team.collision_rule.to_str(),
            color: team.color as i32,
            player_prefix: &team.player_prefix,
            player_suffix: &team.player_suffix,
        };

        target.send_je(&CSetPlayerTeam {
            team_name: team.name.clone(),
            method: TeamMethod::Update,
            parameters: Some(parameters),
            players: Box::new([]),
        });

        self.teams.insert(team.name.clone(), team);
    }

    pub fn remove_team(&mut self, target: &impl ScoreboardTarget, name: &str) {
        if !self.teams.contains_key(name) {
            warn!("Tried to remove Team which does not exist, {}", name);
            return;
        }

        target.send_je(&CSetPlayerTeam {
            team_name: name.to_string(),
            method: TeamMethod::Remove,
            parameters: None,
            players: Box::new([]),
        });

        self.teams.remove(name);
    }

    pub fn add_player_to_team(
        &mut self,
        target: &impl ScoreboardTarget,
        team_name: &str,
        player: String,
    ) {
        let Some(team) = self.teams.get_mut(team_name) else {
            warn!(
                "Tried to add player to Team which does not exist, {}",
                team_name
            );
            return;
        };

        if team.players.contains(&player) {
            return;
        }

        target.send_je(&CSetPlayerTeam {
            team_name: team_name.to_string(),
            method: TeamMethod::AddPlayers,
            parameters: None,
            players: vec![player.clone()].into(),
        });

        team.players.push(player);
    }

    pub fn remove_player_from_team(
        &mut self,
        target: &impl ScoreboardTarget,
        team_name: &str,
        player: &str,
    ) {
        let Some(team) = self.teams.get_mut(team_name) else {
            warn!(
                "Tried to remove player from Team which does not exist, {}",
                team_name
            );
            return;
        };

        if !team.players.contains(&player.to_string()) {
            return;
        }

        target.send_je(&CSetPlayerTeam {
            team_name: team_name.to_string(),
            method: TeamMethod::RemovePlayers,
            parameters: None,
            players: vec![player.to_string()].into(),
        });

        team.players.retain(|p| p != player);
    }

    pub fn clear_team_players(&mut self, target: &impl ScoreboardTarget, team_name: &str) {
        let Some(team) = self.teams.get_mut(team_name) else {
            warn!(
                "Tried to clear players from Team which does not exist, {}",
                team_name
            );
            return;
        };

        if team.players.is_empty() {
            return;
        }

        let players_to_remove = team.players.clone();
        target.send_je(&CSetPlayerTeam {
            team_name: team_name.to_string(),
            method: TeamMethod::RemovePlayers,
            parameters: None,
            players: players_to_remove.into(),
        });

        team.players.clear();
    }

    pub fn send_to_player(&self, player: &Player) {
        for objective in self.objectives.values() {
            let je_update = CUpdateObjectives::new(
                objective.name.clone(),
                Mode::Add,
                objective.display_name.clone(),
                objective.render_type,
                objective.number_format.clone(),
            );
            let be_update = BSetDisplayObjective {
                display_slot_name: "sidebar".to_string(),
                objective_name: objective.name.clone(),
                objective_display_name: objective.display_name.clone().get_text(),
                criteria_name: "dummy".to_string(),
                sort_order: VarInt(0),
            };
            player.try_enqueue_packet_editioned(&je_update, &be_update);
        }

        for (slot, objective_name) in &self.display_slots {
            let slot_str = match slot {
                ScoreboardDisplaySlot::List => "list",
                ScoreboardDisplaySlot::BelowName => "belowname",
                _ => "sidebar",
            };
            let display_name = self.objectives.get(objective_name).map_or_else(
                || objective_name.clone(),
                |o| o.display_name.clone().get_text(),
            );
            let je_display = CDisplayObjective::new(*slot, objective_name.clone());
            let be_display = BSetDisplayObjective {
                display_slot_name: slot_str.to_string(),
                objective_name: objective_name.clone(),
                objective_display_name: display_name,
                criteria_name: "dummy".to_string(),
                sort_order: VarInt(0),
            };
            player.try_enqueue_packet_editioned(&je_display, &be_display);
        }

        for objective_scores in self.scores.values() {
            for score in objective_scores.values() {
                let je_packet = CUpdateScore::new(
                    score.entity_name.clone(),
                    score.objective_name.clone(),
                    score.value,
                    score.display_name.clone(),
                    score.number_format.clone(),
                );
                let be_packet = BSetScore {
                    action: VarInt(0),
                    entries: vec![BScoreEntry {
                        scoreboard_id: score.entity_name.as_ptr() as i64,
                        objective_name: score.objective_name.clone(),
                        score: score.value,
                        entry_type: VarInt(3),
                        entity_unique_id: 0,
                        custom_name: score.entity_name.clone(),
                    }],
                };
                player.try_enqueue_packet_editioned(&je_packet, &be_packet);
            }
        }

        for team in self.teams.values() {
            let parameters = TeamParameters {
                display_name: &team.display_name,
                options: team.options,
                nametag_visibility: team.nametag_visibility.to_str(),
                collision_rule: team.collision_rule.to_str(),
                color: team.color as i32,
                player_prefix: &team.player_prefix,
                player_suffix: &team.player_suffix,
            };
            let je_packet = CSetPlayerTeam {
                team_name: team.name.clone(),
                method: TeamMethod::Create,
                parameters: Some(parameters),
                players: team.players.clone().into(),
            };
            player.try_send_client_packet(&je_packet);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreboardObjective {
    pub name: String,
    pub display_name: TextComponent,
    pub render_type: RenderType,
    pub number_format: Option<NumberFormat>,
    pub criterion: String,
}

impl ScoreboardObjective {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        display_name: TextComponent,
        render_type: RenderType,
        number_format: Option<NumberFormat>,
        criterion: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            display_name,
            render_type,
            number_format,
            criterion: criterion.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreboardScore {
    pub entity_name: String,
    pub objective_name: String,
    pub value: VarInt,
    pub display_name: Option<TextComponent>,
    pub number_format: Option<NumberFormat>,
    pub locked: bool,
}

impl ScoreboardScore {
    #[must_use]
    pub fn new(
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        value: VarInt,
        display_name: Option<TextComponent>,
        number_format: Option<NumberFormat>,
    ) -> Self {
        Self {
            entity_name: entity_name.into(),
            objective_name: objective_name.into(),
            value,
            display_name,
            number_format,
            locked: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameTagVisibility {
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl NameTagVisibility {
    #[must_use]
    pub const fn to_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::HideForOtherTeams => "hideForOtherTeams",
            Self::HideForOwnTeam => "hideForOwnTeam",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionRule {
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

impl CollisionRule {
    #[must_use]
    pub const fn to_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::PushOtherTeams => "pushOtherTeams",
            Self::PushOwnTeam => "pushOwnTeam",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Team {
    pub name: String,
    pub display_name: TextComponent,
    pub options: i8,
    pub nametag_visibility: NameTagVisibility,
    pub collision_rule: CollisionRule,
    pub color: NamedColor,
    pub player_prefix: TextComponent,
    pub player_suffix: TextComponent,
    pub players: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BedrockSortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BedrockDisplaySlot {
    PlayerList,
    Sidebar,
    BelowName,
}

impl BedrockDisplaySlot {
    #[must_use]
    pub const fn to_str(&self) -> &'static str {
        match self {
            Self::PlayerList => "list",
            Self::Sidebar => "sidebar",
            Self::BelowName => "belowname",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BedrockObjective {
    pub name: String,
    pub display_name: String,
    pub sort_order: BedrockSortOrder,
}

#[derive(Clone, Debug, Default)]
pub struct BedrockScoreboard {
    pub objectives: FxHashMap<String, BedrockObjective>,
    pub display_slots: FxHashMap<BedrockDisplaySlot, String>,
    pub scores: FxHashMap<(String, String), i32>,
}

impl BedrockScoreboard {
    pub fn add_objective(&mut self, player: &Player, objective: BedrockObjective) {
        let be_update = BSetDisplayObjective {
            display_slot_name: "sidebar".to_string(),
            objective_name: objective.name.clone(),
            objective_display_name: objective.display_name.clone(),
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(match objective.sort_order {
                BedrockSortOrder::Ascending => 0,
                BedrockSortOrder::Descending => 1,
            }),
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CUpdateObjectives::new(
                objective.name.clone(),
                pumpkin_protocol::java::client::play::Mode::Add,
                TextComponent::text(objective.display_name.clone()),
                pumpkin_protocol::java::client::play::RenderType::Integer,
                None,
            ),
            &be_update,
        );

        self.objectives.insert(objective.name.clone(), objective);
    }

    pub fn update_objective(&mut self, player: &Player, objective: BedrockObjective) {
        let be_update = BSetDisplayObjective {
            display_slot_name: "sidebar".to_string(),
            objective_name: objective.name.clone(),
            objective_display_name: objective.display_name.clone(),
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(match objective.sort_order {
                BedrockSortOrder::Ascending => 0,
                BedrockSortOrder::Descending => 1,
            }),
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CUpdateObjectives::new(
                objective.name.clone(),
                pumpkin_protocol::java::client::play::Mode::Update,
                TextComponent::text(objective.display_name.clone()),
                pumpkin_protocol::java::client::play::RenderType::Integer,
                None,
            ),
            &be_update,
        );

        self.objectives.insert(objective.name.clone(), objective);
    }

    pub fn remove_objective(&mut self, player: &Player, name: &str) {
        let be_remove = BRemoveObjective {
            objective_name: name.to_string(),
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CUpdateObjectives::new(
                name.to_string(),
                pumpkin_protocol::java::client::play::Mode::Remove,
                TextComponent::text(""),
                pumpkin_protocol::java::client::play::RenderType::Integer,
                None,
            ),
            &be_remove,
        );
        self.objectives.remove(name);
        self.display_slots.retain(|_, v| v != name);
        self.scores.retain(|(_, obj), _| obj != name);
    }

    pub fn set_display_objective(
        &mut self,
        player: &Player,
        slot: BedrockDisplaySlot,
        objective_name: Option<&str>,
    ) {
        let obj_name_str = objective_name.unwrap_or("");
        let display_name = objective_name
            .and_then(|name| self.objectives.get(name))
            .map_or_else(|| obj_name_str.to_string(), |o| o.display_name.clone());

        let be_display = BSetDisplayObjective {
            display_slot_name: slot.to_str().to_string(),
            objective_name: obj_name_str.to_string(),
            objective_display_name: display_name,
            criteria_name: "dummy".to_string(),
            sort_order: VarInt(0),
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CDisplayObjective::new(
                match slot {
                    BedrockDisplaySlot::PlayerList => ScoreboardDisplaySlot::List,
                    BedrockDisplaySlot::Sidebar => ScoreboardDisplaySlot::Sidebar,
                    BedrockDisplaySlot::BelowName => ScoreboardDisplaySlot::BelowName,
                },
                obj_name_str.to_string(),
            ),
            &be_display,
        );

        if let Some(name) = objective_name {
            self.display_slots.insert(slot, name.to_string());
        } else {
            self.display_slots.remove(&slot);
        }
    }

    pub fn clear_display_objective(&mut self, player: &Player, slot: BedrockDisplaySlot) {
        self.set_display_objective(player, slot, None);
    }

    pub fn update_score(
        &mut self,
        player: &Player,
        entity_name: &str,
        objective_name: &str,
        value: i32,
    ) {
        let score = ScoreboardScore::new(
            entity_name.to_string(),
            objective_name.to_string(),
            VarInt(value),
            None,
            None,
        );
        let be_score = BSetScore {
            action: VarInt(0),
            entries: vec![],
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CUpdateScore::new(
                score.entity_name.clone(),
                score.objective_name.clone(),
                score.value,
                score.display_name.clone(),
                score.number_format,
            ),
            &be_score,
        );

        self.scores
            .insert((entity_name.to_string(), objective_name.to_string()), value);
    }

    pub fn add_score(
        &mut self,
        player: &Player,
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        delta: i32,
    ) -> i32 {
        let entity_s = entity_name.into();
        let obj_s = objective_name.into();
        let current = self
            .scores
            .get(&(entity_s.clone(), obj_s.clone()))
            .copied()
            .unwrap_or(0);
        let new_val = current + delta;
        self.update_score(player, &entity_s, &obj_s, new_val);
        new_val
    }

    pub fn remove_score(&mut self, player: &Player, entity_name: &str, objective_name: &str) {
        let be_score = BSetScore {
            action: VarInt(1),
            entries: vec![],
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CResetScore::new(
                entity_name.to_string(),
                Some(objective_name.to_string()),
            ),
            &be_score,
        );
        self.scores
            .remove(&(entity_name.to_string(), objective_name.to_string()));
    }

    pub fn reset_scores_for_entity(&mut self, player: &Player, entity_name: &str) {
        let be_score = BSetScore {
            action: VarInt(1),
            entries: vec![],
        };
        player.try_enqueue_packet_editioned(
            &pumpkin_protocol::java::client::play::CResetScore::new(entity_name.to_string(), None),
            &be_score,
        );
        self.scores.retain(|(entity, _), _| entity != entity_name);
    }

    pub fn send_to_player(&self, player: &Player) {
        for objective in self.objectives.values() {
            let be_update = BSetDisplayObjective {
                display_slot_name: "sidebar".to_string(),
                objective_name: objective.name.clone(),
                objective_display_name: objective.display_name.clone(),
                criteria_name: "dummy".to_string(),
                sort_order: VarInt(match objective.sort_order {
                    BedrockSortOrder::Ascending => 0,
                    BedrockSortOrder::Descending => 1,
                }),
            };
            player.try_enqueue_packet_editioned(
                &pumpkin_protocol::java::client::play::CUpdateObjectives::new(
                    objective.name.clone(),
                    pumpkin_protocol::java::client::play::Mode::Add,
                    TextComponent::text(objective.display_name.clone()),
                    pumpkin_protocol::java::client::play::RenderType::Integer,
                    None,
                ),
                &be_update,
            );
        }

        for (slot, objective_name) in &self.display_slots {
            let display_name = self
                .objectives
                .get(objective_name)
                .map_or_else(|| objective_name.clone(), |o| o.display_name.clone());
            let be_display = BSetDisplayObjective {
                display_slot_name: slot.to_str().to_string(),
                objective_name: objective_name.clone(),
                objective_display_name: display_name,
                criteria_name: "dummy".to_string(),
                sort_order: VarInt(0),
            };
            player.try_enqueue_packet_editioned(
                &pumpkin_protocol::java::client::play::CDisplayObjective::new(
                    match slot {
                        BedrockDisplaySlot::PlayerList => ScoreboardDisplaySlot::List,
                        BedrockDisplaySlot::Sidebar => ScoreboardDisplaySlot::Sidebar,
                        BedrockDisplaySlot::BelowName => ScoreboardDisplaySlot::BelowName,
                    },
                    objective_name.clone(),
                ),
                &be_display,
            );
        }

        for ((entity_name, objective_name), value) in &self.scores {
            let score = ScoreboardScore::new(
                entity_name.clone(),
                objective_name.clone(),
                VarInt(*value),
                None,
                None,
            );
            let be_score = BSetScore {
                action: VarInt(0),
                entries: vec![],
            };
            player.try_enqueue_packet_editioned(
                &pumpkin_protocol::java::client::play::CUpdateScore::new(
                    score.entity_name,
                    score.objective_name,
                    score.value,
                    score.display_name,
                    score.number_format,
                ),
                &be_score,
            );
        }
    }
}

#[derive(Default, Debug)]
pub struct ScoreboardBuilder {
    scoreboard: Scoreboard,
}

impl ScoreboardBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn objective(mut self, name: impl Into<String>, display_name: TextComponent) -> Self {
        let obj = ScoreboardObjective::new(
            name.into(),
            display_name,
            RenderType::Integer,
            None,
            "dummy",
        );
        self.scoreboard.objectives.insert(obj.name.clone(), obj);
        self
    }

    #[must_use]
    pub fn objective_with_render(
        mut self,
        name: impl Into<String>,
        display_name: TextComponent,
        render_type: RenderType,
    ) -> Self {
        let obj = ScoreboardObjective::new(name.into(), display_name, render_type, None, "dummy");
        self.scoreboard.objectives.insert(obj.name.clone(), obj);
        self
    }

    #[must_use]
    pub fn display_slot(
        mut self,
        slot: ScoreboardDisplaySlot,
        objective_name: impl Into<String>,
    ) -> Self {
        self.scoreboard
            .display_slots
            .insert(slot, objective_name.into());
        self
    }

    #[must_use]
    pub fn score(
        mut self,
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        value: i32,
    ) -> Self {
        let entity_s = entity_name.into();
        let obj_s = objective_name.into();
        let score =
            ScoreboardScore::new(entity_s.clone(), obj_s.clone(), VarInt(value), None, None);
        self.scoreboard
            .scores
            .entry(entity_s)
            .or_default()
            .insert(obj_s, score);
        self
    }

    #[must_use]
    pub fn team(mut self, team: Team) -> Self {
        self.scoreboard.teams.insert(team.name.clone(), team);
        self
    }

    #[must_use]
    pub fn build(self) -> Scoreboard {
        self.scoreboard
    }
}

#[derive(Default, Debug)]
pub struct BedrockScoreboardBuilder {
    scoreboard: BedrockScoreboard,
}

impl BedrockScoreboardBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn objective(
        mut self,
        name: impl Into<String>,
        display_name: impl Into<String>,
        sort_order: BedrockSortOrder,
    ) -> Self {
        let name = name.into();
        let obj = BedrockObjective {
            name: name.clone(),
            display_name: display_name.into(),
            sort_order,
        };
        self.scoreboard.objectives.insert(name, obj);
        self
    }

    #[must_use]
    pub fn display_slot(
        mut self,
        slot: BedrockDisplaySlot,
        objective_name: impl Into<String>,
    ) -> Self {
        self.scoreboard
            .display_slots
            .insert(slot, objective_name.into());
        self
    }

    #[must_use]
    pub fn score(
        mut self,
        entity_name: impl Into<String>,
        objective_name: impl Into<String>,
        value: i32,
    ) -> Self {
        self.scoreboard
            .scores
            .insert((entity_name.into(), objective_name.into()), value);
        self
    }

    #[must_use]
    pub fn build(self) -> BedrockScoreboard {
        self.scoreboard
    }
}

const SCOREBOARD_FILE: &str = "scoreboard.dat";

/// The scoreboards of the open saves, by root folder. Vanilla keeps one scoreboard for the
/// whole server, so every dimension of a save shares the one loaded from its `scoreboard.dat`.
static SHARED_SCOREBOARDS: LazyLock<
    std::sync::Mutex<FxHashMap<PathBuf, Weak<std::sync::Mutex<Scoreboard>>>>,
> = LazyLock::new(Default::default);

/// The scoreboard shared by the dimensions saved under `root_folder`, loaded on first use.
#[must_use]
pub fn shared_scoreboard(root_folder: &Path) -> Arc<std::sync::Mutex<Scoreboard>> {
    let mut shared = SHARED_SCOREBOARDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    shared.retain(|_, board| board.strong_count() > 0);
    if let Some(board) = shared.get(root_folder).and_then(Weak::upgrade) {
        return board;
    }
    let board = Arc::new(std::sync::Mutex::new(
        read_data_file(root_folder, SCOREBOARD_FILE)
            .map(|data| Scoreboard::from_nbt(&data))
            .unwrap_or_default(),
    ));
    shared.insert(root_folder.to_path_buf(), Arc::downgrade(&board));
    board
}

pub fn save_scoreboard(root_folder: &Path, scoreboard: &Scoreboard) -> Result<(), WorldInfoError> {
    write_data_file(
        root_folder,
        SCOREBOARD_FILE,
        DATA_VERSION,
        scoreboard.to_nbt(),
    )
}

const TEAM_SLOTS: [(ScoreboardDisplaySlot, NamedColor); 16] = [
    (ScoreboardDisplaySlot::TeamBlack, NamedColor::Black),
    (ScoreboardDisplaySlot::TeamDarkBlue, NamedColor::DarkBlue),
    (ScoreboardDisplaySlot::TeamDarkGreen, NamedColor::DarkGreen),
    (ScoreboardDisplaySlot::TeamDarkAqua, NamedColor::DarkAqua),
    (ScoreboardDisplaySlot::TeamDarkRed, NamedColor::DarkRed),
    (
        ScoreboardDisplaySlot::TeamDarkPurple,
        NamedColor::DarkPurple,
    ),
    (ScoreboardDisplaySlot::TeamGold, NamedColor::Gold),
    (ScoreboardDisplaySlot::TeamGray, NamedColor::Gray),
    (ScoreboardDisplaySlot::TeamDarkGray, NamedColor::DarkGray),
    (ScoreboardDisplaySlot::TeamBlue, NamedColor::Blue),
    (ScoreboardDisplaySlot::TeamGreen, NamedColor::Green),
    (ScoreboardDisplaySlot::TeamAqua, NamedColor::Aqua),
    (ScoreboardDisplaySlot::TeamRed, NamedColor::Red),
    (
        ScoreboardDisplaySlot::TeamLightPurple,
        NamedColor::LightPurple,
    ),
    (ScoreboardDisplaySlot::TeamYellow, NamedColor::Yellow),
    (ScoreboardDisplaySlot::TeamWhite, NamedColor::White),
];

/// Vanilla `DisplaySlot` serialized name.
fn slot_name(slot: ScoreboardDisplaySlot) -> String {
    match slot {
        ScoreboardDisplaySlot::List => "list".to_string(),
        ScoreboardDisplaySlot::Sidebar => "sidebar".to_string(),
        ScoreboardDisplaySlot::BelowName => "below_name".to_string(),
        team => TEAM_SLOTS
            .iter()
            .find(|(s, _)| *s == team)
            .map_or_else(String::new, |(_, color)| {
                format!("sidebar.team.{}", color.name())
            }),
    }
}

fn slot_from_name(name: &str) -> Option<ScoreboardDisplaySlot> {
    match name {
        "list" => Some(ScoreboardDisplaySlot::List),
        "sidebar" => Some(ScoreboardDisplaySlot::Sidebar),
        "below_name" | "belowName" => Some(ScoreboardDisplaySlot::BelowName),
        _ => {
            let color = NamedColor::try_from(name.strip_prefix("sidebar.team.")?).ok()?;
            TEAM_SLOTS
                .iter()
                .find(|(_, c)| *c == color)
                .map(|(slot, _)| *slot)
        }
    }
}

fn text_to_nbt(text: &TextComponent) -> NbtTag {
    text.to_nbt_tag_for_version(&JavaMinecraftVersion::V_26_3)
}

fn compounds(list: Option<&[NbtTag]>) -> impl Iterator<Item = &NbtCompound> {
    list.unwrap_or_default().iter().filter_map(|tag| match tag {
        NbtTag::Compound(compound) => Some(compound),
        _ => None,
    })
}

fn team_from_nbt(nbt: &NbtCompound) -> Option<Team> {
    let name = nbt.get_string("Name")?;
    let text = |key: &str| {
        nbt.get(key)
            .map_or_else(TextComponent::empty, TextComponent::from_nbt)
    };
    let mut options = 0;
    if nbt.get_bool("AllowFriendlyFire").unwrap_or(true) {
        options |= 0x01;
    }
    if nbt.get_bool("SeeFriendlyInvisibles").unwrap_or(true) {
        options |= 0x02;
    }
    Some(Team {
        name: name.to_string(),
        display_name: nbt.get("DisplayName").map_or_else(
            || TextComponent::text(name.to_string()),
            TextComponent::from_nbt,
        ),
        options,
        nametag_visibility: match nbt.get_string("NameTagVisibility") {
            Some("never") => NameTagVisibility::Never,
            Some("hideForOtherTeams") => NameTagVisibility::HideForOtherTeams,
            Some("hideForOwnTeam") => NameTagVisibility::HideForOwnTeam,
            _ => NameTagVisibility::Always,
        },
        collision_rule: match nbt.get_string("CollisionRule") {
            Some("never") => CollisionRule::Never,
            Some("pushOtherTeams") => CollisionRule::PushOtherTeams,
            Some("pushOwnTeam") => CollisionRule::PushOwnTeam,
            _ => CollisionRule::Always,
        },
        color: nbt
            .get_string("TeamColor")
            .and_then(|color| NamedColor::try_from(color).ok())
            .unwrap_or(NamedColor::White),
        player_prefix: text("MemberNamePrefix"),
        player_suffix: text("MemberNameSuffix"),
        players: nbt
            .get_list("Players")
            .unwrap_or_default()
            .iter()
            .filter_map(|tag| match tag {
                NbtTag::String(player) => Some(player.to_string()),
                _ => None,
            })
            .collect(),
    })
}

impl Scoreboard {
    /// The `data` compound of vanilla's `scoreboard.dat`. Number formats are not saved.
    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let objectives = self
            .objectives
            .values()
            .map(|objective| {
                let mut nbt = NbtCompound::new();
                nbt.put_string("Name", objective.name.clone());
                nbt.put_string("CriteriaName", objective.criterion.clone());
                nbt.put("DisplayName", text_to_nbt(&objective.display_name));
                nbt.put_string(
                    "RenderType",
                    match objective.render_type {
                        RenderType::Integer => "integer",
                        RenderType::Hearts => "hearts",
                    }
                    .to_string(),
                );
                nbt.put_bool("display_auto_update", false);
                NbtTag::Compound(nbt)
            })
            .collect();
        let scores = self
            .scores
            .values()
            .flat_map(FxHashMap::values)
            .map(|score| {
                let mut nbt = NbtCompound::new();
                nbt.put_string("Name", score.entity_name.clone());
                nbt.put_string("Objective", score.objective_name.clone());
                nbt.put_int("Score", score.value.0);
                nbt.put_bool("Locked", score.locked);
                if let Some(display) = &score.display_name {
                    nbt.put("display", text_to_nbt(display));
                }
                NbtTag::Compound(nbt)
            })
            .collect();
        let teams = self
            .teams
            .values()
            .map(|team| {
                let mut nbt = NbtCompound::new();
                nbt.put_string("Name", team.name.clone());
                nbt.put("DisplayName", text_to_nbt(&team.display_name));
                // PEX has no "reset" team colour; white is the default it stands in for.
                if team.color != NamedColor::White {
                    nbt.put_string("TeamColor", team.color.name().to_string());
                }
                nbt.put_bool("AllowFriendlyFire", team.options & 0x01 != 0);
                nbt.put_bool("SeeFriendlyInvisibles", team.options & 0x02 != 0);
                nbt.put("MemberNamePrefix", text_to_nbt(&team.player_prefix));
                nbt.put("MemberNameSuffix", text_to_nbt(&team.player_suffix));
                nbt.put_string(
                    "NameTagVisibility",
                    team.nametag_visibility.to_str().to_string(),
                );
                nbt.put_string("DeathMessageVisibility", "always".to_string());
                nbt.put_string("CollisionRule", team.collision_rule.to_str().to_string());
                nbt.put_list(
                    "Players",
                    team.players
                        .iter()
                        .map(|player| NbtTag::String(player.clone().into()))
                        .collect(),
                );
                NbtTag::Compound(nbt)
            })
            .collect();
        let mut display_slots = NbtCompound::new();
        for (slot, objective) in &self.display_slots {
            display_slots.put_string(&slot_name(*slot), objective.clone());
        }

        let mut data = NbtCompound::new();
        data.put_list("Objectives", objectives);
        data.put_list("PlayerScores", scores);
        data.put_list("Teams", teams);
        data.put_compound("DisplaySlots", display_slots);
        data
    }

    /// Reads the `data` compound of vanilla's `scoreboard.dat`.
    #[must_use]
    pub fn from_nbt(data: &NbtCompound) -> Self {
        let mut board = Self::default();
        for nbt in compounds(data.get_list("Objectives")) {
            let Some(name) = nbt.get_string("Name") else {
                continue;
            };
            let display_name = nbt.get("DisplayName").map_or_else(
                || TextComponent::text(name.to_string()),
                TextComponent::from_nbt,
            );
            let render_type = if nbt.get_string("RenderType") == Some("hearts") {
                RenderType::Hearts
            } else {
                RenderType::Integer
            };
            let criterion = nbt.get_string("CriteriaName").unwrap_or("dummy");
            board.objectives.insert(
                name.to_string(),
                ScoreboardObjective::new(name, display_name, render_type, None, criterion),
            );
        }
        for nbt in compounds(data.get_list("PlayerScores")) {
            let (Some(entity), Some(objective)) =
                (nbt.get_string("Name"), nbt.get_string("Objective"))
            else {
                continue;
            };
            if !board.objectives.contains_key(objective) {
                continue;
            }
            let mut score = ScoreboardScore::new(
                entity,
                objective,
                VarInt(nbt.get_int("Score").unwrap_or(0)),
                nbt.get("display").map(TextComponent::from_nbt),
                None,
            );
            score.locked = nbt.get_bool("Locked").unwrap_or(false);
            board
                .scores
                .entry(objective.to_string())
                .or_default()
                .insert(entity.to_string(), score);
        }
        for team in compounds(data.get_list("Teams")).filter_map(team_from_nbt) {
            board.teams.insert(team.name.clone(), team);
        }
        if let Some(slots) = data.get_compound("DisplaySlots") {
            for (slot, objective) in &slots.child_tags {
                if let (Some(slot), NbtTag::String(objective)) = (slot_from_name(slot), objective)
                    && board.objectives.contains_key(&**objective)
                {
                    board.display_slots.insert(slot, objective.to_string());
                }
            }
        }
        board
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scoreboard_survives_a_save_in_vanilla_fields() {
        let mut board = Scoreboard::new();
        board.add_objective(
            &NoTarget,
            ScoreboardObjective::new(
                "kills",
                TextComponent::text("Kills"),
                RenderType::Hearts,
                None,
                "playerKillCount",
            ),
        );
        board.set_score_value(&NoTarget, "Steve", "kills", 7);
        board.set_display_objective(&NoTarget, ScoreboardDisplaySlot::Sidebar, Some("kills"));
        board.set_display_objective(&NoTarget, ScoreboardDisplaySlot::TeamRed, Some("kills"));
        board.teams.insert(
            "red".to_string(),
            Team {
                name: "red".to_string(),
                display_name: TextComponent::text("Red"),
                options: 0x02,
                nametag_visibility: NameTagVisibility::HideForOtherTeams,
                collision_rule: CollisionRule::PushOwnTeam,
                color: NamedColor::Red,
                player_prefix: TextComponent::text("[R] "),
                player_suffix: TextComponent::empty(),
                players: vec!["Steve".to_string()],
            },
        );

        let data = board.to_nbt();
        let slots = data.get_compound("DisplaySlots").unwrap();
        assert_eq!(slots.get_string("sidebar.team.red"), Some("kills"));
        let loaded = Scoreboard::from_nbt(&data);

        let objective = loaded.get_objective("kills").unwrap();
        assert_eq!(objective.render_type, RenderType::Hearts);
        assert_eq!(objective.criterion, "playerKillCount");
        assert_eq!(loaded.get_score_value("Steve", "kills"), Some(7));
        assert_eq!(
            loaded.get_display_objective(ScoreboardDisplaySlot::TeamRed),
            Some("kills")
        );
        let team = loaded.get_team("red").unwrap();
        assert_eq!(team.options, 0x02);
        assert_eq!(team.color, NamedColor::Red);
        assert_eq!(
            team.nametag_visibility,
            NameTagVisibility::HideForOtherTeams
        );
        assert_eq!(team.collision_rule, CollisionRule::PushOwnTeam);
        assert_eq!(team.players, ["Steve"]);
    }

    #[test]
    fn every_dimension_of_a_save_shares_one_scoreboard() {
        let root = std::env::temp_dir().join(format!("pex-scoreboard-{}", std::process::id()));
        let overworld = shared_scoreboard(&root);
        let nether = shared_scoreboard(&root);
        assert!(Arc::ptr_eq(&overworld, &nether));
        let other = shared_scoreboard(&root.join("other"));
        assert!(!Arc::ptr_eq(&overworld, &other));
    }
}
