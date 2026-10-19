//! `PEXProtect`: CoreProtect-style block logging, inspection, lookup, rollback and restore.
//!
//! Events fire before the change happens and don't say what the block becomes, so each handler
//! records the old state and the next tick reads the new one. Changes another plugin cancelled
//! are dropped automatically, because the block didn't change.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod db;
mod params;

use db::{Filter, Record};
use pex_common::Prefix;
use pumpkin::command::argument_builder::{ArgumentBuilder, argument, command};
use pumpkin::command::argument_types::core::string::StringArgumentType;
use pumpkin::command::context::command_context::CommandContext;
use pumpkin::command::context::command_source::CommandSource;
use pumpkin::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin::entity::EntityBase;
use pumpkin::plugin::block::block_break::BlockBreakEvent;
use pumpkin::plugin::block::block_burn::BlockBurnEvent;
use pumpkin::plugin::block::block_place::BlockPlaceEvent;
use pumpkin::plugin::entity::entity_change_block::EntityChangeBlockEvent;
use pumpkin::plugin::entity::entity_explode::EntityExplodeEvent;
use pumpkin::plugin::player::player_bucket::{PlayerBucketEmptyEvent, PlayerBucketFillEvent};
use pumpkin::plugin::player::player_interact_event::PlayerInteractEvent;
use pumpkin::plugin::server::server_tick_start::ServerTickStartEvent;
use pumpkin::plugin::{BoxFuture, Context, EventHandler, EventPriority, Plugin, PluginFuture};
use pumpkin::server::Server;
use pumpkin::world::World;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::permission::{Permission, PermissionDefault};
use pumpkin_util::text::color::NamedColor;
use pumpkin_world::world::BlockFlags;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pex_common::export_plugin!(
    PexProtect,
    "Block logging with inspect, lookup, rollback and restore."
);

const PREFIX: Prefix = Prefix {
    name: "Protect",
    color: NamedColor::DarkGreen,
};
const PERM_COMMAND: &str = "pexprotect:command.co";
const PERM_ROLLBACK: &str = "pexprotect:rollback";
const ROLLBACK_BLOCKS_PER_TICK: usize = 4096;
const MAX_ROLLBACK: usize = 200_000;
const LOOKUP_LINES: usize = 10;
const INSPECT_LINES: usize = 6;
/// Half-width of the area snapshotted around an explosion. TNT reaches about 5 blocks.
const EXPLOSION_RADIUS: i32 = 6;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

const fn arr(pos: &BlockPos) -> [i32; 3] {
    [pos.0.x, pos.0.y, pos.0.z]
}

const fn block_name(state: u16) -> &'static str {
    Block::from_state_id(BlockStateId::new_or_air(state)).name
}

enum Msg {
    Insert(Vec<Record>),
    RolledBack(Vec<i64>, bool),
}

/// A change seen in an event, waiting for the next tick to learn what the block became.
struct Pending {
    world: Arc<World>,
    pos: BlockPos,
    actor: String,
    action: &'static str,
    old: u16,
    time: i64,
}

/// Blocks around an explosion before it happened.
struct Snapshot {
    world: Arc<World>,
    actor: String,
    cells: Vec<(BlockPos, u16)>,
    time: i64,
}

/// A queued rollback or restore: positions and the state to put back.
struct Job {
    world: Arc<World>,
    changes: VecDeque<(i64, BlockPos, u16)>,
    done: Vec<i64>,
    rolled_back: bool,
    requester: String,
    total: usize,
}

struct Inner {
    writer: Sender<Msg>,
    reader: Mutex<rusqlite::Connection>,
    pending: Mutex<Vec<Pending>>,
    snapshots: Mutex<Vec<Snapshot>>,
    inspecting: Mutex<HashSet<uuid::Uuid>>,
    jobs: Mutex<VecDeque<Job>>,
    /// Kept so the writer thread lives as long as the plugin; it exits when `writer` is dropped.
    _worker: JoinHandle<()>,
}

#[derive(Clone)]
struct Shared(Arc<Inner>);

impl Shared {
    fn send(&self, msg: Msg) {
        if self.0.writer.send(msg).is_err() {
            tracing::error!(
                "PEXProtect: the database writer stopped; changes are not being logged"
            );
        }
    }

    fn capture(&self, world: Arc<World>, pos: BlockPos, actor: String, action: &'static str) {
        let old = world.get_block_state_id(&pos).as_u16();
        lock(&self.0.pending).push(Pending {
            world,
            pos,
            actor,
            action,
            old,
            time: now(),
        });
    }

