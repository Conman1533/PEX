// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{blocks::BlockPattern, config::Config, host::World, state::PluginState};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

mod geometry;
mod writer;

use geometry::CuboidIter;
pub use geometry::{BlockPos, Cuboid, Selection};
use writer::{WorldAccess, WriteStrategy, apply_forward, replay_change};

// Patterns are deterministic inside one edit, but each queued edit gets a fresh distribution.
static PATTERN_SEED: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    world_id: String,
    changes: Vec<BlockChange>,
}

impl HistoryEntry {
    pub const fn new(world_id: String, changes: Vec<BlockChange>) -> Self {
        Self { world_id, changes }
    }

    pub const fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn world_id(&self) -> &str {
        &self.world_id
    }
}

#[derive(Clone, Debug)]
pub struct BlockChange {
    pos: BlockPos,
    old_state: u16,
    new_state: u16,
    old_block_entity: Option<Vec<u8>>,
    new_block_entity: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BlockSnapshot {
    state: u16,
    block_entity: Option<Vec<u8>>,
}

struct MoveState {
    positions: Vec<BlockPos>,
    offset: BlockPos,
    snapshots: HashMap<BlockPos, BlockSnapshot>,
    cells: Vec<MoveCell>,
    phase: MovePhase,
    next_index: usize,
}

struct MoveCell {
    pos: BlockPos,
    old: BlockSnapshot,
    new: BlockSnapshot,
}

enum MovePhase {
    Snapshot,
    Apply,
}

enum EditKind {
    Set {
        to: Arc<BlockPattern>,
    },
    Replace {
        from: u16,
        to: Arc<BlockPattern>,
    },
    Move(MoveState),
    Replay {
        history: HistoryEntry,
        direction: ReplayDirection,
        next_index: usize,
    },
}

#[derive(Clone, Copy)]
pub enum ReplayDirection {
    Undo,
    Redo,
}

pub struct EditOperation {
    owner: String,
    world: World,
    world_id: String,
    kind: EditKind,
    positions: EditPositions,
    history: Vec<BlockChange>,
    state: Arc<Mutex<PluginState>>,
    remaining: u64,
    pattern_seed: u64,
}

impl EditOperation {
    /// Creates a deferred cuboid fill with a fresh seed for deterministic pattern choices.
    pub fn set(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to: Arc::new(to) },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            pattern_seed: next_pattern_seed(),
        }
    }

    /// Creates a deferred edit that replaces only states matching `from` with pattern choices.
    pub fn replace(
        owner: String,
        world: World,
        cuboid: Cuboid,
        from: u16,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Replace {
                from,
                to: Arc::new(to),
            },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            pattern_seed: next_pattern_seed(),
        }
    }

    /// Creates a deferred pattern fill of the cuboid's vertical faces without duplicate positions.
    pub fn walls(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let positions = cuboid.wall_positions();
        let remaining = positions.len() as u64;
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to: Arc::new(to) },
            positions: EditPositions::vec(positions),
            history: Vec::new(),
            state,
            remaining,
            pattern_seed: next_pattern_seed(),
        }
    }

    pub fn move_blocks(
        owner: String,
        world: World,
        cuboid: Cuboid,
        offset: BlockPos,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let positions: Vec<_> = cuboid.iter().collect();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Move(MoveState {
                snapshots: HashMap::with_capacity(positions.len().saturating_mul(2)),
                positions,
                offset,
                cells: Vec::new(),
                phase: MovePhase::Snapshot,
                next_index: 0,
            }),
            positions: EditPositions::empty(),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            pattern_seed: 0,
        }
    }

    /// Creates a deferred history replay, traversing changes backward for undo and forward for redo.
    pub fn replay(
        owner: String,
        world: World,
        history: HistoryEntry,
        direction: ReplayDirection,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let remaining = history.len() as u64;
        let next_index = match direction {
            ReplayDirection::Undo => history.len(),
            ReplayDirection::Redo => 0,
        };
        Self {
            owner,
            world,
            world_id: history.world_id().to_owned(),
            kind: EditKind::Replay {
                history,
                direction,
                next_index,
            },
            positions: EditPositions::empty(),
            history: Vec::new(),
            state,
            remaining,
            pattern_seed: 0,
        }
    }

    const fn pending_blocks(&self) -> u64 {
        match &self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } => self.remaining,
            EditKind::Move(move_state) => match move_state.phase {
                MovePhase::Snapshot => move_state
                    .positions
                    .len()
                    .saturating_sub(move_state.next_index)
                    as u64,
                MovePhase::Apply => {
                    move_state.cells.len().saturating_sub(move_state.next_index) as u64
                }
            },
            EditKind::Replay {
                history,
                direction,
                next_index,
            } => match direction {
                ReplayDirection::Undo => *next_index as u64,
                ReplayDirection::Redo => history.len().saturating_sub(*next_index) as u64,
            },
        }
    }

    fn process(&mut self, budget: usize, config: &Config) -> ProcessResult {
        let writer = WriteStrategy::new(config);
        if let EditKind::Replay {
            history,
            direction,
            next_index,
        } = &mut self.kind
        {
            return process_replay(
                &self.world,
                history,
                *direction,
                next_index,
                budget,
                &writer,
            );
        }

        if let EditKind::Move(move_state) = &mut self.kind {
            return process_move(
                &self.world,
                &mut self.history,
                move_state,
                budget,
                config,
                &writer,
            );
        }

        match self.kind {
            EditKind::Set { ref to } => {
                self.process_forward(budget, config, &writer, None, to.clone())
            }
            EditKind::Replace { from, ref to } => {
                self.process_forward(budget, config, &writer, Some(from), to.clone())
            }
            // Handled by the early returns above.
            EditKind::Move(_) | EditKind::Replay { .. } => ProcessResult::Finished { scanned: 0 },
        }
    }

    fn finish(&mut self) {
        match &self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } | EditKind::Move(_) => {
                let history =
                    HistoryEntry::new(self.world_id.clone(), std::mem::take(&mut self.history));
                if history.len() > 0 {
                    self.state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push_undo_history(self.owner.clone(), history);
                }
            }
            EditKind::Replay {
                history, direction, ..
            } => {
                let history = history.clone();
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match direction {
                    ReplayDirection::Undo => state.push_redo_history(self.owner.clone(), history),
                    ReplayDirection::Redo => {
                        state.push_replayed_undo_history(self.owner.clone(), history);
                    }
                }
            }
        }
    }

    /// Visits at most `budget` positions, applying the pattern and recording changes up to the history limit.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the caller clones the Arc to release its borrow of `self.kind`"
    )]
    fn process_forward(
        &mut self,
        budget: usize,
        config: &Config,
        writer: &WriteStrategy,
        replace_from: Option<u16>,
        pattern: Arc<BlockPattern>,
    ) -> ProcessResult {
        let mut visited = 0;

        while visited < budget {
            let Some(pos) = self.positions.next() else {
                return ProcessResult::Finished { scanned: visited };
            };

            visited += 1;
            let to = pattern.choose(pos, self.pattern_seed);
            let mut access = WorldAccess::new(&self.world, writer);
            if let Some(change) = apply_forward(
                &mut access,
                pos,
                to,
                replace_from,
                self.history.len() < config.max_history_blocks,
            ) {
                self.history.push(change);
            }
            self.remaining = self.remaining.saturating_sub(1);
        }

        ProcessResult::Pending { scanned: visited }
    }
}

