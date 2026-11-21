//! Riding mobs from Bedrock: mounting a saddled horse or a pig by clicking it, and steering it
//! so the Java player sees it move.

use std::sync::Arc;

use pumpkin_core::entity::{
    EntityBase,
    passive::{camel::CamelEntity, horse::HorseEntity},
    player::Player,
};
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};

use super::{
    BEDROCK_NAME, BedrockBot, BedrockTestServer, Checks, HeadlessBot, bot, run_console,
    summon_next_to_java, tick, tick_until,
};

pub(super) async fn run_riding_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
) -> Result<(), String> {
    let player: Arc<Player> = server
        .server
        .get_player_by_name(BEDROCK_NAME)
        .ok_or_else(|| format!("{BEDROCK_NAME} is not on the server"))?;
    let start = bedrock.position;

    let horse = summon(server, bedrock, java, &player, &EntityType::HORSE).await?;
    let tamed = (horse.as_ref() as &dyn std::any::Any)
        .downcast_ref::<HorseEntity>()
        .ok_or("the horse is not a HorseEntity")?;
    tamed.set_tame(true);
    tamed.set_saddled(true);
    ride_and_steer(
        server,
        bedrock,
        java,
        checks,
        [
            "bedrock mounts a saddled horse",
            "java sees bedrock steer a saddled horse",
        ],
        &horse,
    )
    .await?;
    horse.get_entity().remove();
    walk_back(server, bedrock, java, start).await?;

    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with carrot_on_a_stick"),
    );
    let pig = summon(server, bedrock, java, &player, &EntityType::PIG).await?;
    pig.get_mob()
        .ok_or("the pig is not a mob")?
        .set_saddled(true);
    ride_and_steer(
        server,
        bedrock,
        java,
        checks,
        [
            "bedrock mounts a saddled pig with a carrot on a stick",
            "java sees bedrock steer a saddled pig with a carrot on a stick",
        ],
        &pig,
    )
    .await?;
    pig.get_entity().remove();
    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with air"),
    );
    walk_back(server, bedrock, java, start).await?;

    // Without the carrot on a stick the pig carries the player but goes its own way.
    let pig = summon(server, bedrock, java, &player, &EntityType::PIG).await?;
    pig.get_mob()
        .ok_or("the pig is not a mob")?
        .set_saddled(true);
    let pig_id = pig.get_entity().entity_id;
    bedrock.interact(pig_id as u64).await?;
    let rider_id = bedrock.unique_id;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a rider link from the Bedrock bot to the pig",
        |bot, _| {
            bot.ride_link(rider_id)
                .is_some_and(|(id, _)| id == i64::from(pig_id))
        },
    )
    .await?;
    let pig_start = pig.get_entity().pos.load();
    drive_east(server, bedrock, java, pig_id).await?;
    for _ in 0..5 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    let moved = pig.get_entity().pos.load().x - pig_start.x;
    checks.record(
        "bedrock can't steer a pig without a carrot on a stick",
        if moved < 1.0 {
            Ok(())
        } else {
            Err(format!(
                "the pig went {moved:.2} blocks east with the rider"
            ))
        },
    );
    bedrock.stop_riding(pig_id as u64).await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    pig.get_entity().remove();
    walk_back(server, bedrock, java, start).await?;

    run_more_mount_checks(server, bedrock, java, checks, &player, start).await?;
    run_jump_checks(server, bedrock, java, checks, &player, start).await?;
    run_minecart_check(server, bedrock, java, checks, &player, start).await
}

/// A strider with a warped fungus on a stick, a camel and a harnessed happy ghast.
async fn run_more_mount_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
    start: Vector3<f32>,
) -> Result<(), String> {
    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with warped_fungus_on_a_stick"),
    );
    let strider = summon(server, bedrock, java, player, &EntityType::STRIDER).await?;
    strider
        .get_mob()
        .ok_or("the strider is not a mob")?
        .set_saddled(true);
    ride_and_steer(
        server,
        bedrock,
        java,
        checks,
        [
            "bedrock mounts a saddled strider with a warped fungus on a stick",
            "java sees bedrock steer a saddled strider",
        ],
        &strider,
    )
    .await?;
    strider.get_entity().remove();
    walk_back(server, bedrock, java, start).await?;

    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with air"),
    );
    let camel = summon(server, bedrock, java, player, &EntityType::CAMEL).await?;
    as_camel(&camel)?.set_saddled(true);
    ride_and_steer(
        server,
        bedrock,
        java,
        checks,
        [
            "bedrock mounts a saddled camel",
            "java sees bedrock steer a saddled camel",
        ],
        &camel,
    )
    .await?;
    camel.get_entity().remove();
    walk_back(server, bedrock, java, start).await?;

    // The first click puts the harness on (it stays in a creative hand), the second rides.
    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with white_harness"),
    );
    let ghast = summon(server, bedrock, java, player, &EntityType::HAPPY_GHAST).await?;
    bedrock
        .interact(ghast.get_entity().entity_id as u64)
        .await?;
    for _ in 0..2 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    ride_and_steer(
        server,
        bedrock,
        java,
        checks,
        [
            "bedrock rides a harnessed happy ghast",
            "java sees bedrock fly a happy ghast",
        ],
        &ghast,
    )
    .await?;
    ghast.get_entity().remove();
    run_console(
        server,
        &format!("item replace entity {BEDROCK_NAME} hotbar.0 with air"),
    );
    walk_back(server, bedrock, java, start).await
}

