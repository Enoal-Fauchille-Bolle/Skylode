//! # Skylode Core
//!
//! Pure game-logic library for Skylode, a mining/idle game inspired by
//! Minecraft. This crate is UI-agnostic: it models the game world, blocks,
//! materials, tools and progression, and exposes them as plain data types and
//! methods. Front-ends (such as the `skylode-tui` crate) build on top of it.
//!
//! ## Module map
//!
//! - [`pickaxe`]: the player's tool, its tier and mining power.
//! - [`enchant`]: enchantments that modify a pickaxe's behaviour.
pub mod enchant;
pub mod pickaxe;

#[cfg(test)]
mod tests {
    // Integration tests for the crate's public API go here.
}
