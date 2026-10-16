//! Vanilla's game event listeners and vibration system (`GameEventDispatcher`,
//! `VibrationSystem`, `VibrationSelector`).
//!
//! A game event is offered to every vibration listener whose radius reaches it. A listener that
//! accepts it keeps the closest candidate of that tick, sends the vibration particle and receives
//! the vibration one tick per block of travel later.

use std::sync::{Arc, Mutex};

use pumpkin_data::game_event::GameEvent;
use pumpkin_data::particle::Particle;
use pumpkin_data::tag::{self, Tag, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::entity::EntityBase;
use crate::world::World;

/// `GameEvent.notificationRadius`: 16 unless registered otherwise.
#[must_use]
pub const fn notification_radius(event: GameEvent) -> i32 {
    match event {
        GameEvent::JukeboxPlay | GameEvent::JukeboxStopPlay => 10,
        GameEvent::Shriek => 32,
        _ => 16,
    }
}

/// Whether a game event is in a game event tag. Tags are matched by name: the generated enum is
/// alphabetical, which is not vanilla's registry order for every event.
#[must_use]
pub fn event_has_tag(event: GameEvent, tag: &Tag) -> bool {
    tag.0.contains(&event.name())
}

/// `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`.
#[must_use]
pub const fn event_frequency(event: GameEvent) -> i32 {
    match event {
        GameEvent::Step | GameEvent::Swim | GameEvent::Flap | GameEvent::Resonate1 => 1,
        GameEvent::ProjectileLand
        | GameEvent::HitGround
        | GameEvent::Splash
        | GameEvent::Bounce
        | GameEvent::Resonate2 => 2,
        GameEvent::ItemInteractFinish
        | GameEvent::ProjectileShoot
        | GameEvent::InstrumentPlay
        | GameEvent::Resonate3 => 3,
        GameEvent::EntityAction
        | GameEvent::ElytraGlide
        | GameEvent::Unequip
        | GameEvent::Resonate4 => 4,
        GameEvent::EntityDismount | GameEvent::Equip | GameEvent::Resonate5 => 5,
        GameEvent::EntityInteract
        | GameEvent::Shear
        | GameEvent::EntityMount
        | GameEvent::Resonate6 => 6,
        GameEvent::EntityDamage | GameEvent::Resonate7 => 7,
        GameEvent::Drink | GameEvent::Eat | GameEvent::Resonate8 => 8,
        GameEvent::ContainerClose
        | GameEvent::BlockClose
        | GameEvent::BlockDeactivate
        | GameEvent::BlockDetach
        | GameEvent::Resonate9 => 9,
        GameEvent::ContainerOpen
        | GameEvent::BlockOpen
        | GameEvent::BlockActivate
        | GameEvent::BlockAttach
        | GameEvent::PrimeFuse
        | GameEvent::NoteBlockPlay
        | GameEvent::Resonate10 => 10,
        GameEvent::BlockChange | GameEvent::Resonate11 => 11,
        GameEvent::BlockDestroy | GameEvent::FluidPickup | GameEvent::Resonate12 => 12,
        GameEvent::BlockPlace | GameEvent::FluidPlace | GameEvent::Resonate13 => 13,
        GameEvent::EntityPlace
        | GameEvent::LightningStrike
        | GameEvent::Teleport
        | GameEvent::Resonate14 => 14,
        GameEvent::EntityDie | GameEvent::Explode | GameEvent::Resonate15 => 15,
        _ => 0,
    }
}

/// `VibrationSystem.getResonanceEventByFrequency`.
#[must_use]
pub const fn resonance_event(frequency: i32) -> Option<GameEvent> {
    Some(match frequency {
        1 => GameEvent::Resonate1,
        2 => GameEvent::Resonate2,
        3 => GameEvent::Resonate3,
        4 => GameEvent::Resonate4,
        5 => GameEvent::Resonate5,
        6 => GameEvent::Resonate6,
        7 => GameEvent::Resonate7,
        8 => GameEvent::Resonate8,
        9 => GameEvent::Resonate9,
        10 => GameEvent::Resonate10,
        11 => GameEvent::Resonate11,
        12 => GameEvent::Resonate12,
        13 => GameEvent::Resonate13,
        14 => GameEvent::Resonate14,
        15 => GameEvent::Resonate15,
        _ => return None,
    })
}

/// `VibrationSystem.getRedstoneStrengthForDistance`.
#[must_use]
pub fn redstone_strength_for_distance(distance: f32, listener_radius: i32) -> u8 {
    let power_scale = 15.0 / f64::from(listener_radius);
    (15 - (power_scale * f64::from(distance)).floor() as i32).max(1) as u8
}

/// `GameEvent.Context`: who caused the event and, for block events, the block state involved.
#[derive(Clone, Default)]
pub struct GameEventContext {
    pub source: Option<Arc<dyn EntityBase>>,
    pub affected_state: Option<BlockStateId>,
}

impl GameEventContext {
    #[must_use]
    pub fn of(source: Option<Arc<dyn EntityBase>>, affected_state: Option<BlockStateId>) -> Self {
        Self {
            source,
            affected_state,
        }
    }
}

/// The shared handle of an entity, for a game event context.
#[must_use]
pub fn entity_handle(world: &World, entity: &dyn EntityBase) -> Option<Arc<dyn EntityBase>> {
    let id = entity.get_entity().entity_id;
    world
        .get_player_by_id(id)
        .map(|player| player as Arc<dyn EntityBase>)
        .or_else(|| world.get_entity_by_id(id))
}

/// `VibrationInfo`.
#[derive(Clone)]
pub struct VibrationInfo {
    pub event: GameEvent,
    pub distance: f32,
    pub pos: Vector3<f64>,
    pub source: Option<Arc<dyn EntityBase>>,
}

/// `VibrationSelector`: of the vibrations offered in one tick, keeps the closest, and on a tie the
/// one with the higher frequency. It is chosen on a later tick.
#[derive(Default)]
pub struct VibrationSelector {
    candidate: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    #[must_use]
    pub const fn new() -> Self {
        Self { candidate: None }
    }

    pub fn add_candidate(&mut self, vibration: VibrationInfo, tick: i64) {
        let replace = match &self.candidate {
            None => true,
            Some((previous, previous_tick)) => {
                tick == *previous_tick
                    && (vibration.distance < previous.distance
                        || (vibration.distance <= previous.distance
                            && event_frequency(vibration.event) > event_frequency(previous.event)))
            }
        };
        if replace {
            self.candidate = Some((vibration, tick));
        }
    }

    fn chosen_candidate(&self, time: i64) -> Option<VibrationInfo> {
        self.candidate
            .as_ref()
            .filter(|(_, tick)| *tick < time)
            .map(|(vibration, _)| vibration.clone())
    }

    fn start_over(&mut self) {
        self.candidate = None;
    }
}

/// `VibrationSystem.Data`.
#[derive(Default)]
pub struct VibrationData {
    pub current: Option<VibrationInfo>,
    pub selector: VibrationSelector,
    pub travel_time_in_ticks: i32,
}

/// `VibrationSystem.User`, implemented by each listener.
pub trait VibrationUser: Send + Sync {
    fn vibration_data(&self) -> &Mutex<VibrationData>;
    fn listener_radius(&self) -> i32;
    /// Where the listener is: the block centre for block listeners.
    fn listener_pos(&self) -> Vector3<f64>;
    fn can_receive_vibration(
        &self,
        world: &World,
        origin: &BlockPos,
        event: GameEvent,
        context: &GameEventContext,
    ) -> bool;
    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        origin: &BlockPos,
        event: GameEvent,
        source: Option<&Arc<dyn EntityBase>>,
        distance: f32,
    );
    fn listenable_events(&self) -> &'static Tag {
        &tag::GameEvent::MINECRAFT_VIBRATIONS
    }
    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        false
    }
    fn travel_time_in_ticks(&self, distance: f32) -> i32 {
        distance.floor() as i32
    }

    /// `VibrationSystem.User.isValidVibration`.
    fn is_valid_vibration(&self, event: GameEvent, context: &GameEventContext) -> bool {
        if !event_has_tag(event, self.listenable_events()) {
            return false;
        }
        if let Some(source) = &context.source {
            let entity = source.get_entity();
            if source
                .get_player()
                .is_some_and(crate::entity::player::Player::is_spectator)
            {
                return false;
            }
            // isSteppingCarefully: crouching
            if entity.is_sneaking()
                && event_has_tag(event, &tag::GameEvent::MINECRAFT_IGNORE_VIBRATIONS_SNEAKING)
            {
                return false;
            }
        }
        context.affected_state.is_none_or(|state| {
            !Block::from_state_id(state).has_tag(&tag::Block::MINECRAFT_DAMPENS_VIBRATIONS)
        })
    }
}

