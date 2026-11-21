use std::sync::Arc;

use uuid::Uuid;

use crate::entity::EntityBase;
use crate::world::World;

use super::super::memory::position_tracker::{BlockPosTracker, EntityTracker, PositionTracker};
use super::super::memory::walk_target::WalkTarget;
use super::super::memory::{MemoryModuleType, types};
use super::super::{BrainState, VisibilityContext};

#[must_use]
pub fn entity_is_visible(ctx: &VisibilityContext<'_>, target: &dyn EntityBase) -> bool {
    ctx.state
        .get(types::NEAREST_VISIBLE_LIVING_ENTITIES)
        .is_some_and(|visible| visible.contains(target, ctx))
}

#[must_use]
pub fn can_see(ctx: &VisibilityContext<'_>, target: &dyn EntityBase) -> bool {
    entity_is_visible(ctx, target)
}

#[must_use]
pub fn target_is_valid(
    ctx: &VisibilityContext<'_>,
    memory: MemoryModuleType<Arc<dyn EntityBase>>,
    predicate: impl Fn(&Arc<dyn EntityBase>) -> bool,
) -> bool {
    ctx.state.get(memory).is_some_and(|target| {
        predicate(target)
            && target.get_entity().is_alive()
            && entity_is_visible(ctx, target.as_ref())
    })
}

pub fn look_at_entity(brain: &mut BrainState, target: Arc<dyn EntityBase>) {
    brain.set(
        types::LOOK_TARGET,
        Arc::new(EntityTracker::new(target, true)) as Arc<dyn PositionTracker>,
    );
}

pub fn set_walk_and_look_target_memories(
    brain: &mut BrainState,
    target: Arc<dyn PositionTracker>,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    brain.set(types::LOOK_TARGET, Arc::clone(&target));
    brain.set(
        types::WALK_TARGET,
        WalkTarget::new(target, speed_modifier, close_enough_dist),
    );
}

pub fn set_walk_and_look_target_memories_to_entity(
    brain: &mut BrainState,
    target: Arc<dyn EntityBase>,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    set_walk_and_look_target_memories(
        brain,
        Arc::new(EntityTracker::new(target, true)),
        speed_modifier,
        close_enough_dist,
    );
}

pub fn set_walk_and_look_target_memories_to_block(
    brain: &mut BrainState,
    target: pumpkin_util::math::position::BlockPos,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    set_walk_and_look_target_memories(
        brain,
        Arc::new(BlockPosTracker::new(target)),
        speed_modifier,
        close_enough_dist,
    );
}

#[must_use]
pub fn get_target_nearest_me(
    body: &dyn EntityBase,
    first: Arc<dyn EntityBase>,
    second: Arc<dyn EntityBase>,
) -> Arc<dyn EntityBase> {
    let pos = body.get_entity().pos.load();
    let first_dist = pos.squared_distance_to_vec(&first.get_entity().pos.load());
    let second_dist = pos.squared_distance_to_vec(&second.get_entity().pos.load());
    if first_dist < second_dist {
        first
    } else {
        second
    }
}

#[must_use]
pub fn get_nearest_target(
    body: &dyn EntityBase,
    first: Option<Arc<dyn EntityBase>>,
    second: Arc<dyn EntityBase>,
) -> Arc<dyn EntityBase> {
    match first {
        Some(first) => get_target_nearest_me(body, first, second),
        None => second,
    }
}

#[must_use]
pub fn get_living_entity_from_uuid_memory(
    brain: &BrainState,
    world: &World,
    memory: MemoryModuleType<Uuid>,
) -> Option<Arc<dyn EntityBase>> {
    let uuid = *brain.get(memory)?;
    // Vanilla `ServerLevel.getEntity(uuid)` finds players too.
    let entity = world.get_entity_by_uuid(uuid).or_else(|| {
        world
            .get_player_by_uuid(uuid)
            .map(|player| player as Arc<dyn EntityBase>)
    })?;
    entity.get_living_entity().is_some().then_some(entity)
}

#[must_use]
pub fn is_other_target_much_further_away_than_current_attack_target(
    brain: &BrainState,
    body: &dyn EntityBase,
    other_target: &dyn EntityBase,
    how_much_further_away: f64,
) -> bool {
    let Some(current) = brain.get(types::ATTACK_TARGET) else {
        return false;
    };
    let pos = body.get_entity().pos.load();
    let dist_to_current = pos.squared_distance_to_vec(&current.get_entity().pos.load());
    let dist_to_other = pos.squared_distance_to_vec(&other_target.get_entity().pos.load());
    dist_to_other > dist_to_current + how_much_further_away * how_much_further_away
}

/// Vanilla `LivingEntity.isBaby` for ageable mobs.
#[must_use]
pub fn is_baby(entity: &dyn EntityBase) -> bool {
    entity
        .get_mob()
        .and_then(crate::entity::mob::Mob::as_ageable)
        .is_some_and(crate::entity::ageable::AgeableMob::is_baby)
}

#[must_use]
pub fn is_breeding(brain: &BrainState) -> bool {
    brain.has_memory_value(types::BREED_TARGET.id())
}

/// The item in one of the mob's hands.
fn item_in_slot(
    body: &dyn crate::entity::mob::Mob,
    slot: &pumpkin_data::data_component_impl::EquipmentSlot,
) -> pumpkin_data::item_stack::ItemStack {
    body.get_mob_entity()
        .living_entity
        .entity_equipment
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .equipment
        .get(slot)
        .cloned()
        .unwrap_or_else(|| pumpkin_data::item_stack::ItemStack::EMPTY.clone())
}

/// Vanilla `ProjectileWeaponItem.getDefaultProjectileRange` for bows and crossbows.
const fn default_projectile_range(item: &pumpkin_data::item::Item) -> Option<i32> {
    use pumpkin_data::item::Item;
    if item.id == Item::BOW.id {
        Some(15)
    } else if item.id == Item::CROSSBOW.id {
        Some(8)
    } else {
        None
    }
}

/// Vanilla `BehaviorUtils.isWithinAttackRange`: within the weapon's range less the margin for a
/// usable bow or crossbow in the main hand, otherwise within melee range.
#[must_use]
pub fn is_within_attack_range(
    body: &dyn crate::entity::mob::Mob,
    target: &dyn EntityBase,
    projectile_attack_range_margin: i32,
) -> bool {
    let main_hand = item_in_slot(
        body,
        &pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
    );
    if let Some(range) = default_projectile_range(main_hand.item)
        && body.can_use_non_melee_weapon(&main_hand)
    {
        let max_allowed_distance = f64::from(range - projectile_attack_range_margin);
        return body
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target.get_entity().pos.load())
            < max_allowed_distance * max_allowed_distance;
    }
    body.get_mob_entity().is_in_attack_range(target)
}

/// Vanilla `MeleeAttack.isHoldingUsableNonMeleeWeapon`: either hand holds a weapon the mob can
/// use from range.
#[must_use]
pub fn is_holding_usable_non_melee_weapon(body: &dyn crate::entity::mob::Mob) -> bool {
    [
        pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
        pumpkin_data::data_component_impl::EquipmentSlot::OFF_HAND,
    ]
    .iter()
    .any(|slot| body.can_use_non_melee_weapon(&item_in_slot(body, slot)))
}
