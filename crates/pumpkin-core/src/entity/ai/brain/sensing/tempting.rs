use std::sync::Arc;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::entity::player::Player;

const REQUIRES: [MemoryModuleId; 1] = [types::TEMPTING_PLAYER.id()];

/// Vanilla's temptation predicate, which also gets the tempted mob.
pub type Temptations = fn(&dyn Mob, &ItemStack) -> bool;

/// Vanilla `TemptingSensor`: the nearest player holding a temptation in either hand, within the
/// mob's `tempt_range`, line of sight ignored.
pub struct TemptingSensor {
    temptations: Temptations,
}

impl TemptingSensor {
    #[must_use]
    pub const fn new(temptations: Temptations) -> Self {
        Self { temptations }
    }

    /// Vanilla `TemptingSensor.forAnimal`, the `food_temptations` sensor.
    #[must_use]
    pub const fn for_animal() -> Self {
        Self::new(animal_food)
    }

    /// The `frog_temptations` sensor: vanilla `FrogAi.getTemptations`.
    #[must_use]
    pub const fn for_frog() -> Self {
        Self::new(frog_food)
    }

    /// The `nautilus_temptations` sensor: vanilla `NautilusAi.getTemptations`.
    #[must_use]
    pub const fn for_nautilus() -> Self {
        Self::new(nautilus_food)
    }

    fn player_holding_temptation(&self, mob: &dyn Mob, player: &Player) -> bool {
        let inventory = player.inventory();
        (self.temptations)(mob, &inventory.held_item())
            || (self.temptations)(mob, &inventory.off_hand_item())
    }
}

fn animal_food(mob: &dyn Mob, stack: &ItemStack) -> bool {
    mob.as_animal().is_some_and(|animal| animal.is_food(stack))
}

fn frog_food(_mob: &dyn Mob, stack: &ItemStack) -> bool {
    stack.item.has_tag(&tag::Item::MINECRAFT_FROG_FOOD)
}

fn nautilus_food(_mob: &dyn Mob, stack: &ItemStack) -> bool {
    stack.item.has_tag(&tag::Item::MINECRAFT_NAUTILUS_FOOD)
}

impl Sensor for TemptingSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let range = tick
            .mob
            .get_mob_entity()
            .living_entity
            .get_attribute_value(&Attributes::TEMPT_RANGE);
        let targeting = TargetPredicate::create_non_attackable()
            .ignore_visibility()
            .set_base_max_distance(range);
        let body = tick.mob.get_entity();
        let position = body.pos.load();
        let tempting = tick
            .world
            .players
            .load()
            .iter()
            .filter(|player| {
                !player.is_spectator()
                    && targeting.test(tick.world, Some(tick.mob), player.as_ref())
                    && self.player_holding_temptation(tick.mob, player)
                    && !body.has_passenger(player.entity_id())
            })
            .min_by(|a, b| {
                position
                    .squared_distance_to_vec(&a.living_entity.entity.pos.load())
                    .total_cmp(
                        &position.squared_distance_to_vec(&b.living_entity.entity.pos.load()),
                    )
            })
            .map(Arc::clone);
        match tempting {
            Some(player) => tick.state.set(types::TEMPTING_PLAYER, player),
            None => tick.state.erase(types::TEMPTING_PLAYER.id()),
        }
    }
}
