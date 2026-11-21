use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::util::land_random_pos;
use crate::entity::mob::Mob;
use crate::world::villager_poi::{self, Occupancy};

/// How far around the mob village POIs are scanned before the goal looks for one: the 15 block
/// random position plus the 10 block POI search around it.
const SCAN_RANGE: i32 = 25;
/// Vanilla keeps the last 16 visited POIs.
const MAX_VISITED: usize = 15;

/// Vanilla `MoveThroughVillageGoal`: a mob near a village walks from one occupied village POI
/// (bed, job site or bell) to the next, not returning to the last ones it reached.
///
/// PEX's navigator plans the path itself, so the goal hands it the POI and skips vanilla's
/// partial path toward it and the cut short of the path at the first wooden door.
pub struct MoveThroughVillageGoal {
    speed_modifier: f64,
    only_at_night: bool,
    distance_to_poi: i32,
    poi_pos: Option<BlockPos>,
    visited: Vec<BlockPos>,
}

impl MoveThroughVillageGoal {
    #[must_use]
    pub fn new(speed_modifier: f64, only_at_night: bool, distance_to_poi: i32) -> Box<Self> {
        Box::new(Self {
            speed_modifier,
            only_at_night,
            distance_to_poi,
            poi_pos: None,
            visited: Vec::new(),
        })
    }

    fn has_not_visited(&self, poi: &BlockPos) -> bool {
        !self.visited.contains(poi)
    }

    fn update_visited(&mut self) {
        if self.visited.len() > MAX_VISITED {
            self.visited.remove(0);
        }
    }

    /// The closest occupied village POI within 10 blocks of `origin` not visited yet.
    fn find_poi(&self, mob: &dyn Mob, origin: BlockPos) -> Option<BlockPos> {
        let world = mob.get_entity().world.load();
        let storage = world
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        storage
            .find_all_closest_first(origin, 10, |_| true, Occupancy::IsOccupied)
            .into_iter()
            .map(|(position, _)| position)
            .find(|position| self.has_not_visited(position))
    }

    /// Vanilla `BlockPos.closerToCenterThan`.
    fn close_to(mob: &dyn Mob, poi: BlockPos, distance: f64) -> bool {
        let center = Vector3::new(
            f64::from(poi.0.x) + 0.5,
            f64::from(poi.0.y) + 0.5,
            f64::from(poi.0.z) + 0.5,
        );
        center.squared_distance_to_vec(&mob.get_entity().pos.load()) < distance * distance
    }
}

impl Goal for MoveThroughVillageGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.update_visited();
        let world = mob.get_entity().world.load_full();
        if self.only_at_night && world.is_bright_outside() {
            return false;
        }
        let pos = mob.get_entity().block_pos.load();
        villager_poi::ensure_scanned(&world, pos, SCAN_RANGE);
        let sections = world
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .village_sections();
        // Vanilla `ServerLevel.isCloseToVillage(pos, 6)`.
        if sections.sections_to_village(pos) > 6 {
            return false;
        }
        let Some(land_pos) = land_random_pos::get_pos_scored(mob, 15, 7, |candidate| {
            if !sections.is_village(*candidate) {
                return f64::NEG_INFINITY;
            }
            self.find_poi(mob, *candidate)
                .map_or(f64::NEG_INFINITY, |poi| {
                    let delta = poi.0 - pos.0;
                    -(f64::from(delta.x).powi(2)
                        + f64::from(delta.y).powi(2)
                        + f64::from(delta.z).powi(2))
                })
        }) else {
            return false;
        };
        let Some(poi) = self.find_poi(mob, BlockPos::floored_v(land_pos)) else {
            return false;
        };
        self.poi_pos = Some(poi);
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(poi) = self.poi_pos else {
            return false;
        };
        let width = f64::from(mob.get_entity().entity_dimension.load().width);
        !mob.is_navigator_idle()
            && !Self::close_to(mob, poi, width + f64::from(self.distance_to_poi))
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(poi) = self.poi_pos else {
            return;
        };
        mob.get_mob_entity()
            .steered()
            .navigator()
            .set_progress(NavigatorGoal {
                current_progress: mob.get_entity().pos.load(),
                destination: Vector3::new(
                    f64::from(poi.0.x) + 0.5,
                    f64::from(poi.0.y),
                    f64::from(poi.0.z) + 0.5,
                ),
                speed: self.speed_modifier,
                reach_range: 0,
            });
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let Some(poi) = self.poi_pos else {
            return;
        };
        if mob.is_navigator_idle() || Self::close_to(mob, poi, f64::from(self.distance_to_poi)) {
            self.visited.push(poi);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
