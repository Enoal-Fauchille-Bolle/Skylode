//! What a prestige rank costs, and what it is worth forever after.
//!
//! Prestige is the endgame loop `docs/MECHANICS.md` adds where SkyMines had paid
//! ranks: the player trades the whole run — pickaxe, enchants, inventory, every
//! mine, and the mining level itself — for a rank that permanently multiplies ore
//! yield and experience gain. This module is the *arithmetic* half of
//! that trade. The trade itself is [`GameState::prestige`], because a reset that
//! touches nine fields of the run belongs to whatever owns the nine.
//!
//! ## Two things it multiplies, and one it deliberately does not
//!
//! **Mining speed was a third, and phase 10 took it out.** The reason is not that it was
//! too strong but that it was strong in the wrong half of the run. Past the point where a
//! pickaxe instamines — a block falls in one tick and no further power buys anything — the
//! speed multiplier is thrown away, so it paid nothing during the endgame it was meant to
//! reward. What it *did* do was compound with the yield and experience multipliers during
//! the **climb**, the stretch a reset player spends walking six pickaxe tiers back up, and
//! three multipliers on one stretch shrank it eleven-fold across ten ranks. The loop's own
//! content was the thing being deleted. Removing the speed term leaves the climb scaling
//! with roughly the square of the multiplier instead of its cube; see
//! [`PRESTIGE_MULT_PER_RANK_PERMILLE`].
//!
//! ## Why the multiplier is an integer
//!
//! Every yield in this game is a whole number: a block drops 1 ore, or 9. Applied
//! block by block, a `×1.2` truncates to `×1.0` and stays there — so the rank-I
//! multiplier would be worth exactly nothing through the entire post-prestige early
//! game, which is the one stretch it exists to shorten. Two devices fix that, and
//! neither is optional:
//!
//! 1. **One multiplication per swing, not per block.** A Nuke that drops two hundred
//!    cells multiplies 200 once instead of multiplying 1 two hundred times.
//! 2. **The fraction is carried.** [`apply_with_carry`] pays the whole part and keeps
//!    the remainder for the next swing, so five swings at rank I on a 1-drop block
//!    pay six ore rather than five.
//!
//! That is the same device — and the same reason — as the auto-miner's microblock
//! carries (see [`GameState`]): a fractional rate truncated on every application is a
//! rate of zero.
//!
//! There is **no exception left**: mining speed was the one path that multiplied an
//! `f32` and so needed no carry, and it no longer takes the multiplier at all. Every
//! consumer now goes through [`multiplier_permille`] and pays a whole number.
//!
//! [`GameState`]: crate::game::GameState
//! [`GameState::prestige`]: crate::game::GameState::prestige

use crate::economy::Cost;
use crate::material::Material;
use crate::pickaxe::PickaxeTier;
use crate::tunables::{
    AMETHYST_PER_CLIMB, LEVEL_CAP, PRESTIGE_MULT_PER_RANK_PERMILLE, PRESTIGE_SURCHARGE_BASE,
    PRESTIGE_SURCHARGE_PER_RANK_PERMILLE,
};

/// The denominator every permille in this module is quoted against.
///
/// Declared here rather than in [`tunables`](crate::tunables), which
/// [`EnchantType::proc_permille`] and [`Rng::chance_permille`] also decline to use for
/// it: a permille *is* a thousandth, so this is a unit and not a dial, and an entry in
/// a module called *tunables* would be an invitation to turn a number that has no
/// other value.
///
/// `pub(crate)` because the auto-miner scales its **rate** rather than its yield and
/// so divides by this itself, in `u64`, without ever building a
/// [`multiplier_permille`] product it could hand to [`apply_with_carry`].
///
/// [`EnchantType::proc_permille`]: crate::enchant::EnchantType
/// [`Rng::chance_permille`]: crate::rng::Rng
pub(crate) const PERMILLE: u32 = 1_000;

