use std::sync::Arc;

use pumpkin_data::Block;
use pumpkin_data::block_properties::CampfireLikeProperties;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;

use super::super::BrainTick;
use super::super::behavior::utils;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;
use super::piglin_brute_specific::find_nearby_adult_piglins;
use crate::entity::EntityBase;
use crate::entity::mob::abstract_piglin;
use crate::entity::mob::piglin::ai::{
    REPELLENT_DETECTION_RANGE_HORIZONTAL, REPELLENT_DETECTION_RANGE_VERTICAL,
    is_player_holding_loved_item, is_wearing_safe_armor, is_zombified,
};
use crate::entity::player::Player;
use crate::world::World;

const REQUIRES: [MemoryModuleId; 12] = [
    types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
    types::NEAREST_LIVING_ENTITIES.id(),
    types::NEAREST_VISIBLE_NEMESIS.id(),
    types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD.id(),
    types::NEAREST_PLAYER_HOLDING_WANTED_ITEM.id(),
    types::NEAREST_VISIBLE_HUNTABLE_HOGLIN.id(),
    types::NEAREST_VISIBLE_BABY_HOGLIN.id(),
    types::NEAREST_VISIBLE_ADULT_PIGLINS.id(),
    types::NEARBY_ADULT_PIGLINS.id(),
    types::VISIBLE_ADULT_PIGLIN_COUNT.id(),
    types::VISIBLE_ADULT_HOGLIN_COUNT.id(),
    types::NEAREST_REPELLENT.id(),
];

/// Vanilla `PiglinSpecificSensor` (`piglin_specific_sensor`).
///
/// What a piglin sees around it, sorted into nemesis, hoglins to hunt or ride, fellow piglins, players and zombified piglins, plus the
/// nearest repellent block.
pub struct PiglinSpecificSensor;

impl Sensor for PiglinSpecificSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let repellent = find_nearest_repellent(tick.world, tick.mob.get_entity().block_pos.load());
        tick.state.set_optional(types::NEAREST_REPELLENT, repellent);

        let mut nemesis: Option<Arc<dyn EntityBase>> = None;
        let mut huntable_hoglin: Option<Arc<dyn EntityBase>> = None;
        let mut baby_hoglin: Option<Arc<dyn EntityBase>> = None;
        let mut baby_piglin_seen = false;
        let mut zombified: Option<Arc<dyn EntityBase>> = None;
        let mut player_not_wearing_gold: Option<Arc<Player>> = None;
        let mut player_holding_wanted_item: Option<Arc<Player>> = None;
        let mut visible_adult_hoglin_count = 0;
        let mut visible_adult_piglins: Vec<Arc<dyn EntityBase>> = Vec::new();

        let visible: Vec<Arc<dyn EntityBase>> = {
            let ctx = tick.visibility();
            tick.state
                .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
                .map(|visible| visible.find_all(&ctx, |_| true).cloned().collect())
                .unwrap_or_default()
        };
        let world = tick.world;
        for entity in visible {
            let entity_type = entity.get_entity().entity_type;
            if entity_type == &EntityType::HOGLIN {
                if utils::is_baby(entity.as_ref()) {
                    if baby_hoglin.is_none() {
                        baby_hoglin = Some(entity);
                    }
                } else {
                    visible_adult_hoglin_count += 1;
                    if huntable_hoglin.is_none()
                        && abstract_piglin::hoglin_can_be_hunted(entity.as_ref())
                    {
                        huntable_hoglin = Some(entity);
                    }
                }
            } else if entity_type == &EntityType::PIGLIN_BRUTE {
                visible_adult_piglins.push(entity);
            } else if entity_type == &EntityType::PIGLIN {
                if utils::is_baby(entity.as_ref()) {
                    // Vanilla tracks the first baby piglin but never stores it.
                    baby_piglin_seen = true;
                } else {
                    visible_adult_piglins.push(entity);
                }
            } else if entity_type == &EntityType::PLAYER
                && let Some(player) = world.get_player_by_id(entity.get_entity().entity_id)
            {
                if player_not_wearing_gold.is_none()
                    && !is_wearing_safe_armor(&player.living_entity)
                    && tick.mob.can_attack(entity.as_ref())
                {
                    player_not_wearing_gold = Some(player.clone());
                }
                if player_holding_wanted_item.is_none()
                    && !player.is_spectator()
                    && is_player_holding_loved_item(&player)
                {
                    player_holding_wanted_item = Some(player);
                }
            } else if nemesis.is_some()
                || (entity_type != &EntityType::WITHER_SKELETON
                    && entity_type != &EntityType::WITHER)
            {
                if zombified.is_none() && is_zombified(entity_type) {
                    zombified = Some(entity);
                }
            } else {
                nemesis = Some(entity);
            }
        }
        let _ = baby_piglin_seen;

        let adult_piglins = find_nearby_adult_piglins(tick);
        let visible_adult_piglin_count =
            i32::try_from(visible_adult_piglins.len()).unwrap_or(i32::MAX);
        let state = &mut *tick.state;
        state.set_optional(types::NEAREST_VISIBLE_NEMESIS, nemesis);
        state.set_optional(types::NEAREST_VISIBLE_HUNTABLE_HOGLIN, huntable_hoglin);
        state.set_optional(types::NEAREST_VISIBLE_BABY_HOGLIN, baby_hoglin);
        state.set_optional(types::NEAREST_VISIBLE_ZOMBIFIED, zombified);
        state.set_optional(
            types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD,
            player_not_wearing_gold,
        );
        state.set_optional(
            types::NEAREST_PLAYER_HOLDING_WANTED_ITEM,
            player_holding_wanted_item,
        );
        state.set(types::NEARBY_ADULT_PIGLINS, adult_piglins);
        state.set(types::NEAREST_VISIBLE_ADULT_PIGLINS, visible_adult_piglins);
        state.set(
            types::VISIBLE_ADULT_PIGLIN_COUNT,
            visible_adult_piglin_count,
        );
        state.set(
            types::VISIBLE_ADULT_HOGLIN_COUNT,
            visible_adult_hoglin_count,
        );
    }
}