    fn query(
        &self,
        filter: &Filter,
        oldest_first: bool,
        limit: usize,
    ) -> Result<Vec<Record>, String> {
        db::query(&lock(&self.0.reader), filter, oldest_first, limit).map_err(|err| {
            tracing::error!("PEXProtect: lookup failed: {err}");
            "Lookup failed; see the server log.".to_owned()
        })
    }
}

/// Owns the write connection and commits batches until every sender is gone.
fn run_writer(mut conn: rusqlite::Connection, rx: &Receiver<Msg>) {
    loop {
        let first = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let mut inserts = Vec::new();
        for msg in std::iter::once(first).chain(rx.try_iter()) {
            match msg {
                Msg::Insert(records) => inserts.extend(records),
                Msg::RolledBack(ids, rolled_back) => {
                    if let Err(err) = db::set_rolled_back(&mut conn, &ids, rolled_back) {
                        tracing::error!("PEXProtect: could not mark rollback: {err}");
                    }
                }
            }
        }
        if !inserts.is_empty()
            && let Err(err) = db::insert(&mut conn, &inserts)
        {
            tracing::error!(
                "PEXProtect: could not save {} records: {err}",
                inserts.len()
            );
        }
    }
}

pub struct PexProtect;

impl PexProtect {
    const fn new() -> Self {
        Self
    }
}

impl Plugin for PexProtect {
    fn on_load(&self, context: Arc<Context>) -> PluginFuture<'_, Result<(), String>> {
        Box::pin(async move {
            context.init_log();
            let path = context.get_data_folder().join("blocks.db");
            let open = |path: &std::path::Path| {
                db::open(path).map_err(|err| format!("could not open {}: {err}", path.display()))
            };
            let writer_conn = open(&path)?;
            let reader = open(&path)?;
            let (tx, rx) = mpsc::channel();
            let worker = std::thread::Builder::new()
                .name("pexprotect-db".into())
                .spawn(move || run_writer(writer_conn, &rx))
                .map_err(|err| format!("could not start the database thread: {err}"))?;
            let shared = Shared(Arc::new(Inner {
                writer: tx,
                reader: Mutex::new(reader),
                pending: Mutex::new(Vec::new()),
                snapshots: Mutex::new(Vec::new()),
                inspecting: Mutex::new(HashSet::new()),
                jobs: Mutex::new(VecDeque::new()),
                _worker: worker,
            }));

            for (node, description) in [
                (PERM_COMMAND, "Allows /co inspect and lookup"),
                (PERM_ROLLBACK, "Allows /co rollback, restore and purge"),
            ] {
                context.register_permission(Permission::new(
                    node,
                    description,
                    PermissionDefault::Op(PermissionLvl::Two),
                ))?;
            }
            for name in ["co", "coreprotect"] {
                let node = command(name, "Block logging: inspect, lookup, rollback, restore")
                    .executes(CoCommand(shared.clone()))
                    .then(
                        argument("args", StringArgumentType::GreedyPhrase)
                            .executes(CoCommand(shared.clone())),
                    );
                context.register_command(node, PERM_COMMAND);
            }

            macro_rules! observe {
                ($event:ty, $handler:ident) => {
                    context.register_event::<$event, _>(
                        Arc::new($handler(shared.clone())),
                        EventPriority::Lowest,
                        false,
                    );
                };
            }
            observe!(BlockBreakEvent, LogBreak);
            observe!(BlockPlaceEvent, LogPlace);
            observe!(PlayerBucketEmptyEvent, LogBucketEmpty);
            observe!(PlayerBucketFillEvent, LogBucketFill);
            observe!(BlockBurnEvent, LogBurn);
            observe!(EntityExplodeEvent, LogExplode);
            observe!(EntityChangeBlockEvent, LogEntityChange);
            context.register_event::<PlayerInteractEvent, _>(
                Arc::new(InspectInteract(shared.clone())),
                EventPriority::Highest,
                true,
            );
            context.register_event::<BlockBreakEvent, _>(
                Arc::new(InspectBreak(shared.clone())),
                EventPriority::Highest,
                true,
            );
            context.register_event::<ServerTickStartEvent, _>(
                Arc::new(Tick(shared)),
                EventPriority::Normal,
                true,
            );
            tracing::info!("PEXProtect loaded, logging to {}", path.display());
            Ok(())
        })
    }
}

