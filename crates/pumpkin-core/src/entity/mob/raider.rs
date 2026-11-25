use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;

use crate::entity::EntityBase;
use crate::entity::ai::brain::behavior::utils::is_baby;
use crate::entity::ai::goal::goal_selector::GoalSelector;
use crate::entity::ai::goal::open_door::OpenDoorGoal;
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::pathfinder::path::Path;
use crate::entity::item::ItemEntity;
use crate::entity::mob::Mob;
use crate::entity::mob::patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster};
use crate::world::raid::Raid;

#[must_use]
pub fn create_ominous_banner() -> ItemStack {
    use pumpkin_data::data_component::DataComponent;
    use pumpkin_data::data_component_impl::{CustomNameImpl, DataComponentImpl};
    let mut stack = ItemStack::new(1, &Item::WHITE_BANNER);
    stack.patch.push((
        DataComponent::CustomName,
        Some(
            CustomNameImpl {
                name: TextComponent::translate("block.minecraft.ominous_banner", []),
            }
            .to_dyn(),
        ),
    ));
    stack
}

/// Vanilla `ItemStack.matches(stack, Raid.getOminousBannerInstance(..))`.
#[must_use]
pub fn is_ominous_banner(stack: &ItemStack) -> bool {
    stack.item_count == 1 && stack.are_items_and_components_equal(&create_ominous_banner())
}

/// Vanilla `PatrollingMonster.registerGoals`, then `Raider.registerGoals`. Every raider registers
/// these before its own goals, and the insertion order breaks priority ties.
pub fn register_raider_goals(goal_selector: &GoalSelector) {
    goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
    goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal::default()));
    goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
    goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
    goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
}

/// Vanilla `AbstractVillager`: villagers and wandering traders.
#[must_use]
pub fn is_abstract_villager(entity_type: &EntityType) -> bool {
    entity_type == &EntityType::VILLAGER || entity_type == &EntityType::WANDERING_TRADER
}

/// Vanilla `AbstractIllager.canAttack`, which the ravager's villager target predicate matches:
/// raiders leave baby villagers alone.
#[must_use]
pub fn raider_can_attack(mob: &dyn Mob, target: &dyn EntityBase) -> bool {
    let target_type = target.get_entity().entity_type;
    if is_abstract_villager(target_type) && is_baby(target) {
        return false;
    }
    // `Mob::can_attack`'s default.
    target_type != &EntityType::GHAST && mob.get_mob_entity().living_entity.can_attack(target)
}

/// The raid the raider belongs to, while the world still has it.
fn with_current_raid<R>(raider: &dyn Raider, f: impl FnOnce(&Raid) -> R) -> Option<R> {
    let raid_id = raider.get_raider_data().raid_id.load()?;
    let world = raider.get_mob_entity().living_entity.entity.world.load();
    let raids = world
        .raids
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    raids.get(raid_id).map(f)
}

pub struct RaiderData {
    pub patrol_data: PatrolData,
    pub wave: AtomicI32,
    pub can_join_raid: AtomicBool,
    pub ticks_outside_raid: AtomicI32,
    pub is_celebrating: AtomicBool,
    pub raid_id: AtomicCell<Option<i32>>,
}

impl Default for RaiderData {
    fn default() -> Self {
        Self {
            patrol_data: PatrolData::default(),
            wave: AtomicI32::new(0),
            can_join_raid: AtomicBool::new(false),
            ticks_outside_raid: AtomicI32::new(0),
            is_celebrating: AtomicBool::new(false),
            raid_id: AtomicCell::new(None),
        }
    }
}

pub trait Raider: PatrollingMonster {
    fn get_raider_data(&self) -> &RaiderData;

    fn can_join_raid(&self) -> bool {
        self.get_raider_data().can_join_raid.load(Ordering::Relaxed)
    }

    fn set_can_join_raid(&self, can_join: bool) {
        self.get_raider_data()
            .can_join_raid
            .store(can_join, Ordering::Relaxed);
    }

    fn get_wave(&self) -> i32 {
        self.get_raider_data().wave.load(Ordering::Relaxed)
    }

    fn set_wave(&self, wave: i32) {
        self.get_raider_data().wave.store(wave, Ordering::Relaxed);
    }

    fn is_celebrating(&self) -> bool {
        self.get_raider_data()
            .is_celebrating
            .load(Ordering::Relaxed)
    }

