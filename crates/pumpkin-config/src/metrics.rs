use serde::{Deserialize, Serialize};

/// Configuration for the tick metrics written to `metrics/` in the server directory.
#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Whether slow tick reports and the per-minute summary are written.
    pub enabled: bool,
    /// A tick that takes at least this many milliseconds is reported in `slow-ticks.log`.
    pub slow_tick_ms: u32,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            slow_tick_ms: 100,
        }
    }
}
