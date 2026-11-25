use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use uuid::Uuid;

use pumpkin_data::BlockDirection;
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
        look_at_entity::LookAtEntityGoal, revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks,
        wander_around::WanderAroundGoal,
    },
    mob::vex::VexEntity,
    mob::{
        Mob, MobEntity,
        patrol::{PatrolData, PatrollingMonster},
        raider::{
            Raider, RaiderData, is_abstract_villager, raider_can_attack, register_raider_goals,
        },
        spawn::finalize_spawn,
    },
    projectile::evoker_fangs::EvokerFangsEntity,
    r#type::from_type,
};
use crate::world::vibration::{GameEventContext, entity_handle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IllagerSpell {
    None = 0,
    SummonVex = 1,
    Fangs = 2,
    Wololo = 3,
    Disappear = 4,
    Blindness = 5,
}

impl IllagerSpell {
    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::SummonVex,
            2 => Self::Fangs,
            3 => Self::Wololo,
            4 => Self::Disappear,
            5 => Self::Blindness,
            _ => Self::None,
        }
    }
}

pub struct EvokerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    spell_casting_tick_count: AtomicI32,
    current_spell: AtomicU8,
    /// Vanilla `Evoker.wololoTarget`, by entity id.
    wololo_target_id: Mutex<Option<i32>>,
}

impl EvokerEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let evoker = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            spell_casting_tick_count: AtomicI32::new(0),
            current_spell: AtomicU8::new(IllagerSpell::None as u8),
            wololo_target_id: Mutex::new(None),
        };
        let mob_arc = Arc::new(evoker);
        let mob_weak: Weak<Self> = Arc::downgrade(&mob_arc);
        let mob_interface_weak: Weak<dyn Mob> = {
            let mob_dyn: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_dyn)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            register_raider_goals(goal_selector);

            // Vanilla `Evoker.registerGoals`.
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(EvokerCastingSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(
                2,
                Box::new(AvoidEntityGoal::new(&EntityType::PLAYER, 8.0, 0.6, 1.0)),
            );
            goal_selector.add_goal(
                3,
                Box::new(AvoidEntityGoal::new(&EntityType::CREAKING, 8.0, 0.6, 1.0)),
            );
            goal_selector.add_goal(
                4,
                Box::new(EvokerUseSpellGoal::new(
                    mob_weak.clone(),
                    EvokerSpell::SummonVex,
                )),
            );
            goal_selector.add_goal(
                5,
                Box::new(EvokerUseSpellGoal::new(
                    mob_weak.clone(),
                    EvokerSpell::Fangs,
                )),
            );
            goal_selector.add_goal(
                6,
                Box::new(EvokerUseSpellGoal::new(mob_weak, EvokerSpell::Wololo)),
            );
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_interface_weak,
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true)
                    .remembering_unseen(300),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 10, false, |target, _| {
                    is_abstract_villager(target.entity.entity_type)
                })
                .remembering_unseen(300),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, false),
            );
        };

        mob_arc
    }

    pub fn is_casting_spell(&self) -> bool {
        self.spell_casting_tick_count.load(Ordering::Relaxed) > 0
    }

    pub fn set_is_casting_spell(&self, spell: IllagerSpell) {
        self.current_spell.store(spell as u8, Ordering::Relaxed);
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_synced_data(
            pumpkin_data::tracked_data::evoker::SPELL_CASTING_ID,
            spell as u8 as i8,
        );
    }

    pub fn get_current_spell(&self) -> IllagerSpell {
        IllagerSpell::from_u8(self.current_spell.load(Ordering::Relaxed))
    }

    pub fn get_spell_casting_time(&self) -> i32 {
        self.spell_casting_tick_count.load(Ordering::Relaxed)
    }

    pub fn set_spell_casting_time(&self, ticks: i32) {
        self.spell_casting_tick_count
            .store(ticks, Ordering::Relaxed);
    }

    fn set_wololo_target(&self, id: Option<i32>) {
        *self
            .wololo_target_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = id;
    }

    /// Vanilla `Evoker.getWololoTarget`: the sheep picked for the wololo spell, while it is
    /// still in the world.
    fn wololo_target(&self) -> Option<Arc<dyn EntityBase>> {
        let id = (*self
            .wololo_target_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner))?;
        self.mob_entity
            .living_entity
            .entity
            .world
            .load()
            .get_entity_by_id(id)
    }
}

