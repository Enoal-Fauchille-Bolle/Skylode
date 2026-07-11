//! Player progression state.
//!
//! The [`Player`] aggregates everything about the person mining: their current
//! [`level`](Player::level) and [`experience`](Player::experience), their
//! [`pickaxe`](Player::pickaxe), and their [`prestige`](Player::prestige)
//! count. It owns the level-up curve and experience bookkeeping.

use crate::pickaxe::Pickaxe;

/// The player's persistent progression state.
pub struct Player {
    /// Current level; starts at 1 and only increases.
    pub level: u32,
    /// Experience banked toward the *next* level (never exceeds
    /// [`experience_to_next_level`](Player::experience_to_next_level) after an
    /// XP grant is fully processed).
    pub experience: u32,
    /// The pickaxe the player currently mines with.
    pub pickaxe: Pickaxe,
    /// Number of times the player has prestiged (soft-reset for meta rewards).
    pub prestige: u32,
}

impl Default for Player {
    /// Equivalent to [`Player::new`]: a fresh level-1 player.
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    /// Creates a brand-new player: level 1, no experience, default (Wooden)
    /// pickaxe, and zero prestige.
    pub fn new() -> Self {
        Self {
            level: 1,
            experience: 0,
            pickaxe: Pickaxe::default(),
            prestige: 0,
        }
    }

    /// Grants `amount` experience and applies any resulting level-ups.
    ///
    /// The loop handles multiple level-ups from a single large grant: it keeps
    /// subtracting the current level's requirement and incrementing the level
    /// until the leftover experience no longer fills a level. Because the
    /// requirement grows with level, the loop always terminates. The remaining
    /// experience is carried over toward the next level rather than discarded.
    pub fn add_experience(&mut self, amount: u32) {
        self.experience += amount;
        while self.experience >= self.experience_to_next_level() {
            self.experience -= self.experience_to_next_level();
            self.level += 1;
        }
    }

    /// Returns the experience required to advance from the current level to the
    /// next one.
    ///
    /// A simple linear curve: `level * 100` (100 XP for level 1→2, 200 for
    /// 2→3, …), so each level costs a fixed 100 XP more than the previous one.
    pub fn experience_to_next_level(&self) -> u32 {
        self.level * 100
    }
}
