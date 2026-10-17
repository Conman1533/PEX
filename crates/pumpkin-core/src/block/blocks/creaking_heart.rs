use std::sync::Arc;

use pumpkin_data::block_properties::{
    Axis, CreakingHeartLikeProperties, CreakingHeartState, PaleOakWoodLikeProperties,
};
use pumpkin_data::damage::DamageType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::entities::creaking_heart::CreakingHeartBlockEntity;
use crate::block::{
    BlockBehaviour, BrokenArgs, ExplodeArgs, GetComparatorOutputArgs, OnNeighborUpdateArgs,
    OnPlaceArgs, PlacedArgs,
};
use crate::entity::experience_orb::ExperienceOrbEntity;

#[pumpkin_block("minecraft:creaking_heart")]
pub struct CreakingHeartBlock;

impl CreakingHeartBlock {
    fn has_matching_log(
        world: &dyn pumpkin_world::world::BlockAccessor,
        pos: &pumpkin_util::math::position::BlockPos,
        axis: Axis,
    ) -> bool {
        let state_id = world.get_block_state_id(pos);
        let block = Block::from_state_id(state_id);
        block.has_tag(&tag::Block::MINECRAFT_PALE_OAK_LOGS)
            && PaleOakWoodLikeProperties::from_state_id(state_id).axis == axis
    }

    pub fn check_active_logs(
        world: &dyn pumpkin_world::world::BlockAccessor,
        pos: &pumpkin_util::math::position::BlockPos,
        axis: Axis,
    ) -> bool {
        let (pos_a, pos_b) = match axis {
            Axis::X => (pos.west(), pos.east()),
            Axis::Y => (pos.down(), pos.up()),
            Axis::Z => (pos.north(), pos.south()),
        };

        Self::has_matching_log(world, &pos_a, axis) && Self::has_matching_log(world, &pos_b, axis)
    }
}

impl BlockBehaviour for CreakingHeartBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = CreakingHeartLikeProperties::from_state_id(args.block.default_state.id);
        props.axis = match args.direction {
            pumpkin_data::BlockDirection::North | pumpkin_data::BlockDirection::South => Axis::Z,
            pumpkin_data::BlockDirection::East | pumpkin_data::BlockDirection::West => Axis::X,
            pumpkin_data::BlockDirection::Up | pumpkin_data::BlockDirection::Down => Axis::Y,
        };
        props.creaking_heart_state = CreakingHeartState::Uprooted;
        props.to_state_id(args.block)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let entity = CreakingHeartBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(entity));

        let state_id = args.world.get_block_state_id(args.position);
        let mut props = CreakingHeartLikeProperties::from_state_id(state_id);

        if Self::check_active_logs(args.world.as_ref(), args.position, props.axis) {
            props.creaking_heart_state = if args.world.creaking_active(args.position) {
                CreakingHeartState::Awake
            } else {
                CreakingHeartState::Dormant
            };
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL,
            );

            args.world.play_sound(
                Sound::BlockCreakingHeartSpawn,
                SoundCategory::Blocks,
                &args.position.to_f64(),
            );
        }
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let state_id = args.world.get_block_state_id(args.position);
        let mut props = CreakingHeartLikeProperties::from_state_id(state_id);

        let active_logs = Self::check_active_logs(args.world.as_ref(), args.position, props.axis);
        let new_state = if active_logs {
            if props.creaking_heart_state == CreakingHeartState::Uprooted {
                args.world.play_sound(
                    Sound::BlockCreakingHeartSpawn,
                    SoundCategory::Blocks,
                    &args.position.to_f64(),
                );
            }
            if args.world.creaking_active(args.position) {
                CreakingHeartState::Awake
            } else {
                CreakingHeartState::Dormant
            }
        } else {
            CreakingHeartState::Uprooted
        };

        if props.creaking_heart_state != new_state {
            props.creaking_heart_state = new_state;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL,
            );
        }
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        if let Some(be) = args.world.get_block_entity(args.position)
            && let Some(heart_be) = be.as_any().downcast_ref::<CreakingHeartBlockEntity>()
        {
            heart_be.remove_protector(args.world, Some(DamageType::PLAYER_ATTACK));
        }

        let props = CreakingHeartLikeProperties::from_state_id(args.state.id);
        if props.natural && !args.player.is_creative() && !args.player.is_spectator() {
            let xp = rand::rng().random_range(20..=24);
            ExperienceOrbEntity::spawn(args.world, args.position.to_centered_f64(), xp);
        }

        args.world.play_sound(
            Sound::BlockCreakingHeartBreak,
            SoundCategory::Blocks,
            &args.position.to_f64(),
        );
        args.world
            .drop_stack(args.position, ItemStack::new(1, &Item::CREAKING_HEART));
    }

    fn explode(&self, args: ExplodeArgs<'_>) {
        if let Some(be) = args.world.get_block_entity(args.position)
            && let Some(heart_be) = be.as_any().downcast_ref::<CreakingHeartBlockEntity>()
        {
            heart_be.remove_protector(args.world, Some(DamageType::EXPLOSION));
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        let props = CreakingHeartLikeProperties::from_state_id(args.state.id);
        if props.creaking_heart_state == CreakingHeartState::Uprooted {
            return Some(0);
        }
        Some(
            args.world
                .get_block_entity(args.position)
                .and_then(|be| {
                    be.as_any()
                        .downcast_ref::<CreakingHeartBlockEntity>()
                        .map(CreakingHeartBlockEntity::get_analog_output_signal)
                })
                .unwrap_or(0),
        )
    }
}
