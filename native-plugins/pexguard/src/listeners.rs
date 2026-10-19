//! Event handlers that enforce region flags.

use crate::region::{Flag, entered};
use crate::{PREFIX, Shared};
use pumpkin::entity::EntityBase;
use pumpkin::entity::player::Player;
use pumpkin::plugin::block::block_break::BlockBreakEvent;
use pumpkin::plugin::block::block_burn::BlockBurnEvent;
use pumpkin::plugin::block::block_explode::BlockExplodeEvent;
use pumpkin::plugin::block::block_from_to::BlockFromToEvent;
use pumpkin::plugin::block::block_ignite::BlockIgniteEvent;
use pumpkin::plugin::block::block_piston::{BlockPistonExtendEvent, BlockPistonRetractEvent};
use pumpkin::plugin::block::block_place::BlockPlaceEvent;
use pumpkin::plugin::block::block_spread::BlockSpreadEvent;
use pumpkin::plugin::entity::entity_damage_by_entity::EntityDamageByEntityEvent;
use pumpkin::plugin::entity::entity_explode::EntityExplodeEvent;
use pumpkin::plugin::entity::entity_spawn::EntitySpawnEvent;
use pumpkin::plugin::player::player_bucket::{PlayerBucketEmptyEvent, PlayerBucketFillEvent};
use pumpkin::plugin::player::player_interact_entity_event::PlayerInteractEntityEvent;
use pumpkin::plugin::player::player_interact_event::{InteractAction, PlayerInteractEvent};
use pumpkin::plugin::player::player_move::PlayerMoveEvent;
use pumpkin::plugin::{BoxFuture, Context, EventHandler, EventPriority};
use pumpkin::server::Server;
use pumpkin::world::World;
use pumpkin_data::Block;
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

pub const PERM_BYPASS: &str = "pexguard:bypass";
/// Half-width of the box checked around an explosion; the event doesn't list destroyed blocks.
const EXPLOSION_RADIUS: i32 = 8;

/// Defines a blocking event handler that runs `$body` with the server's runtime entered.
macro_rules! handler {
    ($name:ident, $event:ty, |$shared:ident, $server:ident, $ev:ident| $body:block) => {
        struct $name(Shared);

        impl EventHandler<$event> for $name {
            fn handle_blocking<'a>(
                &'a self,
                server: &'a Arc<Server>,
                event: &'a mut $event,
            ) -> BoxFuture<'a, ()> {
                let _runtime = server.runtime.enter();
                if !event.cancelled {
                    let $shared = &self.0;
                    let $server = server;
                    let $ev = event;
                    $body
                }
                Box::pin(async {})
            }
        }
    };
}

pub fn register(context: &Context, shared: &Shared) {
    macro_rules! reg {
        ($event:ty, $handler:ident) => {
            context.register_event::<$event, _>(
                Arc::new($handler(shared.clone())),
                EventPriority::High,
                true,
            );
        };
    }
    reg!(BlockBreakEvent, BreakHandler);
    reg!(BlockPlaceEvent, PlaceHandler);
    reg!(PlayerBucketEmptyEvent, BucketEmptyHandler);
    reg!(PlayerBucketFillEvent, BucketFillHandler);
    reg!(PlayerInteractEvent, InteractHandler);
    reg!(PlayerInteractEntityEvent, InteractEntityHandler);
    reg!(EntityDamageByEntityEvent, DamageHandler);
    reg!(EntitySpawnEvent, SpawnHandler);
    reg!(EntityExplodeEvent, EntityExplodeHandler);
    reg!(BlockExplodeEvent, BlockExplodeHandler);
    reg!(BlockSpreadEvent, SpreadHandler);
    reg!(BlockBurnEvent, BurnHandler);
    reg!(BlockIgniteEvent, IgniteHandler);
    reg!(BlockFromToEvent, FlowHandler);
    reg!(BlockPistonExtendEvent, PistonExtendHandler);
    reg!(BlockPistonRetractEvent, PistonRetractHandler);
    reg!(PlayerMoveEvent, MoveHandler);
}

