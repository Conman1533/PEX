use super::EnderDragonPhase;
use crate::entity::area_effect_cloud::AreaEffectCloudEntity;
use crate::entity::boss::ender_dragon::{EnderDragonEntity, HEAD_SIZE};
use crate::entity::{Entity, EntityBase};
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Arc, Mutex, PoisonError};

const FLAME_DURATION: i32 = 200;
const SITTING_FLAME_ATTACKS_COUNT: i32 = 4;
const WARMUP_TIME: i32 = 10;
const FLAME_RADIUS: f32 = 5.0;

#[derive(Default)]
struct State {
    flame_ticks: i32,
    flame_count: i32,
    flame: Option<Arc<AreaEffectCloudEntity>>,
}

/// Vanilla `DragonSittingFlamingPhase`: breathe a cloud of dragon's breath in front of the
/// portal, up to four times per landing.
#[derive(Default)]
pub struct SittingFlamingPhase {
    state: Mutex<State>,
}

impl SittingFlamingPhase {
    pub fn reset_flame_count(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flame_count = 0;
    }

    fn spawn_flame(dragon: &EnderDragonEntity) -> Arc<AreaEffectCloudEntity> {
        let entity = dragon.get_entity();
        let world = entity.world.load_full();
        let pos = entity.pos.load();
        let head = dragon.parts[0].entity.pos.load();
        let look = Vector3::new(head.x - pos.x, 0.0, head.z - pos.z).normalize();
        let x = head.x + look.x * f64::from(FLAME_RADIUS) / 2.0;
        let z = head.z + look.z * f64::from(FLAME_RADIUS) / 2.0;
        let initial_y = head.y + HEAD_SIZE * 0.5;

        // Down from the head to the first block that isn't air.
        let mut y = initial_y;
        let mut block_pos = Vector3::new(x, y, z).to_block_pos();
        while world.get_block_state(&block_pos).is_air() {
            y -= 1.0;
            if y < 0.0 {
                y = initial_y;
                break;
            }
            block_pos = BlockPos::floored(x, y, z);
        }
        let y = y.floor() + 1.0;

        let flame = AreaEffectCloudEntity::dragon_breath(
            Entity::new(
                world.clone(),
                Vector3::new(x, y, z),
                &EntityType::AREA_EFFECT_CLOUD,
            ),
            (&StatusEffect::INSTANT_DAMAGE, 0, 0, false, true, true),
            FLAME_DURATION,
            FLAME_RADIUS,
            0.0,
        );
        world.spawn_entity(flame.clone());
        flame
    }
}

impl super::Phase for SittingFlamingPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.flame_ticks += 1;
        if state.flame_ticks >= FLAME_DURATION {
            let next = if state.flame_count >= SITTING_FLAME_ATTACKS_COUNT {
                EnderDragonPhase::Takeoff
            } else {
                EnderDragonPhase::SittingScanning
            };
            drop(state);
            dragon.set_phase(next);
        } else if state.flame_ticks == WARMUP_TIME {
            state.flame = Some(Self::spawn_flame(dragon));
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.flame_ticks = 0;
        state.flame_count += 1;
    }

    fn end(&self, _dragon: &EnderDragonEntity) {
        let flame = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flame
            .take();
        if let Some(flame) = flame {
            flame.get_entity().remove();
        }
    }
}
