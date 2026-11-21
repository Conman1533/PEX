use pumpkin_data::translation;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use pumpkin_world::world::BlockFlags;

use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::block::BlockArgumentType;
use crate::command::argument_types::coordinates::block_pos::BlockPosArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::world::World;

const DESCRIPTION: &str = "Place a block.";
const PERMISSION: &str = "minecraft:command.setblock";

const ERROR_FAILED: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_SETBLOCK_FAILED,
    translation::java::COMMANDS_SETBLOCK_FAILED,
);

#[derive(Clone, Copy)]
enum Mode {
    /// with particles + item drops
    Destroy,

    /// only replaces air
    Keep,

    /// default; without particles
    Replace,

    /// places block without triggering updates around it
    Strict,
}

/// Loads the block entity data a block argument gave into the block entity at `pos`, as
/// vanilla `BlockInput.place` does. The block entity keeps its own id and position.
pub(super) fn load_block_entity_data(world: &World, pos: &BlockPos, data: &NbtCompound) {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let mut nbt = NbtCompound::new();
    block_entity.write_internal(&mut nbt);
    world.write_kept_block_entity_nbt(block_entity.as_ref(), &mut nbt);
    nbt.merge(data);
    nbt.put_string("id", block_entity.resource_location().to_string());
    nbt.put_int("x", pos.0.x);
    nbt.put_int("y", pos.0.y);
    nbt.put_int("z", pos.0.z);
    if let Some(loaded) = crate::block::entities::block_entity_from_nbt(&nbt) {
        world.add_block_entity(loaded);
    }
}

struct SetBlockExecutor(Mode);

impl CommandExecutor for SetBlockExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let block = BlockArgumentType::get_input(context, "block")?;
        let block_state_id = block.state;
        let mode = self.0;
        let world = context.source.world();
        let pos = BlockPosArgumentType::get_loaded_block_pos(context, "pos")?;
        // Vanilla places with `UPDATE_CLIENTS` only and updates the neighbours afterwards, the
        // placed block being the source (`ServerLevel.updateNeighboursOnBlockSet`).
        let place_flags = BlockFlags::NOTIFY_LISTENERS | BlockFlags::FORCE_STATE;
        let mut old_state_id = world.get_block_state_id(&pos);

        let success = match mode {
            Mode::Destroy => {
                world.break_block(
                    &pos,
                    None,
                    BlockFlags::SKIP_DROPS | BlockFlags::NOTIFY_ALL | BlockFlags::FORCE_STATE,
                );
                old_state_id = world.get_block_state_id(&pos);
                world.set_block_state(&pos, block_state_id, place_flags);
                true
            }
            Mode::Replace => {
                world.set_block_state(&pos, block_state_id, place_flags);
                true
            }
            Mode::Keep => {
                if old_state_id.to_state().is_air() {
                    world.set_block_state(&pos, block_state_id, place_flags);
                    true
                } else {
                    false
                }
            }
            Mode::Strict => {
                world.set_block_state(
                    &pos,
                    block_state_id,
                    BlockFlags::NOTIFY_LISTENERS
                        | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
                        | BlockFlags::FORCE_STATE,
                );
                true
            }
        };

        if success {
            if let Some(data) = &block.nbt {
                load_block_entity_data(world, &pos, data);
            }
            if !matches!(mode, Mode::Strict) {
                world.update_neighbours_on_block_set(&pos, old_state_id);
            }
            world.flush_block_updates();
            context.source.send_feedback(
                TextComponent::translate_cross(
                    pumpkin_data::translation::java::COMMANDS_SETBLOCK_SUCCESS,
                    pumpkin_data::translation::bedrock::COMMANDS_SETBLOCK_SUCCESS,
                    [
                        TextComponent::text(pos.0.x.to_string()),
                        TextComponent::text(pos.0.y.to_string()),
                        TextComponent::text(pos.0.z.to_string()),
                    ],
                ),
                true,
            );
            Ok(1)
        } else {
            Err(ERROR_FAILED.create_without_context())
        }
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("setblock", DESCRIPTION).requires(PERMISSION).then(
            argument("pos", BlockPosArgumentType).then(
                argument("block", BlockArgumentType)
                    .executes(SetBlockExecutor(Mode::Replace))
                    .then(literal("destroy").executes(SetBlockExecutor(Mode::Destroy)))
                    .then(literal("keep").executes(SetBlockExecutor(Mode::Keep)))
                    .then(literal("replace").executes(SetBlockExecutor(Mode::Replace)))
                    .then(literal("strict").executes(SetBlockExecutor(Mode::Strict))),
            ),
        ),
    );
}
