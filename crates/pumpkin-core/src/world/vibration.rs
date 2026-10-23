//! Vanilla `VibrationSystem`: game events picked up by a listener travel to it as vibrations.
//!
//! A listener that accepts an event keeps the closest candidate of that game tick. On a later tick
//! the candidate becomes the current vibration, which arrives one tick per block of travel.

use std::sync::{Mutex, MutexGuard, PoisonError};

use pumpkin_data::{
    BlockDirection,
    game_event::{GameEvent, PositionSourceType},
    particle::Particle,
    tag::{Tag, Taggable},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::{codec::var_int::VarInt, java::client::play::CParticle, ser::WritingError};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use uuid::Uuid;

use super::{
    World,
    game_event::{GameEventContext, GameEventListener},
};
use crate::entity::Entity;

const PARTICLE_SEND_DISTANCE: f64 = 32.0;

/// Vanilla `VibrationInfo`.
#[derive(Clone)]
pub struct VibrationInfo {
    pub event: GameEvent,
    pub distance: f32,
    pub pos: Vector3<f64>,
    pub source: Option<Uuid>,
    pub projectile_owner: Option<Uuid>,
}

impl VibrationInfo {
    fn read_nbt(nbt: &NbtCompound) -> Option<Self> {
        let event = GameEvent::from_name(nbt.get_string("game_event")?)?;
        let distance = nbt.get_float("distance")?;
        let pos = nbt.get_list("pos")?;
        if pos.len() != 3 || !distance.is_finite() || distance < 0.0 {
            return None;
        }
        let pos = Vector3::new(
            pos[0].extract_double()?,
            pos[1].extract_double()?,
            pos[2].extract_double()?,
        );
        if !pos.x.is_finite() || !pos.y.is_finite() || !pos.z.is_finite() {
            return None;
        }
        Some(Self {
            event,
            distance,
            pos,
            source: nbt.get_uuid("source"),
            projectile_owner: nbt.get_uuid("projectile_owner"),
        })
    }

    fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("game_event", format!("minecraft:{}", self.event.name()));
        nbt.put_float("distance", self.distance);
        nbt.put_list(
            "pos",
            vec![
                NbtTag::Double(self.pos.x),
                NbtTag::Double(self.pos.y),
                NbtTag::Double(self.pos.z),
            ],
        );
        if let Some(source) = self.source {
            nbt.put_uuid("source", source);
        }
        if let Some(owner) = self.projectile_owner {
            nbt.put_uuid("projectile_owner", owner);
        }
        nbt
    }
}

/// Vanilla `VibrationSelector`: of the vibrations offered in one game tick, keeps the closest.
/// It is chosen on a later tick.
#[derive(Default)]
pub struct VibrationSelector {
    candidate: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    /// Vanilla `VibrationSelector.addCandidate`.
    // TODO: vanilla breaks equal distances by `VibrationSystem.getGameEventFrequency`, which is
    // not extracted into `pumpkin-data` yet.
    pub fn add_candidate(&mut self, vibration: VibrationInfo, tick: i64) {
        if self.candidate.as_ref().is_none_or(|(previous, previous_tick)| {
            *previous_tick == tick && vibration.distance < previous.distance
        }) {
            self.candidate = Some((vibration, tick));
        }
    }

    /// Vanilla `VibrationSelector.chosenCandidate`: the candidate once its tick has passed.
    #[must_use]
    pub fn chosen_candidate(&self, time: i64) -> Option<&VibrationInfo> {
        self.candidate
            .as_ref()
            .filter(|(_, tick)| *tick < time)
            .map(|(vibration, _)| vibration)
    }

    /// Vanilla `VibrationSelector.startOver`.
    pub fn start_over(&mut self) {
        self.candidate = None;
    }

    #[must_use]
    pub const fn has_candidate(&self) -> bool {
        self.candidate.is_some()
    }

