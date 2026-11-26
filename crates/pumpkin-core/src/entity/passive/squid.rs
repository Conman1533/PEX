use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossbeam::atomic::AtomicCell;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{Goal, to_goal_ticks},
        util::goal_utils,
    },
    mob::{Mob, MobEntity},
};

/// Represents a Squid, a passive aquatic mob that swims by pulsing its tentacles.
///
/// Wiki: <https://minecraft.wiki/w/Squid>
pub struct SquidEntity {
    pub mob_entity: MobEntity,
    pub movement: Arc<SquidMovement>,
}

impl SquidEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let movement = Arc::new(SquidMovement::new());
        register_squid_goals(&mob_entity, &movement);
        Arc::new(Self {
            mob_entity,
            movement,
        })
    }
}

/// Vanilla `Squid.registerGoals`: a new heading now and then, and fleeing whoever
/// hurt it. Neither takes a control, so both run together, the flight overriding
/// the heading.
pub fn register_squid_goals(mob_entity: &MobEntity, movement: &Arc<SquidMovement>) {
    let goals = &mob_entity.goals_selector;
    goals.add_goal(
        0,
        Box::new(SquidRandomMovementGoal {
            movement: movement.clone(),
        }),
    );
    goals.add_goal(
        1,
        Box::new(SquidFleeGoal {
            movement: movement.clone(),
        }),
    );
}

impl Mob for SquidEntity {
    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn movement_emission(&self) -> crate::entity::MovementEmission {
        crate::entity::MovementEmission::Events
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntitySquidAmbient)
    }

    /// Vanilla `WaterAnimal.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        crate::entity::mob::ANIMAL_AMBIENT_SOUND_INTERVAL
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_ai_step(&self) {
        self.movement.tick(self);
    }

    fn travel(&self, caller: &dyn EntityBase) -> bool {
        SquidMovement::travel(self, caller)
    }

    /// Vanilla `Squid.hurtServer`: inks when an attacker hurts it.
    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if source.is_some() {
            spawn_ink(self, Sound::EntitySquidSquirt, Particle::SquidInk);
        }
    }

    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }
}

/// Vanilla `Squid.spawnInk`: a squirt and a cloud of ink shot out under the squid.
pub fn spawn_ink(mob: &dyn Mob, sound: Sound, particle: Particle) {
    let living = &mob.get_mob_entity().living_entity;
    let entity = &living.entity;
    let world = entity.world.load();
    let pos = entity.pos.load();
    world.play_sound_fine(sound, SoundCategory::Neutral, &pos, 0.4, living.get_pitch());

    // Vanilla `rotateVector`: `xRot` by the body pitch, then `yRot` by minus the body yaw.
    let (pitch_sin, pitch_cos) = f64::from(entity.pitch.load().to_radians()).sin_cos();
    let (yaw_sin, yaw_cos) = f64::from(-entity.body_yaw.load().to_radians()).sin_cos();
    let rotate = |v: Vector3<f64>| {
        let y = v.y * pitch_cos + v.z * pitch_sin;
        let z = v.z * pitch_cos - v.y * pitch_sin;
        Vector3::new(v.x * yaw_cos + z * yaw_sin, y, z * yaw_cos - v.x * yaw_sin)
    };
    let origin = pos + rotate(Vector3::new(0.0, -1.0, 0.0));
    let mut rng = mob.get_random();
    for _ in 0..30 {
        let dir = rotate(Vector3::new(
            f64::from(rng.random::<f32>()) * 0.6 - 0.3,
            -1.0,
            f64::from(rng.random::<f32>()) * 0.6 - 0.3,
        ));
        let offset = dir * f64::from(0.3 + rng.random::<f32>() * 2.0);
        world.spawn_particle(
            Vector3::new(origin.x, origin.y + 0.5, origin.z),
            Vector3::new(offset.x as f32, offset.y as f32, offset.z as f32),
            0.1,
            0,
            particle,
        );
    }
}

/// How vanilla squid get about, which is nothing like other mobs: no navigation,
/// just a heading picked now and then and a shove along it once per tentacle
/// stroke.
pub struct SquidMovement {
    movement_vector: AtomicCell<Vector3<f64>>,
    tentacle_movement: AtomicCell<f32>,
    tentacle_speed: AtomicCell<f32>,
}

impl Default for SquidMovement {
    fn default() -> Self {
        Self::new()
    }
}

impl SquidMovement {
    #[must_use]
    pub fn new() -> Self {
        Self {
            movement_vector: AtomicCell::new(Vector3::default()),
            tentacle_movement: AtomicCell::new(0.0),
            tentacle_speed: AtomicCell::new(random_tentacle_speed(&mut rand::rng())),
        }
    }

