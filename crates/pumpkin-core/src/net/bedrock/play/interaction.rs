#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    pub fn handle_interaction(&self, player: &Player, packet: &SInteract) {
        if matches!(packet.action, Action::StopRiding) {
            // The client sends where it would land; the server picks the spot itself, as
            // vanilla's dismount does for Java players.
            player.dismount();
        } else if matches!(packet.action, Action::OpenInventory) {
            if self.inventory_opened.load(Ordering::Relaxed) {
                return;
            }
            self.inventory_opened.store(true, Ordering::Relaxed);
            self.try_enqueue_client_packet(&CContainerOpen {
                container_id: 0,
                container_type: 0xff,
                position: BlockPos::ZERO,
                target_entity_id: VarLong(-1),
            });
        }
    }
}