    fn read_nbt(nbt: &NbtCompound) -> Option<Self> {
        let tick = nbt.get_long("tick")?;
        Some(Self {
            candidate: nbt
                .get_compound("event")
                .and_then(VibrationInfo::read_nbt)
                .map(|event| (event, tick)),
        })
    }

    fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_long(
            "tick",
            self.candidate.as_ref().map_or(-1, |(_, tick)| *tick),
        );
        if let Some((event, _)) = &self.candidate {
            nbt.put_compound("event", event.write_nbt());
        }
        nbt
    }
}

/// Vanilla `VibrationSystem.Data`.
#[derive(Default)]
pub struct VibrationData {
    pub current: Option<VibrationInfo>,
    pub selector: VibrationSelector,
    pub travel_time_in_ticks: i32,
    reload_particle: bool,
}

impl VibrationData {
    #[must_use]
    pub fn read_nbt(nbt: &NbtCompound) -> Self {
        let Some(selector) = nbt
            .get_compound("selector")
            .and_then(VibrationSelector::read_nbt)
        else {
            return Self::default();
        };
        let travel_time_in_ticks = nbt.get_int("event_delay").unwrap_or(0);
        if travel_time_in_ticks < 0 {
            return Self::default();
        }
        Self {
            current: nbt.get_compound("event").and_then(VibrationInfo::read_nbt),
            selector,
            travel_time_in_ticks,
            reload_particle: true,
        }
    }

    #[must_use]
    pub fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if let Some(event) = &self.current {
            nbt.put_compound("event", event.write_nbt());
        }
        nbt.put_compound("selector", self.selector.write_nbt());
        nbt.put_int("event_delay", self.travel_time_in_ticks);
        nbt
    }
}

/// Vanilla `VibrationSystem.User`, for listeners whose position source is their entity's eyes.
pub trait VibrationUser: Send + Sync {
    fn vibration_data(&self) -> &Mutex<VibrationData>;

    /// The entity the vibration travels to, at its eye height.
    fn listener_entity(&self) -> &Entity;

    fn listener_radius(&self) -> i32;

    fn listenable_events(&self) -> &'static Tag;

    fn can_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        event: GameEvent,
        context: &GameEventContext<'_>,
    ) -> bool;

    fn on_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        vibration: &VibrationInfo,
        distance: f32,
    );

    fn travel_time_in_ticks(&self, distance: f32) -> i32 {
        distance.floor() as i32
    }

    /// Vanilla `VibrationSystem.User.isValidVibration`.
    fn is_valid_vibration(&self, event: GameEvent, context: &GameEventContext<'_>) -> bool {
        if !self.listenable_events().0.contains(&event.name()) {
            return false;
        }
        let Some(source) = context.source_entity else {
            return true;
        };
        let ignored_while_sneaking =
            pumpkin_data::tag::GameEvent::MINECRAFT_IGNORE_VIBRATIONS_SNEAKING
                .0
                .contains(&event.name());
        !source.is_spectator()
            && (!source.get_entity().is_sneaking() || !ignored_while_sneaking)
            && !source.dampens_vibrations()
    }
}

/// Vanilla `VibrationSystem.Listener`.
pub struct VibrationListener<'a>(pub &'a dyn VibrationUser);

impl GameEventListener for VibrationListener<'_> {
    fn listener_position(&self) -> Vector3<f64> {
        self.0.listener_entity().get_eye_pos()
    }

    fn listener_radius(&self) -> i32 {
        self.0.listener_radius()
    }

    fn handle_game_event(
        &self,
        world: &World,
        event: GameEvent,
        context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool {
        let user = self.0;
        if lock(user.vibration_data()).current.is_some()
            || !user.is_valid_vibration(event, context)
        {
            return false;
        }
        let destination = self.listener_position();
        if !user.can_receive_vibration(
            world,
            BlockPos::floored_v(source_position),
            event,
            context,
        ) || is_occluded(world, source_position, destination)
        {
            return false;
        }
        lock(user.vibration_data()).selector.add_candidate(
            VibrationInfo {
                event,
                distance: (source_position - destination).length() as f32,
                pos: source_position,
                source: context
                    .source_entity
                    .map(|source| source.get_entity().entity_uuid),
                projectile_owner: None,
            },
            world.get_world_age(),
        );
        true
    }
}

