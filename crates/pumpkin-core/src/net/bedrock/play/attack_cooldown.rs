//! The Java attack cooldown, drawn for Bedrock players as a charge bar under the crosshair.
//!
//! Bedrock clients have no attack indicator, so after a hit the server draws one in the title
//! subtitle slot, as Geyser does by default: ten dots that fill as the attack charges.

use std::sync::atomic::Ordering;

use pumpkin_data::attributes::Attributes;
use pumpkin_protocol::bedrock::client::{TitleType, set_title::CSetTitle};
use pumpkin_util::GameMode;

use crate::{entity::player::Player, net::bedrock::BedrockClient};

/// Dots in the charge bar.
const BAR_SEGMENTS: usize = 10;
/// An attack speed above this means a plugin has turned the cooldown off, as Geyser assumes.
const MAX_SHOWN_ATTACK_SPEED: f64 = 20.0;
/// Long enough that the bar never fades out between two redraws.
const BAR_STAY_TICKS: i32 = 1000;

impl BedrockClient {
    /// Draws the charge bar from the next tick on, until the attack is charged again.
    pub fn show_attack_cooldown(&self) {
        self.attack_cooldown_shown.store(true, Ordering::Relaxed);
    }

    /// Redraws the charge bar each player tick, and clears it once the attack is charged.
    pub fn tick_attack_cooldown(&self, player: &Player) {
        if !self.attack_cooldown_shown.load(Ordering::Relaxed) {
            return;
        }
        let attack_speed = player
            .living_entity
            .get_attribute_value(&Attributes::ATTACK_SPEED);
        let progress = player.get_attack_cooldown_progress(0.0, attack_speed);
        if progress >= 1.0
            || attack_speed <= 0.0
            || attack_speed > MAX_SHOWN_ATTACK_SPEED
            || player.gamemode.load() == GameMode::Spectator
        {
            self.clear_attack_cooldown();
            return;
        }
        if !self.attack_cooldown_drawn.swap(true, Ordering::Relaxed) {
            self.send_title(TitleType::Times, String::new(), (0, BAR_STAY_TICKS, 0));
        }
        // The client shows no subtitle without a title.
        self.send_title(TitleType::Title, " ".to_string(), (0, 0, 0));
        self.send_title(TitleType::Subtitle, charge_bar(progress), (0, 0, 0));
    }

    /// Takes the bar down and puts back the title times last set for this player.
    fn clear_attack_cooldown(&self) {
        self.attack_cooldown_shown.store(false, Ordering::Relaxed);
        if self.attack_cooldown_drawn.swap(false, Ordering::Relaxed) {
            self.send_title(TitleType::Clear, String::new(), (0, 0, 0));
            self.send_title(TitleType::Times, String::new(), self.title_times.load());
        }
    }

    fn send_title(&self, title_type: TitleType, text: String, times: (i32, i32, i32)) {
        let (fade_in, stay, fade_out) = times;
        self.try_enqueue_client_packet(&CSetTitle::new(title_type, text, fade_in, stay, fade_out));
    }
}

/// Dark gray dots for the charge so far, then gray dots for the rest.
fn charge_bar(progress: f64) -> String {
    let charged = ((progress.clamp(0.0, 1.0) * BAR_SEGMENTS as f64) as usize).min(BAR_SEGMENTS);
    format!(
        "§8{}§7{}",
        "˙".repeat(charged),
        "˙".repeat(BAR_SEGMENTS - charged)
    )
}

#[cfg(test)]
mod tests {
    use super::charge_bar;

    #[test]
    fn charge_bar_fills_with_progress() {
        assert_eq!(charge_bar(0.0), "§8§7˙˙˙˙˙˙˙˙˙˙");
        assert_eq!(charge_bar(0.55), "§8˙˙˙˙˙§7˙˙˙˙˙");
        assert_eq!(charge_bar(1.0), "§8˙˙˙˙˙˙˙˙˙˙§7");
    }
}
