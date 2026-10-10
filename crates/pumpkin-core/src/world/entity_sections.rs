use std::sync::{Arc, PoisonError, RwLock};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::get_section_cord;
use pumpkin_util::math::position::chunk_section_from_pos;
use pumpkin_util::math::vector3::Vector3;
use rustc_hash::FxBuildHasher;

use crate::entity::{Entity, EntityBase};

/// An entity is filed under the section of its feet, so a box query also looks this far into
/// the neighbouring sections.
pub const CHONKY_ENTITY_SEARCH_GRACE: f64 = 2.0;
pub const MAX_NON_CHONKY_ENTITY_SIZE: f64 = 4.0;

/// Above this many sections a query walks the filed sections instead of looking each one up.
const MAX_SECTION_LOOKUPS: i64 = 256;

/// Non-player entities filed by chunk section, like vanilla's `EntitySectionStorage`.
#[derive(Default)]
pub struct EntitySectionStorage {
    sections: DashMap<Vector3<i32>, Vec<Arc<dyn EntityBase>>, FxBuildHasher>,
    /// Types too big for the search grace. Vanilla finds the ender dragon through its parts,
    /// which Pumpkin doesn't add to the world, so these are checked by every query instead.
    oversized: RwLock<Vec<Arc<dyn EntityBase>>>,
}

impl EntitySectionStorage {
    fn is_oversized(entity_type: &EntityType) -> bool {
        let [width, height] = entity_type.dimension;
        f64::from(width) > MAX_NON_CHONKY_ENTITY_SIZE
            || f64::from(height) > MAX_NON_CHONKY_ENTITY_SIZE
    }

    pub fn add(&self, entity: &Arc<dyn EntityBase>) {
        let base = entity.get_entity();
        if Self::is_oversized(base.entity_type) {
            self.oversized
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .push(entity.clone());
            return;
        }
        let mut filed = base
            .entity_section
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let section = chunk_section_from_pos(&base.block_pos.load());
        if let Some(previous) = filed.replace(section) {
            self.take(previous, base.entity_id);
        }
        self.sections
            .entry(section)
            .or_default()
            .push(entity.clone());
    }

    pub fn remove(&self, entity: &Entity) {
        if Self::is_oversized(entity.entity_type) {
            self.oversized
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|e| e.get_entity().entity_id != entity.entity_id);
            return;
        }
        let mut filed = entity
            .entity_section
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(section) = filed.take() {
            self.take(section, entity.entity_id);
        }
    }

    /// Vanilla `PersistentEntitySectionManager.Callback.onMove`, called once the block position
    /// has changed.
    pub fn on_move(&self, entity: &Entity) {
        let mut filed = entity
            .entity_section
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Not filed: a player, an oversized entity, or one that isn't in a world yet.
        let Some(previous) = *filed else {
            return;
        };
        let section = chunk_section_from_pos(&entity.block_pos.load());
        if previous == section {
            return;
        }
        if let Some(moved) = self.take(previous, entity.entity_id) {
            self.sections.entry(section).or_default().push(moved);
            *filed = Some(section);
        }
    }

    fn take(&self, section: Vector3<i32>, entity_id: i32) -> Option<Arc<dyn EntityBase>> {
        let Entry::Occupied(mut entry) = self.sections.entry(section) else {
            return None;
        };
        let entities = entry.get_mut();
        let index = entities
            .iter()
            .position(|e| e.get_entity().entity_id == entity_id)?;
        let entity = entities.swap_remove(index);
        if entities.is_empty() {
            entry.remove();
        }
        Some(entity)
    }

    /// Vanilla `EntitySectionStorage.getEntities(AABB, ...)`.
    pub fn get_entities(&self, aabb: &BoundingBox) -> Vec<Arc<dyn EntityBase>> {
        let min = Vector3::new(
            section_coord(aabb.min.x - CHONKY_ENTITY_SEARCH_GRACE),
            section_coord(aabb.min.y - MAX_NON_CHONKY_ENTITY_SIZE),
            section_coord(aabb.min.z - CHONKY_ENTITY_SEARCH_GRACE),
        );
        let max = Vector3::new(
            section_coord(aabb.max.x + CHONKY_ENTITY_SEARCH_GRACE),
            section_coord(aabb.max.y),
            section_coord(aabb.max.z + CHONKY_ENTITY_SEARCH_GRACE),
        );
        self.collect(min, max, |entity| {
            entity.get_entity().bounding_box.load().intersects(aabb)
        })
    }

    /// Entities whose position is within `radius` of `pos`.
    pub fn get_entities_near(&self, pos: Vector3<f64>, radius: f64) -> Vec<Arc<dyn EntityBase>> {
        let radius_squared = radius.powi(2);
        let min = Vector3::new(
            section_coord(pos.x - radius),
            section_coord(pos.y - radius),
            section_coord(pos.z - radius),
        );
        let max = Vector3::new(
            section_coord(pos.x + radius),
            section_coord(pos.y + radius),
            section_coord(pos.z + radius),
        );
        self.collect(min, max, |entity| {
            entity.get_entity().pos.load().squared_distance_to_vec(&pos) <= radius_squared
        })
    }

    fn collect(
        &self,
        min: Vector3<i32>,
        max: Vector3<i32>,
        predicate: impl Fn(&Arc<dyn EntityBase>) -> bool,
    ) -> Vec<Arc<dyn EntityBase>> {
        let mut found = Vec::new();
        let mut add_matching = |entities: &[Arc<dyn EntityBase>]| {
            found.extend(entities.iter().filter(|e| predicate(e)).cloned());
        };

        let lookups = (i64::from(max.x) - i64::from(min.x) + 1)
            * (i64::from(max.y) - i64::from(min.y) + 1)
            * (i64::from(max.z) - i64::from(min.z) + 1);
        if lookups > MAX_SECTION_LOOKUPS {
            for entry in &self.sections {
                let section = entry.key();
                if (min.x..=max.x).contains(&section.x)
                    && (min.y..=max.y).contains(&section.y)
                    && (min.z..=max.z).contains(&section.z)
                {
                    add_matching(entry.value());
                }
            }
        } else {
            for x in min.x..=max.x {
                for z in min.z..=max.z {
                    for y in min.y..=max.y {
                        if let Some(entities) = self.sections.get(&Vector3::new(x, y, z)) {
                            add_matching(&entities);
                        }
                    }
                }
            }
        }

        add_matching(
            &self
                .oversized
                .read()
                .unwrap_or_else(PoisonError::into_inner),
        );
        found
    }
}

