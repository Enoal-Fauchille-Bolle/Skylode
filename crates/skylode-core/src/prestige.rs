//! What a prestige rank costs, and what it is worth forever after.
//!
//! Prestige is the endgame loop `docs/MECHANICS.md` adds where SkyMines had paid
//! ranks: the player trades the whole run — pickaxe, enchants, inventory, every
//! mine, and the mining level itself — for a rank that permanently multiplies ore
//! yield, mining speed and experience gain. This module is the *arithmetic* half of
//! that trade. The trade itself is [`GameState::prestige`], because a reset that
//! touches nine fields of the run belongs to whatever owns the nine.
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
//! **The one exception is mining speed**, which multiplies a power that is already an
//! `f32` ([`multiplier`]). Nothing is truncated there, so nothing needs carrying.
//!
//! [`GameState`]: crate::game::GameState
//! [`GameState::prestige`]: crate::game::GameState::prestige

use crate::economy::{Cost, cost_curve};
use crate::material::Material;
use crate::pickaxe::PickaxeTier;
use crate::tunables::{
    LEVEL_CAP, PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, PRESTIGE_MULT_PER_RANK_PERMILLE,
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
/// purpose (whether prestige is an endless loop or leads to a win condition is still
/// open in `docs/ROADMAP.md`), so this is the arithmetic that has to survive the
/// answer being *endless*.
///
/// [`Pickaxe::haste_multiplier`]: crate::pickaxe::Pickaxe::haste_multiplier
pub fn multiplier_permille(rank: u32) -> u32 {
    PERMILLE.saturating_add(PRESTIGE_MULT_PER_RANK_PERMILLE.saturating_mul(rank))
}

/// The same multiplier as an `f32`, for the one consumer that wants one.
///
/// Mining speed multiplies [`Pickaxe::mining_power`], which is already an `f32` and is
/// already multiplied by the boost — so folding the rank in there is exact and needs
/// no carry. Everywhere else the yield is a whole number and
/// [`multiplier_permille`] is what to reach for.
///
/// Exactly `1.0` at rank 0, since `1000 / 1000` is representable without loss.
///
/// [`Pickaxe::mining_power`]: crate::pickaxe::Pickaxe::mining_power
pub fn multiplier(rank: u32) -> f32 {
    multiplier_permille(rank) as f32 / PERMILLE as f32
}

/// What buying the rank *after* `rank` costs, in Amethyst.
///
/// Reads off the shared geometric [`cost_curve`] like every other price in the game,
/// with the steepest slope of any track — a rank is priced against a whole run rather
/// than against one upgrade's production gain (see [`PRESTIGE_COST_GROWTH`]).
///
/// **Amethyst, and only Amethyst.** `docs/DECISIONS.md` makes it dual-use on purpose:
/// the same ore pushes the End's enchant cap or buys a rank, which is what turns the
/// End's richness dial into a real three-way decision instead of a slider that is
/// always worth maxing.
///
/// A [`Cost`] rather than a bare number, so the price is quoted and paid in the same
/// denominations as everything else — 512 raw reads as `5 Compressed Amethyst +
/// 12 Amethyst`, never as a flat 512.
///
/// Far out the curve **saturates** rather than wrapping ([`cost_curve`] explains the
/// cast), so an absurd rank becomes unbuyable instead of cheap. That is the whole of
/// this module's answer to the unbounded rank: no cap is written anywhere, and none is
/// needed for the arithmetic to stay honest.
pub fn cost(rank: u32) -> Cost {
    Cost::single(
        Material::Amethyst,
        cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, rank),
    )
}

/// Why a run cannot prestige yet — or that it can.
///
/// The condition `docs/DECISIONS.md` settles is a **fully realised run**: the mining
/// level at [`LEVEL_CAP`], the pickaxe at [`Netherite`](PickaxeTier::Netherite), and
/// its Efficiency at that tier's cap. Reaching the End (level 30) is no longer enough
/// — it left the game's shortest path to prestige an XP race that never climbed a
/// tier, which the balance harness measured at ~2.6 h. See `docs/DECISIONS.md`.
///
/// A **struct of three [`Option`]s**, the shape [`MineLock`](crate::mine_kind::MineLock)
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
    efficiency: Option<u8>,
}

impl PrestigeLock {
    /// Whether every progression gate is open. The Amethyst price is still owed
    /// separately — an open lock is "you may pay", not "you have prestiged".
    pub fn is_open(self) -> bool {
        self.level.is_none() && self.tier.is_none() && self.efficiency.is_none()
    }

    /// The mining level still owed ([`LEVEL_CAP`]), or [`None`] if already there.
    pub fn missing_level(self) -> Option<u32> {
        self.level
    }

    /// The pickaxe tier still owed ([`Netherite`](PickaxeTier::Netherite)), or [`None`].
    pub fn missing_tier(self) -> Option<PickaxeTier> {
        self.tier
    }

    /// The Efficiency level still owed (Netherite's cap), or [`None`].
    pub fn missing_efficiency(self) -> Option<u8> {
        self.efficiency
    }
}

