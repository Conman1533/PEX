#[allow(clippy::wildcard_imports)]
use super::*;

/// Ticks without a confirmation after which vanilla `updateAwaitingTeleport` sends the teleport
/// again.
const AWAITING_TELEPORT_RESEND_TICKS: i32 = 20;

impl JavaClient {
    const fn clamp_horizontal(pos: f64) -> f64 {
        pos.clamp(-3.0E7, 3.0E7)
    }

    const fn clamp_vertical(pos: f64) -> f64 {
        pos.clamp(-2.0E7, 2.0E7)
    }

    /// Returns whether syncing the position was needed
    fn sync_position(
        player: &Arc<Player>,
        world: &World,
        entity: &Entity,
        pos: Vector3<f64>,
        last_pos: Vector3<f64>,
        on_ground: bool,
    ) -> bool {
        let delta = Vector3::new(pos.x - last_pos.x, pos.y - last_pos.y, pos.z - last_pos.z);
        let entity_id = player.entity_id();

        // Teleport when more than 8 blocks (-8..=7.999755859375)
        if delta.length_squared() < 64.0 {
            return false;
        }
        // Sync position with tracking players only.
        world.send_to_tracking_players_editioned(
            entity,
            &CEntityPositionSync::new(
                entity_id.into(),
                pos,
                Vector3::new(0.0, 0.0, 0.0),
                entity.yaw.load(),
                entity.pitch.load(),
                on_ground,
            ),
            &bedrock_move_player_packet(entity, pos, CMovePlayer::MODE_TELEPORT, on_ground),
        );
        // Bedrock ignores head yaw on teleport -> follow-up normal move re-asserts.
        world.send_to_tracking_players_bedrock(
            entity,
            &bedrock_move_player_packet(entity, pos, CMovePlayer::MODE_NORMAL, on_ground),
        );
        true
    }

