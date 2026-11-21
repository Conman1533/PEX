use super::super::memory::{MemoryStatus, types};
use super::{OneShot, utils};

/// Vanilla `BackUpIfTooClose`: a mob closer than `too_close_distance` to an attack target it can
/// see strafes backwards, looking at it, with its body turned to its head.
#[must_use]
pub fn create(too_close_distance: i32, strafe_speed: f32) -> OneShot {
    let too_close_sqr = f64::from(too_close_distance * too_close_distance);
    OneShot::new(
        "BackUpIfTooClose",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::LOOK_TARGET.id(), MemoryStatus::Registered),
            (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
            (
                types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                MemoryStatus::ValuePresent,
            ),
        ],
        move |tick| {
            let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
                return false;
            };
            let body = tick.mob.get_entity();
            let too_close = target
                .get_entity()
                .pos
                .load()
                .squared_distance_to_vec(&body.pos.load())
                < too_close_sqr;
            if !too_close || !utils::entity_is_visible(&tick.visibility(), target.as_ref()) {
                return false;
            }
            utils::look_at_entity(tick.state, target);
            tick.mob
                .get_mob_entity()
                .move_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .strafe(-strafe_speed, 0.0);
            // Vanilla `Mth.rotateIfNecessary(yRot, yHeadRot, 0)`: the body takes the head's yaw.
            body.yaw.store(body.head_yaw.load());
            true
        },
    )
}
