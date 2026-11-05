//! PEX tick metrics: slow tick reports, a per-minute summary and background events.
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
use pumpkin_util::background_event::{self, BackgroundEvent};
use tracing::warn;

const SUMMARY_INTERVAL: Duration = Duration::from_secs(60);
/// Vanilla `SharedConstants.TICKS_PER_SECOND`.
const TICKS_PER_SECOND: usize = 20;
/// Tick samples one summary covers at the default tick rate.
const SUMMARY_WINDOW_TICKS: usize = SUMMARY_INTERVAL.as_secs() as usize * TICKS_PER_SECOND;
const LAGGY_TICK_NANOS: i64 = 50_000_000;
const CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 1024;
/// How long the writer waits for a tick before it writes the events that came in meanwhile.
const EVENT_POLL: Duration = Duration::from_millis(250);

/// Phase timings of one world tick, in the order `World::tick` runs them.
#[derive(Clone, Debug, Default)]
pub struct WorldTickReport {
    pub world: &'static str,
    /// From the start of the server's world phase until this world's tick began, which is time
    /// spent waiting for a Rayon worker.
    pub queued: Duration,
    pub total: Duration,
    /// `flush_block_updates` and `flush_synced_block_events`.
    pub block_updates: Duration,
    pub active_chunks: Duration,
    /// `tick_environment`: time, weather, spawning, and the autosave and memory cleanup triggers.
    pub environment: Duration,
    pub raids: Duration,
    pub chunks: Duration,
    /// Player ticks and the pickup boxes taken after them.
    pub players: Duration,
    pub player_count: usize,
    /// Island planning, which `entities` does not include.
    pub islands: Duration,
    pub entities: Duration,
    pub entity_count: usize,
    pub tracker: Duration,
    pub block_entities: Duration,
    pub block_entity_count: usize,
    /// `ChunkLoading::send_change`.
    pub chunk_tickets: Duration,
    pub dragon_fight: Duration,
    pub loaded_chunks: usize,
}

impl WorldTickReport {
    fn phases(&self) -> Duration {
        self.block_updates
            + self.active_chunks
            + self.environment
            + self.raids
            + self.chunks
            + self.players
            + self.islands
            + self.entities
            + self.tracker
            + self.block_entities
            + self.chunk_tickets
            + self.dragon_fight
    }
}

/// Phase timings of `Server::tick`, in order. A phase that didn't run this tick stays zero.
#[derive(Clone, Debug, Default)]
pub struct ServerTickReport {
    pub suspend_flushes: Duration,
    /// The `#minecraft:tick` functions.
    pub tick_functions: Duration,
    pub plugin_loaders: Duration,
    pub scheduled_functions: Duration,
    pub time_sync: Duration,
    /// All worlds, ticked in parallel. Each world's own phases are in `worlds`.
    pub world_ticks: Duration,
    pub player_data: Duration,
    /// `tick_players_and_network`, which runs instead of the world ticks while the game is frozen.
    pub frozen_players: Duration,
    pub block_flush: Duration,
    pub resume_flushes: Duration,
    pub worlds: Vec<WorldTickReport>,
}

impl ServerTickReport {
    fn phases(&self) -> Duration {
        self.suspend_flushes
            + self.tick_functions
            + self.plugin_loaders
            + self.scheduled_functions
            + self.time_sync
            + self.world_ticks
            + self.player_data
            + self.frozen_players
            + self.block_flush
            + self.resume_flushes
    }
}

/// Phases of `Ticker::run` around `Server::tick`.
#[derive(Clone, Copy, Debug)]
pub struct TickLoopReport {
    pub started: SystemTime,
    /// Overload check, `TickRateManager::tick` and the sprint bookkeeping.
    pub rate_manager: Duration,
    pub start_event: Duration,
    pub game_tests: Duration,
    /// `ServerTickEndEvent` runs after the tick time is taken, so it is not part of the total.
    pub end_event: Duration,
}

impl TickLoopReport {
    #[must_use]
    pub const fn new(started: SystemTime) -> Self {
        Self {
            started,
            rate_manager: Duration::ZERO,
            start_event: Duration::ZERO,
            game_tests: Duration::ZERO,
            end_event: Duration::ZERO,
        }
    }

    fn phases(&self) -> Duration {
        self.rate_manager + self.start_event + self.game_tests
    }
}

struct TickSample {
    tick: i32,
    nanos: i64,
    tick_loop: TickLoopReport,
    server: ServerTickReport,
}

