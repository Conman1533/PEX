use crate::server::Server;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::warn;

const FUNCTION_CALLBACK: &str = "minecraft:function";
const FUNCTION_TAG_CALLBACK: &str = "minecraft:function_tag";

/// One pending `/schedule`, vanilla's `TimerQueue.Event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledFunctionEvent {
    /// The schedule id: the function id, or `#` and the tag id.
    pub id: String,
    /// The game time (overworld `Time`) the event runs at.
    pub trigger_time: i64,
    /// Order among events with the same trigger time.
    pub sequence: u64,
    /// What to run, as `execute_function` takes it: `#` and the tag id for a tag.
    pub function_name: String,
    pub is_tag: bool,
}

impl ScheduledFunctionEvent {
    /// The function or tag id without the `#` a tag's name starts with.
    fn callback_id(&self) -> &str {
        self.function_name
            .strip_prefix('#')
            .unwrap_or(&self.function_name)
    }

    fn to_nbt(&self) -> NbtCompound {
        let mut callback = NbtCompound::new();
        let kind = if self.is_tag {
            FUNCTION_TAG_CALLBACK
        } else {
            FUNCTION_CALLBACK
        };
        callback.put_string("type", kind.to_string());
        callback.put_string("id", self.callback_id().to_string());
        let mut event = NbtCompound::new();
        event.put_long("trigger_time", self.trigger_time);
        event.put_string("id", self.id.clone());
        event.put_compound("callback", callback);
        event
    }
}

impl PartialOrd for ScheduledFunctionEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledFunctionEvent {
    /// Reversed, so the `BinaryHeap` pops the earliest event first.
    fn cmp(&self, other: &Self) -> Ordering {
        (other.trigger_time, other.sequence).cmp(&(self.trigger_time, self.sequence))
    }
}

#[derive(Default)]
struct QueueState {
    events: BinaryHeap<ScheduledFunctionEvent>,
    next_sequence: u64,
}

/// The `/schedule` queue, vanilla's `TimerQueue`, saved as `data/minecraft/scheduled_events.dat`.
#[derive(Default)]
pub struct ScheduledFunctionQueue {
    queue: Mutex<QueueState>,
}

