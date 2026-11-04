use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use crate::tick::{MAX_TICK_DELAY, OrderedTick, ScheduledTick};

pub struct ChunkTickScheduler<T> {
    inner: Mutex<Option<Box<ChunkTickSchedulerInner<T>>>>,
    offset: AtomicUsize,
}

struct ChunkTickSchedulerInner<T> {
    tick_queue: [Vec<OrderedTick<T>>; MAX_TICK_DELAY],
    queued_ticks: FxHashSet<(BlockPos, T)>,
}

impl<'a, T: std::hash::Hash + Eq> ChunkTickScheduler<&'a T> {
    pub fn step_tick(&self) -> Vec<OrderedTick<&'a T>> {
        // The offset only changes under `inner`, so `schedule_tick` can't pick a slot that was
        // just drained.
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current_offset = self.offset.fetch_add(1, Ordering::SeqCst) % MAX_TICK_DELAY;
        let next_offset = (current_offset + 1) % MAX_TICK_DELAY;
        self.offset.store(next_offset, Ordering::SeqCst);

        let Some(inner) = inner_guard.as_mut() else {
            return Vec::new();
        };

        let res = std::mem::take(&mut inner.tick_queue[current_offset]);

        if !res.is_empty() {
            for next_tick in &res {
                inner
                    .queued_ticks
                    .remove(&(next_tick.position, next_tick.value));
            }
            if inner.queued_ticks.is_empty() {
                *inner_guard = None;
            }
        }
        res
    }

    /// Passes a tick without running it, for a chunk that isn't simulated. Vanilla keeps the
    /// absolute trigger tick, so the ticks due now carry over and run with the next slot's.
    pub fn skip_tick(&self) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current_offset = self.offset.load(Ordering::SeqCst);
        let next_offset = (current_offset + 1) % MAX_TICK_DELAY;
        self.offset.store(next_offset, Ordering::SeqCst);

        if let Some(inner) = inner_guard.as_mut() {
            let mut overdue = std::mem::take(&mut inner.tick_queue[current_offset]);
            if !overdue.is_empty() {
                for tick in &mut overdue {
                    tick.overdue += 1;
                }
                overdue.append(&mut inner.tick_queue[next_offset]);
                inner.tick_queue[next_offset] = overdue;
            }
        }
    }

    pub fn schedule_tick(&self, tick: &ScheduledTick<&'a T>, sub_tick_order: i64) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let offset = self.offset.load(Ordering::SeqCst);
        let inner = inner_guard.get_or_insert_with(|| {
            Box::new(ChunkTickSchedulerInner {
                tick_queue: std::array::from_fn(|_| Vec::new()),
                queued_ticks: FxHashSet::default(),
            })
        });

        if inner.queued_ticks.insert((tick.position, tick.value)) {
            // `offset` is the queue the next `step_tick` drains, so a delay of N lands N - 1 slots
            // ahead. Vanilla runs a delay 0 tick on the next tick too.
            let index = (offset + (tick.delay as usize).max(1) - 1) % MAX_TICK_DELAY;

            inner.tick_queue[index].push(OrderedTick {
                overdue: 0,
                priority: tick.priority,
                sub_tick_order,
                position: tick.position,
                value: tick.value,
            });
        }
    }

    /// Puts back ticks that `step_tick` returned but that didn't run because the game tick hit
    /// `MAX_TICKS_PER_GAME_TICK`. Vanilla never took them out, so they keep their order and are
    /// overdue in the next game tick.
    pub fn carry_over(&self, ticks: impl IntoIterator<Item = OrderedTick<&'a T>>) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let offset = self.offset.load(Ordering::SeqCst);
        let inner = inner_guard.get_or_insert_with(|| {
            Box::new(ChunkTickSchedulerInner {
                tick_queue: std::array::from_fn(|_| Vec::new()),
                queued_ticks: FxHashSet::default(),
            })
        });
        for mut tick in ticks {
            if inner.queued_ticks.insert((tick.position, tick.value)) {
                tick.overdue += 1;
                inner.tick_queue[offset].push(tick);
            }
        }
        if inner.queued_ticks.is_empty() {
            *inner_guard = None;
        }
    }

    pub fn is_scheduled(&self, pos: BlockPos, value: &T) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| inner.queued_ticks.contains(&(pos, value)))
    }

    pub fn clear_area(&self, min: &BlockPos, max: &BlockPos) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return;
        };

        let contains = |position: &BlockPos| {
            position.0.x >= min.0.x
                && position.0.x < max.0.x
                && position.0.y >= min.0.y
                && position.0.y < max.0.y
                && position.0.z >= min.0.z
                && position.0.z < max.0.z
        };

        for queue in &mut inner.tick_queue {
            queue.retain(|tick| !contains(&tick.position));
        }
        inner
            .queued_ticks
            .retain(|(position, _)| !contains(position));
        let became_empty = inner.queued_ticks.is_empty();

        if became_empty {
            *inner_guard = None;
        }
    }

    pub fn has_ticks(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| !inner.queued_ticks.is_empty())
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<ScheduledTick<&'a T>> {
        let inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let offset = self.offset.load(Ordering::SeqCst);
        let Some(inner) = inner_guard.as_ref() else {
            return Vec::new();
        };

        let mut res = Vec::new();

        for i in 0..MAX_TICK_DELAY {
            let index = (offset + i) % MAX_TICK_DELAY;
            // Inverse of `schedule_tick`: the queue at `offset` runs next tick, i.e. delay 1.
            // The last slot is never filled because delay is a u8.
            res.extend(inner.tick_queue[index].iter().map(|x| {
                (
                    x.sub_tick_order,
                    ScheduledTick {
                        delay: (i + 1) as u8,
                        priority: x.priority,
                        position: x.position,
                        value: x.value,
                    },
                )
            }));
        }
        // Vanilla `LevelChunkTicks.pack` saves in sub-tick order, which loading numbers again.
        res.sort_by_key(|(sub_tick_order, _)| *sub_tick_order);
        res.into_iter().map(|(_, tick)| tick).collect()
    }
}