/// The permanent global multiplier a player at `rank` mines under, in permille.
///
/// `1000 + PER_RANK × rank`, so **rank 0 returns exactly `PERMILLE`**. That identity
/// is load-bearing rather than incidental, for the reason
/// [`Pickaxe::haste_multiplier`] documents about its own: every number the game is
/// balanced on today — the hardness table, the twelve XP bases, the golden vectors
/// that pin the generator — was measured before a single prestige. A multiplier that
/// missed `1.0` at rank 0 would silently re-tune all of it, and the tests that would
/// catch it are the ones nobody expects a prestige commit to touch.
///
/// Saturating throughout, and it is not doctrine here: the rank is unbounded on
/// purpose — and, since phase 10, unbounded *by design* rather than pending a decision.
/// The price below no longer outgrows the income that pays it, so a player who keeps
/// prestiging past whatever the achievements mark keeps meeting runs of much the same
/// length instead of a wall. This is the arithmetic that has to survive that being true.
///
/// [`Pickaxe::haste_multiplier`]: crate::pickaxe::Pickaxe::haste_multiplier
pub fn multiplier_permille(rank: u32) -> u32 {
    PERMILLE.saturating_add(PRESTIGE_MULT_PER_RANK_PERMILLE.saturating_mul(rank))
}

/// What buying the rank *after* `rank` costs, in Amethyst.
///
/// **A sum, not a curve**, and the two terms answer different questions:
///
/// ```text
/// price(n) = AMETHYST_PER_CLIMB + PRESTIGE_SURCHARGE_BASE × (1 + SURCHARGE_PER_RANK × n)
///            └─ what the climb   └─ what the player must actually go and mine
///               already banked
/// ```
///
/// The first term is [measured](AMETHYST_PER_CLIMB), not chosen: a run banks about five
/// thousand Amethyst while grinding the experience between the End and the level cap,
/// whatever its rank and whatever its strategy. Any price under that figure is **free** —
/// the run reaches the gates already holding it — so quoting a total means quoting a
/// number whose first five thousand do nothing. The second term is the whole of what the
/// design controls, and because the income rate is known (`≈ 2 700 × multiplier` per
/// hour) it converts to minutes directly.
///
/// This replaces a shared geometric [`cost_curve`](crate::economy::cost_curve) that
/// doubled per rank. Doubling against a multiplier that only *adds* per rank is a race an
/// exponential always wins, and the harness measured what winning looked like: the price
/// stayed under the free five thousand and cost nothing at all for six ranks, then passed
/// it and grew to swallow the run, 3.5 h of a 3.5 h rank-10 run spent banking. Both halves
/// of that were the same bug — a price with no fixed relationship to the income paying it.
/// Two linear terms have one, and it is [a comparison of two
/// slopes](PRESTIGE_SURCHARGE_PER_RANK_PERMILLE).
///
/// **Amethyst, and only Amethyst.** `docs/decisions/0066` makes it dual-use on purpose:
/// the same ore pushes the End's enchant cap or buys a rank, which is what turns the
/// End's richness dial into a real three-way decision instead of a slider that is
/// always worth maxing.
///
/// A [`Cost`] rather than a bare number, so the price is quoted and paid in the same
/// denominations as everything else — 6 540 raw reads as `65 Compressed Amethyst +
/// 40 Amethyst`, never as a flat 6 540.
///
/// Far out it **saturates** rather than wrapping, so an absurd rank becomes unbuyable
/// instead of cheap — the same promise the geometric curve made, kept for the same
/// reason, though it now takes a rank in the millions to reach.
pub fn cost(rank: u32) -> Cost {
    Cost::single(Material::Amethyst, amethyst_price(rank))
}

/// The price as a bare raw total, split out so the two saturating steps are readable.
///
/// Private because a raw total is half an answer: every price in the game is quoted in
/// both denominations, and a caller handed the number alone would print `6540` where the
/// till expects `65 Compressed + 40`. [`cost`] is the whole answer.
///
/// `u64` inside for the same reason [`apply_with_carry`] widens: the surcharge product
/// overflows a `u32` well before the rank does, and the crate's lints refuse a debug-build
/// panic where a widening will do.
fn amethyst_price(rank: u32) -> u32 {
    let growth = PERMILLE.saturating_add(PRESTIGE_SURCHARGE_PER_RANK_PERMILLE.saturating_mul(rank));
    let surcharge = u64::from(PRESTIGE_SURCHARGE_BASE) * u64::from(growth) / u64::from(PERMILLE);
    u32::try_from(u64::from(AMETHYST_PER_CLIMB) + surcharge).unwrap_or(u32::MAX)
}

