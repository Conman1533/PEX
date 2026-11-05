use super::EnderDragonPhase;
use crate::entity::boss::ender_dragon::EnderDragonEntity;
use std::sync::{Mutex, PoisonError};

const ROAR_DURATION: i32 = 40;

/// Vanilla `DragonSittingAttackingPhase`: roar (the client plays the growl from the synced phase),
/// then breathe.
#[derive(Default)]
pub struct SittingAttackingPhase {
    attacking_ticks: Mutex<i32>,
}

impl super::Phase for SittingAttackingPhase {
    fn do_server_tick(&self, dragon: &EnderDragonEntity) {
        let mut attacking_ticks = self
            .attacking_ticks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let ticks = *attacking_ticks;
        *attacking_ticks += 1;
        drop(attacking_ticks);
        if ticks >= ROAR_DURATION {
            dragon.set_phase(EnderDragonPhase::SittingFlaming);
        }
    }

    fn begin(&self, _dragon: &EnderDragonEntity) {
        *self
            .attacking_ticks
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = 0;
    }
}
