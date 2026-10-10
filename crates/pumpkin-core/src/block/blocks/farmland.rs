use std::sync::Arc;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnLandedUponArgs, OnPlaceArgs,
    OnScheduledTickArgs, PathComputationType, RandomTickArgs, push_entities_up,
};
use crate::entity::EntityBase;
use crate::plugin::api::events::block::moisture_change::MoistureChangeEvent;
use crate::plugin::api::events::entity::entity_change_block::EntityChangeBlockEvent;
use crate::world::World;
use pumpkin_data::block_properties::FarmlandLikeProperties;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;
use pumpkin_world::world::BlockFlags;

type FarmlandProperties = FarmlandLikeProperties;

const MAX_MOISTURE: u8 = 7;

/// Vanilla `FarmlandBlock.fallOn`: entities smaller than this volume never trample.
const TRAMPLE_MIN_VOLUME: f32 = 0.512;

#[pumpkin_block("minecraft:farmland")]
pub struct FarmlandBlock;

impl BlockBehaviour for FarmlandBlock {
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if !can_place_at(args.world.as_ref(), args.position) {
            turn_to_base_block(args.world, args.position);
        }
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            return Block::DIRT.default_state.id;
        }
        args.block.default_state.id
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Up && !can_place_at(args.world, args.position) {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_at(args.block_accessor, args.position)
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        let state_id = args.world.get_block_state_id(args.position);
        let mut props = FarmlandProperties::from_state_id(state_id);
        let moisture = props.moisture;

        let new_moisture = if !is_near_water(args.world, args.position)
            && !args.world.is_raining_at(&args.position.up())
        {
            if moisture > 0 {
                moisture - 1
            } else {
                if !should_maintain_farmland(args.world.as_ref(), args.position) {
                    turn_to_base_block(args.world, args.position);
                }
                return;
            }
        } else if moisture < MAX_MOISTURE {
            MAX_MOISTURE
        } else {
            return;
        };

        let Some(new_moisture) = fire_moisture_change(args.world, args.position, new_moisture)
        else {
            return;
        };
        props.moisture = new_moisture;
        args.world.set_block_state(
            args.position,
            props.to_state_id(args.block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }

    fn on_landed_upon(&self, args: OnLandedUponArgs<'_>) {
        if let Some(living) = args.entity.get_living_entity() {
            if rand::random::<f32>() < args.fall_distance - 0.5
                && can_trample(args.world, args.position, args.entity)
            {
                let entity_id = args.entity.get_entity().entity_id;
                let mut event = EntityChangeBlockEvent::new(
                    entity_id,
                    *args.position,
                    "minecraft:dirt".to_string(),
                );
                if let Some(server) = args.world.server.upgrade() {
                    server.plugin_manager.fire_blocking(&server, &mut event);
                }
                if !event.cancelled {
                    turn_to_base_block(args.world, args.position);
                }
            }
            living.handle_fall_damage(args.entity, args.fall_distance, 1.0);
        }
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Players can't trample inside spawn protection; mobs need `mobGriefing`.
fn can_trample(world: &Arc<World>, pos: &BlockPos, entity: &dyn EntityBase) -> bool {
    let allowed = entity.get_player().map_or_else(
        || world.level_info.load().game_rules.mob_griefing,
        |player| !world.is_in_spawn_protection(player, pos),
    );
    let dimensions = entity.get_entity().entity_dimension.load();
    allowed && dimensions.width * dimensions.width * dimensions.height > TRAMPLE_MIN_VOLUME
}

/// Returns the moisture a plugin settled on, or `None` if it cancelled the change.
fn fire_moisture_change(world: &Arc<World>, pos: &BlockPos, new_moisture: u8) -> Option<u8> {
    let Some(server) = world.server.upgrade() else {
        return Some(new_moisture);
    };
    let mut event = MoistureChangeEvent::new(*pos, world.clone(), i32::from(new_moisture));
    server.plugin_manager.fire_blocking(&server, &mut event);
    if event.cancelled {
        return None;
    }
    Some(event.new_moisture.clamp(0, i32::from(MAX_MOISTURE)) as u8)
}

/// Vanilla `FarmlandBlock.turnToBaseBlock`.
fn turn_to_base_block(world: &Arc<World>, pos: &BlockPos) {
    let state = world.get_block_state(pos);
    push_entities_up(world, state, Block::DIRT.default_state, pos);
    world.set_block_state(pos, Block::DIRT.default_state.id, BlockFlags::NOTIFY_ALL);
    world.emit_game_event(GameEvent::BlockChange.name(), pos.to_centered_f64());
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    !world.get_block_state(&block_pos.up()).is_solid() || should_maintain_farmland(world, block_pos)
}

fn should_maintain_farmland(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    world
        .get_block(&block_pos.up())
        .has_tag(&tag::Block::MINECRAFT_MAINTAINS_FARMLAND)
}

fn is_near_water(world: &Arc<World>, block_pos: &BlockPos) -> bool {
    for dx in -4..=4 {
        for dy in 0..=1 {
            for dz in -4..=4 {
                let check_pos = block_pos.offset(Vector3 {
                    x: dx,
                    y: dy,
                    z: dz,
                });
                if world
                    .get_fluid(&check_pos)
                    .has_tag(&tag::Fluid::MINECRAFT_WATER)
                {
                    return true;
                }
            }
        }
    }
    false
}
