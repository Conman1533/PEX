use std::{sync::Arc, time::Instant};

use pumpkin_core::world::BlockFlags;
use pumpkin_core::{
    block::entities::{BlockEntity, mob_spawner::MobSpawnerBlockEntity},
    command::CommandSender,
    entity::EntityBase,
    server::Server,
    world::World,
};
use pumpkin_data::{Block, entity::EntityType};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    Difficulty,
    math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3},
};

use crate::bot::HeadlessBot;

/// The name the dark room's spawner puts in its `SpawnData`.
const SPAWNED_NAME: &str = "Spawned";
/// Ticks each phase waits. A spawner with `Delay` 0 tries 4 spots every tick until one works.
const PEACEFUL_TICKS: usize = 100;
const NORMAL_TICKS: usize = 400;

/// One sealed stone room under the bot, with a spawner at `center`.
struct Room {
    center: BlockPos,
    entity: &'static EntityType,
    torch: bool,
    custom_name: bool,
}

impl Room {
    /// The air inside the room, as a box for entity searches.
    fn interior(&self) -> BoundingBox {
        let c = self.center.0;
        BoundingBox::new(
            Vector3::new(f64::from(c.x - 4), f64::from(c.y - 1), f64::from(c.z - 4)),
            Vector3::new(f64::from(c.x + 5), f64::from(c.y + 3), f64::from(c.z + 5)),
        )
    }

    /// Four torches on the floor, so every spot the spawner can pick is lit.
    fn torch_positions(&self) -> [BlockPos; 4] {
        let c = self.center.0;
        [(2, 2), (2, -2), (-2, 2), (-2, -2)]
            .map(|(dx, dz)| BlockPos::new(c.x + dx, c.y - 1, c.z + dz))
    }

    /// Mobs of the room's type inside it, and how many of them carry the `SpawnData` name.
    fn count(&self, world: &Arc<World>) -> (usize, usize) {
        let mobs: Vec<_> = world
            .get_entities_at_box(&self.interior())
            .into_iter()
            .filter(|entity| entity.get_entity().entity_type.id == self.entity.id)
            .collect();
        let named = mobs
            .iter()
            .filter(|mob| {
                mob.get_entity()
                    .custom_name
                    .load()
                    .as_ref()
                    .as_ref()
                    .is_some_and(|name| name.clone().get_text() == SPAWNED_NAME)
            })
            .count();
        (mobs.len(), named)
    }
}

