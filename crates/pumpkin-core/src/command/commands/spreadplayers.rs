use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, tag, translation};
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::coordinates::vec2::Vec2ArgumentType;
use crate::command::argument_types::core::bool::BoolArgumentType;
use crate::command::argument_types::core::float::FloatArgumentType;
use crate::command::argument_types::core::integer::IntegerArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::world::World;

const DESCRIPTION: &str = "Teleports entities to random surface locations within an area.";

const PERMISSION: &str = "minecraft:command.spreadplayers";

const ARG_CENTER: &str = "center";
const ARG_SPREAD_DISTANCE: &str = "spreadDistance";
const ARG_MAX_RANGE: &str = "maxRange";
const ARG_MAX_HEIGHT: &str = "maxHeight";
const ARG_RESPECT_TEAMS: &str = "respectTeams";
const ARG_TARGETS: &str = "targets";

/// The maximum number of relaxation/retry iterations, matching Vanilla.
const MAX_ITERATIONS: usize = 10000;

const FAILED_ENTITIES_ERROR_TYPE: CommandErrorType<4> = CommandErrorType::new(
    translation::java::COMMANDS_SPREADPLAYERS_FAILED_ENTITIES,
    translation::bedrock::COMMANDS_SPREADPLAYERS_FAILURE_PLAYERS,
);

const FAILED_TEAMS_ERROR_TYPE: CommandErrorType<4> = CommandErrorType::new(
    translation::java::COMMANDS_SPREADPLAYERS_FAILED_TEAMS,
    translation::bedrock::COMMANDS_SPREADPLAYERS_FAILURE_TEAMS,
);

const INVALID_MAX_HEIGHT_ERROR_TYPE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::COMMANDS_SPREADPLAYERS_FAILED_INVALID_HEIGHT,
    translation::java::COMMANDS_SPREADPLAYERS_FAILED_INVALID_HEIGHT,
);

/// A candidate spread location, equivalent to Vanilla's
/// `SpreadPlayersCommand.Position`.
#[derive(Clone, Copy, Default)]
struct Pile {
    x: f64,
    z: f64,
}

impl Pile {
    fn distance(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx.hypot(dz)
    }

    fn randomize(&mut self, min_x: f64, min_z: f64, max_x: f64, max_z: f64) {
        self.x = rand::random_range(min_x..=max_x);
        self.z = rand::random_range(min_z..=max_z);
    }

