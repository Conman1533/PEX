use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::vector3::Vector3;

use crate::{
    entity::{
        Entity, EntityBase,
        area_effect_cloud::AreaEffectCloudEntity,
        projectile::{
            ProjectileHit, ThrownItemEntity,
            fireball::{AIR_INERTIA, INITIAL_ACCELERATION_POWER, WATER_INERTIA},
        },
    },
    server::Server,
};

/// Vanilla `DragonFireball.SPLASH_RANGE`: a living entity this close to the impact gets the cloud.
pub const SPLASH_RANGE: f64 = 4.0;
const CLOUD_RADIUS: f32 = 3.0;
const CLOUD_MAX_RADIUS: f32 = 7.0;
const CLOUD_DURATION: i32 = 600;

/// The dragon's fireball from the strafe phase. It leaves a growing cloud of dragon's breath
/// where it lands.
pub struct DragonFireballEntity {
    pub thrown: ThrownItemEntity,
    pub acceleration_power: AtomicU64,
}

impl DragonFireballEntity {
    #[must_use]
    pub const fn new(entity: Entity) -> Self {
        Self {
            thrown: ThrownItemEntity {
                entity,
                owner_id: None,
                collides_with_projectiles: false,
                has_hit: AtomicBool::new(false),
                gravity: 0.0,
            },
            acceleration_power: AtomicU64::new(INITIAL_ACCELERATION_POWER.to_bits()),
        }
    }

    /// Vanilla `new DragonFireball(level, mob, direction)`.
    #[must_use]
    pub fn new_shot(entity: Entity, shooter: &Entity, direction: Vector3<f64>) -> Self {
        let thrown = ThrownItemEntity::new(entity, shooter, 0.0);
        let accel = INITIAL_ACCELERATION_POWER;
        thrown
            .entity
            .velocity
            .store(direction.normalize().multiply(accel, accel, accel));
        Self {
            thrown,
            acceleration_power: AtomicU64::new(accel.to_bits()),
        }
    }

    fn acceleration_power(&self) -> f64 {
        f64::from_bits(self.acceleration_power.load(Ordering::Relaxed))
    }

    /// Vanilla `DragonFireball.onHit` on the server, minus the owner check, which
    /// `ThrownItemEntity` makes by never hitting the dragon.
    fn spawn_breath_cloud(&self, hit_pos: Vector3<f64>) {
        let entity = self.get_entity();
        let world = entity.world.load_full();
        let dimension = entity.entity_dimension.load();
        let splash_box = BoundingBox::new_from_pos(hit_pos.x, hit_pos.y, hit_pos.z, &dimension)
            .expand(SPLASH_RANGE, 2.0, SPLASH_RANGE);

        let mut nearby = world.get_entities_at_box(&splash_box);
        nearby.extend(
            world
                .get_players_at_box(&splash_box)
                .into_iter()
                .map(|player| player as std::sync::Arc<dyn EntityBase>),
        );
        let cloud_pos = nearby
            .iter()
            .filter(|nearby| nearby.get_living_entity().is_some())
            .map(|nearby| nearby.get_entity().pos.load())
            .find(|pos| pos.squared_distance_to_vec(&hit_pos) < SPLASH_RANGE * SPLASH_RANGE)
            .unwrap_or(hit_pos);

        let cloud = AreaEffectCloudEntity::dragon_breath(
            Entity::new(world.clone(), cloud_pos, &EntityType::AREA_EFFECT_CLOUD),
            (&StatusEffect::INSTANT_DAMAGE, 1, 1, false, true, true),
            CLOUD_DURATION,
            CLOUD_RADIUS,
            (CLOUD_MAX_RADIUS - CLOUD_RADIUS) / CLOUD_DURATION as f32,
        );
        world.sync_world_event(
            WorldEvent::ParticlesDragonFireballSplash,
            hit_pos.to_block_pos(),
            if entity.is_silent() { -1 } else { 1 },
        );
        world.spawn_entity(cloud);
    }
}

impl EntityBase for DragonFireballEntity {
    fn get_owner_id(&self) -> Option<i32> {
        self.thrown.owner_id
    }

    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_double("acceleration_power", self.acceleration_power());
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        if let Some(accel) = nbt.get_double("acceleration_power") {
            self.acceleration_power
                .store(accel.to_bits(), Ordering::Relaxed);
        }
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        let entity = self.get_entity();
        let velocity = entity.velocity.load();
        let inertia = if entity.touching_water.load(Ordering::Relaxed) {
            WATER_INERTIA
        } else {
            AIR_INERTIA
        };
        let accel = self.acceleration_power();
        if velocity.length() > 1e-6 {
            let velocity = velocity
                .normalize()
                .multiply(accel, accel, accel)
                .add(&velocity)
                .multiply(inertia, inertia, inertia);
            entity.velocity.store(velocity);
        }

        self.thrown.process_tick(caller);
    }

    fn get_entity(&self) -> &Entity {
        self.thrown.get_entity()
    }

    fn get_living_entity(&self) -> Option<&crate::entity::living::LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }

    fn on_hit(&self, hit: ProjectileHit) {
        self.spawn_breath_cloud(hit.hit_pos());
    }
}
