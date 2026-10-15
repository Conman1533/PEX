use std::sync::{Arc, Mutex};

use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::{create_generic_9x3, create_hopper};
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;

use crate::entity::vehicle::container::VehicleInventory;
use crate::entity::{Entity, player::Player};

struct MinecartScreenFactory {
    inventory: Arc<VehicleInventory>,
    title: TextComponent,
    hopper: bool,
}

impl ScreenHandlerFactory for MinecartScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        if self.inventory.has_loot_table() && player.is_spectator() {
            return None;
        }
        let inventory: Arc<dyn Inventory> = self.inventory.clone();
        let handler = if self.hopper {
            create_hopper(sync_id, player_inventory, inventory, player)
        } else {
            create_generic_9x3(sync_id, player_inventory, inventory, player)
        };
        Some(Arc::new(Mutex::new(handler)) as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        self.title.clone()
    }
}

pub(super) fn open(
    custom_name: Option<TextComponent>,
    player: &Arc<Player>,
    inventory: &Arc<VehicleInventory>,
    title: TextComponent,
    hopper: bool,
) -> bool {
    if !player.is_spectator() {
        inventory.unpack_loot();
    }

    player
        .open_handled_screen(
            &MinecartScreenFactory {
                inventory: inventory.clone(),
                title: custom_name.unwrap_or(title),
                hopper,
            },
            None,
        )
        .is_some()
}

pub(super) fn velocity(
    entity: &Entity,
    inventory: &VehicleInventory,
    velocity: Vector3<f64>,
) -> Vector3<f64> {
    let has_loot = inventory
        .loot_table
        .try_lock()
        .is_ok_and(|guard| guard.is_some());
    let signal = if has_loot {
        0
    } else if let Ok(items) = inventory.items.try_read() {
        let mut total_fill = 0.0;
        let mut has_items = false;
        for stack in items.iter() {
            if !stack.is_empty() {
                let max_count = stack.get_max_stack_size();
                total_fill += f64::from(stack.item_count) / f64::from(max_count);
                has_items = true;
            }
        }
        if has_items {
            let factor = total_fill / inventory.size as f64;
            (factor * 14.0).floor() as u8 + 1
        } else {
            0
        }
    } else {
        0
    };
    // Vanilla `AbstractMinecartContainer.applyNaturalSlowdown`, in `float`.
    let mut keep = 0.98f32;
    if !has_loot {
        keep += f32::from(15 - signal) * 0.001;
    }
    if entity
        .touching_water
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        keep *= 0.95;
    }
    let keep = f64::from(keep);
    velocity.multiply(keep, 0.0, keep)
}
