use std::collections::VecDeque;

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use crate::block::BlockEvent;

/// The block events waiting to run, vanilla `ServerLevel.blockEvents`.
///
/// Like vanilla's linked hash set it keeps the order events were added in and holds each event
/// once.
#[derive(Default)]
pub struct BlockEventQueue {
    order: VecDeque<BlockEvent>,
    queued: FxHashSet<BlockEvent>,
}

impl BlockEventQueue {
    /// Adds an event to the end, unless the same event is already waiting.
    pub fn push(&mut self, event: BlockEvent) {
        if self.queued.insert(event) {
            self.order.push_back(event);
        }
    }

    /// Takes the oldest event.
    pub fn pop(&mut self) -> Option<BlockEvent> {
        let event = self.order.pop_front()?;
        self.queued.remove(&event);
        Some(event)
    }

    /// Drops the events inside the box from `min` (inclusive) to `max` (exclusive).
    pub fn clear_box(&mut self, min: &BlockPos, max: &BlockPos) {
        let inside = |pos: &BlockPos| {
            (min.0.x..max.0.x).contains(&pos.0.x)
                && (min.0.y..max.0.y).contains(&pos.0.y)
                && (min.0.z..max.0.z).contains(&pos.0.z)
        };
        self.order.retain(|event| !inside(&event.pos));
        self.queued.retain(|event| !inside(&event.pos));
    }

    #[cfg(test)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    #[cfg(test)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::Block;

    use super::*;

    fn event(x: i32, r#type: u8) -> BlockEvent {
        BlockEvent {
            pos: BlockPos::new(x, 64, 0),
            block: &Block::NOTE_BLOCK,
            r#type,
            data: 0,
        }
    }

    /// Events come out in the order they went in, and a repeat of a waiting event is dropped.
    #[test]
    fn keeps_order_and_drops_repeats() {
        let mut queue = BlockEventQueue::default();
        queue.push(event(0, 0));
        queue.push(event(1, 0));
        queue.push(event(0, 0));
        queue.push(event(0, 1));
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop(), Some(event(0, 0)));
        // Once it has run, the same event can wait again.
        queue.push(event(0, 0));
        assert_eq!(queue.pop(), Some(event(1, 0)));
        assert_eq!(queue.pop(), Some(event(0, 1)));
        assert_eq!(queue.pop(), Some(event(0, 0)));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn clear_box_drops_only_events_inside() {
        let mut queue = BlockEventQueue::default();
        for x in 0..4 {
            queue.push(event(x, 0));
        }
        queue.clear_box(&BlockPos::new(1, 0, 0), &BlockPos::new(3, 100, 1));
        assert_eq!(queue.pop(), Some(event(0, 0)));
        assert_eq!(queue.pop(), Some(event(3, 0)));
        assert!(queue.is_empty());
        queue.push(event(1, 0));
        assert_eq!(queue.len(), 1);
    }
}
