//! Conversion helpers between in-world simulation types and network IDs.
//! Keeps protocol-facing enums stable while allowing internal block enums
//! and storage representations to evolve independently.
use crate::net::protocol::{BlockCell, BlockIdNet};
use crate::world::Block;

#[inline]
pub fn block_to_net(block: Block) -> BlockIdNet {
    match block {
        Block::Air => BlockIdNet::Air,
        Block::Grass => BlockIdNet::Grass,
        Block::Dirt => BlockIdNet::Dirt,
        Block::Stone => BlockIdNet::Stone,
        Block::Sand => BlockIdNet::Sand,
        Block::Snow => BlockIdNet::Snow,
        Block::Wood => BlockIdNet::Wood,
        Block::Leaves => BlockIdNet::Leaves,
        Block::Red => BlockIdNet::Red,
        Block::Blue => BlockIdNet::Blue,
        Block::Yellow => BlockIdNet::Yellow,
        Block::Purple => BlockIdNet::Purple,
        Block::Cyan => BlockIdNet::Cyan,
    }
}

#[inline]
pub fn block_from_net(block: BlockIdNet) -> Block {
    match block {
        BlockIdNet::Air => Block::Air,
        BlockIdNet::Grass => Block::Grass,
        BlockIdNet::Dirt => Block::Dirt,
        BlockIdNet::Stone => Block::Stone,
        BlockIdNet::Sand => Block::Sand,
        BlockIdNet::Snow => Block::Snow,
        BlockIdNet::Wood => Block::Wood,
        BlockIdNet::Leaves => Block::Leaves,
        BlockIdNet::Red => Block::Red,
        BlockIdNet::Blue => Block::Blue,
        BlockIdNet::Yellow => Block::Yellow,
        BlockIdNet::Purple => Block::Purple,
        BlockIdNet::Cyan => Block::Cyan,
    }
}

#[inline]
pub fn block_cell(x: i32, y: i32, z: i32, block: Block) -> BlockCell {
    BlockCell {
        x,
        y,
        z,
        block: block_to_net(block),
    }
}
