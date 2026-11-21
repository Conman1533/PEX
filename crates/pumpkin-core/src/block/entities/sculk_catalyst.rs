use super::BlockEntity;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use std::sync::Mutex;

pub struct SculkCatalystBlockEntity {
    pub position: BlockPos,
    /// Vanilla `SculkSpreader.cursors`, kept as saved: PEX does not spread sculk from a catalyst
    /// yet, so the charges are carried through saves unchanged.
    pub cursors: Mutex<Vec<NbtTag>>,
}

impl BlockEntity for SculkCatalystBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let cursors = nbt
            .get_list("cursors")
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        Self {
            position,
            cursors: Mutex::new(cursors),
        }
    }

    /// Vanilla `SculkSpreader.save`: `cursors` is always written, empty included.
    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let cursors = self
            .cursors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        nbt.put("cursors", NbtTag::List(cursors));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SculkCatalystBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_catalyst";
    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            cursors: Mutex::new(Vec::new()),
        }
    }
}
