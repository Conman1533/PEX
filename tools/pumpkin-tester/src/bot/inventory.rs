//! The bot's copy of its inventory and open menu, kept from server packets the way vanilla
//! `ClientPacketListener` applies them.

use pumpkin_data::{item::Item, item_stack::ItemStack};

/// Vanilla `Inventory.getContainerSize()`: 36 main slots, then feet, legs, chest, head,
/// offhand, body armor and saddle.
const INVENTORY_CONTAINER_SIZE: usize = 43;
/// Vanilla `Inventory.INVENTORY_SIZE`.
const INVENTORY_SIZE: usize = 36;
/// Vanilla `Inventory.SELECTION_SIZE`.
const SELECTION_SIZE: usize = 9;
/// `Inventory` index of the head slot, the first armor slot `InventoryMenu` shows.
const SLOT_HEAD: usize = 39;
/// Vanilla `Inventory.SLOT_OFFHAND`.
const SLOT_OFFHAND: usize = 40;

/// Vanilla `InventoryMenu` slot constants.
pub mod inventory_menu {
    pub const RESULT_SLOT: usize = 0;
    pub const CRAFT_SLOT_START: usize = 1;
    pub const CRAFT_SLOT_END: usize = 5;
    pub const ARMOR_SLOT_END: usize = 9;
    pub const USE_ROW_SLOT_START: usize = 36;
    pub const SHIELD_SLOT: usize = 45;
}

/// Vanilla `CraftingMenu` slot constants.
pub mod crafting_menu {
    pub const RESULT_SLOT: usize = 0;
    pub const CRAFT_SLOT_START: usize = 1;
}

enum SlotRef {
    Container(usize),
    Inventory(usize),
}

/// One menu as the client sees it.
pub struct Menu {
    pub container_id: i32,
    /// Menu type id from `ClientboundOpenScreenPacket`; `None` for the player's `InventoryMenu`.
    pub window_type: Option<i32>,
    pub state_id: i32,
    /// Set once the server has sent the menu's contents.
    pub initialized: bool,
    /// Slots that don't show the player's inventory, such as a crafting result and grid.
    container_slots: Vec<ItemStack>,
}

impl Menu {
    fn inventory_menu() -> Self {
        Self {
            container_id: 0,
            window_type: None,
            state_id: 0,
            initialized: false,
            container_slots: vec![ItemStack::EMPTY.clone(); inventory_menu::CRAFT_SLOT_END],
        }
    }

    const fn slot_ref(&self, slot: usize) -> Option<SlotRef> {
        use inventory_menu::{ARMOR_SLOT_END, CRAFT_SLOT_END, SHIELD_SLOT, USE_ROW_SLOT_START};
        if self.window_type.is_none() {
            return match slot {
                0..CRAFT_SLOT_END => Some(SlotRef::Container(slot)),
                CRAFT_SLOT_END..ARMOR_SLOT_END => {
                    Some(SlotRef::Inventory(SLOT_HEAD - (slot - CRAFT_SLOT_END)))
                }
                ARMOR_SLOT_END..USE_ROW_SLOT_START => Some(SlotRef::Inventory(slot)),
                USE_ROW_SLOT_START..SHIELD_SLOT => {
                    Some(SlotRef::Inventory(slot - USE_ROW_SLOT_START))
                }
                SHIELD_SLOT => Some(SlotRef::Inventory(SLOT_OFFHAND)),
                _ => None,
            };
        }
        if slot < self.container_slots.len() {
            return Some(SlotRef::Container(slot));
        }
        // Every other menu ends with `addStandardInventorySlots`: 27 main slots, then the hotbar.
        let offset = slot - self.container_slots.len();
        if offset < INVENTORY_SIZE - SELECTION_SIZE {
            Some(SlotRef::Inventory(offset + SELECTION_SIZE))
        } else if offset < INVENTORY_SIZE {
            Some(SlotRef::Inventory(
                offset - (INVENTORY_SIZE - SELECTION_SIZE),
            ))
        } else {
            None
        }
    }

    /// The slot showing main inventory index `index` (below `Inventory.INVENTORY_SIZE`).
    const fn slot_of(&self, index: usize) -> usize {
        let hotbar = index < SELECTION_SIZE;
        if self.window_type.is_none() {
            if hotbar {
                inventory_menu::USE_ROW_SLOT_START + index
            } else {
                index
            }
        } else if hotbar {
            self.container_slots.len() + INVENTORY_SIZE - SELECTION_SIZE + index
        } else {
            self.container_slots.len() + index - SELECTION_SIZE
        }
    }
}

/// The player's inventory, cursor and menus as the client knows them.
pub struct ClientInventory {
    /// Vanilla `Inventory` contents by index.
    pub items: Vec<ItemStack>,
    /// The stack on the cursor (vanilla `AbstractContainerMenu.carried`).
    pub carried: ItemStack,
    /// Vanilla `Inventory.selected`.
    pub selected: usize,
    pub inventory_menu: Menu,
    /// The menu from the last `ClientboundOpenScreenPacket`, until it is closed.
    pub open_menu: Option<Menu>,
}

