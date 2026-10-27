#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_data::data_component_impl::PiercingWeaponImpl;
use pumpkin_data::entity::EntityType;
use pumpkin_data::tag::{self, Taggable};

/// Ticks of attack strength vanilla `ServerGamePacketListenerImpl.handleAttack` adds before
/// checking an item's `minimum_attack_charge` (`cannotAttackWithItem(mainHandItem, 5)`).
pub(super) const ATTACK_CHARGE_TOLERANCE_TICKS: u32 = 5;

/// The vanilla `AbstractArrow` subclasses.
const ABSTRACT_ARROWS: [&EntityType; 3] = [
    &EntityType::ARROW,
    &EntityType::SPECTRAL_ARROW,
    &EntityType::TRIDENT,
];

impl JavaClient {
    /// Vanilla `ServerGamePacketListenerImpl.handleAttack`.
    pub fn handle_attack(&self, player: &Arc<Player>, attack: &SAttack, server: &Arc<Server>) {
        if !player.has_client_loaded() || player.is_spectator() {
            return;
        }
        player.update_last_action_time();
        // A target that died or despawned while the packet was on its way is ignored, not kicked.
        let world = player.world();
        let Some(target) = world.get_entity_by_id(attack.entity_id.0) else {
            return;
        };
        if !player.can_attack_entity(target.get_entity()) {
            return;
        }
        let main_hand_item = player.inventory().held_item();
        // Piercing weapons hit through the `STAB` player action instead.
        if main_hand_item
            .get_data_component::<PiercingWeaponImpl>()
            .is_some()
        {
            return;
        }
        if is_invalid_attack_target(player, target.get_entity()) {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_ENTITY_ATTACKED,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_ENTITY_ATTACKED,
                [],
            ));
            warn!(
                "Player {} tried to attack an invalid entity",
                player.gameprofile.name
            );
            return;
        }
        if player.cannot_attack_with_item(&main_hand_item, ATTACK_CHARGE_TOLERANCE_TICKS) {
            return;
        }

        let config = &server.advanced_config.pvp;
        if !config.enabled {
            return;
        }
        if let Some(player_victim) = target.get_player() {
            if player_victim.living_entity.health.load() <= 0.0 {
                return;
            }
            if config.protect_creative && player_victim.gamemode.load() == GameMode::Creative {
                world.play_sound(
                    Sound::EntityPlayerAttackNodamage,
                    SoundCategory::Players,
                    &player_victim.position(),
                );
                return;
            }
        }
        player.attack(&target);
    }
}

/// Vanilla `handleAttack` disconnects a player who attacks an item, an experience orb, itself, or
/// an arrow that `AbstractArrow.isAttackable` refuses.
fn is_invalid_attack_target(player: &Player, target: &Entity) -> bool {
    let entity_type = target.entity_type;
    entity_type == &EntityType::ITEM
        || entity_type == &EntityType::EXPERIENCE_ORB
        || target.entity_id == player.entity_id()
        || (ABSTRACT_ARROWS.contains(&entity_type)
            && !entity_type.has_tag(&tag::EntityType::MINECRAFT_REDIRECTABLE_PROJECTILE))
}
