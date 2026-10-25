#[allow(clippy::wildcard_imports)]
use super::*;

/// Vanilla `ServerGamePacketListenerImpl.containsInvalidValues`.
const fn contains_invalid_values(position: Vector3<f64>, yaw: f32, pitch: f32) -> bool {
    position.x.is_nan()
        || position.y.is_nan()
        || position.z.is_nan()
        || !yaw.is_finite()
        || !pitch.is_finite()
}

impl JavaClient {
    /// Vanilla `ServerGamePacketListenerImpl.handleAcceptTeleportPacket`.
    pub fn handle_confirm_teleport(&self, player: &Player, confirm_teleport: &SConfirmTeleport) {
        let position = {
            let mut awaiting_teleport = player
                .awaiting_teleport
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // A confirmation of an older teleport arrives when the server teleported the player
            // again before the client answered. Only the last teleport counts.
            if confirm_teleport.teleport_id.0 != awaiting_teleport.id {
                return;
            }
            if contains_invalid_values(
                confirm_teleport.position,
                confirm_teleport.yaw,
                confirm_teleport.pitch,
            ) {
                None
            } else {
                awaiting_teleport.position.take()
            }
        };

        let Some(position) = position else {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                [],
            ));
            return;
        };
        // Vanilla then handles the position in the confirmation like a movement packet; PEX takes
        // the client's position from its next movement packet.
        player.get_entity().set_pos(position);
    }
}