/// The progression gate on prestige, as a pure function of the three numbers it reads.
///
/// Takes the values rather than a [`Player`](crate::player::Player) — like
/// [`MineKind::lock`](crate::mine_kind::MineKind::lock) — so it is testable without
/// building a run and the preview can ask it about a hypothetical. Efficiency is
/// measured against **Netherite's** cap always: a player below Netherite is capped at
/// 5 and so is reported short, which is honest — they owe the tier *and* the levels it
/// unlocks.
pub fn lock(level: u32, tier: PickaxeTier, efficiency: u8) -> PrestigeLock {
    let efficiency_cap = PickaxeTier::Netherite.efficiency_cap();
    PrestigeLock {
        level: (level < LEVEL_CAP).then_some(LEVEL_CAP),
        tier: (tier < PickaxeTier::Netherite).then_some(PickaxeTier::Netherite),
        efficiency: (efficiency < efficiency_cap).then_some(efficiency_cap),
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
        assert_eq!(multiplier(0), 1.0);
    }

    #[test]
    fn each_rank_adds_its_share_and_never_compounds() {
        assert_eq!(multiplier_permille(1), 1_200);
        assert_eq!(multiplier_permille(2), 1_400);
        assert_eq!(multiplier_permille(3), 1_600);
    }

    /// `docs/UI.md` §6.8 draws the preview at rank II → III with `×1.40 → ×1.60`. The
    /// mock is the specification here, not an illustration.
    #[test]
    fn the_ui_preview_quotes_the_multipliers_this_module_computes() {
        assert_eq!(multiplier(2), 1.4);
        assert_eq!(multiplier(3), 1.6);
    }

    /// The other half of the same mock: `Cost 512 Amethyst` on the rank II → III step.
    #[test]
    fn the_third_rank_costs_what_the_ui_mock_quotes() {
        let lines = cost(2);
        let lines = lines.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].material, Material::Amethyst);
        // 512 raw, quoted in the two denominations the till takes.
        assert_eq!(
            lines[0].requirements(),
            vec![
                (Item::Compressed(Material::Amethyst), 5),
                (Item::Raw(Material::Amethyst), 12)
            ]
        );
    }

    #[test]
    fn the_ladder_doubles_from_its_base() {
        assert_eq!(cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, 0), 128);
        assert_eq!(cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, 1), 256);
        assert_eq!(cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, 2), 512);
    }

    /// A rank must never be cheaper than the one before it, over the range a run can
    /// plausibly reach before the curve saturates.
    #[test]
    fn the_prestige_ladder_only_climbs() {
        for rank in 0..20 {
            let (here, next) = (
                cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, rank),
                cost_curve(PRESTIGE_COST_BASE, PRESTIGE_COST_GROWTH, rank + 1),
            );
            assert!(
                next > here,
                "rank {rank} is not cheaper than rank {}",
                rank + 1
            );
        }
    }

    /// The arithmetic the carry exists for, spelled out: at rank I a 1-drop block pays
    /// 1, 1, 1, 1, 2 — six ore over five swings, which is exactly `5 × 1.2`. Truncating
    /// would pay five and the multiplier would be worth nothing.
    #[test]
    fn a_carried_remainder_pays_the_sixth_ore() {
        let permille = multiplier_permille(1);
        let mut carry = 0;
        let paid: Vec<u32> = (0..5)
            .map(|_| apply_with_carry(1, permille, &mut carry))
            .collect();

        assert_eq!(paid, vec![1, 1, 1, 1, 2]);
        assert_eq!(paid.iter().sum::<u32>(), 6);
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

    /// The gate opens only for a fully realised run — the level cap, Netherite, its
    /// Efficiency cap — and below any of them names exactly what is owed.
    #[test]
    fn the_gate_opens_only_for_a_fully_realised_run() {
        let cap = PickaxeTier::Netherite.efficiency_cap();

        let open = lock(LEVEL_CAP, PickaxeTier::Netherite, cap);
        assert!(open.is_open());
        assert_eq!(open.missing_level(), None);
        assert_eq!(open.missing_tier(), None);
        assert_eq!(open.missing_efficiency(), None);

        let fresh = lock(1, PickaxeTier::Wooden, 0);
        assert!(!fresh.is_open());
        assert_eq!(fresh.missing_level(), Some(LEVEL_CAP));
        assert_eq!(fresh.missing_tier(), Some(PickaxeTier::Netherite));
        assert_eq!(fresh.missing_efficiency(), Some(cap));
    }

    /// The level boundary is the cap itself, not one past it: reaching [`LEVEL_CAP`]
    /// clears the gate, and `Diamond` — one tier below the top — is still short.
    #[test]
    fn the_gates_bite_at_their_exact_boundaries() {
        let cap = PickaxeTier::Netherite.efficiency_cap();

        assert_eq!(
            lock(LEVEL_CAP - 1, PickaxeTier::Netherite, cap).missing_level(),
            Some(LEVEL_CAP)
        );
        assert_eq!(
            lock(LEVEL_CAP, PickaxeTier::Netherite, cap).missing_level(),
            None
        );

        // Diamond's own Efficiency cap (5) is below Netherite's (15), so a maxed
        // Diamond is still short the tier *and* the levels it unlocks.
        let diamond = lock(LEVEL_CAP, PickaxeTier::Diamond, 5);
        assert_eq!(diamond.missing_tier(), Some(PickaxeTier::Netherite));
        assert_eq!(diamond.missing_efficiency(), Some(cap));
    }
}
