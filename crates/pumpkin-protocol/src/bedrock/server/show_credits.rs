use pumpkin_macros::packet;

use crate::{
    codec::{var_int::VarInt, var_ulong::VarULong},
    serial::PacketRead,
};

/// Sent by the client when the player closes the end credits.
#[derive(Debug, PacketRead)]
#[packet(75)]
pub struct SShowCredits {
    /// Runtime entity identifier of the player viewing the credits.
    pub player_runtime_id: VarULong,
    /// 0 while the credits start, 1 once the player has closed them.
    pub credits_state: VarInt,
}

impl SShowCredits {
    pub const STOP: i32 = 1;
}
