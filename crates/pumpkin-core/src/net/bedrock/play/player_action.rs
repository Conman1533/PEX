#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    #[expect(clippy::match_same_arms)]
    pub fn handle_player_action(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        packet: &SPlayerAction,
    ) {
        if !player.has_client_loaded()
            || ((player.living_entity.dead.load(Ordering::Relaxed)
                || player.living_entity.health.load() <= 0.0)
                && !matches!(packet.action, PlayerAction::Respawn))
        {
            return;
        }
        player.update_last_action_time();

        match packet.action {
            PlayerAction::StartDestroyBlock
            | PlayerAction::CreativeDestroyBlock
            | PlayerAction::ContinueDestroyBlock => {
                self.start_destroy_block(player, server, packet.block_position);
            }
            action @ (PlayerAction::PredictDestroyBlock | PlayerAction::StopDestroyBlock) => {
                self.finish_destroy_block(
                    player,
                    server,
                    packet.block_position,
                    matches!(action, PlayerAction::StopDestroyBlock),
                );
            }
            PlayerAction::CrackBlock => {
                // Don't do anything for this action. It is no longer used. Block
                // cracking is done fully server-side.
            }
            PlayerAction::AbortDestroyBlock => {
                let world = player.world();
                let position = *player
                    .mining_pos
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut abort_event =
                    crate::plugin::api::events::block::block_damage_abort::BlockDamageAbortEvent::new(
                        player.clone(),
                        position,
                        world,
                        player.inventory().held_item(),
                    );
                server
                    .plugin_manager
                    .fire_blocking(server, &mut abort_event);
                player.stop_mining();
            }
            PlayerAction::DropItem => {
                player.drop_held_item(false);
            }
            PlayerAction::StartGliding => {
                player.try_start_fall_flying(server);
            }
            PlayerAction::StopGliding => {
                player.stop_fall_flying(server);
            }
            PlayerAction::StartSwimming => {
                player.get_entity().set_swimming(true);
                player.update_player_pose();
            }
            PlayerAction::StopSwimming => {
                player.get_entity().set_swimming(false);
                player.update_player_pose();
            }
            PlayerAction::StartSpinAttack => {
                player
                    .get_entity()
                    .set_pose(pumpkin_data::entity::EntityPose::SpinAttack);
            }
            PlayerAction::StopSpinAttack => {
                player.update_player_pose();
            }
            PlayerAction::StartSprinting => {
                player.set_sprinting_from_bedrock(server, true);
            }
            PlayerAction::StopSprinting => {
                player.set_sprinting_from_bedrock(server, false);
            }
            PlayerAction::StartSneaking => {
                player.set_sneaking_from_bedrock(server, true);
            }
            PlayerAction::StopSneaking => {
                player.set_sneaking_from_bedrock(server, false);
            }
            PlayerAction::StartCrawling | PlayerAction::StopCrawling => {
                player.update_player_pose();
            }
            PlayerAction::StopSleeping => {
                player.wake_up();
            }
            PlayerAction::Respawn
                if player.living_entity.dead.load(Ordering::Relaxed)
                    || player.living_entity.health.load() <= 0.0 =>
            {
                let player_c = player.clone();
                player.spawn_task(async move {
                    player_c.world().respawn_player(&player_c, false).await;
                });
            }
            // TODO
            _ => {}
        }
    }

    /// Puts the server's copy of a block back on the client, after a break the server refused.
    fn resend_block(&self, world: &crate::world::World, position: BlockPos) {
        let state_id = world.get_block_state_id(&position);
        let runtime_id = pumpkin_data::BlockState::to_be_network_id(state_id);
        self.try_enqueue_client_packet(&CUpdateBlock::new(position, runtime_id));
    }

    /// The client says it is still digging the block the server has it mining: only the speed
    /// can have changed (a new tool, haste), so tell everyone the new one.
    fn update_breaking_speed(player: &Player, world: &crate::world::World, position: BlockPos) {
        let (block, state) = world.get_block_and_state(&position);
        if player.gamemode.load() == GameMode::Creative || state.is_air() {
            return;
        }
        let speed = crate::block::calc_block_breaking(player, state, block);
        let old_speed = player
            .current_block_breaking_speed
            .swap(speed.to_bits(), Ordering::Relaxed);
        if old_speed != speed.to_bits() {
            world.set_block_breaking(
                player.get_entity(),
                position,
                BlockBreakingProgress::Update {
                    stage: player.current_block_destroy_stage.load(Ordering::Relaxed),
                    speed: Some(speed),
                },
            );
        }
    }

    /// A Bedrock player starts digging, or switches to another block while holding the button.
    /// The same steps as the Java `StartedDigging` path: the interact and damage events, the
    /// game mode and held-item rules, then an instant break or the start of a timed one.
    fn start_destroy_block(&self, player: &Arc<Player>, server: &Arc<Server>, position: BlockPos) {
        if !player.can_interact_with_block_at(&position, 1.0) {
            return;
        }
        let entity = player.get_entity();
        let world = entity.world.load_full();
        let (block, state) = world.get_block_and_state(&position);

        let mining = player.mining.load(Ordering::Relaxed);
        let mining_pos = *player
            .mining_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if mining && mining_pos == position {
            Self::update_breaking_speed(player, &world, position);
            return;
        }
        if mining {
            player.stop_mining();
        }

        let mut interact_event = PlayerInteractEvent::new(
            player,
            InteractAction::LeftClickBlock,
            block,
            Some(position),
        );
        server
            .plugin_manager
            .fire_blocking(server, &mut interact_event);
        if interact_event.cancelled {
            self.resend_block(&world, position);
            return;
        }
        let mut damage_event =
            crate::plugin::api::events::block::block_damage::BlockDamageEvent::new(
                player.clone(),
                block,
                position,
                false,
            );
        server
            .plugin_manager
            .fire_blocking(server, &mut damage_event);
        if damage_event.cancelled || player.cannot_mine(server, block) {
            self.resend_block(&world, position);
            return;
        }

        if player.gamemode.load() == GameMode::Creative {
            let new_state = world.break_block(
                &position,
                Some(player),
                BlockFlags::NOTIFY_ALL | BlockFlags::SKIP_DROPS,
            );
            if new_state.is_some() {
                server
                    .block_registry
                    .broken(&world, block, player, &position, server, state);
            } else {
                self.resend_block(&world, position);
            }
            return;
        }
        player.start_mining_time.store(
            player.tick_counter.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        if state.is_air() {
            return;
        }
        server
            .block_registry
            .attack(&world, block, state, &position, player);
        let speed = crate::block::calc_block_breaking(player, state, block);
        if speed >= 1.0 {
            if !player.destroy_block(&world, server, position) {
                self.resend_block(&world, position);
            }
            return;
        }
        player.mining.store(true, Ordering::Relaxed);
        *player
            .mining_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = position;
        let progress = (speed * 10.0) as i32;
        player
            .current_block_breaking_speed
            .store(speed.to_bits(), Ordering::Relaxed);
        world.set_block_breaking(
            entity,
            position,
            BlockBreakingProgress::Start {
                stage: progress,
                speed,
            },
        );
        player
            .current_block_destroy_stage
            .store(progress, Ordering::Relaxed);
    }

    /// The client says a timed break is done (`PredictDestroyBlock`), or the button came up on
    /// one (`StopDestroyBlock`). The server accepts it on the same terms as a Java
    /// `FinishedDigging`: at least `STOP_DESTROY_MIN_PROGRESS` of its own progress, or a delayed
    /// destroy that the player tick completes. Anything else puts the block back on the client.
    fn finish_destroy_block(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        position: BlockPos,
        button_up: bool,
    ) {
        if !player.can_interact_with_block_at(&position, 1.0) {
            return;
        }
        let entity = player.get_entity();
        let world = entity.world.load_full();
        if player.gamemode.load() == GameMode::Creative {
            return;
        }
        if world.get_block_state(&position).is_air() {
            // Already broken, by the prediction before this button-up or by the delayed destroy.
            return;
        }
        let same_block = *player
            .mining_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            == position;
        if !player.mining.load(Ordering::Relaxed) || !same_block {
            if !button_up {
                self.resend_block(&world, position);
            }
            return;
        }
        if player.stop_destroy_block(&world, position) {
            world.set_block_breaking(entity, position, BlockBreakingProgress::Stop);
            if !player.destroy_block(&world, server, position) {
                self.resend_block(&world, position);
            }
        } else if player.delayed_destroy.load().is_none() {
            self.resend_block(&world, position);
        }
    }
}
