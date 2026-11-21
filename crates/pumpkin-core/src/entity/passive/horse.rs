use crate::entity::ai::goal::run_around_like_crazy::{RideTamable, RunAroundLikeCrazyGoal};
use crate::entity::passive::animal::create_offspring;
use crate::entity::passive::donkey::DonkeyEntity;
use crate::world::World;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicI32, AtomicU8, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        breed::BreedGoal, escape_danger::EscapeDangerGoal, follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};

pub const FLAG_TAME: u8 = 2;
/// Vanilla `AbstractHorse.getAmbientSoundInterval`.
pub const HORSE_AMBIENT_SOUND_INTERVAL: i32 = 400;
/// The number of vanilla `Variant` coat colors.
const VARIANT_COUNT: i32 = 7;
/// The number of vanilla `Markings`.
const MARKINGS_COUNT: i32 = 5;
pub const FLAG_SADDLE: u8 = 4;
pub const FLAG_BRED: u8 = 8;
pub const FLAG_EATING: u8 = 16;
pub const FLAG_STANDING: u8 = 32;
pub const FLAG_OPEN_MOUTH: u8 = 64;

pub struct HorseEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub variant: AtomicI32,
    pub flags: AtomicU8,
    pub temper: AtomicI32,
    pub owner: AtomicCell<Option<Uuid>>,
    /// Vanilla `standCounter`: ticks spent rearing.
    pub stand_counter: AtomicI32,
}

impl HorseEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let mut rng = rand::rng();
        let color = rng.random_range(0..VARIANT_COUNT);
        let style = rng.random_range(0..MARKINGS_COUNT);
        let variant = color | (style << 8);

        let horse = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            variant: AtomicI32::new(variant),
            flags: AtomicU8::new(0),
            temper: AtomicI32::new(0),
            owner: AtomicCell::new(None),
            stand_counter: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(horse);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(1.2));
            let ride_weak: Weak<dyn RideTamable> = {
                let ride_arc: Arc<dyn RideTamable> = mob_arc.clone();
                Arc::downgrade(&ride_arc)
            };
            goal_selector.add_goal(1, Box::new(RunAroundLikeCrazyGoal::new(ride_weak, 1.2)));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                Box::new(TemptGoal::new(
                    1.25,
                    |_, stack| {
                        stack
                            .get_item()
                            .has_tag(&tag::Item::MINECRAFT_HORSE_TEMPT_ITEMS)
                    },
                    false,
                )),
            );
            goal_selector.add_goal(4, Box::new(FollowParentGoal::new(1.0)));
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::water_avoiding(0.7)));
            goal_selector.add_goal(
                7,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn get_variant(&self) -> i32 {
        self.variant.load(Ordering::Relaxed)
    }

    pub fn set_variant(&self, variant: i32) {
        self.variant.store(variant, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::horse::DATA_ID_TYPE_VARIANT,
            VarInt(variant),
        );
    }

    #[must_use]
    pub fn has_flag(&self, flag: u8) -> bool {
        (self.flags.load(Ordering::Relaxed) & flag) != 0
    }

    pub fn set_flag(&self, flag: u8, val: bool) {
        let current = self.flags.load(Ordering::Relaxed);
        let new_flags = if val { current | flag } else { current & !flag };
        self.flags.store(new_flags, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::horse::DATA_ID_FLAGS,
            new_flags as i8,
        );
    }

    #[must_use]
    pub fn is_tame(&self) -> bool {
        self.has_flag(FLAG_TAME)
    }

    pub fn set_tame(&self, val: bool) {
        self.set_flag(FLAG_TAME, val);
    }

    #[must_use]
    pub fn is_saddled(&self) -> bool {
        self.has_flag(FLAG_SADDLE)
    }

    pub fn set_saddled(&self, val: bool) {
        self.set_flag(FLAG_SADDLE, val);
    }
}

impl AgeableMob for HorseEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