impl<'a, T: std::hash::Hash + Eq + 'static> FromIterator<ScheduledTick<&'a T>>
    for ChunkTickScheduler<&'a T>
{
    /// Vanilla `LevelChunkTicks.unpack`: loaded ticks keep their saved order, numbered from `-n`
    /// so that they run before ticks scheduled later for the same game tick.
    fn from_iter<I: IntoIterator<Item = ScheduledTick<&'a T>>>(iter: I) -> Self {
        let scheduler = Self::default();
        let ticks: Vec<_> = iter.into_iter().collect();

        let lower = ticks.len();
        if lower > 0 {
            let mut inner_guard = scheduler
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let inner = inner_guard.get_or_insert_with(|| {
                Box::new(ChunkTickSchedulerInner {
                    tick_queue: std::array::from_fn(|_| Vec::new()),
                    queued_ticks: FxHashSet::default(),
                })
            });
            inner.queued_ticks.reserve(lower);
        }

        let first = -i64::try_from(lower).unwrap_or(i64::MAX);
        for (sub_tick_order, tick) in (first..).zip(ticks) {
            scheduler.schedule_tick(&tick, sub_tick_order);
        }
        scheduler
    }
}

impl<T> Default for ChunkTickScheduler<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            offset: AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::{MAX_TICKS_PER_GAME_TICK, TickPriority, sort_and_cap};

    static BLOCK: u8 = 0;

    fn tick(delay: u8) -> ScheduledTick<&'static u8> {
        tick_at(delay, 0)
    }

    fn tick_at(delay: u8, x: i32) -> ScheduledTick<&'static u8> {
        ScheduledTick {
            delay,
            priority: TickPriority::Normal,
            position: BlockPos::new(x, 0, 0),
            value: &BLOCK,
        }
    }

    fn xs(ticks: &[OrderedTick<&u8>]) -> Vec<i32> {
        ticks.iter().map(|tick| tick.position.0.x).collect()
    }

    #[test]
    fn loaded_ticks_keep_saved_order() {
        // Vanilla `LevelChunkTicks.unpack` numbers saved ticks from `-n` in list order, so they
        // run in saved order and before ticks scheduled after loading for the same game tick.
        let saved: Vec<i32> = (0..64).map(|i| (i * 37) % 64).collect();
        let scheduler: ChunkTickScheduler<&u8> = saved.iter().map(|&x| tick_at(1, x)).collect();
        for order in (0..64).rev() {
            scheduler.schedule_tick(&tick_at(1, 100 + order), i64::from(order));
        }
        let mut ran = scheduler.step_tick();
        ran.sort_unstable();
        let later: Vec<i32> = (100..164).collect();
        assert_eq!(xs(&ran), [saved, later].concat());
    }

    #[test]
    fn saved_order_round_trips() {
        let scheduler: ChunkTickScheduler<&u8> = [tick_at(2, 3), tick_at(1, 1), tick_at(2, 4)]
            .into_iter()
            .collect();
        let saved: Vec<_> = scheduler
            .to_vec()
            .iter()
            .map(|tick| tick.position.0.x)
            .collect();
        assert_eq!(saved, [3, 1, 4]);
    }

    #[test]
    fn overdue_ticks_run_first() {
        // Vanilla drains by trigger tick before priority, so a tick that came due while the chunk
        // wasn't simulated runs before a higher-priority tick due now.
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick_at(1, 1), 0);
        scheduler.skip_tick();
        scheduler.schedule_tick(
            &ScheduledTick {
                priority: TickPriority::ExtremelyHigh,
                ..tick_at(1, 2)
            },
            1,
        );
        let mut ran = scheduler.step_tick();
        ran.sort_unstable();
        assert_eq!(xs(&ran), [1, 2]);
    }

    #[test]
    fn ticks_over_cap_run_next_tick() {
        // Vanilla runs at most 65536 block ticks a game tick; the rest stay scheduled and run
        // first in the next one.
        let extra = 3;
        let scheduler = ChunkTickScheduler::default();
        for x in 0..(MAX_TICKS_PER_GAME_TICK + extra) {
            let x = i32::try_from(x).unwrap();
            scheduler.schedule_tick(&tick_at(1, x), i64::from(x));
        }
        let mut ran = scheduler.step_tick();
        let left = sort_and_cap(&mut ran);
        assert_eq!(ran.len(), MAX_TICKS_PER_GAME_TICK);
        assert_eq!(left.len(), extra);
        scheduler.carry_over(left);
        assert!(scheduler.is_scheduled(BlockPos::new(65536, 0, 0), &BLOCK));

        scheduler.schedule_tick(
            &ScheduledTick {
                priority: TickPriority::ExtremelyHigh,
                ..tick_at(1, -1)
            },
            -1,
        );
        let mut next = scheduler.step_tick();
        assert!(sort_and_cap(&mut next).is_empty());
        assert_eq!(xs(&next), [65536, 65537, 65538, -1]);
    }

    #[test]
    fn delay_counts_game_ticks_like_vanilla() {
        // Vanilla: scheduled at game time T with delay 2, it runs at T + 2.
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(2), 0);
        assert!(scheduler.step_tick().is_empty());
        assert_eq!(scheduler.step_tick().len(), 1);
    }

    #[test]
    fn overdue_ticks_run_on_the_first_tick_after_a_skip() {
        // Vanilla keeps the trigger tick while a chunk isn't simulated, so a tick that came due
        // during the skip runs as soon as the chunk ticks again, not `delay` ticks later.
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(3), 0);
        for _ in 0..10 {
            scheduler.skip_tick();
        }
        assert_eq!(scheduler.step_tick().len(), 1);
        assert!(!scheduler.has_ticks());
    }

    #[test]
    fn saved_delay_round_trips() {
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(5), 0);
        scheduler.step_tick();
        assert_eq!(scheduler.to_vec()[0].delay, 4);
    }
}
