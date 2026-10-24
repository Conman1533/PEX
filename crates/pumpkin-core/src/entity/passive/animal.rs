use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use pumpkin_data::dye_color::DyeColor;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::crafting::crafting_inventory::CraftingInventory;
use pumpkin_inventory::crafting::crafting_screen_handler::match_crafting_recipe;
use pumpkin_inventory::inventory::Inventory;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::ageable::get_speed_up_seconds_when_feeding;
use crate::entity::experience_orb::ExperienceOrbEntity;
use crate::entity::r#type::from_type;
use crate::entity::{
    EntityBase,
    mob::{Mob, MobEntity},
    player::Player,
};
use crate::world::World;

/// Vanilla `Animal.PARENT_AGE_AFTER_BREEDING`.
pub const PARENT_AGE_AFTER_BREEDING: i32 = 6000;
/// The love time vanilla `Animal.setInLove` gives.
const IN_LOVE_TIME: i32 = 600;

pub trait Animal: Mob {
    fn is_food(&self, item_stack: &ItemStack) -> bool;

    /// Vanilla `Animal.playEatingSound`, silent unless the animal overrides it.
    fn play_eating_sound(&self) {}

    /// Animals prefer grass, then bright spots.
    fn animal_walk_target_value(&self, pos: &pumpkin_util::math::position::BlockPos) -> f32 {
        let world = self.get_mob_entity().living_entity.entity.world.load();
        if world.get_block(&pos.down()).id == pumpkin_data::Block::GRASS_BLOCK.id {
            return 10.0;
        }
        world.get_light_level_dependent_magic_value(pos) - 0.5
    }

    /// The love part of vanilla `Animal.aiStep`. The hearts vanilla adds there never leave a
    /// server.
    fn animal_ai_step(&self) {
        let mob_entity = self.get_mob_entity();
        if mob_entity.living_entity.entity.age.load(Relaxed) != 0 {
            self.reset_love();
        }
        let love_ticks = mob_entity.love_ticks.load(Relaxed);
        if love_ticks > 0 {
            mob_entity.love_ticks.store(love_ticks - 1, Relaxed);
        }
    }

    /// Vanilla `Animal.canFallInLove`.
    fn can_fall_in_love(&self) -> bool {
        self.get_mob_entity().love_ticks.load(Relaxed) <= 0
    }

    /// Vanilla `Animal.setInLove`. A plugin can cancel it through `EntityEnterLoveModeEvent`.
    fn set_in_love(&self, player: Option<&Arc<Player>>) {
        if !self.enter_love_mode(player.map(|player| player.entity_id()), IN_LOVE_TIME) {
            return;
        }
        let mob_entity = self.get_mob_entity();
        mob_entity.love_ticks.store(IN_LOVE_TIME, Relaxed);
        if let Some(player) = player {
            mob_entity.breeder.store(Some(player.gameprofile.id));
        }
        let entity = &mob_entity.living_entity.entity;
        entity.world.load().send_entity_status(
            entity,
            EntityStatus::InLoveHearts,
            Some(ActorEventID::InLoveHearts),
        );
    }

    /// Vanilla `Animal.getLoveCause`.
    fn get_love_cause(&self) -> Option<Arc<Player>> {
        let uuid = self.get_mob_entity().breeder.load()?;
        self.get_entity().world.load().get_player_by_uuid(uuid)
    }

    /// Vanilla `Animal.resetLove`.
    fn reset_love(&self) {
        self.get_mob_entity().love_ticks.store(0, Relaxed);
    }

    /// Vanilla `Animal.canMate`.
    fn can_mate(&self, partner: &dyn Mob) -> bool {
        animal_can_mate(self.get_mob_entity(), partner)
    }

