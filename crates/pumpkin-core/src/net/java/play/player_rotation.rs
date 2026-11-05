#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub fn handle_rotation(&self, player: &Player, rotation: &SPlayerRotation) {
        if !player.has_client_loaded() {
            return;
        }
        // A movement packet was received this tick — tracked for SClientTickEnd zeroing.
        self.received_movement_this_tick
            .store(true, Ordering::Relaxed);
        if !rotation.yaw.is_finite() || !rotation.pitch.is_finite() {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                [],
            ));
            return;
        }
        let entity = &player.get_entity();
        // Vanilla `handleMovePlayer` only takes the rotation until the client confirms the last
        // teleport.
        if !self.update_awaiting_teleport(player) {
            entity.on_ground.store(rotation.ground, Ordering::Relaxed);
        }
        entity.set_rotation(
            wrap_degrees(rotation.yaw) % 360.0,
            wrap_degrees(rotation.pitch),
        );
        // Bedrock watchers only; the entity tracker sends Java watchers the rotation and head
        // rotation (`TrackedEntity::send_changes`).
        let world = entity.world.load_full();
        let pos = entity.pos.load();

        // MODE_ROTATION not used for other players -> AvatarEntity always sends
        // MODE_NORMAL (client already lerps). MODE_ROTATION drops live head yaw on Bedrock.
        let be_packet =
            bedrock_move_player_packet(entity, pos, CMovePlayer::MODE_NORMAL, rotation.ground);

        world.send_to_tracking_players_bedrock(entity, &be_packet);
    }
}
