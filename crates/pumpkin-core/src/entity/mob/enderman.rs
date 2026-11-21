use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use crate::entity::attributes::Modifier;
use crate::entity::attributes::ModifierOperation;
use crate::world::{World, game_event::GameEventContext, loot::LootContextParameters};
use pumpkin_data::{Block, BlockState, BlockStateId, attributes::Attributes};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::{
    Enchantment,
    damage::DamageType,
    data_component_impl::EquipmentSlot,
    entity::{EntityStatus, EntityType},
    item::Item,
    item_stack::ItemStack,
    sound::{Sound, SoundCategory},
    tag,
    tag::Taggable,
    world::WorldEvent,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{
            active_target::ActiveTargetGoal, chase_player::ChasePlayerGoal,
            look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
            melee_attack::MeleeAttackGoal, pick_up_block::PickUpBlockGoal,
            place_block::PlaceBlockGoal, reset_universal_anger::ResetUniversalAngerGoal,
            revenge::RevengeGoal, swim::SwimGoal,
            teleport_towards_player::TeleportTowardsPlayerGoal, wander_around::WanderAroundGoal,
        },
        pathfinder::node::PathType,
    },
    mob::{
        Mob, MobEntity,
        neutral::{NeutralData, NeutralMob},
    },
    player::Player,
};

const SPEED_BOOST: f64 = 0.15;
const ENDERMAN_SPEED_BOOST_ID: &str = "minecraft:attacking";

pub const ENDERMAN_EYE_HEIGHT: f64 = 2.55;
pub const ENDERMAN_BODY_Y_OFFSET: f64 = 1.45;
pub const PLAYER_EYE_HEIGHT: f64 = 1.62;

fn is_projectile_damage(dt: DamageType) -> bool {
    let (names, _) = pumpkin_data::tag::DamageType::MINECRAFT_IS_PROJECTILE;
    names.contains(&dt.message_id)
}

pub struct EndermanEntity {
    pub mob_entity: MobEntity,
    neutral_data: NeutralData,
    carried_block: AtomicCell<Option<BlockStateId>>,
    /// Synced CREEPY flag, the open-mouth pose. Anger lives in `NeutralMob`.
    creepy: AtomicBool,
    provoked: AtomicBool,
    speed_boosted: AtomicBool,
    /// Vanilla `targetChangeTime`: the tick the current target was set, 0 without one.
    target_change_time: AtomicI32,
}