enum EditPositions {
    Cuboid(CuboidIter),
    Vec(std::vec::IntoIter<BlockPos>),
}

impl EditPositions {
    const fn cuboid(cuboid: Cuboid) -> Self {
        Self::Cuboid(cuboid.iter())
    }

    fn vec(positions: Vec<BlockPos>) -> Self {
        Self::Vec(positions.into_iter())
    }

    fn empty() -> Self {
        Self::Vec(Vec::new().into_iter())
    }
}

impl Iterator for EditPositions {
    type Item = BlockPos;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Cuboid(iter) => iter.next(),
            Self::Vec(iter) => iter.next(),
        }
    }
}

#[derive(Default)]
pub struct EditQueue {
    queue: VecDeque<EditOperation>,
    queued_blocks: u64,
}

impl EditQueue {
    pub fn can_enqueue(&self, blocks: u64, config: &Config) -> Result<(), String> {
        if self.queue.len() >= config.max_queued_operations {
            return Err(format!(
                "Too many edits are already queued ({}/{}).",
                self.queue.len(),
                config.max_queued_operations
            ));
        }

        let Some(total_blocks) = self.queued_blocks.checked_add(blocks) else {
            return Err("Too many blocks are queued.".to_owned());
        };
        if total_blocks > config.max_queued_blocks {
            return Err(format!(
                "Too many blocks are already queued: {} queued, {blocks} new, limit is {}.",
                self.queued_blocks, config.max_queued_blocks
            ));
        }

        Ok(())
    }

