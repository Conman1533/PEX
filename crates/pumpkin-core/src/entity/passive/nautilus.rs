use crossbeam::atomic::AtomicCell;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
};
use uuid::Uuid;

use pumpkin_data::{
    attributes::Attributes,
    damage::DamageType,
    effect::StatusEffect,
    entity::EntityStatus,
    item_stack::ItemStack,
    particle::Particle,
    potion::Effect,
    sound::{Sound, SoundCategory},
    tag::{self, Taggable},
    zombie_nautilus_variant::ZombieNautilusVariant,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::{
        control::{
            smooth_swimming_look_control::SmoothSwimmingLookControl,
            smooth_swimming_move_control::SmoothSwimmingMoveControl,
        },
        goal::{
            Controls, Goal, breed::BreedGoal, charge_attack::ChargeAttackGoal,
            escape_danger::EscapeDangerGoal, random_swimming::RandomSwimmingGoal, tempt::TemptGoal,
        },
        pathfinder::Navigator,
        target_predicate::TargetPredicate,
    },
    custom_sound::CustomSound,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;

/// Vanilla `NautilusAi.ANGER_DURATION`.
const ANGER_DURATION: i64 = 400;
/// Vanilla `NautilusAi.TIME_BETWEEN_NON_PLAYER_ATTACKS`.
const NON_PLAYER_ATTACK_COOLDOWN: std::ops::RangeInclusive<i64> = 2400..=3600;
/// Vanilla `NautilusAi.TIME_BETWEEN_ATTACKS`.
const TIME_BETWEEN_ATTACKS: i64 = 80;

/// The nautilus and the zombie nautilus (vanilla `AbstractNautilus`).
pub struct NautilusEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub is_tame: AtomicBool,
    pub owner: AtomicCell<Option<Uuid>>,
    pub is_dashing: AtomicBool,
    pub dash_cooldown: AtomicI32,
    pub is_saddled: AtomicBool,
    pub inventory: Mutex<Vec<ItemStack>>,
    /// A zombie nautilus.
    pub zombie: bool,
    /// The zombie nautilus variant id.
    pub variant: AtomicU8,
    /// Vanilla `ANGRY_AT` memory: who hurt it, until when (world age).
    angry_at: AtomicCell<Option<(Uuid, i64)>>,
}

