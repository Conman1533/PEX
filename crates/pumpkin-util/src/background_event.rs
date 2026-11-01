//! Timestamped events from work that runs beside the tick (chunk autosaves, region reads, memory
//! trims, player saves), so a slow tick can be matched with what else was running at the time.
//!
//! The server installs a sink when its metrics are on; without one, `emit` does nothing and its
//! detail closure never runs.

use std::sync::RwLock;
use std::time::SystemTime;

/// One background event. `at` is when the work started; `detail` holds `key=value` pairs.
pub struct BackgroundEvent {
    pub at: SystemTime,
    pub name: &'static str,
    pub detail: String,
}

type Sink = Box<dyn Fn(BackgroundEvent) + Send + Sync>;

static SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Sends every later event to `sink`, replacing the previous one.
pub fn set_sink(sink: impl Fn(BackgroundEvent) + Send + Sync + 'static) {
    *SINK
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(sink));
}

/// Whether a sink is installed, for callers that would read the clock only to emit.
#[must_use]
pub fn is_enabled() -> bool {
    SINK.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

/// Hands an event to the sink. `detail` runs only when a sink is installed.
pub fn emit(name: &'static str, at: SystemTime, detail: impl FnOnce() -> String) {
    let sink = SINK
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(sink) = sink.as_ref() {
        sink(BackgroundEvent {
            at,
            name,
            detail: detail(),
        });
    }
}
