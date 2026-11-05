use std::{
    f64::consts::TAU,
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{Entity, EntityBase, passive::pig::PigEntity},
    server::Server,
};
use pumpkin_data::{attributes::Attributes, entity::EntityType, item::Item, item_stack::ItemStack};
use pumpkin_protocol::java::client::play::{
    CEntityVelocity, CHeadRot, CSetEntityMetadata, CSpawnEntity,
};
use pumpkin_util::{Hand, math::vector3::Vector3};

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// Ticks the pig is watched for.
const WATCH_TICKS: u32 = 300;
/// Vanilla `EntityType.PIG` `updateInterval(3)`.
const PIG_UPDATE_INTERVAL: u32 = 3;
/// The bot circles its start position at this radius, a little more than 1/40 of a turn per
/// tick, so the tempted pig keeps turning its head after it.
const CIRCLE_RADIUS: f64 = 2.0;
const CIRCLE_STEP: f64 = TAU / 40.0;

/// Vanilla `ServerEntity.sendChanges` sends `ClientboundRotateHeadPacket` only on update ticks
/// (every 3 for a pig) or when metadata or velocity forces an update. Pumpkin used to send one on
/// every tick the head turned.
pub async fn run_mob_head_rot_interval_test(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let player = server
        .get_player_by_name(&bot.name)
        .ok_or("Could not find the bot for the mob head rotation test")?;
    let world = player.world();
    let center = bot.position;
    let pig = PigEntity::new(Entity::new(
        world.clone(),
        center + Vector3::new(3.0, 0.0, 0.0),
        &EntityType::PIG,
    ));
    let pig_id = pig.get_entity().entity_id;
    // The pig keeps its AI but cannot walk, so it turns its head after the bot without moving.
    // A walking pig gets pushed and sends velocity, and each velocity packet allows one more head
    // rotation, which would hide the per-tick head rotations this test is about.
    pig.get_living_entity()
        .ok_or("Mob head rotation: the pig has no living entity")?
        .set_attribute_base(&Attributes::MOVEMENT_SPEED, 0.0);
    // A carrot tempts the pig, which then looks at the bot on every tick.
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(1, &Item::CARROT));
    if !world.spawn_entity(pig.clone()) {
        return Err("Mob head rotation: could not spawn the pig".to_string());
    }

    let result = watch_pig(server, bot, center, pig_id).await;
    pig.get_entity().remove();
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::EMPTY.clone());
    bot.move_to(center, true).await?;
    let (ticks, head, metadata, velocity) = result?;

    let allowed = ticks.div_ceil(PIG_UPDATE_INTERVAL) as usize + metadata + velocity;
    if head > allowed {
        return Err(format!(
            "Mob head rotation: the bot got {head} head rotations for the pig in {ticks} ticks, \
             with {metadata} metadata and {velocity} velocity packets; expected at most {allowed}"
        ));
    }
    summary_lines.push(format!(
        "[PASS] bot:mob_head_rot_interval ({head} head rotations, {metadata} metadata, \
         {velocity} velocity in {ticks} ticks, {}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// Returns the ticks counted and the head rotation, metadata and velocity packets the bot got for
/// the pig in them.
async fn watch_pig(
    server: &Arc<Server>,
    bot: &mut HeadlessBot,
    center: Vector3<f64>,
    pig_id: i32,
) -> Result<(u32, usize, usize, usize), String> {
    bot.entity_packets.clear();
    let mut spawned = false;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        bot.poll_packets().await?;
        if bot.entity_packet_count::<CSpawnEntity>(pig_id) > 0 {
            spawned = true;
            break;
        }
    }
    if !spawned {
        return Err("Mob head rotation: the bot never saw the pig spawn".to_string());
    }

    // The pairing sends its own head rotation, so count from here.
    bot.entity_packets.clear();
    let mut angle = 0.0f64;
    for _ in 0..WATCH_TICKS {
        angle += CIRCLE_STEP;
        let target = center + Vector3::new(angle.cos(), 0.0, angle.sin()) * CIRCLE_RADIUS;
        bot.move_to(target, true).await?;
        server.tick();
        tokio::time::sleep(Duration::from_millis(5)).await;
        bot.poll_packets().await?;
    }
    // The server answers the ping on the next tick, after everything the earlier ticks sent.
    // Packets from that tick may arrive too, so it counts.
    let ping = bot.send_ping().await?;
    let mut ticks = WATCH_TICKS;
    for _ in 0..MAX_POLL_TICKS {
        server.tick();
        ticks += 1;
        tokio::time::sleep(Duration::from_millis(5)).await;
        bot.poll_packets().await?;
        if bot.has_pong(ping) {
            return Ok((
                ticks,
                bot.entity_packet_count::<CHeadRot>(pig_id),
                bot.entity_packet_count::<CSetEntityMetadata>(pig_id),
                bot.entity_packet_count::<CEntityVelocity>(pig_id),
            ));
        }
    }
    Err("Mob head rotation: the server did not answer a ping".to_string())
}
