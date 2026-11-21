//! Survival stages. Each stage builds a small area with console commands, then one bot plays
//! through it with only the packets a vanilla client sends (digging, using items, clicking menu
//! slots, walking over drops), and the stage checks the outcome on the server.
//!
//! # Deviations
//!
//! What the stages accept that a vanilla server wouldn't, or that a real client would do
//! differently (Goal 6's answers). Any other deviation goes here and in the commit that adds it.
//!
//! - Movement is client-authoritative: the bot sends positions without jump or fall physics, and
//!   the server takes them as they come.
//! - No anti-cheat: nothing checks reach, speed or how fast the bot clicks.
//! - Digging waits Paper's delayed-dig tick count ([`HeadlessBot::destroy_ticks`]), not vanilla's
//!   per-tick progress check.
//! - The bot aims a bow straight at its target with a fixed lift for the arrow's drop
//!   ([`StageContext::shoot_bow_at`]), so it only hits near targets.

mod wood_to_stone_pickaxe;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use pumpkin_core::{command::CommandSender, entity::player::Player, server::Server, world::World};
use pumpkin_data::{Block, entity::EntityType, item::Item};
use pumpkin_protocol::java::server::play::{SClickSlot, SlotActionType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::bot::{HeadlessBot, MAX_POLL_TICKS};

/// Real time between ticks, so packets reach the other side before the next tick.
const TICK_PAUSE: Duration = Duration::from_millis(5);
const LEAVE_TIMEOUT: Duration = Duration::from_secs(30);
/// Ticks the server gets to answer a ping. Generous, because a busy machine can take many ticks
/// to read a packet; waiting longer only lets game time pass, like a lagging client.
const MAX_SYNC_TICKS: usize = 1000;
/// Ticks the server gets to generate and send a stage area.
const MAX_TERRAIN_TICKS: usize = 6000;
/// Ticks a bot gets to find and pick up the drops it is after.
const MAX_COLLECT_TICKS: usize = 200;
/// How far from the bot a drop can be for the bot to go after it.
const COLLECT_RADIUS: f64 = 8.0;
/// Blocks a bot walks per tick, just under vanilla's walking speed.
const WALK_STEP: f64 = 0.2;
/// Ticks a bot keeps eating before giving up: vanilla food takes 32 (dried kelp 16).
const MAX_EAT_TICKS: usize = 60;
/// Vanilla `BowItem.getPowerForTime` reaches full power after 20 ticks of drawing.
const BOW_DRAW_TICKS: usize = 20;
/// Ticks between swings, so each hit lands at full strength (`attack_speed` 4 recharges in 5;
/// a slower weapon needs more) and after the target's 10 invulnerable ticks.
const ATTACK_INTERVAL_TICKS: usize = 20;
/// Ticks the server gets to move the bot to another dimension and send it chunks there. The first
/// visit generates the destination's chunks, which takes seconds while the tester sleeps 5 ms a
/// tick.
const MAX_DIMENSION_TICKS: usize = 6000;

pub async fn run_survival_stages(
    server: &Arc<Server>,
    addr: SocketAddr,
    summary_lines: &mut Vec<String>,
) -> Result<(), String> {
    let start = Instant::now();
    let mut stage = StageContext::join(server, addr, "SurvivalBot").await?;
    wood_to_stone_pickaxe::run(&mut stage)
        .await
        .map_err(|err| format!("Survival stage wood_to_stone_pickaxe failed: {err}"))?;
    stage.leave().await?;
    summary_lines.push(format!(
        "[PASS] survival:wood_to_stone_pickaxe ({}ms)",
        start.elapsed().as_millis()
    ));
    Ok(())
}

/// The server and the bot a stage drives. The server-side player is only for checking outcomes;
/// everything the stage does in the game goes through the bot's packets.
pub struct StageContext<'a> {
    pub server: &'a Arc<Server>,
    pub bot: HeadlessBot,
    pub player: Arc<Player>,
}

