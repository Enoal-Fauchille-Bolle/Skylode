//! Player progression state.
//!
//! The [`Player`] aggregates everything about the person mining: their current
//! level and experience, their pickaxe, their inventory, and their prestige
//! count. It owns the level-up curve and experience bookkeeping.
//!
//! Every field is private and reached through an accessor, so the level-up
//! invariant — banked experience never covers another level — cannot be broken
//! from outside. At [`LEVEL_CAP`] the invariant holds in its degenerate form:
//! there is no next level to cover, and the bank is emptied to say so.

use crate::{inventory::Inventory, pickaxe::Pickaxe, tunables::LEVEL_CAP};

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

    /// Grants `amount` experience, applies any resulting level-ups, and returns
    /// **how many levels were gained** — zero if the grant only banked.
    ///
    /// The loop handles multiple level-ups from a single large grant: it keeps
    /// subtracting the current level's requirement and incrementing the level
    /// until the leftover experience no longer fills a level. The remaining
    /// experience is carried over toward the next level rather than discarded.
    ///
    /// The count is returned rather than merely applied because a grant is not
    /// self-describing: the systems that hang off a level-up — the world unlocks
    /// at [`NETHER_UNLOCK_LEVEL`](crate::tunables::NETHER_UNLOCK_LEVEL) and
    /// [`END_UNLOCK_LEVEL`](crate::tunables::END_UNLOCK_LEVEL), and the per-level
    /// reward bundle — owe one thing *per level crossed*, and a lump grant of
    /// offline experience can cross several at once. Recomputing that from the
    /// outside means sampling the level before and after and trusting the two
    /// samples to bracket the call; the loop already knows the answer.
    ///
    /// Deliberately **not** `#[must_use]`: a caller that only wants the
    /// experience applied is not making a mistake, and the tick loop is expected
    /// to be one.
    ///
    /// # The cap
    ///
    /// The climb stops at [`LEVEL_CAP`], and the loop needs no guard of its own
    /// to do it: [`experience_to_next_level`](Player::experience_to_next_level)
    /// stops offering a price, so the `while let` runs out of `Some` and exits.
    /// The ceiling is therefore written in exactly one place —
    /// [`xp_for_level`](Player::xp_for_level) — instead of once per loop that
    /// walks the curve.
    ///
    /// Experience granted at the cap is **dropped**, not banked: past the last
    /// level there is nothing for it to buy, and a number climbing beside a bar
    /// that reads "MAX" would only invite the question of what it is for.
    ///
    /// `saturating_add`, not `+=`, because a grant near [`u32::MAX`] would
    /// overflow — a panic in a debug build, and the workspace lints are explicit
    /// that this crate refuses rather than traps. Saturating costs nothing here:
    /// any amount large enough to saturate is far past the cap, where the
    /// surplus is discarded anyway.
    pub fn add_experience(&mut self, amount: u32) -> u32 {
        let level_before = self.level;
        self.experience = self.experience.saturating_add(amount);
        while let Some(needed) = self.experience_to_next_level() {
            if self.experience < needed {
                break;
            }
            self.experience -= needed;
            self.level += 1;
        }
        if self.experience_to_next_level().is_none() {
            self.experience = 0;
        }
        self.level - level_before
    }

    /// Returns the experience required to advance from the current level to the
    /// next one, or [`None`] once the player is at [`LEVEL_CAP`].
    ///
    /// The current-state view of [`xp_for_level`](Player::xp_for_level), which
    /// carries the curve and the reasoning behind the [`Option`].
    pub fn experience_to_next_level(&self) -> Option<u32> {
        Self::xp_for_level(self.level)
    }

    /// Returns the experience required to advance *from* `level` to the next
    /// one, or [`None`] if no such level exists.
    ///
    /// A simple linear curve: `level * 100` (100 XP for level 1→2, 200 for
    /// 2→3, …), so each level costs a fixed 100 XP more than the previous one.
    /// The *shape* is provisional — `docs/ROADMAP.md` still lists the XP curve
    /// as an open tunable — but the *query* is not: this is the single place the
    /// curve is written, and the single place [`LEVEL_CAP`] is enforced.
    ///
    /// **An associated function, not a method**, because its whole job is to
    /// answer about a level the player has not reached: the Levels screen draws
    /// the entire 1→[`LEVEL_CAP`] ladder in advance, and a `&self` method could
    /// only ever report on the rung the player is standing on.
    ///
    /// **[`Option`] rather than a `0` sentinel**, and the difference is not
    /// stylistic. `0` would leave `while experience >= needed` true forever on an
    /// unsigned type, and would divide a progress gauge by zero the first time
    /// someone hit the cap. `None` states "there is no next level" in the type,
    /// so neither caller can be written by accident.
    ///
    /// `None` for level `0` as well: it is below the starting level and names no
    /// real rung, so there is no meaningful price to quote for it.
    pub fn xp_for_level(level: u32) -> Option<u32> {
        (1..LEVEL_CAP).contains(&level).then(|| level * 100)
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
        assert_eq!(player.experience_to_next_level(), Some(100));
        player.level = 2;
        assert_eq!(player.experience_to_next_level(), Some(200));
        player.level = 10;
        assert_eq!(player.experience_to_next_level(), Some(1000));
    }

    /// The schedule has to answer for the rung *below* the cap — the last level
    /// the player can actually buy — and refuse for the cap itself and for the
    /// level-0 non-rung. Off by one here either freezes the player at 49 or lets
    /// them buy a level 51 that no world, reward or UI knows about.
    #[test]
    fn the_last_level_before_the_cap_still_has_a_price() {
        assert_eq!(Player::xp_for_level(1), Some(100));
        assert_eq!(
            Player::xp_for_level(LEVEL_CAP - 1),
            Some((LEVEL_CAP - 1) * 100)
        );
        assert_eq!(Player::xp_for_level(LEVEL_CAP), None);
        assert_eq!(Player::xp_for_level(LEVEL_CAP + 1), None);
        assert_eq!(Player::xp_for_level(0), None);
    }

    /// The two views of the curve must not drift: `experience_to_next_level` is
    /// the running one, `xp_for_level` the one the Levels screen queries ahead of
    /// time, and a player reading a roadmap that disagrees with their own bar has
    /// been lied to by one of them.
    #[test]
    fn the_schedule_agrees_with_the_running_curve() {
        let mut player = Player::new();
        for level in 1..=LEVEL_CAP {
            player.level = level;
            assert_eq!(
                player.experience_to_next_level(),
                Player::xp_for_level(level),
                "the two curves disagree at level {level}"
            );
        }
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
    /// another level. At the cap it holds in its degenerate form — there is no
    /// next level, and the bank is empty.
    #[test]
    fn banked_experience_never_covers_another_level() {
        let mut player = Player::new();
        for grant in [37, 250, 1, 999, 12_345] {
            player.add_experience(grant);
            match player.experience_to_next_level() {
                Some(needed) => assert!(
                    player.experience < needed,
                    "level {} is holding {} banked XP, enough to buy another level",
                    player.level,
                    player.experience
                ),
                None => assert_eq!(player.experience, 0),
            }
        }
    }

    /// The total the curve asks for to walk from level 1 to the cap: the sum of
    /// `n * 100` over every rung that has a price.
    const XP_TO_CAP: u32 = 100 * (LEVEL_CAP - 1) * LEVEL_CAP / 2;

    /// An absurd grant must stop at the cap rather than run the loop off the end
    /// of the curve.
    #[test]
    fn the_level_stops_at_the_cap() {
        let mut player = Player::new();
        player.add_experience(u32::MAX);
        assert_eq!(player.level, LEVEL_CAP);
    }

    /// Past the last level there is nothing left to buy, so the surplus is
    /// dropped rather than left sitting beside a bar that reads "MAX".
    #[test]
    fn experience_at_the_cap_is_dropped_rather_than_banked() {
        let mut player = Player::new();
        player.add_experience(XP_TO_CAP + 50_000);
        assert_eq!(player.level, LEVEL_CAP);
        assert_eq!(player.experience, 0);
        assert_eq!(player.experience_to_next_level(), None);
    }

    /// The cap is an end state, not a refusal: granting into it is legal, it
    /// simply buys nothing. The zero return is how the reward system will know
    /// to hand out nothing.
    #[test]
    fn a_capped_player_gains_no_further_levels() {
        let mut player = Player::new();
        player.add_experience(XP_TO_CAP);
        assert_eq!(player.level, LEVEL_CAP);
        assert_eq!(player.add_experience(9_999), 0);
        assert_eq!(player.level, LEVEL_CAP);
        assert_eq!(player.experience, 0);
    }

    /// The count is what the world unlocks and the per-level rewards will read:
    /// a lump grant of offline experience owes one reward per level *crossed*,
    /// not one per call.
    #[test]
    fn add_experience_reports_the_levels_it_bought() {
        let mut player = Player::new();
        assert_eq!(player.add_experience(50), 0);
        assert_eq!(player.add_experience(50), 1);
        // 200 + 300 buys levels 3 and 4 from level 2, with 100 left banked.
        assert_eq!(player.add_experience(600), 2);
        assert_eq!(player.level, 4);
        assert_eq!(player.experience, 100);
    }

    /// The piecemeal/lump agreement has to survive the cap too, since that is
    /// exactly where the two paths could diverge: a lump grant saturates and is
    /// discarded in one step, while small grants arrive after the bank has
    /// already been emptied.
    #[test]
    fn piecemeal_and_lump_grants_agree_across_the_cap() {
        let mut piecemeal = Player::new();
        for _ in 0..200 {
            piecemeal.add_experience(1_000);
        }
        let mut lump = Player::new();
        lump.add_experience(200 * 1_000);

        assert_eq!(piecemeal.level, LEVEL_CAP);
        assert_eq!(piecemeal.level, lump.level);
        assert_eq!(piecemeal.experience, lump.experience);
    }
}