    /// Vanilla `AgeableMob.getBreedOffspring`: a mob of the same type unless overridden.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        _partner: &dyn Mob,
    ) -> Option<Arc<dyn EntityBase>> {
        Some(create_offspring(
            self.get_mob_entity(),
            self.get_entity().entity_type,
            world,
        ))
    }

    /// Vanilla `Animal.spawnChildFromBreeding`. A plugin can cancel it through
    /// `EntityBreedEvent`.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Mob) {
        let Some(offspring) = self.get_breed_offspring(world, partner) else {
            return;
        };
        if !self.breed(
            self.get_entity().entity_id,
            partner.get_entity().entity_id,
            offspring.get_entity().entity_id,
        ) {
            return;
        }
        if let Some(mob) = offspring.get_mob() {
            mob.spawn_as_baby();
        }
        self.finalize_spawn_child_from_breeding(world, partner, Some(&offspring));
        world.spawn_entity(offspring);
    }

    /// Vanilla `Animal.finalizeSpawnChildFromBreeding`.
    fn finalize_spawn_child_from_breeding(
        &self,
        world: &Arc<World>,
        partner: &dyn Mob,
        offspring: Option<&Arc<dyn EntityBase>>,
    ) {
        let entity = self.get_entity();
        let cause = self
            .get_love_cause()
            .or_else(|| partner.as_animal().and_then(Animal::get_love_cause));
        if let Some(player) = cause {
            player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::AnimalsBred as i32,
                1,
            );
            player.trigger_advancement_criterion(
                pumpkin_data::advancement::Advancement::HUSBANDRY_BREED_AN_ANIMAL,
                "bred",
            );
            let bred_type =
                offspring.map_or(entity.entity_type, |child| child.get_entity().entity_type);
            player.trigger_advancement_criterion(
                pumpkin_data::advancement::Advancement::HUSBANDRY_BRED_ALL_ANIMALS,
                &format!("minecraft:{}", bred_type.resource_name),
            );
        }

        if let Some(ageable) = self.as_ageable() {
            ageable.set_age(PARENT_AGE_AFTER_BREEDING);
        }
        if let Some(ageable) = partner.as_ageable() {
            ageable.set_age(PARENT_AGE_AFTER_BREEDING);
        }
        self.reset_love();
        if let Some(animal) = partner.as_animal() {
            animal.reset_love();
        }
        world.send_entity_status(
            entity,
            EntityStatus::InLoveHearts,
            Some(ActorEventID::InLoveHearts),
        );
        if world.level_info.load().game_rules.mob_drops {
            ExperienceOrbEntity::spawn(
                world,
                entity.pos.load(),
                self.get_random().random_range(1..=7),
            );
        }
    }

    fn write_animal_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
        let mob_entity = self.get_mob_entity();
        nbt.put_int("InLove", mob_entity.love_ticks.load(Relaxed));
        if let Some(uuid) = mob_entity.breeder.load() {
            nbt.put_uuid("LoveCause", uuid);
        }
    }

    fn read_animal_nbt(&self, nbt: &pumpkin_nbt::compound::NbtCompound) {
        let mob_entity = self.get_mob_entity();
        mob_entity
            .love_ticks
            .store(nbt.get_int("InLove").unwrap_or(0), Relaxed);
        mob_entity.breeder.store(nbt.get_uuid("LoveCause"));
    }

    /// Vanilla `Animal.mobInteract`: food puts an adult in love and speeds up a baby.
    fn animal_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.is_food(item_stack) {
            let age = self.get_entity().age.load(Relaxed);
            if age == 0 && self.can_fall_in_love() {
                item_stack.decrement_unless_creative(player.gamemode.load(), 1);
                self.set_in_love(Some(player));
                self.play_eating_sound();
                return true;
            }

            if let Some(ageable) = self.as_ageable()
                && ageable.can_age_up()
            {
                item_stack.decrement_unless_creative(player.gamemode.load(), 1);
                ageable.age_up(get_speed_up_seconds_when_feeding(-age), true);
                self.play_eating_sound();
                return true;
            }
        }

        self.get_mob_entity()
            .mob_interact(player, item_stack, self.can_be_leashed())
    }
}

