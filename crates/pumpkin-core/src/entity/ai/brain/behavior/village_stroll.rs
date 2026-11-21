//! Vanilla `VillageBoundRandomStroll`, `GoToClosestVillage`, `InsideBrownianWalk` and
//! `SetClosestHomeAsWalkTarget`.

use std::f64::consts::FRAC_PI_2;

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use rand::seq::SliceRandom;
use rustc_hash::FxHashMap;

use super::super::BrainTick;
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryStatus, types};
use super::OneShot;
use super::acquire_poi::{SCAN_RANGE, find_path_to_pois};
use crate::entity::ai::util::{default_random_pos, land_random_pos};
use crate::world::villager_poi::{self, Occupancy, PoiKind, VillageSections, dist_sqr, section_of};

/// A snapshot of the village sections around the mob, after scanning its surroundings.
#[must_use]
pub fn village_sections(tick: &BrainTick<'_>) -> VillageSections {
    villager_poi::ensure_scanned(
        tick.world,
        tick.mob.get_entity().block_pos.load(),
        SCAN_RANGE,
    );
    tick.world
        .villager_poi
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .village_sections()
}

/// The bottom center of a section's center block, vanilla
/// `Vec3.atBottomCenterOf(sectionPos.center())`.
fn section_bottom_center(section: (i32, i32, i32)) -> Vector3<f64> {
    Vector3::new(
        f64::from(section.0 * 16 + 8) + 0.5,
        f64::from(section.1 * 16 + 8),
        f64::from(section.2 * 16 + 8) + 0.5,
    )
}

/// Vanilla `VillageBoundRandomStroll.create(speedModifier)`.
#[must_use]
pub fn village_bound_random_stroll(speed_modifier: f32) -> OneShot {
    village_bound_random_stroll_range(speed_modifier, 10, 7)
}

/// Vanilla `VillageBoundRandomStroll.create(speedModifier, maxXyDist, maxYDist)`: a random land
/// spot inside the village, or towards it from outside.
#[must_use]
pub fn village_bound_random_stroll_range(
    speed_modifier: f32,
    max_xz_dist: i32,
    max_y_dist: i32,
) -> OneShot {
    OneShot::new(
        "VillageBoundRandomStroll",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        move |tick| {
            let body = tick.mob.get_entity().block_pos.load();
            let sections = village_sections(tick);
            let land = if sections.is_village(body) {
                land_random_pos::get_pos(tick.mob, max_xz_dist, max_y_dist)
            } else {
                let section = section_of(body);
                let optimal = sections.find_section_closest_to_village(section, 2);
                if optimal == section {
                    land_random_pos::get_pos(tick.mob, max_xz_dist, max_y_dist)
                } else {
                    default_random_pos::get_pos_towards(
                        tick.mob,
                        max_xz_dist,
                        max_y_dist,
                        section_bottom_center(optimal),
                        FRAC_PI_2,
                    )
                }
            };
            tick.state.set_optional(
                types::WALK_TARGET,
                land.map(|pos| WalkTarget::from_vec(pos, speed_modifier, 0)),
            );
            true
        },
    )
}

/// Vanilla `GoToClosestVillage.create`: outside a village, walks to land that is closer to one.
#[must_use]
pub fn go_to_closest_village(speed_modifier: f32, close_enough_distance: i32) -> OneShot {
    OneShot::new(
        "GoToClosestVillage",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        move |tick| {
            let body = tick.mob.get_entity().block_pos.load();
            let sections = village_sections(tick);
            if sections.is_village(body) {
                return false;
            }
            let here = sections.sections_to_village(body);
            let mut target = None;
            for _ in 0..5 {
                let Some(land) = land_random_pos::get_pos_scored(tick.mob, 15, 7, |pos| {
                    -f64::from(sections.sections_to_village(*pos))
                }) else {
                    continue;
                };
                let there = sections.sections_to_village(BlockPos::floored_v(land));
                if there < here {
                    target = Some(land);
                    break;
                }
                if there == here {
                    target = Some(land);
                }
            }
            if let Some(target) = target {
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::from_vec(target, speed_modifier, close_enough_distance),
                );
            }
            true
        },
    )
}