impl NautilusEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, false)
    }

    pub fn new_zombie(entity: Entity) -> Arc<Self> {
        Self::with_kind(entity, true)
    }

    fn with_kind(entity: Entity, zombie: bool) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        // Vanilla `AbstractNautilus`: smooth swimming controls and water-bound navigation.
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(SmoothSwimmingMoveControl::new(85, 10, 0.011, 0.0, true));
        *mob_entity
            .look_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(SmoothSwimmingLookControl::new(10));
        *mob_entity
            .navigator
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::water_bound(false);
        let nautilus = Arc::new(Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            is_tame: AtomicBool::new(false),
            owner: AtomicCell::new(None),
            is_dashing: AtomicBool::new(false),
            dash_cooldown: AtomicI32::new(0),
            is_saddled: AtomicBool::new(false),
            inventory: Mutex::new(vec![ItemStack::new(0, &pumpkin_data::item::Item::AIR); 9]),
            zombie,
            variant: AtomicU8::new(ZombieNautilusVariant::default().id()),
            angry_at: AtomicCell::new(None),
        });

        // Vanilla `NautilusAi` and `ZombieNautilusAi`, whose Brain activities map onto goals.
        // The dash sets the velocity every tick, so it wins over the core panic; the target
        // goal finds nobody while the nautilus breeds.
        let goals = &nautilus.mob_entity.goals_selector;
        let (tempt_speed, charge_speed, dash_sound) = if zombie {
            (0.9, 0.5, Sound::EntityZombieNautilusDash)
        } else {
            (1.3, 0.6, Sound::EntityNautilusDash)
        };
        if !zombie {
            goals.add_goal(2, BreedGoal::new(0.4));
        }
        goals.add_goal(
            3,
            Box::new(TemptGoal::new(
                tempt_speed,
                |_, stack| {
                    stack
                        .get_item()
                        .has_tag(&tag::Item::MINECRAFT_NAUTILUS_FOOD)
                },
                false,
            )),
        );
        goals.add_goal(
            0,
            Box::new(ChargeAttackGoal::new(
                TIME_BETWEEN_ATTACKS,
                charge_speed,
                2.0,
                12.0,
                11.0,
                dash_sound,
            )),
        );
        if !zombie {
            goals.add_goal(1, EscapeDangerGoal::new(1.6));
        }
        goals.add_goal(5, Box::new(RandomSwimmingGoal::new(1.0, 10)));
        nautilus
            .mob_entity
            .target_selector
            .add_goal(1, Box::new(NautilusTargetGoal::default()));

        nautilus
    }

    #[must_use]
    pub fn get_variant(&self) -> ZombieNautilusVariant {
        ZombieNautilusVariant::from_id(self.variant.load(Ordering::Relaxed)).unwrap_or_default()
    }

    pub fn set_variant(&self, variant: ZombieNautilusVariant) {
        self.variant.store(variant.id(), Ordering::Relaxed);
        if self.zombie {
            self.get_entity().set_synced_data(
                pumpkin_data::tracked_data::zombie_nautilus::DATA_VARIANT_ID,
                VarInt(i32::from(variant.id())),
            );
        }
    }

    fn is_angry_at(&self, world: &World) -> Option<Arc<dyn EntityBase>> {
        let (uuid, until) = self.angry_at.load()?;
        if world.get_world_age() >= until {
            self.angry_at.store(None);
            return None;
        }
        world.get_entity_by_uuid(uuid)
    }

    pub fn is_dashing(&self) -> bool {
        self.is_dashing.load(Ordering::Relaxed)
    }

    pub fn set_dashing(&self, dashing: bool) {
        self.is_dashing.store(dashing, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::nautilus::DASH, dashing);
    }

    pub fn is_tame(&self) -> bool {
        self.is_tame.load(Ordering::Relaxed)
    }

    pub fn set_tame(&self, tame: bool, owner: Option<Uuid>) {
        self.is_tame.store(tame, Ordering::Relaxed);
        self.owner.store(owner);
    }

    pub fn get_dash_sound(&self) -> Sound {
        let is_water = self
            .mob_entity
            .living_entity
            .entity
            .touching_water
            .load(Ordering::Relaxed);
        if self.zombie {
            return if is_water {
                Sound::EntityZombieNautilusDash
            } else {
                Sound::EntityZombieNautilusDashLand
            };
        }
        if is_water {
            Sound::EntityNautilusDash
        } else {
            Sound::EntityNautilusDashLand
        }
    }

    pub fn get_dash_ready_sound(&self) -> Sound {
        let is_water = self
            .mob_entity
            .living_entity
            .entity
            .touching_water
            .load(Ordering::Relaxed);
        if self.zombie {
            return if is_water {
                Sound::EntityZombieNautilusDashReady
            } else {
                Sound::EntityZombieNautilusDashReadyLand
            };
        }
        if is_water {
            Sound::EntityNautilusDashReady
        } else {
            Sound::EntityNautilusDashReadyLand
        }
    }

    pub fn get_eat_sound(&self) -> Sound {
        if self.zombie {
            return Sound::EntityZombieNautilusEat;
        }
        let is_baby = self
            .mob_entity
            .living_entity
            .entity
            .age
            .load(Ordering::Relaxed)
            < 0;
        if is_baby {
            Sound::EntityBabyNautilusEat
        } else {
            Sound::EntityNautilusEat
        }
    }

    pub fn get_swim_sound(&self) -> Sound {
        if self.zombie {
            return Sound::EntityZombieNautilusSwim;
        }
        let is_baby = self
            .mob_entity
            .living_entity
            .entity
            .age
            .load(Ordering::Relaxed)
            < 0;
        if is_baby {
            Sound::EntityBabyNautilusSwim
        } else {
            Sound::EntityNautilusSwim
        }
    }
}