/// Vanilla `PiglinSpecificSensor.findNearestRepellent`: the first `#piglin_repellents` block (a
/// soul campfire only when lit) in vanilla's Manhattan-ordered walk of the 8x4x8 box.
fn find_nearest_repellent(world: &World, origin: BlockPos) -> Option<BlockPos> {
    manhattan_ordered(
        origin,
        REPELLENT_DETECTION_RANGE_HORIZONTAL,
        REPELLENT_DETECTION_RANGE_VERTICAL,
        REPELLENT_DETECTION_RANGE_HORIZONTAL,
    )
    .find(|pos| {
        let (block, state) = world.get_block_and_state(pos);
        block.has_tag(&tag::Block::MINECRAFT_PIGLIN_REPELLENTS)
            && (block.id != Block::SOUL_CAMPFIRE.id
                || CampfireLikeProperties::from_state_id(state.id).lit)
    })
}

/// Vanilla `BlockPos.withinBoxByManhattanDistance`: every position of the box, nearest Manhattan
/// shell first, in vanilla's x, y, then +z/-z order.
fn manhattan_ordered(
    origin: BlockPos,
    reach_x: i32,
    reach_y: i32,
    reach_z: i32,
) -> impl Iterator<Item = BlockPos> {
    let max_depth = reach_x + reach_y + reach_z;
    (0..=max_depth).flat_map(move |depth| {
        let max_x = reach_x.min(depth);
        (-max_x..=max_x).flat_map(move |x| {
            let max_y = reach_y.min(depth - x.abs());
            (-max_y..=max_y).flat_map(move |y| {
                let z = depth - x.abs() - y.abs();
                let at = |z: i32| BlockPos::new(origin.0.x + x, origin.0.y + y, origin.0.z + z);
                let first = (z <= reach_z).then(|| at(z));
                let mirror = (z <= reach_z && z != 0).then(|| at(-z));
                first.into_iter().chain(mirror)
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use pumpkin_util::math::position::BlockPos;

    use super::manhattan_ordered;

    #[test]
    fn manhattan_walk_covers_the_box_nearest_first() {
        let origin = BlockPos::new(0, 0, 0);
        let positions: Vec<BlockPos> = manhattan_ordered(origin, 8, 4, 8).collect();
        assert_eq!(positions.len(), 17 * 9 * 17);
        assert_eq!(positions[0], origin);
        // Vanilla's order at depth 1: x=-1, then y=-1, z=+1, z=-1, y=+1, then x=+1.
        assert_eq!(
            &positions[1..7],
            &[
                BlockPos::new(-1, 0, 0),
                BlockPos::new(0, -1, 0),
                BlockPos::new(0, 0, 1),
                BlockPos::new(0, 0, -1),
                BlockPos::new(0, 1, 0),
                BlockPos::new(1, 0, 0),
            ]
        );
        let depths: Vec<i32> = positions
            .iter()
            .map(|pos| pos.0.x.abs() + pos.0.y.abs() + pos.0.z.abs())
            .collect();
        assert!(depths.windows(2).all(|pair| pair[0] <= pair[1]));
    }
}
