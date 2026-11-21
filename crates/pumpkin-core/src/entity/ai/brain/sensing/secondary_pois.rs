//! Vanilla `SecondaryPoiSensor`.

use pumpkin_data::Block;
use pumpkin_data::villager::VillagerProfession;
use pumpkin_util::math::position::BlockPos;

use super::super::BrainTick;
use super::super::memory::global_pos::GlobalPos;
use super::super::memory::{MemoryModuleId, types};
use super::Sensor;

const REQUIRES: [MemoryModuleId; 1] = [types::SECONDARY_JOB_SITE.id()];

/// Vanilla `VillagerProfession.secondaryPoi`: only the farmer has one, farmland.
#[must_use]
pub fn is_secondary_poi(profession: VillagerProfession, block: &Block) -> bool {
    profession == VillagerProfession::Farmer && block == &Block::FARMLAND
}

pub struct SecondaryPoiSensor;

impl Sensor for SecondaryPoiSensor {
    fn requires(&self) -> &'static [MemoryModuleId] {
        &REQUIRES
    }

    fn do_tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(profession) = tick.mob.villager_profession() else {
            return;
        };
        let center = tick.mob.get_entity().block_pos.load();
        let mut job_sites = Vec::new();
        if profession == VillagerProfession::Farmer {
            for x in -4..=4 {
                for y in -2..=2 {
                    for z in -4..=4 {
                        let test = BlockPos::new(center.0.x + x, center.0.y + y, center.0.z + z);
                        if is_secondary_poi(profession, tick.world.get_block(&test))
                            && let Some(pos) = GlobalPos::in_world(tick.world, test)
                        {
                            job_sites.push(pos);
                        }
                    }
                }
            }
        }
        if job_sites.is_empty() {
            tick.state.erase(types::SECONDARY_JOB_SITE.id());
        } else {
            tick.state.set(types::SECONDARY_JOB_SITE, job_sites);
        }
    }
}
