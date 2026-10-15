use std::sync::{Arc, Mutex};

use pumpkin_inventory::{
    Inventory,
    generic_container_screen_handler::create_generic_9x3,
    player::player_inventory::PlayerInventory,
    screen_handler::{InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler},
};
use pumpkin_util::text::TextComponent;

use crate::entity::vehicle::container::VehicleInventory;

pub(super) struct BoatScreenFactory {
    pub(super) inventory: Arc<VehicleInventory>,
    pub(super) title: TextComponent,
}

impl ScreenHandlerFactory for BoatScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        // Vanilla `AbstractChestBoat.createMenu`: a spectator can't open unpacked loot.
        if self.inventory.has_loot_table() && player.is_spectator() {
            return None;
        }
        let inventory: Arc<dyn Inventory> = self.inventory.clone();
        let handler = create_generic_9x3(sync_id, player_inventory, inventory, player);
        Some(Arc::new(Mutex::new(handler)) as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        self.title.clone()
    }
}
