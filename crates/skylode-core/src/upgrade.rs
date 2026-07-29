//! The pickaxe roadmap: every rung the player will ever climb, and what each costs.
//!
//! [`economy`] prices **one** purchase at a time — the next
//! Efficiency level, or the jump out of this tier. That is the right shape for a
//! till, and the wrong shape for a screen: `docs/UI.md` §5.4 draws all forty-six
//! rungs at once, marks how far the player could climb in one go, and warns about a
//! tier jump *before* it is paid for. This module is the difference between the two.
//!
//! ## Why a module of its own
//!
//! It is the same split [`prestige`](crate::prestige) makes: the arithmetic of a
//! trade lives apart from the state the trade mutates. A roadmap straddles
//! [`pickaxe`](crate::pickaxe) (what a rung is worth) and
//! [`economy`] (what a rung costs), so putting it in either would
//! make that one import the other for a purpose neither has. Nothing here mutates
//! anything or draws from the [`Rng`](crate::rng::Rng): every function is a pure
//! question about a pickaxe and an inventory.
//!
//! ## The ladder is linear, and that is a constraint from the rules
//!
//! [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe) is a **single linear step** —
//! Efficiency up to the tier's cap, then reset to 0 and advance a tier — so **no rung
//! can be skipped**. A screen listing the rungs is therefore a roadmap and not a
//! menu, and "can I afford this rung" is meaningless on its own: the only honest
//! question is *"can I afford every rung from where I stand through this one"*. That
//! is what [`chain_affordability`] answers and what makes the `✓` region a
//! contiguous prefix rather than a scatter.

use crate::economy::{self, Affordability, Cost};
use crate::enchant::EnchantType;
use crate::inventory::Inventory;
use crate::pickaxe::{Pickaxe, PickaxeTier};

/// One step of the pickaxe roadmap: a `(tier, efficiency)` pair and the price of
/// arriving at it.
///
/// **The price is of *arriving*, not of leaving.** Rung `Diamond Eff IV` costs what
/// [`economy::pickaxe_efficiency_cost`] charges to go from III to IV; rung
/// `Netherite Pickaxe` costs what [`economy::pickaxe_tier_cost`] charges to leave
/// Diamond. Quoting it the other way round would put every price one row away from
/// the row the player has their cursor on.
///
/// The whole ladder is a **constant of the game** — both cost functions are pure in
/// the tier and the level, and neither consults an inventory — which is why
/// [`ladder`] takes no arguments and why carrying the [`Cost`] here costs nothing in
/// correctness. It is also what lets a front-end render the forty-six rows from one
/// call rather than one call per row.
#[derive(Debug, Clone, PartialEq)]
pub struct PickaxeRung {
    /// The tier the pickaxe is at on this rung.
    pub tier: PickaxeTier,
    /// The Efficiency level it carries there — `0` on a rung that is a tier jump.
    pub efficiency: u8,
    /// What buying this rung costs, or [`None`] on the very first one.
    ///
    /// [`None`] is not "free": rung zero is a bare Wooden pickaxe, which is where
    /// every run *starts*. It was never bought, so there is no price to state, and a
    /// `Cost` of nothing would read as one that had been paid.
    pub cost: Option<Cost>,
}

impl PickaxeRung {
    /// Whether this rung is a **tier jump** — the purchase that resets Efficiency to
    /// zero in exchange for a stronger base.
    ///
    /// Derived rather than stored, so a rung cannot be built claiming to be a jump
    /// while sitting at Efficiency 4. The two conditions together are exact: an
    /// Efficiency of `0` marks the first rung of a tier, and a price marks it as one
    /// somebody had to buy — which the starting Wooden rung, the only other rung at
    /// Efficiency 0, does not have.
    pub fn is_tier_jump(&self) -> bool {
        self.efficiency == 0 && self.cost.is_some()
    }
}

