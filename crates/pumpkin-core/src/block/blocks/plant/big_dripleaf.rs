use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::block::blocks::plant::big_dripleaf_stem::place_stem;
use crate::block::blocks::redstone::block_receives_redstone_power;
use crate::block::{
    BlockBehaviour, BonemealArgs, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnEntityStepArgs,
    OnNeighborUpdateArgs, OnPlaceArgs, OnProjectileHitArgs, OnScheduledTickArgs,
};
use crate::entity::EntityBase;
use crate::world::World;
use pumpkin_data::block_properties::{
    BigDripleafLikeProperties, HorizontalFacing, LadderLikeProperties, Tilt,
};
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockStateId, tag};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::{BlockAccessor, BlockFlags};
use rand::RngExt;

type BigDripleafStemProperties = LadderLikeProperties;

#[pumpkin_block("minecraft:big_dripleaf")]
pub struct BigDripleafBlock;

impl BlockBehaviour for BigDripleafBlock {
    // Vanilla uses `entityInside`; the leaf's collision box sits inside its own block space, so
    // an entity standing on it is both "inside" and "stepping on" it.
    fn on_entity_step(&self, args: OnEntityStepArgs<'_>) {
        let props = BigDripleafLikeProperties::from_state_id(args.state.id);
        if props.tilt == Tilt::None
            && can_entity_tilt(args.position, args.entity)
            && !block_receives_redstone_power(args.world, args.position)
        {
            set_tilt_and_schedule_tick(
                args.state.id,
                args.world,
                args.position,
                Tilt::Unstable,
                None,
            );
        }
    }

    fn on_projectile_hit(&self, args: OnProjectileHitArgs<'_>) {
        set_tilt_and_schedule_tick(
            args.state.id,
            args.world,
            args.position,
            Tilt::Full,
            Some(Sound::BlockBigDripleafTiltDown),
        );
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let state_id = args.world.get_block_state_id(args.position);
        if block_receives_redstone_power(args.world, args.position) {
            reset_tilt(state_id, args.world, args.position);
            return;
        }

        match BigDripleafLikeProperties::from_state_id(state_id).tilt {
            Tilt::Unstable => set_tilt_and_schedule_tick(
                state_id,
                args.world,
                args.position,
                Tilt::Partial,
                Some(Sound::BlockBigDripleafTiltDown),
            ),
            Tilt::Partial => set_tilt_and_schedule_tick(
                state_id,
                args.world,
                args.position,
                Tilt::Full,
                Some(Sound::BlockBigDripleafTiltDown),
            ),
            Tilt::Full => reset_tilt(state_id, args.world, args.position),
            Tilt::None => {}
        }
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        if block_receives_redstone_power(args.world, args.position) {
            let state_id = args.world.get_block_state_id(args.position);
            reset_tilt(state_id, args.world, args.position);
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_survive(args.block_accessor, args.position)
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let (below_block, below_state_id) =
            args.world.get_block_and_state_id(&args.position.down());
        let facing = if below_block == &Block::BIG_DRIPLEAF {
            BigDripleafLikeProperties::from_state_id(below_state_id).facing
        } else if below_block == &Block::BIG_DRIPLEAF_STEM {
            BigDripleafStemProperties::from_state_id(below_state_id).facing
        } else {
            args.player
                .living_entity
                .entity
                .get_horizontal_facing()
                .opposite()
        };
        let mut props = BigDripleafLikeProperties::default(args.block);
        props.facing = facing;
        props.waterlogged = args.replacing.water_source();
        props.to_state_id(args.block)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Down && !can_survive(args.world, args.position) {
            return Block::AIR.default_state.id;
        }
        // A leaf placed on top turns this leaf into a stem.
        if args.direction == BlockDirection::Up
            && args.neighbor_state_id.to_block() == &Block::BIG_DRIPLEAF
        {
            let props = BigDripleafLikeProperties::from_state_id(args.state_id);
            let mut stem_props = BigDripleafStemProperties::default(&Block::BIG_DRIPLEAF_STEM);
            stem_props.facing = props.facing;
            stem_props.waterlogged = props.waterlogged;
            return stem_props.to_state_id(&Block::BIG_DRIPLEAF_STEM);
        }
        args.state_id
    }

    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        can_grow_into(args.world, &args.position.up())
    }

    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        let above = args.position.up();
        if can_grow_into(args.world, &above) {
            let props = BigDripleafLikeProperties::from_state_id(args.state_id);
            place_stem(args.world, args.position, props.facing);
            place_leaf(args.world, &above, props.facing);
        }
    }
}