    fn set_celebrating(&self, celebrating: bool) {
        self.get_raider_data()
            .is_celebrating
            .store(celebrating, Ordering::Relaxed);
        let entity = &self.get_mob_entity().living_entity.entity;
        entity.set_synced_data(tracked_data::pillager::IS_CELEBRATING, celebrating);
    }

    fn has_active_raid(&self) -> bool {
        self.get_raider_data().raid_id.load().is_some()
    }

    fn is_captain(&self) -> bool {
        self.is_patrol_leader()
    }

    fn get_celebrate_sound(&self) -> Sound;

    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {}

    fn write_raider_nbt(&self, nbt: &mut NbtCompound) {
        self.write_patrol_nbt(nbt);
        let data = self.get_raider_data();
        nbt.put_int("Wave", data.wave.load(Ordering::Relaxed));
        nbt.put_bool("CanJoinRaid", data.can_join_raid.load(Ordering::Relaxed));
        if let Some(raid_id) = data.raid_id.load() {
            nbt.put_int("RaidId", raid_id);
        }
    }

    fn read_raider_nbt(&self, nbt: &NbtCompound) {
        self.read_patrol_nbt(nbt);
        let data = self.get_raider_data();
        if let Some(wave) = nbt.get_int("Wave") {
            data.wave.store(wave, Ordering::Relaxed);
        }
        if let Some(can_join) = nbt.get_bool("CanJoinRaid") {
            data.can_join_raid.store(can_join, Ordering::Relaxed);
        }
        // Vanilla `Raider.readAdditionalSaveData`: a raider loaded with a raid id joins that raid's
        // wave again, or forgets the id when the raid is gone.
        if let Some(raid_id) = nbt.get_int("RaidId") {
            let entity = &self.get_mob_entity().living_entity.entity;
            let world = entity.world.load();
            let mut raids = world
                .raids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let raid = raids.get_mut(raid_id);
            data.raid_id.store(raid.is_some().then_some(raid_id));
            if let Some(raid) = raid {
                let wave = data.wave.load(Ordering::Relaxed);
                raid.group_raider_map
                    .entry(wave)
                    .or_default()
                    .insert(entity.entity_uuid);
                if self.is_patrol_leader() {
                    raid.group_to_leader_map.insert(wave, entity.entity_uuid);
                }
            }
        }
    }
}

/// Goal for raiders holding ground during patrols when spotting distant targets, alerting nearby raiders.
pub struct HoldGroundAttackGoal {
    hostile_radius_sqr: f64,
}

impl HoldGroundAttackGoal {
    #[must_use]
    pub fn new(hostile_radius: f32) -> Self {
        Self {
            hostile_radius_sqr: f64::from(hostile_radius * hostile_radius),
        }
    }
}

impl Goal for HoldGroundAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        if raider.has_active_raid() || !raider.is_patrolling() {
            return false;
        }
        let target = mob.get_mob_entity().get_target().clone();
        target.is_some()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let target = mob.get_mob_entity().get_target().clone();
        target.is_some()
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();

        let target = mob.get_mob_entity().get_target().clone();
        if let Some(target) = target {
            let entity = mob.get_entity();
            let world = entity.world.load();
            let bb = entity.bounding_box.load().expand(8.0, 8.0, 8.0);
            let nearby = world.get_entities_at_box(&bb);

            for cand in nearby {
                if cand.get_entity().entity_id != entity.entity_id
                    && let Some(cand_mob) = cand.get_mob()
                    && cand_mob.as_raider().is_some()
                {
                    cand_mob.get_mob_entity().set_target(Some(target.clone()));
                }
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let target = mob.get_mob_entity().get_target().clone();
        if let Some(target) = target {
            let mob_pos = mob.get_entity().pos.load();
            let target_pos = target.get_entity().pos.load();
            let dist_sq = mob_pos.squared_distance_to_vec(&target_pos);

            if dist_sq > self.hostile_radius_sqr {
                mob.get_mob_entity()
                    .look_control
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .look_at_entity_with_range(&target, 30.0, 30.0);
            }
        }
    }
}

/// Vanilla `Raider.ObtainRaidLeaderBannerGoal`: a raider of a raid wave without a living leader
/// walks to a reachable ominous banner and picks it up.
#[derive(Default)]
pub struct ObtainRaidLeaderBannerGoal {
    /// Banner item entity ids and the game time until which they count as unreachable.
    unreachable_banner_cache: HashMap<i32, i64>,
    path_to_banner: Option<Path>,
    pursued_banner: Option<Arc<dyn EntityBase>>,
}

