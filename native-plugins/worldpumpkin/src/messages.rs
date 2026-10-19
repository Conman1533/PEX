// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::RGBColor;

const BRAND: &str = "WORLDPUMPKIN";
const BRAND_START: RGBColor = RGBColor::new(255, 104, 24);
const BRAND_END: RGBColor = RGBColor::new(255, 190, 64);

#[derive(Clone, Copy)]
pub enum MessageKind {
    Info,
    Error,
}

/// Builds a complete WorldPumpkin-prefixed message for the requested message kind.
pub fn prefixed(kind: MessageKind, message: &str) -> TextComponent {
    let root = append_brand_gradient(TextComponent::text(""));
    let root = append_colored(root, " › ", RGBColor::new(255, 156, 42), true);
    append_colored(root, message, body_color(kind), false)
}

/// Returns `root` with the bold, gradient-colored brand name appended.
pub fn append_brand_gradient(mut root: TextComponent) -> TextComponent {
    let max_index = BRAND.chars().count().saturating_sub(1).max(1);
    for (index, character) in BRAND.chars().enumerate() {
        let color = lerp(BRAND_START, BRAND_END, index, max_index);
        root = append_colored(root, &character.to_string(), color, true);
    }
    root
}

fn append_colored(root: TextComponent, text: &str, color: RGBColor, bold: bool) -> TextComponent {
    let child = TextComponent::text(text.to_owned()).color_rgb(color);
    root.add_child(if bold { child.bold() } else { child })
}

fn lerp(start: RGBColor, end: RGBColor, pos: usize, max: usize) -> RGBColor {
    RGBColor::new(
        lerp_channel(start.red, end.red, pos, max),
        lerp_channel(start.green, end.green, pos, max),
        lerp_channel(start.blue, end.blue, pos, max),
    )
}

fn lerp_channel(start: u8, end: u8, pos: usize, max: usize) -> u8 {
    let start = i32::from(start);
    let end = i32::from(end);
    (start + ((end - start) * pos as i32 / max as i32)) as u8
}

const fn body_color(kind: MessageKind) -> RGBColor {
    match kind {
        MessageKind::Info => RGBColor::new(238, 238, 238),
        MessageKind::Error => RGBColor::new(255, 105, 97),
    }
}