/// Defines a non-blocking handler that only records what is about to change.
macro_rules! logger {
    ($name:ident, $event:ty, |$shared:ident, $server:ident, $ev:ident| $body:block) => {
        struct $name(Shared);

        impl EventHandler<$event> for $name {
            fn handle<'a>(&'a self, server: &'a Arc<Server>, event: &'a $event) -> BoxFuture<'a, ()> {
                let _runtime = server.runtime.enter();
                let $shared = &self.0;
                let $server = server;
                let $ev = event;
                $body
                Box::pin(async {})
            }
        }
    };
}

fn find_entity(server: &Server, id: i32) -> Option<(Arc<World>, Arc<dyn EntityBase>)> {
    server.worlds.load().iter().find_map(|world| {
        world
            .get_entity_by_id(id)
            .map(|entity| (world.clone(), entity))
    })
}

logger!(LogBreak, BlockBreakEvent, |shared, _server, event| {
    if let Some(player) = &event.player {
        shared.capture(
            player.world(),
            event.block_position,
            player.gameprofile.name.clone(),
            "break",
        );
    }
});

logger!(LogPlace, BlockPlaceEvent, |shared, _server, event| {
    let player = &event.player;
    shared.capture(
        player.world(),
        event.block_position,
        player.gameprofile.name.clone(),
        "place",
    );
});

logger!(
    LogBucketEmpty,
    PlayerBucketEmptyEvent,
    |shared, _server, event| {
        let player = &event.player;
        shared.capture(
            player.world(),
            event.block_pos,
            player.gameprofile.name.clone(),
            "bucket",
        );
    }
);

logger!(
    LogBucketFill,
    PlayerBucketFillEvent,
    |shared, _server, event| {
        let player = &event.player;
        shared.capture(
            player.world(),
            event.block_pos,
            player.gameprofile.name.clone(),
            "bucket",
        );
    }
);

logger!(LogBurn, BlockBurnEvent, |shared, _server, event| {
    shared.capture(
        event.world.clone(),
        event.block_pos,
        "#fire".to_owned(),
        "burn",
    );
});

logger!(
    LogEntityChange,
    EntityChangeBlockEvent,
    |shared, server, event| {
        if let Some((world, entity)) = find_entity(server, event.entity_id) {
            let actor = format!("#{}", entity.get_entity().entity_type.resource_name);
            shared.capture(world, event.block_pos, actor, "change");
        }
    }
);

// PEX fires this event with `entity_id` 0 and no world, so snapshot every world that has the
// chunk loaded; worlds where nothing changes produce no records.
logger!(LogExplode, EntityExplodeEvent, |shared, server, event| {
    let center = BlockPos::floored_v(event.position);
    let r = EXPLOSION_RADIUS;
    for world in server.worlds.load().iter() {
        if world.get_block_state_id_if_loaded(&center).is_none() {
            continue;
        }
        let mut cells = Vec::new();
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    let pos = BlockPos::new(center.0.x + dx, center.0.y + dy, center.0.z + dz);
                    let state = world.get_block_state_id(&pos).as_u16();
                    if state != 0 {
                        cells.push((pos, state));
                    }
                }
            }
        }
        lock(&shared.0.snapshots).push(Snapshot {
            world: world.clone(),
            actor: "#explosion".to_owned(),
            cells,
            time: now(),
        });
    }
});

struct Tick(Shared);

impl EventHandler<ServerTickStartEvent> for Tick {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        _event: &'a mut ServerTickStartEvent,
    ) -> BoxFuture<'a, ()> {
        let _runtime = server.runtime.enter();
        let inner = &self.0.0;
        let mut records = Vec::new();

        let pending = std::mem::take(&mut *lock(&inner.pending));
        for change in pending {
            let new = change.world.get_block_state_id(&change.pos).as_u16();
            if new != change.old {
                records.push(Record {
                    id: 0,
                    time: change.time,
                    world: change.world.get_world_name().to_owned(),
                    pos: arr(&change.pos),
                    actor: change.actor,
                    action: change.action.to_owned(),
                    old_state: change.old,
                    new_state: new,
                    rolled_back: false,
                });
            }
        }

        let snapshots = std::mem::take(&mut *lock(&inner.snapshots));
        for snapshot in snapshots {
            let world_name = snapshot.world.get_world_name().to_owned();
            for (pos, old) in snapshot.cells {
                let new = snapshot.world.get_block_state_id(&pos).as_u16();
                if new != old {
                    records.push(Record {
                        id: 0,
                        time: snapshot.time,
                        world: world_name.clone(),
                        pos: arr(&pos),
                        actor: snapshot.actor.clone(),
                        action: "explode".to_owned(),
                        old_state: old,
                        new_state: new,
                        rolled_back: false,
                    });
                }
            }
        }
        if !records.is_empty() {
            self.0.send(Msg::Insert(records));
        }

        run_jobs(&self.0, server);
        Box::pin(async {})
    }
}