impl ObtainRaidLeaderBannerGoal {
    /// Vanilla `Raider.ALLOWED_ITEMS`.
    fn is_allowed(item: &ItemEntity) -> bool {
        item.get_pickup_delay() == 0
            && item.get_entity().is_alive()
            && is_ominous_banner(
                &item
                    .get_item_stack()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )
    }

    fn cannot_pick_up_banner(raider: &dyn Raider) -> bool {
        if !raider.has_active_raid() || !raider.can_be_leader() {
            return true;
        }
        let head = raider
            .get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::HEAD);
        if is_ominous_banner(&head) {
            return true;
        }
        let wave = raider.get_wave();
        let Some((over, leader)) = with_current_raid(raider, |raid| {
            (raid.is_over(), raid.group_to_leader_map.get(&wave).copied())
        }) else {
            return true;
        };
        over || leader.is_some_and(|uuid| {
            let world = raider.get_mob_entity().living_entity.entity.world.load();
            world
                .get_entity_by_uuid(uuid)
                .is_some_and(|leader| leader.get_entity().is_alive())
        })
    }

    /// Vanilla `Raider.pickUpItem` for the ominous banner.
    fn pick_up_banner(raider: &dyn Raider, item_entity: &Arc<dyn EntityBase>) {
        let Some(item) = item_entity.cast_any().downcast_ref::<ItemEntity>() else {
            return;
        };
        let wave = raider.get_wave();
        let has_leader =
            with_current_raid(raider, |raid| raid.group_to_leader_map.contains_key(&wave))
                .unwrap_or(true);
        let stack = item
            .get_item_stack()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if !raider.has_active_raid() || has_leader || !is_ominous_banner(&stack) {
            return;
        }
        let mob_entity = raider.get_mob_entity();
        mob_entity
            .living_entity
            .pickup(item.get_entity(), u32::from(stack.item_count));
        mob_entity.set_item_slot(&EquipmentSlot::HEAD, stack);
        item.get_entity().remove();
        if let Some(raid_id) = raider.get_raider_data().raid_id.load() {
            let world = mob_entity.living_entity.entity.world.load();
            let mut raids = world
                .raids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(raid) = raids.get_mut(raid_id) {
                raid.group_to_leader_map
                    .insert(wave, mob_entity.living_entity.entity.entity_uuid);
            }
        }
        raider.set_patrol_leader(true);
    }
}

impl Goal for ObtainRaidLeaderBannerGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        if Self::cannot_pick_up_banner(raider) {
            return false;
        }
        let living = &mob.get_mob_entity().living_entity;
        let entity = &living.entity;
        let world = entity.world.load();
        let game_time = world
            .level_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_gametime();
        let follow_range = living.get_attribute_value(&Attributes::FOLLOW_RANGE);
        let area = entity
            .bounding_box
            .load()
            .expand(follow_range, 8.0, follow_range);
        let mut cache = HashMap::new();
        for candidate in world.get_entities_at_box(&area) {
            let Some(item) = candidate.cast_any().downcast_ref::<ItemEntity>() else {
                continue;
            };
            if !Self::is_allowed(item) {
                continue;
            }
            let id = candidate.get_entity().entity_id;
            let unreachable_until = self
                .unreachable_banner_cache
                .get(&id)
                .copied()
                .unwrap_or(i64::MIN);
            if game_time < unreachable_until {
                cache.insert(id, unreachable_until);
                continue;
            }
            let path = mob.get_mob_entity().steered().navigator().create_path(
                living,
                candidate.get_entity().pos.load(),
                1,
            );
            if let Some(path) = path.filter(Path::can_reach) {
                self.path_to_banner = Some(path);
                self.pursued_banner = Some(candidate);
                return true;
            }
            cache.insert(id, game_time + 600);
        }
        self.unreachable_banner_cache = cache;
        false
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        // Vanilla checks the path it handed to the navigator.
        self.pursued_banner
            .as_ref()
            .is_some_and(|banner| !banner.get_entity().is_removed())
            && !mob.get_mob_entity().steered().navigator().is_done()
            && !Self::cannot_pick_up_banner(raider)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let path = self.path_to_banner.take();
        mob.get_mob_entity().steered().navigator().move_to_path(
            path,
            1.15,
            &mob.get_mob_entity().living_entity,
        );
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.path_to_banner = None;
        self.pursued_banner = None;
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let (Some(raider), Some(banner)) = (mob.as_raider(), &self.pursued_banner) else {
            return;
        };
        let distance = banner
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&mob.get_entity().pos.load());
        if distance < 1.414 * 1.414 {
            Self::pick_up_banner(raider, banner);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `AbstractIllager.RaiderOpenDoorGoal`: opens doors only during a raid.
pub struct RaiderOpenDoorGoal(OpenDoorGoal);

impl Default for RaiderOpenDoorGoal {
    fn default() -> Self {
        Self(OpenDoorGoal::new(false))
    }
}

impl Goal for RaiderOpenDoorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.0.can_start(mob) && mob.as_raider().is_some_and(Raider::has_active_raid)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.0.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.0.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.0.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.0.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.0.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.0.controls()
    }
}