impl<'a> StageContext<'a> {
    pub async fn join(
        server: &'a Arc<Server>,
        addr: SocketAddr,
        name: &str,
    ) -> Result<Self, String> {
        let bot = HeadlessBot::connect_and_join(addr, name).await?;
        for _ in 0..MAX_POLL_TICKS {
            server.tick();
            tokio::time::sleep(TICK_PAUSE).await;
            if let Some(player) = server.get_player_by_name(name) {
                let mut stage = Self {
                    server,
                    bot,
                    player,
                };
                // Let the server take the join teleport's confirmation before a setup command
                // teleports the bot again.
                stage.sync().await?;
                return Ok(stage);
            }
        }
        Err(format!("Bot '{name}' never appeared on the server"))
    }

    pub async fn leave(self) -> Result<(), String> {
        let Self { server, bot, .. } = self;
        let name = bot.name.clone();
        drop(bot);
        // The server removes a player in its own tasks, which take longer than MAX_POLL_TICKS
        // ticks when the CT is busy, so also wait at least LEAVE_TIMEOUT.
        let deadline = std::time::Instant::now() + LEAVE_TIMEOUT;
        let mut ticks = 0;
        while ticks < MAX_POLL_TICKS || std::time::Instant::now() < deadline {
            server.tick();
            tokio::time::sleep(TICK_PAUSE).await;
            ticks += 1;
            if server.get_player_by_name(&name).is_none() {
                return Ok(());
            }
        }
        Err(format!(
            "Bot '{name}' stayed on the server after disconnecting"
        ))
    }

    pub fn world(&self) -> Arc<World> {
        self.player.world()
    }

    /// Runs a setup command as the server console.
    pub fn console(&self, command: &str) -> Result<(), String> {
        let source = CommandSender::Console.into_source(self.server);
        self.server
            .command_dispatcher
            .load()
            .execute_input(command, &source)
            .map(|_| ())
            .map_err(|err| {
                format!(
                    "Setup command '{command}' failed: {}",
                    err.message.get_text()
                )
            })
    }

    /// One server tick, then whatever the bot received.
    pub async fn tick(&mut self) -> Result<(), String> {
        self.server.tick();
        tokio::time::sleep(TICK_PAUSE).await;
        self.bot.poll_packets().await
    }

    /// Ticks until `done` holds for the bot.
    pub async fn tick_until(
        &mut self,
        what: &str,
        max_ticks: usize,
        done: impl Fn(&HeadlessBot) -> bool,
    ) -> Result<(), String> {
        for _ in 0..max_ticks {
            if done(&self.bot) {
                return Ok(());
            }
            self.tick().await?;
        }
        if done(&self.bot) {
            Ok(())
        } else {
            Err(format!("Waited {max_ticks} ticks for {what}"))
        }
    }

    /// Ticks until the server has handled everything the bot sent so far and the bot has read
    /// the answers.
    pub async fn sync(&mut self) -> Result<(), String> {
        let ping = self.bot.send_ping().await?;
        tokio::time::sleep(TICK_PAUSE).await;
        self.tick_until("the server to answer a ping", MAX_SYNC_TICKS, |bot| {
            bot.has_pong(ping)
        })
        .await
    }

    /// Ticks until the server has sent the chunk holding `pos`.
    pub async fn wait_for_terrain(&mut self, pos: BlockPos) -> Result<(), String> {
        let chunk = (pos.0.x >> 4, pos.0.z >> 4);
        self.tick_until(
            &format!("chunk {chunk:?} to load"),
            MAX_TERRAIN_TICKS,
            |bot| bot.chunks.contains(&chunk),
        )
        .await
    }

    /// Walks in a straight line over flat ground, one step per tick.
    pub async fn walk_to(&mut self, target: Vector3<f64>) -> Result<(), String> {
        loop {
            let offset = target - self.bot.position;
            let distance = offset.length();
            if distance < f64::EPSILON {
                return Ok(());
            }
            let step = if distance <= WALK_STEP {
                target
            } else {
                self.bot.position + offset * (WALK_STEP / distance)
            };
            self.bot.move_to(step, true).await?;
            self.tick().await?;
        }
    }