/// Every rung of the pickaxe roadmap, in climbing order.
///
/// Forty-six of them on today's curve — `5 × (1 + 5)` for the tiers that cap
/// Efficiency at 5, plus `1 + 15` for Netherite — but **that count is written down
/// nowhere**, here least of all: it falls out of walking [`PickaxeTier::next`] and
/// asking each tier its [`efficiency_cap`](PickaxeTier::efficiency_cap). Raise a cap
/// in `pickaxe` and this ladder grows with it instead of contradicting it.
///
/// Rung `0` is the bare Wooden pickaxe a run opens with. Every later rung is one
/// purchase, and they are strictly ordered: index `n` cannot be reached without
/// buying index `n - 1` first.
pub fn ladder() -> Vec<PickaxeRung> {
    let mut rungs = Vec::new();
    // The tier being *left*, which is what `pickaxe_tier_cost` is keyed by — so the
    // walk carries it forward rather than asking a tier for its predecessor, a
    // question `PickaxeTier` deliberately cannot answer.
    let mut leaving: Option<PickaxeTier> = None;
    let mut tier = Some(PickaxeTier::Wooden);

    while let Some(current) = tier {
        rungs.push(PickaxeRung {
            tier: current,
            efficiency: 0,
            cost: leaving.map(economy::pickaxe_tier_cost),
        });
        for level in 1..=current.efficiency_cap() {
            rungs.push(PickaxeRung {
                tier: current,
                efficiency: level,
                // `level - 1`: the price of a step is keyed by the level being left,
                // and this rung is the level being reached.
                cost: Some(economy::pickaxe_efficiency_cost(current, level - 1)),
            });
        }
        leaving = Some(current);
        tier = current.next();
    }

    rungs
}

/// Where `pickaxe` currently stands on `ladder`, as an index into it.
///
/// **Takes the ladder rather than building one**, because every caller already has
/// it: a screen draws the forty-six rows and marks one of them, and
/// [`chain_affordability`] walks the rungs it is about to price. Rebuilding it here
/// would either allocate a second copy per call or force a second implementation of
/// "where does a tier start", free to disagree with the first.
///
/// Falls back to rung `0` for a pickaxe the ladder does not contain. That is
/// unreachable through the rules — [`Pickaxe::upgrade`](Pickaxe) never leaves an
/// Efficiency above its tier's cap — and the fallback exists because this crate
/// leaves no `unwrap` to spend on "cannot happen", and because a front-end whose
/// cursor fell off the ladder should draw the bottom of it rather than panic
/// mid-frame.
pub fn position(ladder: &[PickaxeRung], pickaxe: &Pickaxe) -> usize {
    let tier = pickaxe.get_tier();
    let efficiency = pickaxe.enchants().get_level(EnchantType::Efficiency);
    ladder
        .iter()
        .position(|rung| rung.tier == tier && rung.efficiency == efficiency)
        .unwrap_or(0)
}

/// What it would take to climb from where `pickaxe` stands **through** rung `to`.
///
/// **The chain is simulated, not summed, and that is the whole subtlety of this
/// module.** A [`Cost`] is quoted in two denominations at once (`650` raw becomes
/// `6 Compressed + 50 raw`, see [`CostLine`](crate::economy::CostLine)) and the
/// payment rule is strict: the player must hold that exact shape. Adding two rungs'
/// prices and re-splitting the total is wrong in *both* directions — `30 raw` plus
/// `80 raw` re-splits to `1 Compressed + 10 raw`, refusing a player who could have
/// paid both steps out of loose ore, while a player holding the compressed unit and
/// no loose ore would be told they could afford a chain neither step of which they
/// can pay for. So the rungs are paid, in order, against a **copy** of the
/// inventory, and the first refusal is the chain's verdict.
///
/// That copy is why nothing here can change anything: [`Inventory`] is [`Clone`], the
/// clone is local, and the real one is only ever read.
///
/// Returns [`Affordability::Affordable`] for a `to` at or below the player's current
/// rung — a chain of no purchases is one the player has already made — and for a `to`
/// past the end of the ladder it prices every rung that exists.
pub fn chain_affordability(inventory: &Inventory, pickaxe: &Pickaxe, to: usize) -> Affordability {
    let ladder = ladder();
    let from = position(&ladder, pickaxe);

    let mut purse = inventory.clone();
    for rung in ladder.iter().take(to + 1).skip(from + 1) {
        let Some(cost) = &rung.cost else { continue };
        match economy::affordability(&purse, cost) {
            Affordability::Affordable => {
                // Cannot fail: `affordability` has just said every line is held in
                // the denomination it is owed in, which is exactly what `pay` checks.
                let _ = economy::pay(&mut purse, cost);
            }
            refusal => return refusal,
        }
    }
    Affordability::Affordable
}