/// Vanilla `BigDripleafBlock.placeWithRandomHeight`, used when a small dripleaf is bonemealed.
pub fn place_with_random_height(
    world: &Arc<World>,
    stem_bottom: &BlockPos,
    facing: HorizontalFacing,
) {
    let desired_height = rand::rng().random_range(2..=5);
    let mut height = 0;
    while height < desired_height && can_grow_into(world, &stem_bottom.up_height(height)) {
        height += 1;
    }

    let leaf_y = height - 1;
    for y in 0..leaf_y {
        place_stem(world, &stem_bottom.up_height(y), facing);
    }
    place_leaf(world, &stem_bottom.up_height(leaf_y.max(0)), facing);
}

/// Vanilla `BigDripleafBlock.canGrowInto`.
pub fn can_grow_into(world: &World, pos: &BlockPos) -> bool {
    let block = world.get_block(pos);
    world.is_in_height_limit(pos.0.y)
        && (block == &Block::AIR
            || block == &Block::CAVE_AIR
            || block == &Block::VOID_AIR
            || block == &Block::WATER
            || block == &Block::SMALL_DRIPLEAF)
}

/// Vanilla `BigDripleafBlock.place`.
pub fn place_leaf(world: &Arc<World>, pos: &BlockPos, facing: HorizontalFacing) {
    let mut props = BigDripleafLikeProperties::default(&Block::BIG_DRIPLEAF);
    props.facing = facing;
    props.waterlogged = is_water_source(world, pos);
    world.set_block_state(
        pos,
        props.to_state_id(&Block::BIG_DRIPLEAF),
        BlockFlags::NOTIFY_ALL,
    );
}

pub fn is_water_source(world: &World, pos: &BlockPos) -> bool {
    let (fluid, fluid_state) = world.get_fluid_and_fluid_state(pos);
    fluid_state.is_source && fluid.has_tag(&tag::Fluid::MINECRAFT_WATER)
}

fn can_survive(block_accessor: &dyn BlockAccessor, pos: &BlockPos) -> bool {
    let below = block_accessor.get_block(&pos.down());
    below == &Block::BIG_DRIPLEAF
        || below == &Block::BIG_DRIPLEAF_STEM
        || below.has_tag(&tag::Block::MINECRAFT_SUPPORTS_BIG_DRIPLEAF)
}

fn set_tilt_and_schedule_tick(
    state_id: BlockStateId,
    world: &Arc<World>,
    pos: &BlockPos,
    tilt: Tilt,
    sound: Option<Sound>,
) {
    set_tilt(state_id, world, pos, tilt);
    if let Some(sound) = sound {
        play_tilt_sound(world, pos, sound);
    }
    let tick_delay = match tilt {
        Tilt::None => return,
        Tilt::Unstable | Tilt::Partial => 10,
        Tilt::Full => 100,
    };
    world.schedule_block_tick(&Block::BIG_DRIPLEAF, *pos, tick_delay, TickPriority::Normal);
}

fn reset_tilt(state_id: BlockStateId, world: &Arc<World>, pos: &BlockPos) {
    set_tilt(state_id, world, pos, Tilt::None);
    if BigDripleafLikeProperties::from_state_id(state_id).tilt != Tilt::None {
        play_tilt_sound(world, pos, Sound::BlockBigDripleafTiltUp);
    }
}

fn set_tilt(state_id: BlockStateId, world: &Arc<World>, pos: &BlockPos, tilt: Tilt) {
    let mut props = BigDripleafLikeProperties::from_state_id(state_id);
    let previous_tilt = props.tilt;
    props.tilt = tilt;
    world.set_block_state(
        pos,
        props.to_state_id(&Block::BIG_DRIPLEAF),
        BlockFlags::NOTIFY_LISTENERS,
    );
    if causes_vibration(tilt) && tilt != previous_tilt {
        world.emit_game_event(GameEvent::BlockChange.name(), pos.to_centered_f64());
    }
}

/// Vanilla `Tilt.causesVibration`.
const fn causes_vibration(tilt: Tilt) -> bool {
    !matches!(tilt, Tilt::Unstable)
}

fn play_tilt_sound(world: &Arc<World>, pos: &BlockPos, sound: Sound) {
    let pitch = rand::rng().random_range(0.8f32..1.2f32);
    world.play_sound_fine(
        sound,
        SoundCategory::Blocks,
        &pos.to_centered_f64(),
        1.0,
        pitch,
    );
}

fn can_entity_tilt<T: EntityBase + ?Sized>(pos: &BlockPos, entity: &T) -> bool {
    entity.get_entity().on_ground.load(Ordering::Relaxed)
        && entity.get_entity().pos.load().y > f64::from(pos.0.y) + 0.6875
}
