//! Riding mobs from Bedrock: mounting a saddled horse or a pig by clicking it, and steering it
//! so the Java player sees it move.

use std::sync::Arc;

use pumpkin_core::entity::{EntityBase, passive::horse::HorseEntity, player::Player};
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
    Ok(())
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
