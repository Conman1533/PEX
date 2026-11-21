use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam::atomic::AtomicCell;

use crate::entity::projectile::ProjectileHit;
use crate::{
    entity::{
        Entity, EntityBase, EntityType, mob::endermite::EndermiteEntity,
        projectile::ThrownItemEntity,
    },
    server::Server,
};
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::{EntityPose, EntityStatus};
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_util::math::vector3::Vector3;
use uuid::Uuid;

const GRAVITY: f64 = 0.03;
const PARTICLE_COUNT: i32 = 32;
const ENDERMITE_SPAWN_CHANCE: f32 = 0.05;

/// Every key the pearl's `write_nbt` can write (`Entity::write_nbt` and its own). A loaded key
/// outside this list is kept and written back, as D7-1 decided.
const MODELED_PEARL_KEYS: &[&str] = &[
    "id",
    "UUID",
    "Pos",
    "Motion",
    "Rotation",
    "Air",
    "Fire",
    "OnGround",
    "Invulnerable",
    "PortalCooldown",
    "HasVisualFire",
    "TicksFrozen",
    "CustomName",
    "CustomNameVisible",
    "Tags",
    "PumpkinCustomData",
    "leash",
    "NoGravity",
    "Owner",
    "LeftOwner",
    "HasBeenShot",
    // The player file adds this to its pearls; it is never the pearl's own.
    crate::entity::player::saved_keys::ENDER_PEARL_DIMENSION,
];

pub struct EnderPearlEntity {
    pub thrown: ThrownItemEntity,
    /// Vanilla `Projectile.owner`'s UUID, saved as `Owner`.
    owner_uuid: AtomicCell<Option<Uuid>>,
    /// Vanilla `Projectile.hasBeenShot`: set on the first tick.
    has_been_shot: AtomicBool,
    /// Loaded keys PEX doesn't model, written back unchanged.
    kept_nbt: Mutex<NbtCompound>,
}

impl EnderPearlEntity {
    pub fn new(entity: Entity) -> Self {
        entity.set_velocity(Vector3::new(0.0, 0.1, 0.0));

        let thrown = ThrownItemEntity {
            entity,
            owner_id: None,
            collides_with_projectiles: false,
            has_hit: AtomicBool::new(false),
            left_owner: AtomicBool::new(false),
            gravity: GRAVITY,
        };

        Self {
            thrown,
            owner_uuid: AtomicCell::new(None),
            has_been_shot: AtomicBool::new(false),
            kept_nbt: Mutex::new(NbtCompound::new()),
        }
    }

    pub fn new_shot(entity: Entity, shooter: &Entity) -> Self {
        let thrown = ThrownItemEntity::new(entity, shooter, GRAVITY);
        thrown.entity.set_velocity(Vector3::new(0.0, 0.1, 0.0));
        Self {
            thrown,
            owner_uuid: AtomicCell::new(Some(shooter.entity_uuid)),
            has_been_shot: AtomicBool::new(false),
            kept_nbt: Mutex::new(NbtCompound::new()),
        }
    }

    pub fn set_owner_uuid(&self, owner: Uuid) {
        self.owner_uuid.store(Some(owner));
    }

    /// A pearl coming back from its owner's player file (vanilla
    /// `ServerPlayer.loadAndSpawnEnderPearl`), still thrown by that player.
    pub fn new_owned(entity: Entity, owner_id: i32, owner_uuid: Uuid) -> Self {
        Self {
            thrown: ThrownItemEntity {
                entity,
                owner_id: Some(owner_id),
                collides_with_projectiles: false,
                has_hit: AtomicBool::new(false),
                left_owner: AtomicBool::new(false),
                gravity: GRAVITY,
            },
            owner_uuid: AtomicCell::new(Some(owner_uuid)),
            has_been_shot: AtomicBool::new(false),
            kept_nbt: Mutex::new(NbtCompound::new()),
        }
    }
}

