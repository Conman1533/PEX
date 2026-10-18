// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{BlockChange, BlockPos, ReplayDirection};
use crate::{blocks::has_block_entity, config::Config, host::World};
use pumpkin_world::world::BlockFlags;
use std::{cell::RefCell, collections::HashMap};

pub(super) struct WriteStrategy {
    entity_states: RefCell<HashMap<u16, bool>>,
    fast_flags: BlockFlags,
    entity_callback_flags: BlockFlags,
}

/// The edit/history logic also runs against an in-memory world in regression tests.
pub(super) trait BlockAccess {
    /// Reads the current block state at a position.
    fn state(&mut self, pos: BlockPos) -> u16;
    /// Reports whether the state supports a block entity.
    fn has_entity(&self, state: u16) -> bool;
    /// Reads serialized block-entity NBT when an entity exists at the position.
    fn entity(&self, pos: BlockPos) -> Option<Vec<u8>>;
    /// Writes a state, enabling entity lifecycle callbacks when requested.
    fn write_state(&mut self, pos: BlockPos, state: u16, entity_callbacks: bool);
    /// Restores the supplied entity NBT; `None` leaves cleanup to the state write.
    fn write_entity(&mut self, pos: BlockPos, nbt: Option<&[u8]>);
}

pub(super) struct WorldAccess<'a> {
    world: &'a World,
    writer: &'a WriteStrategy,
}

impl<'a> WorldAccess<'a> {
    pub(super) const fn new(world: &'a World, writer: &'a WriteStrategy) -> Self {
        Self { world, writer }
    }
}

impl BlockAccess for WorldAccess<'_> {
    fn state(&mut self, pos: BlockPos) -> u16 {
        self.writer.get_block_state_id(self.world, pos)
    }

    /// Checks and caches whether the state has a block-entity type.
    fn has_entity(&self, state: u16) -> bool {
        if let Some(has_entity) = self.writer.entity_states.borrow().get(&state) {
            return *has_entity;
        }
        let has_entity = has_block_entity(state);
        self.writer
            .entity_states
            .borrow_mut()
            .insert(state, has_entity);
        has_entity
    }

    fn entity(&self, pos: BlockPos) -> Option<Vec<u8>> {
        self.world.get_block_entity_nbt(pos)
    }

    /// Writes a state through entity callbacks when needed, otherwise using the configured fast path.
    fn write_state(&mut self, pos: BlockPos, state: u16, entity_callbacks: bool) {
        if entity_callbacks {
            self.writer
                .set_block_state_with_entity_callbacks(self.world, pos, state);
        } else {
            self.writer.set_block_state(self.world, pos, state);
        }
    }

    fn write_entity(&mut self, pos: BlockPos, nbt: Option<&[u8]>) {
        self.writer.set_block_entity(self.world, pos, nbt);
    }
}

/// Applies a state change unless the replacement filter misses or the state is unchanged.
///
/// Returns old and new state/entity snapshots only when history recording is enabled.
/// A `None` return can therefore still mean a write occurred without history.
pub(super) fn apply_forward(
    access: &mut impl BlockAccess,
    pos: BlockPos,
    to: u16,
    replace_from: Option<u16>,
    record_history: bool,
) -> Option<BlockChange> {
    let old_state = access.state(pos);
    if replace_from.is_some_and(|from| from != old_state) || old_state == to {
        return None;
    }
    let old_has_entity = access.has_entity(old_state);
    let new_has_entity = access.has_entity(to);
    let old_block_entity = if record_history && old_has_entity {
        access.entity(pos)
    } else {
        None
    };
    access.write_state(pos, to, old_has_entity || new_has_entity);
    if !record_history {
        return None;
    }
    let new_block_entity = if new_has_entity {
        access.entity(pos)
    } else {
        None
    };
    Some(BlockChange {
        pos,
        old_state,
        new_state: to,
        old_block_entity,
        new_block_entity,
    })
}

/// Restores the state and optional entity snapshot selected by the replay direction.
///
/// Enables entity callbacks when either state or saved snapshot requires them.
pub(super) fn replay_change(
    access: &mut impl BlockAccess,
    change: &BlockChange,
    direction: ReplayDirection,
) {
    let (state, entity) = match direction {
        ReplayDirection::Undo => (change.old_state, change.old_block_entity.as_deref()),
        ReplayDirection::Redo => (change.new_state, change.new_block_entity.as_deref()),
    };
    let current_state = access.state(change.pos);
    let callbacks = access.has_entity(current_state)
        || access.has_entity(state)
        || change.old_block_entity.is_some()
        || change.new_block_entity.is_some();
    access.write_state(change.pos, state, callbacks);
    access.write_entity(change.pos, entity);
}