impl EndermanEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let entity = Self {
            mob_entity,
            neutral_data: NeutralData::default(),
            carried_block: AtomicCell::new(None),
            creepy: AtomicBool::new(false),
            provoked: AtomicBool::new(false),
            speed_boosted: AtomicBool::new(false),
            target_change_time: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(entity);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        let mut navigator = mob_arc
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        navigator.set_pathfinding_malus(PathType::Water, -1.0);
        drop(navigator);

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ChasePlayerGoal::new(&mob_arc)));
            goal_selector.add_goal(2, Box::new(MeleeAttackGoal::new(1.0, false)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(10, Box::new(PlaceBlockGoal::new(&mob_arc)));
            goal_selector.add_goal(11, Box::new(PickUpBlockGoal::new(&mob_arc)));

            target_selector.add_goal(1, Box::new(TeleportTowardsPlayerGoal::new(&mob_arc)));
            target_selector.add_goal(2, Box::new(RevengeGoal::new(true)));
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::ENDERMITE, true),
            );
            target_selector.add_goal(4, ResetUniversalAngerGoal::new(false));
        };

        mob_arc
    }

    /// Vanilla `Enderman.teleport()`.
    pub fn teleport_randomly(&self) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return false;
        }
        let pos = entity.pos.load();
        let (x, y, z) = {
            let mut rng = self.get_random();
            (
                pos.x + (rng.random_range(0.0..1.0) - 0.5) * 64.0,
                pos.y + (rng.random_range(0i32..64) - 32) as f64,
                pos.z + (rng.random_range(0.0..1.0) - 0.5) * 64.0,
            )
        };

        self.teleport_to(x, y, z)
    }

    pub fn teleport_towards(&self, target: &dyn EntityBase) -> bool {
        let entity = &self.mob_entity.living_entity.entity;
        let pos = entity.pos.load();
        let target_pos = target.get_entity().pos.load();

        let dx = pos.x - target_pos.x;
        let dy = (pos.y + ENDERMAN_BODY_Y_OFFSET) - target.get_entity().get_eye_y();
        let dz = pos.z - target_pos.z;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist < 1e-6 {
            return false;
        }

        let nx = dx / dist;
        let ny = dy / dist;
        let nz = dz / dist;
        let (x, y, z) = {
            let mut rng = self.get_random();
            (
                pos.x + (rng.random_range(0.0..1.0) - 0.5) * 8.0 - nx * 16.0,
                pos.y + (rng.random_range(0i32..16) - 8) as f64 - ny * 16.0,
                pos.z + (rng.random_range(0.0..1.0) - 0.5) * 8.0 - nz * 16.0,
            )
        };

        self.teleport_to(x, y, z)
    }

    /// Vanilla `Enderman.teleport(x, y, z)` over `LivingEntity.randomTeleport`.
    pub fn teleport_to(&self, x: f64, y: f64, z: f64) -> bool {
        if self.is_passenger() {
            return false;
        }
        let entity = &self.mob_entity.living_entity.entity;
        let origin = entity.pos.load();
        let world = entity.world.load();

        let Some(new_pos) = self.find_teleport_spot(&world, x, y, z) else {
            return false;
        };

        if let Some(server) = world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::entity::entity_teleport::EntityTeleportEvent::new(
                    entity.entity_id,
                    origin,
                    new_pos,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return false;
            }
        }

        entity.teleport(new_pos, None, None, &world);
        // Entity event 46: the client draws the portal particles at both ends.
        world.send_entity_status(entity, EntityStatus::Teleport, ActorEventID::None);
        self.mob_entity.steered().navigator().stop();

        world.emit_game_event_with_context(
            "teleport",
            origin,
            &GameEventContext {
                source_entity: Some(self),
            },
        );
        if !entity.is_silent() {
            let old_block = BlockPos::floored_v(origin);
            let new_block = entity.block_pos.load();
            let clamped = BlockPos::new(
                old_block.0.x + (new_block.0.x - old_block.0.x).clamp(-127, 127),
                old_block.0.y + (new_block.0.y - old_block.0.y).clamp(-127, 127),
                old_block.0.z + (new_block.0.z - old_block.0.z).clamp(-127, 127),
            );
            let packed = old_block.pack_difference_in_position(&clamped, 127, 127, 127);
            world.sync_world_event(WorldEvent::ParticlesEndermanTeleport, old_block, packed);
            for pos in &[origin, new_pos] {
                world.play_sound(Sound::EntityEndermanTeleport, SoundCategory::Hostile, pos);
            }
        }

        true
    }

    /// Vanilla `LivingEntity.randomTeleport`: clamp into the border, walk down to the first block
    /// entities can teleport onto, then check that block, the space and the block below.
    fn find_teleport_spot(&self, world: &World, x: f64, y: f64, z: f64) -> Option<Vector3<f64>> {
        let (x, z) = {
            let border = world
                .worldborder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let half = border.new_diameter / 2.0;
            (
                x.clamp(
                    border.center_x - half,
                    (border.center_x + half - 1.0e-5).max(border.center_x - half),
                ),
                z.clamp(
                    border.center_z - half,
                    (border.center_z + half - 1.0e-5).max(border.center_z - half),
                ),
            )
        };
        let min_y = world.dimension.min_y;
        let max_y = min_y + world.dimension.height - 1;
        let mut pos = BlockPos::floored(x, y, z);
        // Vanilla `hasChunkAt` only looks at the column.
        let column = BlockPos::new(pos.0.x, pos.0.y.clamp(min_y, max_y), pos.0.z);
        world.get_block_state_if_loaded(&column)?;

        let mut y = y;
        while pos.0.y > min_y {
            pos = pos.down();
            let state = if pos.0.y > max_y {
                BlockState::from_id(Block::AIR.default_state.id)
            } else {
                world.get_block_state_if_loaded(&pos)?
            };
            let block = Block::from_state_id(state.id);
            if block.has_tag(&tag::Block::MINECRAFT_ENTITIES_CAN_TELEPORT_TO) {
                return self
                    .can_teleport_onto(world, block, x, y, z)
                    .then(|| Vector3::new(x, y, z));
            }
            y -= 1.0;
        }
        None
    }

    /// Vanilla `checkPositionAndTeleport` with the enderman's avoidance tag and
    /// `canRandomlyTeleportTo` (no water right below).
    fn can_teleport_onto(&self, world: &World, ground: &Block, x: f64, y: f64, z: f64) -> bool {
        let avoid = &tag::Block::MINECRAFT_ENDERMAN_DOES_NOT_TELEPORT_TO;
        if ground.has_tag(avoid) {
            return false;
        }
        let entity = &self.mob_entity.living_entity.entity;
        let bb = BoundingBox::new_from_pos(x, y, z, &entity.entity_dimension.load());
        if !world.is_space_empty(bb) || world.contains_any_liquid(bb) {
            return false;
        }
        if BlockPos::iterate(bb.min_block_pos(), bb.max_block_pos())
            .any(|pos| world.get_block(&pos).has_tag(avoid))
        {
            return false;
        }
        !world
            .get_fluid(&BlockPos::floored(x, y, z).down())
            .has_tag(&tag::Fluid::MINECRAFT_WATER)
    }

    /// Vanilla `repeatedlyTryToTeleport`.
    fn repeatedly_try_to_teleport(&self) {
        if self.is_passenger() {
            return;
        }
        for _ in 0..64 {
            if self.teleport_randomly() {
                return;
            }
        }
    }

    pub fn set_target(&self, target: Option<Arc<dyn EntityBase>>) {
        let is_some = target.is_some();
        let mut mob_target = self
            .mob_entity
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *mob_target = target;
        drop(mob_target);

        let entity = &self.mob_entity.living_entity.entity;
        self.target_change_time.store(
            if is_some {
                entity.tick_count.load(Ordering::Relaxed)
            } else {
                0
            },
            Ordering::Relaxed,
        );

        if is_some {
            self.set_creepy(true);
            // Use attribute modifier instead of direct speed arithmetic
            if !self.speed_boosted.swap(true, Ordering::Relaxed) {
                let living = &self.mob_entity.living_entity;
                let modifier = Modifier {
                    id: ENDERMAN_SPEED_BOOST_ID.to_string(),
                    amount: SPEED_BOOST,
                    operation: ModifierOperation::Add,
                };

                living.update_attribute(&Attributes::MOVEMENT_SPEED, |inst| {
                    inst.add_or_replace_modifier(modifier);
                });

                crate::entity::attributes::send_attribute_updates_for_living(
                    living,
                    vec![Attributes::MOVEMENT_SPEED],
                );
            }
        } else {
            self.set_creepy(false);
            self.set_provoked(false);
            if self.speed_boosted.swap(false, Ordering::Relaxed) {
                let living = &self.mob_entity.living_entity;

                living.update_attribute(&Attributes::MOVEMENT_SPEED, |inst| {
                    inst.remove_modifier(ENDERMAN_SPEED_BOOST_ID);
                });

                crate::entity::attributes::send_attribute_updates_for_living(
                    living,
                    vec![Attributes::MOVEMENT_SPEED],
                );
            }
        }
    }

    pub fn set_creepy(&self, creepy: bool) {
        self.creepy.store(creepy, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::enderman::CREEPY, creepy);
    }

    pub fn is_creepy(&self) -> bool {
        self.creepy.load(Ordering::Relaxed)
    }

    pub fn set_provoked(&self, provoked: bool) {
        self.provoked.store(provoked, Ordering::Relaxed);
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::enderman::STARED_AT, provoked);
    }

    pub fn set_carried_block(&self, block_state: Option<BlockStateId>) {
        self.carried_block.store(block_state);
        let value = block_state.map_or(VarInt(0), |id| VarInt(id.as_u16() as i32));
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::enderman::CARRY_STATE, value);
    }

    pub fn get_carried_block(&self) -> Option<BlockStateId> {
        self.carried_block.load()
    }

    pub fn is_player_staring(&self, player: &Player) -> bool {
        let equipment = player.living_entity.entity_equipment.try_lock();
        if let Ok(equipment) = equipment
            && let Some(head_stack) = equipment.equipment.get(&EquipmentSlot::HEAD)
            && !head_stack.is_empty()
            && head_stack
                .item
                .has_tag(&tag::Item::MINECRAFT_GAZE_DISGUISE_EQUIPMENT)
        {
            return false;
        }

        // Vanilla `isLookingAtMe(player, 0.025, true, false, getEyeY())`.
        let entity = &self.mob_entity.living_entity.entity;
        let enderman_pos = entity.pos.load();
        let enderman_eye_y = entity.get_eye_y();

        let player_entity = player.get_entity();
        let player_pos = player_entity.pos.load();
        let player_eye_y = player_entity.get_eye_y();

        let pitch = player_entity.pitch.load().to_radians();
        let yaw = -player_entity.yaw.load().to_radians();

        let cos_pitch = pitch.cos();
        let look_dir = Vector3::new(
            (yaw.sin() * cos_pitch) as f64,
            (-pitch.sin()) as f64,
            (yaw.cos() * cos_pitch) as f64,
        );

        let dx = enderman_pos.x - player_pos.x;
        let dy = enderman_eye_y - player_eye_y;
        let dz = enderman_pos.z - player_pos.z;
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();

        if distance < 0.1 {
            return false;
        }

        let dir_x = dx / distance;
        let dir_y = dy / distance;
        let dir_z = dz / distance;

        let dot = look_dir.x * dir_x + look_dir.y * dir_y + look_dir.z * dir_z;

        if dot <= 1.0 - 0.025 / distance {
            return false;
        }

        // Vanilla `hasLineOfSight(enderman, COLLIDER, NONE, eyeY)` from the player's eyes:
        // glass and other see-through blocks still hide the stare.
        let enderman_eye_pos = Vector3::new(enderman_pos.x, enderman_eye_y, enderman_pos.z);
        let player_eye_pos = Vector3::new(player_pos.x, player_eye_y, player_pos.z);
        if enderman_eye_pos.squared_distance_to_vec(&player_eye_pos) > 128.0 * 128.0 {
            return false;
        }
        let world = entity.world.load();
        world
            .clip(&pumpkin_world::clip::ClipContext::new(
                player_eye_pos,
                enderman_eye_pos,
                pumpkin_world::clip::ClipBlock::Collider,
                pumpkin_world::clip::ClipFluid::None,
            ))
            .is_miss
    }
}