/// Goal for raiders celebrating victory.
pub struct RaiderCelebrationGoal;

impl Goal for RaiderCelebrationGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        let target = mob.get_mob_entity().get_target().clone();
        target.is_none() && raider.is_celebrating()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        let target = mob.get_mob_entity().get_target().clone();
        target.is_none() && raider.is_celebrating()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(raider) = mob.as_raider() {
            raider.set_celebrating(true);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(raider) = mob.as_raider() {
            raider.set_celebrating(false);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(raider) = mob.as_raider() else {
            return;
        };
        let entity = mob.get_entity();
        let pos = entity.pos.load();
        let world = entity.world.load();

        let r: f32 = rand::random();
        if r < 0.02 && !entity.silent.load(Ordering::Relaxed) {
            world.play_sound(
                raider.get_celebrate_sound(),
                pumpkin_data::sound::SoundCategory::Hostile,
                &pos,
            );
        }
    }
}

/// Goal for moving through village homes during active raids.
pub struct RaiderMoveThroughVillageGoal {
    speed_modifier: f64,
}

impl RaiderMoveThroughVillageGoal {
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }
}

impl Goal for RaiderMoveThroughVillageGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        if !raider.has_active_raid() {
            return false;
        }
        let target = mob.get_mob_entity().get_target().clone();
        target.is_none()
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let pos = entity.pos.load();
        let steered = mob.get_mob_entity().steered();
        let mut nav = steered.navigator();

        if nav.is_idle() {
            let dx: f64 = (rand::random::<f64>() - 0.5) * 32.0;
            let dz: f64 = (rand::random::<f64>() - 0.5) * 32.0;
            let dest = Vector3::new(pos.x + dx, pos.y, pos.z + dz);
            nav.set_progress(NavigatorGoal {
                current_progress: pos,
                destination: dest,
                speed: self.speed_modifier,
                reach_range: 1,
            });
        }
    }
}

pub use crate::entity::ai::goal::pathfind_to_raid::PathfindToRaidGoal;

#[cfg(test)]
mod tests {
    use pumpkin_util::difficulty::Difficulty;
    use pumpkin_util::math::position::BlockPos;
    use uuid::Uuid;

    use super::*;
    use crate::entity::r#type::from_type;
    use crate::world::raid::Raid;

    /// Vanilla `Raider.readAdditionalSaveData`: a raider loaded with `RaidId` joins that raid's
    /// wave again, and one whose raid is gone forgets the id.
    #[tokio::test]
    async fn a_loaded_raider_rejoins_its_raid_or_forgets_a_missing_one() {
        let world = crate::world::test_world();
        world
            .raids
            .lock()
            .unwrap()
            .raid_map
            .insert(2, Raid::new(2, BlockPos::new(0, 64, 0), Difficulty::Normal));
        for (raid_id, joins) in [(2, true), (9, false)] {
            let uuid = Uuid::new_v4();
            let entity = from_type(
                &EntityType::PILLAGER,
                Vector3::new(0.5, 64.0, 0.5),
                &world,
                uuid,
            );
            let mut nbt = NbtCompound::new();
            nbt.put_int("Wave", 3);
            nbt.put_int("RaidId", raid_id);
            entity.read_nbt_non_mut(&nbt);

            let raider = entity.get_mob().unwrap().as_raider().unwrap();
            assert_eq!(
                raider.get_raider_data().raid_id.load(),
                joins.then_some(raid_id)
            );
            let raids = world.raids.lock().unwrap();
            let wave = raids.get(2).unwrap().group_raider_map.get(&3);
            assert_eq!(wave.is_some_and(|wave| wave.contains(&uuid)), joins);
        }
    }
}
