//! PEX tick metrics: slow tick reports and a per-minute summary.
//!
//! A background thread writes them to `metrics/` in the server directory so the tick loop never
//! waits on disk. The `metrics` section of the config turns it off and sets the slow tick threshold.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pumpkin_config::metrics::MetricsConfig;
use tracing::warn;

const SUMMARY_INTERVAL: Duration = Duration::from_secs(60);
/// Vanilla `SharedConstants.TICKS_PER_SECOND`.
const TICKS_PER_SECOND: usize = 20;
/// Tick samples one summary covers at the default tick rate.
const SUMMARY_WINDOW_TICKS: usize = SUMMARY_INTERVAL.as_secs() as usize * TICKS_PER_SECOND;
const LAGGY_TICK_NANOS: i64 = 50_000_000;
const CHANNEL_CAPACITY: usize = 256;

/// Phase timings of one world tick.
#[derive(Clone, Debug)]
pub struct WorldTickReport {
    pub world: &'static str,
    pub total: Duration,
    pub chunks: Duration,
    pub players: Duration,
    pub player_count: usize,
    pub entities: Duration,
    pub entity_count: usize,
    pub tracker: Duration,
    pub block_entities: Duration,
    pub block_entity_count: usize,
    pub loaded_chunks: usize,
}

struct TickSample {
    tick: i32,
    nanos: i64,
    worlds: Vec<WorldTickReport>,
}

pub struct Metrics {
    sender: Option<SyncSender<TickSample>>,
    pending_worlds: Mutex<Vec<WorldTickReport>>,
}

impl Metrics {
    #[must_use]
    pub fn new(config: &MetricsConfig) -> Self {
        if !config.enabled {
            return Self::disabled();
        }
        let slow_nanos = i64::from(config.slow_tick_ms) * 1_000_000;
        let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
        let spawned = std::thread::Builder::new()
            .name("pex-metrics".into())
            .spawn(move || writer_loop(&receiver, slow_nanos));
        if let Err(e) = spawned {
            warn!("Metrics disabled, could not start the writer thread: {e}");
            return Self::disabled();
        }
        Self {
            sender: Some(sender),
            pending_worlds: Mutex::new(Vec::new()),
        }
    }

    const fn disabled() -> Self {
        Self {
            sender: None,
            pending_worlds: Mutex::new(Vec::new()),
        }
    }

    /// Stores the world reports of the tick that is running; they go out with `record_tick`.
    pub fn set_world_reports(&self, reports: Vec<WorldTickReport>) {
        if self.sender.is_some() {
            *self
                .pending_worlds
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = reports;
        }
    }

    pub fn record_tick(&self, tick: i32, nanos: i64) {
        let Some(sender) = &self.sender else {
            return;
        };
        let worlds = std::mem::take(
            &mut *self
                .pending_worlds
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        // Dropping a sample beats stalling the tick if the writer falls behind.
        if let Err(TrySendError::Disconnected(_)) = sender.try_send(TickSample {
            tick,
            nanos,
            worlds,
        }) {
            warn!("Metrics writer stopped");
        }
    }
}

fn open_append(name: &str, header: Option<&str>) -> Option<BufWriter<File>> {
    if let Err(e) = fs::create_dir_all("metrics") {
        warn!("Could not create the metrics directory: {e}");
        return None;
    }
    let path = format!("metrics/{name}");
    let is_new = !std::path::Path::new(&path).exists();
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            let mut writer = BufWriter::new(file);
            if is_new && let Some(header) = header {
                let _ = writeln!(writer, "{header}");
                let _ = writer.flush();
            }
            Some(writer)
        }
        Err(e) => {
            warn!("Could not open {path}: {e}");
            None
        }
    }
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Resident memory of this process in MiB, or 0 where it can't be read.
fn rss_mib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kib| kib.parse::<u64>().ok())
        })
        .map_or(0, |kib| kib / 1024)
}

const fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn writer_loop(receiver: &Receiver<TickSample>, slow_nanos: i64) {
    let mut slow_log = open_append("slow-ticks.log", None);
    let mut summary = open_append(
        "summary.csv",
        Some(
            "unix_time,ticks,avg_mspt,p99_mspt,max_mspt,ticks_over_50ms,players,entities,block_entities,loaded_chunks,rss_mib",
        ),
    );
    let mut window: Vec<i64> = Vec::with_capacity(SUMMARY_WINDOW_TICKS);
    let mut last_worlds: Vec<WorldTickReport> = Vec::new();
    let mut next_summary = Instant::now() + SUMMARY_INTERVAL;

    loop {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(sample) => {
                window.push(sample.nanos);
                if sample.nanos >= slow_nanos
                    && let Some(log) = slow_log.as_mut()
                {
                    let _ = writeln!(log, "{}", slow_tick_line(&sample));
                    let _ = log.flush();
                }
                if !sample.worlds.is_empty() {
                    last_worlds = sample.worlds;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if Instant::now() >= next_summary {
            next_summary += SUMMARY_INTERVAL;
            if let Some(out) = summary.as_mut() {
                let _ = writeln!(out, "{}", summary_line(&mut window, &last_worlds));
                let _ = out.flush();
            }
            window.clear();
        }
    }
}

fn slow_tick_line(sample: &TickSample) -> String {
    let mut line = format!(
        "{} tick={} total={:.1}ms rss={}MiB",
        unix_secs(),
        sample.tick,
        sample.nanos as f64 / 1e6,
        rss_mib()
    );
    for w in &sample.worlds {
        let _ = write!(
            line,
            " | {} {:.1}ms: chunks={:.1}ms({}) players={:.1}ms({}) entities={:.1}ms({}) tracker={:.1}ms block_entities={:.1}ms({})",
            w.world,
            ms(w.total),
            ms(w.chunks),
            w.loaded_chunks,
            ms(w.players),
            w.player_count,
            ms(w.entities),
            w.entity_count,
            ms(w.tracker),
            ms(w.block_entities),
            w.block_entity_count,
        );
    }
    line
}

fn summary_line(window: &mut [i64], worlds: &[WorldTickReport]) -> String {
    window.sort_unstable();
    let ticks = window.len();
    let avg = if ticks == 0 {
        0.0
    } else {
        window.iter().sum::<i64>() as f64 / ticks as f64 / 1e6
    };
    let p99 = window
        .get((ticks * 99 / 100).min(ticks.saturating_sub(1)))
        .map_or(0.0, |n| *n as f64 / 1e6);
    let max = window.last().map_or(0.0, |n| *n as f64 / 1e6);
    let laggy = window.iter().filter(|n| **n > LAGGY_TICK_NANOS).count();
    let players: usize = worlds.iter().map(|w| w.player_count).sum();
    let entities: usize = worlds.iter().map(|w| w.entity_count).sum();
    let block_entities: usize = worlds.iter().map(|w| w.block_entity_count).sum();
    let chunks: usize = worlds.iter().map(|w| w.loaded_chunks).sum();
    format!(
        "{},{ticks},{avg:.2},{p99:.2},{max:.2},{laggy},{players},{entities},{block_entities},{chunks},{}",
        unix_secs(),
        rss_mib()
    )
}
