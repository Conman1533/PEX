use std::sync::{Arc, atomic::Ordering};

use pumpkin_util::math::position::BlockPos;

use crate::{block::entities::BlockEntity, world::World};

pub use pumpkin_inventory::ViewerCountTracker;

pub trait ViewerCountTrackerExt {
    fn update_viewer_count<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static;
}

impl ViewerCountTrackerExt for ViewerCountTracker {
    fn update_viewer_count<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        let current = self.current.load(Ordering::Relaxed);
        let old = self.old.swap(current, Ordering::Relaxed);
        if old != current {
            let center = position.to_centered_f64();
            let source = world
                .get_closest_player(center, 8.0)
                .map(|p| p as Arc<dyn crate::entity::EntityBase>);
            match (old, current) {
                (n, 0) if n > 0 => {
                    entity.on_container_close(world, position);
                    world.game_event(
                        pumpkin_data::game_event::GameEvent::ContainerClose,
                        center,
                        crate::world::vibration::GameEventContext::of(source, None),
                    );
                }
                (0, n) if n > 0 => {
                    entity.on_container_open(world, position);
                    world.game_event(
                        pumpkin_data::game_event::GameEvent::ContainerOpen,
                        center,
                        crate::world::vibration::GameEventContext::of(source, None),
                    );
                }
                _ => {} // Ignore
            }

            entity.on_viewer_count_update(world, position, old, current);
        }
    }
}

pub trait ViewerCountListener: Send + Sync {
    fn on_container_open(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_container_close(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_viewer_count_update(
        &self,
        _world: &Arc<World>,
        _position: &BlockPos,
        _old: u16,
        _new: u16,
    ) {
    }
}
