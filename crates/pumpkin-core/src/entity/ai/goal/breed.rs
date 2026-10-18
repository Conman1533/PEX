use std::sync::Arc;

use crate::entity::{EntityBase, ai::pathfinder::NavigatorGoal, mob::Mob};

use super::{Controls, Goal};

/// Vanilla `BreedGoal.PARTNER_TARGETING` range.
const PARTNER_RANGE: f64 = 8.0;
const BREED_DISTANCE_SQ: f64 = 9.0;
const LOVE_TIME: i32 = 60;

pub struct BreedGoal {
    speed: f64,
    partner: Option<Arc<dyn EntityBase>>,
    love_time: i32,
}

impl BreedGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            speed,
            partner: None,
            love_time: 0,
        })
    }

    /// Vanilla `BreedGoal.getFreePartner`: the closest animal this one can mate with that is not
    /// panicking.
    fn get_free_partner(mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let animal = mob.as_animal()?;
        let entity = mob.get_entity();
        let pos = entity.pos.load();
        let search_box =
            entity
                .bounding_box
                .load()
                .expand(PARTNER_RANGE, PARTNER_RANGE, PARTNER_RANGE);

        let mut closest: Option<(f64, Arc<dyn EntityBase>)> = None;
        for candidate in entity.world.load().get_entities_at_box(&search_box) {
            let Some(candidate_mob) = candidate.get_mob() else {
                continue;
            };
            let dist_sq = pos.squared_distance_to_vec(&candidate.get_entity().pos.load());
            if dist_sq > PARTNER_RANGE * PARTNER_RANGE
                || !animal.can_mate(candidate_mob)
                || candidate.is_panicking()
            {
                continue;
            }
            if closest
                .as_ref()
                .is_none_or(|(best_dist_sq, _)| dist_sq < *best_dist_sq)
            {
                closest = Some((dist_sq, candidate.clone()));
            }
        }

        closest.map(|(_, partner)| partner)
    }
}

impl Goal for BreedGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if !mob.get_mob_entity().is_in_love() {
            return false;
        }
        self.partner = Self::get_free_partner(mob);
        self.partner.is_some()
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        let Some(partner) = &self.partner else {
            return false;
        };
        partner.get_entity().is_alive()
            && partner
                .get_mob()
                .is_some_and(|partner| partner.get_mob_entity().is_in_love())
            && self.love_time < LOVE_TIME
            && !partner.is_panicking()
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.partner = None;
        self.love_time = 0;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(partner) = self.partner.clone() else {
            return;
        };
        let mob_entity = mob.get_mob_entity();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&partner, 10.0, mob.get_max_look_pitch_change());

        let pos = mob.get_entity().pos.load();
        let partner_pos = partner.get_entity().pos.load();
        mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_progress(NavigatorGoal::new(pos, partner_pos, self.speed));

        self.love_time += 1;
        if self.love_time >= self.get_tick_count(LOVE_TIME)
            && pos.squared_distance_to_vec(&partner_pos) < BREED_DISTANCE_SQ
            && let Some(animal) = mob.as_animal()
            && let Some(partner) = partner.get_mob()
        {
            animal.spawn_child_from_breeding(&mob.get_entity().world.load_full(), partner);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
