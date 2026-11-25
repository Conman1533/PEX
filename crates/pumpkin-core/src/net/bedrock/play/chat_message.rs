#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    pub async fn handle_chat_message(
        &self,
        server: &Arc<Server>,
        player: &Arc<Player>,
        packet: SText<'_>,
    ) {
        player.update_last_action_time();
        if player.check_chat_spam(server, crate::entity::player::SpamType::Chat) {
            return;
        }
        let gameprofile = &player.gameprofile;

        send_cancellable! {{
            server;
            PlayerChatEvent::new(player.clone(), packet.message.into_owned(), vec![], None);

            'after: {
                info!("<chat> {}: {}", gameprofile.name, event.message);

                let config = &server.advanced_config;

                let message = match seasonal_events::modify_chat_message(&event.message, config) {
                    Some(m) => m,
                    None => event.message.clone(),
                };

                let decorated_message = TextComponent::chat_decorated(
                    &config.chat.format,
                    &gameprofile.name,
                    &message,
                );

                // A Bedrock message has no Java chat signature, so it goes out as system chat
                // whether or not `allow_chat_reports` is on.
                let je_packet = CSystemChatMessage::new(&decorated_message, false);

                let be_packet = SText::chat_with_xuid(
                    message,
                    gameprofile.name.clone(),
                    packet.xuid.into_owned(),
                    packet.filtered_message.map(std::borrow::Cow::into_owned),
                );

                player.get_entity().world.load().broadcast_editioned(&je_packet, &be_packet);
            }
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::java::JavaClient;
    use crate::net::{ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig};
    use pumpkin_data::packet::CURRENT_MC_VERSION;
    use pumpkin_protocol::MultiVersionJavaPacket;
    use pumpkin_util::math::vector3::Vector3;
    use std::{collections::HashMap, net::SocketAddr};

    fn profile(name: &str) -> GameProfile {
        GameProfile {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            properties: arc_swap::ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bedrock_chat_broadcasts_with_chat_reports() {
        let folder = tempfile::tempdir().unwrap();
        let basic_config = pumpkin_config::BasicConfiguration {
            default_level_name: folder.path().to_string_lossy().into_owned(),
            allow_nether: false,
            allow_end: false,
            allow_chat_reports: true,
            ..Default::default()
        };
        let mut advanced_config = pumpkin_config::AdvancedConfiguration::default();
        // Metrics would write `metrics/` into the crate folder.
        advanced_config.metrics.enabled = false;
        let server = Server::new(
            basic_config,
            advanced_config,
            pumpkin_config::TelemetryConfig::default(),
            crate::data::VanillaData::default(),
            Vec::new(),
        )
        .await
        .unwrap();
        assert!(server.basic_config.allow_chat_reports);

        let world = crate::world::test_world();
        let mut java = JavaClient::new_for_test(profile("JavaListener"));
        java.version.store(CURRENT_MC_VERSION);
        let mut queue = java.take_outgoing_queue().expect("outgoing queue");
        crate::world::test_player_with_client(&world, Vector3::new(0.5, 64.0, 0.5), java);

        let client = Arc::new(BedrockClient::new(
            crate::net::bedrock::nethernet::test_session().await,
            SocketAddr::from(([127, 0, 0, 1], 19132)),
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            PacketRateLimiter::from_config(&pumpkin_config::PacketLimiterConfig::default()),
        ));
        let player = Arc::new(Player::new(
            Arc::new(ClientPlatform::Bedrock(client.clone())),
            profile("BedrockChatter"),
            PlayerConfig::default(),
            &world,
            GameMode::Survival,
        ));

        client
            .handle_chat_message(
                &server,
                &player,
                SText::chat_with_xuid(
                    "hello from bedrock".to_string(),
                    "BedrockChatter".to_string(),
                    String::new(),
                    None,
                ),
            )
            .await;

        let ids = JavaClient::queued_packet_ids(&mut queue);
        assert!(
            ids.contains(&CSystemChatMessage::to_id(CURRENT_MC_VERSION)),
            "the Java player got no chat line, sent {ids:?}"
        );
    }
}