    /// The rest of vanilla's `Squid.aiStep`, after the goals and `travel`: a shove
    /// along the heading once per tentacle stroke, or a fall out of water.
    pub fn tick(&self, mob: &dyn Mob) {
        let living = &mob.get_mob_entity().living_entity;
        let entity = &living.entity;
        let in_water = entity.is_in_water();
        let mut rng = mob.get_random();

        let mut tentacle = self.tentacle_movement.load() + self.tentacle_speed.load();
        if tentacle > TAU {
            tentacle -= TAU;
            if rng.random_range(0..10) == 0 {
                self.tentacle_speed.store(random_tentacle_speed(&mut rng));
            }
            // Keeps the client's stroke animation in step with the shoves.
            entity.world.load().send_entity_status(
                entity,
                EntityStatus::SquidAnimSynch,
                ActorEventID::None,
            );
        }
        self.tentacle_movement.store(tentacle);

        if in_water {
            // Plain stores throughout: the tracker syncs motion, and set_velocity
            // would send a packet every tick.
            if tentacle < PI {
                if tentacle / PI > 0.75 {
                    entity.velocity.store(self.movement_vector.load());
                }
            } else {
                entity.velocity.store(entity.velocity.load() * 0.9);
            }
            // The body swings round to face where the squid is going.
            let velocity = entity.velocity.load();
            let heading = -(velocity.x.atan2(velocity.z) as f32).to_degrees();
            let body_yaw = entity.body_yaw.load();
            let body_yaw = body_yaw + (heading - body_yaw) * 0.1;
            entity.body_yaw.store(body_yaw);
            entity.yaw.store(body_yaw);
        } else {
            let fall = entity.velocity.load().y - living.get_attribute_value(&Attributes::GRAVITY);
            entity.velocity.store(Vector3::new(0.0, fall * 0.98, 0.0));
        }
    }

    /// Vanilla's `Squid.travel`: a squid only ever moves by the velocity `tick`
    /// gave it.
    pub fn travel(mob: &dyn Mob, caller: &dyn EntityBase) -> bool {
        mob.get_mob_entity().living_entity.make_move(caller);
        true
    }
}

/// Vanilla `Squid.hasMovementVector`.
fn has_movement_vector(movement: Vector3<f64>) -> bool {
    movement.length_squared() > 1.0e-5
}

/// Vanilla `SquidRandomMovementGoal`: always running, and now and then (or when out
/// of water or still) it picks a new heading. A squid left alone for 100 ticks
/// stops.
struct SquidRandomMovementGoal {
    movement: Arc<SquidMovement>,
}

impl Goal for SquidRandomMovementGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let living = &mob.get_mob_entity().living_entity;
        let mut rng = mob.get_random();
        if living.no_action_time.load(Ordering::Relaxed) > 100 {
            self.movement.movement_vector.store(Vector3::default());
        } else if rng.random_range(0..to_goal_ticks(50)) == 0
            || !living.entity.is_in_water()
            || !has_movement_vector(self.movement.movement_vector.load())
        {
            let angle = rng.random::<f32>() * TAU;
            self.movement.movement_vector.store(Vector3::new(
                f64::from(angle.cos() * 0.2),
                f64::from(-0.1 + rng.random::<f32>() * 0.2),
                f64::from(angle.sin() * 0.2),
            ));
        }
    }
}

/// Vanilla `SquidFleeGoal`: while in water and whoever last hurt the squid is
/// within ten blocks, head straight away from them, every tick.
struct SquidFleeGoal {
    movement: Arc<SquidMovement>,
}

impl SquidFleeGoal {
    /// Vanilla `getLastHurtByMob`: whoever last hurt the squid, forgotten 100 ticks
    /// after the hit.
    fn attacker(mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let living = &mob.get_mob_entity().living_entity;
        let entity = &living.entity;
        let id = living.last_attacker_id.load(Ordering::Relaxed);
        let since_hit = entity.tick_count.load(Ordering::Relaxed)
            - living.last_attacked_time.load(Ordering::Relaxed);
        if id == 0 || since_hit > 100 {
            return None;
        }
        entity.world.load().get_entity_by_id(id)
    }
}

impl Goal for SquidFleeGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        entity.is_in_water()
            && Self::attacker(mob).is_some_and(|attacker| {
                entity
                    .pos
                    .load()
                    .squared_distance_to_vec(&attacker.get_entity().pos.load())
                    < 100.0
            })
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(attacker) = Self::attacker(mob) else {
            return;
        };
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = entity.pos.load();
        let mut away = pos - attacker.get_entity().pos.load();
        let probe = BlockPos::floored(pos.x + away.x, pos.y + away.y, pos.z + away.z);
        let open_air = world.get_block_state(&probe).is_air();
        if goal_utils::is_water(&world, &probe) || open_air {
            let length = away.length();
            if length > 0.0 {
                // Vanilla normalizes a copy of the offset here and discards it,
                // so the scale applies to the raw offset.
                let mut scale = 3.0;
                if length > 5.0 {
                    scale -= (length - 5.0) / 5.0;
                }
                if scale > 0.0 {
                    away = away * scale;
                }
            }
            if open_air {
                away.y = 0.0;
            }
            self.movement.movement_vector.store(away / 20.0);
        }
        // Vanilla also adds a bubble every 10 ticks of flight, through
        // `Level.addParticle`, which does nothing on the server.
    }
}

fn random_tentacle_speed(rng: &mut impl RngExt) -> f32 {
    1.0 / (rng.random::<f32>() + 1.0) * 0.2
}