impl WriteStrategy {
    pub(super) fn new(config: &Config) -> Self {
        Self {
            entity_states: RefCell::new(HashMap::new()),
            fast_flags: block_flags(config),
            entity_callback_flags: block_entity_flags(config),
        }
    }

    #[expect(
        clippy::unused_self,
        reason = "kept on the strategy like the other world accessors"
    )]
    pub(super) fn get_block_state_id(&self, world: &World, pos: BlockPos) -> u16 {
        world.get_block_state_id(pos)
    }

    // Natively there's no Wasm boundary to amortize, so every write goes through the world;
    // fast mode is expressed purely through the flags.
    pub(super) fn set_block_state(&self, world: &World, pos: BlockPos, state: u16) {
        world.set_block_state(pos, state, self.fast_flags);
    }

    /// Restores supplied entity NBT; `None` performs no entity write.
    #[expect(
        clippy::unused_self,
        reason = "kept on the strategy like the other world accessors"
    )]
    pub(super) fn set_block_entity(&self, world: &World, pos: BlockPos, nbt: Option<&[u8]>) {
        let Some(nbt) = nbt else {
            return;
        };
        if let Err(error) = world.set_block_entity_nbt(pos, nbt) {
            tracing::warn!("WorldPumpkin: failed to restore block entity at {pos:?}: {error}");
        }
    }

    /// Writes with entity lifecycle flags that suppress replacement drops.
    pub(super) fn set_block_state_with_entity_callbacks(
        &self,
        world: &World,
        pos: BlockPos,
        state: u16,
    ) {
        world.set_block_state(pos, state, self.entity_callback_flags);
    }
}

/// Builds ordinary write flags from client-notification and fast-mode settings.
fn block_flags(config: &Config) -> BlockFlags {
    let mut flags = BlockFlags::empty();

    if config.notify_clients {
        flags |= BlockFlags::NOTIFY_LISTENERS;
    }

    if config.fast_mode {
        // Keep fast edits free of physics as far as Pumpkin allows.
        flags |= BlockFlags::SKIP_DROPS
            | BlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT
            | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
            | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
    } else {
        flags |= BlockFlags::NOTIFY_NEIGHBORS;
    }

    flags
}