fn as_camel(camel: &Arc<dyn EntityBase>) -> Result<&CamelEntity, String> {
    (camel.as_ref() as &dyn std::any::Any)
        .downcast_ref::<CamelEntity>()
        .ok_or_else(|| "the camel is not a CamelEntity".to_string())
}

/// Clicks the mob and waits until the Bedrock bot rides it.
async fn mount(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    vehicle: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    let vehicle_id = vehicle.get_entity().entity_id;
    let rider_id = bedrock.unique_id;
    bedrock.interact(vehicle_id as u64).await?;
    tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "a rider link from the Bedrock bot to the mount",
        |bot, _| {
            bot.ride_link(rider_id)
                .is_some_and(|(id, _)| id == i64::from(vehicle_id))
        },
    )
    .await
}

/// Holds jump for twelve ticks in the saddle and lets go, as a real client charging a jump.
async fn charge_jump(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
) -> Result<(), String> {
    bedrock.held_flags = vec![bot::INPUT_JUMPING];
    for _ in 0..12 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.held_flags.clear();
    tick(server, bedrock, Some(&mut *java)).await
}

/// A charged jump makes a horse rear and a camel dash, which both editions see.
async fn run_jump_checks(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
    start: Vector3<f32>,
) -> Result<(), String> {
    let horse = summon(server, bedrock, java, player, &EntityType::HORSE).await?;
    let tamed = (horse.as_ref() as &dyn std::any::Any)
        .downcast_ref::<HorseEntity>()
        .ok_or("the horse is not a HorseEntity")?;
    tamed.set_tame(true);
    tamed.set_saddled(true);
    mount(server, bedrock, java, &horse).await?;
    charge_jump(server, bedrock, java).await?;
    checks.record(
        "a bedrock rider's charged jump makes the horse rear",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the horse's standing flag",
            |_, _| tamed.has_flag(pumpkin_core::entity::passive::horse::FLAG_STANDING),
        )
        .await,
    );
    bedrock
        .stop_riding(horse.get_entity().entity_id as u64)
        .await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    horse.get_entity().remove();
    walk_back(server, bedrock, java, start).await?;

    let camel = summon(server, bedrock, java, player, &EntityType::CAMEL).await?;
    let saddled = as_camel(&camel)?;
    saddled.set_saddled(true);
    mount(server, bedrock, java, &camel).await?;
    charge_jump(server, bedrock, java).await?;
    checks.record(
        "a bedrock rider's charged jump makes the camel dash",
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the camel dashing with its cooldown running",
            |_, _| {
                saddled.is_dashing()
                    && saddled
                        .dash_cooldown
                        .load(std::sync::atomic::Ordering::Relaxed)
                        > 0
            },
        )
        .await,
    );
    bedrock
        .stop_riding(camel.get_entity().entity_id as u64)
        .await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    camel.get_entity().remove();
    walk_back(server, bedrock, java, start).await
}

