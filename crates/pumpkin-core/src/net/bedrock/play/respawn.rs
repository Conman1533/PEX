use pumpkin_protocol::bedrock::server::{RespawnState, show_credits::SShowCredits};

#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    pub fn handle_respawn(&self, player: &Arc<Player>, packet: &SRespawn) {
        if packet.state != RespawnState::ClientReadyToSpawn
            || (!player.living_entity.dead.load(Ordering::Relaxed)
                && player.living_entity.health.load() > 0.0)
        {
            return;
        }

        let entity = player.get_entity();
        let position = entity.pos.load();
        self.try_enqueue_client_packet(&SRespawn {
            position: pumpkin_util::math::vector3::Vector3::new(
                position.x as f32,
                position.y as f32 + entity.entity_type.eye_height,
                position.z as f32,
            ),
            state: RespawnState::ReadyToSpawn,
            player_runtime_id: VarULong(player.entity_id() as u64),
        });
    }

    /// The player closed the end credits. As after the Java client's respawn request, it comes
    /// back to its spawn point alive, with everything it has.
    pub fn handle_show_credits(&self, player: &Arc<Player>, packet: &SShowCredits) {
        if packet.credits_state.0 != SShowCredits::STOP
            || !player.won_game.swap(false, Ordering::Relaxed)
        {
            return;
        }
        let player_c = player.clone();
        player.spawn_task(async move {
            player_c.world().respawn_player(&player_c, true).await;
        });
    }
}
