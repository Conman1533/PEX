//! Player file keys beyond what PEX models, the last death location and thrown ender pearls.
//!
//! As D7-1 decided, a key PEX doesn't model is kept as it was loaded and written back, so a
//! vanilla player file passing through PEX keeps its recipe book, shoulder parrots and the rest.

use std::sync::Arc;

use pumpkin_data::dimension::Dimension;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use tracing::warn;

use crate::entity::projectile::ender_pearl::EnderPearlEntity;
use crate::entity::{Entity, EntityBase};
use crate::server::Server;
use crate::world::World;
use pumpkin_util::resource_location::ResourceLocation;

/// Vanilla `ServerPlayer.ENDER_PEARLS_TAG`.
pub const ENDER_PEARLS: &str = "ender_pearls";
/// Vanilla `ServerPlayer.ENDER_PEARL_DIMENSION_TAG`, added to each saved pearl.
pub const ENDER_PEARL_DIMENSION: &str = "ender_pearl_dimension";
/// Vanilla `Player.addAdditionalSaveData`'s death location.
pub const LAST_DEATH_LOCATION: &str = "LastDeathLocation";

/// Every key the player's `write_nbt` can write (`Entity::write_nbt`,
/// `LivingEntity::write_living_nbt` and the player's own), and the ones older PEX saves wrote
/// (`SpawnX` and the rest of the legacy spawn, `AirSupply`, `DrowningTick`, `HasPlayedBefore`).
/// A loaded key outside this list is kept and written back; one inside it is PEX's to write or
/// to leave out, so a key PEX stops writing (a dismounted `RootVehicle`) doesn't come back from
/// the old file.
const MODELED_PLAYER_KEYS: &[&str] = &[
    "id",
    "UUID",
    "Pos",
    "Motion",
    "Rotation",
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
    "Health",
    "AbsorptionAmount",
    "FallDistance",
    "HurtTime",
    "DeathTime",
    "FallFlying",
    "active_effects",
    "equipment",
    "DataVersion",
    "SelectedItemSlot",
    "Inventory",
    "EnderItems",
    "abilities",
    "XpP",
    "XpLevel",
    "XpTotal",
    "XpSeed",
    "Score",
    "SleepTimer",
    "playerGameType",
    "previousPlayerGameType",
    "seenCredits",
    "warden_spawn_tracker",
    "spawn_extra_particles_on_fall",
    "HasPlayedBefore",
    "foodLevel",
    "foodSaturationLevel",
    "foodExhaustionLevel",
    "foodTickTimer",
    "Air",
    "AirSupply",
    "DrowningTick",
    "Dimension",
    "SpawnX",
    "SpawnY",
    "SpawnZ",
    "SpawnDimension",
    "SpawnForced",
    "respawn",
    "RootVehicle",
    "Statistics",
    LAST_DEATH_LOCATION,
    ENDER_PEARLS,
];

/// The keys of a loaded player file that PEX doesn't model.
#[must_use]
pub fn unmodeled_player_keys(nbt: &NbtCompound) -> NbtCompound {
    unmodeled_keys(nbt, MODELED_PLAYER_KEYS)
}

/// The keys of `nbt` outside `modeled`.
#[must_use]
pub fn unmodeled_keys(nbt: &NbtCompound, modeled: &[&str]) -> NbtCompound {
    let mut kept = NbtCompound::new();
    for (key, tag) in &nbt.child_tags {
        if !modeled.contains(&key.as_ref()) {
            kept.put(key, tag.clone());
        }
    }
    kept
}

/// Writes back each kept key `nbt` doesn't already have; what PEX wrote wins.
pub fn write_kept_keys(nbt: &mut NbtCompound, kept: &NbtCompound) {
    for (key, tag) in &kept.child_tags {
        if !nbt.has(key) {
            nbt.put(key, tag.clone());
        }
    }
}

/// Vanilla `GlobalPos`, the way `Player.lastDeathLocation` stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathLocation {
    pub dimension: String,
    pub pos: BlockPos,
}