impl TickSample {
    /// Tick time outside every named server and loop phase.
    fn unattributed(&self) -> Duration {
        Duration::from_nanos(self.nanos.max(0) as u64)
            .saturating_sub(self.tick_loop.phases() + self.server.phases())
    }
}

pub struct Metrics {
    sender: Option<SyncSender<TickSample>>,
    pending: Mutex<ServerTickReport>,
}

impl Metrics {
    #[must_use]
    pub fn new(config: &MetricsConfig) -> Self {
        if !config.enabled {
            return Self::disabled();
        }
        let slow_nanos = i64::from(config.slow_tick_ms) * 1_000_000;
        let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
        let (event_sender, event_receiver) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let spawned = std::thread::Builder::new()
            .name("pex-metrics".into())
            .spawn(move || writer_loop(&receiver, &event_receiver, slow_nanos));
        if let Err(e) = spawned {
            warn!("Metrics disabled, could not start the writer thread: {e}");
            return Self::disabled();
        }
        background_event::set_sink(move |event| {
            // Dropped when the writer falls behind, like tick samples.
            let _ = event_sender.try_send(event);
        });
        Self {
            sender: Some(sender),
            pending: Mutex::new(ServerTickReport::default()),
        }
    }

    fn disabled() -> Self {
        Self {
            sender: None,
            pending: Mutex::new(ServerTickReport::default()),
        }
    }

    /// Stores the report of the server tick that is running; it goes out with `record_tick`.
    pub fn set_server_report(&self, report: ServerTickReport) {
        if self.sender.is_some() {
            *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = report;
        }
    }