impl Animal for NautilusEntity {
    /// Vanilla `AbstractNautilus.isFood`: taming items while wild and grown, food otherwise.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        use pumpkin_data::tag::{self, Taggable};
        let tag = if !self.is_tame() && !self.is_baby() {
            &tag::Item::MINECRAFT_NAUTILUS_TAMING_ITEMS
        } else {
            &tag::Item::MINECRAFT_NAUTILUS_FOOD
        };
        item_stack.get_item().has_tag(tag)
    }

    fn play_eating_sound(&self) {
        self.mob_entity
            .living_entity
            .make_sound(self.get_eat_sound());
    }

    /// Vanilla `ZombieNautilus.getBreedOffspring` is null.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        !self.zombie
            && partner.get_entity().entity_id != self.get_entity().entity_id
            && partner
                .cast_any()
                .downcast_ref::<Self>()
                .is_some_and(|other| !other.zombie)
            && self.get_mob_entity().is_in_love()
            && partner.get_mob_entity().is_in_love()
    }
}

impl CustomSound for NautilusEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        if self.zombie {
            return Some(if entity.is_under_water() {
                Sound::EntityZombieNautilusHurt
            } else {
                Sound::EntityZombieNautilusHurtLand
            });
        }
        Some(if is_baby {
            if is_water {
                Sound::EntityBabyNautilusHurt
            } else {
                Sound::EntityBabyNautilusHurtLand
            }
        } else if is_water {
            Sound::EntityNautilusHurt
        } else {
            Sound::EntityNautilusHurtLand
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        let is_water = entity.touching_water.load(Ordering::Relaxed);
        if self.zombie {
            return Some(if entity.is_under_water() {
                Sound::EntityZombieNautilusDeath
            } else {
                Sound::EntityZombieNautilusDeathLand
            });
        }
        Some(if is_baby {
            if is_water {
                Sound::EntityBabyNautilusDeath
            } else {
                Sound::EntityBabyNautilusDeathLand
            }
        } else if is_water {
            Sound::EntityNautilusDeath
        } else {
            Sound::EntityNautilusDeathLand
        })
    }
}

impl AgeableMob for NautilusEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    /// Vanilla `ZombieNautilus.canBeABaby`.
    fn can_be_a_baby(&self) -> bool {
        !self.zombie
    }
}

impl Mob for NautilusEntity {
    fn get_ambient_sound(&self) -> Option<Sound> {
        let under_water = self.get_entity().is_under_water();
        if self.zombie {
            return Some(if under_water {
                Sound::EntityZombieNautilusAmbient
            } else {
                Sound::EntityZombieNautilusAmbientLand
            });
        }
        Some(match (self.is_baby(), under_water) {
            (true, true) => Sound::EntityBabyNautilusAmbient,
            (true, false) => Sound::EntityBabyNautilusAmbientLand,
            (false, true) => Sound::EntityNautilusAmbient,
            (false, false) => Sound::EntityNautilusAmbientLand,
        })
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn spawn_as_baby(&self) -> bool {
        !self.zombie && {
            self.set_baby(true);
            true
        }
    }

    /// Vanilla `ZombieNautilus.finalizeSpawn` picks the variant for the biome.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if self.zombie {
            let biome = world.get_biome(&self.get_entity().block_pos.load());
            self.set_variant(ZombieNautilusVariant::select_for_biome(biome.registry_id));
        }
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    /// Vanilla `ZombieNautilus.sunProtectionSlot`: its armor shades it.
    fn sun_protection_slot(&self) -> pumpkin_data::data_component_impl::EquipmentSlot {
        if self.zombie {
            pumpkin_data::data_component_impl::EquipmentSlot::BODY
        } else {
            pumpkin_data::data_component_impl::EquipmentSlot::HEAD
        }
    }

    fn mob_set_variant_name(&self, name: &str) {
        if self.zombie
            && let Some(variant) = ZombieNautilusVariant::from_name(name)
        {
            self.set_variant(variant);
        }
    }

    /// Vanilla `AbstractNautilus.hurtServer`: angry at a living attacker for 400 ticks.
    fn on_damage(&self, _damage_type: DamageType, _source: Option<&dyn EntityBase>) {
        let living = &self.mob_entity.living_entity;
        let world = living.entity.world.load();
        let Some(attacker) = living
            .get_last_damage_source()
            .and_then(|(_, attacker_id)| world.get_entity_by_id(attacker_id))
        else {
            return;
        };
        let targeting = TargetPredicate::create_attackable().ignore_visibility();
        if attacker.get_living_entity().is_some()
            && targeting.test(&world, Some(self), attacker.as_ref())
        {
            self.angry_at.store(Some((
                attacker.get_entity().entity_uuid,
                world.get_world_age() + ANGER_DURATION,
            )));
        }
    }