impl DeathLocation {
    /// Vanilla `GlobalPos.CODEC`: `{dimension, pos}` with `pos` an int array.
    #[must_use]
    pub fn from_nbt(nbt: &NbtCompound) -> Option<Self> {
        let dimension = nbt.get_string("dimension")?.to_string();
        let [x, y, z] = nbt.get_int_array("pos")? else {
            return None;
        };
        Some(Self {
            dimension,
            pos: BlockPos(Vector3::new(*x, *y, *z)),
        })
    }

    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("dimension", self.dimension.clone());
        nbt.put(
            "pos",
            NbtTag::IntArray(vec![self.pos.0.x, self.pos.0.y, self.pos.0.z]),
        );
        nbt
    }
}

/// The live ender pearls the entity with `owner_id` threw, in `worlds`, with their world.
#[must_use]
pub fn owned_ender_pearls(
    worlds: &[Arc<World>],
    owner_id: i32,
) -> Vec<(Arc<World>, Arc<dyn EntityBase>)> {
    let mut pearls = Vec::new();
    for world in worlds {
        for entity in world.entities.load().iter() {
            if let Some(pearl) = entity.cast_any().downcast_ref::<EnderPearlEntity>()
                && pearl.thrown.owner_id == Some(owner_id)
                && !entity
                    .get_entity()
                    .removed
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                pearls.push((world.clone(), entity.clone()));
            }
        }
    }
    pearls
}

/// Vanilla `ServerPlayer.saveEnderPearls`: each pearl's own data plus its dimension.
#[must_use]
pub fn ender_pearl_tag(world: &World, pearl: &dyn EntityBase) -> NbtTag {
    let mut nbt = NbtCompound::new();
    pearl.write_nbt(&mut nbt);
    nbt.put_string(
        ENDER_PEARL_DIMENSION,
        world.dimension.minecraft_name.to_string(),
    );
    NbtTag::Compound(nbt)
}

/// Vanilla `ServerPlayer.loadAndSpawnEnderPearl`: puts a saved pearl back into its dimension,
/// thrown by the player with `owner_id` and `owner_uuid`. Returns whether it was spawned.
pub fn spawn_saved_ender_pearl(
    world_for: impl Fn(&Dimension) -> Option<Arc<World>>,
    pearl_nbt: &NbtCompound,
    owner_id: i32,
    owner_uuid: uuid::Uuid,
) -> bool {
    let Some(dimension_name) = pearl_nbt.get_string(ENDER_PEARL_DIMENSION) else {
        return false;
    };
    let Some(world) = Dimension::from_name(dimension_name).and_then(&world_for) else {
        warn!("Trying to load ender pearl without level ({dimension_name}) being loaded, skipping");
        return false;
    };
    let uuid = pearl_nbt
        .get_uuid("UUID")
        .unwrap_or_else(uuid::Uuid::new_v4);
    if world
        .entities
        .load()
        .iter()
        .any(|entity| entity.get_entity().entity_uuid == uuid)
    {
        return false;
    }
    let entity = Entity::from_uuid(
        uuid,
        world.clone(),
        Vector3::new(0.0, 0.0, 0.0),
        &pumpkin_data::entity::EntityType::ENDER_PEARL,
    );
    let pearl = Arc::new(EnderPearlEntity::new_owned(entity, owner_id, owner_uuid));
    pearl.read_nbt_non_mut(pearl_nbt);
    // The player loading the pearl threw it, whatever `Owner` says.
    pearl.set_owner_uuid(owner_uuid);
    pearl.init_data_tracker();
    let chunk = pearl.get_entity().chunk_pos.load();
    if !world.spawn_entity(pearl) {
        return false;
    }
    world.place_ender_pearl_ticket(chunk);
    true
}

impl super::Player {
    /// Vanilla `Player.setLastDeathLocation`.
    pub fn set_last_death_location(&self, location: Option<DeathLocation>) {
        *self
            .last_death_location
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = location;
    }