impl Mob for EvokerEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityEvokerAmbient)
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        nbt.put_int("SpellTicks", self.get_spell_casting_time());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        if let Some(ticks) = nbt.get_int("SpellTicks") {
            self.set_spell_casting_time(ticks);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `AbstractIllager.canAttack`: no baby villagers.
    fn can_attack(&self, target: &dyn EntityBase) -> bool {
        raider_can_attack(self, target)
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    /// Vanilla `SpellcasterIllager.customServerAiStep`.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let ticks = self.spell_casting_tick_count.load(Ordering::Relaxed);
        if ticks > 0 {
            self.spell_casting_tick_count
                .store(ticks - 1, Ordering::Relaxed);
        }
    }
}

impl PatrollingMonster for EvokerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for EvokerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityEvokerCelebrate
    }
}

/// Vanilla `Evoker.EvokerCastingSpellGoal`: stand still while a spell is cast, facing the
/// target or the wololo sheep.
pub struct EvokerCastingSpellGoal {
    evoker: Weak<EvokerEntity>,
}

impl EvokerCastingSpellGoal {
    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self { evoker }
    }
}

impl Goal for EvokerCastingSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.evoker
            .upgrade()
            .is_some_and(|evoker| evoker.is_casting_spell())
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            evoker.set_is_casting_spell(IllagerSpell::None);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        let look_at = mob
            .get_mob_entity()
            .get_target()
            .or_else(|| evoker.wololo_target());
        if let Some(look_at) = look_at {
            mob.get_mob_entity()
                .look_control
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .look_at_entity_with_range(
                    &look_at,
                    mob.get_max_head_rotation(),
                    mob.get_max_look_pitch_change(),
                );
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// The evoker's three `SpellcasterUseSpellGoal`s.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EvokerSpell {
    /// `EvokerSummonSpellGoal`: three vexes.
    SummonVex,
    /// `EvokerAttackSpellGoal`: a line or two rings of fangs.
    Fangs,
    /// `EvokerWololoSpellGoal`: turn a blue sheep red.
    Wololo,
}

impl EvokerSpell {
    /// Vanilla `getCastWarmupTime`.
    const fn cast_warmup_time(self) -> i32 {
        match self {
            Self::Wololo => 40,
            Self::SummonVex | Self::Fangs => 20,
        }
    }

    /// Vanilla `getCastingTime`.
    const fn casting_time(self) -> i32 {
        match self {
            Self::SummonVex => 100,
            Self::Fangs => 40,
            Self::Wololo => 60,
        }
    }

    /// Vanilla `getCastingInterval`.
    const fn casting_interval(self) -> i32 {
        match self {
            Self::SummonVex => 340,
            Self::Fangs => 100,
            Self::Wololo => 140,
        }
    }

    const fn prepare_sound(self) -> Sound {
        match self {
            Self::SummonVex => Sound::EntityEvokerPrepareSummon,
            Self::Fangs => Sound::EntityEvokerPrepareAttack,
            Self::Wololo => Sound::EntityEvokerPrepareWololo,
        }
    }

    const fn illager_spell(self) -> IllagerSpell {
        match self {
            Self::SummonVex => IllagerSpell::SummonVex,
            Self::Fangs => IllagerSpell::Fangs,
            Self::Wololo => IllagerSpell::Wololo,
        }
    }
}

/// Vanilla `SpellcasterIllager.SpellcasterUseSpellGoal` with the evoker's spells.
pub struct EvokerUseSpellGoal {
    evoker: Weak<EvokerEntity>,
    spell: EvokerSpell,
    attack_warmup_delay: i32,
    next_attack_tick_count: i32,
}

