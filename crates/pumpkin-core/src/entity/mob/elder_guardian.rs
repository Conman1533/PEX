use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_protocol::java::client::play::{CGameEvent, GameEvent};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;

use crate::entity::mob::guardian::GuardianState;
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
};

/// Vanilla `ElderGuardian.EFFECT_INTERVAL`.
const EFFECT_INTERVAL: i32 = 1200;
/// Vanilla `ElderGuardian.EFFECT_RADIUS`.
const EFFECT_RADIUS: f64 = 50.0;
/// Vanilla `ElderGuardian.EFFECT_DURATION`.
const EFFECT_DURATION: i32 = 6000;
/// Vanilla `ElderGuardian.EFFECT_AMPLIFIER`.
const EFFECT_AMPLIFIER: u8 = 2;
/// Vanilla `ElderGuardian.EFFECT_DISPLAY_LIMIT`.
const EFFECT_DISPLAY_LIMIT: i32 = 1200;
/// Vanilla `ElderGuardian.customServerAiStep`: `setHomeTo(blockPosition(), 16)`.
const HOME_RADIUS: i32 = 16;

pub struct ElderGuardianEntity {
    pub mob_entity: MobEntity,
    pub state: Arc<GuardianState>,
}

impl ElderGuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        let state = Arc::new(GuardianState::default());
        state.init_controls(&mut mob_entity);
        // Vanilla `ElderGuardian`: `setPersistenceRequired()`.
        mob_entity
            .persistence_required
            .store(true, Ordering::Relaxed);
        let mob_arc = Arc::new(Self { mob_entity, state });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        mob_arc
            .state
            .register_goals(&mob_arc.mob_entity, &mob_weak, true);
        mob_arc
    }

    /// Vanilla `MobEffectUtil.addEffectToPlayersAround` with the elder's mining fatigue: players
    /// in survival or adventure within 50 blocks that are not on its team, unless they already
    /// have a stronger fatigue or one with more than a minute left. Only they get the curse
    /// screen.
    fn curse_players_around(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let pos = entity.pos.load();
        let world = entity.world.load();
        let elder: &dyn EntityBase = self;
        let packet = CGameEvent::new(
            GameEvent::PlayElderGuardianMobAppearance,
            if entity.is_silent() { 0.0 } else { 1.0 },
        );
        for player in world.players.load().iter() {
            let living = &player.living_entity;
            let needs_effect =
                living
                    .get_effect(&StatusEffect::MINING_FATIGUE)
                    .is_none_or(|current| {
                        current.amplifier < EFFECT_AMPLIFIER
                            || (current.duration != -1 && current.duration < EFFECT_DISPLAY_LIMIT)
                    });
            if matches!(
                player.gamemode.load(),
                GameMode::Survival | GameMode::Adventure
            ) && !elder.is_allied_to(player.as_ref() as &dyn EntityBase)
                && player.get_entity().pos.load().squared_distance_to_vec(&pos)
                    < EFFECT_RADIUS * EFFECT_RADIUS
                && needs_effect
            {
                living.add_effect(Effect {
                    effect_type: &StatusEffect::MINING_FATIGUE,
                    duration: EFFECT_DURATION,
                    amplifier: EFFECT_AMPLIFIER,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
                player.try_send_client_packet(&packet);
            }
        }
    }
}

impl Mob for ElderGuardianEntity {
    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityElderGuardianAmbient
        } else {
            Sound::EntityElderGuardianAmbientLand
        })
    }

    /// Vanilla `Guardian.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        160
    }

    /// Vanilla `Guardian.getMaxHeadXRot`.
    fn get_max_look_pitch_change(&self) -> f32 {
        180.0
    }

    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::Events
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn before_mob_ai_step(&self) {
        self.state.ai_step(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        self.state.travel_in_water(self, caller);
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        GuardianState::walk_target_value(self, pos)
    }

    /// Vanilla `ElderGuardian.customServerAiStep`.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let mob_entity = &self.mob_entity;
        let entity = &mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }
        if entity
            .tick_count
            .load(Ordering::Relaxed)
            .wrapping_add(entity.entity_id)
            % EFFECT_INTERVAL
            == 0
        {
            self.curse_players_around();
        }
        if !mob_entity.has_position_target() {
            mob_entity.position_target.store(entity.block_pos.load());
            mob_entity
                .position_target_range
                .store(HOME_RADIUS, Ordering::Relaxed);
        }
    }

    fn pre_damage_with_context(
        &self,
        damage_type: DamageType,
        source: Option<&dyn EntityBase>,
        _cause: Option<&dyn EntityBase>,
    ) -> bool {
        self.state.on_hurt(self, damage_type, source);
        true
    }
}
