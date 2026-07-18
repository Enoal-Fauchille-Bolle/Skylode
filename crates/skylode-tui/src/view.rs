//! The read model the screens render from.
//!
//! Screens never reach into game state directly — they render a **flat snapshot**.
//! Today the core has neither `GameState` nor `tick()` (phases 5–7), so
//! [`View::sample`] hand-fills the numbers drawn in UI-EN.md §5. When the core
//! catches up, `GameState` produces this same struct and **nothing under
//! `screen/` changes**. That indirection is the whole reason UI work can start
//! before the rules exist.
//!
//! Keep it plain data: no methods that decide anything, no `Option`s standing in
//! for rules. A computation that belongs to the game belongs to the core.

/// A frame's worth of game state, already reduced to what the UI prints.
#[derive(Clone, Debug)]
pub struct View {
    /// Mining level — the XP axis of the two-axis progression.
    pub player_level: u32,
    /// XP banked toward the next level, counted from zero (UI-EN.md §5.7.5).
    pub xp: u32,
    /// XP the current level requires in total.
    pub xp_to_next: u32,
    /// Display name of the mine the player is standing in.
    pub mine_name: String,
    /// Pickaxe name plus its enchant summary, as one printable line.
    pub pickaxe: String,
    /// Raw units of the current mine's material that the player holds.
    pub raw_held: u32,
    /// Compressed units of the same material.
    pub compressed_held: u32,
}

impl View {
    /// The placeholder save drawn throughout UI-EN.md §5: level 23, Diamond
    /// pickaxe, standing in the Iron Mine.
    ///
    /// These figures are chosen to match the wireframes so a rendered screen can
    /// be compared against the counted frame, not invented independently.
    pub fn sample() -> Self {
        Self {
            player_level: 23,
            xp: 1_240,
            xp_to_next: 3_200,
            mine_name: "Iron Mine".to_owned(),
            pickaxe: "Diamond Pickaxe  Efficiency IV".to_owned(),
            raw_held: 480,
            compressed_held: 2,
        }
    }
}
