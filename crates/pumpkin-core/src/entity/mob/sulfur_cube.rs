use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};

use crate::entity::EntityBase;
use crate::entity::ai::goal::tempt::{TemptGoal, TemptMovement};
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::item::ItemEntity;
use crate::entity::mob::cube_mob::{
    ContactDamage, CubeKind, CubeMobEntity, CubeSounds, SizedSound, as_cube,
};
use crate::entity::mob::slime::SLIME;
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::player::Player;

pub const SPLIT_COUNT: i32 = 2;
pub const MAX_SIZE: i32 = 2;
/// Vanilla `SulfurCubeSearchForItemsGoal`'s search box, inflated on every axis.
const ITEM_SEARCH_RANGE: f64 = 8.0;

pub const SULFUR_CUBE: CubeKind = CubeKind {
    sounds: CubeSounds {
        jump: SizedSound {
            normal: Sound::EntitySulfurCubeJump,
            small: Sound::EntitySmallSulfurCubeJump,
        },
        squish: SizedSound {
            normal: Sound::EntitySulfurCubeSquish,
            small: Sound::EntitySmallSulfurCubeSquish,
        },
        hurt: SizedSound {
            normal: Sound::EntitySulfurCubeHurt,
            small: Sound::EntitySmallSulfurCubeHurt,
        },
        death: SizedSound {
            normal: Sound::EntitySulfurCubeDeath,
            small: Sound::EntitySmallSulfurCubeDeath,
        },
    },
    sound_category: SoundCategory::Neutral,
    health: |size| f64::from(4 * size),
    size_attributes: &[],
    contact_damage: ContactDamage::Never,
    swallows_items: true,
    split_count: || SPLIT_COUNT,
    // Babies, which spawn at size 1, are not implemented yet.
    spawn_size: |_| MAX_SIZE,
    register_goals: register_sulfur_cube_goals,
    ..SLIME
};

/// Vanilla `SulfurCube.addBehaviourGoals`. `addTargetingGoals` adds nothing: sulfur cubes never
/// attack.
fn register_sulfur_cube_goals(mob: &MobEntity) {
    mob.goals_selector.add_goal(
        2,
        Box::new(
            TemptGoal::with_stop_distance(1.0, is_tempted_by, false, 1.0).moving_with(
                TemptMovement {
                    navigate_towards: hop_towards_player,
                    stop_navigation: stop_hopping,
                },
                Controls::LOOK,
            ),
        ),
    );
    mob.goals_selector
        .add_goal(3, Box::new(SulfurCubeSearchForItemsGoal::default()));
}

/// Vanilla's tempt predicate for an adult (babies, tempted by `#sulfur_cube_food`, are not
/// implemented yet).
fn is_tempted_by(_mob: &dyn Mob, stack: &ItemStack) -> bool {
    CubeMobEntity::is_swallowable_item(stack)
}

/// Vanilla `SulfurCubeTemptGoal.navigateTowards`.
fn hop_towards_player(mob: &dyn Mob, player: &Player) {
    if let Some(cube) = as_cube(mob) {
        let yaw = cube.turn_towards(player.get_entity().pos.load());
        cube.set_direction(yaw, true);
    }
}

/// Vanilla `SulfurCubeTemptGoal.stopNavigation`.
fn stop_hopping(mob: &dyn Mob) {
    if let Some(cube) = as_cube(mob) {
        cube.set_wanted_movement(0.0);
    }
}

/// Vanilla `SulfurCube.SulfurCubeSearchForItemsGoal`: turns towards the nearest swallowable item
/// within 8 blocks and hops after it.
#[derive(Default)]
struct SulfurCubeSearchForItemsGoal {
    target_item: Option<Arc<dyn EntityBase>>,
}

impl SulfurCubeSearchForItemsGoal {
    /// Vanilla `SulfurCube.ALLOWED_ITEMS`.
    fn is_allowed(item: &ItemEntity) -> bool {
        item.get_pickup_delay() == 0
            && !item.get_entity().is_removed()
            && CubeMobEntity::is_swallowable_item(
                &item
                    .get_item_stack()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )
    }
}

impl Goal for SulfurCubeSearchForItemsGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.target_item = None;
        let Some(cube) = as_cube(mob) else {
            return false;
        };
        if cube.pickup_timer() > 0 {
            return false;
        }
        let entity = &cube.mob_entity.living_entity.entity;
        let pos = entity.pos.load();
        let area = entity.bounding_box.load().expand(
            ITEM_SEARCH_RANGE,
            ITEM_SEARCH_RANGE,
            ITEM_SEARCH_RANGE,
        );
        // Vanilla `getNearestEntity`: the first of the closest wins.
        let mut nearest: Option<(f64, Arc<dyn EntityBase>)> = None;
        for other in entity.world.load().get_entities_at_box(&area) {
            let Some(item) = other.cast_any().downcast_ref::<ItemEntity>() else {
                continue;
            };
            if !Self::is_allowed(item) {
                continue;
            }
            let distance = other.get_entity().pos.load().squared_distance_to_vec(&pos);
            if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
                nearest = Some((distance, other));
            }
        }
        self.target_item = nearest.map(|(_, item)| item);
        self.target_item.is_some()
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.target_item = None;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let (Some(cube), Some(item)) = (as_cube(mob), &self.target_item) else {
            return;
        };
        let yaw = cube.turn_towards(item.get_entity().pos.load());
        cube.set_direction(yaw, true);
    }

    fn controls(&self) -> Controls {
        Controls::LOOK
    }
}
