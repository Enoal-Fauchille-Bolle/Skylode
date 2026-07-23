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

use crate::{
    block::Block,
    inventory::Inventory,
    mine_kind::{MineKind, MineLock},
    pickaxe::Pickaxe,
    prestige,
    tunables::LEVEL_CAP,
    world::World,
};
use serde::{Deserialize, Serialize};

/// The player's persistent progression state.
///
/// [`Debug`] because [`GameState`](crate::game::GameState) derives it, and a run
/// that cannot be printed is a run that cannot be dropped into a failing
/// assertion. Every field is already `Debug`.
#[derive(Debug, Serialize, Deserialize)]
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
    /// The unpaid fraction of a point of experience, in permille.
    ///
    /// What makes the prestige multiplier survive being applied to an integer: a
    /// swing worth 7 experience at rank I is worth 8.4, and truncating the 0.4 on
    /// every swing would quietly cost the player a level over a session. The
    /// remainder rides here to the next swing instead — see
    /// [`prestige::apply_with_carry`](crate::prestige).
    ///
    /// It lives beside [`prestige`](Player) rather than on
    /// [`GameState`](crate::game::GameState) for the reason
    /// [`grant_break_experience`](Player::grant_break_experience) is a method at all:
    /// the rank is here, so neither the rank nor its remainder has to travel to meet
    /// the grant. Always below 1000, by construction of the carry.
    xp_carry: u32,
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
            xp_carry: 0,
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
    /// `pub(crate)`, and for the reason [`Mine::take`](crate::mine::Mine) and
    /// [`Boost::new`](crate::boost::Boost) are: it is **free**. A front-end able
    /// to call it could hand itself the 122 500 experience the curve asks for and
    /// stand at [`LEVEL_CAP`] with every world open, in one call. The rules grant
    /// experience through
    /// [`grant_break_experience`](Player::grant_break_experience), which is the
    /// only door that knows *why* experience is arriving — a bare amount is not
    /// something the core should accept from outside.
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
    pub(crate) fn add_experience(&mut self, amount: u32) -> u32 {
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

    /// Grants the experience owed for the blocks one swing broke, and returns
    /// **how many levels it bought** — [`add_experience`](Player::add_experience)'s
    /// count, for the same consumer.
    ///
    /// **Per block, and before Fortune.** The experience is
    /// [`Block::xp_value`] — a property of the cell that stood there, not of what
    /// the player walked away with. That ordering is what holds the two
    /// progression axes apart: levels open worlds, ore opens pickaxes, and if
    /// [Fortune](Pickaxe::fortune_multiplier) multiplied experience as well as
    /// loot, one purchase would advance both and *"neither axis alone carries
    /// progression"* would quietly stop being true. The same goes for
    /// [Excavator](crate::enchant::EnchantType), which substitutes a drop and
    /// leaves the block it came from untouched.
    ///
    /// **It takes blocks and nothing else, and that is the enforcement.** There is
    /// no [`Pickaxe`] parameter here, so Fortune cannot be applied on this path
    /// even by mistake — the rule is a missing argument rather than a comment a
    /// caller has to honour. See `docs/MECHANICS.md`.
    ///
    /// **Every block that fell pays, including the ones a blast took.** That is
    /// not the door Fortune was refused through: Fortune multiplies the yield of
    /// *one* block, while [Explosive, Jackhammer and
    /// Nuke](crate::mine::Mine::resolve_spatial_procs) make *more blocks fall* —
    /// the same kind of gain as breaking faster with Efficiency, which nobody
    /// expects to leave the level bar alone. It also keeps a full grid worth
    /// `base * (1 + 2w)` for a dial weight `w` whatever procs happened to fire,
    /// which is the property phase 10 balances the twelve bases against.
    ///
    /// **One grant per swing, not one per block.** The phase-7 tick holds the
    /// impact cell ([`Mine::dig`](crate::mine::Mine)) and the blasted ones in the
    /// same step, and then owes one
    /// [reward](crate::reward::reward_for_level) *per level crossed*. A single
    /// call yields a single count; splitting it would hand the caller two counts
    /// to add up, and nothing in the types would stop the two from claiming the
    /// same level.
    ///
    /// **A method on [`Player`] rather than a free function**, because the permanent
    /// per-rank multiplier on experience gain needs
    /// [`prestige`](Player::get_prestige), already a field here. Anywhere else, the
    /// rank would have to travel to meet it — and so would the remainder it leaves
    /// behind ([`xp_carry`](Player)).
    ///
    /// **The multiplier is applied to the swing's total, once**, before
    /// [`add_experience`](Player::add_experience) sees it. Not per block, for the
    /// reason directly above — a swing is one grant — and not after the level-up
    /// loop, which would mean scaling levels rather than experience. At rank 0 it
    /// multiplies by exactly 1 and carries nothing, so a run that never prestiged
    /// levels exactly as it did before phase 8.
    ///
    /// `fold` with `saturating_add` rather than `sum()`: `sum()` panics on
    /// overflow in a debug build, and this crate's lints refuse a panic where a
    /// refusal will do. The realistic ceiling is a Nuke over a full grid — 200
    /// cells at 72 — so the saturation is doctrine rather than necessity, and it
    /// costs nothing to keep it true after phase 10 moves the bases.
    ///
    /// `pub(crate)` for [`add_experience`](Player::add_experience)'s reason: the
    /// blocks are supplied by the caller, so a front-end holding this could invent
    /// a swing that broke two hundred End cells.
    pub(crate) fn grant_break_experience(&mut self, broken: &[Block]) -> u32 {
        let total = broken
            .iter()
            .fold(0u32, |total, block| total.saturating_add(block.xp_value()));
        let earned = prestige::apply_with_carry(
            total,
            prestige::multiplier_permille(self.prestige),
            &mut self.xp_carry,
        );
        self.add_experience(earned)
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

    /// Whether the player's mining level has opened `world`.
    ///
    /// A **query, not a stored set.** The unlocked worlds are an interval that
    /// grows with [`get_level`](Player::get_level), so there is nothing to keep
    /// in sync — and nothing for phase 8's prestige to remember to clear when it
    /// resets the level. See [`World::is_unlocked_at`].
    pub fn has_unlocked(&self, world: World) -> bool {
        world.is_unlocked_at(self.level)
    }

    /// The furthest world the player's mining level has opened.
    ///
    /// Returns a [`World`] and not an [`Option`]: the Overworld opens at level 1
    /// and the player starts there, so "no world" is not a state that exists.
    ///
    /// This is the argument the enchant path has been waiting for.
    /// [`EnchantType::max_level`](crate::enchant::EnchantType::max_level) and
    /// [`economy::buy_enchant`](crate::economy::buy_enchant) both take *the
    /// highest world unlocked* — not the world the player happens to be mining
    /// in — because [`World::enchant_cap`] is a ceiling reaching a dimension
    /// raises permanently, not a bonus that lapses when you walk back to the
    /// Overworld.
    ///
    /// Tested from the furthest world **downwards**, so the first arm that holds
    /// is the answer. The reverse order would report the Overworld for every
    /// player, since it is unlocked for all of them — the fallthrough is the
    /// *floor*, not a case.
    pub fn highest_unlocked_world(&self) -> World {
        if self.has_unlocked(World::End) {
            World::End
        } else if self.has_unlocked(World::Nether) {
            World::Nether
        } else {
            World::Overworld
        }
    }

    /// Why `kind` is closed to this player, or that it is open.
    ///
    /// The two-axis gate answered in one call: the level half comes from the
    /// mine's [`World`], the tier half from the player's pickaxe. See
    /// [`MineKind::lock`], which owns the rule; this only supplies the player's
    /// two numbers.
    pub fn mine_lock(&self, kind: MineKind) -> MineLock {
        kind.lock(self.level, self.pickaxe.get_tier())
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

    /// Whether this progression could have been produced by the rules.
    ///
    /// What a save can break here that play cannot: the level is the axis every
    /// other gate reads — the worlds it unlocks, the enchant ceiling those worlds
    /// set, the mines they open — so a level outside `1..=LEVEL_CAP` is not one
    /// wrong number but a wrong answer to every question asked afterwards. A level
    /// of `0` in particular makes [`xp_for_level`](Player::xp_for_level) answer
    /// [`None`], which reads as "already at the cap": the run would silently stop
    /// levelling forever.
    ///
    /// The enchant caps are checked against the tier and the world the level
    /// **currently** gives, which is stricter than the rules were when the levels
    /// were bought — a prestige resets the level and re-locks the worlds. It is
    /// deliberately so: [`prestige_reset`](Player::prestige_reset) resets the
    /// pickaxe too, so no honest save has enchants its player could not buy again.
    ///
    /// See [`Mine::validate`](crate::mine::Mine) for why the message is a plain
    /// string.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.level == 0 || self.level > LEVEL_CAP {
            return Err("a player's level is outside the ladder the game has");
        }

        // The carry is a *remainder* of a division by 1000; at or above it, a whole
        // point of experience is owed and was never paid.
        if self.xp_carry >= prestige::PERMILLE {
            return Err("a player's experience carry is a whole point that was never paid");
        }

        // Banked experience is always below the next level's price: the grant loop
        // spends it down as it crosses. Above it, the player is owed a level-up that
        // will never come, because nothing re-checks between grants.
        if let Some(needed) = self.experience_to_next_level()
            && self.experience >= needed
        {
            return Err("a player has banked a level's worth of experience without gaining it");
        }

        let tier = self.pickaxe.get_tier();
        let world = self.highest_unlocked_world();
        for (kind, level) in self.pickaxe.enchants().iter() {
            if level > kind.max_level(tier, world) {
                return Err("an enchant is above the cap its tier and world allow");
            }
        }

        self.inventory.validate()
    }

    /// The inventory, mutably: where a swing's loot and a level-up's bundle land.
    ///
    /// `pub(crate)`, unlike its `&self` twin. [`Inventory::add`] is free and
    /// unbounded, so a public door here would be every material in the game for the
    /// asking — the argument that closed [`Mine::take`](crate::mine::Mine) and
    /// [`Boost::new`](crate::boost::Boost). Spending is already public through
    /// [`economy`](crate::economy), which is where a debit belongs.
    pub(crate) fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.inventory
    }

    /// The inventory and the pickaxe, mutably, **in one call**.
    ///
    /// Not a convenience: it is the only shape that compiles. Every purchase in
    /// [`economy`](crate::economy) takes `(&mut Inventory, &mut Pickaxe)`, and two
    /// separate `&mut self` accessors would be two overlapping borrows of the same
    /// [`Player`] — the borrow checker rejects the second while the first is live,
    /// however disjoint the fields behind them happen to be. Returning both from one
    /// call is how a *method* hands out the field-precise borrows the caller could
    /// only otherwise take by touching the fields directly, which nothing outside
    /// this module can do.
    ///
    /// The same reason [`draw_cell`](crate::mine) is a free function rather than a
    /// method: a borrow's granularity is the receiver, not the fields it reaches.
    pub(crate) fn inventory_and_pickaxe_mut(&mut self) -> (&mut Inventory, &mut Pickaxe) {
        (&mut self.inventory, &mut self.pickaxe)
    }

    /// Banks one prestige rank and throws the rest of the player away.
    ///
    /// The player's half of the deep reset `docs/MECHANICS.md` specifies: pickaxe back
    /// to Wooden, every enchant to 0, the inventory emptied, the level and its banked
    /// experience back to the start. [`GameState::prestige`] owns the rest of the run
    /// — the mines, the boosts, the loot remainder — because those are its fields, not
    /// this struct's.
    ///
    /// **Written as a struct update from [`new`](Player::new), not field by field**,
    /// and that is the whole point of the shape:
    ///
    /// ```ignore
    /// *self = Self { prestige, ..Self::new() };
    /// ```
    ///
    /// A field added to [`Player`] later is then reset **by default**, and *keeping*
    /// it across a prestige becomes a deliberate edit to this line. Written the other
    /// way — assigning each field back to its starting value — the field somebody
    /// forgets is the one that survives the reset, silently, which is exactly the
    /// leak `docs/DECISIONS.md` closes for a mine's richness. `..` also makes the
    /// compiler no help at all otherwise: an omitted field is not an error, it is a
    /// field left alone.
    ///
    /// **The unlocked worlds need no line here**, because they are a query over the
    /// level ([`has_unlocked`](Player::has_unlocked)) rather than a stored set. The
    /// level going back to 1 *is* the End closing again.
    ///
    /// `saturating_add`, for the reason the rest of this module saturates: the rank is
    /// unbounded on purpose, and a `u32` that wrapped would hand a player at
    /// [`u32::MAX`] a rank-0 multiplier as their reward for the longest run in the
    /// game.
    ///
    /// `pub(crate)` for [`add_experience`](Player::add_experience)'s reason — this is
    /// free, and a front-end able to call it could bank ranks without ever paying
    /// Amethyst. The gated door is [`GameState::prestige`].
    ///
    /// [`GameState::prestige`]: crate::game::GameState::prestige
    pub(crate) fn prestige_reset(&mut self) {
        let prestige = self.prestige.saturating_add(1);
        *self = Self {
            prestige,
            ..Self::new()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::{EnchantType, Enchants};
    use crate::material::{Item, Material};
    use crate::pickaxe::PickaxeTier;
    use crate::tunables::{END_UNLOCK_LEVEL, NETHER_UNLOCK_LEVEL};
    use crate::world::ALL_WORLDS;

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

    /// The worlds open in order as the level climbs, and one level short of a
    /// threshold is still short. Checked through the player rather than through
    /// [`World::is_unlocked_at`] directly, since it is the player's level the gate
    /// is meant to read.
    #[test]
    fn the_worlds_open_one_after_the_other_as_the_level_climbs() {
        let mut player = Player::new();
        for (level, expected) in [
            (1, World::Overworld),
            (NETHER_UNLOCK_LEVEL - 1, World::Overworld),
            (NETHER_UNLOCK_LEVEL, World::Nether),
            (END_UNLOCK_LEVEL - 1, World::Nether),
            (END_UNLOCK_LEVEL, World::End),
            (LEVEL_CAP, World::End),
        ] {
            player.level = level;
            assert_eq!(
                player.highest_unlocked_world(),
                expected,
                "at level {level} the furthest world should be {}",
                expected.name()
            );
        }
    }

    /// `has_unlocked` and `highest_unlocked_world` are two views of one interval,
    /// so a world at or below the furthest one must read as unlocked and every
    /// world above it must not. This is what makes the *set* the design asks for
    /// derivable, rather than something the state has to carry.
    #[test]
    fn everything_up_to_the_furthest_world_is_unlocked_and_nothing_past_it() {
        let mut player = Player::new();
        for level in 1..=LEVEL_CAP {
            player.level = level;
            let furthest = player.highest_unlocked_world();
            for world in ALL_WORLDS {
                assert_eq!(
                    player.has_unlocked(world),
                    world.unlock_level() <= furthest.unlock_level(),
                    "at level {level}, {} disagrees with a furthest world of {}",
                    world.name(),
                    furthest.name()
                );
            }
        }
    }

    /// What the phase-5 enchant path has been waiting for: the cap it prices
    /// against is the *highest world unlocked*, so levelling up must be what
    /// raises it. Goes through
    /// [`World::enchant_cap`](crate::world::World::enchant_cap) rather than
    /// comparing worlds, because a query that named the right world but fed the
    /// wrong number to `buy_enchant` would still be broken.
    #[test]
    fn levelling_up_is_what_raises_the_enchant_ceiling() {
        let mut player = Player::new();
        let ceiling = |player: &Player| player.highest_unlocked_world().enchant_cap();

        assert_eq!(ceiling(&player), World::Overworld.enchant_cap());
        player.level = NETHER_UNLOCK_LEVEL;
        assert_eq!(ceiling(&player), World::Nether.enchant_cap());
        player.level = END_UNLOCK_LEVEL;
        assert_eq!(ceiling(&player), World::End.enchant_cap());
    }

    /// The player-facing form of the two-axis gate, and the one case that proves
    /// it reads *both* of the player's numbers: a fresh player is short a
    /// dimension and a pickaxe for the Obsidian mine, and open on the Stone one.
    #[test]
    fn a_fresh_player_is_short_of_both_axes_on_the_late_mines() {
        let player = Player::new();
        assert!(player.mine_lock(MineKind::Stone).is_open());

        let lock = player.mine_lock(MineKind::Obsidian);
        assert_eq!(lock.missing_level(), Some(NETHER_UNLOCK_LEVEL));
        assert_eq!(lock.missing_tier(), Some(PickaxeTier::Diamond));
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

    #[test]
    fn breaking_one_block_grants_exactly_its_xp_value() {
        let mut player = Player::new();
        assert_eq!(player.grant_break_experience(&[Block::IronOre]), 0);
        assert_eq!(player.experience, Block::IronOre.xp_value());
    }

    /// A swing is the impact cell plus whatever the spatial enchants took with
    /// it, and every one of them paid for standing there.
    #[test]
    fn a_swing_grants_the_sum_of_every_block_it_broke() {
        let mut player = Player::new();
        let swing = [Block::IronOre, Block::Stone, Block::Stone, Block::IronBlock];
        player.grant_break_experience(&swing);
        assert_eq!(player.experience, 3 + 1 + 1 + 9);
    }

    /// The settled rule, pinned: the cells a blast takes pay their own way, so
    /// how the swing is grouped cannot change what a grid is worth. Without this,
    /// a later refactor could quietly make the experience a swing yields depend
    /// on which procs fired.
    #[test]
    fn a_blast_pays_the_same_as_breaking_its_cells_one_by_one() {
        let blast = [Block::Endstone, Block::Amethyst, Block::Endstone];

        let mut in_one_swing = Player::new();
        in_one_swing.grant_break_experience(&blast);

        let mut one_at_a_time = Player::new();
        for block in blast {
            one_at_a_time.grant_break_experience(&[block]);
        }

        assert_eq!(in_one_swing.level, one_at_a_time.level);
        assert_eq!(in_one_swing.experience, one_at_a_time.experience);
    }

    /// A tick where nothing broke still calls this, so an empty swing has to be
    /// a no-op rather than a special case the tick loop remembers to skip.
    #[test]
    fn breaking_nothing_grants_nothing() {
        let mut player = Player::new();
        player.add_experience(40);
        assert_eq!(player.grant_break_experience(&[]), 0);
        assert_eq!(player.experience, 40);
        assert_eq!(player.level, 1);
    }

    /// The rule the whole two-axis gate rests on: experience is what the *block*
    /// was worth, never what the player walked away with. Fortune is pushed to
    /// its ceiling and the grant must not move — if it ever does, one purchase
    /// has started advancing both progression axes at once.
    #[test]
    fn fortune_does_not_touch_the_experience_a_block_grants() {
        let mut player = Player::new();
        while player
            .pickaxe
            .upgrade_enchant(EnchantType::Fortune, World::End)
            .is_ok()
        {}
        assert!(
            player.pickaxe.fortune_multiplier() > 1,
            "the test proves nothing unless Fortune is actually installed"
        );

        player.grant_break_experience(&[Block::DiamondOre]);
        assert_eq!(player.experience, Block::DiamondOre.xp_value());
    }

    /// The hook the phase-7 tick needs: a single swing can cross a level, and the
    /// count it reports is what the per-level rewards will be paid against.
    #[test]
    fn a_swing_can_cross_a_level_and_says_so() {
        let mut player = Player::new();
        // Level 1 costs 100 XP; a full Nuke of Amethyst is worth 72 apiece.
        assert_eq!(player.grant_break_experience(&[Block::Amethyst; 2]), 1);
        assert_eq!(player.level, 2);
        assert_eq!(player.experience, 2 * 72 - 100);
    }

    /// The regression guard the whole phase rests on: at rank 0 the multiplier is
    /// exactly 1 and the carry never moves, so a run that never prestiged levels
    /// exactly as it did before prestige existed. Every XP figure asserted above this
    /// line is only still true because of it.
    #[test]
    fn an_unprestiged_player_earns_the_raw_xp_value() {
        let mut player = Player::new();
        player.grant_break_experience(&[Block::IronOre, Block::Stone]);
        assert_eq!(player.experience, 3 + 1);
        assert_eq!(player.xp_carry, 0);
    }

    /// A rank scales the swing's total, not each block: 4 XP at rank I is 4.8, which
    /// banks 4 and keeps 800 permille for the next swing.
    #[test]
    fn a_rank_scales_the_experience_a_swing_is_worth() {
        let mut player = Player::new();
        player.prestige = 1;
        player.grant_break_experience(&[Block::IronOre, Block::Stone]);
        assert_eq!(player.experience, 4);
        assert_eq!(player.xp_carry, 800);
    }

    /// The carry's whole reason, on the experience side: five swings worth 1 XP each
    /// pay six at rank I. Truncating each swing would pay five and the rank would be
    /// worth nothing to a player breaking Stone — which is every player who has just
    /// prestiged.
    #[test]
    fn a_carried_remainder_pays_the_sixth_experience_point() {
        let mut player = Player::new();
        player.prestige = 1;
        for _ in 0..5 {
            player.grant_break_experience(&[Block::Stone]);
        }
        assert_eq!(player.experience, 6);
        assert_eq!(player.xp_carry, 0);
    }

    /// Prestige banks the rank and nothing else. The reset is written as a struct
    /// update from [`Player::new`] precisely so this test does not have to be edited
    /// every time a field is added — a new field is reset by default, and only a
    /// deliberate exception would need a line here.
    #[test]
    fn a_prestige_banks_the_rank_and_throws_the_rest_away() {
        let mut player = Player::new();
        player.add_experience(450);
        player.inventory.add(Item::Raw(Material::Diamond), 64);
        player.pickaxe.upgrade().ok();
        player.xp_carry = 700;
        assert!(player.level > 1, "the reset must have something to undo");

        player.prestige_reset();

        assert_eq!(player.prestige, 1);
        assert_eq!(player.level, 1);
        assert_eq!(player.experience, 0);
        assert_eq!(player.xp_carry, 0);
        assert_eq!(player.pickaxe.get_tier(), PickaxeTier::Wooden);
        assert_eq!(player.inventory.count(Item::Raw(Material::Diamond)), 0);
    }

    /// The level going back to 1 *is* the End closing: the unlocked set is a query
    /// over the level, so there is no second place for the reset to forget.
    #[test]
    fn a_prestige_shuts_every_world_the_level_had_opened() {
        let mut player = Player::new();
        player.level = END_UNLOCK_LEVEL;
        assert!(player.has_unlocked(World::End));

        player.prestige_reset();

        assert!(!player.has_unlocked(World::End));
        assert!(!player.has_unlocked(World::Nether));
        assert!(player.has_unlocked(World::Overworld));
    }

    /// Ranks accumulate across prestiges rather than resetting with everything else —
    /// it is the one thing the trade buys.
    #[test]
    fn ranks_accumulate_across_prestiges() {
        let mut player = Player::new();
        for expected in 1..=3 {
            player.prestige_reset();
            assert_eq!(player.get_prestige(), expected);
        }
    }

    /// A player the rules built is valid, at both ends of the ladder.
    #[test]
    fn a_player_the_rules_built_is_valid() {
        let mut player = Player::new();
        assert!(player.validate().is_ok());

        player.grant_break_experience(&[Block::Amethyst; 700]);
        assert!(
            player.validate().is_ok(),
            "a levelled player is a normal player"
        );
    }

    /// The level is the axis every other gate reads, so a level off the ladder is
    /// not one wrong number: level 0 in particular makes `xp_for_level` answer
    /// `None`, which every caller reads as "already at the cap" — the run would stop
    /// levelling for good, silently.
    #[test]
    fn a_level_off_the_ladder_is_refused() {
        let mut player = Player::new();

        player.level = 0;
        assert!(player.validate().is_err());

        player.level = LEVEL_CAP + 1;
        assert!(player.validate().is_err());

        player.level = LEVEL_CAP;
        assert!(player.validate().is_ok(), "the cap itself is a legal level");
    }

    /// Banked experience is spent down as levels are crossed, so a bank at or above
    /// the next level's price is a level-up that was owed and never happened —
    /// nothing re-checks between grants.
    #[test]
    fn banked_experience_past_the_next_level_is_refused() {
        let mut player = Player::new();
        let needed = match player.experience_to_next_level() {
            Some(needed) => needed,
            None => unreachable!("a level-1 player has a next level"),
        };

        player.experience = needed - 1;
        assert!(player.validate().is_ok());

        player.experience = needed;
        assert!(player.validate().is_err());
    }

    /// The carry is the remainder of a division by 1000; a whole point sitting in it
    /// is experience the player earned and will never be paid.
    #[test]
    fn a_carry_holding_a_whole_point_is_refused() {
        let mut player = Player::new();
        player.xp_carry = prestige::PERMILLE;

        assert!(player.validate().is_err());
    }

    /// The caps are the whole of the two-axis progression: an enchant above what its
    /// tier and world allow is an upgrade the player could not have bought.
    #[test]
    fn an_enchant_above_its_cap_is_refused() {
        let mut player = Player::new();
        let mut enchants = Enchants::new();
        for _ in 0..10 {
            let _ = enchants.upgrade(EnchantType::Fortune, PickaxeTier::Netherite, World::End);
        }
        player.pickaxe = Pickaxe::new(PickaxeTier::Netherite, enchants);

        // Legal at the End's ceiling; the same pickaxe on a level-1 player is not,
        // because the worlds their level opens set a lower one.
        assert!(player.validate().is_err());
    }
}
