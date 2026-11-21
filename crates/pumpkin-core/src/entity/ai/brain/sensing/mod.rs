use rand::{Rng, RngExt};

use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;

use super::memory::{MemoryModuleId, types};
use super::{BrainTick, VisibilityContext};

pub mod adult;
pub mod dummy;
pub mod hurt_by;
pub mod mob_sensor;
pub mod nearest_items;
pub mod nearest_living_entities;
pub mod nearest_players;
pub mod piglin_brute_specific;
pub mod piglin_specific;
pub mod tempting;

pub use adult::AdultSensor;
pub use dummy::DummySensor;
pub use hurt_by::HurtBySensor;
pub use mob_sensor::MobSensor;
pub use nearest_living_entities::NearestLivingEntitySensor;
pub use nearest_players::PlayerSensor;
pub use tempting::TemptingSensor;

pub const DEFAULT_SCAN_RATE: i32 = 20;
/// Vanilla `SensorType.ARMADILLO_SCARE_DETECTED`'s scan rate.
const ARMADILLO_SCARE_SCAN_RATE: i32 = 5;

pub trait Sensor: Send + Sync {
    fn requires(&self) -> &'static [MemoryModuleId];
    fn do_tick(&mut self, tick: &mut BrainTick<'_>);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SensorType {
    Dummy,
    NearestLivingEntities,
    NearestPlayers,
    HurtBy,
    NearestItems,
    FoodTemptations,
    FrogTemptations,
    NautilusTemptations,
    NearestAdult,
    NearestAdultAnyType,
    ArmadilloScareDetected,
    PiglinBruteSpecific,
    PiglinSpecific,
}

impl SensorType {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dummy => "minecraft:dummy",
            Self::NearestLivingEntities => "minecraft:nearest_living_entities",
            Self::NearestPlayers => "minecraft:nearest_players",
            Self::HurtBy => "minecraft:hurt_by",
            Self::NearestItems => "minecraft:nearest_items",
            Self::FoodTemptations => "minecraft:food_temptations",
            Self::FrogTemptations => "minecraft:frog_temptations",
            Self::NautilusTemptations => "minecraft:nautilus_temptations",
            Self::NearestAdult => "minecraft:nearest_adult",
            Self::NearestAdultAnyType => "minecraft:nearest_adult_any_type",
            Self::ArmadilloScareDetected => "minecraft:armadillo_scare_detected",
            Self::PiglinBruteSpecific => "minecraft:piglin_brute_specific_sensor",
            Self::PiglinSpecific => "minecraft:piglin_specific_sensor",
        }
    }

    #[must_use]
    pub const fn scan_rate(self) -> i32 {
        match self {
            Self::Dummy
            | Self::NearestLivingEntities
            | Self::NearestPlayers
            | Self::HurtBy
            | Self::NearestItems
            | Self::FoodTemptations
            | Self::FrogTemptations
            | Self::NautilusTemptations
            | Self::NearestAdult
            | Self::NearestAdultAnyType
            | Self::PiglinBruteSpecific
            | Self::PiglinSpecific => DEFAULT_SCAN_RATE,
            Self::ArmadilloScareDetected => ARMADILLO_SCARE_SCAN_RATE,
        }
    }

    fn create_sensor(self) -> Box<dyn Sensor> {
        match self {
            Self::Dummy => Box::new(DummySensor),
            Self::NearestLivingEntities => Box::new(NearestLivingEntitySensor),
            Self::NearestPlayers => Box::new(PlayerSensor),
            Self::HurtBy => Box::new(HurtBySensor),
            Self::NearestItems => Box::new(nearest_items::NearestItemSensor),
            Self::FoodTemptations => Box::new(TemptingSensor::for_animal()),
            Self::FrogTemptations => Box::new(TemptingSensor::for_frog()),
            Self::NautilusTemptations => Box::new(TemptingSensor::for_nautilus()),
            Self::NearestAdult => Box::new(AdultSensor::same_type()),
            Self::NearestAdultAnyType => Box::new(AdultSensor::any_type()),
            Self::PiglinBruteSpecific => Box::new(piglin_brute_specific::PiglinBruteSpecificSensor),
            Self::PiglinSpecific => Box::new(piglin_specific::PiglinSpecificSensor),
            Self::ArmadilloScareDetected => Box::new(MobSensor::new(
                crate::entity::passive::armadillo::is_scared_by_mob,
                crate::entity::passive::armadillo::can_stay_rolled_up_in_brain,
                types::DANGER_DETECTED_RECENTLY,
                i64::from(crate::entity::passive::armadillo::SCARE_CHECK_INTERVAL),
            )),
        }
    }

    pub fn create<R: Rng>(self, rng: &mut R) -> SensorEntry {
        SensorEntry::new(self, rng)
    }
}

