use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::OneShot;

/// Vanilla `BecomePassiveIfMemoryPresent`: while the pacifying memory is present, the mob forgets
/// its `attack_target` and stays `pacified` for the duration.
#[must_use]
pub fn create(pacifying_memory: MemoryModuleId, pacify_duration: i32) -> OneShot {
    OneShot::new(
        "BecomePassiveIfMemoryPresent",
        vec![
            (types::ATTACK_TARGET.id(), MemoryStatus::Registered),
            (types::PACIFIED.id(), MemoryStatus::ValueAbsent),
            (pacifying_memory, MemoryStatus::ValuePresent),
        ],
        move |tick| {
            tick.state
                .set_with_expiry(types::PACIFIED, true, i64::from(pacify_duration));
            tick.state.erase(types::ATTACK_TARGET.id());
            true
        },
    )
}