/// Why a run cannot prestige yet — or that it can.
///
/// The condition `docs/decisions/0066` settles is a **fully realised run**, and it is two
/// progression gates: the mining level at [`LEVEL_CAP`] and the pickaxe at
/// [`Netherite`](PickaxeTier::Netherite). Reaching the End (level 30) alone is not
/// enough — that left the shortest path an XP race that never climbed a tier, which the
/// balance harness measured at ~2.6 h; the Netherite gate is what closes it.
///
/// **Efficiency 15 used to be a third gate and no longer is.** It was redundant with
/// the Amethyst price below — which already forces a run to reach *and work* the End —
/// and it was the single source of the mono-mine Obsidian grind that phase 10 exists to
/// flatten. Dropping it leaves Netherite's Efficiency `6..=15` a pure optimisation: the
/// speedrunner skips it and farms Amethyst, the completionist maxes it, and the two
/// finally diverge. See `docs/decisions/0066`.
///
/// A **struct of two [`Option`]s**, the shape [`MineLock`](crate::mine_kind::MineLock)
/// takes for the mine gate and for the same reason: `docs/UI.md` §6.8 prints each
/// unmet requirement as its own line, so the preview needs the *why*, not a bare
/// "not yet". The Amethyst price is deliberately **not** here — it is paid through the
/// till like every other price, and quoted only once these gates are open, because
/// Amethyst only drops in the End and quoting it to a player short of the level
/// answers the wrong question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrestigeLock {
    level: Option<u32>,
    tier: Option<PickaxeTier>,
}

impl PrestigeLock {
    /// Whether every progression gate is open. The Amethyst price is still owed
    /// separately — an open lock is "you may pay", not "you have prestiged".
    pub fn is_open(self) -> bool {
        self.level.is_none() && self.tier.is_none()
    }

    /// The mining level still owed ([`LEVEL_CAP`]), or [`None`] if already there.
    pub fn missing_level(self) -> Option<u32> {
        self.level
    }

    /// The pickaxe tier still owed ([`Netherite`](PickaxeTier::Netherite)), or [`None`].
    pub fn missing_tier(self) -> Option<PickaxeTier> {
        self.tier
    }
}

/// The progression gate on prestige, as a pure function of the two numbers it reads.
///
/// Takes the values rather than a [`Player`](crate::player::Player) — like
/// [`MineKind::lock`](crate::mine_kind::MineKind::lock) — so it is testable without
/// building a run and the preview can ask it about a hypothetical.
pub fn lock(level: u32, tier: PickaxeTier) -> PrestigeLock {
    PrestigeLock {
        level: (level < LEVEL_CAP).then_some(LEVEL_CAP),
        tier: (tier < PickaxeTier::Netherite).then_some(PickaxeTier::Netherite),
    }
}