    pub fn enqueue(&mut self, operation: EditOperation) {
        self.queued_blocks = self
            .queued_blocks
            .saturating_add(operation.pending_blocks());
        self.queue.push_back(operation);
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub const fn queued_blocks(&self) -> u64 {
        self.queued_blocks
    }

    pub fn process_tick(&mut self, config: &Config) {
        let budget = config.blocks_per_tick;
        if budget == 0 {
            return;
        }

        let Some(operation) = self.queue.front_mut() else {
            return;
        };

        let result = operation.process(budget, config);
        self.queued_blocks = self.queued_blocks.saturating_sub(result.scanned());
        if result.is_finished()
            && let Some(mut operation) = self.queue.pop_front()
        {
            operation.finish();
        }
    }
}

enum ProcessResult {
    Pending { scanned: usize },
    Finished { scanned: usize },
}

fn process_move(
    world: &World,
    history: &mut Vec<BlockChange>,
    move_state: &mut MoveState,
    budget: usize,
    config: &Config,
    writer: &WriteStrategy,
) -> ProcessResult {
    match move_state.phase {
        MovePhase::Snapshot => {
            let start = move_state.next_index;
            let end = (start + budget).min(move_state.positions.len());
            for index in start..end {
                let source = move_state.positions[index];
                let target = source.checked_offset(move_state.offset).unwrap_or(source);
                snapshot_block(world, move_state, writer, source);
                snapshot_block(world, move_state, writer, target);
            }
            move_state.next_index = end;
            if end < move_state.positions.len() {
                return ProcessResult::Pending {
                    scanned: end - start,
                };
            }

            move_state.cells = build_move_cells(move_state);
            move_state.phase = MovePhase::Apply;
            move_state.next_index = 0;
            if move_state.cells.is_empty() {
                return ProcessResult::Finished {
                    scanned: end - start,
                };
            }
            ProcessResult::Pending {
                scanned: end - start,
            }
        }
        MovePhase::Apply => {
            let start = move_state.next_index;
            let end = (start + budget).min(move_state.cells.len());
            for cell in &move_state.cells[start..end] {
                writer.set_block_state_with_entity_callbacks(world, cell.pos, cell.new.state);
                writer.set_block_entity(world, cell.pos, cell.new.block_entity.as_deref());
                if history.len() < config.max_history_blocks {
                    history.push(BlockChange {
                        pos: cell.pos,
                        old_state: cell.old.state,
                        new_state: cell.new.state,
                        old_block_entity: cell.old.block_entity.clone(),
                        new_block_entity: cell.new.block_entity.clone(),
                    });
                }
            }
            move_state.next_index = end;
            if end == move_state.cells.len() {
                ProcessResult::Finished {
                    scanned: end - start,
                }
            } else {
                ProcessResult::Pending {
                    scanned: end - start,
                }
            }
        }
    }
}

fn snapshot_block(
    world: &World,
    move_state: &mut MoveState,
    writer: &WriteStrategy,
    pos: BlockPos,
) {
    if move_state.snapshots.contains_key(&pos) {
        return;
    }
    move_state.snapshots.insert(
        pos,
        BlockSnapshot {
            state: writer.get_block_state_id(world, pos),
            block_entity: world.get_block_entity_nbt(pos),
        },
    );
}

fn build_move_cells(move_state: &MoveState) -> Vec<MoveCell> {
    let mut after = HashMap::with_capacity(move_state.snapshots.len());
    let mut affected = Vec::with_capacity(move_state.snapshots.len());
    let mut seen = HashSet::with_capacity(move_state.snapshots.len());

    for source in &move_state.positions {
        let target = source.checked_offset(move_state.offset).unwrap_or(*source);
        if seen.insert(*source) {
            affected.push(*source);
        }
        if seen.insert(target) {
            affected.push(target);
        }
        if let Some(snapshot) = move_state.snapshots.get(source) {
            after.insert(target, snapshot.clone());
        }
    }

    for source in &move_state.positions {
        if !after.contains_key(source) {
            after.insert(
                *source,
                BlockSnapshot {
                    state: 0,
                    block_entity: None,
                },
            );
        }
    }

    affected
        .into_iter()
        .filter_map(|pos| {
            let old = move_state.snapshots.get(&pos)?.clone();
            let new = after.get(&pos)?.clone();
            (old != new).then_some(MoveCell { pos, old, new })
        })
        .collect()
}

impl ProcessResult {
    const fn scanned(&self) -> u64 {
        match self {
            Self::Pending { scanned } | Self::Finished { scanned } => *scanned as u64,
        }
    }