/// Vanilla `SectionPos.posToSectionCoord`.
const fn section_coord(coord: f64) -> i32 {
    get_section_cord(coord.floor() as i32)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::boundingbox::BoundingBox;
    use pumpkin_util::math::vector3::Vector3;

    use crate::entity::{Entity, EntityBase};
    use crate::world::{World, test_world};

    fn spawn(world: &Arc<World>, pos: Vector3<f64>, entity_type: &'static EntityType) -> i32 {
        let entity: Arc<dyn EntityBase> = Arc::new(Entity::new(world.clone(), pos, entity_type));
        world.entity_sections.add(&entity);
        entity.get_entity().entity_id
    }

    fn found(world: &World, min: Vector3<f64>, max: Vector3<f64>) -> Vec<i32> {
        world
            .entity_sections
            .get_entities(&BoundingBox::new(min, max))
            .iter()
            .map(|entity| entity.get_entity().entity_id)
            .collect()
    }

    #[tokio::test]
    async fn an_entity_is_found_in_the_section_it_moved_to() {
        let world = test_world();
        let cow = Arc::new(Entity::new(
            world.clone(),
            Vector3::new(8.5, 64.0, 8.5),
            &EntityType::COW,
        ));
        world
            .entity_sections
            .add(&(cow.clone() as Arc<dyn EntityBase>));

        cow.set_pos(Vector3::new(40.5, 64.0, 8.5));

        let near = |x: f64| {
            found(
                &world,
                Vector3::new(x - 1.0, 64.0, 7.5),
                Vector3::new(x + 1.0, 65.0, 9.5),
            )
        };
        assert!(near(8.5).is_empty());
        assert_eq!(near(40.5), [cow.entity_id]);
    }

    #[tokio::test]
    async fn a_box_finds_an_entity_whose_feet_are_in_the_section_below() {
        let world = test_world();
        // Feet in section y 3, head in section y 4.
        let cow = spawn(&world, Vector3::new(8.5, 63.5, 8.5), &EntityType::COW);

        assert_eq!(
            found(
                &world,
                Vector3::new(8.0, 64.5, 8.0),
                Vector3::new(9.0, 65.0, 9.0)
            ),
            [cow]
        );
    }

    #[tokio::test]
    async fn an_ender_dragon_is_found_beyond_the_search_grace() {
        let world = test_world();
        // 16 wide, so its box reaches x 8 while its feet are in section x 1.
        let dragon = spawn(
            &world,
            Vector3::new(16.0, 64.0, 8.0),
            &EntityType::ENDER_DRAGON,
        );

        assert_eq!(
            found(
                &world,
                Vector3::new(8.2, 64.0, 8.0),
                Vector3::new(8.4, 65.0, 8.5)
            ),
            [dragon]
        );
    }
}