/// Vanilla `InsideBrownianWalk.create`: indoors, shuffles to a neighbouring covered spot.
#[must_use]
pub fn inside_brownian_walk(speed_modifier: f32) -> OneShot {
    OneShot::new(
        "InsideBrownianWalk",
        vec![(types::WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        move |tick| {
            let body = tick.mob.get_entity().block_pos.load();
            if tick.world.can_see_sky(&body) {
                return false;
            }
            let mut poses: Vec<BlockPos> = BlockPos::iterate(
                BlockPos::new(body.0.x - 1, body.0.y - 1, body.0.z - 1),
                BlockPos::new(body.0.x + 1, body.0.y + 1, body.0.z + 1),
            )
            .collect();
            poses.shuffle(&mut tick.mob.get_random());
            if let Some(target) = poses.into_iter().find(|pos| {
                !tick.world.can_see_sky(pos) && tick.world.get_block_state(pos).is_solid()
            }) {
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::from_block_pos(target, speed_modifier, 0),
                );
            }
            true
        },
    )
}

/// Vanilla `SetClosestHomeAsWalkTarget`'s `CACHE_TIMEOUT`.
const CACHE_TIMEOUT: i64 = 40;
/// Vanilla `SetClosestHomeAsWalkTarget`'s `BATCH_SIZE`.
const BATCH_SIZE: usize = 5;
/// Vanilla `SetClosestHomeAsWalkTarget`'s `RATE`.
const RATE: i64 = 20;
/// Vanilla `SetClosestHomeAsWalkTarget`'s `OK_DISTANCE_SQR`.
const OK_DISTANCE_SQR: i64 = 4;

/// Vanilla `SetClosestHomeAsWalkTarget.create`: a villager without a bed of its own walks to the
/// closest reachable bed.
#[must_use]
pub fn set_closest_home_as_walk_target(speed_modifier: f32) -> OneShot {
    let mut batch_cache: FxHashMap<BlockPos, i64> = FxHashMap::default();
    let mut last_update = 0i64;
    OneShot::new(
        "SetClosestHomeAsWalkTarget",
        vec![
            (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (types::HOME.id(), MemoryStatus::ValueAbsent),
        ],
        move |tick| {
            if tick.time - last_update < RATE {
                return false;
            }
            let body = tick.mob.get_entity().block_pos.load();
            villager_poi::ensure_scanned(tick.world, body, SCAN_RANGE);
            let homes = tick
                .world
                .villager_poi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .find_all_closest_first(
                    body,
                    SCAN_RANGE,
                    |kind| kind == PoiKind::Home,
                    Occupancy::Any,
                );
            let Some((closest, _)) = homes.first().copied() else {
                return false;
            };
            if dist_sqr(closest, body) <= OK_DISTANCE_SQR {
                return false;
            }
            last_update = tick.time + i64::from(tick.mob.get_random().random_range(0..20));
            let mut tried = 0;
            let mut batch = Vec::new();
            for (pos, kind) in homes {
                if batch_cache.contains_key(&pos) {
                    continue;
                }
                tried += 1;
                if tried >= BATCH_SIZE {
                    break;
                }
                batch_cache.insert(pos, last_update + CACHE_TIMEOUT);
                batch.push((pos, kind));
            }
            if let Some(target) = find_path_to_pois(tick, &batch) {
                tick.state.set(
                    types::WALK_TARGET,
                    WalkTarget::from_block_pos(target, speed_modifier, 1),
                );
            } else if tried < BATCH_SIZE {
                batch_cache.retain(|_, expiry| *expiry >= last_update);
            }
            true
        },
    )
}
