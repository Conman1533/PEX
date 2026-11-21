use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::Block;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, breed::BreedGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, move_to_block::MoveToBlockGoal, swim::SwimGoal,
        tempt::TemptGoal, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

/// Vanilla `fluid_tags/supports_frogspawn` is water.
fn supports_frogspawn(world: &World, pos: &BlockPos) -> bool {
    world.get_block(pos) == &Block::WATER
}

/// A frogspawn spot next to `below`, the block under a standing frog: water with air above.
fn frogspawn_spot(world: &World, below: &BlockPos) -> Option<BlockPos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .find_map(|(dx, dz)| {
            let side = BlockPos::new(below.0.x + dx, below.0.y, below.0.z + dz);
            (supports_frogspawn(world, &side) && world.get_block_state(&side.up()).is_air())
                .then(|| side.up())
        })
}

/// Vanilla frog `LAY_SPAWN` activity: a pregnant frog walks to land beside water
/// (`TryFindLandNearLiquid`, range 8) and lays frogspawn on the water
/// (`TryLaySpawnOnFluidNearLand`).
struct FrogLaySpawnGoal {
    frog: Weak<FrogEntity>,
    move_to_block: MoveToBlockGoal,
}

impl FrogLaySpawnGoal {
    fn new(frog: Weak<FrogEntity>) -> Self {
        let move_to_block = MoveToBlockGoal::new(
            1.0,
            8,
            1,
            Box::new(|world, pos| {
                !world.get_block_state(&pos).is_air()
                    && !supports_frogspawn(world, &pos)
                    && world.get_block_state(&pos.up()).is_air()
                    && frogspawn_spot(world, &pos).is_some()
            }),
        );
        Self {
            frog,
            move_to_block,
        }
    }

    fn pregnant(&self) -> bool {
        self.frog
            .upgrade()
            .is_some_and(|frog| frog.pregnant.load(Ordering::Relaxed))
    }
}

impl Goal for FrogLaySpawnGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.pregnant()
            && mob.get_mob_entity().get_target().is_none()
            && self.move_to_block.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.pregnant() && self.move_to_block.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_block.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.move_to_block.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_block.tick(mob);
        let entity = mob.get_entity();
        if entity.is_in_water() || !entity.on_ground.load(Ordering::Relaxed) {
            return;
        }
        let world = entity.world.load();
        let below = entity.block_pos.load().down();
        if let Some(spawn_pos) = frogspawn_spot(&world, &below) {
            world.set_block_state(
                &spawn_pos,
                Block::FROGSPAWN.default_state.id,
                BlockFlags::NOTIFY_ALL,
            );
            world.play_sound_fine(
                Sound::EntityFrogLaySpawn,
                pumpkin_data::sound::SoundCategory::Blocks,
                &entity.pos.load(),
                1.0,
                1.0,
            );
            if let Some(frog) = self.frog.upgrade() {
                frog.pregnant.store(false, Ordering::Relaxed);
            }
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.move_to_block.controls()
    }
}

use pumpkin_data::frog_variant::FrogVariant;

/// Represents a Frog, an amphibious mob that can eat small slimes and magma cubes.
///
/// Wiki: <https://minecraft.wiki/w/Frog>
pub struct FrogEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub variant: AtomicI32,
    pub tongue_target_id: AtomicI32,
    /// Vanilla `IS_PREGNANT` memory: set by breeding, cleared once frogspawn is laid.
    pub pregnant: AtomicBool,
}

impl FrogEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let frog = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            variant: AtomicI32::new(FrogVariant::Temperate.id() as i32),
            tongue_target_id: AtomicI32::new(-1),
            pregnant: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(frog);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            // Vanilla `FrogAi` idle activity: AnimalMakeLove (0) outranks FollowTemptation (1),
            // so a tempted frog in love still goes to its partner. Spawn laying is its own
            // activity while pregnant.
            goal_selector.add_goal(1, BreedGoal::new(1.0));
            goal_selector.add_goal(1, Box::new(FrogLaySpawnGoal::new(Arc::downgrade(&mob_arc))));
            goal_selector.add_goal(
                2,
                Box::new(TemptGoal::new(
                    1.0,
                    |_, stack| stack.get_item().has_tag(&tag::Item::MINECRAFT_FROG_FOOD),
                    false,
                )),
            );
            goal_selector.add_goal(3, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                4,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(5, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn get_variant(&self) -> FrogVariant {
        FrogVariant::from_id(self.variant.load(Ordering::Relaxed) as u32)
    }

    pub fn set_variant(&self, variant: FrogVariant) {
        self.variant.store(variant.id() as i32, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::frog::VARIANT,
            VarInt(variant.id() as i32),
        );
    }
}

impl AgeableMob for FrogEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn can_be_a_baby(&self) -> bool {
        false
    }
}

impl Animal for FrogEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_FROG_FOOD)
    }

    /// Vanilla `Frog.spawnChildFromBreeding`: no baby, the frog becomes pregnant instead.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Mob) {
        self.finalize_spawn_child_from_breeding(world, partner, None);
        self.pregnant.store(true, Ordering::Relaxed);
    }

    fn play_eating_sound(&self) {
        let entity = self.get_entity();
        entity.world.load().play_sound_fine(
            Sound::EntityFrogEat,
            pumpkin_data::sound::SoundCategory::Neutral,
            &entity.pos.load(),
            2.0,
            1.0,
        );
    }
}

impl Mob for FrogEntity {
    fn mob_step_sound(&self) -> crate::entity::mob::StepSound {
        crate::entity::mob::StepSound::Sound(pumpkin_data::sound::Sound::EntityFrogStep, 0.15)
    }

    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(pumpkin_data::sound::Sound::EntityFrogAmbient)
    }

    fn mob_get_component(
        &self,
        component: pumpkin_data::data_component::DataComponent,
    ) -> Option<Box<dyn pumpkin_data::data_component_impl::DataComponentImpl>> {
        use pumpkin_data::{data_component::DataComponent, data_component_impl::FrogVariantImpl};
        match component {
            DataComponent::FrogVariant => Some(Box::new(FrogVariantImpl {
                value: format!("minecraft:{}", self.get_variant().to_name()).into(),
            })),
            _ => None,
        }
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_string("variant", self.get_variant().asset_id().to_string());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(variant_str) = nbt.get_string("variant") {
            self.set_variant(FrogVariant::from_name(variant_str).unwrap_or_default());
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_set_variant_name(&self, name: &str) {
        self.set_variant(FrogVariant::from_name(name).unwrap_or_default());
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::frog::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::frog::VARIANT,
            VarInt(self.get_variant().id() as i32),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack)
    }
}
