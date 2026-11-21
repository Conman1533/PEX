#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub fn handle_player_command(
        &self,
        player: &Arc<Player>,
        command: &SPlayerCommand,
        server: &Arc<Server>,
    ) {
        if command.entity_id != player.entity_id().into() {
            return;
        }
        if !player.has_client_loaded() {
            return;
        }
        player.update_last_action_time();

        let entity = &player.get_entity();
        match command.action {
            Action::StartSprinting => {
                if !entity.is_sprinting() {
                    send_cancellable_blocking! {{
                        server;
                        PlayerToggleSprintEvent::new(player.clone(), true);
                        'after: {
                            player.set_sprinting(event.is_sprinting);
                            player.update_player_pose();
                        }
                    }}
                }
            }
            Action::StopSprinting => {
                if entity.is_sprinting() {
                    send_cancellable_blocking! {{
                        server;
                        PlayerToggleSprintEvent::new(player.clone(), false);
                        'after: {
                            player.set_sprinting(event.is_sprinting);
                            player.update_player_pose();
                        }
                    }}
                }
            }
            Action::LeaveBed => player.wake_up(),

            // Vanilla `handlePlayerCommand`: the client makes the jump itself; the mount plays
            // its part (a horse rears, a camel dashes).
            Action::StartHorseJump => {
                let power = command.jump_boost.0;
                if let Some(vehicle) = entity.get_vehicle()
                    && let Some(mob) = vehicle.get_mob()
                    && power > 0
                    && mob.can_rider_jump()
                {
                    mob.handle_start_jump(power);
                }
            }
            Action::StopHorseJump => {
                if let Some(vehicle) = entity.get_vehicle()
                    && let Some(mob) = vehicle.get_mob()
                {
                    mob.handle_stop_jump();
                }
            }
            Action::OpenVehicleInventory => {
                debug!("todo");
            }
            Action::StartFlyingElytra => {
                player.try_start_fall_flying(server);
            }
            // <= 1.21.5
            Action::StartSneaking | Action::StopSneaking => {
                self.handle_player_input(
                    player,
                    &SPlayerInput {
                        input: SPlayerInput::SNEAK,
                    },
                    server,
                );
            }
        }
    }
}