impl ScheduledFunctionQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Queues `function_name` to run at game time `trigger_time` under `id`. Like vanilla,
    /// a second event with the same id and trigger time is dropped.
    pub fn schedule(&self, id: String, trigger_time: i64, function_name: String, is_tag: bool) {
        let mut state = self.lock();
        if state
            .events
            .iter()
            .any(|event| event.id == id && event.trigger_time == trigger_time)
        {
            return;
        }
        state.next_sequence += 1;
        let sequence = state.next_sequence;
        state.events.push(ScheduledFunctionEvent {
            id,
            trigger_time,
            sequence,
            function_name,
            is_tag,
        });
    }

    /// Removes every event scheduled under `id` and returns how many there were.
    pub fn remove(&self, id: &str) -> usize {
        let mut state = self.lock();
        let before = state.events.len();
        state.events.retain(|event| event.id != id);
        before - state.events.len()
    }

    #[must_use]
    pub fn get_event_ids(&self) -> Vec<String> {
        let state = self.lock();
        let mut ids: Vec<String> = state.events.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Runs every event due at game time `game_time`.
    pub fn tick(&self, server: &Arc<Server>, game_time: i64) {
        let mut to_run = Vec::new();
        {
            let mut state = self.lock();
            while state
                .events
                .peek()
                .is_some_and(|event| event.trigger_time <= game_time)
            {
                if let Some(event) = state.events.pop() {
                    to_run.push(event);
                }
            }
        }
        for event in to_run {
            let _ = crate::data::datapack::DatapackManager::execute_function_from_console(
                server,
                &event.function_name,
            );
        }
    }

    /// The `data` compound of vanilla's `scheduled_events.dat` (`TimerQueue.CODEC`).
    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let state = self.lock();
        let mut events: Vec<&ScheduledFunctionEvent> = state.events.iter().collect();
        events.sort_by_key(|event| (event.trigger_time, event.sequence));
        let events = events
            .into_iter()
            .map(|event| NbtTag::Compound(event.to_nbt()))
            .collect();
        let mut data = NbtCompound::new();
        data.put_list("events", events);
        data
    }

    /// Reads the `data` compound of vanilla's `scheduled_events.dat`. Events keep their saved
    /// order, which is vanilla's run order.
    #[must_use]
    pub fn from_nbt(data: &NbtCompound) -> Self {
        let queue = Self::new();
        for tag in data.get_list("events").unwrap_or_default() {
            let NbtTag::Compound(event) = tag else {
                continue;
            };
            let (Some(trigger_time), Some(id), Some(callback)) = (
                event.get_long("trigger_time"),
                event.get_string("id"),
                event.get_compound("callback"),
            ) else {
                warn!("Skipping a malformed scheduled event: {event:?}");
                continue;
            };
            let callback_id = callback.get_string("id").unwrap_or_default();
            let (function_name, is_tag) = match callback.get_string("type") {
                Some(FUNCTION_CALLBACK) => (callback_id.to_string(), false),
                Some(FUNCTION_TAG_CALLBACK) => (format!("#{callback_id}"), true),
                other => {
                    warn!("Skipping scheduled event {id} with unknown callback type {other:?}");
                    continue;
                }
            };
            queue.schedule(id.to_string(), trigger_time, function_name, is_tag);
        }
        queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vanilla_data() -> NbtCompound {
        let root = pumpkin_nbt::nbt_compress::read_gzip_compound_tag(std::io::Cursor::new(
            include_bytes!("testdata/scheduled_events_vanilla_26_3.dat"),
        ))
        .unwrap();
        root.get_compound("data").unwrap().clone()
    }

    /// `testdata/scheduled_events_vanilla_26_3.dat` is what vanilla 26.3 saved at game time 170
    /// after `/schedule function test:hello 100000t`, `.. test:hello 200000t append` and
    /// `/schedule function #test:tagged 150000t`.
    #[test]
    fn vanilla_scheduled_events_load_and_save_unchanged() {
        let data = vanilla_data();
        let queue = ScheduledFunctionQueue::from_nbt(&data);
        assert_eq!(queue.to_nbt(), data);
        assert_eq!(queue.get_event_ids(), ["#test:tagged", "test:hello"]);

        let state = queue.lock();
        let mut events: Vec<_> = state.events.iter().cloned().collect();
        events.sort_by_key(|event| (event.trigger_time, event.sequence));
        let summary: Vec<_> = events
            .iter()
            .map(|e| (e.trigger_time, e.function_name.as_str(), e.is_tag))
            .collect();
        assert_eq!(
            summary,
            [
                (100_110, "test:hello", false),
                (150_150, "#test:tagged", true),
                (200_130, "test:hello", false),
            ]
        );
    }

    #[test]
    fn same_id_and_time_is_scheduled_once_and_clear_removes_all_times() {
        let queue = ScheduledFunctionQueue::new();
        queue.schedule("a:f".into(), 10, "a:f".into(), false);
        queue.schedule("a:f".into(), 10, "a:f".into(), false);
        queue.schedule("a:f".into(), 20, "a:f".into(), false);
        queue.schedule("b:g".into(), 10, "b:g".into(), false);
        assert_eq!(queue.lock().events.len(), 3);
        assert_eq!(queue.remove("a:f"), 2);
        assert_eq!(queue.get_event_ids(), ["b:g"]);
    }

    #[test]
    fn events_with_the_same_time_keep_their_schedule_order() {
        let queue = ScheduledFunctionQueue::new();
        queue.schedule("z:last".into(), 5, "z:last".into(), false);
        queue.schedule("a:first".into(), 5, "a:first".into(), false);
        let saved = queue.to_nbt();
        let ids: Vec<_> = saved
            .get_list("events")
            .unwrap()
            .iter()
            .filter_map(|tag| match tag {
                NbtTag::Compound(event) => event.get_string("id").map(str::to_string),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["z:last", "a:first"]);
    }
}