pub struct SensorEntry {
    ty: SensorType,
    sensor: Box<dyn Sensor>,
    scan_rate: i32,
    time_to_tick: i64,
}

impl SensorEntry {
    pub fn new<R: Rng>(ty: SensorType, rng: &mut R) -> Self {
        let scan_rate = ty.scan_rate();
        Self {
            ty,
            sensor: ty.create_sensor(),
            scan_rate,
            time_to_tick: i64::from(rng.random_range(0..scan_rate)),
        }
    }

    #[must_use]
    pub const fn sensor_type(&self) -> SensorType {
        self.ty
    }

    #[must_use]
    pub fn requires(&self) -> &'static [MemoryModuleId] {
        self.sensor.requires()
    }

    pub fn tick(&mut self, tick: &mut BrainTick<'_>) {
        self.time_to_tick -= 1;
        if self.time_to_tick <= 0 {
            self.time_to_tick = i64::from(self.scan_rate);
            self.sensor.do_tick(tick);
        }
    }
}

fn is_living_alive(entity: &dyn EntityBase) -> bool {
    entity
        .get_living_entity()
        .is_some_and(|living| living.entity.is_alive() && !living.is_dead_or_dying())
}

fn is_current_attack_target(ctx: &VisibilityContext<'_>, target: &dyn EntityBase) -> bool {
    ctx.state
        .get(types::ATTACK_TARGET)
        .is_some_and(|current| current.get_entity().entity_id == target.get_entity().entity_id)
}

// Vanilla's `ignoreInvisibilityTesting` drops the visibility-percent range scaling, which is
// `use_distance_scaling_factor` here, not the line of sight check.
fn targeting_predicate(
    ctx: &VisibilityContext<'_>,
    target: &dyn EntityBase,
    attackable: bool,
) -> TargetPredicate {
    let predicate = if attackable {
        TargetPredicate::create_attackable()
    } else {
        TargetPredicate::create_non_attackable()
    }
    .set_base_max_distance(ctx.follow_range);
    if is_current_attack_target(ctx, target) {
        predicate.ignore_distance_scaling_factor()
    } else {
        predicate
    }
}

#[must_use]
pub fn is_entity_targetable(ctx: &VisibilityContext<'_>, target: &dyn EntityBase) -> bool {
    targeting_predicate(ctx, target, false).test(ctx.world, Some(ctx.mob), target)
}

#[must_use]
pub fn is_entity_attackable(ctx: &VisibilityContext<'_>, target: &dyn EntityBase) -> bool {
    targeting_predicate(ctx, target, true).test_with_alliance(
        ctx.world,
        Some(ctx.mob),
        target,
        |tester, target| is_allied(ctx, tester, target),
    )
}

fn is_allied(
    ctx: &VisibilityContext<'_>,
    tester: &dyn EntityBase,
    target: &dyn EntityBase,
) -> bool {
    tester.get_entity().entity_id == target.get_entity().entity_id
        || ctx
            .mob
            .mob_considers_entity_as_ally(target, Some(ctx.state))
            .unwrap_or_else(|| tester.considers_entity_as_ally(target))
        || target.considers_entity_as_ally(tester)
}

#[must_use]
pub fn is_entity_attackable_ignoring_line_of_sight(
    ctx: &VisibilityContext<'_>,
    target: &dyn EntityBase,
) -> bool {
    targeting_predicate(ctx, target, true)
        .ignore_visibility()
        .test_with_alliance(ctx.world, Some(ctx.mob), target, |tester, target| {
            is_allied(ctx, tester, target)
        })
}