/// The base vanilla `Animal.canMate`: another mob of the same type, both in love.
#[must_use]
pub fn animal_can_mate(animal: &MobEntity, partner: &dyn Mob) -> bool {
    let entity = &animal.living_entity.entity;
    let partner_entity = partner.get_entity();
    partner_entity.entity_id != entity.entity_id
        && partner_entity.entity_type == entity.entity_type
        && animal.is_in_love()
        && partner.get_mob_entity().is_in_love()
}

/// Vanilla `EntityType.create(level, EntitySpawnReason.BREEDING)` at the parent's position.
#[must_use]
pub fn create_offspring(
    parent: &MobEntity,
    entity_type: &'static EntityType,
    world: &Arc<World>,
) -> Arc<dyn EntityBase> {
    from_type(
        entity_type,
        parent.living_entity.entity.pos.load(),
        world,
        Uuid::new_v4(),
    )
}

/// Vanilla `DyeColor.getMixedColor`: the dye the two dyes craft into, else either of them.
#[must_use]
pub fn get_mixed_color(color1: u8, color2: u8) -> u8 {
    find_color_mix_in_recipes(color1, color2).unwrap_or_else(|| {
        if rand::random::<bool>() {
            color1
        } else {
            color2
        }
    })
}

fn find_color_mix_in_recipes(color1: u8, color2: u8) -> Option<u8> {
    let dye_item = |color: u8| {
        let name = DyeColor::by_id(color)?.name();
        Item::from_registry_key(&format!("{name}_dye"))
    };
    let input = CraftingInventory::new(2, 1);
    input.set_stack(0, ItemStack::new(1, dye_item(color1)?));
    input.set_stack(1, ItemStack::new(1, dye_item(color2)?));
    let result = match_crafting_recipe(&input, None)?;
    let item = Item::from_registry_key(
        result
            .item_id
            .strip_prefix("minecraft:")
            .unwrap_or(&result.item_id),
    )?;
    get_dye_color_from_item(item)
}

#[must_use]
pub fn get_dye_color_from_item(item: &pumpkin_data::item::Item) -> Option<u8> {
    use pumpkin_data::data_component::DataComponent;
    use pumpkin_data::data_component_impl::{DyeImpl, get};

    for (id, component) in item.components {
        if *id == DataComponent::Dye {
            return Some(get::<DyeImpl>(*component).color.id());
        }
    }
    None
}

#[must_use]
pub fn get_carpet_color_from_item(item: &pumpkin_data::item::Item) -> Option<u8> {
    match item.registry_key {
        "white_carpet" => Some(0),
        "orange_carpet" => Some(1),
        "magenta_carpet" => Some(2),
        "light_blue_carpet" => Some(3),
        "yellow_carpet" => Some(4),
        "lime_carpet" => Some(5),
        "pink_carpet" => Some(6),
        "gray_carpet" => Some(7),
        "light_gray_carpet" => Some(8),
        "cyan_carpet" => Some(9),
        "purple_carpet" => Some(10),
        "blue_carpet" => Some(11),
        "brown_carpet" => Some(12),
        "green_carpet" => Some(13),
        "red_carpet" => Some(14),
        "black_carpet" => Some(15),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::get_mixed_color;
    use pumpkin_data::dye_color::DyeColor;

    #[test]
    fn mixed_color_uses_the_two_dye_recipes() {
        let mix = |a: DyeColor, b: DyeColor| get_mixed_color(a.id(), b.id());
        assert_eq!(mix(DyeColor::Red, DyeColor::Yellow), DyeColor::Orange.id());
        assert_eq!(mix(DyeColor::Yellow, DyeColor::Red), DyeColor::Orange.id());
        assert_eq!(mix(DyeColor::White, DyeColor::Black), DyeColor::Gray.id());
        assert_eq!(mix(DyeColor::Blue, DyeColor::Red), DyeColor::Purple.id());
        assert_eq!(mix(DyeColor::Green, DyeColor::Blue), DyeColor::Cyan.id());
        assert_eq!(mix(DyeColor::Red, DyeColor::White), DyeColor::Pink.id());
    }
}