/// Vanilla `VibrationSystem.Ticker.tick`.
pub fn tick(user: &dyn VibrationUser, world: &World) {
    let mut data = lock(user.vibration_data());
    if data.current.is_none()
        && let Some(vibration) = data
            .selector
            .chosen_candidate(world.get_world_age())
            .cloned()
    {
        data.travel_time_in_ticks = user.travel_time_in_ticks(vibration.distance);
        send_particle(world, user, vibration.pos, data.travel_time_in_ticks);
        data.current = Some(vibration);
        data.selector.start_over();
    }
    let Some(current) = &data.current else {
        return;
    };
    if data.reload_particle {
        let initial = user.travel_time_in_ticks(current.distance);
        let alpha = 1.0 - f64::from(data.travel_time_in_ticks) / f64::from(initial);
        let pos = current
            .pos
            .lerp(&user.listener_entity().get_eye_pos(), alpha);
        if send_particle(world, user, pos, data.travel_time_in_ticks) {
            data.reload_particle = false;
        }
    }
    data.travel_time_in_ticks = (data.travel_time_in_ticks - 1).max(0);
    if data.travel_time_in_ticks > 0 {
        return;
    }
    let Some(vibration) = data.current.take() else {
        return;
    };
    drop(data);
    // Vanilla `VibrationSystem.Ticker.receiveVibration`.
    let origin = BlockPos::floored_v(vibration.pos);
    let destination = BlockPos::floored_v(user.listener_entity().get_eye_pos());
    let distance = f64::from(origin.squared_distance(&destination)).sqrt() as f32;
    user.on_receive_vibration(world, origin, &vibration, distance);
}

fn lock(data: &Mutex<VibrationData>) -> MutexGuard<'_, VibrationData> {
    data.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Vanilla `VibrationSystem.Listener.isOccluded`: blocked when the lines from all six nudged
/// origins pass through a block that occludes vibration signals.
fn is_occluded(world: &World, origin: Vector3<f64>, destination: Vector3<f64>) -> bool {
    let from = BlockPos::floored_v(origin).to_centered_f64();
    let to = BlockPos::floored_v(destination).to_centered_f64();
    BlockDirection::all().iter().all(|direction| {
        let nudged_source = from + direction.to_offset().to_f64() * f64::from(1.0e-5f32);
        pumpkin_world::clip::traverse_blocks(nudged_source, to, |pos| {
            world
                .get_block(pos)
                .has_tag(&pumpkin_data::tag::Block::MINECRAFT_OCCLUDES_VIBRATION_SIGNALS)
                .then_some(())
        })
        .is_some()
    })
}

/// Sends the `minecraft:vibration` particle travelling to the listener's eyes. Returns whether any
/// player was close enough to receive it.
fn send_particle(world: &World, user: &dyn VibrationUser, pos: Vector3<f64>, delay: i32) -> bool {
    let entity = user.listener_entity();
    let Ok(payload) = particle_payload(entity.entity_id, entity.get_eye_height() as f32, delay)
    else {
        return false;
    };
    let packet = CParticle::new(
        false,
        false,
        pos,
        Vector3::new(0.0, 0.0, 0.0),
        0.0,
        1,
        VarInt(i32::from(Particle::Vibration.to_id())),
        &payload,
    );
    let mut sent = false;
    for player in world.players.load().iter() {
        if player.position().squared_distance_to_vec(&pos)
            <= PARTICLE_SEND_DISTANCE * PARTICLE_SEND_DISTANCE
        {
            player.try_send_client_packet(&packet);
            sent = true;
        }
    }
    sent
}