/// Applies up to the per-tick budget of the front rollback or restore job.
fn run_jobs(shared: &Shared, server: &Server) {
    let finished = {
        let mut jobs = lock(&shared.0.jobs);
        let Some(job) = jobs.front_mut() else {
            return;
        };
        // Rollback writes must not drop items or trigger physics, or they'd change more blocks.
        let flags = BlockFlags::NOTIFY_LISTENERS | BlockFlags::FORCE_STATE | BlockFlags::SKIP_DROPS;
        for _ in 0..ROLLBACK_BLOCKS_PER_TICK {
            let Some((id, pos, state)) = job.changes.pop_front() else {
                break;
            };
            job.world
                .set_block_state(&pos, BlockStateId::new_or_air(state), flags);
            job.done.push(id);
        }
        if job.changes.is_empty() {
            jobs.pop_front()
        } else {
            None
        }
    };
    if let Some(job) = finished {
        let verb = if job.rolled_back {
            "Rollback"
        } else {
            "Restore"
        };
        let message = format!("{verb} finished: {} blocks.", job.total);
        shared.send(Msg::RolledBack(job.done, job.rolled_back));
        if let Some(player) = server.get_player_by_name(&job.requester) {
            PREFIX.send(&player, message);
        } else {
            tracing::info!("PEXProtect: {message}");
        }
    }
}

struct InspectInteract(Shared);

impl EventHandler<PlayerInteractEvent> for InspectInteract {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a mut PlayerInteractEvent,
    ) -> BoxFuture<'a, ()> {
        let _runtime = server.runtime.enter();
        let inspecting = lock(&self.0.0.inspecting).contains(&event.player.gameprofile.id);
        if inspecting && let Some(clicked) = event.clicked_pos {
            event.cancelled = true;
            let player = event.player.clone();
            let filter = Filter {
                world: Some(player.world().get_world_name().to_owned()),
                area: Some((arr(&clicked), 0)),
                ..Filter::default()
            };
            let pos = arr(&clicked);
            match self.0.query(&filter, false, INSPECT_LINES) {
                Ok(records) if records.is_empty() => {
                    PREFIX.send(
                        &player,
                        format!("No changes at {} {} {}.", pos[0], pos[1], pos[2]),
                    );
                }
                Ok(records) => {
                    PREFIX.send(
                        &player,
                        format!("Changes at {} {} {}:", pos[0], pos[1], pos[2]),
                    );
                    let now = now();
                    for record in &records {
                        PREFIX.send(&player, describe(record, now));
                    }
                }
                Err(err) => PREFIX.send_error(&player, err),
            }
        }
        Box::pin(async {})
    }
}

/// Stops inspectors from breaking the block they're inspecting.
struct InspectBreak(Shared);

impl EventHandler<BlockBreakEvent> for InspectBreak {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        if let Some(player) = &event.player
            && lock(&self.0.0.inspecting).contains(&player.gameprofile.id)
        {
            event.cancelled = true;
        }
        Box::pin(async {})
    }
}

fn describe(record: &Record, now: i64) -> String {
    let block = if record.action == "place" {
        block_name(record.new_state)
    } else {
        block_name(record.old_state)
    };
    let verb = match record.action.as_str() {
        "break" => "broke",
        "place" => "placed",
        "bucket" => "used a bucket on",
        "burn" => "burned",
        "explode" => "blew up",
        _ => "changed",
    };
    format!(
        "{}{} {} {verb} {block} at {} {} {}",
        if record.rolled_back {
            "[rolled back] "
        } else {
            ""
        },
        params::ago(now - record.time),
        record.actor,
        record.pos[0],
        record.pos[1],
        record.pos[2]
    )
}

struct CoCommand(Shared);

