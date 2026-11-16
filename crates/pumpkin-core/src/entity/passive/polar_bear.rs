use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::entity::EntityType;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag;
use pumpkin_nbt::compound::NbtCompound;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::behavior::neutral::apply_targets,
    ai::goal::{
        active_target::{ActiveTargetGoal, TargetCondition},
        escape_danger::EscapeDangerGoal,
        follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal,
        melee_attack::MeleeAttackGoal,
        revenge::RevengeGoal,
        swim::SwimGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        neutral::{NeutralData, NeutralMob},
    },
    passive::animal::Animal,
    player::Player,
};

fn is_baby(mob: &dyn Mob) -> bool {
    mob.as_ageable().is_some_and(AgeableMob::is_baby)
}

pub struct PolarBearEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub neutral_data: NeutralData,
    pub standing: AtomicBool,
    warning_sound_ticks: AtomicI32,
}

impl PolarBearEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let polar_bear = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            neutral_data: NeutralData::default(),
            standing: AtomicBool::new(false),
            warning_sound_ticks: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(polar_bear);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(
                1,
                Box::new(MeleeAttackGoal::new(1.25, true).warning_with(polar_bear_warning)),
            );
            goal_selector.add_goal(
                1,
                EscapeDangerGoal::new(2.0).with_panic_causes(|mob| {
                    if is_baby(mob) {
                        &tag::DamageType::MINECRAFT_PANIC_CAUSES
                    } else {
                        &tag::DamageType::MINECRAFT_PANIC_ENVIRONMENTAL_CAUSES
                    }
                }),
            );
            goal_selector.add_goal(4, Box::new(FollowParentGoal::new(1.25)));
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(7, Box::new(RandomLookAroundGoal::default()));
        };

        {
            let target_selector = &mob_arc.mob_entity.target_selector;

            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .raising_alarm_when(is_baby)
                        .alerting_only(|other| !is_baby(other)),
                ),
            );
            // TODO: PolarBearAttackPlayersGoal at 2 -> adults charge when a cub is near.
            apply_targets(target_selector, &mob_arc.mob_entity, 3, 5, false);
            target_selector.add_goal(
                4,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::FOX, true)
                    .when(TargetCondition::Adult),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_standing(&self) -> bool {
        self.standing.load(Ordering::Relaxed)
    }

    pub fn set_standing(&self, standing: bool) {
        if self.standing.swap(standing, Ordering::Relaxed) == standing {
            return;
        }
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::polar_bear::DATA_STANDING_ID,
            standing,
        );
    }
}

impl PolarBearEntity {
    /// Vanilla `PolarBear.playWarningSound`: at most every 2 seconds.
    pub fn play_warning_sound(&self) {
        if self.warning_sound_ticks.load(Ordering::Relaxed) <= 0 {
            self.mob_entity
                .living_entity
                .make_sound(Sound::EntityPolarBearWarning);
            self.warning_sound_ticks.store(40, Ordering::Relaxed);
        }
    }
}

/// Stand-up warning of vanilla `PolarBearMeleeAttackGoal`.
fn polar_bear_warning(mob: &dyn Mob, standing: bool) {
    let Some(bear) = mob.cast_any().downcast_ref::<PolarBearEntity>() else {
        return;
    };
    bear.set_standing(standing);
    if standing {
        bear.play_warning_sound();
    }
}

impl AgeableMob for PolarBearEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for PolarBearEntity {
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }
}

impl NeutralMob for PolarBearEntity {
    fn get_neutral_data(&self) -> &NeutralData {
        &self.neutral_data
    }
}

impl Mob for PolarBearEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.is_baby() {
            pumpkin_data::sound::Sound::EntityPolarBearAmbientBaby
        } else {
            pumpkin_data::sound::Sound::EntityPolarBearAmbient
        })
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn as_neutral(&self) -> Option<&dyn NeutralMob> {
        Some(self)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let ticks = self.warning_sound_ticks.load(Ordering::Relaxed);
        if ticks > 0 {
            self.warning_sound_ticks.store(ticks - 1, Ordering::Relaxed);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::polar_bear::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::polar_bear::DATA_STANDING_ID,
            self.is_standing(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}
