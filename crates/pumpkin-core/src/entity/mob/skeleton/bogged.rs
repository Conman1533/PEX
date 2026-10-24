use crate::entity::{
    Entity, EntityBase,
    item::ItemEntity,
    mob::{Mob, MobEntity, skeleton::SkeletonEntityBase},
};
use crate::world::loot::{LootContextParameters, generate_loot_from_handle};
use pumpkin_data::data_component_impl::{EquipmentSlot, PotionContentsImpl, StatusEffectInstance};
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use rand::RngExt;
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct BoggedSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
    sheared: AtomicBool,
}

impl BoggedSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        entity.set_synced_data(pumpkin_data::tracked_data::bogged::DATA_SHEARED, false);
        let entity = SkeletonEntityBase::with_attack_intervals(
            entity,
            SkeletonEntityBase::INCREASED_HARD_ATTACK_INTERVAL,
            SkeletonEntityBase::INCREASED_NORMAL_ATTACK_INTERVAL,
        );
        let bogged = Self {
            entity,
            sheared: AtomicBool::new(false),
        };
        Arc::new(bogged)
    }

    pub fn is_sheared(&self) -> bool {
        self.sheared.load(Ordering::Relaxed)
    }

    fn ready_for_shearing(&self) -> bool {
        !self.is_sheared() && self.get_entity().is_alive()
    }

    /// Vanilla `Bogged.shear`: drops the `shearing/bogged` loot.
    fn shear(&self, category: SoundCategory, tool: &ItemStack) {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        world.play_sound(Sound::EntityBoggedShear, category, &pos);
        if let Some(table) = world.get_loot_table("minecraft:shearing/bogged") {
            let params = LootContextParameters {
                world: Some(&world),
                position: Some(pos),
                this_entity: Some(self),
                tool: Some(tool.clone()),
                ..Default::default()
            };
            let seed = self.get_random().random();
            for stack in generate_loot_from_handle(&table, seed, &params) {
                world.spawn_entity(Arc::new(ItemEntity::new(
                    Entity::new(world.clone(), pos.add_raw(0.0, 1.0, 0.0), &EntityType::ITEM),
                    stack,
                )));
            }
        }
        self.set_sheared(true);
    }

    pub fn set_sheared(&self, sheared: bool) {
        let old = self.sheared.swap(sheared, Ordering::Relaxed);
        if old != sheared {
            self.entity
                .mob_entity
                .living_entity
                .entity
                .set_synced_data(pumpkin_data::tracked_data::bogged::DATA_SHEARED, sheared);
        }
    }
}

impl Mob for BoggedSkeletonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("sheared", self.is_sheared());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.entity.mob_read_nbt(nbt);
        if let Some(sheared) = nbt.get_bool("sheared") {
            self.set_sheared(sheared);
        }
    }

    fn mob_tick(&self, caller: &dyn EntityBase) {
        self.entity.mob_tick(caller);
    }

    fn on_equip_item(&self, slot: &EquipmentSlot) {
        self.entity.on_equip_item(slot);
    }

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntityBoggedAmbient)
    }

    /// Vanilla `Bogged.getArrow`: every arrow carries 5 seconds of poison.
    fn get_arrow_projectile(&self) -> Option<ItemStack> {
        // A plain arrow, so the tipped-arrow duration scale does not shorten the poison.
        let mut arrow = ItemStack::new(1, &Item::ARROW);
        arrow.set_data_component(PotionContentsImpl {
            potion_id: None,
            custom_color: None,
            custom_effects: vec![StatusEffectInstance {
                effect_id: Cow::Borrowed("minecraft:poison"),
                amplifier: 0,
                duration: 100,
                ambient: false,
                show_particles: true,
                show_icon: true,
            }],
            custom_name: None,
        });
        Some(arrow)
    }

    /// Vanilla `Bogged.mobInteract`.
    fn mob_interact(
        &self,
        player: &Arc<crate::entity::player::Player>,
        item_stack: &mut ItemStack,
    ) -> bool {
        if item_stack.get_item() != &Item::SHEARS || !self.ready_for_shearing() {
            return self
                .entity
                .mob_entity
                .mob_interact(player, item_stack, self.can_be_leashed());
        }
        let entity = self.get_entity();
        let world = entity.world.load();
        if let Some(server) = world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::player::player_shear_entity::PlayerShearEntityEvent {
                    player: player.clone(),
                    entity_id: entity.entity_id,
                    hand: 0,
                    cancelled: false,
                };
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return false;
            }
        }
        self.shear(SoundCategory::Players, item_stack);
        world.emit_game_event_with_context(
            GameEvent::Shear.name(),
            entity.pos.load(),
            &crate::world::game_event::GameEventContext {
                source_entity: Some(player.as_ref()),
            },
        );
        let _ = player.damage_item_stack(item_stack, 1);
        true
    }
}