    #[expect(clippy::too_many_lines)]
    pub fn handle_position(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        packet: &SPlayerPosition,
    ) {
        if !player.has_client_loaded() {
            return;
        }
        // A movement packet was received this tick — tracked for SClientTickEnd zeroing.
        self.received_movement_this_tick
            .store(true, Ordering::Relaxed);
        if player.get_entity().has_vehicle() {
            return;
        }
        // y = feet Y
        let position = packet.position;
        if position.x.is_nan() || position.y.is_nan() || position.z.is_nan() {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                [],
            ));
            return;
        }
        // Vanilla `handleMovePlayer` ignores positions until the client confirms the last teleport.
        if self.update_awaiting_teleport(player) {
            return;
        }
        if player.is_movement_locked.load(Ordering::Relaxed) {
            self.force_tp(player, player.get_entity().pos.load());
            return;
        }
        let position = Vector3::new(
            Self::clamp_horizontal(position.x),
            Self::clamp_vertical(position.y),
            Self::clamp_horizontal(position.z),
        );

        send_cancellable_blocking! {{
            server;
            PlayerMoveEvent {
                player: player.clone(),
                from: player.get_entity().pos.load(),
                to: position,
                cancelled: false,
            };

            'after: {
                let pos = event.to;
                let entity = &player.get_entity();
                let last_pos = entity.pos.load();
                player.get_entity().set_pos(pos);

                let distance = last_pos.squared_distance_to_vec(&pos).sqrt();
                let cm = (distance * 100.0) as i32;
                if cm > 0 {
                    let stat = player.get_movement_statistic();
                    player.increment_stat(StatisticCategory::Custom, stat as i32, cm);
                }

                let height_difference = pos.y - last_pos.y;
                if entity.on_ground.load(Ordering::Relaxed) && packet.collision & FLAG_ON_GROUND == 0 && height_difference > 0.0 {
                    player.jump();
                }

                let new_on_ground = packet.collision & FLAG_ON_GROUND != 0;
                entity.on_ground.store(new_on_ground, Ordering::Relaxed);
                let movement = pos.sub(&last_pos);
                if movement.length_squared() < 64.0 {
                    entity.emit_movement_events(player.as_ref(), movement);
                }
                if new_on_ground && entity.is_fall_flying() {
                    entity.set_fall_flying(false);
                }
                let world = &player.world();

                // TODO: Warn when player moves to quickly
                if !Self::sync_position(player, world, entity, pos, last_pos, packet.collision & FLAG_ON_GROUND != 0) {
                    // Send the new position to tracking players only.
                    world.send_to_tracking_players_editioned(
                        entity,
                        &CUpdateEntityPos::new(
                            player.entity_id().into(),
                            Vector3::new(
                                pos.x.mul_add(4096.0, -(last_pos.x * 4096.0)) as i16,
                                pos.y.mul_add(4096.0, -(last_pos.y * 4096.0)) as i16,
                                pos.z.mul_add(4096.0, -(last_pos.z * 4096.0)) as i16,
                            ),
                            packet.collision & FLAG_ON_GROUND != 0,
                        ),
                        &bedrock_move_player_packet(
                            entity,
                            pos,
                            CMovePlayer::MODE_NORMAL,
                            packet.collision & FLAG_ON_GROUND != 0,
                        ),
                    );
                }

                // Only process fall damage if player is alive
                if !player.abilities.lock().unwrap_or_else(std::sync::PoisonError::into_inner).flying
                    && player.living_entity.health.load() > 0.0
                    && !player.living_entity.dead.load(Ordering::Relaxed)
                {
                    player.living_entity.fall(
                        player.as_ref(),
                        height_difference,
                        packet.collision & FLAG_ON_GROUND != 0,
                    );
                }
                chunker::update_position(player);
                let delta = Vector3::new(
                    pos.x - last_pos.x,
                    pos.y - last_pos.y,
                    pos.z - last_pos.z,
                );
                // Only update idle timeout if there's actual movement (vanilla threshold)
                if delta.length_squared() > 1.0E-5 {
                    player.update_last_action_time();
                    player.check_location_enchantments(pos, packet.collision & FLAG_ON_GROUND != 0);
                }
                player.progress_motion(delta);
            }

            'cancelled: {
                self.force_tp(player, player.get_entity().pos.load());
            }
        }}
    }

    #[expect(clippy::too_many_lines)]
    pub fn handle_position_rotation(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        packet: &SPlayerPositionRotation,
    ) {
        if !player.has_client_loaded() {
            return;
        }
        // A movement packet was received this tick — tracked for SClientTickEnd zeroing.
        self.received_movement_this_tick
            .store(true, Ordering::Relaxed);
        if player.get_entity().has_vehicle() {
            return;
        }
        // y = feet Y
        let position = packet.position;
        if !position.x.is_finite()
            || !position.y.is_finite()
            || !position.z.is_finite()
            || !packet.yaw.is_finite()
            || !packet.pitch.is_finite()
        {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                [],
            ));
            return;
        }
        // Vanilla `handleMovePlayer` ignores positions until the client confirms the last teleport,
        // but still takes the rotation.
        if self.update_awaiting_teleport(player) {
            player
                .get_entity()
                .set_rotation(wrap_degrees(packet.yaw) % 360.0, wrap_degrees(packet.pitch));
            return;
        }
        if player.is_movement_locked.load(Ordering::Relaxed) {
            let entity = player.get_entity();
            entity.set_rotation(packet.yaw, packet.pitch);
            self.force_tp(player, entity.pos.load());
            return;
        }

        let position = Vector3::new(
            Self::clamp_horizontal(position.x),
            Self::clamp_vertical(position.y),
            Self::clamp_horizontal(position.z),
        );

        send_cancellable_blocking! {{
            server;
            PlayerMoveEvent::new(
                player.clone(),
                player.get_entity().pos.load(),
                position,
            );

            'after: {
                let pos = event.to;
                let entity = &player.get_entity();
                let last_pos = entity.pos.load();
                player.get_entity().set_pos(pos);

                let distance = last_pos.squared_distance_to_vec(&pos).sqrt();
                let cm = (distance * 100.0) as i32;
                if cm > 0 {
                    let stat = player.get_movement_statistic();
                    player.increment_stat(StatisticCategory::Custom, stat as i32, cm);
                }

                let height_difference = pos.y - last_pos.y;
                if entity.on_ground.load(Ordering::Relaxed)
                    && (packet.collision & FLAG_ON_GROUND) == 0
                    && height_difference > 0.0
                {
                    player.jump();
                }
                let new_on_ground = (packet.collision & FLAG_ON_GROUND) != 0;
                entity
                    .on_ground
                    .store(new_on_ground, Ordering::Relaxed);
                let movement = pos.sub(&last_pos);
                if movement.length_squared() < 64.0 {
                    entity.emit_movement_events(player.as_ref(), movement);
                }
                if new_on_ground && entity.is_fall_flying() {
                    entity.set_fall_flying(false);
                }

                entity.set_rotation(wrap_degrees(packet.yaw) % 360.0, wrap_degrees(packet.pitch));

                let entity_id = entity.entity_id;

                // TODO: use `pumpkin_util::math::pack_degrees`.
                let yaw = (entity.yaw.load() * 256.0 / 360.0).rem_euclid(256.0);
                let pitch = (entity.pitch.load() * 256.0 / 360.0).rem_euclid(256.0);
                let world = entity.world.load_full();

                // TODO: Warn when player moves to quickly
                if !Self::sync_position(
                    player,
                    &world,
                    entity,
                    pos,
                    last_pos,
                    (packet.collision & FLAG_ON_GROUND) != 0,
                ) {
                    // Send the new position to tracking players only.
                    world.send_to_tracking_players_editioned(
                        entity,
                        &CUpdateEntityPosRot::new(
                            entity_id.into(),
                            Vector3::new(
                                pos.x.mul_add(4096.0, -(last_pos.x * 4096.0)) as i16,
                                pos.y.mul_add(4096.0, -(last_pos.y * 4096.0)) as i16,
                                pos.z.mul_add(4096.0, -(last_pos.z * 4096.0)) as i16,
                            ),
                            yaw as u8,
                            pitch as u8,
                            (packet.collision & FLAG_ON_GROUND) != 0,
                        ),
                        &bedrock_move_player_packet(
                            entity,
                            pos,
                            CMovePlayer::MODE_NORMAL,
                            (packet.collision & FLAG_ON_GROUND) != 0,
                        ),
                    );
                }

                world.send_to_tracking_players(entity, &CHeadRot::new(entity_id.into(), yaw as u8));
                // Only process fall damage if player is alive
                if !player.abilities.lock().unwrap_or_else(std::sync::PoisonError::into_inner).flying
                    && player.living_entity.health.load() > 0.0
                    && !player.living_entity.dead.load(Ordering::Relaxed)
                {
                    player.living_entity.fall(
                        player.as_ref(),
                        height_difference,
                        (packet.collision & FLAG_ON_GROUND) != 0,
                    );
                }
                chunker::update_position(player);
                let delta = Vector3::new(
                    pos.x - last_pos.x,
                    pos.y - last_pos.y,
                    pos.z - last_pos.z,
                );
                // Only update idle timeout if there's actual movement (vanilla threshold)
                if delta.length_squared() > 1.0E-5 {
                    player.update_last_action_time();
                    player.check_location_enchantments(pos, (packet.collision & FLAG_ON_GROUND) != 0);
                }
                player.progress_motion(delta);
            }

            'cancelled: {
                self.force_tp(player, player.get_entity().pos.load());
            }
        }}
    }

    pub fn force_tp(&self, player: &Arc<Player>, position: Vector3<f64>) {
        let entity = player.get_entity();
        self.teleport(
            player,
            position,
            entity.yaw.load(),
            entity.pitch.load(),
            Vec::new(),
        );
    }

    /// Vanilla `ServerGamePacketListenerImpl.teleport`: sends the client to `position` and waits
    /// for it to confirm this teleport. Confirmations of earlier teleports are ignored from now on.
    /// The caller has already moved the player to `position`, which is absolute: `relatives` may
    /// only make the velocity and rotation relative.
    pub fn teleport(
        &self,
        player: &Player,
        position: Vector3<f64>,
        yaw: f32,
        pitch: f32,
        relatives: Vec<PositionFlag>,
    ) {
        let teleport_id = player
            .awaiting_teleport
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .start(position, player.tick_counter.load(Ordering::Relaxed));
        // Sent after the lock is released: plugins see the packet in `PacketSentEvent`.
        self.try_send_packet(&CPlayerPosition::new(
            teleport_id.into(),
            position,
            Vector3::new(0.0, 0.0, 0.0),
            yaw,
            pitch,
            relatives,
        ));
    }

    /// Vanilla `updateAwaitingTeleport`: whether the client still has to confirm the last
    /// teleport. Sends it again when the client keeps moving without confirming it.
    pub(super) fn update_awaiting_teleport(&self, player: &Player) -> bool {
        let tick = player.tick_counter.load(Ordering::Relaxed);
        let resend = {
            let mut awaiting = player
                .awaiting_teleport
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(position) = awaiting.position else {
                awaiting.time = tick;
                return false;
            };
            (tick.wrapping_sub(awaiting.time) > AWAITING_TELEPORT_RESEND_TICKS).then_some(position)
        };
        if let Some(position) = resend {
            let entity = player.get_entity();
            self.teleport(
                player,
                position,
                entity.yaw.load(),
                entity.pitch.load(),
                Vec::new(),
            );
        }
        true
    }
}