/// Minecarts aren't predicted by the client: the held forward key pushes the cart the way the
/// rider looks, from the server.
async fn run_minecart_check(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    player: &Arc<Player>,
    start: Vector3<f32>,
) -> Result<(), String> {
    // A straight north-south track three blocks east, which a placed rail is on its own.
    let feet = player.get_entity().pos.load();
    let (x, y, z) = (
        feet.x.floor() as i32 + 3,
        feet.y.floor() as i32,
        feet.z.floor() as i32,
    );
    run_console(
        server,
        &format!("fill {x} {} {z} {x} {} {} stone", y - 1, y - 1, z + 12),
    );
    run_console(server, &format!("fill {x} {y} {z} {x} {y} {} rail", z + 12));
    run_console(
        server,
        &format!("summon minecraft:minecart {x}.5 {y} {}.5", z + 1),
    );
    let cart = tick_until(
        server,
        bedrock,
        Some(&mut *java),
        "the minecart on the server",
        |_, _| find_minecart(player).is_some(),
    )
    .await
    .and_then(|()| find_minecart(player).ok_or_else(|| "the minecart is gone".to_string()))?;
    mount(server, bedrock, java, &cart).await?;
    let cart_start = cart.get_entity().pos.load();
    // Looking south, down the track. The push is vanilla's slow one, a thousandth of a block a
    // tick per tick.
    bedrock.yaw = 0.0;
    bedrock.held_flags = vec![bot::INPUT_UP];
    for _ in 0..60 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.held_flags.clear();
    let moved = cart.get_entity().pos.load().z - cart_start.z;
    checks.record(
        "bedrock pushes a minecart with the forward key",
        if moved > 0.2 {
            Ok(())
        } else {
            Err(format!("the minecart went {moved:.2} blocks south"))
        },
    );
    bedrock
        .stop_riding(cart.get_entity().entity_id as u64)
        .await?;
    tick(server, bedrock, Some(&mut *java)).await?;
    cart.get_entity().remove();
    run_console(
        server,
        &format!("fill {x} {} {z} {x} {y} {} air", y - 1, z + 12),
    );
    walk_back(server, bedrock, java, start).await
}

fn find_minecart(player: &Arc<Player>) -> Option<Arc<dyn EntityBase>> {
    let pos = player.get_entity().pos.load();
    player
        .world()
        .entities
        .load()
        .iter()
        .find(|entity| {
            let entity = entity.get_entity();
            entity.entity_type == &EntityType::MINECART
                && entity.pos.load().squared_distance_to_vec(&pos) < 400.0
        })
        .cloned()
}

/// Moves the rider two blocks east over four ticks with `ClientPredictedVehicle`, as a real
/// client driving the vehicle with this id does.
async fn drive_east(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    vehicle_id: i32,
) -> Result<(), String> {
    bedrock.driving = Some((i64::from(vehicle_id), Vector2::new(0.0, 270.0)));
    let rider_start = bedrock.position;
    for step in 1..=4u8 {
        let position = rider_start + Vector3::new(f32::from(step) * 0.5, 0.0, 0.0);
        bedrock.send_input(position, &[]).await?;
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    bedrock.driving = None;
    Ok(())
}

async fn summon(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    player: &Arc<Player>,
    entity: &EntityType,
) -> Result<Arc<dyn EntityBase>, String> {
    let id = summon_next_to_java(server, bedrock, java, entity).await?;
    let found = player
        .world()
        .get_entity_by_id(id)
        .ok_or_else(|| format!("the {} is not in the world", entity.resource_name))?;
    Ok(found)
}

async fn walk_back(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    start: Vector3<f32>,
) -> Result<(), String> {
    bedrock.send_input(start, &[]).await?;
    for _ in 0..3 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(())
}

/// Clicks the mob as a real client does, then drives it two blocks east.
async fn ride_and_steer(
    server: &BedrockTestServer,
    bedrock: &mut BedrockBot,
    java: &mut HeadlessBot,
    checks: &mut Checks,
    [mount_check, steer_check]: [&'static str; 2],
    vehicle: &Arc<dyn EntityBase>,
) -> Result<(), String> {
    let vehicle_id = vehicle.get_entity().entity_id;
    let rider_id = bedrock.unique_id;
    bedrock.interact(vehicle_id as u64).await?;
    checks.record(
        mount_check,
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "a rider link on the Bedrock bot and the rider on the Java bot",
            |bot, java| {
                bot.ride_link(rider_id) == Some((i64::from(vehicle_id), bot::LINK_RIDER))
                    && java
                        .and_then(|java| java.passengers.get(&vehicle_id))
                        .is_some_and(|riders| riders.iter().any(|id| i64::from(*id) == rider_id))
            },
        )
        .await,
    );

    let start = vehicle.get_entity().pos.load();
    drive_east(server, bedrock, java, vehicle_id).await?;
    let target = start.x + 2.0;
    checks.record(
        steer_check,
        tick_until(
            server,
            bedrock,
            Some(&mut *java),
            "the mob two blocks on, on the Java bot",
            |_, java| {
                java.and_then(|java| java.entities.get(&vehicle_id))
                    .is_some_and(|seen| (seen.position.x - target).abs() < 0.3)
            },
        )
        .await,
    );
    bedrock.stop_riding(vehicle_id as u64).await?;
    for _ in 0..2 {
        tick(server, bedrock, Some(&mut *java)).await?;
    }
    Ok(())
}
