use crate::entity::EntityBase;
use crate::entity::boss::ender_dragon::{DragonPath, EnderDragonEntity};
use crate::entity::player::Player;
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Arc, Mutex, PoisonError};

mod charging_player;
mod dying;
mod holding_pattern;
mod hover;
mod landing;
mod landing_approach;
mod sitting_attacking;
mod sitting_flaming;
mod sitting_scanning;
mod strafe_player;
mod takeoff;

pub use charging_player::ChargingPlayerPhase;
pub use dying::DyingPhase;
pub use holding_pattern::HoldingPatternPhase;
pub use hover::HoverPhase;
pub use landing::LandingPhase;
pub use landing_approach::LandingApproachPhase;
pub use sitting_attacking::SittingAttackingPhase;
pub use sitting_flaming::SittingFlamingPhase;
pub use sitting_scanning::SittingScanningPhase;
pub use strafe_player::StrafePlayerPhase;
pub use takeoff::TakeoffPhase;

/// Vanilla `DragonPhaseInstance`, server side.
pub trait Phase: Send + Sync {
    fn do_server_tick(&self, dragon: &EnderDragonEntity);
    fn on_crystal_destroyed(&self, _dragon: &EnderDragonEntity, _player: Option<Arc<Player>>) {}
    fn begin(&self, _dragon: &EnderDragonEntity) {}
    fn end(&self, _dragon: &EnderDragonEntity) {}
    fn get_fly_speed(&self) -> f32 {
        0.6
    }
    fn get_turn_speed(&self, dragon: &EnderDragonEntity) -> f32 {
        let rot_speed = dragon.horizontal_speed() + 1.0;
        let dist = rot_speed.min(40.0);
        0.7 / dist / rot_speed
    }
    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        None
    }
}

/// Vanilla `EnderDragonPhase`; the discriminants are the ids saved as `DragonPhase` and synced as
/// `DATA_PHASE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnderDragonPhase {
    HoldingPattern = 0,
    StrafePlayer = 1,
    LandingApproach = 2,
    Landing = 3,
    Takeoff = 4,
    SittingFlaming = 5,
    SittingScanning = 6,
    SittingAttacking = 7,
    ChargingPlayer = 8,
    Dying = 9,
    Hovering = 10,
}

impl EnderDragonPhase {
    const ALL: [Self; 11] = [
        Self::HoldingPattern,
        Self::StrafePlayer,
        Self::LandingApproach,
        Self::Landing,
        Self::Takeoff,
        Self::SittingFlaming,
        Self::SittingScanning,
        Self::SittingAttacking,
        Self::ChargingPlayer,
        Self::Dying,
        Self::Hovering,
    ];

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// Vanilla `EnderDragonPhase.getById`: unknown ids fall back to the holding pattern.
    #[must_use]
    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(Self::HoldingPattern)
    }

    /// Vanilla `DragonPhaseInstance.isSitting`: the three `AbstractDragonSittingPhase`s and hover.
    #[must_use]
    pub const fn is_sitting(self) -> bool {
        matches!(
            self,
            Self::SittingFlaming | Self::SittingScanning | Self::SittingAttacking | Self::Hovering
        )
    }
}

/// Vanilla `EnderDragonPhaseManager`: one instance of each phase, kept across phase changes.
pub struct PhaseManager {
    current: Mutex<EnderDragonPhase>,
    pub holding_pattern: HoldingPatternPhase,
    pub strafe_player: StrafePlayerPhase,
    pub landing_approach: LandingApproachPhase,
    pub landing: LandingPhase,
    pub takeoff: TakeoffPhase,
    pub sitting_flaming: SittingFlamingPhase,
    pub sitting_scanning: SittingScanningPhase,
    pub sitting_attacking: SittingAttackingPhase,
    pub charging_player: ChargingPlayerPhase,
    pub dying: DyingPhase,
    pub hover: HoverPhase,
}

impl Default for PhaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseManager {
    /// Starts in [`EnderDragonPhase::Hovering`] like vanilla, whose `begin` has nothing to reset yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: Mutex::new(EnderDragonPhase::Hovering),
            holding_pattern: HoldingPatternPhase::default(),
            strafe_player: StrafePlayerPhase::default(),
            landing_approach: LandingApproachPhase::default(),
            landing: LandingPhase::default(),
            takeoff: TakeoffPhase::default(),
            sitting_flaming: SittingFlamingPhase::default(),
            sitting_scanning: SittingScanningPhase::default(),
            sitting_attacking: SittingAttackingPhase::default(),
            charging_player: ChargingPlayerPhase::default(),
            dying: DyingPhase::default(),
            hover: HoverPhase::default(),
        }
    }

    #[must_use]
    pub fn current(&self) -> EnderDragonPhase {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn set_current(&self, phase: EnderDragonPhase) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = phase;
    }

    #[must_use]
    pub fn get(&self, phase: EnderDragonPhase) -> &dyn Phase {
        match phase {
            EnderDragonPhase::HoldingPattern => &self.holding_pattern,
            EnderDragonPhase::StrafePlayer => &self.strafe_player,
            EnderDragonPhase::LandingApproach => &self.landing_approach,
            EnderDragonPhase::Landing => &self.landing,
            EnderDragonPhase::Takeoff => &self.takeoff,
            EnderDragonPhase::SittingFlaming => &self.sitting_flaming,
            EnderDragonPhase::SittingScanning => &self.sitting_scanning,
            EnderDragonPhase::SittingAttacking => &self.sitting_attacking,
            EnderDragonPhase::ChargingPlayer => &self.charging_player,
            EnderDragonPhase::Dying => &self.dying,
            EnderDragonPhase::Hovering => &self.hover,
        }
    }
}

/// The `navigateToNextPathNode` shared by the holding pattern, strafe and landing approach: the
/// next node, up to 20 blocks higher, then advance.
fn next_path_target(path: &mut DragonPath) -> Option<Vector3<f64>> {
    let node = path.next_node_pos()?;
    path.advance();
    let y = f64::from(node.y) + f64::from(rand::random::<f32>() * 20.0);
    Some(Vector3::new(f64::from(node.x), y, f64::from(node.z)))
}

/// Keeps a node index on the outer ring (0..12) or the inner ring (12..20).
const fn wrap_node_index(index: i32, outer_ring: bool) -> usize {
    if outer_ring {
        index.rem_euclid(12) as usize
    } else {
        (((index - 12) & 7) + 12) as usize
    }
}

/// Vanilla `Vec3.distanceToSqr` between the dragon and a target location.
fn distance_to_target(dragon: &EnderDragonEntity, target: Option<Vector3<f64>>) -> f64 {
    target.map_or(0.0, |target| {
        target.squared_distance_to_vec(&dragon.get_entity().pos.load())
    })
}

/// The `distToTarget < 100 || distToTarget > 22500 || horizontalCollision || verticalCollision`
/// test the flying phases use to pick their next node.
fn needs_new_target(dragon: &EnderDragonEntity, dist_to_target: f64) -> bool {
    let entity = dragon.get_entity();
    !(100.0..=22500.0).contains(&dist_to_target)
        || entity
            .horizontal_collision
            .load(std::sync::atomic::Ordering::Relaxed)
        || entity
            .vertical_collision
            .load(std::sync::atomic::Ordering::Relaxed)
}
