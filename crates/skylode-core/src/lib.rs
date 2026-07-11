//! # Skylode Core
//!
//! Pure game-logic library for Skylode, a mining/idle game inspired by
//! Minecraft. This crate is UI-agnostic: it models the game world, blocks,
//! materials, tools and progression, and exposes them as plain data types and
//! methods. Front-ends (such as the `skylode-tui` crate) build on top of it.
//!
//! ## Module map
//!
//! - [`world`]: the three dimensions (Overworld, Nether, End) and which
//!   blocks/materials belong to each.
//! - [`block`]: individual placeable/mineable blocks and their properties
//!   (hardness, required pickaxe tier, drops).
//! - [`material`]: the raw resources that blocks yield when mined.
//! - [`mine`]: a generated grid of blocks the player digs through, sized by
//!   level.
//! - [`pickaxe`]: the player's tool, its tier and mining power.
//! - [`enchant`]: enchantments that modify a pickaxe's behaviour.
pub mod block;
pub mod enchant;
pub mod material;
pub mod mine;
pub mod pickaxe;
pub mod world;

#[cfg(test)]
mod tests {
    // Integration tests for the crate's public API go here.
}
