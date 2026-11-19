use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data::illusioner::DATA_SPELL_CASTING_ID;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::Difficulty;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, bow_attack::BowAttackGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, swim::SwimGoal,
        to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        equipment::RegionalDifficulty,
        evoker::IllagerSpell,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            HoldGroundAttackGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider,
            RaiderCelebrationGoal, RaiderData, RaiderMoveThroughVillageGoal,
        },
    },
};

pub struct IllusionerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    /// Vanilla `SpellcasterIllager.spellCastingTickCount`.
    spell_casting_tick_count: AtomicI32,
    current_spell: AtomicU8,
}

impl IllusionerEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let illusioner = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            spell_casting_tick_count: AtomicI32::new(0),
            current_spell: AtomicU8::new(IllagerSpell::None as u8),
        };
        let mob_arc = Arc::new(illusioner);
        let illusioner_weak: Weak<Self> = Arc::downgrade(&mob_arc);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                1,
                Box::new(IllusionerCastingSpellGoal {
                    illusioner: illusioner_weak.clone(),
                }),
            );
            goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal));
            goal_selector.add_goal(2, Box::new(HoldGroundAttackGoal::new(10.0)));
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(4, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(
                4,
                Box::new(IllusionerUseSpellGoal::new(
                    illusioner_weak.clone(),
                    IllusionerSpell::Mirror,
                )),
            );
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(
                5,
                Box::new(IllusionerUseSpellGoal::new(
                    illusioner_weak,
                    IllusionerSpell::Blindness,
                )),
            );
            goal_selector.add_goal(6, Box::new(BowAttackGoal::new(0.5, 20, 15.0)));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::VILLAGER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
        };

        mob_arc
    }
}

impl IllusionerEntity {
    fn is_casting_spell(&self) -> bool {
        self.spell_casting_tick_count.load(Ordering::Relaxed) > 0
    }

    /// Vanilla `SpellcasterIllager.setIsCastingSpell`.
    fn set_is_casting_spell(&self, spell: IllagerSpell) {
        self.current_spell.store(spell as u8, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(DATA_SPELL_CASTING_ID, spell as u8 as i8);
    }
}

impl Mob for IllusionerEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityIllusionerAmbient)
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `SpellcasterIllager.customServerAiStep`.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let ticks = self.spell_casting_tick_count.load(Ordering::Relaxed);
        if ticks > 0 {
            self.spell_casting_tick_count
                .store(ticks - 1, Ordering::Relaxed);
        }
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        nbt.put_int(
            "SpellTicks",
            self.spell_casting_tick_count.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        if let Some(ticks) = nbt.get_int("SpellTicks") {
            self.spell_casting_tick_count
                .store(ticks, Ordering::Relaxed);
        }
    }
}

/// Vanilla `SpellcasterIllager.SpellcasterCastingSpellGoal`: stand still and face the
/// target while a spell is being cast.
pub struct IllusionerCastingSpellGoal {
    illusioner: Weak<IllusionerEntity>,
}

impl Goal for IllusionerCastingSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.illusioner
            .upgrade()
            .is_some_and(|i| i.is_casting_spell())
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(illusioner) = self.illusioner.upgrade() {
            illusioner.set_is_casting_spell(IllagerSpell::None);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target() {
            mob.get_mob_entity()
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity_with_range(&target, 30.0, 30.0);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IllusionerSpell {
    /// `IllusionerMirrorSpellGoal`: turn invisible.
    Mirror,
    /// `IllusionerBlindnessSpellGoal`: blind the target.
    Blindness,
}

impl IllusionerSpell {
    const fn casting_interval(self) -> i32 {
        match self {
            Self::Mirror => 340,
            Self::Blindness => 180,
        }
    }

    const fn prepare_sound(self) -> Sound {
        match self {
            Self::Mirror => Sound::EntityIllusionerPrepareMirror,
            Self::Blindness => Sound::EntityIllusionerPrepareBlindness,
        }
    }

    const fn illager_spell(self) -> IllagerSpell {
        match self {
            Self::Mirror => IllagerSpell::Disappear,
            Self::Blindness => IllagerSpell::Blindness,
        }
    }
}

/// Vanilla `SpellcasterIllager.SpellcasterUseSpellGoal` with the illusioner's two spells.
pub struct IllusionerUseSpellGoal {
    illusioner: Weak<IllusionerEntity>,
    spell: IllusionerSpell,
    attack_warmup_delay: i32,
    next_attack_tick_count: i32,
    last_target_id: i32,
}

impl IllusionerUseSpellGoal {
    /// Vanilla `SpellcasterUseSpellGoal.getCastWarmupTime`.
    const CAST_WARMUP_TIME: i32 = 20;
    /// Both illusioner spells return 20 from `getCastingTime`.
    const CASTING_TIME: i32 = 20;

    #[must_use]
    pub const fn new(illusioner: Weak<IllusionerEntity>, spell: IllusionerSpell) -> Self {
        Self {
            illusioner,
            spell,
            attack_warmup_delay: 0,
            next_attack_tick_count: 0,
            last_target_id: 0,
        }
    }
}

impl Goal for IllusionerUseSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return false;
        };
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        let entity = mob.get_entity();
        if !target.get_entity().is_alive()
            || illusioner.is_casting_spell()
            || entity.tick_count.load(Ordering::Relaxed) < self.next_attack_tick_count
        {
            return false;
        }
        match self.spell {
            IllusionerSpell::Mirror => !mob
                .get_mob_entity()
                .living_entity
                .has_effect(&StatusEffect::INVISIBILITY),
            IllusionerSpell::Blindness => {
                target.get_entity().entity_id != self.last_target_id
                    && RegionalDifficulty::at(&entity.world.load_full(), entity.pos.load())
                        .effective_difficulty
                        > Difficulty::Normal as i32 as f32
            }
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|t| t.get_entity().is_alive())
            && self.attack_warmup_delay > 0
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return;
        };
        self.attack_warmup_delay = to_goal_ticks(Self::CAST_WARMUP_TIME);
        illusioner
            .spell_casting_tick_count
            .store(Self::CASTING_TIME, Ordering::Relaxed);
        let entity = mob.get_entity();
        self.next_attack_tick_count =
            entity.tick_count.load(Ordering::Relaxed) + self.spell.casting_interval();
        entity.play_sound(self.spell.prepare_sound());
        illusioner.set_is_casting_spell(self.spell.illager_spell());
        if self.spell == IllusionerSpell::Blindness
            && let Some(target) = mob.get_mob_entity().get_target()
        {
            self.last_target_id = target.get_entity().entity_id;
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.attack_warmup_delay -= 1;
        if self.attack_warmup_delay != 0 {
            return;
        }
        match self.spell {
            IllusionerSpell::Mirror => {
                mob.get_mob_entity().living_entity.add_effect(Effect {
                    effect_type: &StatusEffect::INVISIBILITY,
                    duration: 1200,
                    amplifier: 0,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            }
            IllusionerSpell::Blindness => {
                if let Some(target) = mob.get_mob_entity().get_target()
                    && let Some(living) = target.get_living_entity()
                {
                    living.add_effect(Effect {
                        effect_type: &StatusEffect::BLINDNESS,
                        duration: 400,
                        amplifier: 0,
                        ambient: false,
                        show_particles: true,
                        show_icon: true,
                        blend: false,
                    });
                }
            }
        }
        mob.get_entity()
            .play_sound(Sound::EntityIllusionerCastSpell);
    }
}

impl PatrollingMonster for IllusionerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for IllusionerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityEvokerCelebrate
    }
}