impl EvokerUseSpellGoal {
    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>, spell: EvokerSpell) -> Self {
        Self {
            evoker,
            spell,
            attack_warmup_delay: 0,
            next_attack_tick_count: 0,
        }
    }

    /// Vanilla `SpellcasterUseSpellGoal.canUse`.
    fn can_use_spell(&self, evoker: &EvokerEntity) -> bool {
        evoker
            .mob_entity
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive())
            && !evoker.is_casting_spell()
            && evoker
                .mob_entity
                .living_entity
                .entity
                .tick_count
                .load(Ordering::Relaxed)
                >= self.next_attack_tick_count
    }

    /// Vanilla `EvokerSummonSpellGoal.canUse`: fewer vexes within 16 blocks than a roll of 1
    /// to 8.
    fn can_summon(evoker: &EvokerEntity) -> bool {
        let entity = &evoker.mob_entity.living_entity.entity;
        let pos = entity.pos.load();
        let vexes = entity
            .world
            .load()
            .get_entities_at_box(&entity.bounding_box.load().expand(16.0, 16.0, 16.0))
            .iter()
            .filter(|candidate| {
                let candidate = candidate.get_entity();
                candidate.entity_type == &EntityType::VEX
                    && candidate.is_alive()
                    && candidate.pos.load().squared_distance_to_vec(&pos) <= 16.0 * 16.0
            })
            .count();
        evoker.get_random().random_range(0..8) + 1 > vexes
    }

    /// Vanilla `EvokerWololoSpellGoal.canUse`: a random blue sheep within 16 blocks that the
    /// evoker can see, with `mobGriefing` on and no target.
    fn pick_wololo_target(&self, evoker: &EvokerEntity) -> bool {
        let entity = &evoker.mob_entity.living_entity.entity;
        if evoker.mob_entity.get_target().is_some()
            || evoker.is_casting_spell()
            || entity.tick_count.load(Ordering::Relaxed) < self.next_attack_tick_count
        {
            return false;
        }
        let world = entity.world.load();
        if !world.level_info.load().game_rules.mob_griefing {
            return false;
        }
        let pos = entity.pos.load();
        let sheep: Vec<i32> = world
            .get_entities_at_box(&entity.bounding_box.load().expand(16.0, 4.0, 16.0))
            .iter()
            .filter(|candidate| {
                let sheep_entity = candidate.get_entity();
                sheep_entity.is_alive()
                    && sheep_entity.pos.load().squared_distance_to_vec(&pos) <= 16.0 * 16.0
                    && candidate
                        .cast_any()
                        .downcast_ref::<crate::entity::passive::sheep::SheepEntity>()
                        .is_some_and(|sheep| sheep.get_color() == BLUE)
                    && evoker.has_line_of_sight(sheep_entity)
            })
            .map(|candidate| candidate.get_entity().entity_id)
            .collect();
        if sheep.is_empty() {
            return false;
        }
        let picked = sheep[evoker.get_random().random_range(0..sheep.len())];
        evoker.set_wololo_target(Some(picked));
        true
    }

    /// Vanilla `EvokerAttackSpellGoal.performSpellCasting`: two rings of fangs around the
    /// evoker when the target is within 3 blocks, a line of 16 toward it otherwise.
    fn cast_fangs(evoker: &EvokerEntity, target: &Arc<dyn EntityBase>) {
        let evoker_pos = evoker.mob_entity.living_entity.entity.pos.load();
        let target_pos = target.get_entity().pos.load();
        let min_y = target_pos.y.min(evoker_pos.y);
        let max_y = target_pos.y.max(evoker_pos.y) + 1.0;
        let angle_towards_target =
            (target_pos.z - evoker_pos.z).atan2(target_pos.x - evoker_pos.x) as f32;
        if evoker_pos.squared_distance_to_vec(&target_pos) < 9.0 {
            for i in 0..5 {
                let angle = (i as f32).mul_add(std::f32::consts::PI * 0.4, angle_towards_target);
                Self::create_spell_entity(evoker, angle, 1.5, min_y, max_y, 0);
            }
            for i in 0..8 {
                let angle = (i as f32).mul_add(
                    std::f32::consts::PI * 2.0 / 8.0,
                    angle_towards_target + std::f32::consts::PI * 2.0 / 5.0,
                );
                Self::create_spell_entity(evoker, angle, 2.5, min_y, max_y, 3);
            }
        } else {
            for i in 0..16 {
                let reach = 1.25 * f64::from(i + 1);
                Self::create_spell_entity(
                    evoker,
                    angle_towards_target,
                    reach,
                    min_y,
                    max_y,
                    i as u32,
                );
            }
        }
    }

    /// Vanilla `EvokerAttackSpellGoal.createSpellEntity`: fangs `reach` blocks from the evoker
    /// along `angle`, on the first block with a sturdy top found going down from `max_y` to one
    /// below `min_y`, or nowhere.
    fn create_spell_entity(
        evoker: &EvokerEntity,
        angle: f32,
        reach: f64,
        min_y: f64,
        max_y: f64,
        delay_ticks: u32,
    ) {
        let entity = &evoker.mob_entity.living_entity.entity;
        let evoker_pos = entity.pos.load();
        let x = f64::from(angle.cos()).mul_add(reach, evoker_pos.x);
        let z = f64::from(angle.sin()).mul_add(reach, evoker_pos.z);
        let world = entity.world.load_full();
        let mut pos = BlockPos::containing(x, max_y, z);
        let lowest = min_y.floor() as i32 - 1;
        let mut top_offset = 0.0;
        let found = loop {
            let below = pos.down();
            if world
                .get_block_state(&below)
                .is_side_solid(BlockDirection::Up)
            {
                let state = world.get_block_state(&pos);
                if !state.is_air() {
                    top_offset = state
                        .get_block_collision_shapes()
                        .map(|shape| shape.max.y)
                        .fold(0.0, f64::max);
                }
                break true;
            }
            pos = below;
            if pos.0.y < lowest {
                break false;
            }
        };
        if !found {
            return;
        }
        let fangs_pos = Vector3::new(x, f64::from(pos.0.y) + top_offset, z);
        let fangs_entity = Entity::from_uuid(
            Uuid::new_v4(),
            world.clone(),
            fangs_pos,
            &EntityType::EVOKER_FANGS,
        );
        world.spawn_entity(Arc::new(EvokerFangsEntity::new(
            fangs_entity,
            delay_ticks,
            angle,
            Some(entity.entity_id),
        )));
        world.game_event(
            GameEvent::EntityPlace,
            fangs_pos,
            GameEventContext::of(entity_handle(&world, entity), None),
        );
    }

    /// Vanilla `EvokerSummonSpellGoal.performSpellCasting`: three vexes bound to spots around
    /// the evoker, living 30 to 119 seconds.
    fn summon_vexes(evoker: &Arc<EvokerEntity>) {
        let entity = &evoker.mob_entity.living_entity.entity;
        let world = entity.world.load_full();
        let owner: Arc<dyn EntityBase> = evoker.clone();
        for _ in 0..3 {
            let (dx, dz, life) = {
                let mut random = evoker.get_random();
                (
                    random.random_range(0..5) - 2,
                    random.random_range(0..5) - 2,
                    20 * (30 + random.random_range(0..90)),
                )
            };
            let pos = entity.block_pos.load().offset(Vector3::new(dx, 1, dz));
            let spawn_pos = Vector3::new(
                f64::from(pos.0.x) + 0.5,
                f64::from(pos.0.y),
                f64::from(pos.0.z) + 0.5,
            );
            let vex = from_type(&EntityType::VEX, spawn_pos, &world, Uuid::new_v4());
            finalize_spawn(&vex, &world, None);
            if let Some(summoned) = vex.cast_any().downcast_ref::<VexEntity>() {
                summoned.set_owner(&owner);
                summoned.set_bound_origin(Some(pos));
                summoned.set_limited_life(life);
            }
            world.spawn_entity(vex);
            world.game_event(
                GameEvent::EntityPlace,
                pos.to_centered_f64(),
                GameEventContext::of(entity_handle(&world, entity), None),
            );
        }
    }

    /// Vanilla `EvokerWololoSpellGoal.performSpellCasting`.
    fn cast_wololo(evoker: &EvokerEntity) {
        if let Some(target) = evoker.wololo_target()
            && target.get_entity().is_alive()
            && let Some(sheep) = target
                .cast_any()
                .downcast_ref::<crate::entity::passive::sheep::SheepEntity>()
        {
            sheep.set_color(RED);
        }
    }
}