    pub fn record_tick(&self, tick: i32, nanos: i64, tick_loop: TickLoopReport) {
        let Some(sender) = &self.sender else {
            return;
        };
        let server = std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        // Dropping a sample beats stalling the tick if the writer falls behind.
        if let Err(TrySendError::Disconnected(_)) = sender.try_send(TickSample {
            tick,
            nanos,
            tick_loop,
            server,
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

/// Unix time in seconds with millisecond digits, to line slow ticks up with events.
fn unix_millis_text(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03}", since.as_secs(), since.subsec_millis())
}

/// Resident memory of this process in MiB, or 0 where it can't be read.
pub(crate) fn rss_mib() -> u64 {
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

fn writer_loop(
    receiver: &Receiver<TickSample>,
    events: &Receiver<BackgroundEvent>,
    slow_nanos: i64,
) {
    let mut slow_log = open_append("slow-ticks.log", None);
    let mut event_log = open_append("events.log", None);
    let mut summary = open_append(
        "summary.csv",
        Some(
            "unix_time,ticks,avg_mspt,p99_mspt,max_mspt,ticks_over_50ms,players,entities,block_entities,loaded_chunks,rss_mib,p50_mspt,p95_mspt",
        ),
    );
    let mut window: Vec<i64> = Vec::with_capacity(SUMMARY_WINDOW_TICKS);
    let mut last_worlds: Vec<WorldTickReport> = Vec::new();
    let mut next_summary = Instant::now() + SUMMARY_INTERVAL;

    loop {
        match receiver.recv_timeout(EVENT_POLL) {
            Ok(sample) => {
                window.push(sample.nanos);
                if sample.nanos >= slow_nanos
                    && let Some(log) = slow_log.as_mut()
                {
                    let _ = writeln!(log, "{}", slow_tick_line(&sample));
                    let _ = log.flush();
                }
                if !sample.server.worlds.is_empty() {
                    last_worlds = sample.server.worlds;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        let mut wrote_event = false;
        while let Ok(event) = events.try_recv() {
            if let Some(log) = event_log.as_mut() {
                let _ = writeln!(
                    log,
                    "{} {} {}",
                    unix_millis_text(event.at),
                    event.name,
                    event.detail
                );
                wrote_event = true;
            }
        }
        if wrote_event && let Some(log) = event_log.as_mut() {
            let _ = log.flush();
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
    let l = &sample.tick_loop;
    let s = &sample.server;
    let mut line = format!(
        "{} tick={} start={} total={:.2}ms unattributed={:.2}ms rss={}MiB | server: rate_manager={:.2}ms start_event={:.2}ms suspend_flushes={:.2}ms tick_functions={:.2}ms plugin_loaders={:.2}ms scheduled_functions={:.2}ms time_sync={:.2}ms worlds={:.2}ms player_data={:.2}ms frozen_players={:.2}ms block_flush={:.2}ms resume_flushes={:.2}ms game_tests={:.2}ms end_event={:.2}ms",
        unix_secs(),
        sample.tick,
        unix_millis_text(l.started),
        sample.nanos as f64 / 1e6,
        ms(sample.unattributed()),
        rss_mib(),
        ms(l.rate_manager),
        ms(l.start_event),
        ms(s.suspend_flushes),
        ms(s.tick_functions),
        ms(s.plugin_loaders),
        ms(s.scheduled_functions),
        ms(s.time_sync),
        ms(s.world_ticks),
        ms(s.player_data),
        ms(s.frozen_players),
        ms(s.block_flush),
        ms(s.resume_flushes),
        ms(l.game_tests),
        ms(l.end_event),
    );
    for w in &s.worlds {
        let _ = write!(
            line,
            " | {} {:.2}ms: queued={:.2}ms block_updates={:.2}ms active_chunks={:.2}ms environment={:.2}ms raids={:.2}ms chunks={:.2}ms({}) players={:.2}ms({}) islands={:.2}ms entities={:.2}ms({}) tracker={:.2}ms block_entities={:.2}ms({}) chunk_tickets={:.2}ms dragon_fight={:.2}ms other={:.2}ms",
            w.world,
            ms(w.total),
            ms(w.queued),
            ms(w.block_updates),
            ms(w.active_chunks),
            ms(w.environment),
            ms(w.raids),
            ms(w.chunks),
            w.loaded_chunks,
            ms(w.players),
            w.player_count,
            ms(w.islands),
            ms(w.entities),
            w.entity_count,
            ms(w.tracker),
            ms(w.block_entities),
            w.block_entity_count,
            ms(w.chunk_tickets),
            ms(w.dragon_fight),
            ms(w.total.saturating_sub(w.phases())),
        );
    }
    line
}

/// Vanilla `TickCommand.executeQuery`: the sample at `floor(len * percent / 100)` of the sorted
/// window, as `tick query` reports it.
fn percentile_ms(sorted: &[i64], percent: usize) -> f64 {
    sorted
        .get((sorted.len() * percent / 100).min(sorted.len().saturating_sub(1)))
        .map_or(0.0, |n| *n as f64 / 1e6)
}

fn summary_line(window: &mut [i64], worlds: &[WorldTickReport]) -> String {
    window.sort_unstable();
    let ticks = window.len();
    let avg = if ticks == 0 {
        0.0
    } else {
        window.iter().sum::<i64>() as f64 / ticks as f64 / 1e6
    };
    let p50 = percentile_ms(window, 50);
    let p95 = percentile_ms(window, 95);
    let p99 = percentile_ms(window, 99);
    let max = window.last().map_or(0.0, |n| *n as f64 / 1e6);
    let laggy = window.iter().filter(|n| **n > LAGGY_TICK_NANOS).count();
    let players: usize = worlds.iter().map(|w| w.player_count).sum();
    let entities: usize = worlds.iter().map(|w| w.entity_count).sum();
    let block_entities: usize = worlds.iter().map(|w| w.block_entity_count).sum();
    let chunks: usize = worlds.iter().map(|w| w.loaded_chunks).sum();
    format!(
        "{},{ticks},{avg:.2},{p99:.2},{max:.2},{laggy},{players},{entities},{block_entities},{chunks},{},{p50:.2},{p95:.2}",
        unix_secs(),
        rss_mib()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 200 ticks of 1 to 200 ms, slowest first. Sorted, index 100 holds 101 ms, index 190 holds
    /// 191 ms and index 198 holds 199 ms. A nearest-rank or `(len - 1) * p` percentile reads the
    /// tick before each of them.
    #[test]
    fn summary_line_percentiles() {
        let mut window: Vec<i64> = (1..=200).rev().map(|ms| ms * 1_000_000).collect();
        let line = summary_line(&mut window, &[]);
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 13, "{line}");
        assert_eq!(fields[1], "200");
        assert_eq!(fields[2], "100.50", "average");
        assert_eq!(fields[3], "199.00", "P99");
        assert_eq!(fields[4], "200.00", "maximum");
        assert_eq!(fields[5], "150", "ticks over 50 ms");
        assert_eq!(fields[11], "101.00", "P50");
        assert_eq!(fields[12], "191.00", "P95");
    }
}