impl CommandExecutor for CoCommand {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let source = &context.source;
        let _runtime = source.server().runtime.enter();
        let args = StringArgumentType::get(context, "args").unwrap_or_default();
        match run(&self.0, source, args) {
            Ok(message) => {
                source.send_message(PREFIX.info(message));
                Ok(1)
            }
            Err(message) => {
                source.send_message(PREFIX.error(message));
                Ok(0)
            }
        }
    }
}

fn here(source: &CommandSource) -> (String, [i32; 3]) {
    let pos = source.position;
    (
        source.world().get_world_name().to_owned(),
        [
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        ],
    )
}

fn run(shared: &Shared, source: &CommandSource, args: &str) -> Result<String, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let Some((sub, rest)) = parts.split_first() else {
        return Ok("Usage: /co <inspect|lookup|rollback|restore|purge> [u:user t:time r:radius a:action c:x,y,z w:world]".to_owned());
    };
    let (world, pos) = here(source);
    match sub.to_ascii_lowercase().as_str() {
        "i" | "inspect" => {
            let player = source.as_player().ok_or("Only players can inspect.")?;
            let mut inspecting = lock(&shared.0.inspecting);
            let id = player.gameprofile.id;
            Ok(if inspecting.remove(&id) {
                "Inspector off.".to_owned()
            } else {
                inspecting.insert(id);
                "Inspector on. Click a block to see its history.".to_owned()
            })
        }
        "l" | "lookup" => {
            let params = params::parse(rest)?;
            let filter = params.filter(now(), (&world, pos));
            let records = shared.query(&filter, false, LOOKUP_LINES + 1)?;
            if records.is_empty() {
                return Ok("No results.".to_owned());
            }
            let now = now();
            for record in records.iter().take(LOOKUP_LINES) {
                source.send_message(PREFIX.info(describe(record, now)));
            }
            Ok(if records.len() > LOOKUP_LINES {
                format!("Showing the newest {LOOKUP_LINES}; narrow it with t:, u: or r:.")
            } else {
                format!("{} results.", records.len())
            })
        }
        sub @ ("rb" | "rollback" | "rs" | "restore") => {
            if !source.has_permission(PERM_ROLLBACK) {
                return Err("You don't have permission for that.".to_owned());
            }
            let rollback = matches!(sub, "rb" | "rollback");
            let params = params::parse(rest)?;
            if params.time.is_none() {
                return Err("Give a time, like t:1h.".to_owned());
            }
            if params.actors.is_empty() && params.radius.is_none() {
                return Err("Give a user (u:) or a radius (r:).".to_owned());
            }
            let mut filter = params.filter(now(), (&world, pos));
            // Each block is rolled back or restored once; repeating the command does nothing.
            filter.rolled_back = Some(!rollback);
            let target_world = filter.world.clone().unwrap_or(world);
            filter.world = Some(target_world.clone());
            let records = shared.query(&filter, !rollback, MAX_ROLLBACK)?;
            if records.is_empty() {
                return Ok("Nothing to change.".to_owned());
            }
            let world = source
                .server()
                .worlds
                .load()
                .iter()
                .find(|w| w.get_world_name() == target_world)
                .cloned()
                .ok_or(format!("World {target_world} isn't loaded."))?;
            let changes: VecDeque<_> = records
                .iter()
                .map(|r| {
                    let state = if rollback { r.old_state } else { r.new_state };
                    (r.id, BlockPos::new(r.pos[0], r.pos[1], r.pos[2]), state)
                })
                .collect();
            let total = changes.len();
            lock(&shared.0.jobs).push_back(Job {
                world,
                changes,
                done: Vec::new(),
                rolled_back: rollback,
                requester: source.name.clone(),
                total,
            });
            Ok(format!(
                "{} of {total} changes queued.",
                if rollback { "Rollback" } else { "Restore" }
            ))
        }
        "purge" => {
            if !source.has_permission(PERM_ROLLBACK) {
                return Err("You don't have permission for that.".to_owned());
            }
            let time = params::parse(rest)?
                .time
                .ok_or("Give an age, like t:30d; older records are deleted.")?;
            let removed = db::purge(&lock(&shared.0.reader), now() - time).map_err(|err| {
                tracing::error!("PEXProtect: purge failed: {err}");
                "Purge failed; see the server log.".to_owned()
            })?;
            Ok(format!("Deleted {removed} records."))
        }
        _ => Err("Unknown subcommand. Use inspect, lookup, rollback, restore or purge.".to_owned()),
    }
}
