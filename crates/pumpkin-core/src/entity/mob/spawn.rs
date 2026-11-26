//! Vanilla `Mob.finalizeSpawn`: the step run on freshly created mobs before they enter the world.

use std::sync::Arc;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::effect::StatusEffect;
use rand::RngExt;

use crate::entity::EntityBase;
use crate::entity::ageable::AgeableMob;
use crate::entity::attributes::{Modifier, ModifierOperation};
use crate::entity::mob::MobEntity;
use crate::world::World;

const RANDOM_SPAWN_BONUS_ID: &str = "minecraft:random_spawn_bonus";

/// State shared by every mob of one spawn group (vanilla `SpawnGroupData`).
pub enum SpawnGroupData {
    School(std::sync::Weak<dyn EntityBase>),
    /// The effect the first spider of a group rolled on hard difficulty.
    SpiderEffects(Option<&'static StatusEffect>),
    /// Vanilla `AgeableMob.AgeableMobGroupData`.
    Ageable(AgeableGroupData),
    /// Vanilla `Fox.FoxGroupData`: the pack's variant, on an `AgeableMobGroupData` that never
    /// rolls a baby.
    FoxGroupData {
        variant: crate::entity::passive::fox::FoxVariant,
        ageable: AgeableGroupData,
    },
    /// Vanilla `Husk.HuskGroupData`: whether a husk of the group tried the camel husk jockey.
    Husk {
        tried_camel_husk: bool,
    },
}

/// Vanilla `AgeableMobGroupData`'s default baby chance.
pub const DEFAULT_BABY_SPAWN_CHANCE: f32 = 0.05;

/// Vanilla `AgeableMob.AgeableMobGroupData`: how many ageable mobs of the group have spawned, and
/// the chance that one after the first spawns as a baby.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgeableGroupData {
    pub group_size: u32,
    pub should_spawn_baby: bool,
    pub baby_spawn_chance: f32,
}

impl AgeableGroupData {
    #[must_use]
    pub const fn new(should_spawn_baby: bool, baby_spawn_chance: f32) -> Self {
        Self {
            group_size: 0,
            should_spawn_baby,
            baby_spawn_chance,
        }
    }

    /// The check in vanilla `AgeableMob.finalizeSpawn`: a mob after the first of the group is a
    /// baby when `roll` (drawn only then) is at most the chance. The group grows by one either
    /// way.
    pub fn roll_baby_and_grow(&mut self, roll: impl FnOnce() -> f32) -> bool {
        let baby =
            self.should_spawn_baby && self.group_size > 0 && roll() <= self.baby_spawn_chance;
        self.group_size += 1;
        baby
    }
}

impl SpawnGroupData {
    /// The `AgeableMobGroupData` part of this group, for the kinds that extend it in vanilla.
    pub const fn ageable_mut(&mut self) -> Option<&mut AgeableGroupData> {
        match self {
            Self::Ageable(ageable) | Self::FoxGroupData { ageable, .. } => Some(ageable),
            _ => None,
        }
    }
}

/// `group_data`, or a new `AgeableMobGroupData` when there is none (vanilla
/// `if (groupData == null) groupData = new AgeableMobGroupData(...)`).
#[must_use]
pub fn ageable_group_or(
    group_data: Option<SpawnGroupData>,
    should_spawn_baby: bool,
    baby_spawn_chance: f32,
) -> Option<SpawnGroupData> {
    group_data.or(Some(SpawnGroupData::Ageable(AgeableGroupData::new(
        should_spawn_baby,
        baby_spawn_chance,
    ))))
}

/// Vanilla `AgeableMob.finalizeSpawn`, with the group data the mob's own `finalizeSpawn` hands to
/// it ([`AgeableMob::ageable_spawn_group`]).
pub fn ageable_finalize_spawn(
    mob: &dyn AgeableMob,
    group_data: Option<SpawnGroupData>,
) -> Option<SpawnGroupData> {
    let mut group_data = mob.ageable_spawn_group(group_data);
    if let Some(group) = group_data.as_mut().and_then(SpawnGroupData::ageable_mut)
        && group.roll_baby_and_grow(|| mob.get_random().random::<f32>())
    {
        mob.set_age(mob.get_baby_start_age());
    }
    group_data
}

