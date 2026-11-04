use std::str::FromStr;

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::position::BlockPos,
    resource_location::{FromResourceLocation, ResourceLocation, ToResourceLocation},
};

pub mod order;
pub mod scheduler;

const MAX_TICK_DELAY: usize = 1 << 8;

/// Vanilla `ServerLevel.tick` runs at most this many block ticks, and as many fluid ticks, in a
/// game tick. The rest stay scheduled and are overdue in the next one.
pub const MAX_TICKS_PER_GAME_TICK: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
#[repr(i32)]
pub enum TickPriority {
    ExtremelyHigh = -3,
    VeryHigh = -2,
    High = -1,
    Normal = 0,
    Low = 1,
    VeryLow = 2,
    ExtremelyLow = 3,
}

impl TickPriority {
    #[must_use]
    pub const fn values() -> [Self; 7] {
        [
            Self::ExtremelyHigh,
            Self::VeryHigh,
            Self::High,
            Self::Normal,
            Self::Low,
            Self::VeryLow,
            Self::ExtremelyLow,
        ]
    }
}

#[derive(Debug)]
pub struct TickPriorityNotFound;

impl TryFrom<i32> for TickPriority {
    type Error = TickPriorityNotFound;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -3 => Ok(Self::ExtremelyHigh),
            -2 => Ok(Self::VeryHigh),
            -1 => Ok(Self::High),
            0 => Ok(Self::Normal),
            1 => Ok(Self::Low),
            2 => Ok(Self::VeryLow),
            3 => Ok(Self::ExtremelyLow),
            _ => Err(TickPriorityNotFound),
        }
    }
}

#[derive(Clone)]
pub struct ScheduledTick<T> {
    pub delay: u8,
    pub priority: TickPriority,
    pub position: BlockPos,
    pub value: T,
}

#[derive(Clone)]
pub struct OrderedTick<T> {
    /// Game ticks since this tick came due, 0 for one due now. Vanilla drains by trigger tick
    /// first, so overdue ticks run before the ones due now.
    pub overdue: u32,
    pub priority: TickPriority,
    /// Vanilla `subTickOrder`. Ticks loaded with a chunk are numbered from `-n`, so they run
    /// before ticks scheduled later for the same game tick.
    pub sub_tick_order: i64,

    pub position: BlockPos,
    pub value: T,
}

impl<T> OrderedTick<T> {
    pub const fn new(position: BlockPos, value: T) -> Self {
        Self {
            overdue: 0,
            priority: TickPriority::Normal,
            sub_tick_order: 0,
            position,
            value,
        }
    }
}

impl<T> PartialEq for OrderedTick<T> {
    fn eq(&self, other: &Self) -> bool {
        self.overdue == other.overdue
            && self.priority == other.priority
            && self.sub_tick_order == other.sub_tick_order
    }
}

impl<T> Eq for OrderedTick<T> {}

impl<T> PartialOrd for OrderedTick<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}

impl<T> Ord for OrderedTick<T> {
    /// Vanilla `ScheduledTick.DRAIN_ORDER`: trigger tick, priority, then sub-tick order.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .overdue
            .cmp(&self.overdue)
            .then_with(|| self.priority.cmp(&other.priority))
            .then_with(|| self.sub_tick_order.cmp(&other.sub_tick_order))
    }
}

/// Sorts the ticks due in this game tick into vanilla's drain order and splits off the ones past
/// `MAX_TICKS_PER_GAME_TICK`, which the caller puts back with `ChunkTickScheduler::carry_over`.
pub fn sort_and_cap<T>(ticks: &mut Vec<OrderedTick<T>>) -> Vec<OrderedTick<T>> {
    ticks.sort_unstable();
    if ticks.len() > MAX_TICKS_PER_GAME_TICK {
        ticks.split_off(MAX_TICKS_PER_GAME_TICK)
    } else {
        Vec::new()
    }
}

impl<T> ScheduledTick<T>
where
    T: ToResourceLocation,
{
    #[must_use]
    pub fn to_nbt_compound(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_int("x", self.position.0.x);
        nbt.put_int("y", self.position.0.y);
        nbt.put_int("z", self.position.0.z);
        nbt.put_int("t", self.delay as i32);
        nbt.put_int("p", self.priority as i32);
        nbt.put_string("i", self.value.to_resource_location());
        nbt
    }
}

impl<T> ScheduledTick<T>
where
    T: FromResourceLocation,
{
    #[must_use]
    pub fn from_nbt_compound(nbt: &NbtCompound) -> Option<Self> {
        let x = nbt.get_int("x")?;
        let y = nbt.get_int("y")?;
        let z = nbt.get_int("z")?;
        let delay = nbt.get_int("t")? as u8;
        let priority = TickPriority::try_from(nbt.get_int("p")?).ok()?;
        let res_loc_str = nbt.get_string("i")?;
        let res_loc = ResourceLocation::from_str(res_loc_str).ok()?;
        let value = T::from_resource_location(&res_loc)?;

        Some(Self {
            delay,
            priority,
            position: BlockPos::new(x, y, z),
            value,
        })
    }
}
