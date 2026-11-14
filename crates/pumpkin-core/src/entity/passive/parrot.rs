use std::sync::{Arc, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        escape_danger::EscapeDangerGoal, follow_mob::FollowMobGoal, follow_owner::FollowOwnerGoal,
        look_at_entity::LookAtEntityGoal, sit_when_ordered_to::SitWhenOrderedToGoal,
        swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::{
        animal::Animal,
        tamable::{TamableAnimal, TamableData},
    },
    player::Player,
};

/// Duration in ticks of the poison a parrot gets from eating a cookie, matching
/// vanilla `Parrot.mobInteract`.
const COOKIE_POISON_DURATION: i32 = 900;

/// Represents a Parrot, a passive flying mob that can mimic nearby mob sounds.
///
/// Wiki: <https://minecraft.wiki/w/Parrot>
pub struct ParrotEntity {
    pub mob_entity: MobEntity,
    pub tamable_data: TamableData,
}

impl ParrotEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let parrot = Self {
            mob_entity,
            tamable_data: TamableData::default(),
        };
        let mob_arc = Arc::new(parrot);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, EscapeDangerGoal::new(1.25).teleporting_to_owner());
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                1,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(2, Box::new(SitWhenOrderedToGoal::new()));
            goal_selector.add_goal(2, FollowOwnerGoal::new(1.0, 5.0, 1.0));
            goal_selector.add_goal(2, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(3, Box::new(FollowMobGoal::new(1.0, 3.0, 7.0)));
        };

        mob_arc
    }

    /// Feeds the parrot a cookie: it is poisoned and then killed, as in vanilla
    /// `Parrot.mobInteract`.
    fn eat_cookie(&self, player: &Arc<Player>, item_stack: &mut ItemStack) {
        item_stack.decrement_unless_creative(player.gamemode.load(), 1);

        self.mob_entity
            .living_entity
            .add_effect(pumpkin_data::potion::Effect {
                effect_type: &StatusEffect::POISON,
                duration: COOKIE_POISON_DURATION,
                amplifier: 0,
                ambient: false,
                show_particles: true,
                show_icon: true,
                blend: true,
            });

        // Vanilla guards this call with `player.isCreative() || !this.isInvulnerable()`,
        // but `hurt` re-checks invulnerability itself and `player_attack` doesn't bypass
        // it, so the guard only skips a call that would do nothing anyway.
        self.damage_with_context(
            self,
            f32::MAX,
            DamageType::PLAYER_ATTACK,
            None,
            Some(player.as_ref()),
            Some(player.as_ref()),
        );
    }
}

/// Vanilla `Parrot` is an `Animal` that never breeds: nothing is food and nothing can mate.
impl Animal for ParrotEntity {
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }

    fn can_mate(&self, _partner: &dyn Mob) -> bool {
        false
    }

    fn get_breed_offspring(
        &self,
        _world: &Arc<crate::world::World>,
        _partner: &dyn Mob,
    ) -> Option<Arc<dyn EntityBase>> {
        None
    }
}

impl TamableAnimal for ParrotEntity {
    fn get_tamable_data(&self) -> &TamableData {
        &self.tamable_data
    }
}

impl ParrotEntity {
    /// Vanilla `Parrot.mobInteract` with `parrot_food`: eaten, then a 1 in 10 chance to tame.
    fn feed_seeds(&self, player: &Arc<Player>, item_stack: &mut ItemStack) {
        item_stack.decrement_unless_creative(player.gamemode.load(), 1);
        let entity = self.get_entity();
        let world = entity.world.load();
        if !entity.silent.load(std::sync::atomic::Ordering::Relaxed) {
            let mut random = self.get_random();
            let pitch = 1.0 + (random.random::<f32>() - random.random::<f32>()) * 0.2;
            world.play_sound_fine(
                Sound::EntityParrotEat,
                SoundCategory::Neutral,
                &entity.pos.load(),
                1.0,
                pitch,
            );
        }
        if self.get_random().random_range(0..10) == 0 {
            TamableAnimal::tame(self, player.gameprofile.id);
            world.send_entity_status(
                entity,
                EntityStatus::TamingSucceeded,
                ActorEventID::TamingSucceeded,
            );
        } else {
            world.send_entity_status(
                entity,
                EntityStatus::TamingFailed,
                ActorEventID::TamingFailed,
            );
        }
    }
}

impl Mob for ParrotEntity {
    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn as_tamable(&self) -> Option<&dyn TamableAnimal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_tamable_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_tamable_nbt(nbt);
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        // Vanilla `Parrot.getAmbient` also imitates a nearby monster one time in a thousand.
        Some(pumpkin_data::sound::Sound::EntityParrotAmbient)
    }

    /// Vanilla `Animal.getAmbientSoundInterval`; parrots are animals there.
    fn get_ambient_sound_interval(&self) -> i32 {
        crate::entity::mob::ANIMAL_AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla `Animal.removeWhenFarAway`: parrots are animals there, unlike in Pumpkin.
    fn remove_when_far_away(&self, _distance_sq: f64) -> bool {
        false
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `Parrot.mobInteract`: seeds tame a wild parrot, the owner toggles sitting on a
    /// perched (not flying) parrot, cookies kill it.
    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();
        if !self.is_tame() && item.has_tag(&tag::Item::MINECRAFT_PARROT_FOOD) {
            self.feed_seeds(player, item_stack);
            return true;
        }
        if !item.has_tag(&tag::Item::MINECRAFT_PARROT_POISONOUS_FOOD) {
            let flying = !self
                .get_entity()
                .on_ground
                .load(std::sync::atomic::Ordering::Relaxed);
            if !flying && self.is_tame() && self.is_owned_by(&player.gameprofile.id) {
                self.set_ordered_to_sit(!self.is_ordered_to_sit());
                return true;
            }
            return self
                .mob_entity
                .mob_interact(player, item_stack, self.can_be_leashed());
        }

        self.eat_cookie(player, item_stack);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::COOKIE_POISON_DURATION;
    use pumpkin_data::item::Item;
    use pumpkin_data::tag::{self, Taggable};

    /// The interaction is gated on the vanilla `parrot_poisonous_food` tag rather than
    /// on a hardcoded cookie id, so check the tag actually resolves the way the
    /// interaction assumes.
    #[test]
    fn cookie_is_poisonous_parrot_food() {
        assert!(Item::COOKIE.has_tag(&tag::Item::MINECRAFT_PARROT_POISONOUS_FOOD));
    }

    /// Seeds tame a parrot in vanilla and must not reach the poison branch.
    #[test]
    fn parrot_food_is_not_poisonous() {
        assert!(!Item::WHEAT_SEEDS.has_tag(&tag::Item::MINECRAFT_PARROT_POISONOUS_FOOD));
        assert!(!Item::COOKED_CHICKEN.has_tag(&tag::Item::MINECRAFT_PARROT_POISONOUS_FOOD));
    }

    #[test]
    fn poison_lasts_45_seconds() {
        assert_eq!(COOKIE_POISON_DURATION, 900);
    }
}