/// `VibrationSystem.Listener.handleGameEvent`.
pub fn handle_game_event(
    user: &dyn VibrationUser,
    world: &World,
    event: GameEvent,
    context: &GameEventContext,
    source_pos: Vector3<f64>,
) -> bool {
    if lock(user.vibration_data()).current.is_some() || !user.is_valid_vibration(event, context) {
        return false;
    }
    let destination = user.listener_pos();
    let origin = BlockPos::floored_v(source_pos);
    if !user.can_receive_vibration(world, &origin, event, context)
        || is_occluded(world, source_pos, destination)
    {
        return false;
    }
    let distance = source_pos.sub(&destination).length() as f32;
    lock(user.vibration_data()).selector.add_candidate(
        VibrationInfo {
            event,
            distance,
            pos: source_pos,
            source: context.source.clone(),
        },
        world.get_world_age(),
    );
    true
}

/// `VibrationSystem.Listener.forceScheduleVibration`: queues a vibration without the listener's
/// checks, as a sculk sensor does for an entity stepping on it.
pub fn force_schedule_vibration(
    user: &dyn VibrationUser,
    world: &World,
    event: GameEvent,
    context: &GameEventContext,
    origin: Vector3<f64>,
) {
    let distance = origin.sub(&user.listener_pos()).length() as f32;
    lock(user.vibration_data()).selector.add_candidate(
        VibrationInfo {
            event,
            distance,
            pos: origin,
            source: context.source.clone(),
        },
        world.get_world_age(),
    );
}