/// Vanilla `AbstractHorse.canParent`.
#[must_use]
pub fn can_parent(mob: &dyn Mob, tamed: bool) -> bool {
    let mob_entity = mob.get_mob_entity();
    let living = &mob_entity.living_entity;
    !living.entity.has_passengers()
        && !living.entity.has_vehicle()
        && tamed
        && !mob.as_ageable().is_some_and(AgeableMob::is_baby)
        && living.health.load() >= living.get_max_health()
        && mob_entity.is_in_love()
}

/// Whether `mob` is a tamed horse or donkey, `None` when it is neither.
#[must_use]
pub fn horse_or_donkey_tamed(mob: &dyn Mob) -> Option<bool> {
    mob.cast_any()
        .downcast_ref::<HorseEntity>()
        .map(HorseEntity::is_tame)
        .or_else(|| {
            mob.cast_any()
                .downcast_ref::<DonkeyEntity>()
                .map(DonkeyEntity::is_tame)
        })
}

/// Vanilla `AbstractHorse.handleEating`, or `Llama.handleEating` when `llama`.
///
/// The food heals, ages a foal and calms an untamed animal, and only a tamed adult falls in love.
/// Plays the eating sound and returns whether the food was used.
pub fn handle_horse_eating(
    mob: &dyn Mob,
    temper: &AtomicI32,
    tamed: bool,
    llama: bool,
    eat_sound: Sound,
    player: &Arc<Player>,
    item: &'static Item,
) -> bool {
    let (heal, age_up, temper_gain, can_love): (f32, i32, i32, bool) = if llama {
        if item == &Item::WHEAT {
            (2.0, 10, 3, false)
        } else if item == &Item::HAY_BLOCK {
            (10.0, 90, 6, true)
        } else {
            (0.0, 0, 0, false)
        }
    } else if item == &Item::WHEAT {
        (2.0, 20, 3, false)
    } else if item == &Item::SUGAR {
        (1.0, 30, 3, false)
    } else if item == &Item::HAY_BLOCK {
        (20.0, 180, 0, false)
    } else if item == &Item::APPLE {
        (3.0, 60, 3, false)
    } else if item == &Item::RED_MUSHROOM {
        (3.0, 0, 3, false)
    } else if item == &Item::CARROT {
        (3.0, 60, 3, false)
    } else if item == &Item::GOLDEN_CARROT {
        (4.0, 60, 5, true)
    } else if item == &Item::GOLDEN_APPLE || item == &Item::ENCHANTED_GOLDEN_APPLE {
        (10.0, 240, 10, true)
    } else {
        (0.0, 0, 0, false)
    };

    let living = &mob.get_mob_entity().living_entity;
    let entity = &living.entity;
    let mut used = false;
    if can_love
        && tamed
        && entity.age.load(Ordering::Relaxed) == 0
        && let Some(animal) = mob.as_animal()
        && animal.can_fall_in_love()
    {
        used = true;
        animal.set_in_love(Some(player));
    }
    if heal > 0.0 && living.health.load() < living.get_max_health() {
        living.heal(heal);
        used = true;
    }
    if age_up > 0
        && let Some(ageable) = mob.as_ageable()
        && ageable.can_age_up()
    {
        ageable.age_up(age_up, false);
        used = true;
    }
    let max_temper = if llama { 30 } else { 100 };
    let current = temper.load(Ordering::Relaxed);
    if temper_gain > 0 && (used || !tamed) && current < max_temper {
        temper.store(
            (current + temper_gain).clamp(0, max_temper),
            Ordering::Relaxed,
        );
        used = true;
    }
    if used && !entity.silent.load(Ordering::Relaxed) {
        let mut rng = mob.get_random();
        let pitch = 1.0 + (rng.random::<f32>() - rng.random::<f32>()) * 0.2;
        entity.world.load().play_sound_fine(
            eat_sound,
            SoundCategory::Neutral,
            &entity.pos.load(),
            1.0,
            pitch,
        );
    }
    used
}