/// `VibrationParticleOption` with an `EntityPositionSource` destination.
fn particle_payload(entity_id: i32, eye_offset: f32, delay: i32) -> Result<Vec<u8>, WritingError> {
    let mut payload = Vec::new();
    VarInt(PositionSourceType::Entity.to_id()).encode(&mut payload)?;
    VarInt(entity_id).encode(&mut payload)?;
    payload.extend_from_slice(&eye_offset.to_be_bytes());
    VarInt(delay).encode(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(event: GameEvent, distance: f32) -> VibrationInfo {
        VibrationInfo {
            event,
            distance,
            pos: Vector3::new(0.0, 0.0, 0.0),
            source: None,
            projectile_owner: None,
        }
    }

    #[test]
    fn entity_destination_particle_matches_vanilla_stream_codec() -> Result<(), WritingError> {
        assert_eq!(
            particle_payload(300, 0.36, 12)?,
            vec![1, 172, 2, 62, 184, 81, 236, 12]
        );
        Ok(())
    }

    #[test]
    fn selector_keeps_the_closest_vibration_of_its_tick() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(GameEvent::Step, 5.0), 10);
        selector.add_candidate(info(GameEvent::BlockPlace, 6.0), 10);
        selector.add_candidate(info(GameEvent::Flap, 3.0), 10);
        // A later tick never replaces the pending candidate, even when closer.
        selector.add_candidate(info(GameEvent::Step, 1.0), 11);
        assert!(selector.chosen_candidate(10).is_none());
        let chosen = selector.chosen_candidate(11).map(|chosen| chosen.event);
        assert_eq!(chosen, Some(GameEvent::Flap));
        selector.start_over();
        assert!(!selector.has_candidate());
    }

    #[test]
    fn data_round_trips_current_and_selector_through_nbt() {
        let source = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
        let owner = Uuid::from_u128(0x9999_aaaa_bbbb_cccc_dddd_eeee_ffff_0000);
        let mut data = VibrationData {
            current: Some(VibrationInfo {
                event: GameEvent::Step,
                distance: 3.0,
                pos: Vector3::new(10.5, 70.0, 20.5),
                source: Some(owner),
                projectile_owner: None,
            }),
            travel_time_in_ticks: 3,
            ..VibrationData::default()
        };
        data.selector.add_candidate(
            VibrationInfo {
                event: GameEvent::ProjectileLand,
                distance: 4.5,
                pos: Vector3::new(1.25, 64.0, -3.5),
                source: Some(source),
                projectile_owner: Some(owner),
            },
            42,
        );

        let restored = VibrationData::read_nbt(&data.write_nbt());
        assert!(restored.reload_particle);
        assert_eq!(restored.travel_time_in_ticks, 3);
        let current = restored
            .current
            .as_ref()
            .map(|current| (current.event, current.distance, current.pos, current.source));
        assert_eq!(
            current,
            Some((
                GameEvent::Step,
                3.0,
                Vector3::new(10.5, 70.0, 20.5),
                Some(owner)
            ))
        );
        let chosen = restored.selector.chosen_candidate(43).map(|chosen| {
            (
                chosen.event,
                chosen.distance,
                chosen.pos,
                chosen.source,
                chosen.projectile_owner,
            )
        });
        assert_eq!(
            chosen,
            Some((
                GameEvent::ProjectileLand,
                4.5,
                Vector3::new(1.25, 64.0, -3.5),
                Some(source),
                Some(owner)
            ))
        );
    }

    #[test]
    fn data_without_a_selector_or_with_a_negative_delay_resets() {
        let mut nbt = VibrationData::default().write_nbt();
        nbt.put_int("event_delay", -1);
        assert!(!VibrationData::read_nbt(&nbt).reload_particle);
        assert!(!VibrationData::read_nbt(&NbtCompound::new()).reload_particle);
    }
}