/// A vehicle a mob spawns riding (vanilla `startRiding` in `finalizeSpawn`), and the riders
/// that board it after the mob.
pub struct PendingVehicle {
    pub vehicle: Arc<dyn EntityBase>,
    pub other_riders: Vec<Arc<dyn EntityBase>>,
}

impl MobEntity {
    /// Vanilla `Mob.finalizeSpawn`: random follow range bonus and left-handedness.
    pub fn finalize_spawn_base(&self) {
        let mut rng = rand::rng();
        {
            let mut attributes = self
                .living_entity
                .attributes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(follow_range) = attributes.get_mut(&Attributes::FOLLOW_RANGE.id)
                && !follow_range
                    .modifiers
                    .iter()
                    .any(|modifier| modifier.id == RANDOM_SPAWN_BONUS_ID)
            {
                follow_range.add_or_replace_modifier(Modifier {
                    id: RANDOM_SPAWN_BONUS_ID.to_string(),
                    // Triangle distribution around 0.
                    amount: 0.114_85 * f64::from(rng.random::<f32>() - rng.random::<f32>()),
                    operation: ModifierOperation::MultiplyBase,
                });
            }
        }
        self.set_left_handed(rng.random::<f32>() < 0.05);
    }

    /// Queues a mob to be added and mounted onto this one when it enters the world.
    pub fn add_pending_rider(&self, rider: Arc<dyn EntityBase>) {
        self.pending_riders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(rider);
    }

    /// The mob spawns riding `vehicle`, followed by `other_riders`; the world adds them all.
    pub fn set_pending_vehicle(
        &self,
        vehicle: Arc<dyn EntityBase>,
        other_riders: Vec<Arc<dyn EntityBase>>,
    ) {
        *self
            .pending_vehicle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(PendingVehicle {
            vehicle,
            other_riders,
        });
    }