    /// Selects a hotbar slot, like scrolling to it.
    pub async fn select_hotbar_slot(&mut self, slot: usize) -> Result<(), String> {
        self.bot.set_held_item(slot as i16).await?;
        self.sync().await
    }

    /// Mines `pos`, which should hold `block`, with the held item: starts digging, then, unless the
    /// block breaks at once, waits at least as long as a vanilla client would and finishes.
    pub async fn dig(&mut self, pos: BlockPos, block: &'static Block) -> Result<(), String> {
        let ticks = self
            .bot
            .destroy_ticks(block)
            .ok_or_else(|| format!("The bot can't break {}", block.name))?;
        let sequence = self.bot.next_sequence();
        self.bot.start_digging(pos, sequence).await?;
        self.sync().await?;
        if ticks > 0 {
            for _ in 0..ticks {
                self.tick().await?;
            }
            let sequence = self.bot.next_sequence();
            self.bot.finish_digging(pos, sequence).await?;
            self.sync().await?;
        }
        let left = self.world().get_block(&pos);
        if left.id == Block::AIR.id {
            Ok(())
        } else {
            Err(format!(
                "{} at {pos} is still {} after digging it for {ticks} ticks",
                block.name, left.name
            ))
        }
    }

    /// Walks over dropped items near the bot until it holds `count` of `item`.
    pub async fn collect(&mut self, item: &'static Item, count: u32) -> Result<(), String> {
        for _ in 0..MAX_COLLECT_TICKS {
            if self.bot.inventory.count(item) >= count {
                return Ok(());
            }
            let position = self.bot.position;
            let nearest = self
                .bot
                .item_entities
                .values()
                .map(|drop| (drop.squared_distance_to_vec(&position), drop))
                .filter(|(distance, _)| *distance <= COLLECT_RADIUS * COLLECT_RADIUS)
                .min_by(|(a, _), (b, _)| a.total_cmp(b))
                .map(|(_, drop)| *drop);
            if let Some(drop) = nearest {
                self.walk_to(Vector3::new(drop.x, position.y, drop.z))
                    .await?;
            }
            self.tick().await?;
        }
        Err(format!(
            "Picked up {} of {count} {} within {MAX_COLLECT_TICKS} ticks",
            self.bot.inventory.count(item),
            item.registry_key
        ))
    }

    /// Clicks a slot of the menu on screen and waits for the server's answer.
    pub async fn click(
        &mut self,
        slot: usize,
        button: i8,
        mode: SlotActionType,
    ) -> Result<(), String> {
        self.bot.click_slot(slot, button, mode).await?;
        self.sync().await
    }

    pub async fn left_click(&mut self, slot: usize) -> Result<(), String> {
        self.click(slot, SClickSlot::BUTTON_LEFT, SlotActionType::Pickup)
            .await
    }

    pub async fn right_click(&mut self, slot: usize) -> Result<(), String> {
        self.click(slot, SClickSlot::BUTTON_RIGHT, SlotActionType::Pickup)
            .await
    }

    pub async fn shift_click(&mut self, slot: usize) -> Result<(), String> {
        self.click(slot, SClickSlot::BUTTON_LEFT, SlotActionType::QuickMove)
            .await
    }

    /// Right-clicks the top face of `pos` with the held item.
    pub async fn use_block(&mut self, pos: BlockPos) -> Result<(), String> {
        let sequence = self.bot.next_sequence();
        self.bot.use_block(pos, sequence).await?;
        self.sync().await
    }

    /// Right-clicks the block at `pos` and waits until the server has opened a menu of type
    /// `window_type` and sent its contents.
    pub async fn open_menu(&mut self, pos: BlockPos, window_type: i32) -> Result<(), String> {
        self.use_block(pos).await?;
        self.tick_until(
            &format!("menu type {window_type} to open"),
            MAX_SYNC_TICKS,
            |bot| {
                bot.inventory
                    .open_menu
                    .as_ref()
                    .is_some_and(|menu| menu.window_type == Some(window_type) && menu.initialized)
            },
        )
        .await
    }

