// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    /// Adds an offset to each coordinate, returning `None` if any coordinate overflows.
    pub(super) fn checked_offset(self, offset: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(offset.x)?,
            y: self.y.checked_add(offset.y)?,
            z: self.z.checked_add(offset.z)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Selection {
    pub pos1: Option<BlockPos>,
    pub pos2: Option<BlockPos>,
}

impl Selection {
    /// Returns the normalized inclusive cuboid, or `None` when either endpoint is unset.
    pub fn cuboid(self) -> Option<Cuboid> {
        Some(Cuboid::new(self.pos1?, self.pos2?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cuboid {
    min: BlockPos,
    max: BlockPos,
}

impl Cuboid {
    /// Normalizes two corners into an inclusive cuboid with ordered bounds on every axis.
    pub fn new(a: BlockPos, b: BlockPos) -> Self {
        Self {
            min: BlockPos {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
                z: a.z.min(b.z),
            },
            max: BlockPos {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
                z: a.z.max(b.z),
            },
        }
    }

    /// Counts all blocks in the inclusive cuboid, saturating at `u64::MAX` on overflow.
    pub fn volume(self) -> u64 {
        let [x, y, z] = self.dimensions();
        x.saturating_mul(y).saturating_mul(z)
    }

    /// Counts unique blocks on the vertical faces, saturating at `u64::MAX` on overflow.
    pub fn wall_volume(self) -> u64 {
        let [x, y, z] = self.dimensions();
        let perimeter = if x == 1 || z == 1 {
            x * z
        } else {
            2 * x + 2 * z - 4
        };
        perimeter.saturating_mul(y)
    }

    /// Iterates every included position with X advancing fastest, then Z, then Y.
    pub const fn iter(self) -> CuboidIter {
        CuboidIter {
            cuboid: self,
            next: Some(self.min),
        }
    }

    /// Collects unique positions on the vertical faces, including degenerate one-block widths.
    pub fn wall_positions(self) -> Vec<BlockPos> {
        let mut positions = Vec::new();
        for y in self.min.y..=self.max.y {
            for z in self.min.z..=self.max.z {
                if z == self.min.z || z == self.max.z {
                    positions.extend((self.min.x..=self.max.x).map(|x| BlockPos { x, y, z }));
                } else {
                    positions.push(BlockPos {
                        x: self.min.x,
                        y,
                        z,
                    });
                    if self.min.x != self.max.x {
                        positions.push(BlockPos {
                            x: self.max.x,
                            y,
                            z,
                        });
                    }
                }
            }
        }
        positions
    }

    /// Returns the inclusive X, Y, and Z lengths using widened coordinate arithmetic.
    fn dimensions(self) -> [u64; 3] {
        [
            (i64::from(self.max.x) - i64::from(self.min.x) + 1) as u64,
            (i64::from(self.max.y) - i64::from(self.min.y) + 1) as u64,
            (i64::from(self.max.z) - i64::from(self.min.z) + 1) as u64,
        ]
    }

    /// Offsets both corners, returning `None` if any translated coordinate overflows.
    pub fn translated(self, offset: BlockPos) -> Option<Self> {
        Some(Self {
            min: self.min.checked_offset(offset)?,
            max: self.max.checked_offset(offset)?,
        })
    }
}

pub struct CuboidIter {
    cuboid: Cuboid,
    next: Option<BlockPos>,
}

impl Iterator for CuboidIter {
    type Item = BlockPos;

    /// Yields the next included position in X, Z, Y order, or `None` after the final corner.
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = advance_position(self.cuboid, current);
        Some(current)
    }
}

/// Advances within the inclusive cuboid in X, Z, Y order without stepping past its bounds.
const fn advance_position(cuboid: Cuboid, current: BlockPos) -> Option<BlockPos> {
    if current.x < cuboid.max.x {
        return Some(BlockPos {
            x: current.x + 1,
            ..current
        });
    }
    if current.z < cuboid.max.z {
        return Some(BlockPos {
            x: cuboid.min.x,
            z: current.z + 1,
            ..current
        });
    }
    if current.y < cuboid.max.y {
        return Some(BlockPos {
            x: cuboid.min.x,
            y: current.y + 1,
            z: cuboid.min.z,
        });
    }
    None
}
