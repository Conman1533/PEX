#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_data::entity::EntityPose;

impl BedrockClient {
    #[expect(clippy::too_many_lines)]
    pub fn handle_player_auth_input(
        &self,
        player: &Arc<Player>,
        packet: SPlayerAuthInput,
        server: &Arc<Server>,
    ) {
        if !player.has_client_loaded() {
            return;
        }
        if player.living_entity.dead.load(Ordering::Relaxed)
            || player.living_entity.health.load() <= 0.0
        {
            return;
        }
        let entity = player.get_entity();
        let input_data = &packet.input_data;
        let on_ground = input_data.get(InputData::VerticalCollision as usize)
            && packet.delta.y < 0.0
            && !entity.has_vehicle();
        entity.on_ground.store(on_ground, Ordering::Relaxed);
        self.input_tick.store(packet.tick.0, Ordering::Relaxed);

        let new_pos = packet
            .position
            .add_raw(0.0, -entity.entity_type.eye_height, 0.0)
            .to_f64();
        let old_pos = player.position();

        let new_pitch = packet.pitch;
        let new_yaw = packet.yaw;

        let old_pitch = entity.pitch.load();
        let old_yaw = entity.yaw.load();

        let pos_changed = new_pos != old_pos;
        let rot_changed = new_pitch != old_pitch || new_yaw != old_yaw;

        if pos_changed || rot_changed {
            let world = player.world();

            if pos_changed {
                player.get_entity().set_pos(new_pos);
            }
            if rot_changed {
                entity.pitch.store(new_pitch);
                entity.yaw.store(new_yaw);
                entity.head_yaw.store(new_yaw);
            }

            let delta = pumpkin_util::math::vector3::Vector3::new(
                new_pos.x - old_pos.x,
                new_pos.y - old_pos.y,
                new_pos.z - old_pos.z,
            );

            let bedrock_move_packet = pumpkin_protocol::bedrock::client::CMovePlayer::new(
                pumpkin_protocol::codec::var_ulong::VarULong(player.entity_id() as u64),
                pumpkin_util::math::vector3::Vector3::new(
                    new_pos.x as f32,
                    new_pos.y as f32 + entity.entity_type.eye_height,
                    new_pos.z as f32,
                ),
                new_pitch,
                new_yaw,
                new_yaw, // Head yaw
                pumpkin_protocol::bedrock::client::CMovePlayer::MODE_NORMAL,
                on_ground,
                pumpkin_protocol::codec::var_ulong::VarULong(0),
                0,
                0,
                pumpkin_protocol::codec::var_ulong::VarULong(0),
            );

            // Java watchers get this player's movement, rotation and head rotation from the
            // entity tracker (`TrackedEntity::send_changes`).
            world.broadcast_packet_except_bedrock(&[player.gameprofile.id], &bedrock_move_packet);

            if pos_changed {
                chunker::update_position(player);
                Self::apply_movement_effects(player, delta, on_ground);
                player.check_location_enchantments(new_pos, on_ground);
                player.progress_motion(delta);
            }
        }

        if input_data.get(InputData::StartJumping as usize) && !entity.has_vehicle() {
            player.jump();
        }

        // Fall distance builds up while falling and is applied on landing, as for Java players
        // in `handle_position`.
        let flying = player
            .abilities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flying;
        if !flying && !entity.has_vehicle() {
            player
                .living_entity
                .fall(player.as_ref(), new_pos.y - old_pos.y, on_ground);
        }

        crate::entity::client_push::send_bedrock_client_push(player, packet.delta.to_f64());

        if input_data.get(InputData::StartSprinting as usize) {
            player.set_sprinting_from_bedrock(server, true);
        } else if input_data.get(InputData::StopSprinting as usize) {
            player.set_sprinting_from_bedrock(server, false);
        }

        if input_data.get(InputData::StartSneaking as usize) {
            player.set_sneaking_from_bedrock(server, true);
        } else if input_data.get(InputData::StopSneaking as usize) {
            player.set_sneaking_from_bedrock(server, false);
        }

        // The server decides the pose from the space around the player, as for Java players.
        if input_data.get(InputData::StartCrawling as usize)
            || input_data.get(InputData::StopCrawling as usize)
        {
            player.update_player_pose();
        }

        if input_data.get(InputData::StartGliding as usize) {
            player.try_start_fall_flying(server);
        } else if input_data.get(InputData::StopGliding as usize) {
            player.stop_fall_flying(server);
        }

        if input_data.get(InputData::StartSwimming as usize) {
            entity.set_swimming(true);
            player.update_player_pose();
        } else if input_data.get(InputData::StopSwimming as usize) {
            entity.set_swimming(false);
            player.update_player_pose();
        }

        if input_data.get(InputData::StartSpinAttack as usize) {
            entity.set_pose(EntityPose::SpinAttack);
        } else if input_data.get(InputData::StopSpinAttack as usize) {
            player.update_player_pose();
        }

        if input_data.get(InputData::StartFlying as usize) {
            let flying = {
                player
                    .abilities
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .flying
            };
            if !flying {
                send_cancellable_blocking! {{
                    server;
                    PlayerToggleFlightEvent::new(player.clone(), true);
                    'after: {
                        player.living_entity.fall_distance.store(0.0);
                        {
                            player.abilities.lock().unwrap_or_else(std::sync::PoisonError::into_inner).flying = true;
                        };
                        player.send_abilities_update();
                    }
                    'cancelled: {
                        player.send_abilities_update();
                    }
                }}
            }
        } else if input_data.get(InputData::StopFlying as usize) {
            let flying = {
                player
                    .abilities
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .flying
            };
            if flying {
                send_cancellable_blocking! {{
                    server;
                    PlayerToggleFlightEvent::new(player.clone(), false);
                    'after: {
                        {
                            player.abilities.lock().unwrap_or_else(std::sync::PoisonError::into_inner).flying = false;
                        };
                        player.send_abilities_update();
                    }
                    'cancelled: {
                        player.send_abilities_update();
                    }
                }}
            }
        }

        // Java clients send their held movement keys in `PlayerInput`; minecarts and other
        // server-driven vehicles read them from here.
        player
            .last_input
            .store(Self::java_input_bits(input_data), Ordering::Relaxed);

        if entity.has_vehicle() {
            let paddling_left = input_data.get(InputData::PaddlingLeft as usize);
            let paddling_right = input_data.get(InputData::PaddlingRight as usize);
            let vehicle = entity
                .vehicle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(vehicle) = vehicle {
                vehicle.set_paddle_state(paddling_left, paddling_right);
                Self::steer_vehicle(player, &vehicle, &packet, new_pos.sub(&old_pos));
                let jumping = input_data.get(InputData::Jumping as usize)
                    || input_data.get(InputData::JumpDown as usize);
                self.charge_riding_jump(player, &vehicle, jumping);
            }
        } else {
            self.riding_jump_ticks.store(-1, Ordering::Relaxed);
        }

        if let Some(block_actions) = packet.block_actions {
            for action in &block_actions {
                self.handle_player_block_action(player, server, action);
            }
        }
    }

    /// The Java `PlayerInput` bits for the keys a Bedrock client reports held.
    fn java_input_bits(input_data: &pumpkin_protocol::codec::bitset::Bitset<66>) -> i8 {
        use pumpkin_protocol::java::server::play::SPlayerInput;
        let held = |flag: InputData| input_data.get(flag as usize);
        let mut bits = 0;
        for (flags, bit) in [
            (
                [InputData::Up, InputData::UpLeft, InputData::UpRight],
                SPlayerInput::FORWARD,
            ),
            (
                [InputData::Down, InputData::DownLeft, InputData::DownRight],
                SPlayerInput::BACKWARD,
            ),
            (
                [InputData::Left, InputData::UpLeft, InputData::DownLeft],
                SPlayerInput::LEFT,
            ),
            (
                [InputData::Right, InputData::UpRight, InputData::DownRight],
                SPlayerInput::RIGHT,
            ),
        ] {
            if flags.into_iter().any(held) {
                bits |= bit;
            }
        }
        for (flag, bit) in [
            (InputData::Jumping, SPlayerInput::JUMP),
            (InputData::Sneaking, SPlayerInput::SNEAK),
            (InputData::Sprinting, SPlayerInput::SPRINT),
        ] {
            if held(flag) {
                bits |= bit;
            }
        }
        bits
    }

    /// A Bedrock driver moves its vehicle itself and says so with `ClientPredictedVehicle`, as a
    /// Java driver does with `MoveVehicle`: the vehicle follows the rider's move and takes the
    /// rotation the client predicted. Only the passenger in control moves it.
    fn steer_vehicle(
        player: &Arc<Player>,
        vehicle: &Arc<dyn EntityBase>,
        packet: &SPlayerAuthInput,
        movement: pumpkin_util::math::vector3::Vector3<f64>,
    ) {
        let vehicle_entity = vehicle.get_entity();
        let predicted = packet
            .vehicle_unique_id
            .as_ref()
            .is_some_and(|id| id.0 == i64::from(vehicle_entity.entity_id));
        // Mobs decide who steers them (a saddle, a carrot on a stick); other vehicles take the
        // first passenger, as vanilla's boats and minecarts do.
        let controller = vehicle.get_controlling_passenger().or_else(|| {
            if vehicle.get_living_entity().is_some() {
                return None;
            }
            vehicle_entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .first()
                .cloned()
        });
        let in_control =
            controller.is_some_and(|rider| rider.get_entity().entity_id == player.entity_id());
        if !predicted || !in_control {
            return;
        }
        if movement.length_squared() > 0.0 {
            vehicle_entity.set_pos(vehicle_entity.pos.load().add(&movement));
        }
        if let Some(rotation) = packet.vehicle_rotation {
            // Pitch, then yaw, as in `AddActor`.
            vehicle_entity.set_rotation(rotation.y, rotation.x);
        }
    }

    /// A Java client charges its mount's jump while the key is held and sends
    /// `START_RIDING_JUMP` with the power when it is let go (vanilla `LocalPlayer.aiStep`). A
    /// Bedrock client charges its own jump bar but tells the server nothing about it, so the
    /// server counts the held ticks the same way and lets the mount play its part on release.
    fn charge_riding_jump(
        &self,
        player: &Arc<Player>,
        vehicle: &Arc<dyn EntityBase>,
        jumping: bool,
    ) {
        let in_control = vehicle
            .get_controlling_passenger()
            .is_some_and(|rider| rider.get_entity().entity_id == player.entity_id());
        let Some(mob) = vehicle.get_mob().filter(|_| in_control) else {
            self.riding_jump_ticks.store(-1, Ordering::Relaxed);
            return;
        };
        let held = self.riding_jump_ticks.load(Ordering::Relaxed);
        if jumping {
            self.riding_jump_ticks
                .store(if held < 0 { 0 } else { held + 1 }, Ordering::Relaxed);
        } else if held >= 0 {
            self.riding_jump_ticks.store(-1, Ordering::Relaxed);
            let power = crate::entity::mob::riding_jump_power(held);
            if power > 0 && mob.can_rider_jump() {
                mob.handle_start_jump(power);
            }
        }
    }

    /// What a Java player's move does besides moving: statistics, game events (footstep
    /// vibrations), ending an elytra flight on landing, and resetting the idle timer.
    fn apply_movement_effects(
        player: &Arc<Player>,
        movement: pumpkin_util::math::vector3::Vector3<f64>,
        on_ground: bool,
    ) {
        let entity = player.get_entity();
        let cm = (movement.length() * 100.0) as i32;
        if cm > 0 {
            player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                player.get_movement_statistic() as i32,
                cm,
            );
        }
        if movement.length_squared() < 64.0 {
            entity.emit_movement_events(player.as_ref(), movement);
        }
        if on_ground && entity.is_fall_flying() {
            entity.set_fall_flying(false);
        }
        if movement.length_squared() > 1.0E-5 {
            player.update_last_action_time();
        }
    }
}