const fn pos(pos: &BlockPos) -> [i32; 3] {
    [pos.0.x, pos.0.y, pos.0.z]
}

const fn floored(pos: Vector3<f64>) -> [i32; 3] {
    [
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    ]
}

/// Whether `player` may do `flag` at `at` in their current world, telling them if not.
fn player_allowed(
    shared: &Shared,
    server: &Server,
    player: &Arc<Player>,
    at: [i32; 3],
    flag: Flag,
) -> bool {
    if player.has_permission(server, PERM_BYPASS) {
        return true;
    }
    let world = player.world();
    let uuid = player.gameprofile.id.to_string();
    let allowed = shared
        .regions()
        .allows(world.get_world_name(), at, flag, Some(&uuid));
    if !allowed {
        player.send_system_message_raw(&PREFIX.error("You can't do that here."), true);
    }
    allowed
}

/// For events without a world: whether `flag` is denied at `at` in any world.
fn any_world_denies(shared: &Shared, server: &Server, at: [i32; 3], flag: Flag) -> bool {
    let regions = shared.regions();
    server
        .worlds
        .load()
        .iter()
        .any(|world| !regions.allows(world.get_world_name(), at, flag, None))
}

fn find_entity(server: &Server, id: i32) -> Option<(Arc<World>, Arc<dyn EntityBase>)> {
    server.worlds.load().iter().find_map(|world| {
        world
            .get_entity_by_id(id)
            .map(|entity| (world.clone(), entity))
    })
}

fn explosion_denied(shared: &Shared, world: &str, center: [i32; 3]) -> bool {
    let min = center.map(|c| c.saturating_sub(EXPLOSION_RADIUS));
    let max = center.map(|c| c.saturating_add(EXPLOSION_RADIUS));
    shared
        .regions()
        .denies_in_box(world, min, max, Flag::Explosions)
}

const CONTAINERS: &[&str] = &[
    "chest",
    "trapped_chest",
    "barrel",
    "furnace",
    "blast_furnace",
    "smoker",
    "hopper",
    "dispenser",
    "dropper",
    "brewing_stand",
    "crafter",
    "chiseled_bookshelf",
    "decorated_pot",
    "lectern",
    "jukebox",
    "beacon",
    "ender_chest",
];
const INTERACTIVE_SUFFIXES: &[&str] = &[
    "_door",
    "_trapdoor",
    "_fence_gate",
    "_button",
    "_bed",
    "_shulker_box",
    "_sign",
    "_hanging_sign",
    "_cauldron",
    "_anvil",
];
const INTERACTIVE: &[&str] = &[
    "lever",
    "repeater",
    "comparator",
    "note_block",
    "bell",
    "daylight_detector",
    "cake",
    "respawn_anchor",
    "campfire",
    "soul_campfire",
    "composter",
    "cauldron",
    "flower_pot",
    "anvil",
    "shulker_box",
    "beehive",
    "bee_nest",
    "cartography_table",
    "loom",
    "stonecutter",
    "grindstone",
    "smithing_table",
    "enchanting_table",
];

fn block_flag(block: &Block) -> Option<Flag> {
    let name = block.name;
    if CONTAINERS.contains(&name) || name.ends_with("_shulker_box") {
        Some(Flag::ChestAccess)
    } else if INTERACTIVE.contains(&name)
        || INTERACTIVE_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
    {
        Some(Flag::Interact)
    } else {
        None
    }
}

handler!(BreakHandler, BlockBreakEvent, |shared, server, event| {
    // Breaks without a player (explosions, physics) are covered by the other flags.
    if let Some(player) = event.player.clone()
        && !player_allowed(
            shared,
            server,
            &player,
            pos(&event.block_position),
            Flag::Build,
        )
    {
        event.cancelled = true;
    }
});