    const fn is_finished(&self) -> bool {
        matches!(self, Self::Finished { .. })
    }
}

/// Replays at most `budget` history changes, restoring both block states and entity snapshots.
fn process_replay(
    world: &World,
    history: &HistoryEntry,
    direction: ReplayDirection,
    next_index: &mut usize,
    budget: usize,
    writer: &WriteStrategy,
) -> ProcessResult {
    let mut visited = 0;
    while visited < budget {
        let Some(change) = next_replay_change(history, direction, next_index) else {
            return ProcessResult::Finished { scanned: visited };
        };
        replay_change(&mut WorldAccess::new(world, writer), change, direction);
        visited += 1;
    }
    ProcessResult::Pending { scanned: visited }
}

fn next_replay_change<'a>(
    history: &'a HistoryEntry,
    direction: ReplayDirection,
    next_index: &mut usize,
) -> Option<&'a BlockChange> {
    match direction {
        ReplayDirection::Undo => {
            *next_index = next_index.checked_sub(1)?;
            history.changes.get(*next_index)
        }
        ReplayDirection::Redo => {
            let change = history.changes.get(*next_index)?;
            *next_index += 1;
            Some(change)
        }
    }
}

fn next_pattern_seed() -> u64 {
    PATTERN_SEED.fetch_add(1, Ordering::Relaxed)
}