    pub(crate) fn take_pending_vehicle(&self) -> Option<PendingVehicle> {
        self.pending_vehicle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    pub(crate) fn take_pending_riders(&self) -> Vec<Arc<dyn EntityBase>> {
        std::mem::take(
            &mut *self
                .pending_riders
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

/// Finalizes `entity` if it is a mob, and returns the group data for the next mob of the group.
pub fn finalize_spawn(
    entity: &Arc<dyn EntityBase>,
    world: &Arc<World>,
    group_data: Option<SpawnGroupData>,
) -> Option<SpawnGroupData> {
    match entity.get_mob() {
        Some(mob) => {
            let group_data = mob.finalize_spawn(world, group_data);
            match mob.as_ageable() {
                Some(ageable) => ageable_finalize_spawn(ageable, group_data),
                None => group_data,
            }
        }
        None => group_data,
    }
}

impl MobEntity {
    /// Baby state kept only as a negative age and the shared baby flag (zombified piglins, which
    /// are not `AgeableMob`s here).
    pub fn set_baby_by_age(&self) {
        let entity = &self.living_entity.entity;
        entity
            .age
            .store(-24000, std::sync::atomic::Ordering::Relaxed);
        entity.set_synced_data(pumpkin_data::tracked_data::ageable_mob::DATA_BABY_ID, true);
    }

    /// Baby state kept as a mob's own flag and synced key (vanilla zombies and piglins), so it
    /// never ages up.
    pub fn set_baby_flag(
        &self,
        flag: &std::sync::atomic::AtomicBool,
        tracked: pumpkin_data::tracked_data::TrackedData,
        baby: bool,
    ) {
        flag.store(baby, std::sync::atomic::Ordering::Relaxed);
        let entity = &self.living_entity.entity;
        entity.set_synced_data(tracked, baby);
        // Pumpkin's negative age is what reports the Bedrock baby flag.
        entity.age.store(
            if baby { -24000 } else { 0 },
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering::Relaxed;

    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::vector3::Vector3;

    use super::{AgeableGroupData, DEFAULT_BABY_SPAWN_CHANCE, finalize_spawn};
    use crate::entity::r#type::from_type;
    use crate::world::{World, test_world};

    #[test]
    fn the_first_of_a_group_is_never_a_baby() {
        let mut group = AgeableGroupData::new(true, 1.0);
        assert!(!group.roll_baby_and_grow(|| 0.0));
        assert_eq!(group.group_size, 1);
        assert!(group.roll_baby_and_grow(|| 0.0));
        assert_eq!(group.group_size, 2);
    }

    #[test]
    fn a_baby_when_the_roll_is_at_most_the_chance() {
        let mut group = AgeableGroupData::new(true, DEFAULT_BABY_SPAWN_CHANCE);
        group.group_size = 1;
        assert!(group.roll_baby_and_grow(|| 0.05));
        assert!(!group.roll_baby_and_grow(|| 0.050_001));
        assert_eq!(group.group_size, 3);
    }

    #[test]
    fn a_group_without_babies_never_rolls() {
        let mut group = AgeableGroupData::new(false, 1.0);
        let mut rolls = 0;
        for _ in 0..3 {
            assert!(!group.roll_baby_and_grow(|| {
                rolls += 1;
                0.0
            }));
        }
        assert_eq!(rolls, 0, "a group without babies rolled");
        assert_eq!(group.group_size, 3);
    }

    /// Finalizes `count` mobs of one group the way the natural spawner does. Returns whether the
    /// first was a baby, and how many babies there were in all.
    fn spawn_group(
        world: &Arc<World>,
        entity_type: &'static EntityType,
        count: usize,
    ) -> (bool, usize) {
        let mut group_data = None;
        let mut first_is_baby = false;
        let mut babies = 0;
        for i in 0..count {
            let entity = from_type(
                entity_type,
                Vector3::new(0.5, 64.0, 0.5),
                world,
                uuid::Uuid::new_v4(),
            );
            group_data = finalize_spawn(&entity, world, group_data);
            let baby = entity.get_entity().age.load(Relaxed) < 0;
            if i == 0 {
                first_is_baby = baby;
            }
            babies += usize::from(baby);
        }
        (first_is_baby, babies)
    }

    /// Vanilla `AgeableMob.finalizeSpawn` and the groups each mob hands it. The bounds are about
    /// four standard deviations either side.
    #[tokio::test]
    async fn natural_groups_spawn_babies_at_the_vanilla_rates() {
        let world = test_world();

        // The default `AgeableMobGroupData`: 5%, so about 200 of the 4000 after the first.
        let (first, calves) = spawn_group(&world, &EntityType::COW, 4001);
        assert!(!first, "the first cow of a group was a calf");
        assert!((140..=260).contains(&calves), "{calves} calves in 4000");

        // `summon` and spawn eggs pass no group, so each cow is the first of its own group and
        // never a calf.
        let summoned_calves: usize = (0..1000)
            .map(|_| spawn_group(&world, &EntityType::COW, 1).1)
            .sum();
        assert_eq!(summoned_calves, 0, "summoned cows were calves");

        // `Horse.HorseGroupData`: 5%, so about 200 of the 4000 after the first.
        let (first, foals) = spawn_group(&world, &EntityType::HORSE, 4001);
        assert!(!first, "the first horse of a group was a foal");
        assert!((140..=260).contains(&foals), "{foals} horse foals in 4000");

        // `AbstractHorse.finalizeSpawn`: 20%, so about 400 of the 2000 after the first.
        let (first, foals) = spawn_group(&world, &EntityType::DONKEY, 2001);
        assert!(!first, "the first donkey of a group was a foal");
        assert!((320..=480).contains(&foals), "{foals} donkey foals in 2000");

        // `Rabbit.RabbitGroupData`: every rabbit after the first.
        let (first, babies) = spawn_group(&world, &EntityType::RABBIT, 50);
        assert!(!first, "the first rabbit of a group was a baby");
        assert_eq!(babies, 49);

        // `AbstractVillager` and `Wolf.WolfPackData`: never.
        assert_eq!(spawn_group(&world, &EntityType::VILLAGER, 200).1, 0);
        assert_eq!(spawn_group(&world, &EntityType::WOLF, 200).1, 0);
    }
}