/// Multiplies `amount` by `permille`, pays the whole part, and keeps the fraction in
/// `carry` for next time.
///
/// **The one implementation of "scale an integer yield without losing the fraction"**,
/// shared by the loot path ([`GameState`]) and the experience path ([`Player`]) so the
/// two cannot drift into two rounding rules. The carry is passed in by `&mut` rather
/// than owned here because it is *run state*: it has to survive being written to the
/// save, and a module of pure functions has nowhere to put it.
///
/// The identity it buys, and the reason it exists at all: over any run of calls, the
/// total paid is within one of `Σ amount × permille / 1000`. Truncating instead would
/// lose up to `999/1000` of a unit **per call**, which for a 1-drop block at rank I is
/// the entire multiplier.
///
/// `u64` inside because `amount × permille` overflows a `u32` at a little over four
/// million — a figure a maxed Nuke on a rich End grid can reach — and the crate's
/// lints refuse a debug-build panic where a widening will do. The result narrows with
/// `try_from(…).unwrap_or(u32::MAX)`, saturating for the same reason
/// [`Player::add_experience`] does: any amount large enough to saturate is far past
/// anything the inventory can hold.
///
/// [`GameState`]: crate::game::GameState
/// [`Player`]: crate::player::Player
/// [`Player::add_experience`]: crate::player::Player
pub(crate) fn apply_with_carry(amount: u32, permille: u32, carry: &mut u32) -> u32 {
    let scaled = u64::from(amount) * u64::from(permille) + u64::from(*carry);
    // The remainder of a division by 1000 is under 1000, so the narrowing is exact and
    // the carry can never grow without bound.
    *carry = (scaled % u64::from(PERMILLE)) as u32;
    u32::try_from(scaled / u64::from(PERMILLE)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Item;

    /// The identity the whole pre-prestige balance rests on. See
    /// [`multiplier_permille`].
    #[test]
    fn rank_zero_multiplies_by_exactly_one() {
        assert_eq!(multiplier_permille(0), PERMILLE);
    }

    #[test]
    fn each_rank_adds_its_share_and_never_compounds() {
        assert_eq!(multiplier_permille(1), 1_100);
        assert_eq!(multiplier_permille(2), 1_200);
        assert_eq!(multiplier_permille(3), 1_300);
    }

    /// `docs/UI.md` §6.8 draws the preview at rank II → III with `×1.20 → ×1.30`. The
    /// mock is the specification here, not an illustration.
    #[test]
    fn the_ui_preview_quotes_the_multipliers_this_module_computes() {
        assert_eq!(multiplier_permille(2), 1_200);
        assert_eq!(multiplier_permille(3), 1_300);
    }

    /// The other half of the same mock: `Cost 6 540 Amethyst` on the rank II → III step,
    /// which is `5 000 + 1 100 × 1.4`.
    #[test]
    fn the_third_rank_costs_what_the_ui_mock_quotes() {
        let lines = cost(2);
        let lines = lines.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].material, Material::Amethyst);
        // 6 540 raw, quoted in the two denominations the till takes.
        assert_eq!(
            lines[0].requirements(),
            vec![
                (Item::Compressed(Material::Amethyst), 65),
                (Item::Raw(Material::Amethyst), 40)
            ]
        );
    }

    /// The shape of the price, stated where it can be read: **a fixed floor plus a
    /// surcharge that grows in a straight line**.
    ///
    /// Rank 0 pays the base surcharge and nothing more, and every rank after adds the
    /// same 220 (`1 100 × 0.2`). The old test in this slot asserted the opposite shape —
    /// that the ladder *doubled* — so leaving it and only changing its numbers would have
    /// preserved a claim the price no longer makes.
    #[test]
    fn a_rank_costs_one_climb_plus_a_surcharge_that_grows_in_a_straight_line() {
        assert_eq!(amethyst_price(0), AMETHYST_PER_CLIMB + 1_100);
        assert_eq!(amethyst_price(1), AMETHYST_PER_CLIMB + 1_320);
        assert_eq!(amethyst_price(2), AMETHYST_PER_CLIMB + 1_540);

        // A straight line is a constant step, which is the whole claim.
        let steps: Vec<u32> = (0..9)
            .map(|rank| amethyst_price(rank + 1) - amethyst_price(rank))
            .collect();
        assert_eq!(steps, vec![220; 9]);
    }

    /// **Every price must clear the floor the climb hands over for free.**
    ///
    /// A rank priced at or below [`AMETHYST_PER_CLIMB`] costs the player no time at all:
    /// they arrive at the gates already holding it and prestige on the spot. That is not
    /// a hypothetical failure mode — it is what the previous geometric curve did for its
    /// first six ranks, and the reason it went unnoticed is that a price *looks* fine
    /// while doing it. The surcharge being strictly positive is what rules it out, at
    /// every rank rather than at the one someone thought to check.
    #[test]
    fn no_rank_is_paid_for_by_the_climb_alone() {
        for rank in 0..64 {
            assert!(
                amethyst_price(rank) > AMETHYST_PER_CLIMB,
                "rank {rank} costs {} — at or under the {AMETHYST_PER_CLIMB} a climb \
                 banks by itself, so it would cost no time to buy",
                amethyst_price(rank)
            );
        }
    }

    /// A rank must never be cheaper than the one before it, over the range a run can
    /// plausibly reach — and far past it, into where the arithmetic saturates.
    #[test]
    fn the_prestige_ladder_only_climbs() {
        for rank in 0..20 {
            assert!(
                amethyst_price(rank + 1) > amethyst_price(rank),
                "rank {rank} is not cheaper than rank {}",
                rank + 1
            );
        }
    }

    /// An absurd rank must become **unbuyable, not cheap**. The price is now linear
    /// rather than doubling, so reaching the saturating end takes a rank no run produces
    /// — which is exactly why it needs asserting rather than assuming: nothing else in
    /// the game will ever walk this far and notice a wrap.
    #[test]
    fn an_absurd_rank_saturates_instead_of_wrapping() {
        assert_eq!(amethyst_price(u32::MAX), u32::MAX);
        assert!(amethyst_price(u32::MAX / 2) > amethyst_price(1_000));
    }

    /// The arithmetic the carry exists for, spelled out: at rank I a 1-drop block pays
    /// 1 nine times and then 2 — eleven ore over ten swings, which is exactly `10 × 1.1`.
    /// Truncating would pay ten and the multiplier would be worth nothing.
    ///
    /// Ten swings and not five because the rank-I multiplier is now `×1.10`: the carry
    /// takes ten swings to fill rather than five, which is precisely the case that makes
    /// truncation worse and the carry more necessary, not less.
    #[test]
    fn a_carried_remainder_pays_the_eleventh_ore() {
        let permille = multiplier_permille(1);
        let mut carry = 0;
        let paid: Vec<u32> = (0..10)
            .map(|_| apply_with_carry(1, permille, &mut carry))
            .collect();

        assert_eq!(paid, vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 2]);
        assert_eq!(paid.iter().sum::<u32>(), 11);
        assert_eq!(carry, 0);
    }

    /// The property the loot path relies on over a long session: nothing is lost, and
    /// the drift from the exact product never exceeds the one unit still in the carry.
    #[test]
    fn a_long_run_of_calls_stays_within_one_of_the_exact_product() {
        let permille = multiplier_permille(3);
        let mut carry = 0;
        let total: u32 = (1..=500)
            .map(|amount| apply_with_carry(amount, permille, &mut carry))
            .sum();

        let exact = (1..=500u64).sum::<u64>() * u64::from(permille) / u64::from(PERMILLE);
        assert_eq!(u64::from(total), exact);
    }

    /// Rank 0 must leave a yield untouched *and* leave the carry at zero — a carry
    /// that crept up at rank 0 would hand out a free ore on some later swing of a run
    /// that never prestiged.
    #[test]
    fn rank_zero_pays_the_yield_back_unchanged() {
        let mut carry = 0;
        for amount in [0, 1, 9, 200] {
            assert_eq!(
                apply_with_carry(amount, multiplier_permille(0), &mut carry),
                amount
            );
            assert_eq!(carry, 0);
        }
    }

    /// A swing big enough to overflow `u32` in the intermediate product must still be
    /// paid, not panic — the `u64` widening is what holds this.
    #[test]
    fn an_enormous_yield_saturates_instead_of_overflowing() {
        let mut carry = 0;
        assert_eq!(
            apply_with_carry(u32::MAX, multiplier_permille(1), &mut carry),
            u32::MAX
        );
    }

    /// The gate opens only for a fully realised run — the level cap and Netherite —
    /// and below either of them names exactly what is owed.
    #[test]
    fn the_gate_opens_only_for_a_fully_realised_run() {
        let open = lock(LEVEL_CAP, PickaxeTier::Netherite);
        assert!(open.is_open());
        assert_eq!(open.missing_level(), None);
        assert_eq!(open.missing_tier(), None);

        let fresh = lock(1, PickaxeTier::Wooden);
        assert!(!fresh.is_open());
        assert_eq!(fresh.missing_level(), Some(LEVEL_CAP));
        assert_eq!(fresh.missing_tier(), Some(PickaxeTier::Netherite));
    }

    /// The level boundary is the cap itself, not one past it: reaching [`LEVEL_CAP`]
    /// clears the gate, and `Diamond` — one tier below the top — is still short.
    #[test]
    fn the_gates_bite_at_their_exact_boundaries() {
        assert_eq!(
            lock(LEVEL_CAP - 1, PickaxeTier::Netherite).missing_level(),
            Some(LEVEL_CAP)
        );
        assert_eq!(
            lock(LEVEL_CAP, PickaxeTier::Netherite).missing_level(),
            None
        );

        // A maxed level on Diamond — one tier below the top — is still short the tier.
        let diamond = lock(LEVEL_CAP, PickaxeTier::Diamond);
        assert_eq!(diamond.missing_tier(), Some(PickaxeTier::Netherite));
    }
}