/// The furthest rung the player could climb to right now, buying every rung on the
/// way — the index of the last `✓` in `docs/UI.md` §5.4's mark column.
///
/// **The `✓` region is a contiguous prefix by construction, not by luck.** Adding a
/// cost to a chain cannot make it affordable, so once the walk stops it never starts
/// again; a screen that painted the marks from repeated
/// [`chain_affordability`] calls would get the same shape, and this function is that
/// shape stated once. It is also what `M` — *buy max* — spends.
///
/// Equal to the player's current rung when nothing at all is affordable, which is the
/// honest answer: buying to *here* is buying nothing.
pub fn max_affordable(inventory: &Inventory, pickaxe: &Pickaxe) -> usize {
    let ladder = ladder();
    let from = position(&ladder, pickaxe);

    let mut purse = inventory.clone();
    let mut reached = from;
    for (index, rung) in ladder.iter().enumerate().skip(from + 1) {
        let Some(cost) = &rung.cost else { continue };
        if economy::pay(&mut purse, cost).is_err() {
            break;
        }
        reached = index;
    }
    reached
}

/// What climbing to a rung would do to the pickaxe's power — the numbers
/// `docs/UI.md` §5.4's dip box and §6.7's modal are made of.
///
/// **Power, and not the price**: [`chain_affordability`] answers whether the climb is
/// payable, this answers whether it is *worth it*. The two are asked about the same
/// rung and kept apart because a player can afford a purchase that makes them slower,
/// which is exactly the case the dip modal exists for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpgradePreview {
    /// The pickaxe's power as it stands.
    pub power_before: f32,
    /// Its power once the chain is bought.
    pub power_after: f32,
    /// The first rung past the target where the power is back at or above
    /// [`power_before`](UpgradePreview::power_before), or [`None`] when nothing was
    /// lost.
    ///
    /// This is `docs/UI.md` §6.7's *"You get it back at Netherite Efficiency V"* —
    /// the sentence that turns a loss into a plan. It is [`None`] whenever
    /// [`is_dip`](UpgradePreview::is_dip) is false, because there is nothing to get
    /// back.
    pub repaid_at: Option<usize>,
    /// Whether the chain crosses a tier jump — which is what the pane labels
    /// `tier jump`, and the only way a climb can cost power at all.
    pub crosses_tier_jump: bool,
}

impl UpgradePreview {
    /// Whether this climb ends **below** the power it started at.
    ///
    /// **The single definition of "a dip", read by the screen and by the modal.** The
    /// modal fires on a net regression and never on an ordinary Efficiency step
    /// (`docs/UI.md` §6.7): a confirmation on every purchase is one nobody reads. A
    /// front-end asking the question itself would be a second definition, free to
    /// disagree about the boundary case — a chain that crosses a jump and climbs all
    /// the way *back* is not a dip, however alarming its middle looks.
    pub fn is_dip(&self) -> bool {
        self.power_after < self.power_before
    }
}