impl NeutralMob for EndermanEntity {
    fn get_neutral_data(&self) -> &NeutralData {
        &self.neutral_data
    }
}

impl Mob for EndermanEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.creepy.load(std::sync::atomic::Ordering::Relaxed) {
            pumpkin_data::sound::Sound::EntityEndermanScream
        } else {
            pumpkin_data::sound::Sound::EntityEndermanAmbient
        })
    }

    fn as_neutral(&self) -> Option<&dyn NeutralMob> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(block_state) = self.carried_block.load() {
            nbt.put(
                "carriedBlockState",
                pumpkin_world::chunk::format::block_state_tag(block_state),
            );
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(block_state) = nbt
            .get("carriedBlockState")
            .and_then(pumpkin_world::chunk::format::block_state_from_tag)
        {
            self.set_carried_block(BlockStateId::new(block_state.as_u16()));
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn is_allowed_in_peaceful(&self) -> bool {
        false
    }

    fn mob_metadata(&self) -> Option<crate::entity::mob::MobMetadata> {
        self.is_creepy()
            .then(|| crate::entity::mob::MobMetadata::new().screaming(true))
    }

    fn set_mob_target(&self, target: Option<Arc<dyn EntityBase>>) {
        self.set_target(self.as_valid_target(target));
    }

    /// Vanilla `requiresCustomPersistence`: an enderman holding a block never despawns.
    fn requires_custom_persistence(&self) -> bool {
        self.carried_block.load().is_some()
    }

    /// Vanilla `Enderman.customServerAiStep`: in daylight, ten seconds after its last target
    /// change, an enderman under open sky may drop its target and teleport away.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        if !world.is_bright_outside()
            || entity.tick_count.load(Ordering::Relaxed)
                < self.target_change_time.load(Ordering::Relaxed) + 600
        {
            return;
        }
        let brightness =
            world.get_light_level_dependent_magic_value(&BlockPos::floored_v(entity.get_eye_pos()));
        if brightness > 0.5
            && world.can_see_sky(&entity.block_pos.load())
            && self.get_random().random::<f32>() * 30.0 < (brightness - 0.4) * 2.0
        {
            self.set_target(None);
            self.teleport_randomly();
        }
    }

    // TODO: angerable system, ambient sound override
    fn mob_tick(&self, caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }

        let world = entity.world.load();
        let raining_at_feet = world.is_raining_at(&entity.block_pos.load());
        let raining_at_head = world.is_raining_at(&entity.bounding_box.load().max_block_pos());
        if entity.touching_water.load(Ordering::SeqCst) || raining_at_feet || raining_at_head {
            caller.damage(caller, 1.0, DamageType::DROWN);
        }
    }

    /// Vanilla `Enderman.hurtServer`: projectiles never hurt an enderman that is not riding
    /// anything, even when all 64 teleport attempts fail.
    fn pre_damage(&self, damage_type: DamageType, _source: Option<&dyn EntityBase>) -> bool {
        if is_projectile_damage(damage_type) && !self.is_passenger() {
            self.repeatedly_try_to_teleport();
            return false;
        }
        true
    }

    fn on_damage(&self, _damage_type: DamageType, source: Option<&dyn EntityBase>) {
        if source.is_some_and(|s| s.get_living_entity().is_some()) {
            return;
        }
        let should_teleport = self.get_random().random_range(0..10) != 0;
        if should_teleport {
            self.teleport_randomly();
        }
    }

    /// Vanilla `Enderman.dropCustomDeathLoot`: the carried block drops as if mined with the
    /// silk-touch diamond axe of `ENDERMAN_LOOT_DROP`.
    fn drop_inventory(&self) {
        let Some(state_id) = self.carried_block.load() else {
            return;
        };
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        if !world.level_info.load().game_rules.mob_drops {
            return;
        }
        let block = Block::from_state_id(state_id);
        let Some(table) = block.loot_table.and_then(|key| world.get_loot_table(key)) else {
            return;
        };
        let mut tool = ItemStack::new(1, &Item::DIAMOND_AXE);
        tool.add_enchantment(&Enchantment::SILK_TOUCH, 1);
        let params = LootContextParameters {
            world: Some(&world),
            block_state: Some(BlockState::from_id(state_id)),
            this_entity: Some(self),
            position: Some(entity.pos.load()),
            tool: Some(tool),
            ..Default::default()
        };
        let items = crate::world::loot::generate_loot_from_handle(&table, rand::random(), &params);
        for stack in items {
            entity.spawn_at_location(stack);
        }
    }
}