/// `VibrationSystem.Ticker.tick`.
pub fn tick(user: &dyn VibrationUser, world: &Arc<World>) {
    let arrived = {
        let mut data = lock(user.vibration_data());
        if data.current.is_none()
            && let Some(vibration) = data.selector.chosen_candidate(world.get_world_age())
        {
            data.travel_time_in_ticks = user.travel_time_in_ticks(vibration.distance);
            send_vibration_particle(
                world,
                vibration.pos,
                user.listener_pos(),
                data.travel_time_in_ticks,
            );
            data.current = Some(vibration);
            data.selector.start_over();
        }
        let Some(current) = data.current.clone() else {
            return;
        };
        data.travel_time_in_ticks -= 1;
        if data.travel_time_in_ticks > 0 {
            return;
        }
        current
    };
    // receiveVibration
    let origin = BlockPos::floored_v(arrived.pos);
    let destination = BlockPos::floored_v(user.listener_pos());
    if user.requires_adjacent_chunks_to_be_ticking() && !adjacent_chunks_loaded(world, &destination)
    {
        return;
    }
    let distance = (origin.squared_distance(&destination) as f32).sqrt();
    lock(user.vibration_data()).current = None;
    user.on_receive_vibration(
        world,
        &origin,
        arrived.event,
        arrived.source.as_ref(),
        distance,
    );
}

fn lock(data: &Mutex<VibrationData>) -> std::sync::MutexGuard<'_, VibrationData> {
    data.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `VibrationSystem.Ticker.areAdjacentChunksTicking`, approximated by the chunks being loaded.
fn adjacent_chunks_loaded(world: &World, pos: &BlockPos) -> bool {
    let chunk = pos.chunk_position();
    (-1..=1).all(|dx| {
        (-1..=1).all(|dz| {
            world
                .level
                .is_chunk_loaded(&pumpkin_util::math::vector2::Vector2::new(
                    chunk.x + dx,
                    chunk.y + dz,
                ))
        })
    })
}

/// The `minecraft:vibration` particle: a block position source and the arrival time.
fn send_vibration_particle(
    world: &World,
    origin: Vector3<f64>,
    destination: Vector3<f64>,
    ticks: i32,
) {
    let mut data = Vec::with_capacity(16);
    // PositionSourceType: 0 = block
    if VarInt(0).encode(&mut data).is_err() {
        return;
    }
    data.extend_from_slice(&BlockPos::floored_v(destination).as_long().to_be_bytes());
    if VarInt(ticks).encode(&mut data).is_err() {
        return;
    }
    world.spawn_particles_with_data(
        Particle::Vibration,
        origin,
        1,
        Vector3::new(0.0, 0.0, 0.0),
        0.0,
        &data,
    );
}

/// `VibrationSystem.Listener.isOccluded`: blocked when every line from the six nudged origins to
/// the listener passes a block that occludes vibration signals (wool and carpets).
const NUDGE: f64 = 1.0e-5;

fn is_occluded(world: &World, origin: Vector3<f64>, destination: Vector3<f64>) -> bool {
    let from = Vector3::new(
        origin.x.floor() + 0.5,
        origin.y.floor() + 0.5,
        origin.z.floor() + 0.5,
    );
    let to = Vector3::new(
        destination.x.floor() + 0.5,
        destination.y.floor() + 0.5,
        destination.z.floor() + 0.5,
    );
    [
        BlockDirection::Down,
        BlockDirection::Up,
        BlockDirection::North,
        BlockDirection::South,
        BlockDirection::West,
        BlockDirection::East,
    ]
    .iter()
    .all(|direction| {
        let offset = direction.to_offset();
        let nudged = Vector3::new(
            from.x + f64::from(offset.x) * NUDGE,
            from.y + f64::from(offset.y) * NUDGE,
            from.z + f64::from(offset.z) * NUDGE,
        );
        line_hits(nudged, to, |pos| {
            Block::from_state_id(world.get_block_state_id(&pos))
                .has_tag(&tag::Block::MINECRAFT_OCCLUDES_VIBRATION_SIGNALS)
        })
    })
}

