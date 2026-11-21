//! Vanilla `RailState`: a rail working out its shape from the rails around it.

use std::sync::Arc;

use pumpkin_data::block_properties::RailShape;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockStateId, tag};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::world::World;

use super::RailProperties;

fn is_rail(world: &World, pos: &BlockPos) -> bool {
    world.get_block(pos).has_tag(&tag::Block::MINECRAFT_RAILS)
}

/// `NORTH_SOUTH` and `EAST_WEST` climb towards a rail one block up, as vanilla checks at the end
/// of `place` and `connectTo`.
fn with_slope(world: &World, pos: BlockPos, shape: Option<RailShape>) -> Option<RailShape> {
    match shape {
        Some(RailShape::NorthSouth) => Some(if is_rail(world, &pos.south().up()) {
            RailShape::AscendingSouth
        } else if is_rail(world, &pos.north().up()) {
            RailShape::AscendingNorth
        } else {
            RailShape::NorthSouth
        }),
        Some(RailShape::EastWest) => Some(if is_rail(world, &pos.west().up()) {
            RailShape::AscendingWest
        } else if is_rail(world, &pos.east().up()) {
            RailShape::AscendingEast
        } else {
            RailShape::EastWest
        }),
        other => other,
    }
}

pub(super) struct RailState {
    pos: BlockPos,
    block: &'static Block,
    properties: RailProperties,
    connections: Vec<BlockPos>,
}

impl RailState {
    /// The rail at `pos`, if there is one.
    pub(super) fn new(world: &World, pos: BlockPos) -> Option<Self> {
        let (block, state_id) = world.get_block_and_state_id(&pos);
        block
            .has_tag(&tag::Block::MINECRAFT_RAILS)
            .then(|| Self::with_state(pos, block, state_id))
    }

    fn with_state(pos: BlockPos, block: &'static Block, state_id: BlockStateId) -> Self {
        let properties = RailProperties::new(state_id, block);
        let shape = properties.shape();
        let mut rail = Self {
            pos,
            block,
            properties,
            connections: Vec::with_capacity(3),
        };
        rail.update_connections(shape);
        rail
    }

    pub(super) const fn shape(&self) -> RailShape {
        self.properties.shape()
    }

    const fn is_straight(&self) -> bool {
        !self.properties.can_curve()
    }

    fn update_connections(&mut self, shape: RailShape) {
        let p = self.pos;
        self.connections.clear();
        self.connections.extend(match shape {
            RailShape::NorthSouth => [p.north(), p.south()],
            RailShape::EastWest => [p.west(), p.east()],
            RailShape::AscendingEast => [p.west(), p.east().up()],
            RailShape::AscendingWest => [p.west().up(), p.east()],
            RailShape::AscendingNorth => [p.north().up(), p.south()],
            RailShape::AscendingSouth => [p.north(), p.south().up()],
            RailShape::SouthEast => [p.east(), p.south()],
            RailShape::SouthWest => [p.west(), p.south()],
            RailShape::NorthWest => [p.west(), p.north()],
            RailShape::NorthEast => [p.east(), p.north()],
        });
    }

    /// Keeps only the connections whose rail connects back.
    fn remove_soft_connections(&mut self, world: &World) {
        let mut i = 0;
        while i < self.connections.len() {
            match Self::get_rail(world, self.connections[i]) {
                Some(rail) if rail.connects_to(self) => {
                    self.connections[i] = rail.pos;
                    i += 1;
                }
                _ => {
                    self.connections.remove(i);
                }
            }
        }
    }

    fn has_rail(world: &World, pos: BlockPos) -> bool {
        is_rail(world, &pos) || is_rail(world, &pos.up()) || is_rail(world, &pos.down())
    }

    fn get_rail(world: &World, pos: BlockPos) -> Option<Self> {
        Self::new(world, pos)
            .or_else(|| Self::new(world, pos.up()))
            .or_else(|| Self::new(world, pos.down()))
    }

    fn connects_to(&self, rail: &Self) -> bool {
        self.has_connection(rail.pos)
    }

    fn has_connection(&self, rail_pos: BlockPos) -> bool {
        self.connections
            .iter()
            .any(|pos| pos.0.x == rail_pos.0.x && pos.0.z == rail_pos.0.z)
    }

