use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::{Block, BlockDirection, BlockId, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use tracing::error;

use super::World;

/// Vanilla's default `max-chained-neighbor-updates`.
pub const DEFAULT_MAX_CHAINED_UPDATES: i32 = 1_000_000;

/// The server's `max_chained_neighbor_updates`: how many updates one cascade may submit, any
/// negative value meaning no limit (vanilla `CollectingNeighborUpdater.maxChainedNeighborUpdates`).
static MAX_CHAINED_UPDATES: AtomicI32 = AtomicI32::new(DEFAULT_MAX_CHAINED_UPDATES);

/// Sets the server's chained neighbour update limit; negative means unlimited.
pub fn set_max_chained_updates(limit: i32) {
    MAX_CHAINED_UPDATES.store(limit, Ordering::Relaxed);
}

/// The chained update limit as a budget, `None` when unlimited.
fn max_chained_updates() -> Option<usize> {
    usize::try_from(MAX_CHAINED_UPDATES.load(Ordering::Relaxed)).ok()
}

/// Vanilla `Level.setBlock`'s default `updateLimit`: how many shape updates deep a block change
/// may reach.
pub(super) const SHAPE_UPDATE_DEPTH: u32 = 512;

thread_local! {
    /// The `updateLimit` of the block change this thread is making: [`SHAPE_UPDATE_DEPTH`], or
    /// what the shape update being run carried.
    static SHAPE_UPDATE_LIMIT: Cell<u32> = const { Cell::new(SHAPE_UPDATE_DEPTH) };
}

/// The `updateLimit` of the block change this thread is making. A change with 0 left updates no
/// neighbour's shape (vanilla `Level.setBlock`).
pub(super) fn shape_update_limit() -> u32 {
    SHAPE_UPDATE_LIMIT.with(Cell::get)
}

/// Puts the previous `updateLimit` back once a shape update has run, even if it panicked.
struct ShapeLimitGuard(u32);

impl Drop for ShapeLimitGuard {
    fn drop(&mut self) {
        SHAPE_UPDATE_LIMIT.with(|limit| limit.set(self.0));
    }
}

#[derive(Clone, Copy)]
enum UpdateKind {
    Neighbor {
        position: BlockPos,
        source_position: BlockPos,
        source_block: BlockId,
        include_fluid: bool,
    },
    Shape {
        position: BlockPos,
        direction: BlockDirection,
        neighbor_position: BlockPos,
        neighbor_state_id: BlockStateId,
        flags: BlockFlags,
        /// The `updateLimit` the block change this update makes runs with.
        update_limit: u32,
    },
}

impl UpdateKind {
    const fn position(&self) -> &BlockPos {
        match self {
            Self::Neighbor { position, .. } | Self::Shape { position, .. } => position,
        }
    }
}

struct QueuedUpdate<W> {
    world: W,
    kind: UpdateKind,
}

struct UpdateStack<W> {
    remaining: Vec<QueuedUpdate<W>>,
    staged: Vec<QueuedUpdate<W>>,
    submitted: usize,
    limit: Option<usize>,
    gave_up: bool,
}

impl<W: Clone> UpdateStack<W> {
    const fn new(limit: Option<usize>) -> Self {
        Self {
            remaining: Vec::new(),
            staged: Vec::new(),
            submitted: 0,
            limit,
            gave_up: false,
        }
    }

    /// Stages one submission, which counts once however many updates it holds, like a vanilla
    /// `MultiNeighborUpdate` in `CollectingNeighborUpdater.addAndRun`.
    fn stage(&mut self, world: &W, mut kinds: impl Iterator<Item = UpdateKind>) {
        if let Some(limit) = self.limit
            && self.submitted >= limit
        {
            if !self.gave_up
                && let Some(kind) = kinds.next()
            {
                self.gave_up = true;
                error!(
                    "block updates chained beyond {limit}, updates starting at {} have been skipped",
                    kind.position()
                );
            }
            return;
        }
        self.submitted += 1;
        self.staged.extend(kinds.map(|kind| QueuedUpdate {
            world: world.clone(),
            kind,
        }));
    }

    fn promote_staged(&mut self) {
        // pass in newly submitted updates to be run before older siblings
        self.remaining.extend(self.staged.drain(..).rev());
    }
}

// allow each Rayon worker to drain its cascade
thread_local! {
    static PENDING_UPDATES: RefCell<Option<UpdateStack<Arc<World>>>> =
        const { RefCell::new(None) };
}

struct CascadeGuard;

impl Drop for CascadeGuard {
    fn drop(&mut self) {
        drop(PENDING_UPDATES.try_with(|pending| pending.borrow_mut().take()));
    }
}

pub(super) fn update_neighbors_at(
    world: &Arc<World>,
    position: &BlockPos,
    source_block: &Block,
    except: Option<BlockDirection>,
) {
    let source_block = source_block.id;
    let source_position = *position;
    submit(
        world,
        BlockDirection::update_order()
            .into_iter()
            .filter(|direction| except != Some(*direction))
            .map(|direction| UpdateKind::Neighbor {
                position: position.offset(direction.to_offset()),
                source_position,
                source_block,
                include_fluid: true,
            }),
    );
}

pub(super) fn update_neighbor(world: &Arc<World>, position: &BlockPos, source_block: &Block) {
    submit(
        world,
        std::iter::once(UpdateKind::Neighbor {
            position: *position,
            source_position: *position,
            source_block: source_block.id,
            include_fluid: false,
        }),
    );
}

pub(super) fn update_shape(
    world: &Arc<World>,
    position: &BlockPos,
    direction: BlockDirection,
    flags: BlockFlags,
) {
    let neighbor_position = position.offset(direction.to_offset());
    // captures the source state when an update is submitted
    let neighbor_state_id = world.get_block_state_id(&neighbor_position);
    submit(
        world,
        std::iter::once(UpdateKind::Shape {
            position: *position,
            direction,
            neighbor_position,
            neighbor_state_id,
            flags,
            update_limit: shape_update_limit().saturating_sub(1),
        }),
    );
}

fn cascade_in_progress() -> bool {
    PENDING_UPDATES.with(|pending| pending.borrow().is_some())
}

fn submit(world: &Arc<World>, updates: impl Iterator<Item = UpdateKind>) {
    if cascade_in_progress() {
        PENDING_UPDATES.with(|pending| {
            if let Some(stack) = pending.borrow_mut().as_mut() {
                stack.stage(world, updates);
            }
        });
        return;
    }

    let mut stack = UpdateStack::new(max_chained_updates());
    stack.stage(world, updates);
    stack.promote_staged();

    PENDING_UPDATES.with(|pending| {
        *pending.borrow_mut() = Some(stack);
    });

    let _guard = CascadeGuard;
    run_cascade();
}

fn run_cascade() {
    while let Some(update) = pop_update() {
        execute(update);
        PENDING_UPDATES.with(|pending| {
            if let Some(stack) = pending.borrow_mut().as_mut() {
                stack.promote_staged();
            }
        });
    }
}

fn pop_update() -> Option<QueuedUpdate<Arc<World>>> {
    PENDING_UPDATES.with(|pending| {
        pending
            .borrow_mut()
            .as_mut()
            .and_then(|stack| stack.remaining.pop())
    })
}

fn execute(update: QueuedUpdate<Arc<World>>) {
    let QueuedUpdate { world, kind } = update;
    match kind {
        UpdateKind::Neighbor {
            position,
            source_position,
            source_block,
            include_fluid,
        } => {
            world.execute_neighbor_update(
                &position,
                &source_position,
                source_block.to_block(),
                include_fluid,
            );
        }
        UpdateKind::Shape {
            position,
            direction,
            neighbor_position,
            neighbor_state_id,
            flags,
            update_limit,
        } => {
            let _restore =
                ShapeLimitGuard(SHAPE_UPDATE_LIMIT.with(|limit| limit.replace(update_limit)));
            world.execute_shape_update(
                &position,
                direction,
                &neighbor_position,
                neighbor_state_id,
                flags,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CascadeGuard, DEFAULT_MAX_CHAINED_UPDATES, PENDING_UPDATES, UpdateKind, UpdateStack,
        cascade_in_progress,
    };
    use pumpkin_data::{BlockDirection, BlockId};
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector3::Vector3;
    use pumpkin_world::world::BlockFlags;

    const MAX_CHAINED_UPDATES: Option<usize> = Some(DEFAULT_MAX_CHAINED_UPDATES as usize);
    const OVERWORLD: &str = "overworld";
    const NETHER: &str = "nether";

    fn neighbor_at(x: i32) -> UpdateKind {
        UpdateKind::Neighbor {
            position: BlockPos(Vector3::new(x, 0, 0)),
            source_position: BlockPos(Vector3::new(x, 0, 0)),
            source_block: BlockId::AIR,
            include_fluid: true,
        }
    }

    fn remaining_positions(stack: &UpdateStack<&'static str>) -> Vec<i32> {
        stack
            .remaining
            .iter()
            .map(|update| update.kind.position().0.x)
            .collect()
    }

    fn remaining_worlds(stack: &UpdateStack<&'static str>) -> Vec<&'static str> {
        stack.remaining.iter().map(|update| update.world).collect()
    }

    #[test]
    fn staged_updates_are_popped_in_submission_order() {
        let mut stack = UpdateStack::new(MAX_CHAINED_UPDATES);
        stack.stage(
            &OVERWORLD,
            [neighbor_at(1), neighbor_at(2), neighbor_at(3)].into_iter(),
        );
        stack.promote_staged();

        assert_eq!(remaining_positions(&stack), vec![3, 2, 1]);
        assert_eq!(
            stack
                .remaining
                .pop()
                .map(|update| update.kind.position().0.x),
            Some(1)
        );
    }

    #[test]
    fn updates_staged_while_running_are_handled_before_the_rest() {
        let mut stack = UpdateStack::new(MAX_CHAINED_UPDATES);
        stack.stage(&OVERWORLD, [neighbor_at(1), neighbor_at(2)].into_iter());
        stack.promote_staged();
        stack.remaining.pop();

        stack.stage(&OVERWORLD, [neighbor_at(10), neighbor_at(11)].into_iter());
        stack.promote_staged();

        assert_eq!(remaining_positions(&stack), vec![2, 11, 10]);
    }

    #[test]
    fn updates_stay_bound_to_the_world_that_staged_them() {
        let mut stack = UpdateStack::new(MAX_CHAINED_UPDATES);
        stack.stage(&OVERWORLD, [neighbor_at(1), neighbor_at(2)].into_iter());
        stack.promote_staged();

        assert_eq!(
            stack
                .remaining
                .pop()
                .map(|update| (update.world, update.kind.position().0.x)),
            Some((OVERWORLD, 1)),
        );

        stack.stage(&NETHER, std::iter::once(neighbor_at(10)));
        stack.promote_staged();

        assert_eq!(remaining_positions(&stack), vec![2, 10]);
        assert_eq!(remaining_worlds(&stack), vec![OVERWORLD, NETHER]);
    }

    #[test]
    fn updates_chained_past_the_budget_are_skipped() {
        let mut stack = UpdateStack::new(Some(2));
        stack.stage(&OVERWORLD, std::iter::once(neighbor_at(1)));
        stack.stage(&OVERWORLD, std::iter::once(neighbor_at(2)));
        stack.stage(
            &OVERWORLD,
            std::iter::once(UpdateKind::Shape {
                position: BlockPos(Vector3::new(4, 0, 0)),
                direction: BlockDirection::Up,
                neighbor_position: BlockPos(Vector3::new(4, 1, 0)),
                neighbor_state_id: pumpkin_data::BlockStateId::AIR,
                flags: BlockFlags::NOTIFY_ALL,
                update_limit: 511,
            }),
        );
        stack.promote_staged();

        assert!(stack.gave_up);
        assert_eq!(remaining_positions(&stack), vec![2, 1]);
    }

    /// G5-14: the six updates around a changed block are one submission, so they count once
    /// against the limit (vanilla `CollectingNeighborUpdater.addAndRun`).
    #[test]
    fn multi_neighbor_update_counts_once() {
        let mut stack = UpdateStack::new(Some(2));
        stack.stage(&OVERWORLD, (1..=6).map(neighbor_at));
        stack.stage(&OVERWORLD, std::iter::once(neighbor_at(7)));
        stack.promote_staged();

        assert!(!stack.gave_up);
        assert_eq!(remaining_positions(&stack), vec![7, 6, 5, 4, 3, 2, 1]);

        stack.stage(&OVERWORLD, std::iter::once(neighbor_at(8)));
        assert!(stack.gave_up);
    }

    /// G5-14: a negative limit means no limit.
    #[test]
    fn an_unlimited_stack_never_gives_up() {
        let mut stack = UpdateStack::new(None);
        for x in 0..10_000 {
            stack.stage(&OVERWORLD, std::iter::once(neighbor_at(x)));
        }
        assert!(!stack.gave_up);
        assert_eq!(stack.staged.len(), 10_000);
    }

    /// G5-14: a block change reaches 512 shape updates deep and no further (vanilla
    /// `Level.setBlock` `updateLimit`). Each portal block in a frameless row turns to air when its
    /// neighbour along the axis goes, so breaking the first one removes the next 512 only.
    #[tokio::test]
    async fn shape_updates_stop_at_depth_512() {
        use pumpkin_data::Block;
        use pumpkin_data::block_properties::{HorizontalAxis, NetherPortalLikeProperties};

        let world = crate::world::test_world_with_blocks();
        for chunk_x in 0..40 {
            world.level.loaded_chunks.insert(
                pumpkin_util::math::vector2::Vector2::new(chunk_x, 0),
                pumpkin_world::chunk::ChunkData::empty_sync(chunk_x, 0),
            );
        }
        let mut props = NetherPortalLikeProperties::default(&Block::NETHER_PORTAL);
        props.axis = HorizontalAxis::X;
        let portal = props.to_state_id(&Block::NETHER_PORTAL);
        let at = |x: i32| BlockPos(Vector3::new(x, 64, 4));
        for x in 0..600 {
            world.set_block_state(
                &at(x),
                portal,
                BlockFlags::UPDATE_KNOWN_SHAPE | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK,
            );
        }

        world.set_block_state(&at(0), Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);

        assert!((1..=512).all(|x| world.get_block_state_id(&at(x)) != portal));
        assert_eq!(world.get_block_state_id(&at(513)), portal);
    }

    #[test]
    #[expect(clippy::panic, reason = "Checks that unwinding resets the cascade")]
    fn panic_does_not_leave_a_cascade_running() {
        let result = std::panic::catch_unwind(|| {
            PENDING_UPDATES.with(|pending| {
                *pending.borrow_mut() = Some(UpdateStack::new(MAX_CHAINED_UPDATES));
            });
            let _guard = CascadeGuard;
            panic!("block callback panicked");
        });
        assert!(result.is_err());
        assert!(!cascade_in_progress());
    }
}
