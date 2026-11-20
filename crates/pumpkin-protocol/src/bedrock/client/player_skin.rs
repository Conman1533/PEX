use crate::serial::PacketWrite;
use pumpkin_macros::packet;
use std::io::{Error, Write};
use uuid::Uuid;

use super::player_list::Skin;

/// Changes the skin of a player who is already in the receiver's player list.
#[packet(93)]
pub struct CPlayerSkin {
    /// The player whose skin changes.
    pub uuid: Uuid,
    /// The new skin.
    pub skin: Skin,
}

impl PacketWrite for CPlayerSkin {
    fn write<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.uuid.write(writer)?;
        self.skin.write(writer)?;
        // The new and old skin names are unused since 1.13 and left empty.
        String::new().write(writer)?;
        String::new().write(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_uuid_skin_and_two_empty_names() {
        let uuid = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let skin = Skin::steve();

        let mut expected = Vec::new();
        uuid.write(&mut expected).unwrap();
        skin.write(&mut expected).unwrap();
        expected.extend_from_slice(&[0, 0]);

        let mut written = Vec::new();
        CPlayerSkin { uuid, skin }.write(&mut written).unwrap();
        assert_eq!(written, expected);
    }
}