/// Vanilla `BaseSpawner.serverTick` with `Monster.checkMonsterSpawnRules`: only trial spawners
/// skip the light check, so a zombie spawner by a torch spawns nothing; nothing hostile spawns on
/// Peaceful; the `SpawnData` `entity` NBT reaches the mob; and a blaze (`checkAnyLightMonsterSpawnRules`)
/// still spawns in a lit room.
pub async fn run_spawner_tests(
    server: &Arc<Server>,
    bot: &HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the spawner test")?;
    let world = player.world();
    let feet = player.get_entity().block_pos.load().0;
    // Within the spawners' 16 block RequiredPlayerRange of the bot, and under the surface so no
    // sky light reaches in.
    let rooms = [
        Room {
            center: BlockPos::new(feet.x - 10, feet.y - 10, feet.z),
            entity: &EntityType::ZOMBIE,
            torch: true,
            custom_name: false,
        },
        Room {
            center: BlockPos::new(feet.x, feet.y - 10, feet.z),
            entity: &EntityType::ZOMBIE,
            torch: false,
            custom_name: true,
        },
        Room {
            center: BlockPos::new(feet.x + 10, feet.y - 10, feet.z),
            entity: &EntityType::BLAZE,
            torch: true,
            custom_name: false,
        },
    ];
    let original_difficulty = world.level_info.load().difficulty;

    let result = run_rooms(server, &world, &rooms).await;

    for room in &rooms {
        world.remove_block_entity(&room.center);
        world.set_block_state(
            &room.center,
            Block::STONE.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        for entity in world.get_entities_at_box(&room.interior()) {
            entity.get_entity().remove();
        }
    }
    server.set_difficulty(original_difficulty, true);
    result.map_err(|err| format!("Spawners: {err}"))?;

    summary_lines.push(format!(
        "[PASS] bot:spawner_rules_and_spawn_data ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Seals each room in stone, lights the torch rooms and checks the light has settled.
async fn build_rooms(
    server: &Arc<Server>,
    world: &Arc<World>,
    rooms: &[Room],
) -> Result<(), String> {
    for room in rooms {
        let c = room.center.0;
        console(
            server,
            &format!(
                "fill {} {} {} {} {} {} stone",
                c.x - 5,
                c.y - 2,
                c.z - 5,
                c.x + 5,
                c.y + 3,
                c.z + 5
            ),
        )?;
    }
    for room in rooms {
        let c = room.center.0;
        console(
            server,
            &format!(
                "fill {} {} {} {} {} {} air",
                c.x - 4,
                c.y - 1,
                c.z - 4,
                c.x + 4,
                c.y + 2,
                c.z + 4
            ),
        )?;
        if room.torch {
            for torch in room.torch_positions() {
                let t = torch.0;
                console(server, &format!("setblock {} {} {} torch", t.x, t.y, t.z))?;
            }
        }
    }
    // Lighting runs off the tick; let it settle before checking it.
    for _ in 0..40 {
        server.tick();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    for room in rooms {
        let c = room.center.0;
        let probe = BlockPos::new(c.x - 3, c.y - 1, c.z - 3);
        let block = world.get_block_light_level(&probe).unwrap_or(0);
        let sky = world.get_sky_light_level(&probe);
        if sky != 0 || (room.torch == (block == 0)) {
            return Err(format!(
                "the {} room's light at {probe:?} is block {block}, sky {sky}; expected sky 0 and \
                 block light {}",
                room.entity.resource_name,
                if room.torch { "above 0" } else { "0" }
            ));
        }
    }

    Ok(())
}

async fn run_rooms(server: &Arc<Server>, world: &Arc<World>, rooms: &[Room]) -> Result<(), String> {
    build_rooms(server, world, rooms).await?;

    for room in rooms {
        let mut entity = NbtCompound::new();
        entity.put_string("id", format!("minecraft:{}", room.entity.resource_name));
        if room.custom_name {
            entity.put_string("CustomName", SPAWNED_NAME.to_string());
        }
        let mut spawn_data = NbtCompound::new();
        spawn_data.put_compound("entity", entity);
        let mut nbt = NbtCompound::new();
        nbt.put_short("Delay", 0);
        nbt.put_compound("SpawnData", spawn_data);
        world.set_block_state(
            &room.center,
            Block::SPAWNER.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        world.add_block_entity(Arc::new(MobSpawnerBlockEntity::from_nbt(&nbt, room.center)));
    }

    server.set_difficulty(Difficulty::Peaceful, true);
    for _ in 0..PEACEFUL_TICKS {
        server.tick();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    for room in rooms {
        let (count, _) = room.count(world);
        if count != 0 {
            return Err(format!(
                "{count} {} spawned on Peaceful",
                room.entity.resource_name
            ));
        }
    }

    server.set_difficulty(Difficulty::Normal, true);
    let [lit_zombies, dark_zombies, blazes] = rooms else {
        return Err("expected three rooms".to_string());
    };
    for _ in 0..NORMAL_TICKS {
        server.tick();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        if dark_zombies.count(world).1 > 0 && blazes.count(world).0 > 0 {
            break;
        }
    }
    let (lit, _) = lit_zombies.count(world);
    let (dark, named) = dark_zombies.count(world);
    let (blaze, _) = blazes.count(world);
    if lit != 0 {
        return Err(format!(
            "the zombie spawner next to a torch spawned {lit} zombies"
        ));
    }
    if named == 0 {
        return Err(format!(
            "the dark room has {dark} zombies, none named '{SPAWNED_NAME}' from its SpawnData"
        ));
    }
    if blaze == 0 {
        return Err("the blaze spawner in the lit room spawned nothing".to_string());
    }
    Ok(())
}

fn console(server: &Arc<Server>, command: &str) -> Result<(), String> {
    server
        .command_dispatcher
        .load()
        .execute_input(command, &CommandSender::Console.into_source(server))
        .map(|_| ())
        .map_err(|err| format!("'{command}' failed: {}", err.message.get_text()))
}