impl EntityBase for EnderPearlEntity {
    fn get_owner_id(&self) -> Option<i32> {
        self.thrown.owner_id
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        self.has_been_shot.store(true, Ordering::Relaxed);
        self.thrown.process_tick(caller);
        // Vanilla `ThrownEnderpearl.tick`: a player's pearl keeps its chunk loaded.
        let entity = self.get_entity();
        if self.thrown.owner_id.is_some() && entity.is_alive() {
            entity
                .world
                .load()
                .place_ender_pearl_ticket(entity.chunk_pos.load());
        }
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

    /// Vanilla `Projectile.addAdditionalSaveData` and `ThrowableItemProjectile`'s `Item`.
    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        let entity = self.get_entity();
        if entity.has_no_gravity() {
            nbt.put_bool("NoGravity", true);
        }
        if let Some(owner) = self.owner_uuid.load() {
            nbt.put_uuid("Owner", owner);
        }
        if self.thrown.left_owner.load(Ordering::Relaxed) {
            nbt.put_bool("LeftOwner", true);
        }
        nbt.put_bool("HasBeenShot", self.has_been_shot.load(Ordering::Relaxed));
        let kept = self
            .kept_nbt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::entity::player::saved_keys::write_kept_keys(nbt, &kept);
        // Not modelled: a pearl PEX threw holds a plain ender pearl.
        if !nbt.has("Item") {
            let mut item = NbtCompound::new();
            item.put_string("id", "minecraft:ender_pearl".to_string());
            item.put_int("count", 1);
            nbt.put("Item", NbtTag::Compound(item));
        }
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        if let Some(no_gravity) = nbt.get_bool("NoGravity") {
            self.get_entity().set_has_no_gravity(no_gravity);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner_uuid.store(Some(owner));
        }
        if let Some(shot) = nbt.get_bool("HasBeenShot") {
            self.has_been_shot.store(shot, Ordering::Relaxed);
        }
        if let Some(left_owner) = nbt.get_bool("LeftOwner") {
            self.thrown.left_owner.store(left_owner, Ordering::Relaxed);
        }
        *self
            .kept_nbt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            crate::entity::player::saved_keys::unmodeled_keys(nbt, MODELED_PEARL_KEYS);
    }

    fn on_hit(&self, hit: ProjectileHit) {
        let entity = self.get_entity();
        let world = entity.world.load();

        let attacker = self
            .thrown
            .owner_id
            .and_then(|id| world.get_entity_by_id(id));

        // Spawn portal particles at hit position
        let hit_pos = hit.hit_pos();
        for _ in 0..PARTICLE_COUNT {
            let offset = Vector3::new(
                rand::random::<f32>() as f64 - 0.5,
                rand::random::<f32>() as f64 * 2.0,
                rand::random::<f32>() as f64 - 0.5,
            );
            let speed = Vector3::new(
                rand::random::<f64>() - 0.5,
                0.0,
                rand::random::<f64>() - 0.5,
            );

            world.spawn_particle(
                hit_pos.add(&offset),
                Vector3::new(speed.x as f32, speed.y as f32, speed.z as f32),
                1.0,
                1,
                Particle::Portal,
            );
        }

        let owner_id = self.thrown.owner_id;
        let teleport_pos = entity.last_pos.load();

        if let (
            ProjectileHit::Entity {
                entity: hit_entity,
                hit_pos,
                ..
            },
            Some(owner),
        ) = (&hit, attacker)
        {
            let victim_ref = &**hit_entity;
            hit_entity.damage_with_context(
                victim_ref,
                0.0,
                DamageType::THROWN,
                Some(*hit_pos),
                Some(owner.get_entity()),
                Some(victim_ref),
            );
        }

        if let Some(owner_id) = owner_id
            && let Some(owner) = world.get_entity_by_id(owner_id)
            && owner.get_entity().is_alive()
            && owner.get_living_entity().is_none_or(|living| {
                living.health.load() > 0.0 && owner.get_entity().pose.load() != EntityPose::Sleeping
            })
        {
            let should_spawn_endermite = rand::random::<f32>() < ENDERMITE_SPAWN_CHANCE;
            if world.should_spawn_monsters() && should_spawn_endermite {
                let entity = Entity::new(
                    world.clone(),
                    owner.get_entity().pos.load(),
                    &EntityType::ENDERMITE,
                );
                let endermite = EndermiteEntity::new(entity);
                world.spawn_entity(endermite);
            }

            // In vanilla, teleport handles everything including sound
            owner.teleport(
                teleport_pos,
                Some(owner.get_entity().yaw.load()),
                Some(owner.get_entity().pitch.load()),
                world.clone(),
            );

            // Play teleport sound at new position
            world.play_sound(
                Sound::EntityPlayerTeleport,
                SoundCategory::Players,
                &teleport_pos,
            );

            // Deal 5 damage to owner
            owner.damage(
                owner.as_ref(),
                5.0,
                pumpkin_data::damage::DamageType::ENDER_PEARL,
            );
        }

        world.send_entity_status(entity, EntityStatus::Death, ActorEventID::Death);
    }
}