    /// Clamps this pile into the spread area. Returns whether it moved.
    fn clamp(&mut self, min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> bool {
        let mut moved = false;
        if self.x < min_x {
            self.x = min_x;
            moved = true;
        } else if self.x > max_x {
            self.x = max_x;
            moved = true;
        }
        if self.z < min_z {
            self.z = min_z;
            moved = true;
        } else if self.z > max_z {
            self.z = max_z;
            moved = true;
        }
        moved
    }

    /// Vanilla `Position.getSpawnY`: the y above the highest block under two air blocks,
    /// scanning down from `max_height`, or `max_height + 1` when the column is all air.
    fn spawn_y(&self, world: &World, max_height: i32) -> i32 {
        let x = self.x.floor() as i32;
        let z = self.z.floor() as i32;
        load_column(world, x, z);
        let is_air = |y: i32| world.get_block_state(&BlockPos::new(x, y, z)).is_air();
        let mut y = max_height + 1;
        let mut air2_above = is_air(y);
        y -= 1;
        let mut air1_above = is_air(y);
        while y > world.min_y {
            y -= 1;
            let current_is_air = is_air(y);
            if !current_is_air && air1_above && air2_above {
                return y + 1;
            }
            air2_above = air1_above;
            air1_above = current_is_air;
        }
        max_height + 1
    }

    /// Vanilla `Position.isSafe`: below `max_height`, not a liquid, and a block entities may
    /// teleport onto.
    fn is_safe(&self, world: &World, max_height: i32) -> bool {
        let pos = BlockPos::new(
            self.x.floor() as i32,
            self.spawn_y(world, max_height) - 1,
            self.z.floor() as i32,
        );
        let state = world.get_block_state(&pos);
        pos.0.y < max_height
            && !state.is_liquid()
            && Block::from_state_id(state.id)
                .has_tag(&tag::Block::MINECRAFT_ENTITIES_CAN_TELEPORT_TO)
    }
}

/// Loads the chunk holding the column, as vanilla's block lookups do, so an unloaded spot is
/// judged by its real blocks instead of air.
fn load_column(world: &World, x: i32, z: i32) {
    let pos = Vector2::new(x >> 4, z >> 4);
    if world.level.is_chunk_loaded(&pos) {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let level = world.level.clone();
    tokio::task::block_in_place(|| {
        handle.block_on(level.get_or_fetch_chunk(pos, |_| ()));
    });
}

struct SpreadArea {
    min_x: f64,
    min_z: f64,
    max_x: f64,
    max_z: f64,
}

/// Vanilla `SpreadPlayersCommand.spreadPositions`: pushes piles apart until every pair is at
/// least `spread_distance` apart inside the area, re-rolling any pile on an unsafe spot, for at
/// most [`MAX_ITERATIONS`]. `Err` carries the closest distance between piles when it gave up.
fn spread_piles(
    world: &World,
    piles: &mut [Pile],
    spread_distance: f64,
    max_height: i32,
    area: &SpreadArea,
) -> Result<(), f64> {
    let (min_x, min_z, max_x, max_z) = (area.min_x, area.min_z, area.max_x, area.max_z);
    let mut has_collisions = true;
    let mut min_distance = f64::from(f32::MAX);
    let mut iteration = 0;
    while iteration < MAX_ITERATIONS && has_collisions {
        has_collisions = false;
        min_distance = f64::from(f32::MAX);
        for i in 0..piles.len() {
            let mut push = Pile::default();
            let mut neighbours = 0;
            for j in 0..piles.len() {
                if i == j {
                    continue;
                }
                let distance = piles[i].distance(&piles[j]);
                min_distance = min_distance.min(distance);
                if distance < spread_distance {
                    neighbours += 1;
                    push.x += piles[j].x - piles[i].x;
                    push.z += piles[j].z - piles[i].z;
                }
            }
            if neighbours > 0 {
                push.x /= f64::from(neighbours);
                push.z /= f64::from(neighbours);
                let length = push.x.hypot(push.z);
                if length > 0.0 {
                    piles[i].x -= push.x / length;
                    piles[i].z -= push.z / length;
                } else {
                    piles[i].randomize(min_x, min_z, max_x, max_z);
                }
                has_collisions = true;
            }
            if piles[i].clamp(min_x, min_z, max_x, max_z) {
                has_collisions = true;
            }
        }
        if !has_collisions {
            for pile in piles.iter_mut() {
                if !pile.is_safe(world, max_height) {
                    pile.randomize(min_x, min_z, max_x, max_z);
                    has_collisions = true;
                }
            }
        }
        iteration += 1;
    }
    if min_distance == f64::from(f32::MAX) {
        min_distance = 0.0;
    }
    if iteration >= MAX_ITERATIONS {
        Err(min_distance)
    } else {
        Ok(())
    }
}

/// Java's `Float.toString` for the plain numbers a center can be: `57.5`, `57.0`.
fn java_float(value: f64) -> String {
    #[expect(clippy::cast_possible_truncation)]
    let text = format!("{}", value as f32);
    if text.contains('.') || text.contains('e') || text.contains("inf") || text.contains("NaN") {
        text
    } else {
        format!("{text}.0")
    }
}

/// Returns the average distance from each pile to its closest neighbor.
fn average_min_distance(piles: &[Pile]) -> f64 {
    if piles.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for pile in piles {
        let mut closest = f64::MAX;
        for other in piles {
            if !std::ptr::eq(pile, other) {
                closest = closest.min(pile.distance(other));
            }
        }
        total += closest;
    }
    #[expect(clippy::cast_precision_loss)]
    let count = piles.len() as f64;
    total / count
}

struct SpreadPlayersExecutor {
    has_max_height: bool,
}

impl CommandExecutor for SpreadPlayersExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let center = Vec2ArgumentType::get_vector2(context, ARG_CENTER)?;
        let spread_distance = f64::from(FloatArgumentType::get(context, ARG_SPREAD_DISTANCE)?);
        let max_range = f64::from(FloatArgumentType::get(context, ARG_MAX_RANGE)?);
        let respect_teams = BoolArgumentType::get(context, ARG_RESPECT_TEAMS)?;
        let targets = EntityArgumentType::get_entities(context, ARG_TARGETS)?;

        let center_x = center.x;
        let center_z = center.y;
        let area = SpreadArea {
            min_x: center_x - max_range,
            min_z: center_z - max_range,
            max_x: center_x + max_range,
            max_z: center_z + max_range,
        };

        let world = context.source.world().clone();
        let min_y = world.min_y;
        let max_height = if self.has_max_height {
            let val = IntegerArgumentType::get(context, ARG_MAX_HEIGHT)?;
            if val < min_y {
                return Err(INVALID_MAX_HEIGHT_ERROR_TYPE.create_without_context(
                    TextComponent::text(val.to_string()),
                    TextComponent::text(min_y.to_string()),
                ));
            }
            val
        } else {
            // Vanilla passes `getMaxY() + 1`.
            world.dimension.height + min_y
        };

        let pile_count = if respect_teams { 1 } else { targets.len() };

        let mut piles = vec![Pile::default(); pile_count];
        for pile in &mut piles {
            pile.randomize(area.min_x, area.min_z, area.max_x, area.max_z);
        }
        if let Err(min_distance) =
            spread_piles(&world, &mut piles, spread_distance, max_height, &area)
        {
            let error_type = if respect_teams {
                &FAILED_TEAMS_ERROR_TYPE
            } else {
                &FAILED_ENTITIES_ERROR_TYPE
            };
            return Err(error_type.create_without_context(
                TextComponent::text(pile_count.to_string()),
                TextComponent::text(java_float(center_x)),
                TextComponent::text(java_float(center_z)),
                TextComponent::text(format!("{min_distance:.2}")),
            ));
        }

        for (index, target) in targets.iter().enumerate() {
            let pile = piles[index % pile_count];
            let y = pile.spawn_y(&world, max_height);
            target.teleport(
                Vector3::new(pile.x.floor() + 0.5, f64::from(y), pile.z.floor() + 0.5),
                None,
                None,
                world.clone(),
            );
        }

        let average_distance = average_min_distance(&piles);

        let success_key = if respect_teams {
            (
                translation::java::COMMANDS_SPREADPLAYERS_SUCCESS_TEAMS,
                translation::java::COMMANDS_SPREADPLAYERS_SUCCESS_TEAMS,
            )
        } else {
            (
                translation::java::COMMANDS_SPREADPLAYERS_SUCCESS_ENTITIES,
                translation::java::COMMANDS_SPREADPLAYERS_SUCCESS_ENTITIES,
            )
        };

        context.source.send_feedback(
            TextComponent::translate_cross(
                success_key.0,
                success_key.1,
                [
                    TextComponent::text(pile_count.to_string()),
                    TextComponent::text(java_float(center_x)),
                    TextComponent::text(java_float(center_z)),
                    TextComponent::text(format!("{average_distance:.2}")),
                ],
            ),
            true,
        );

        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        Ok(pile_count as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("spreadplayers", DESCRIPTION)
            .requires(PERMISSION)
            .then(
                argument(ARG_CENTER, Vec2ArgumentType::Default).then(
                    argument(ARG_SPREAD_DISTANCE, FloatArgumentType::with_min(0.0)).then(
                        argument(ARG_MAX_RANGE, FloatArgumentType::with_min(1.0))
                            .then(argument(ARG_RESPECT_TEAMS, BoolArgumentType).then(
                                argument(ARG_TARGETS, EntityArgumentType::Entities).executes(
                                    SpreadPlayersExecutor {
                                        has_max_height: false,
                                    },
                                ),
                            ))
                            .then(
                                literal("under").then(
                                    argument(ARG_MAX_HEIGHT, IntegerArgumentType::any()).then(
                                        argument(ARG_RESPECT_TEAMS, BoolArgumentType).then(
                                            argument(ARG_TARGETS, EntityArgumentType::Entities)
                                                .executes(SpreadPlayersExecutor {
                                                    has_max_height: true,
                                                }),
                                        ),
                                    ),
                                ),
                            ),
                    ),
                ),
            ),
    );
}
