use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleType, MemoryStatus, MemoryValue};
use super::OneShot;

/// Vanilla `CopyMemoryWithExpiry`: while the predicate holds, copies a present source memory into
/// an absent target memory that expires after `min_duration..=max_duration` ticks (vanilla's
/// `UniformInt`).
#[must_use]
pub fn create<T: MemoryValue + Clone>(
    copy_if_true: impl Fn(&BrainTick<'_>) -> bool + Send + Sync + 'static,
    source: MemoryModuleType<T>,
    target: MemoryModuleType<T>,
    min_duration: i32,
    max_duration: i32,
) -> OneShot {
    OneShot::new(
        "CopyMemoryWithExpiry",
        vec![
            (source.id(), MemoryStatus::ValuePresent),
            (target.id(), MemoryStatus::ValueAbsent),
        ],
        move |tick| {
            if !copy_if_true(tick) {
                return false;
            }
            let Some(value) = tick.state.get(source).cloned() else {
                return false;
            };
            let duration = tick
                .mob
                .get_random()
                .random_range(min_duration..=max_duration);
            tick.state
                .set_with_expiry(target, value, i64::from(duration));
            true
        },
    )
}