impl Animal for HorseEntity {
    fn play_eating_sound(&self) {
        crate::entity::passive::animal::play_eating_sound(
            self,
            if self
                .get_mob_entity()
                .living_entity
                .entity
                .age
                .load(std::sync::atomic::Ordering::Relaxed)
                < 0
            {
                pumpkin_data::sound::Sound::EntityBabyHorseEat
            } else {
                pumpkin_data::sound::Sound::EntityHorseEat
            },
            1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2,
        );
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_HORSE_FOOD)
    }

    /// Vanilla `Horse.canMate`: with a horse or a donkey.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        partner.get_entity().entity_id != self.get_entity().entity_id
            && horse_or_donkey_tamed(partner)
                .is_some_and(|tamed| can_parent(self, self.is_tame()) && can_parent(partner, tamed))
    }

    /// Vanilla `Horse.getBreedOffspring`: a mule with a donkey, else a foal mixing the parents'
    /// coats.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        partner: &dyn Mob,
    ) -> Option<Arc<dyn EntityBase>> {
        if partner.cast_any().is::<DonkeyEntity>() {
            return Some(create_offspring(&self.mob_entity, &EntityType::MULE, world));
        }
        let baby = create_offspring(&self.mob_entity, &EntityType::HORSE, world);
        if let Some(foal) = baby.cast_any().downcast_ref::<Self>()
            && let Some(partner) = partner.cast_any().downcast_ref::<Self>()
        {
            let mut rng = self.get_random();
            let select_skin = rng.random_range(0..9);
            let color = if select_skin < 4 {
                self.get_variant() & 0xFF
            } else if select_skin < 8 {
                partner.get_variant() & 0xFF
            } else {
                rng.random_range(0..VARIANT_COUNT)
            };
            let select_marking = rng.random_range(0..5);
            let markings = if select_marking < 2 {
                self.get_variant() >> 8
            } else if select_marking < 4 {
                partner.get_variant() >> 8
            } else {
                rng.random_range(0..MARKINGS_COUNT)
            };
            foal.set_variant(color | (markings << 8));
        }
        Some(baby)
    }
}

impl RideTamable for HorseEntity {
    fn is_ride_tamed(&self) -> bool {
        self.is_tame()
    }

    fn temper(&self) -> &AtomicI32 {
        &self.temper
    }

    fn tame_with_name(&self, player: &Player) {
        self.owner.store(Some(player.gameprofile.id));
        self.set_tame(true);
    }

    /// Vanilla `makeMad`: rear up (`standIfPossible`) with the angry sound.
    fn make_mad(&self) {
        if !self.has_flag(FLAG_STANDING) {
            self.stand_counter.store(1, Ordering::Relaxed);
            self.set_flag(FLAG_STANDING, true);
            self.get_entity()
                .play_sound(pumpkin_data::sound::Sound::EntityHorseAngry);
        }
    }
}

/// Vanilla `AbstractHorse.playStepSound` while not ridden at a gallop: wood steps on wooden
/// blocks, otherwise the plain (or baby) horse step, both at the block's volume and pitch,
/// and snow layers on top count as the block. Nothing in liquids.
pub(crate) fn horse_step_sound(
    mob: &dyn Mob,
    world: &World,
    pos: &pumpkin_util::math::position::BlockPos,
    block: &'static pumpkin_data::Block,
    state: &'static pumpkin_data::BlockState,
) -> Option<(Sound, f32, f32)> {
    use pumpkin_data::block_sound::BlockSoundType;
    if state.is_liquid() {
        return None;
    }
    let (above_block, above_state) = world.get_block_and_state(&pos.up());
    let sound_type = if above_block.id == pumpkin_data::BlockId::SNOW {
        BlockSoundType::of(above_block.id, above_state.id)
    } else {
        BlockSoundType::of(block.id, state.id)
    };
    let wood = matches!(
        sound_type.place_sound,
        Sound::BlockWoodPlace
            | Sound::BlockNetherWoodPlace
            | Sound::BlockStemPlace
            | Sound::BlockCherryWoodPlace
            | Sound::BlockBambooWoodPlace
    );
    let sound = if wood {
        Sound::EntityHorseStepWood
    } else if mob
        .get_mob_entity()
        .living_entity
        .entity
        .age
        .load(Ordering::Relaxed)
        < 0
    {
        Sound::EntityBabyHorseStep
    } else {
        Sound::EntityHorseStep
    };
    Some((sound, sound_type.volume * 0.15, sound_type.pitch))
}