    #[must_use]
    pub fn last_death_location(&self) -> Option<DeathLocation> {
        self.last_death_location
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The death location in the spawn data the client gets (vanilla
    /// `ServerPlayer.createCommonSpawnInfo`).
    #[must_use]
    pub fn death_location_for_client(&self) -> Option<(ResourceLocation, BlockPos)> {
        self.last_death_location()
            .map(|location| (location.dimension, location.pos))
    }

    /// The worlds the player's pearls can be in: every world of the server, or the player's own
    /// when it has none (tests).
    fn pearl_worlds(&self) -> Vec<Arc<World>> {
        let world = self.world();
        world.server.upgrade().map_or_else(
            || vec![world.clone()],
            |server| server.worlds.load().iter().cloned().collect(),
        )
    }

    /// Pearls from the player file not yet back in a world win; otherwise the live ones.
    fn ender_pearl_tags(&self) -> Vec<NbtTag> {
        let saved = self
            .saved_ender_pearls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !saved.is_empty() {
            return saved.iter().cloned().map(NbtTag::Compound).collect();
        }
        drop(saved);
        owned_ender_pearls(&self.pearl_worlds(), self.entity_id())
            .iter()
            .map(|(world, pearl)| ender_pearl_tag(world, pearl.as_ref()))
            .collect()
    }

    pub(super) fn write_saved_keys(&self, nbt: &mut NbtCompound) {
        if let Some(location) = self.last_death_location() {
            nbt.put_compound(LAST_DEATH_LOCATION, location.to_nbt());
        }
        let pearls = self.ender_pearl_tags();
        if !pearls.is_empty() {
            nbt.put_list(ENDER_PEARLS, pearls);
        }
        // Vanilla saves a player without its entity id (`Entity.saveWithoutId`).
        nbt.child_tags.remove("id");
        let kept = self
            .kept_nbt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        write_kept_keys(nbt, &kept);
    }

    pub(super) fn read_saved_keys(&self, nbt: &NbtCompound) {
        self.set_last_death_location(
            nbt.get_compound(LAST_DEATH_LOCATION)
                .and_then(DeathLocation::from_nbt),
        );
        *self
            .saved_ender_pearls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = nbt
            .get_list(ENDER_PEARLS)
            .map(|pearls| {
                pearls
                    .iter()
                    .filter_map(|tag| tag.extract_compound().cloned())
                    .collect()
            })
            .unwrap_or_default();
        *self
            .kept_nbt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = unmodeled_player_keys(nbt);
    }

    /// Vanilla `ServerPlayer.loadAndSpawnEnderPearls`, once the player has joined.
    pub fn spawn_saved_ender_pearls(&self, server: &Server) {
        let saved = std::mem::take(
            &mut *self
                .saved_ender_pearls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for pearl in &saved {
            if !spawn_saved_ender_pearl(
                |dimension| Some(server.get_world_from_dimension(dimension)),
                pearl,
                self.entity_id(),
                self.gameprofile.id,
            ) {
                warn!(
                    "Failed to spawn ender pearl of {} from its player file",
                    self.gameprofile.name
                );
            }
        }
    }

    /// Vanilla `PlayerList.remove`: the player's pearls are saved with it and leave the world.
    pub fn unload_ender_pearls(&self) {
        for (world, pearl) in owned_ender_pearls(&self.pearl_worlds(), self.entity_id()) {
            world.remove_entity(pearl.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vanilla_player() -> NbtCompound {
        pumpkin_nbt::nbt_compress::read_gzip_compound_tag(std::io::Cursor::new(include_bytes!(
            "testdata/player_vanilla_26_3.dat"
        )))
        .unwrap()
    }

    /// The testdata file is the fixture bot's player file as vanilla 26.3 saved it in the world
    /// round trip fixture: a recipe book, two shoulder parrots, a thrown pearl, a death location.
    #[tokio::test]
    async fn a_vanilla_player_file_keeps_the_keys_pex_does_not_model() {
        let vanilla = vanilla_player();
        let world = crate::world::test_world();
        let player = crate::world::test_player(&world, Vector3::new(0.0, 0.0, 0.0));
        EntityBase::read_nbt_non_mut(player.as_ref(), &vanilla);
        let mut saved = NbtCompound::new();
        EntityBase::write_nbt(player.as_ref(), &mut saved);

        for key in [
            "recipeBook",
            "ShoulderEntityLeft",
            "ShoulderEntityRight",
            "entered_nether_pos",
            "raid_omen_position",
            "post_effects",
            "last_explosion_impact_pos",
            LAST_DEATH_LOCATION,
            ENDER_PEARLS,
            "respawn",
            "attributes",
            "Brain",
        ] {
            assert!(vanilla.has(key), "{key}");
            assert_eq!(saved.get(key), vanilla.get(key), "{key}");
        }
        assert!(!saved.has("id"));
        assert_eq!(
            player.last_death_location(),
            Some(DeathLocation {
                dimension: "minecraft:overworld".to_string(),
                pos: BlockPos(Vector3::new(9, 65, 26)),
            })
        );
    }

    #[tokio::test]
    async fn a_saved_pearl_comes_back_thrown_by_its_player_and_is_saved_again() {
        let vanilla = vanilla_player();
        let vanilla_pearl = vanilla.get_list(ENDER_PEARLS).unwrap()[0]
            .extract_compound()
            .unwrap()
            .clone();
        let world = crate::world::test_world();
        let player = crate::world::test_player(&world, Vector3::new(0.0, 0.0, 0.0));
        EntityBase::read_nbt_non_mut(player.as_ref(), &vanilla);

        let spawned = player
            .saved_ender_pearls
            .lock()
            .unwrap()
            .drain(..)
            .filter(|pearl| {
                spawn_saved_ender_pearl(
                    |_| Some(world.clone()),
                    pearl,
                    player.entity_id(),
                    player.gameprofile.id,
                )
            })
            .count();
        assert_eq!(spawned, 1);
        let pearls = owned_ender_pearls(std::slice::from_ref(&world), player.entity_id());
        assert_eq!(pearls.len(), 1);
        let pearl = pearls[0].1.get_entity();
        assert!(pearl.has_no_gravity());
        assert_eq!(pearl.pos.load(), Vector3::new(104.5, 203.0, 1.5));

        let mut saved = NbtCompound::new();
        EntityBase::write_nbt(player.as_ref(), &mut saved);
        let saved_pearl = saved.get_list(ENDER_PEARLS).unwrap()[0]
            .extract_compound()
            .unwrap()
            .clone();
        for (key, value) in &vanilla_pearl.child_tags {
            if key.as_ref() == "Owner" {
                continue;
            }
            assert_eq!(saved_pearl.get(key), Some(value), "{key}");
        }
        // The pearl's owner is the player that loaded it.
        assert_eq!(saved_pearl.get_uuid("Owner"), Some(player.gameprofile.id));

        player.unload_ender_pearls();
        assert!(owned_ender_pearls(std::slice::from_ref(&world), player.entity_id()).is_empty());
    }

    #[test]
    fn a_death_location_reads_and_writes_vanillas_global_pos() {
        let location = DeathLocation {
            dimension: "minecraft:the_nether".to_string(),
            pos: BlockPos(Vector3::new(9, -65, 26)),
        };
        let nbt = location.to_nbt();
        assert_eq!(
            nbt.get_int_array("pos").map(<[i32]>::to_vec),
            Some(vec![9, -65, 26])
        );
        assert_eq!(DeathLocation::from_nbt(&nbt), Some(location));
        assert_eq!(DeathLocation::from_nbt(&NbtCompound::new()), None);
    }

    #[test]
    fn kept_keys_never_replace_what_pex_wrote() {
        let mut loaded = NbtCompound::new();
        loaded.put_int("XpLevel", 30);
        loaded.put_string("unknown_mod_key", "kept".to_string());
        loaded.put_compound("RootVehicle", NbtCompound::new());
        let kept = unmodeled_player_keys(&loaded);
        assert!(kept.has("unknown_mod_key"));
        assert!(!kept.has("XpLevel"));
        assert!(!kept.has("RootVehicle"));

        let mut written = NbtCompound::new();
        written.put_string("unknown_mod_key", "pex".to_string());
        write_kept_keys(&mut written, &kept);
        assert_eq!(written.get_string("unknown_mod_key"), Some("pex"));
        assert!(!written.has("RootVehicle"));
    }
}