impl ClientInventory {
    pub fn new() -> Self {
        Self {
            items: vec![ItemStack::EMPTY.clone(); INVENTORY_CONTAINER_SIZE],
            carried: ItemStack::EMPTY.clone(),
            selected: 0,
            inventory_menu: Menu::inventory_menu(),
            open_menu: None,
        }
    }

    /// The menu on screen: the open one, or the player's `InventoryMenu`.
    pub fn menu(&self) -> &Menu {
        self.open_menu.as_ref().unwrap_or(&self.inventory_menu)
    }

    /// A slot of the menu on screen.
    pub fn slot(&self, slot: usize) -> &ItemStack {
        let menu = self.menu();
        match menu.slot_ref(slot) {
            Some(SlotRef::Container(i)) => &menu.container_slots[i],
            Some(SlotRef::Inventory(i)) => &self.items[i],
            None => ItemStack::EMPTY,
        }
    }

    /// Selects hotbar slot `slot` (vanilla `Inventory.setSelectedSlot`), ignoring slots outside
    /// the hotbar.
    pub const fn select(&mut self, slot: usize) {
        if slot < SELECTION_SIZE {
            self.selected = slot;
        }
    }

    pub fn held_item(&self) -> &ItemStack {
        &self.items[self.selected]
    }

    /// The slot of the menu on screen holding the first main inventory stack of `item`.
    pub fn find(&self, item: &Item) -> Option<usize> {
        self.items[..INVENTORY_SIZE]
            .iter()
            .position(|stack| stack.item == item)
            .map(|index| self.menu().slot_of(index))
    }

    /// The first empty hotbar slot, as an `Inventory` index.
    pub fn free_hotbar_slot(&self) -> Option<usize> {
        self.items[..SELECTION_SIZE]
            .iter()
            .position(ItemStack::is_empty)
    }

    /// The slot of the menu on screen showing hotbar slot `hotbar`.
    pub fn hotbar_slot(&self, hotbar: usize) -> usize {
        self.menu().slot_of(hotbar)
    }

    /// The hotbar slot holding `item`.
    pub fn find_in_hotbar(&self, item: &Item) -> Option<usize> {
        self.items[..SELECTION_SIZE]
            .iter()
            .position(|stack| stack.item == item)
    }

    /// How many of `item` the main inventory holds.
    pub fn count(&self, item: &Item) -> u32 {
        self.items[..INVENTORY_SIZE]
            .iter()
            .filter(|stack| stack.item == item)
            .map(|stack| u32::from(stack.item_count))
            .sum()
    }

    /// `ClientPacketListener.handleOpenScreen`.
    pub fn open(&mut self, container_id: i32, window_type: i32) {
        self.open_menu = Some(Menu {
            container_id,
            window_type: Some(window_type),
            state_id: 0,
            initialized: false,
            container_slots: Vec::new(),
        });
    }

    fn menu_mut(&mut self, container_id: i32) -> Option<&mut Menu> {
        if container_id == self.inventory_menu.container_id {
            Some(&mut self.inventory_menu)
        } else {
            self.open_menu
                .as_mut()
                .filter(|menu| menu.container_id == container_id)
        }
    }

    /// `ClientPacketListener.handleContainerContent`.
    pub fn set_contents(
        &mut self,
        container_id: i32,
        state_id: i32,
        slots: Vec<ItemStack>,
        carried: ItemStack,
    ) {
        let Some(menu) = self.menu_mut(container_id) else {
            return;
        };
        if menu.window_type.is_some() {
            menu.container_slots =
                vec![ItemStack::EMPTY.clone(); slots.len().saturating_sub(INVENTORY_SIZE)];
        }
        menu.initialized = true;
        for (slot, stack) in slots.into_iter().enumerate() {
            self.set_slot(container_id, state_id, slot, stack);
        }
        self.carried = carried;
    }

    /// `ClientPacketListener.handleContainerSetSlot`.
    pub fn set_slot(&mut self, container_id: i32, state_id: i32, slot: usize, stack: ItemStack) {
        let Some(menu) = self.menu_mut(container_id) else {
            return;
        };
        menu.state_id = state_id;
        match menu.slot_ref(slot) {
            Some(SlotRef::Container(i)) => menu.container_slots[i] = stack,
            Some(SlotRef::Inventory(i)) => self.items[i] = stack,
            None => {}
        }
    }

    /// `ClientPacketListener.handleSetPlayerInventory`.
    pub fn set_inventory_item(&mut self, index: usize, stack: ItemStack) {
        if let Some(item) = self.items.get_mut(index) {
            *item = stack;
        }
    }
}
