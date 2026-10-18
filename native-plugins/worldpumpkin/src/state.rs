// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{
    config::Config,
    engine::{BlockPos, HistoryEntry, Selection},
};
use std::collections::{HashMap, VecDeque};

pub struct PluginState {
    config: Config,
    sessions: HashMap<String, PlayerSession>,
}

impl PluginState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
        }
    }

    pub const fn config(&self) -> &Config {
        &self.config
    }

    pub fn replace_config(&mut self, config: Config) {
        self.config = config;
        self.prune_all_history();
    }

    /// Stores an endpoint for the owner, resetting the previous selection when the world changes.
    pub fn set_position(
        &mut self,
        owner: String,
        world_id: String,
        slot: SelectionSlot,
        pos: BlockPos,
    ) -> Selection {
        let session = self.sessions.entry(owner).or_default();
        if session.selection_world != world_id {
            session.selection = Selection::default();
            session.selection_world = world_id;
        }
        match slot {
            SelectionSlot::Pos1 => session.selection.pos1 = Some(pos),
            SelectionSlot::Pos2 => session.selection.pos2 = Some(pos),
        }
        session.selection
    }

    /// Returns the owner's selection only when it belongs to the requested world.
    pub fn selection(&self, owner: &str, world_id: &str) -> Option<Selection> {
        self.sessions
            .get(owner)
            .filter(|session| session.selection_world == world_id)
            .map(|session| session.selection)
    }

    pub fn push_undo_history(&mut self, owner: String, entry: HistoryEntry) {
        let session = self.sessions.entry(owner).or_default();
        session.redo_history.clear();
        session.redo_blocks = 0;
        push_undo_history(session, entry, &self.config);
    }

    pub fn push_replayed_undo_history(&mut self, owner: String, entry: HistoryEntry) {
        let session = self.sessions.entry(owner).or_default();
        push_undo_history(session, entry, &self.config);
    }

    pub fn push_redo_history(&mut self, owner: String, entry: HistoryEntry) {
        let session = self.sessions.entry(owner).or_default();
        push_history_entry(
            &mut session.redo_history,
            &mut session.redo_blocks,
            entry,
            self.config.max_history_entries,
            self.config.max_history_blocks,
        );
    }

    pub fn pop_undo_history(&mut self, owner: &str) -> Option<HistoryEntry> {
        self.sessions.get_mut(owner).and_then(|session| {
            pop_history_entry(&mut session.undo_history, &mut session.undo_blocks)
        })
    }

    pub fn pop_redo_history(&mut self, owner: &str) -> Option<HistoryEntry> {
        self.sessions.get_mut(owner).and_then(|session| {
            pop_history_entry(&mut session.redo_history, &mut session.redo_blocks)
        })
    }

    pub fn latest_undo_history(&self, owner: &str) -> Option<HistoryInfo> {
        self.sessions
            .get(owner)
            .and_then(|session| history_info(session.undo_history.front()))
    }

    pub fn latest_redo_history(&self, owner: &str) -> Option<HistoryInfo> {
        self.sessions
            .get(owner)
            .and_then(|session| history_info(session.redo_history.front()))
    }

    fn prune_all_history(&mut self) {
        let max_entries = self.config.max_history_entries;
        let max_blocks = self.config.max_history_blocks;
        for session in self.sessions.values_mut() {
            prune_history(
                &mut session.undo_history,
                &mut session.undo_blocks,
                max_entries,
                max_blocks,
            );
            prune_history(
                &mut session.redo_history,
                &mut session.redo_blocks,
                max_entries,
                max_blocks,
            );
        }
    }
}

#[derive(Clone, Copy)]
pub enum SelectionSlot {
    Pos1,
    Pos2,
}

pub struct HistoryInfo {
    pub world_id: String,
    pub blocks: usize,
}

#[derive(Default)]
struct PlayerSession {
    selection: Selection,
    selection_world: String,
    undo_history: VecDeque<HistoryEntry>,
    redo_history: VecDeque<HistoryEntry>,
    undo_blocks: usize,
    redo_blocks: usize,
}

fn push_history_entry(
    history: &mut VecDeque<HistoryEntry>,
    block_count: &mut usize,
    entry: HistoryEntry,
    max_entries: usize,
    max_blocks: usize,
) {
    *block_count = block_count.saturating_add(entry.len());
    history.push_front(entry);
    prune_history(history, block_count, max_entries, max_blocks);
}

fn push_undo_history(session: &mut PlayerSession, entry: HistoryEntry, config: &Config) {
    push_history_entry(
        &mut session.undo_history,
        &mut session.undo_blocks,
        entry,
        config.max_history_entries,
        config.max_history_blocks,
    );
}

fn history_info(entry: Option<&HistoryEntry>) -> Option<HistoryInfo> {
    let entry = entry?;
    Some(HistoryInfo {
        world_id: entry.world_id().to_owned(),
        blocks: entry.len(),
    })
}

fn pop_history_entry(
    history: &mut VecDeque<HistoryEntry>,
    block_count: &mut usize,
) -> Option<HistoryEntry> {
    let entry = history.pop_front()?;
    *block_count = block_count.saturating_sub(entry.len());
    Some(entry)
}

fn prune_history(
    history: &mut VecDeque<HistoryEntry>,
    block_count: &mut usize,
    max_entries: usize,
    max_blocks: usize,
) {
    while history.len() > max_entries || *block_count > max_blocks {
        let Some(entry) = history.pop_back() else {
            *block_count = 0;
            return;
        };
        *block_count = block_count.saturating_sub(entry.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Checks that changing worlds clears old endpoints and hides selections from other worlds.
    #[test]
    fn selections_cannot_cross_worlds() {
        let mut state = PluginState::new(Config::default());
        let pos = BlockPos { x: 1, y: 2, z: 3 };
        state.set_position(
            "player".into(),
            "overworld".into(),
            SelectionSlot::Pos1,
            pos,
        );
        assert!(state.selection("player", "nether").is_none());
        let selection =
            state.set_position("player".into(), "nether".into(), SelectionSlot::Pos2, pos);
        assert!(selection.pos1.is_none());
        assert_eq!(selection.pos2, Some(pos));
        assert!(state.selection("player", "overworld").is_none());
    }
}