/// Hashes signed block coordinates with an edit seed for deterministic pattern selection.
pub fn position_hash(pos: BlockPos, seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for value in [pos.x, pos.y, pos.z] {
        for byte in value.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{BlockPos, BlockSnapshot, Cuboid, MovePhase, MoveState, build_move_cells};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn cuboid_normalizes_and_counts_volume() {
        let cuboid = Cuboid::new(BlockPos { x: 2, y: 4, z: 6 }, BlockPos { x: 1, y: 3, z: 5 });

        assert_eq!(cuboid.volume(), 8);
    }

    #[test]
    fn cuboid_iterates_x_then_z_then_y() {
        let cuboid = Cuboid::new(BlockPos { x: 0, y: 0, z: 0 }, BlockPos { x: 1, y: 0, z: 1 });
        let positions: Vec<_> = cuboid.iter().collect();

        assert_eq!(
            positions,
            vec![
                BlockPos { x: 0, y: 0, z: 0 },
                BlockPos { x: 1, y: 0, z: 0 },
                BlockPos { x: 0, y: 0, z: 1 },
                BlockPos { x: 1, y: 0, z: 1 },
            ]
        );
    }

    #[test]
    fn wall_positions_only_include_vertical_faces() {
        let cuboid = Cuboid::new(pos(0, 0, 0), pos(2, 2, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 24);
        assert!(positions.contains(&pos(0, 1, 1)));
        assert!(positions.contains(&pos(2, 1, 1)));
        assert!(positions.contains(&pos(1, 1, 0)));
        assert!(positions.contains(&pos(1, 1, 2)));
        assert!(!positions.contains(&pos(1, 0, 1)));
        assert!(!positions.contains(&pos(1, 2, 1)));
        assert_unique(&positions);
    }

    #[test]
    fn wall_positions_do_not_duplicate_thin_selections() {
        let cuboid = Cuboid::new(pos(0, 0, 0), pos(0, 2, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 9);
        assert_unique(&positions);
    }

    #[test]
    fn wall_positions_include_one_block_tall_perimeter() {
        let cuboid = Cuboid::new(pos(0, 5, 0), pos(2, 5, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 8);
        assert!(positions.contains(&pos(0, 5, 0)));
        assert!(positions.contains(&pos(1, 5, 0)));
        assert!(positions.contains(&pos(2, 5, 2)));
        assert!(!positions.contains(&pos(1, 5, 1)));
        assert_unique(&positions);
    }

    /// Checks that wall counts match generated positions for thin and ordinary cuboids.
    #[test]
    fn wall_counts_match_positions_for_thin_and_normal_selections() {
        for x in 1..=4 {
            for y in 1..=4 {
                for z in 1..=4 {
                    let cuboid = Cuboid::new(pos(0, 0, 0), pos(x - 1, y - 1, z - 1));
                    assert_eq!(cuboid.wall_volume(), cuboid.wall_positions().len() as u64);
                }
            }
        }
    }

    /// Checks that extreme cuboid counts saturate instead of overflowing or allocating walls.
    #[test]
    fn huge_selections_saturate_without_overflow_or_allocating_walls() {
        let cuboid = Cuboid::new(
            pos(i32::MIN, i32::MIN, i32::MIN),
            pos(i32::MAX, i32::MAX, i32::MAX),
        );
        assert_eq!(cuboid.volume(), u64::MAX);
        assert_eq!(cuboid.wall_volume(), u64::MAX);
    }

    #[test]
    fn move_cells_preserve_overlapping_source_snapshot() {
        let positions = vec![pos(0, 0, 0), pos(1, 0, 0)];
        let mut snapshots = HashMap::new();
        snapshots.insert(
            pos(0, 0, 0),
            BlockSnapshot {
                state: 10,
                block_entity: None,
            },
        );
        snapshots.insert(
            pos(1, 0, 0),
            BlockSnapshot {
                state: 11,
                block_entity: None,
            },
        );
        snapshots.insert(
            pos(2, 0, 0),
            BlockSnapshot {
                state: 12,
                block_entity: None,
            },
        );

        let move_state = MoveState {
            positions,
            offset: pos(1, 0, 0),
            snapshots,
            cells: Vec::new(),
            phase: MovePhase::Snapshot,
            next_index: 0,
        };
        let cells = build_move_cells(&move_state);
        let result: HashMap<_, _> = cells
            .into_iter()
            .map(|cell| (cell.pos, cell.new.state))
            .collect();

        assert_eq!(result.get(&pos(0, 0, 0)), Some(&0));
        assert_eq!(result.get(&pos(1, 0, 0)), Some(&10));
        assert_eq!(result.get(&pos(2, 0, 0)), Some(&11));
    }

    fn pos(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    fn assert_unique(positions: &[BlockPos]) {
        let unique: HashSet<_> = positions.iter().copied().collect();
        assert_eq!(unique.len(), positions.len());
    }
}