/// `BlockGetter.traverseBlocks`: whether `hit` holds for any block the segment passes through.
fn line_hits(from: Vector3<f64>, to: Vector3<f64>, mut hit: impl FnMut(BlockPos) -> bool) -> bool {
    if from == to {
        return false;
    }
    let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
    let (to_x, to_y, to_z) = (
        lerp(-1.0e-7, to.x, from.x),
        lerp(-1.0e-7, to.y, from.y),
        lerp(-1.0e-7, to.z, from.z),
    );
    let (from_x, from_y, from_z) = (
        lerp(-1.0e-7, from.x, to.x),
        lerp(-1.0e-7, from.y, to.y),
        lerp(-1.0e-7, from.z, to.z),
    );
    let (mut x, mut y, mut z) = (
        from_x.floor() as i32,
        from_y.floor() as i32,
        from_z.floor() as i32,
    );
    if hit(BlockPos::new(x, y, z)) {
        return true;
    }
    let (dx, dy, dz) = (to_x - from_x, to_y - from_y, to_z - from_z);
    let (sx, sy, sz) = (
        dx.signum() as i32 * i32::from(dx != 0.0),
        dy.signum() as i32 * i32::from(dy != 0.0),
        dz.signum() as i32 * i32::from(dz != 0.0),
    );
    let t_delta = |s: i32, d: f64| if s == 0 { f64::MAX } else { f64::from(s) / d };
    let (tdx, tdy, tdz) = (t_delta(sx, dx), t_delta(sy, dy), t_delta(sz, dz));
    let frac = |v: f64| v - v.floor();
    let t_max = |s: i32, td: f64, v: f64| td * if s > 0 { 1.0 - frac(v) } else { frac(v) };
    let (mut tx, mut ty, mut tz) = (
        t_max(sx, tdx, from_x),
        t_max(sy, tdy, from_y),
        t_max(sz, tdz, from_z),
    );
    while tx <= 1.0 || ty <= 1.0 || tz <= 1.0 {
        if tx < ty {
            if tx < tz {
                x += sx;
                tx += tdx;
            } else {
                z += sz;
                tz += tdz;
            }
        } else if ty < tz {
            y += sy;
            ty += tdy;
        } else {
            z += sz;
            tz += tdz;
        }
        if hit(BlockPos::new(x, y, z)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redstone_strength_matches_vanilla() {
        assert_eq!(redstone_strength_for_distance(0.0, 8), 15);
        assert_eq!(redstone_strength_for_distance(4.0, 8), 8);
        assert_eq!(redstone_strength_for_distance(8.0, 8), 1);
        assert_eq!(redstone_strength_for_distance(16.0, 16), 1);
    }

    #[test]
    fn frequency_and_tags_use_names() {
        assert_eq!(event_frequency(GameEvent::Step), 1);
        assert_eq!(event_frequency(GameEvent::BlockPlace), 13);
        assert_eq!(event_frequency(GameEvent::Explode), 15);
        assert_eq!(event_frequency(GameEvent::Shriek), 0);
        assert!(event_has_tag(
            GameEvent::Flap,
            &tag::GameEvent::MINECRAFT_VIBRATIONS
        ));
        assert!(event_has_tag(
            GameEvent::SculkSensorTendrilsClicking,
            &tag::GameEvent::MINECRAFT_SHRIEKER_CAN_LISTEN
        ));
        assert!(!event_has_tag(
            GameEvent::Step,
            &tag::GameEvent::MINECRAFT_SHRIEKER_CAN_LISTEN
        ));
        for f in 1..=15 {
            assert_eq!(event_frequency(resonance_event(f).unwrap()), f);
        }
    }

    #[test]
    fn selector_prefers_closer_then_higher_frequency_within_a_tick() {
        let info = |event, distance| VibrationInfo {
            event,
            distance,
            pos: Vector3::new(0.0, 0.0, 0.0),
            source: None,
        };
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(GameEvent::Step, 5.0), 10);
        selector.add_candidate(info(GameEvent::BlockPlace, 5.0), 10);
        selector.add_candidate(info(GameEvent::Step, 6.0), 10);
        selector.add_candidate(info(GameEvent::Step, 1.0), 11);
        assert!(selector.chosen_candidate(10).is_none());
        assert_eq!(
            selector.chosen_candidate(11).unwrap().event,
            GameEvent::BlockPlace
        );
    }

    #[test]
    fn line_traversal_visits_the_blocks_in_between() {
        let mut seen = Vec::new();
        line_hits(
            Vector3::new(0.5, 0.5, 0.5),
            Vector3::new(3.5, 0.5, 0.5),
            |p| {
                seen.push(p.0.x);
                false
            },
        );
        assert_eq!(seen, vec![0, 1, 2, 3]);
    }
}
