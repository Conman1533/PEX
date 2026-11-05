use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::entity::mob::zombie::ZombieEntityBase;
use crate::entity::{
    Entity,
    mob::{
        Mob, MobEntity,
        spawn::{SpawnGroupData, finalize_spawn},
    },
    r#type::from_type,
};
use crate::world::World;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use rand::RngExt;
use uuid::Uuid;

pub struct HuskEntity {
    entity: Arc<ZombieEntityBase>,
}

impl HuskEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = ZombieEntityBase::new(entity);
        let zombie = Self { entity };
        Arc::new(zombie)
    }

    #[must_use]
    pub fn with_can_break_doors(entity: Entity, can_break_doors: bool) -> Arc<Self> {
        let entity = ZombieEntityBase::with_can_break_doors(entity, can_break_doors);
        let zombie = Self { entity };
        Arc::new(zombie)
    }
}

impl HuskEntity {
    /// Vanilla `Husk.finalizeSpawn`: the first husk of a natural group that has room for one
    /// has a 10% chance to ride a camel husk with an iron spear, a parched riding behind.
    fn try_camel_husk_jockey(
        &self,
        world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let mob = &self.entity.mob_entity;
        if !mob.natural_spawn.load(Ordering::Relaxed) {
            return group_data;
        }
        if matches!(
            group_data,
            Some(SpawnGroupData::Husk {
                tried_camel_husk: true
            })
        ) {
            return group_data;
        }
        let entity = &mob.living_entity.entity;
        let pos = entity.pos.load();
        let block_pos = entity.block_pos.load();
        let room = world.is_space_empty(EntityType::CAMEL_HUSK.get_spawn_bounding_box(
            f64::from(block_pos.0.x) + 0.5,
            f64::from(block_pos.0.y),
            f64::from(block_pos.0.z) + 0.5,
        ));
        if !room {
            return Some(SpawnGroupData::Husk {
                tried_camel_husk: false,
            });
        }
        if rand::rng().random::<f32>() < 0.1 {
            mob.living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .put(
                    &EquipmentSlot::MAIN_HAND,
                    ItemStack::new(1, &Item::IRON_SPEAR),
                );
            let camel = from_type(&EntityType::CAMEL_HUSK, pos, world, Uuid::new_v4());
            finalize_spawn(&camel, world, None);
            let parched = from_type(&EntityType::PARCHED, pos, world, Uuid::new_v4());
            parched.get_entity().set_rotation(entity.yaw.load(), 0.0);
            finalize_spawn(&parched, world, None);
            mob.set_pending_vehicle(camel, vec![parched]);
        }
        Some(SpawnGroupData::Husk {
            tried_camel_husk: true,
        })
    }
}

impl Mob for HuskEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.entity.mob_entity.finalize_spawn_base();
        self.try_camel_husk_jockey(world, group_data)
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn spawn_as_baby(&self) -> bool {
        self.entity.spawn_as_baby()
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.entity.mob_write_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.entity.mob_read_nbt(nbt);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use pumpkin_data::data_component_impl::EquipmentSlot;
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item::Item;
    use pumpkin_util::math::vector3::Vector3;

    use crate::entity::mob::spawn::{SpawnGroupData, finalize_spawn};
    use crate::entity::r#type::from_type;
    use crate::world::test_world;

    #[tokio::test]
    async fn a_natural_husk_group_sometimes_rides_a_camel_husk() {
        let world = test_world();
        let mut jockeys = 0;
        for _ in 0..400 {
            let husk = from_type(
                &EntityType::HUSK,
                Vector3::new(0.5, 64.0, 0.5),
                &world,
                uuid::Uuid::new_v4(),
            );
            let mob = husk.get_mob().expect("a mob");
            mob.get_mob_entity()
                .natural_spawn
                .store(true, Ordering::Relaxed);
            let group = finalize_spawn(&husk, &world, None);
            assert!(matches!(
                group,
                Some(SpawnGroupData::Husk {
                    tried_camel_husk: true
                })
            ));
            if let Some(pending) = mob.get_mob_entity().take_pending_vehicle() {
                jockeys += 1;
                assert_eq!(
                    pending.vehicle.get_entity().entity_type,
                    &EntityType::CAMEL_HUSK
                );
                assert_eq!(pending.other_riders.len(), 1);
                assert_eq!(
                    pending.other_riders[0].get_entity().entity_type,
                    &EntityType::PARCHED
                );
                let equipment = mob
                    .get_mob_entity()
                    .living_entity
                    .entity_equipment
                    .lock()
                    .unwrap();
                assert_eq!(
                    equipment.get(&EquipmentSlot::MAIN_HAND).item,
                    &Item::IRON_SPEAR
                );
            }

            // The rest of the group never tries.
            let next = from_type(
                &EntityType::HUSK,
                Vector3::new(0.5, 64.0, 0.5),
                &world,
                uuid::Uuid::new_v4(),
            );
            let next_mob = next.get_mob().expect("a mob");
            next_mob
                .get_mob_entity()
                .natural_spawn
                .store(true, Ordering::Relaxed);
            finalize_spawn(&next, &world, group);
            assert!(next_mob.get_mob_entity().take_pending_vehicle().is_none());
        }
        // 10%: about 40 of 400.
        assert!((10..100).contains(&jockeys), "{jockeys} jockeys");

        // A spawn egg husk never rides.
        let egg = from_type(
            &EntityType::HUSK,
            Vector3::new(0.5, 64.0, 0.5),
            &world,
            uuid::Uuid::new_v4(),
        );
        assert!(finalize_spawn(&egg, &world, None).is_none());
    }
}
