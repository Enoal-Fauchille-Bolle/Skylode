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
//! - [`material`]: the raw resources that blocks yield when mined, and the two
//!   denominations — raw and Compressed — the player holds them in.
//! - [`inventory`]: what the player is carrying, and the compression they
//!   convert it with.
//! - [`economy`]: what upgrades cost, and the transactional path that pays for
//!   them.
//! - [`mine`]: a generated grid of blocks the player digs through, sized by
//!   level.
//! - [`mine_kind`]: the twelve canonical mines and their identity (block pool,
//!   world, gating tier, materials).
//! - [`pickaxe`]: the player's tool, its tier and mining power.
//! - [`enchant`]: enchantments that modify a pickaxe's behaviour.
//! - [`player`]: the player's progression state (level, experience, pickaxe).
//! - [`rng`]: the seeded, replayable source of every draw the rules make.
//! - [`tunables`]: the open balance constants the design left to implementation.
//! - [`error`]: what the rules refuse, and why.

pub mod block;
pub mod economy;
pub mod enchant;
pub mod error;
pub mod inventory;
pub mod material;
pub mod mine;
pub mod mine_kind;
pub mod pickaxe;
pub mod player;
pub mod rng;
pub mod tunables;
pub mod world;