pub fn remember_positives<T>(
    invocations: i32,
    predicate: impl Fn(&T) -> bool,
) -> impl FnMut(&T) -> bool {
    let mut positives_left = 0;
    move |value| {
        if predicate(value) {
            positives_left = invocations;
            true
        } else {
            positives_left -= 1;
            positives_left >= 0
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::damage::DamageType;
    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::vector3::Vector3;

    use super::*;
    use crate::entity::Entity;
    use crate::entity::ai::brain::BrainState;
    use crate::entity::ai::brain::behavior::{BehaviorControl, Status, Timed};
    use crate::entity::ai::test_support::world;
    use crate::entity::passive::cow::CowEntity;
    use crate::world::World;
    use std::sync::Arc;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn cow(world: &Arc<World>, x: f64) -> Arc<CowEntity> {
        CowEntity::new(Entity::new(
            world.clone(),
            Vector3::new(x, 64.0, 0.0),
            &EntityType::COW,
        ))
    }

    fn sense(sensor: &mut dyn Sensor, world: &Arc<World>, body: &CowEntity) -> BrainState {
        let mut state = BrainState::default();
        for id in sensor.requires() {
            state.register_memory(*id);
        }
        sensor.do_tick(&mut BrainTick {
            state: &mut state,
            world,
            mob: body,
            time: 0,
        });
        state
    }

    fn ids(entities: &[Arc<dyn EntityBase>]) -> Vec<i32> {
        entities
            .iter()
            .map(|entity| entity.get_entity().entity_id)
            .collect()
    }

    #[tokio::test]
    async fn nearest_living_entities_are_alive_in_range_and_nearest_first() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let far = cow(&world, 6.0);
        let near = cow(&world, 2.0);
        let dead = cow(&world, 1.0);
        let out_of_range = cow(&world, 40.0);
        dead.mob_entity.living_entity.health.store(0.0);
        world.entities.store(Arc::new(vec![
            body.clone(),
            far.clone(),
            near.clone(),
            dead,
            out_of_range,
        ]));

        let state = sense(&mut NearestLivingEntitySensor, &world, &body);
        world.level.shutdown().await;

        let nearby = state
            .get(types::NEAREST_LIVING_ENTITIES)
            .map(|entities| ids(entities));
        assert_eq!(
            nearby,
            Some(vec![
                near.get_entity().entity_id,
                far.get_entity().entity_id
            ])
        );
        assert!(state.has_memory_value(types::NEAREST_VISIBLE_LIVING_ENTITIES.id()));
        Ok(())
    }

    #[tokio::test]
    async fn hurt_by_remembers_the_attacker_until_it_is_gone() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let attacker = cow(&world, 1.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), attacker.clone()]));
        body.mob_entity.living_entity.damage_with_context(
            &*body,
            1.0,
            DamageType::MOB_ATTACK,
            None,
            Some(&*attacker),
            Some(&*attacker),
        );

        let mut sensor = HurtBySensor;
        let hurt = sense(&mut sensor, &world, &body);
        let remembered = hurt
            .get(types::HURT_BY_ENTITY)
            .map(|entity| entity.get_entity().entity_id);
        let mut state = hurt;
        attacker.get_entity().remove();
        sensor.do_tick(&mut BrainTick {
            state: &mut state,
            world: &world,
            mob: &*body,
            time: 1,
        });
        world.level.shutdown().await;

        assert!(state.has_memory_value(types::HURT_BY.id()));
        assert_eq!(remembered, Some(attacker.get_entity().entity_id));
        assert!(!state.has_memory_value(types::HURT_BY_ENTITY.id()));
        Ok(())
    }

    fn brain_state(ids: &[MemoryModuleId]) -> BrainState {
        let mut state = BrainState::default();
        for id in ids {
            state.register_memory(*id);
        }
        state
    }

    fn run_sensor(
        sensor: &mut dyn Sensor,
        state: &mut BrainState,
        world: &Arc<World>,
        body: &CowEntity,
    ) {
        for id in sensor.requires() {
            state.register_memory(*id);
        }
        sensor.do_tick(&mut BrainTick {
            state,
            world,
            mob: body,
            time: 0,
        });
    }

    fn try_start(
        behavior: &mut dyn BehaviorControl,
        state: &mut BrainState,
        world: &Arc<World>,
        body: &CowEntity,
        time: i64,
    ) -> bool {
        for id in behavior.required_memories() {
            state.register_memory(*id);
        }
        behavior.try_start(&mut BrainTick {
            state,
            world,
            mob: body,
            time,
        })
    }

    fn tick_or_stop(
        behavior: &mut dyn BehaviorControl,
        state: &mut BrainState,
        world: &Arc<World>,
        body: &CowEntity,
        time: i64,
    ) {
        behavior.tick_or_stop(&mut BrainTick {
            state,
            world,
            mob: body,
            time,
        });
    }

    fn tracked_id(state: &BrainState) -> Option<i32> {
        state
            .get(types::LOOK_TARGET)
            .and_then(|target| target.tracked_entity())
            .map(|entity| entity.get_entity().entity_id)
    }

    fn baby(cow: &CowEntity) {
        cow.get_entity()
            .age
            .store(-24000, std::sync::atomic::Ordering::Relaxed);
    }

    #[tokio::test]
    async fn tempting_sensor_remembers_the_nearest_player_holding_food() -> TestResult {
        use pumpkin_data::item::Item;
        use pumpkin_data::item_stack::ItemStack;
        use pumpkin_util::Hand;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let near = crate::world::test_player(&world, Vector3::new(3.0, 64.0, 0.0));
        let far = crate::world::test_player(&world, Vector3::new(6.0, 64.0, 0.0));
        let out_of_range = crate::world::test_player(&world, Vector3::new(30.0, 64.0, 0.0));
        near.inventory()
            .set_stack_in_hand(Hand::Right, ItemStack::new(1, &Item::DIRT));
        far.inventory()
            .set_stack_in_hand(Hand::Left, ItemStack::new(1, &Item::WHEAT));
        out_of_range
            .inventory()
            .set_stack_in_hand(Hand::Right, ItemStack::new(1, &Item::WHEAT));

        let mut sensor = TemptingSensor::for_animal();
        let mut state = BrainState::default();
        run_sensor(&mut sensor, &mut state, &world, &body);
        let tempting = state
            .get(types::TEMPTING_PLAYER)
            .map(|player| player.entity_id());
        far.inventory()
            .set_stack_in_hand(Hand::Left, ItemStack::new(1, &Item::DIRT));
        run_sensor(&mut sensor, &mut state, &world, &body);
        world.level.shutdown().await;

        assert_eq!(tempting, Some(far.entity_id()));
        assert!(!state.has_memory_value(types::TEMPTING_PLAYER.id()));
        Ok(())
    }

    #[tokio::test]
    async fn adult_sensor_finds_the_nearest_adult_of_its_own_type() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let calf = cow(&world, 2.0);
        let adult = cow(&world, 5.0);
        baby(&calf);
        world
            .entities
            .store(Arc::new(vec![body.clone(), calf, adult.clone()]));

        let mut state = BrainState::default();
        run_sensor(&mut NearestLivingEntitySensor, &mut state, &world, &body);
        run_sensor(&mut AdultSensor::same_type(), &mut state, &world, &body);
        world.level.shutdown().await;

        let found = state
            .get(types::NEAREST_VISIBLE_ADULT)
            .map(|adult| adult.get_entity().entity_id);
        assert_eq!(found, Some(adult.get_entity().entity_id));
        Ok(())
    }

    fn any_cow(_mob: &dyn crate::entity::mob::Mob, entity: &dyn EntityBase) -> bool {
        entity.get_entity().entity_type == &EntityType::COW
    }

    fn ready(_tick: &BrainTick<'_>) -> bool {
        true
    }

    fn not_ready(_tick: &BrainTick<'_>) -> bool {
        false
    }

    #[tokio::test]
    async fn mob_sensor_sets_the_memory_while_ready_and_erases_it_otherwise() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let other = cow(&world, 3.0);
        world.entities.store(Arc::new(vec![body.clone(), other]));

        let mut state = brain_state(&[types::DANGER_DETECTED_RECENTLY.id()]);
        run_sensor(&mut NearestLivingEntitySensor, &mut state, &world, &body);
        run_sensor(
            &mut MobSensor::new(any_cow, ready, types::DANGER_DETECTED_RECENTLY, 80),
            &mut state,
            &world,
            &body,
        );
        let remembered = state.time_until_expiry(types::DANGER_DETECTED_RECENTLY.id());
        run_sensor(
            &mut MobSensor::new(any_cow, not_ready, types::DANGER_DETECTED_RECENTLY, 80),
            &mut state,
            &world,
            &body,
        );
        world.level.shutdown().await;

        assert_eq!(remembered, Some(80));
        assert!(!state.has_memory_value(types::DANGER_DETECTED_RECENTLY.id()));
        Ok(())
    }

    #[tokio::test]
    async fn follow_temptation_walks_to_the_player_until_close_enough() -> TestResult {
        use crate::entity::ai::brain::behavior::follow_temptation::{
            FollowTemptation, TEMPTATION_COOLDOWN,
        };

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let player = crate::world::test_player(&world, Vector3::new(6.0, 64.0, 0.0));

        let mut behavior = Timed::new(FollowTemptation::new(|_| 1.25));
        let mut state = BrainState::default();
        assert!(!try_start(&mut behavior, &mut state, &world, &body, 0));
        state.set(types::TEMPTING_PLAYER, player.clone());
        assert!(try_start(&mut behavior, &mut state, &world, &body, 0));
        tick_or_stop(&mut behavior, &mut state, &world, &body, 0);
        let walking = state
            .get(types::WALK_TARGET)
            .map(|walk| (walk.speed_modifier(), walk.close_enough_dist()));
        let looking = tracked_id(&state);

        player.get_entity().set_pos(Vector3::new(2.0, 64.0, 0.0));
        tick_or_stop(&mut behavior, &mut state, &world, &body, 1);
        let walking_when_close = state.has_memory_value(types::WALK_TARGET.id());

        state.erase(types::TEMPTING_PLAYER.id());
        tick_or_stop(&mut behavior, &mut state, &world, &body, 2);
        world.level.shutdown().await;

        assert_eq!(walking, Some((1.25, 2)));
        assert_eq!(looking, Some(player.entity_id()));
        assert!(!walking_when_close);
        assert_eq!(behavior.status(), Status::Stopped);
        assert_eq!(
            state.get(types::TEMPTATION_COOLDOWN_TICKS),
            Some(&TEMPTATION_COOLDOWN)
        );
        assert!(!state.has_memory_value(types::LOOK_TARGET.id()));
        Ok(())
    }

    #[tokio::test]
    async fn animal_make_love_pairs_with_a_visible_partner_in_love() -> TestResult {
        use crate::entity::ai::brain::behavior::animal_make_love::AnimalMakeLove;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let partner = cow(&world, 2.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), partner.clone()]));
        let mut behavior = Timed::new(AnimalMakeLove::for_type(&EntityType::COW));
        let mut state = BrainState::default();
        run_sensor(&mut NearestLivingEntitySensor, &mut state, &world, &body);

        let started_without_love = try_start(&mut behavior, &mut state, &world, &body, 0);
        body.mob_entity
            .love_ticks
            .store(600, std::sync::atomic::Ordering::Relaxed);
        let started_alone_in_love = try_start(&mut behavior, &mut state, &world, &body, 0);
        partner
            .mob_entity
            .love_ticks
            .store(600, std::sync::atomic::Ordering::Relaxed);
        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        world.level.shutdown().await;

        assert!(!started_without_love);
        assert!(!started_alone_in_love);
        assert!(started);
        let partner_id = partner.get_entity().entity_id;
        assert_eq!(
            state
                .get(types::BREED_TARGET)
                .map(|target| target.get_entity().entity_id),
            Some(partner_id)
        );
        assert_eq!(tracked_id(&state), Some(partner_id));
        assert_eq!(
            state
                .get(types::WALK_TARGET)
                .map(|walk| (walk.speed_modifier(), walk.close_enough_dist())),
            Some((1.0, 2))
        );
        Ok(())
    }

    #[tokio::test]
    async fn baby_follow_adult_walks_a_baby_to_an_adult_out_of_range() -> TestResult {
        use crate::entity::ai::brain::behavior::baby_follow_adult;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let adult = cow(&world, 8.0);
        let mut behavior = baby_follow_adult::create(5, 16, 1.25);
        let mut state = BrainState::default();
        state.register_memory(types::NEAREST_VISIBLE_ADULT.id());
        state.set(
            types::NEAREST_VISIBLE_ADULT,
            adult.clone() as Arc<dyn EntityBase>,
        );

        let adult_body_follows = try_start(&mut behavior, &mut state, &world, &body, 0);
        baby(&body);
        let follows = try_start(&mut behavior, &mut state, &world, &body, 0);
        let walk = state
            .get(types::WALK_TARGET)
            .map(|walk| (walk.speed_modifier(), walk.close_enough_dist()));
        state.erase(types::WALK_TARGET.id());
        adult.get_entity().set_pos(Vector3::new(4.0, 64.0, 0.0));
        let follows_when_close = try_start(&mut behavior, &mut state, &world, &body, 1);
        world.level.shutdown().await;

        assert!(!adult_body_follows);
        assert!(follows);
        assert_eq!(walk, Some((1.25, 4)));
        assert!(!follows_when_close);
        Ok(())
    }

    #[tokio::test]
    async fn random_look_around_looks_ahead_and_sets_the_gaze_cooldown() -> TestResult {
        use crate::entity::ai::brain::behavior::random_look_around::RandomLookAround;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let mut behavior = Timed::new(RandomLookAround::new(150, 250, 0.0, 0.0, 0.0));
        let mut state = BrainState::default();

        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        let restarted = try_start(&mut behavior, &mut state, &world, &body, 1);
        world.level.shutdown().await;

        assert!(started);
        assert!(!restarted);
        let target = state
            .get(types::LOOK_TARGET)
            .map(|target| target.current_position())
            .ok_or("no look target")?;
        let ahead = body.get_entity().get_eye_pos() + Vector3::new(0.0, 0.0, 1.0);
        assert!(target.squared_distance_to_vec(&ahead) < 1e-6, "{target:?}");
        let cooldown = *state.get(types::GAZE_COOLDOWN_TICKS).ok_or("no cooldown")?;
        assert!((150..=250).contains(&cooldown));
        Ok(())
    }

    #[tokio::test]
    async fn set_entity_look_target_looks_at_the_closest_match() -> TestResult {
        use crate::entity::ai::brain::behavior::set_entity_look_target;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let near = cow(&world, 2.0);
        let far = cow(&world, 5.0);
        let too_far = cow(&world, 12.0);
        world.entities.store(Arc::new(vec![
            body.clone(),
            near.clone(),
            far.clone(),
            too_far.clone(),
        ]));
        let near_id = near.get_entity().entity_id;
        let too_far_id = too_far.get_entity().entity_id;
        let mut behavior = set_entity_look_target::create_with(
            move |entity| entity.get_entity().entity_id != near_id,
            8.0,
        );
        let mut only_too_far = set_entity_look_target::create_with(
            move |entity| entity.get_entity().entity_id == too_far_id,
            8.0,
        );
        let mut state = BrainState::default();
        run_sensor(&mut NearestLivingEntitySensor, &mut state, &world, &body);

        let too_far_started = try_start(&mut only_too_far, &mut state, &world, &body, 0);
        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        world.level.shutdown().await;

        assert!(!too_far_started);
        assert!(started);
        assert_eq!(tracked_id(&state), Some(far.get_entity().entity_id));
        Ok(())
    }

    fn as_target(cow: &Arc<CowEntity>) -> Arc<dyn EntityBase> {
        cow.clone()
    }

    #[tokio::test]
    async fn stop_attacking_if_target_invalid_forgets_a_dead_target() -> TestResult {
        use crate::entity::ai::brain::behavior::stop_attacking_if_target_invalid;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let target = cow(&world, 2.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), target.clone()]));
        let mut behavior = stop_attacking_if_target_invalid::create();
        let mut state = brain_state(behavior.required_memories());
        state.set(types::ATTACK_TARGET, as_target(&target));

        let started_alive = try_start(&mut behavior, &mut state, &world, &body, 0);
        let kept_alive = state.has_memory_value(types::ATTACK_TARGET.id());
        tick_or_stop(&mut behavior, &mut state, &world, &body, 0);
        target.mob_entity.living_entity.health.store(0.0);
        let started_dead = try_start(&mut behavior, &mut state, &world, &body, 1);
        world.level.shutdown().await;

        assert!(started_alive);
        assert!(kept_alive);
        assert!(started_dead);
        assert!(!state.has_memory_value(types::ATTACK_TARGET.id()));
        Ok(())
    }

    #[tokio::test]
    async fn stop_attacking_if_target_invalid_gives_up_after_ten_unreachable_seconds() -> TestResult
    {
        use crate::entity::ai::brain::behavior::stop_attacking_if_target_invalid;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let target = cow(&world, 2.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), target.clone()]));
        let mut behavior = stop_attacking_if_target_invalid::create();
        let mut state = brain_state(behavior.required_memories());
        state.set(types::ATTACK_TARGET, as_target(&target));
        state.set(types::CANT_REACH_WALK_TARGET_SINCE, 100);

        try_start(&mut behavior, &mut state, &world, &body, 300);
        let kept_at_200 = state.has_memory_value(types::ATTACK_TARGET.id());
        tick_or_stop(&mut behavior, &mut state, &world, &body, 300);
        try_start(&mut behavior, &mut state, &world, &body, 301);
        world.level.shutdown().await;

        assert!(kept_at_200);
        assert!(!state.has_memory_value(types::ATTACK_TARGET.id()));
        Ok(())
    }

    #[tokio::test]
    async fn start_attacking_remembers_the_found_target_and_forgets_cant_reach() -> TestResult {
        use crate::entity::ai::brain::behavior::start_attacking;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let target = cow(&world, 3.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), target.clone()]));
        let found = as_target(&target);
        let mut behavior = start_attacking::create(move |_| Some(found.clone()));
        let mut state = brain_state(behavior.required_memories());
        state.set(types::CANT_REACH_WALK_TARGET_SINCE, 5);

        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        tick_or_stop(&mut behavior, &mut state, &world, &body, 0);
        let started_again = try_start(&mut behavior, &mut state, &world, &body, 1);
        world.level.shutdown().await;

        assert!(started);
        assert!(!started_again);
        assert_eq!(
            state
                .get(types::ATTACK_TARGET)
                .map(|entity| entity.get_entity().entity_id),
            Some(target.get_entity().entity_id)
        );
        assert!(!state.has_memory_value(types::CANT_REACH_WALK_TARGET_SINCE.id()));
        Ok(())
    }

    #[tokio::test]
    async fn walk_target_follows_an_attack_target_out_of_sight() -> TestResult {
        use crate::entity::ai::brain::behavior::set_walk_target_from_attack_target_if_target_out_of_reach as walk_to_target;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let target = cow(&world, 6.0);
        world
            .entities
            .store(Arc::new(vec![body.clone(), target.clone()]));
        let mut behavior = walk_to_target::create(1.0);
        let mut state = brain_state(behavior.required_memories());
        state.set(types::ATTACK_TARGET, as_target(&target));

        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        world.level.shutdown().await;

        assert!(started);
        assert_eq!(tracked_id(&state), Some(target.get_entity().entity_id));
        assert_eq!(
            state
                .get(types::WALK_TARGET)
                .map(|walk| (walk.speed_modifier(), walk.close_enough_dist())),
            Some((1.0, 0))
        );
        Ok(())
    }

    #[tokio::test]
    async fn become_passive_if_memory_present_forgets_the_attack_target() -> TestResult {
        use crate::entity::ai::brain::behavior::become_passive_if_memory_present;
        use pumpkin_util::math::position::BlockPos;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let body = cow(&world, 0.0);
        let target = cow(&world, 2.0);
        let mut behavior =
            become_passive_if_memory_present::create(types::NEAREST_REPELLENT.id(), 200);
        let mut state = brain_state(behavior.required_memories());
        state.set(types::ATTACK_TARGET, as_target(&target));
        state.set(types::NEAREST_REPELLENT, BlockPos::new(1, 64, 0));

        let started = try_start(&mut behavior, &mut state, &world, &body, 0);
        world.level.shutdown().await;

        assert!(started);
        assert!(!state.has_memory_value(types::ATTACK_TARGET.id()));
        assert_eq!(state.time_until_expiry(types::PACIFIED.id()), Some(200));
        Ok(())
    }
}