/// Previews the climb from where `pickaxe` stands through rung `to`.
///
/// `to` is clamped into the ladder at both ends: past the top it previews the top,
/// and below the player's own rung it previews standing still. Neither is a caller
/// error worth a [`Result`] — a cursor is a front-end's business and the honest
/// answer to "what would buying nothing do" is *nothing*.
///
/// Draws nothing, spends nothing, and takes no inventory: this question has the same
/// answer for a player who can afford the climb and one who cannot, which is what
/// makes previewing free on any rung (`docs/UI.md` §5.4).
pub fn preview(pickaxe: &Pickaxe, to: usize) -> UpgradePreview {
    let ladder = ladder();
    let from = position(&ladder, pickaxe);
    let power_before = pickaxe.mining_power();

    // `saturating_sub` rather than `len() - 1`: an empty ladder is impossible today
    // and this crate leaves no panic to spend on proving it.
    let target = to.clamp(from, ladder.len().saturating_sub(1));
    let power_at = |rung: &PickaxeRung| pickaxe.power_with(rung.tier, rung.efficiency);
    let power_after = ladder.get(target).map_or(power_before, power_at);

    let crosses_tier_jump = ladder
        .get(from + 1..=target)
        .is_some_and(|climbed| climbed.iter().any(PickaxeRung::is_tier_jump));

    // Only a loss has something to be repaid, so the search is skipped entirely when
    // the climb gained power — which is also what keeps `repaid_at` and `is_dip` from
    // ever disagreeing.
    let repaid_at = (power_after < power_before).then(|| {
        ladder
            .iter()
            .enumerate()
            .skip(target + 1)
            .find(|(_, rung)| power_at(rung) >= power_before)
            .map(|(index, _)| index)
    });

    UpgradePreview {
        power_before,
        power_after,
        repaid_at: repaid_at.flatten(),
        crosses_tier_jump,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::economy::Shortfall;
    use crate::enchant::Enchants;
    use crate::material::{Item, Material};

    /// A pickaxe at an arbitrary rung, built the way the rules would build it.
    ///
    /// Goes through [`Enchants::upgrade_efficiency`] rather than assembling the level
    /// by hand, so no test here measures a pickaxe no player could hold.
    fn at(tier: PickaxeTier, efficiency: u8) -> Pickaxe {
        let mut enchants = Enchants::new();
        for _ in 0..efficiency {
            assert!(enchants.upgrade_efficiency(tier).is_ok());
        }
        Pickaxe::new(tier, enchants)
    }

    /// An inventory holding `amount` of **both** denominations of every material.
    ///
    /// Both, and that is not padding: every rung of the ladder costs at least
    /// [`COST_BASE`](crate::tunables::COST_BASE) = 100 raw, so every price on it
    /// quotes at least one Compressed unit. A purse of loose ore alone — however
    /// large — is refused at the very first step, which makes it useless for the
    /// tests below that are about *wealth* rather than shape.
    fn purse(amount: u32) -> Inventory {
        let mut inventory = Inventory::new();
        for material in Material::ALL {
            inventory.add(Item::Raw(material), amount);
            inventory.add(Item::Compressed(material), amount);
        }
        inventory
    }

    #[test]
    fn the_ladder_starts_bare_and_ends_maxed() {
        let ladder = ladder();

        let Some(first) = ladder.first() else {
            unreachable!("the ladder is never empty")
        };
        assert_eq!(first.tier, PickaxeTier::Wooden);
        assert_eq!(first.efficiency, 0);
        assert_eq!(
            first.cost, None,
            "a run does not buy the pickaxe it starts with"
        );

        let Some(last) = ladder.last() else {
            unreachable!("the ladder is never empty")
        };
        assert_eq!(last.tier, PickaxeTier::Netherite);
        assert_eq!(last.efficiency, PickaxeTier::Netherite.efficiency_cap());
    }

    /// The count is 46 today, and this test is not here to defend the number — it is
    /// here to say the number comes from the tier table. If a cap moves, the
    /// expectation below moves with it and the assertion still holds; only a ladder
    /// that stopped following [`PickaxeTier::next`] would fail.
    #[test]
    fn the_ladder_holds_every_tier_and_every_level_of_each() {
        let mut expected = 0;
        let mut tier = Some(PickaxeTier::Wooden);
        while let Some(current) = tier {
            expected += 1 + usize::from(current.efficiency_cap());
            tier = current.next();
        }

        assert_eq!(ladder().len(), expected);
        assert_eq!(
            expected, 46,
            "the tier table moved; this is a heads-up, not a bug"
        );
    }

    /// **No rung may be skipped**, so the ladder must never step two levels at once
    /// or leave a tier before its cap. This is the property the whole roadmap rests
    /// on: it is what makes "buy the chain to here" a well-defined act.
    #[test]
    fn each_rung_is_exactly_one_purchase_past_the_last() {
        let ladder = ladder();
        for pair in ladder.windows(2) {
            let [previous, rung] = pair else { continue };
            if rung.tier == previous.tier {
                assert_eq!(
                    rung.efficiency,
                    previous.efficiency + 1,
                    "a gap inside {:?}",
                    rung.tier
                );
            } else {
                assert_eq!(
                    previous.efficiency,
                    previous.tier.efficiency_cap(),
                    "{:?} was left before its Efficiency was maxed",
                    previous.tier
                );
                assert_eq!(rung.efficiency, 0, "a tier jump kept its Efficiency");
                assert_eq!(previous.tier.next(), Some(rung.tier), "a tier was skipped");
            }
        }
    }

    /// A jump is the only rung that is both priced and at Efficiency 0 — which is
    /// what [`PickaxeRung::is_tier_jump`] reads, so this pins the derivation against
    /// the ladder's actual shape rather than against itself.
    #[test]
    fn the_tier_jumps_are_the_priced_rungs_at_efficiency_zero() {
        let jumps: Vec<PickaxeTier> = ladder()
            .iter()
            .filter(|rung| rung.is_tier_jump())
            .map(|rung| rung.tier)
            .collect();

        assert_eq!(
            jumps,
            vec![
                PickaxeTier::Stone,
                PickaxeTier::Iron,
                PickaxeTier::Gold,
                PickaxeTier::Diamond,
                PickaxeTier::Netherite,
            ],
            "Wooden is never jumped *to*, and every other tier is"
        );
    }

    #[test]
    fn a_fresh_pickaxe_stands_on_the_first_rung() {
        assert_eq!(position(&ladder(), &Pickaxe::default()), 0);
    }

    /// Position is read off the ladder, so it has to agree with the ladder's own
    /// contents at every rung — walked exhaustively, since an off-by-one here would
    /// mark the wrong row `●` and price the chain from the wrong place.
    #[test]
    fn every_rung_reports_the_index_it_sits_at() {
        let ladder = ladder();
        for (index, rung) in ladder.iter().enumerate() {
            let pickaxe = at(rung.tier, rung.efficiency);
            assert_eq!(
                position(&ladder, &pickaxe),
                index,
                "{:?} Eff {} reported the wrong rung",
                rung.tier,
                rung.efficiency
            );
        }
    }

    #[test]
    fn a_chain_of_no_purchases_is_always_affordable() {
        let broke = Inventory::new();
        let pickaxe = at(PickaxeTier::Iron, 3);
        let here = position(&ladder(), &pickaxe);

        assert_eq!(
            chain_affordability(&broke, &pickaxe, here),
            Affordability::Affordable,
            "standing still costs nothing"
        );
    }

    /// The first rung of the game against an empty inventory: the ore is not there,
    /// so the refusal is the one that sends the player mining rather than the one
    /// that sends them to the Inventory screen.
    #[test]
    fn an_empty_inventory_cannot_afford_the_first_step() {
        let verdict = chain_affordability(&Inventory::new(), &Pickaxe::default(), 1);
        assert!(
            matches!(verdict, Affordability::Insufficient(_)),
            "expected Insufficient, got {verdict:?}"
        );
    }

    /// **The whole reason the chain is simulated instead of summed, as one
    /// assertion**: the same purse, the same rungs, two different verdicts.
    ///
    /// The Wooden tier's four Efficiency steps are quoted with loose remainders of
    /// 45, 10, 5 and 42 raw. Paid in order they ask for 102 loose Stone and ten
    /// Compressed units. Added up first, the total re-splits into **eleven**
    /// Compressed and two loose — so a player holding exactly what the chain needs is
    /// told to go and compress, for a unit no step of the chain ever asked for.
    ///
    /// The purse is built from the ladder's own numbers rather than written out, so
    /// phase-10 rebalancing keeps the test meaningful; the one thing it needs is that
    /// the remainders still overflow a hundred, which is asserted rather than assumed.
    #[test]
    fn a_chain_payable_step_by_step_is_refused_by_the_sum_of_its_prices() {
        let ladder = ladder();
        // Wooden Eff I is where the player stands, Wooden Eff V is the target.
        let (from, to) = (1, 5);
        let steps: Vec<&Cost> = ladder[from + 1..=to]
            .iter()
            .filter_map(|rung| rung.cost.as_ref())
            .collect();

        let (mut compressed, mut raw) = (0, 0);
        for line in steps.iter().copied().flat_map(Cost::lines) {
            assert_eq!(
                line.material,
                Material::Stone,
                "the Wooden tier is paid in Stone"
            );
            compressed += line.compressed;
            raw += line.raw;
        }
        assert!(
            raw >= 100,
            "the loose remainders no longer overflow a hundred ({raw}), so summing \
             and simulating would agree and this test would prove nothing"
        );

        let mut inventory = Inventory::new();
        inventory.add(Item::Compressed(Material::Stone), compressed);
        inventory.add(Item::Raw(Material::Stone), raw);

        // What the naive implementation would ask: one price, re-split from the total.
        let summed = Cost::single(Material::Stone, compressed * 100 + raw);
        assert!(
            matches!(
                economy::affordability(&inventory, &summed),
                Affordability::CompressFirst(_)
            ),
            "the summed price was expected to demand a denomination no step asks for"
        );

        assert_eq!(
            chain_affordability(&inventory, &at(PickaxeTier::Wooden, 1), to),
            Affordability::Affordable,
            "a chain payable step by step was refused"
        );
    }

    /// Holding the *value* but not the *shape* is the third state, and it must
    /// survive being asked about a chain: the fix is a conversion, not a mine.
    ///
    /// **The very first upgrade in the game is an instance of it.** Efficiency I costs
    /// `COST_BASE` = 100 raw Stone exactly, which quotes as *one Compressed unit and
    /// no loose ore* — so a player who has mined five hundred Stone and never opened
    /// the Inventory screen is refused, and the refusal is the one that teaches them
    /// what compression is for. That is the walk `docs/UI.md` §8.4 designs, arriving
    /// earlier than the frame's Netherite example suggests.
    #[test]
    fn a_purse_of_the_wrong_denomination_asks_to_be_converted() {
        let mut inventory = Inventory::new();
        // Five times the price, and not one unit of it in the denomination owed.
        inventory.add(Item::Raw(Material::Stone), 500);

        let verdict = chain_affordability(&inventory, &Pickaxe::default(), 1);
        assert!(
            matches!(verdict, Affordability::CompressFirst(_)),
            "expected CompressFirst, got {verdict:?}"
        );
        // And it names what to convert, which is what the toast prints.
        if let Affordability::CompressFirst(shortfalls) = verdict {
            assert!(
                shortfalls
                    .iter()
                    .all(|Shortfall { needed, held, .. }| held < needed),
                "a shortfall reported nothing missing"
            );
        }
    }

    /// **The prefix property.** `max_affordable` and `chain_affordability` are two
    /// readings of one walk, so every rung up to the maximum must price as affordable
    /// and every rung past it must not — a `✓` after a `✗` is the bug this whole
    /// design exists to make impossible.
    #[test]
    fn the_affordable_region_is_a_contiguous_prefix() {
        let pickaxe = Pickaxe::default();
        let ladder = ladder();
        let here = position(&ladder, &pickaxe);

        // A purse rich enough to climb a few rungs and poor enough to be stopped —
        // the only setting where the boundary is observable at all.
        let inventory = purse(400);
        let furthest = max_affordable(&inventory, &pickaxe);
        assert!(
            furthest > here && furthest < ladder.len() - 1,
            "this purse must stop somewhere in the middle, not at {furthest}"
        );

        for rung in here..=furthest {
            assert_eq!(
                chain_affordability(&inventory, &pickaxe, rung),
                Affordability::Affordable,
                "rung {rung} is inside the prefix and was refused"
            );
        }
        assert_ne!(
            chain_affordability(&inventory, &pickaxe, furthest + 1),
            Affordability::Affordable,
            "rung {} is past the prefix and was allowed",
            furthest + 1
        );
    }

    #[test]
    fn an_empty_purse_buys_max_nothing() {
        let pickaxe = at(PickaxeTier::Iron, 2);
        let here = position(&ladder(), &pickaxe);

        assert_eq!(max_affordable(&Inventory::new(), &pickaxe), here);
    }

    /// A purse that can pay for everything reaches the top rung — the other end of
    /// the range, and the one that proves the walk does not stop early on a rung it
    /// simply failed to price.
    /// The rung index of a `(tier, efficiency)` pair, for tests that name a rung the
    /// way the frames do rather than by counting.
    fn rung_of(tier: PickaxeTier, efficiency: u8) -> usize {
        position(&ladder(), &at(tier, efficiency))
    }

    /// An ordinary Efficiency step gains power and owes nothing — so the modal must
    /// not fire, which is the whole reason `is_dip` is a question and not a constant.
    #[test]
    fn an_efficiency_step_gains_power_and_is_not_a_dip() {
        let pickaxe = at(PickaxeTier::Diamond, 3);
        let preview = preview(&pickaxe, rung_of(PickaxeTier::Diamond, 4));

        assert!(preview.power_after > preview.power_before);
        assert!(!preview.is_dip());
        assert_eq!(preview.repaid_at, None, "nothing was lost to get back");
        assert!(!preview.crosses_tier_jump);
    }

    /// **The frame's own numbers** (`docs/UI.md` §6.7): a Diamond pickaxe at
    /// Efficiency V is worth 34.0, and buying the jump to Netherite drops it to 9.0.
    ///
    /// This is the case the dip modal was drawn for, and pinning it here is what
    /// stops the spec and the rules drifting apart — the frame quotes two numbers, and
    /// they have to be the two numbers the core computes.
    #[test]
    fn the_jump_to_netherite_costs_a_maxed_diamond_pickaxe_its_power() {
        let pickaxe = at(PickaxeTier::Diamond, 5);
        let preview = preview(&pickaxe, rung_of(PickaxeTier::Netherite, 0));

        assert_eq!(preview.power_before, 34.0, "8 (Diamond) + 5² + 1");
        assert_eq!(
            preview.power_after, 9.0,
            "Netherite's base, Efficiency reset"
        );
        assert!(preview.is_dip());
        assert!(preview.crosses_tier_jump);
    }

    /// The other half of §6.7: the dip names the rung that pays it back, and that
    /// rung really is the first one at or above the power that was lost.
    #[test]
    fn a_dip_names_the_first_rung_that_earns_the_power_back() {
        let pickaxe = at(PickaxeTier::Diamond, 5);
        let ladder = ladder();
        let preview = preview(&pickaxe, rung_of(PickaxeTier::Netherite, 0));

        let Some(repaid) = preview.repaid_at else {
            unreachable!("a dip always has a rung that repays it")
        };
        let Some(rung) = ladder.get(repaid) else {
            unreachable!("the repaying rung is on the ladder")
        };
        assert!(
            pickaxe.power_with(rung.tier, rung.efficiency) >= preview.power_before,
            "the named rung does not reach the power it promises to restore"
        );
        let Some(before) = ladder.get(repaid - 1) else {
            unreachable!("the repaying rung is never the first")
        };
        assert!(
            pickaxe.power_with(before.tier, before.efficiency) < preview.power_before,
            "an earlier rung already repaid it, so this one is not the first"
        );
    }

    /// A chain that crosses a jump and keeps climbing past it is **not** a dip. This
    /// is the boundary the front-end must not re-derive: it crosses a tier jump, the
    /// pane will label it one, and the modal must still stay shut.
    #[test]
    fn a_chain_that_climbs_back_past_the_jump_is_not_a_dip() {
        let pickaxe = at(PickaxeTier::Diamond, 5);
        let Some(repaid) = preview(&pickaxe, rung_of(PickaxeTier::Netherite, 0)).repaid_at else {
            unreachable!("a dip always has a rung that repays it")
        };

        let preview = preview(&pickaxe, repaid);
        assert!(preview.crosses_tier_jump, "the chain does cross the jump");
        assert!(!preview.is_dip(), "but it ends level or better");
        assert_eq!(preview.repaid_at, None);
    }

    /// Both clamps, and the reason neither is a refusal: a cursor is the front-end's
    /// business, and the honest preview of buying nothing is one that changes nothing.
    #[test]
    fn previewing_off_either_end_of_the_ladder_previews_standing_still_or_the_top() {
        let pickaxe = at(PickaxeTier::Iron, 3);
        let here = rung_of(PickaxeTier::Iron, 3);

        let backwards = preview(&pickaxe, 0);
        assert_eq!(backwards.power_before, backwards.power_after);
        assert!(!backwards.is_dip());

        let past_the_end = preview(&pickaxe, usize::MAX);
        assert_eq!(
            past_the_end,
            preview(&pickaxe, ladder().len() - 1),
            "previewing past the top must preview the top"
        );
        assert!(past_the_end.power_after > past_the_end.power_before);
        assert!(here > 0, "this pickaxe is not on the first rung");
    }

    /// A preview asks about power alone, so it must answer the same for a pauper and
    /// a millionaire — which is what makes it free on any rung, affordable or not.
    #[test]
    fn a_preview_is_the_same_whatever_the_player_can_pay() {
        let pickaxe = at(PickaxeTier::Gold, 2);
        let target = rung_of(
            PickaxeTier::Netherite,
            PickaxeTier::Netherite.efficiency_cap(),
        );

        let broke = Inventory::new();
        assert_ne!(
            chain_affordability(&broke, &pickaxe, target),
            Affordability::Affordable,
            "this chain is meant to be out of reach"
        );
        // The preview does not take an inventory at all, which is the strongest form
        // of this guarantee: there is no argument through which wealth could leak in.
        assert_eq!(preview(&pickaxe, target).power_after, 235.0);
    }

    #[test]
    fn a_bottomless_purse_reaches_the_top_of_the_ladder() {
        // Large, but nowhere near `u32::MAX`: a holding is *counted* in raw when
        // wealth is checked, so a hundred million Compressed units would overflow the
        // very arithmetic the test is trying to satisfy.
        let inventory = purse(10_000_000);
        let furthest = max_affordable(&inventory, &Pickaxe::default());

        assert_eq!(furthest, ladder().len() - 1);
    }
}