/// Builds entity write flags that allow creation while suppressing inventory-dropping replacement callbacks.
fn block_entity_flags(config: &Config) -> BlockFlags {
    // Container replacement callbacks drop inventory items, which undo would duplicate.
    let mut flags = BlockFlags::FORCE_STATE | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK;

    if config.notify_clients {
        flags |= BlockFlags::NOTIFY_LISTENERS;
    }

    if config.fast_mode {
        flags |= BlockFlags::SKIP_DROPS | BlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT;
    } else {
        flags |= BlockFlags::NOTIFY_NEIGHBORS;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BlockSnapshot;

    const POS: BlockPos = BlockPos { x: 1, y: 64, z: 2 };
    const STONE: u16 = 1;
    const CHEST: u16 = 2;
    const FURNACE: u16 = 3;

    struct WorldStub {
        block: BlockSnapshot,
        callback_writes: Vec<bool>,
        entity_reads: std::cell::Cell<usize>,
    }

    impl WorldStub {
        /// Creates a single-block test world with initial NBT and empty access counters.
        fn new(state: u16, block_entity: Option<Vec<u8>>) -> Self {
            Self {
                block: BlockSnapshot {
                    state,
                    block_entity,
                },
                callback_writes: vec![],
                entity_reads: std::cell::Cell::new(0),
            }
        }
    }

    impl BlockAccess for WorldStub {
        /// Reads the single state stored by the test world.
        fn state(&mut self, _: BlockPos) -> u16 {
            self.block.state
        }
        /// Treats only the chest and furnace fixture states as entity-bearing blocks.
        fn has_entity(&self, state: u16) -> bool {
            matches!(state, CHEST | FURNACE)
        }
        /// Counts the entity read and clones the stored test NBT.
        fn entity(&self, _: BlockPos) -> Option<Vec<u8>> {
            self.entity_reads.set(self.entity_reads.get() + 1);
            self.block.block_entity.clone()
        }
        /// Records callback usage and simulates entity creation or removal when the state changes.
        fn write_state(&mut self, _: BlockPos, state: u16, callbacks: bool) {
            self.callback_writes.push(callbacks);
            if callbacks && self.block.state != state {
                self.block.block_entity = self.has_entity(state).then(|| vec![0]);
            }
            self.block.state = state;
        }
        /// Replaces the test entity snapshot when NBT is supplied, leaving `None` untouched.
        fn write_entity(&mut self, _: BlockPos, nbt: Option<&[u8]>) {
            if let Some(nbt) = nbt {
                self.block.block_entity = Some(nbt.to_vec());
            }
        }
    }

    /// Checks that undo restores chest inventory and redo removes it using entity callbacks.
    #[test]
    fn set_undo_restores_inventory_and_redo_removes_it() {
        let inventory = vec![1, 7, 42];
        let mut world = WorldStub::new(CHEST, Some(inventory.clone()));
        let change = apply_forward(&mut world, POS, STONE, None, true).unwrap();
        assert_eq!(world.block.block_entity, None);
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.state, CHEST);
        assert_eq!(world.block.block_entity, Some(inventory));
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: STONE,
                block_entity: None
            }
        );
        assert!(world.callback_writes.iter().all(|callbacks| *callbacks));
    }

    /// Checks that replacing one entity block with another records and replays both NBT snapshots.
    #[test]
    fn replace_keeps_both_entity_snapshots() {
        let mut world = WorldStub::new(CHEST, Some(vec![1, 7]));
        let change = apply_forward(&mut world, POS, FURNACE, Some(CHEST), true).unwrap();
        assert_eq!(change.old_block_entity, Some(vec![1, 7]));
        assert_eq!(change.new_block_entity, Some(vec![0]));
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.block_entity, Some(vec![1, 7]));
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: FURNACE,
                block_entity: Some(vec![0])
            }
        );
    }

    /// Checks that placing an entity captures its new NBT for redo after undo removes it.
    #[test]
    fn placing_an_entity_records_the_new_nbt_for_redo() {
        let mut world = WorldStub::new(STONE, None);
        let change = apply_forward(&mut world, POS, CHEST, None, true).unwrap();
        assert_eq!(change.new_block_entity, Some(vec![0]));
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.block_entity, None);
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(world.block.block_entity, Some(vec![0]));
    }

    /// Checks that unchanged states and replacement-filter misses preserve inventory without writes.
    #[test]
    fn unchanged_blocks_and_replace_misses_keep_inventory() {
        let mut world = WorldStub::new(CHEST, Some(vec![42]));
        assert!(apply_forward(&mut world, POS, CHEST, None, true).is_none());
        assert!(apply_forward(&mut world, POS, STONE, Some(FURNACE), true).is_none());
        assert_eq!(world.block.block_entity, Some(vec![42]));
        assert!(world.callback_writes.is_empty());
    }

    /// Checks that disabling history recording still applies the requested block edit.
    #[test]
    fn edits_still_run_when_history_is_full() {
        let mut world = WorldStub::new(CHEST, Some(vec![42]));
        assert!(apply_forward(&mut world, POS, STONE, None, false).is_none());
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: STONE,
                block_entity: None
            }
        );
    }

    /// Checks that ordinary states use direct writes without reading entity NBT.
    #[test]
    fn ordinary_blocks_keep_the_direct_write_path() {
        let mut world = WorldStub::new(STONE, None);
        apply_forward(&mut world, POS, 4, None, true).unwrap();
        assert_eq!(world.callback_writes, [false]);
        assert_eq!(world.entity_reads.get(), 0);
    }

    /// Checks that both write modes suppress inventory drops while permitting entity creation callbacks.
    #[test]
    fn entity_writes_do_not_drop_inventory_in_either_mode() {
        for fast_mode in [true, false] {
            let flags = block_entity_flags(&Config {
                fast_mode,
                ..Config::default()
            });
            assert_ne!(
                (flags & BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK).bits(),
                0
            );
            assert_eq!((flags & BlockFlags::SKIP_BLOCK_ADDED_CALLBACK).bits(), 0);
        }
    }
}