    /// How many of the four sides have a rail, level or one up or down.
    pub(super) fn count_potential_connections(&self, world: &World) -> usize {
        BlockDirection::horizontal()
            .into_iter()
            .filter(|direction| Self::has_rail(world, self.pos.offset(direction.to_offset())))
            .count()
    }

    fn can_connect_to(&self, rail: &Self) -> bool {
        self.connects_to(rail) || self.connections.len() != 2
    }

    fn connect_to(&mut self, world: &Arc<World>, rail: &Self) {
        self.connections.push(rail.pos);
        let p = self.pos;
        let n = self.has_connection(p.north());
        let s = self.has_connection(p.south());
        let w = self.has_connection(p.west());
        let e = self.has_connection(p.east());
        let mut shape = if w || e {
            Some(RailShape::EastWest)
        } else if n || s {
            Some(RailShape::NorthSouth)
        } else {
            None
        };
        if !self.is_straight() {
            if s && e && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if s && w && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if n && w && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if n && e && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        let shape = with_slope(world, p, shape).unwrap_or(RailShape::NorthSouth);
        self.properties.set_shape(shape);
        world.set_block_state(
            &p,
            self.properties.to_state_id(self.block),
            BlockFlags::NOTIFY_ALL,
        );
    }

    fn has_neighbor_rail(&self, world: &World, pos: BlockPos) -> bool {
        Self::get_rail(world, pos).is_some_and(|mut neighbor| {
            neighbor.remove_soft_connections(world);
            neighbor.can_connect_to(self)
        })
    }

    /// Vanilla `RailState.place`: picks the shape from the rails that can connect, a junction's
    /// curve from `has_signal`, sets it when it changed (or `first`) and connects the rails it
    /// now points at.
    pub(super) fn place(
        mut self,
        world: &Arc<World>,
        has_signal: bool,
        first: bool,
        default_shape: RailShape,
    ) -> Self {
        let p = self.pos;
        let n = self.has_neighbor_rail(world, p.north());
        let s = self.has_neighbor_rail(world, p.south());
        let w = self.has_neighbor_rail(world, p.west());
        let e = self.has_neighbor_rail(world, p.east());
        let north_or_south = n || s;
        let west_or_east = w || e;
        let mut shape = if north_or_south && !west_or_east {
            Some(RailShape::NorthSouth)
        } else if west_or_east && !north_or_south {
            Some(RailShape::EastWest)
        } else {
            None
        };
        let (south_east, south_west) = (s && e, s && w);
        let (north_east, north_west) = (n && e, n && w);
        if !self.is_straight() {
            if south_east && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if south_west && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if north_west && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if north_east && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        if shape.is_none() {
            if north_or_south && west_or_east {
                shape = Some(default_shape);
            } else if north_or_south {
                shape = Some(RailShape::NorthSouth);
            } else if west_or_east {
                shape = Some(RailShape::EastWest);
            }
            if !self.is_straight() {
                // The last match wins, so a signal favours the north curves and none the south.
                let order = if has_signal {
                    [
                        (south_east, RailShape::SouthEast),
                        (south_west, RailShape::SouthWest),
                        (north_east, RailShape::NorthEast),
                        (north_west, RailShape::NorthWest),
                    ]
                } else {
                    [
                        (north_west, RailShape::NorthWest),
                        (north_east, RailShape::NorthEast),
                        (south_west, RailShape::SouthWest),
                        (south_east, RailShape::SouthEast),
                    ]
                };
                for (connected, curve) in order {
                    if connected {
                        shape = Some(curve);
                    }
                }
            }
        }
        let shape = with_slope(world, p, shape).unwrap_or(default_shape);

        self.update_connections(shape);
        self.properties.set_shape(shape);
        let state_id = self.properties.to_state_id(self.block);
        if first || world.get_block_state_id(&p) != state_id {
            world.set_block_state(&p, state_id, BlockFlags::NOTIFY_ALL);
            for connection in &self.connections {
                if let Some(mut neighbor) = Self::get_rail(world, *connection) {
                    neighbor.remove_soft_connections(world);
                    if neighbor.can_connect_to(&self) {
                        neighbor.connect_to(world, &self);
                    }
                }
            }
        }
        self
    }
}