impl Mob for HorseEntity {
    fn mob_controlling_passenger(&self) -> Option<std::sync::Arc<dyn crate::entity::EntityBase>> {
        crate::entity::mob::saddled_player_controller(self.get_entity(), self.is_saddled()).or_else(
            || {
                crate::entity::mob::mob_rider_controller(
                    self.get_entity(),
                    self.get_mob_entity().is_no_ai(),
                )
            },
        )
    }

    fn travel(&self, _caller: &dyn crate::entity::EntityBase) -> bool {
        crate::entity::mob::player_steers(self)
    }

    /// Vanilla `AbstractHorse.aiStep`: rearing ends after 20 ticks.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let stand = self.stand_counter.load(Ordering::Relaxed);
        if stand > 0 {
            if stand + 1 > 20 {
                self.stand_counter.store(0, Ordering::Relaxed);
                self.set_flag(FLAG_STANDING, false);
            } else {
                self.stand_counter.store(stand + 1, Ordering::Relaxed);
            }
        }
    }

    fn get_sound_volume(&self) -> f32 {
        0.8
    }

    fn step_sound_on(
        &self,
        world: &crate::world::World,
        pos: &pumpkin_util::math::position::BlockPos,
        block: &'static pumpkin_data::Block,
        state: &'static pumpkin_data::BlockState,
    ) -> Option<(pumpkin_data::sound::Sound, f32, f32)> {
        crate::entity::passive::horse::horse_step_sound(self, world, pos, block, state)
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.is_baby() {
            pumpkin_data::sound::Sound::EntityBabyHorseAmbient
        } else {
            pumpkin_data::sound::Sound::EntityHorseAmbient
        })
    }

    /// Vanilla `AbstractHorse.getAmbientSoundInterval`.
    fn get_ambient_sound_interval(&self) -> i32 {
        HORSE_AMBIENT_SOUND_INTERVAL
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_int("Variant", self.get_variant());
        nbt.put_bool("Tame", self.is_tame());
        nbt.put_int("Temper", self.temper.load(Ordering::Relaxed));
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("Owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(variant) = nbt.get_int("Variant") {
            self.set_variant(variant);
        }
        if let Some(tame) = nbt.get_bool("Tame") {
            self.set_tame(tame);
        }
        if let Some(temper) = nbt.get_int("Temper") {
            self.temper.store(temper, Ordering::Relaxed);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner.store(Some(owner));
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::horse::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::horse::DATA_ID_TYPE_VARIANT,
            VarInt(self.get_variant()),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::horse::DATA_ID_FLAGS,
            self.flags.load(Ordering::Relaxed) as i8,
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();

        if self.is_tame() && item == &Item::SADDLE && !self.is_saddled() && !self.is_baby() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                Sound::EntityHorseSaddle,
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            return true;
        }

        if !self.is_baby() && !self.is_food(item_stack) {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        if self.is_food(item_stack) {
            let used = handle_horse_eating(
                self,
                &self.temper,
                self.is_tame(),
                false,
                Sound::EntityHorseEat,
                player,
                item_stack.get_item(),
            );
            if used {
                item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            }
            return used;
        }

        self.animal_interact(player, item_stack)
    }
}