/// `DyeColor.BLUE` and `DyeColor.RED` ids.
const BLUE: u8 = 11;
const RED: u8 = 14;

impl Goal for EvokerUseSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        match self.spell {
            EvokerSpell::SummonVex => self.can_use_spell(&evoker) && Self::can_summon(&evoker),
            EvokerSpell::Fangs => self.can_use_spell(&evoker),
            EvokerSpell::Wololo => self.pick_wololo_target(&evoker),
        }
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        let ready = match self.spell {
            EvokerSpell::Wololo => evoker.wololo_target().is_some(),
            EvokerSpell::SummonVex | EvokerSpell::Fangs => evoker
                .mob_entity
                .get_target()
                .is_some_and(|target| target.get_entity().is_alive()),
        };
        ready && self.attack_warmup_delay > 0
    }

    fn start(&mut self, _mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        self.attack_warmup_delay = to_goal_ticks(self.spell.cast_warmup_time());
        evoker.set_spell_casting_time(self.spell.casting_time());
        let entity = &evoker.mob_entity.living_entity.entity;
        self.next_attack_tick_count =
            entity.tick_count.load(Ordering::Relaxed) + self.spell.casting_interval();
        entity.play_sound(self.spell.prepare_sound());
        evoker.set_is_casting_spell(self.spell.illager_spell());
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if self.spell == EvokerSpell::Wololo
            && let Some(evoker) = self.evoker.upgrade()
        {
            evoker.set_wololo_target(None);
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        self.attack_warmup_delay -= 1;
        if self.attack_warmup_delay != 0 {
            return;
        }
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        match self.spell {
            EvokerSpell::SummonVex => Self::summon_vexes(&evoker),
            EvokerSpell::Fangs => {
                if let Some(target) = evoker.mob_entity.get_target() {
                    Self::cast_fangs(&evoker, &target);
                }
            }
            EvokerSpell::Wololo => Self::cast_wololo(&evoker),
        }
        evoker
            .mob_entity
            .living_entity
            .entity
            .play_sound(Sound::EntityEvokerCastSpell);
    }
}