    /// Closes the menu on screen, like pressing Escape.
    pub async fn close_menu(&mut self) -> Result<(), String> {
        self.bot.close_container().await?;
        self.sync().await
    }

    /// Eats the held food until the server raises the bot's food level. The bot can't be full.
    pub async fn eat(&mut self) -> Result<(), String> {
        self.sync().await?;
        let food = |stage: &Self| stage.player.hunger_manager.level.load();
        let before = food(self);
        let sequence = self.bot.next_sequence();
        self.bot.use_item(sequence).await?;
        // Once the ping is answered the server has the use packet, so the eat has started. The
        // server player is read because the bot's view can trail it on a slow machine.
        self.sync().await?;
        for _ in 0..MAX_EAT_TICKS {
            if food(self) > before {
                return Ok(());
            }
            self.tick().await?;
        }
        if food(self) > before {
            Ok(())
        } else {
            Err(format!(
                "Waited {MAX_EAT_TICKS} ticks for the food level to rise above {before}"
            ))
        }
    }

    /// Draws the held bow fully at `target` and lets go. Returns the arrow's entity id.
    pub async fn shoot_bow_at(&mut self, target: Vector3<f64>) -> Result<i32, String> {
        let before: Vec<i32> = self
            .bot
            .entities_of_type(&EntityType::ARROW)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        // Aim a little above the target for the drop over a short flight.
        let distance = target.squared_distance_to_vec_xz(self.bot.position).sqrt();
        let aim = Vector3::new(target.x, target.y + distance * 0.05, target.z);
        self.bot.look_at(aim).await?;
        let sequence = self.bot.next_sequence();
        self.bot.use_item(sequence).await?;
        self.sync().await?;
        for _ in 0..BOW_DRAW_TICKS {
            self.tick().await?;
        }
        self.bot.release_use_item().await?;
        self.sync().await?;
        let mut arrow = None;
        self.tick_until("the arrow to spawn", MAX_SYNC_TICKS, |bot| {
            bot.entities_of_type(&EntityType::ARROW)
                .iter()
                .any(|(id, _)| !before.contains(id))
        })
        .await?;
        for (id, _) in self.bot.entities_of_type(&EntityType::ARROW) {
            if !before.contains(&id) {
                arrow = Some(id);
            }
        }
        arrow.ok_or_else(|| "The arrow vanished".to_string())
    }

    /// Looks at the entity and hits it with the held item until the server removes it.
    pub async fn attack_until_gone(
        &mut self,
        entity_id: i32,
        max_hits: usize,
    ) -> Result<usize, String> {
        for hits in 0..max_hits {
            let Some(entity) = self.bot.entities.get(&entity_id).copied() else {
                return Ok(hits);
            };
            self.bot
                .look_at(entity.position + Vector3::new(0.0, 0.5, 0.0))
                .await?;
            self.bot.attack_entity(entity_id).await?;
            self.bot.swing().await?;
            self.sync().await?;
            for _ in 0..ATTACK_INTERVAL_TICKS {
                if !self.bot.entities.contains_key(&entity_id) {
                    return Ok(hits + 1);
                }
                self.tick().await?;
            }
        }
        if self.bot.entities.contains_key(&entity_id) {
            Err(format!(
                "Entity {entity_id} is still there after {max_hits} hits"
            ))
        } else {
            Ok(max_hits)
        }
    }

    /// Ticks until the server has moved the bot to `dimension` and sent it a chunk there.
    pub async fn wait_for_dimension(&mut self, dimension: &str) -> Result<(), String> {
        self.tick_until(
            &format!("the bot to arrive in {dimension}"),
            MAX_DIMENSION_TICKS,
            |bot| bot.dimension == dimension && !bot.chunks.is_empty(),
        )
        .await
        .map_err(|err| {
            format!(
                "{err} (the bot is in {} with {} chunks)",
                self.bot.dimension,
                self.bot.chunks.len()
            )
        })
    }
}