handler!(PlaceHandler, BlockPlaceEvent, |shared, server, event| {
    let player = event.player.clone();
    if !player_allowed(
        shared,
        server,
        &player,
        pos(&event.block_position),
        Flag::Build,
    ) {
        event.cancelled = true;
    }
});

handler!(
    BucketEmptyHandler,
    PlayerBucketEmptyEvent,
    |shared, server, event| {
        let player = event.player.clone();
        if !player_allowed(shared, server, &player, pos(&event.block_pos), Flag::Build) {
            event.cancelled = true;
        }
    }
);

handler!(
    BucketFillHandler,
    PlayerBucketFillEvent,
    |shared, server, event| {
        let player = event.player.clone();
        if !player_allowed(shared, server, &player, pos(&event.block_pos), Flag::Build) {
            event.cancelled = true;
        }
    }
);

handler!(
    InteractHandler,
    PlayerInteractEvent,
    |shared, server, event| {
        if matches!(event.action, InteractAction::RightClickBlock)
            && let (Some(clicked), Some(flag)) = (event.clicked_pos, block_flag(event.block))
        {
            let player = event.player.clone();
            if !player_allowed(shared, server, &player, pos(&clicked), flag) {
                event.cancelled = true;
            }
        }
    }
);

handler!(
    InteractEntityHandler,
    PlayerInteractEntityEvent,
    |shared, server, event| {
        let target = event.target.get_entity();
        let name = target.entity_type.resource_name;
        // Frames, stands and paintings are part of builds; anything else (villagers, animals) is use.
        let flag = if name.contains("item_frame") || name == "armor_stand" || name == "painting" {
            Flag::Build
        } else {
            Flag::Interact
        };
        let player = event.player.clone();
        if !player_allowed(shared, server, &player, floored(target.pos.load()), flag) {
            event.cancelled = true;
        }
    }
);

handler!(
    DamageHandler,
    EntityDamageByEntityEvent,
    |shared, server, event| {
        let Some((world, target)) = find_entity(server, event.entity_id) else {
            return Box::pin(async {});
        };
        let world_name = world.get_world_name().to_owned();
        let target_pos = floored(target.get_entity().pos.load());
        let target_player = world.get_player_by_id(event.entity_id);
        let damager_player = world.get_player_by_id(event.damager_id);
        let denied = match (target_player, damager_player) {
            (Some(_), Some(attacker)) => {
                let attacker_pos = floored(attacker.position());
                let regions = shared.regions();
                let denied = !regions.allows(&world_name, target_pos, Flag::Pvp, None)
                    || !regions.allows(&world_name, attacker_pos, Flag::Pvp, None);
                drop(regions);
                if denied {
                    attacker.send_system_message_raw(&PREFIX.error("PvP is off here."), true);
                }
                denied
            }
            (Some(_), None) => {
                !shared
                    .regions()
                    .allows(&world_name, target_pos, Flag::MobDamage, None)
            }
            // Hitting hostile mobs is always fine; animals, frames and stands count as building.
            (None, Some(attacker)) => {
                !target.get_entity().entity_type.is_enemy
                    && !player_allowed(shared, server, &attacker, target_pos, Flag::Build)
            }
            (None, None) => false,
        };
        if denied {
            event.cancelled = true;
        }
    }
);

handler!(SpawnHandler, EntitySpawnEvent, |shared, _server, event| {
    let is_mob = event
        .entity_type
        .parse::<u16>()
        .ok()
        .and_then(EntityType::from_raw)
        .is_some_and(|entity_type| entity_type.mob);
    if is_mob
        && !shared.regions().allows(
            event.world.get_world_name(),
            floored(event.position),
            Flag::MobSpawning,
            None,
        )
    {
        event.cancelled = true;
    }
});

