//! Player progression state.
//!
//! The [`Player`] aggregates everything about the person mining: their current
//! [`level`](Player::level) and [`experience`](Player::experience), their
//! [`pickaxe`](Player::pickaxe), and their [`prestige`](Player::prestige)
//! count. It owns the level-up curve and experience bookkeeping.

use crate::{inventory::Inventory, pickaxe::Pickaxe};

/// The player's persistent progression state.
pub struct Player {
    /// The pickaxe the player currently mines with.
    pickaxe: Pickaxe,
    /// Current level; starts at 1 and only increases.
    level: u32,
    /// Experience banked toward the *next* level (never exceeds
    /// [`experience_to_next_level`](Player::experience_to_next_level) after an
    /// XP grant is fully processed).
    experience: u32,
    /// The player's inventory of materials
    inventory: Inventory,
    /// Number of times the player has prestiged (soft-reset for meta rewards).
    prestige: u32,
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
            inventory: Inventory::new(),
        }
    }

    /// Returns the player's current level.
    pub fn get_level(&self) -> u32 {
        self.level
    }

    /// Returns the player's current experience banked toward the next level.
    pub fn get_experience(&self) -> u32 {
        self.experience
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

    /// Returns the player's current pickaxe.
    pub fn get_pickaxe(&self) -> &Pickaxe {
        &self.pickaxe
    }

    /// Returns a reference to the player's inventory.
    pub fn get_inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Returns the player's current prestige count.
    pub fn get_prestige(&self) -> u32 {
        self.prestige
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pickaxe::PickaxeTier;

    #[test]
    fn a_new_player_starts_at_level_one_with_a_wooden_pickaxe() {
        let player = Player::new();
        assert_eq!(player.level, 1);
        assert_eq!(player.experience, 0);
        assert_eq!(player.prestige, 0);
        assert_eq!(player.pickaxe.get_tier(), PickaxeTier::Wooden);
    }

    #[test]
    fn default_is_the_same_as_new() {
        let (new, default) = (Player::new(), Player::default());
        assert_eq!(new.get_level(), default.get_level());
        assert_eq!(new.get_experience(), default.get_experience());
        assert_eq!(new.get_prestige(), default.get_prestige());
        assert_eq!(new.get_pickaxe(), default.get_pickaxe());
        assert_eq!(new.get_inventory(), default.get_inventory());
    }

    #[test]
    fn the_level_cost_grows_by_a_hundred_per_level() {
        let mut player = Player::new();
        assert_eq!(player.experience_to_next_level(), 100);
        player.level = 2;
        assert_eq!(player.experience_to_next_level(), 200);
        player.level = 10;
        assert_eq!(player.experience_to_next_level(), 1000);
    }

    #[test]
    fn experience_short_of_the_threshold_is_only_banked() {
        let mut player = Player::new();
        player.add_experience(99);
        assert_eq!(player.level, 1);
        assert_eq!(player.experience, 99);
    }

    #[test]
    fn exactly_enough_experience_levels_up_and_banks_nothing() {
        let mut player = Player::new();
        player.add_experience(100);
        assert_eq!(player.level, 2);
        assert_eq!(player.experience, 0);
    }

    #[test]
    fn surplus_experience_carries_over_instead_of_being_lost() {
        let mut player = Player::new();
        player.add_experience(150);
        assert_eq!(player.level, 2);
        assert_eq!(player.experience, 50);
    }

    /// Offline progress is credited as one lump grant, so a single call has to
    /// cascade through as many levels as it can pay for — not just one.
    #[test]
    fn one_large_grant_cascades_through_several_levels() {
        let mut player = Player::new();
        player.add_experience(100 + 200 + 300 + 50);
        assert_eq!(player.level, 4);
        assert_eq!(player.experience, 50);
    }

    /// Granting the same total in pieces or in one go must land in the same
    /// place; anything else would make offline credit differ from live play.
    #[test]
    fn many_small_grants_match_one_large_grant() {
        let mut piecemeal = Player::new();
        for _ in 0..65 {
            piecemeal.add_experience(10);
        }
        let mut lump = Player::new();
        lump.add_experience(650);

        assert_eq!(piecemeal.level, lump.level);
        assert_eq!(piecemeal.experience, lump.experience);
    }

    #[test]
    fn granting_no_experience_changes_nothing() {
        let mut player = Player::new();
        player.add_experience(60);
        player.add_experience(0);
        assert_eq!(player.level, 1);
        assert_eq!(player.experience, 60);
    }

    /// The invariant that makes the level-up loop's exit condition sound: after
    /// a grant is processed, the banked experience is never enough to buy
    /// another level.
    #[test]
    fn banked_experience_never_covers_another_level() {
        let mut player = Player::new();
        for grant in [37, 250, 1, 999, 12_345] {
            player.add_experience(grant);
            assert!(
                player.experience < player.experience_to_next_level(),
                "level {} is holding {} banked XP, enough to buy another level",
                player.level,
                player.experience
            );
        }
    }
}