    /// Vanilla `AbstractNautilus.travelInWater`.
    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let living = &self.mob_entity.living_entity;
        let entity = &living.entity;
        entity.update_velocity_from_input(
            living.movement_input.load(),
            f64::from(living.get_speed()),
        );
        living.make_move(caller);
        entity.velocity.store(entity.velocity.load() * 0.9);
    }

    /// Vanilla `AbstractNautilus.removeWhenFarAway`: despawns although it is an animal.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        true
    }

    fn as_custom_sound(&self) -> Option<&dyn crate::entity::custom_sound::CustomSound> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        if self.zombie {
            nbt.put_string(
                "variant",
                format!("minecraft:{}", self.get_variant().to_name()),
            );
        }
        nbt.put_bool("IsTame", self.is_tame.load(Ordering::Relaxed));
        nbt.put_bool("Saddled", self.is_saddled.load(Ordering::Relaxed));
        nbt.put_int("DashCooldown", self.dash_cooldown.load(Ordering::Relaxed));
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("Owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if self.zombie
            && let Some(variant) = nbt
                .get_string("variant")
                .and_then(ZombieNautilusVariant::from_name)
        {
            self.variant.store(variant.id(), Ordering::Relaxed);
        }
        if let Some(is_tame) = nbt.get_bool("IsTame") {
            self.is_tame.store(is_tame, Ordering::Relaxed);
        }
        if let Some(saddled) = nbt.get_bool("Saddled") {
            self.is_saddled.store(saddled, Ordering::Relaxed);
        }
        if let Some(dash) = nbt.get_int("DashCooldown") {
            self.dash_cooldown.store(dash, Ordering::Relaxed);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner.store(Some(owner));
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        self.mob_entity.living_entity.entity.set_synced_data(
            pumpkin_data::tracked_data::nautilus::DASH,
            self.is_dashing(),
        );
        if self.zombie {
            self.set_variant(self.get_variant());
        } else if self.is_baby() {
            self.get_entity()
                .set_synced_data(pumpkin_data::tracked_data::nautilus::DATA_BABY_ID, true);
        }
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;

        let Ok(passengers) = entity.passengers.try_lock() else {
            return;
        };
        if let Some(passenger) = passengers.first()
            && let Some(player) = passenger.cast_any().downcast_ref::<Player>()
        {
            let world = entity.world.load();
            let game_time = world.get_world_age();
            if game_time % 40 == 0 {
                let player_arc = world.get_player_by_uuid(player.gameprofile.id);
                if let Some(p) = player_arc {
                    p.add_effect(Effect {
                        effect_type: &StatusEffect::BREATH_OF_THE_NAUTILUS,
                        duration: 60,
                        amplifier: 0,
                        ambient: true,
                        show_particles: true,
                        show_icon: true,
                        blend: true,
                    });
                }
            }
        }

        if self.is_dashing() && self.dash_cooldown.load(Ordering::Relaxed) < 35 {
            self.set_dashing(false);
        }

        let cooldown = self.dash_cooldown.load(Ordering::Relaxed);
        if cooldown > 0 {
            let next = cooldown - 1;
            self.dash_cooldown.store(next, Ordering::Relaxed);
            if next == 0 {
                let world = entity.world.load();
                world.play_sound(
                    self.get_dash_ready_sound(),
                    SoundCategory::Neutral,
                    &entity.pos.load(),
                );
            }
        }

        if entity.touching_water.load(Ordering::Relaxed) {
            let velo = entity.velocity.load();
            let speed = velo.length();
            let prob = (speed * 2.0).clamp(0.15, 1.0);
            if rand::random::<f64>() < prob {
                let world = entity.world.load();
                let pos = entity.pos.load();
                world.spawn_particle(
                    pos + Vector3::new(0.0, 0.25, 0.0),
                    Vector3::new(0.4, 0.4, 0.4),
                    0.5,
                    2,
                    Particle::Bubble,
                );
            }
        }
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let mob_entity = &self.mob_entity;
        let entity = &mob_entity.living_entity.entity;

        if !self.is_tame() && self.is_food(item_stack) {
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            if rand::random::<u32>().is_multiple_of(3) {
                self.set_tame(true, Some(player.gameprofile.id));
                let world = entity.world.load();
                world.send_entity_status(
                    entity,
                    EntityStatus::TamingSucceeded,
                    ActorEventID::TamingSucceeded,
                );
            } else {
                let world = entity.world.load();
                world.send_entity_status(
                    entity,
                    EntityStatus::TamingFailed,
                    ActorEventID::TamingFailed,
                );
            }
            let world = entity.world.load();
            world.play_sound(
                self.get_eat_sound(),
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            return true;
        }

        if self.is_tame() && !player.get_entity().is_sneaking() {
            if !self.is_saddled.load(Ordering::Relaxed)
                && item_stack.item == &pumpkin_data::item::Item::SADDLE
            {
                item_stack.decrement_unless_creative(player.gamemode.load(), 1);
                self.is_saddled.store(true, Ordering::Relaxed);
                let world = entity.world.load();
                world.play_sound(
                    Sound::ItemNautilusSaddleEquip,
                    SoundCategory::Neutral,
                    &entity.pos.load(),
                );
                return true;
            }

            let world = player.world();
            if let Some(vehicle) = world.get_entity_by_id(entity.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                entity.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack)
    }

    fn is_saddled(&self) -> bool {
        self.is_saddled.load(Ordering::Relaxed)
    }

    fn can_be_saddled(&self) -> bool {
        self.mob_entity.living_entity.entity.is_alive()
    }

    fn set_saddled(&self, saddled: bool) {
        self.is_saddled.store(saddled, Ordering::Relaxed);
    }
}

/// Vanilla `NautilusAi.findNearestValidAttackTarget`, run by `StartAttacking`: whoever hurt it,
/// or now and then (half the time, every 2400 to 3600 ticks) the nearest hostile in the water.
#[derive(Default)]
struct NautilusTargetGoal {
    cooldown_until: Option<i64>,
    target: Option<Arc<dyn EntityBase>>,
}

impl NautilusTargetGoal {
    fn find(&mut self, nautilus: &NautilusEntity) -> Option<Arc<dyn EntityBase>> {
        let entity = nautilus.get_entity();
        if nautilus.mob_entity.is_in_love()
            || !entity.is_in_water()
            || nautilus.is_baby()
            || nautilus.is_tame()
        {
            return None;
        }
        let world = entity.world.load();
        let age = world.get_world_age();
        let attackable = TargetPredicate::create_attackable().ignore_visibility();
        if let Some(angry_at) = nautilus.is_angry_at(&world)
            && angry_at.get_entity().is_in_water()
            && attackable.test(&world, Some(nautilus), angry_at.as_ref())
        {
            return Some(angry_at);
        }
        let mut rng = rand::rng();
        // Vanilla `initMemories` starts the cooldown when the brain is made.
        let cooldown_until = *self
            .cooldown_until
            .get_or_insert_with(|| age + rng.random_range(NON_PLAYER_ATTACK_COOLDOWN));
        if age < cooldown_until {
            return None;
        }
        self.cooldown_until = Some(age + rng.random_range(NON_PLAYER_ATTACK_COOLDOWN));
        if rng.random::<f32>() < 0.5 {
            return None;
        }
        // Vanilla `NEAREST_VISIBLE_LIVING_ENTITIES`: within the follow range and in sight.
        let pos = entity.pos.load();
        let range = nautilus
            .mob_entity
            .living_entity
            .get_attribute_value(&Attributes::FOLLOW_RANGE);
        world
            .get_nearby_entities(pos, range)
            .into_values()
            .filter(|other| {
                let other_entity = other.get_entity();
                other_entity.entity_id != entity.entity_id
                    && other.get_living_entity().is_some()
                    && other_entity.is_in_water()
                    && other_entity
                        .entity_type
                        .has_tag(&tag::EntityType::MINECRAFT_NAUTILUS_HOSTILES)
                    && nautilus.has_line_of_sight(other_entity)
            })
            .min_by(|a, b| {
                let da = a.get_entity().pos.load().squared_distance_to_vec(&pos);
                let db = b.get_entity().pos.load().squared_distance_to_vec(&pos);
                da.total_cmp(&db)
            })
    }
}

impl Goal for NautilusTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_mob_entity().get_target().is_some() {
            return false;
        }
        let Some(nautilus) = mob.cast_any().downcast_ref::<NautilusEntity>() else {
            return false;
        };
        self.target = self.find(nautilus);
        self.target.is_some()
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().set_target(self.target.take());
    }

    fn controls(&self) -> Controls {
        Controls::TARGET
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_data::entity::EntityType;
    use pumpkin_data::zombie_nautilus_variant::ZombieNautilusVariant;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_util::math::vector3::Vector3;

    use super::{NautilusEntity, TIME_BETWEEN_ATTACKS};
    use crate::entity::EntityBase;
    use crate::entity::ageable::AgeableMob;
    use crate::entity::ai::goal::{Goal, charge_attack::ChargeAttackGoal};
    use crate::entity::mob::Mob;
    use crate::entity::r#type::from_type;
    use crate::world::test_world;

    fn spawn(entity_type: &'static EntityType, x: f64) -> Arc<dyn EntityBase> {
        from_type(
            entity_type,
            Vector3::new(x, 64.0, 0.5),
            &test_world(),
            uuid::Uuid::new_v4(),
        )
    }

    #[tokio::test]
    async fn zombie_nautilus_is_a_nautilus_that_is_never_a_baby() {
        let zombie = spawn(&EntityType::ZOMBIE_NAUTILUS, 0.5);
        let zombie = zombie
            .cast_any()
            .downcast_ref::<NautilusEntity>()
            .expect("a nautilus entity");
        assert!(zombie.zombie);
        assert!(!zombie.can_be_a_baby());
        assert!(!zombie.spawn_as_baby());
        assert!(!zombie.is_baby());

        let nautilus = spawn(&EntityType::NAUTILUS, 0.5);
        let nautilus = nautilus
            .cast_any()
            .downcast_ref::<NautilusEntity>()
            .expect("a nautilus entity");
        assert!(!nautilus.zombie);
        assert!(nautilus.spawn_as_baby());
    }

    #[tokio::test]
    async fn zombie_nautilus_variant_survives_a_save() {
        let zombie = spawn(&EntityType::ZOMBIE_NAUTILUS, 0.5);
        let zombie_nautilus = zombie
            .cast_any()
            .downcast_ref::<NautilusEntity>()
            .expect("a nautilus entity");
        zombie_nautilus.set_variant(ZombieNautilusVariant::Warm);
        let mut nbt = NbtCompound::new();
        zombie_nautilus.mob_write_nbt(&mut nbt);
        assert_eq!(nbt.get_string("variant"), Some("minecraft:warm"));

        let loaded = spawn(&EntityType::ZOMBIE_NAUTILUS, 0.5);
        let loaded = loaded
            .cast_any()
            .downcast_ref::<NautilusEntity>()
            .expect("a nautilus entity");
        loaded.mob_read_nbt(&nbt);
        assert_eq!(loaded.get_variant(), ZombieNautilusVariant::Warm);
    }

    #[tokio::test]
    async fn the_dash_runs_at_the_target_then_waits() {
        let nautilus = spawn(&EntityType::NAUTILUS, 0.5);
        let mob = nautilus.get_mob().expect("a mob");
        let target = spawn(&EntityType::PUFFERFISH, 6.5);
        mob.get_mob_entity().set_target(Some(target));

        let mut goal = ChargeAttackGoal::new(
            TIME_BETWEEN_ATTACKS,
            0.6,
            2.0,
            12.0,
            11.0,
            pumpkin_data::sound::Sound::EntityNautilusDash,
        );
        assert!(goal.can_start(mob));
        goal.start(mob);
        goal.tick(mob);
        let velocity = nautilus.get_entity().velocity.load();
        assert!((velocity.x - 0.6).abs() < 1e-9, "{velocity:?}");
        assert!(velocity.y.abs() < 1e-9 && velocity.z.abs() < 1e-9);

        goal.stop(mob);
        assert!(mob.get_mob_entity().get_target().is_none());
        mob.get_mob_entity()
            .set_target(Some(spawn(&EntityType::PUFFERFISH, 3.5)));
        assert!(!goal.can_start(mob), "80 ticks between dashes");
    }
}