handler!(
    EntityExplodeHandler,
    EntityExplodeEvent,
    |shared, server, event| {
        // PEX fires this event with `entity_id` 0 and no world, so check every world.
        let center = floored(event.position);
        let denied = server
            .worlds
            .load()
            .iter()
            .any(|world| explosion_denied(shared, world.get_world_name(), center));
        if denied {
            event.cancelled = true;
        }
    }
);

handler!(
    BlockExplodeHandler,
    BlockExplodeEvent,
    |shared, server, event| {
        let center = pos(&event.block_pos);
        if server
            .worlds
            .load()
            .iter()
            .any(|world| explosion_denied(shared, world.get_world_name(), center))
        {
            event.cancelled = true;
        }
    }
);

handler!(SpreadHandler, BlockSpreadEvent, |shared, _server, event| {
    let name = Block::from_state_id(event.new_state_id).name;
    if (name == "fire" || name == "soul_fire")
        && !shared.regions().allows(
            event.world.get_world_name(),
            pos(&event.target_pos),
            Flag::FireSpread,
            None,
        )
    {
        event.cancelled = true;
    }
});

handler!(BurnHandler, BlockBurnEvent, |shared, _server, event| {
    if !shared.regions().allows(
        event.world.get_world_name(),
        pos(&event.block_pos),
        Flag::FireSpread,
        None,
    ) {
        event.cancelled = true;
    }
});

handler!(IgniteHandler, BlockIgniteEvent, |shared, server, event| {
    let at = pos(&event.block_pos);
    let denied = event.player.clone().map_or_else(
        || any_world_denies(shared, server, at, Flag::FireSpread),
        |player| !player_allowed(shared, server, &player, at, Flag::Build),
    );
    if denied {
        event.cancelled = true;
    }
});

handler!(FlowHandler, BlockFromToEvent, |shared, server, event| {
    if any_world_denies(shared, server, pos(&event.to_pos), Flag::LiquidFlow) {
        event.cancelled = true;
    }
});

handler!(
    PistonExtendHandler,
    BlockPistonExtendEvent,
    |shared, server, event| {
        if any_world_denies(shared, server, pos(&event.block_pos), Flag::Pistons) {
            event.cancelled = true;
        }
    }
);

handler!(
    PistonRetractHandler,
    BlockPistonRetractEvent,
    |shared, server, event| {
        if any_world_denies(shared, server, pos(&event.block_pos), Flag::Pistons) {
            event.cancelled = true;
        }
    }
);

handler!(MoveHandler, PlayerMoveEvent, |shared, server, event| {
    let from = floored(event.from);
    let to = floored(event.to);
    if from == to {
        return Box::pin(async {});
    }
    let player = event.player.clone();
    let world = player.world();
    let world_name = world.get_world_name();
    let uuid = player.gameprofile.id.to_string();
    let (entry_denied, greetings, farewells) = {
        let regions = shared.regions();
        let old = regions.at(world_name, from);
        let new = regions.at(world_name, to);
        let entering = entered(&old, &new);
        let entry_denied = !entering.is_empty()
            && !regions.allows(world_name, to, Flag::Entry, Some(&uuid))
            && !player.has_permission(server, PERM_BYPASS);
        let message =
            |region: &crate::region::Region, flag: Flag| region.flags.get(flag.name()).cloned();
        let greetings: Vec<String> = entering
            .iter()
            .filter_map(|region| message(region, Flag::Greeting))
            .collect();
        let farewells: Vec<String> = entered(&new, &old)
            .iter()
            .filter_map(|region| message(region, Flag::Farewell))
            .collect();
        (entry_denied, greetings, farewells)
    };
    if entry_denied {
        event.cancelled = true;
        // PEX teleports the player back itself when a move is cancelled; a second teleport from
        // here would make the client confirm the wrong one and get kicked.
        player.send_system_message_raw(&PREFIX.error("You can't enter this region."), true);
        return Box::pin(async {});
    }
    for text in farewells.into_iter().chain(greetings) {
        player.send_system_message(&PREFIX.info(text));
    }
});
