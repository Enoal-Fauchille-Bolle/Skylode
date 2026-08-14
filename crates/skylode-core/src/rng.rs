//! The seeded, replayable source of randomness.
//!
//! Every draw in the game — which cell a mixed mine puts an ore in, whether an
//! enchant procs — comes from here, and from nowhere else. The point is not the
//! randomness itself but its *replayability*: a run seeded with the same number
//! produces the same sequence of draws, on any machine, forever. That is what
//! makes `#[test]` balance runs ("N ticks yields this level, this inventory")
//! possible at all, and what keeps a saved run one continuous history rather
//! than a fresh roll of the dice at every load.

// `RngExt` and not `Rng`, since rand 0.10 split the two: `Rng` kept the raw
// generator interface, and `random`, `random_range` and `random_bool` — the three
// this module calls — moved to an extension trait blanket-implemented over it.
// Importing the wrong one still compiles until a call site is reached, so the
// error it produces names a missing *method* rather than a missing trait.
//
// `as _` binds no name, which matters twice here: none of these traits is ever
// written out below, and this module already owns a type called `Rng`.
use rand::RngExt as _;
use rand::distr::Distribution as _;
use rand::distr::weighted::WeightedIndex;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng as _;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// The game's random-number generator: a seeded PRNG whose state is part of the
/// run.
///
/// Wraps [`ChaCha8Rng`] rather than `rand`'s `StdRng`, and the choice is load-
/// bearing: `StdRng` is explicitly *not* guaranteed to keep its algorithm across
/// `rand` releases, while `rand_chacha` guarantees reproducibility.
///
/// What a changed algorithm would cost is worth stating precisely, because it is
/// not corruption. The save stores a *position in a sequence*; it would still parse,
/// still pass its HMAC, still load without a word. But the position would now index
/// a different sequence, and the run would quietly continue on other dice. Nothing
/// breaks loudly — which is the problem. Every balance test pinned to a seed would
/// silently stop meaning what it meant, and "send me your save, I will reproduce
/// your bug" would stop being true. `the_sequence_is_pinned_to_a_golden_vector` is
/// what turns that silence into a failing test.
///
/// This wrapper is also the only place in the core that names a `rand` type, so the
/// dependency cannot leak into the rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rng(ChaCha8Rng);

impl Rng {
    /// Starts a generator on the sequence selected by `seed`.
    ///
    /// The seed is *injected*, never invented here. The core may no more reach
    /// for OS entropy than it may read `SystemTime::now()`: both would make a run
    /// depend on something outside the state, and neither offline replay nor a
    /// balance test could reproduce it. A new game gets its seed from the
    /// front-end, and keeps it for life.
    pub fn from_seed(seed: u64) -> Self {
        Rng(ChaCha8Rng::seed_from_u64(seed))
    }

    /// Draws an integer in `0..n`.
    ///
    /// Takes a [`NonZeroU32`] because `below(0)` has no answer to give: the range
    /// `0..0` is empty. The alternative signatures all end badly — panicking is
    /// denied by the workspace lints, and returning an `Option` would push a
    /// `None` no caller can act on onto every call site. Making the bad argument
    /// unrepresentable is the only shape that owes nobody an apology.
    ///
    /// The mine's target draw is the caller that proves the signature earns its
    /// keep: it hands over `standing.len()`, and an empty mine is turned away by
    /// the `NonZeroU32` constructor before it can ask for a cell that is not there.
    pub(crate) fn below(&mut self, n: NonZeroU32) -> u32 {
        self.0.random_range(0..n.get())
    }

    /// Returns `true` with probability `p`, which is forced into `0.0..=1.0`.
    ///
    /// `random_bool` panics outside that range, and a proc chance that drifts out
    /// of it — past 1.0 through some future multiplier stack, or to `NaN` through
    /// a degenerate one — is a balance bug, not a reason to kill the player's run.
    ///
    /// `NaN` is handled *before* the clamp, because `f64::clamp` does not tame it:
    /// it propagates it. Nothing compares meaningfully to `NaN`, so "bring it back
    /// between 0 and 1" is not an operation that exists. An impossible probability
    /// is the only reading of an unspecified one that cannot hand the player a
    /// windfall.
    ///
    /// The enchant procs were expected to land here, and did not: they are quoted
    /// in **permille** and roll through [`chance_permille`](Rng::chance_permille),
    /// which keeps the comparison in integers. This stays for a caller whose
    /// probability is genuinely a real number — a rate derived from a ratio rather
    /// than named as one — and is the only remaining reason to accept an `f64`.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "no integer-quoted caller; see chance_permille")
    )]
    pub(crate) fn chance(&mut self, p: f64) -> bool {
        if p.is_nan() {
            return false;
        }
        self.0.random_bool(p.clamp(0.0, 1.0))
    }

    /// Returns `true` with probability `permille / 1000`.
    ///
    /// The door every enchant proc goes through, and it is **integer all the way
    /// down**: the proc curves are quoted in permille
    /// ([`EnchantType::proc_permille`]), so rolling them means comparing two `u32`s
    /// rather than converting to an `f64` and asking [`chance`](Rng::chance). That
    /// is not a micro-optimisation — it removes floating-point rounding from the
    /// path that decides a proc, and the proc sequence is state a save resumes.
    /// A run must not depend on how a division rounded.
    ///
    /// Needs no clamping, which is the happy accident of drawing on `0..1000`:
    /// `permille = 0` never fires because nothing is below 0, and any `permille` at
    /// or above 1000 always fires because every draw is under it. The degenerate
    /// ends are the correct ones for free.
    ///
    /// [`EnchantType::proc_permille`]: crate::enchant::EnchantType
    pub(crate) fn chance_permille(&mut self, permille: u32) -> bool {
        /// The denominator every proc chance is quoted against.
        const PER_MILLE: NonZeroU32 = match NonZeroU32::new(1_000) {
            Some(n) => n,
            None => unreachable!(),
        };

        self.below(PER_MILLE) < permille
    }

    /// Draws an index into `weights`, each index in proportion to its weight.
    ///
    /// This is how a mixed mine decides that a cell is Crying Obsidian rather
    /// than Obsidian. `None` when the weights cannot describe a distribution —
    /// empty, or summing to zero — which is a caller's mistake to fix, not a
    /// value to guess at.
    pub(crate) fn weighted(&mut self, weights: &[u32]) -> Option<usize> {
        WeightedIndex::new(weights)
            .ok()
            .map(|distribution| distribution.sample(&mut self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_json;

    /// `below` takes a `NonZeroU32`; the tests do not care to say so ten times.
    fn nz(n: u32) -> NonZeroU32 {
        match NonZeroU32::new(n) {
            Some(n) => n,
            None => unreachable!("the tests never ask for below(0)"),
        }
    }

    fn draws(seed: u64, count: usize) -> Vec<u32> {
        let mut rng = Rng::from_seed(seed);
        (0..count).map(|_| rng.0.random::<u32>()).collect()
    }

    /// The founding property, and the one every other guarantee is built on: a
    /// generator is only worth seeding if the seed fully determines what comes out.
    /// Lose this and balance tests cannot assert anything, and a reloaded run stops
    /// being the same run.
    #[test]
    fn the_same_seed_replays_the_same_sequence() {
        assert_eq!(draws(7, 1000), draws(7, 1000));
    }

    #[test]
    fn different_seeds_draw_different_sequences() {
        assert_ne!(draws(7, 100), draws(8, 100));
    }

    /// The load-bearing test of this module. These numbers pin the *exact*
    /// sequence the generator emits: the algorithm (ChaCha8), the seeding, and
    /// the crate version behind them. Swapping the generator, or flipping a
    /// `rand` feature that alters sampling, breaks this test — and it is right to
    /// break, because the save file stores a position in this sequence. A run
    /// reloaded against a different sequence is a different run.
    ///
    /// If it ever fails, the question is never "what are the new numbers?" but
    /// "what did we just do to every existing save?".
    #[test]
    fn the_sequence_is_pinned_to_a_golden_vector() {
        assert_eq!(
            draws(42, 8),
            vec![
                962_419_617,
                2_928_721_845,
                628_724_104,
                4_081_401_798,
                3_317_060_492,
                1_836_168_968,
                1_477_863_250,
                2_694_492_921,
            ]
        );
    }

    /// Saving and reloading must not disturb the dice. The generator's position
    /// is state, so a run restored mid-sequence has to continue it, not restart
    /// it — otherwise a player could reload to reroll an unlucky proc, and,
    /// worse, a run would stop being one continuous history. Proven here in
    /// memory, without serde or a filesystem — its sibling below proves the save
    /// carries exactly what this test restores by hand.
    #[test]
    fn restoring_the_position_resumes_the_sequence() {
        let mut rng = Rng::from_seed(3);
        for _ in 0..5 {
            let _ = rng.0.random::<u32>();
        }

        let position = rng.0.get_word_pos();
        let expected: Vec<u32> = (0..5).map(|_| rng.0.random::<u32>()).collect();

        let mut reloaded = Rng::from_seed(3);
        reloaded.0.set_word_pos(position);
        let resumed: Vec<u32> = (0..5).map(|_| reloaded.0.random::<u32>()).collect();

        assert_eq!(resumed, expected);
    }

    /// The same promise, made by the save this time: the file carries the
    /// *position*, not just the seed. This is the single test that would fail if
    /// `rand_chacha`'s `serde` feature were ever dropped from the manifest — the
    /// run would silently restart its dice at every load, and nothing else in the
    /// crate would notice.
    #[test]
    fn a_serialised_generator_resumes_the_sequence() {
        let mut rng = Rng::from_seed(3);
        for _ in 0..5 {
            let _ = rng.below(nz(100));
        }

        let written = test_json::write(&rng);
        let expected: Vec<u32> = (0..5).map(|_| rng.below(nz(100))).collect();

        let mut reloaded: Rng = test_json::read(&written);
        let resumed: Vec<u32> = (0..5).map(|_| reloaded.below(nz(100))).collect();

        assert_eq!(resumed, expected);
    }

    /// The seed alone is not the state, and a test that only checked "same seed,
    /// same numbers" would pass on a save that stored nothing but the seed.
    #[test]
    fn a_serialised_generator_is_not_a_fresh_one() {
        let mut rng = Rng::from_seed(3);
        for _ in 0..5 {
            let _ = rng.below(nz(100));
        }

        let written = test_json::write(&rng);
        let mut reloaded: Rng = test_json::read(&written);
        let mut fresh = Rng::from_seed(3);

        assert_ne!(
            (0..5).map(|_| reloaded.below(nz(100))).collect::<Vec<_>>(),
            (0..5).map(|_| fresh.below(nz(100))).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn below_stays_inside_its_bound() {
        let mut rng = Rng::from_seed(11);
        for _ in 0..1000 {
            assert!(rng.below(nz(7)) < 7);
        }
        assert_eq!(rng.below(nz(1)), 0, "0..1 leaves nothing to choose");
    }

    /// A generator that is merely *in range* is not enough: a mine that drew its
    /// cells from a lopsided `below` would quietly concentrate its ore in one
    /// corner. 6000 draws over 6 buckets average 1000; a fair generator stays
    /// well inside a ±25% band, and a badly biased one cannot.
    #[test]
    fn below_spreads_its_draws_evenly() {
        let mut rng = Rng::from_seed(12);
        let mut buckets = [0u32; 6];
        for _ in 0..6000 {
            buckets[rng.below(nz(6)) as usize] += 1;
        }
        for (face, &count) in buckets.iter().enumerate() {
            assert!(
                (750..=1250).contains(&count),
                "face {face} came up {count} times out of 6000"
            );
        }
    }

    #[test]
    fn certain_and_impossible_chances_need_no_luck() {
        let mut rng = Rng::from_seed(13);
        for _ in 0..100 {
            assert!(rng.chance(1.0));
            assert!(!rng.chance(0.0));
        }
    }

    /// Probabilities outside `0.0..=1.0` clamp instead of panicking.
    #[test]
    fn out_of_range_chances_clamp_to_certainty_or_impossibility() {
        let mut rng = Rng::from_seed(14);
        assert!(rng.chance(4.2));
        assert!(!rng.chance(-1.0));
        assert!(
            !rng.chance(f64::NAN),
            "NaN clamps to 0.0, so it never fires"
        );
    }

    /// The mixed-mine case, in miniature: a common block and a rare one. The
    /// weights have to actually shape the draw, or Crying Obsidian would be as
    /// common as Obsidian.
    #[test]
    fn weighted_draws_in_proportion_to_the_weights() {
        let mut rng = Rng::from_seed(15);
        let mut hits = [0u32; 3];
        for _ in 0..10_000 {
            match rng.weighted(&[90, 9, 1]) {
                Some(index) => hits[index] += 1,
                None => unreachable!("these weights describe a distribution"),
            }
        }
        assert!((8500..=9500).contains(&hits[0]), "common: {}", hits[0]);
        assert!((800..=1100).contains(&hits[1]), "uncommon: {}", hits[1]);
        assert!((50..=180).contains(&hits[2]), "rare: {}", hits[2]);
    }

    #[test]
    fn a_zero_weight_never_comes_up() {
        let mut rng = Rng::from_seed(16);
        for _ in 0..1000 {
            assert_eq!(rng.weighted(&[1, 0]), Some(0));
        }
    }

    #[test]
    fn weights_that_describe_no_distribution_draw_nothing() {
        let mut rng = Rng::from_seed(17);
        assert_eq!(rng.weighted(&[]), None, "no candidates to choose between");
        assert_eq!(rng.weighted(&[0, 0]), None, "no candidate can be chosen");
    }

    /// The two ends, which `chance_permille` gets for free by drawing on `0..1000`
    /// rather than by clamping. A 0-permille enchant that fired occasionally would
    /// hand a blast to a player who never bought the level.
    #[test]
    fn a_zero_permille_never_fires_and_a_full_thousand_always_does() {
        let mut rng = Rng::from_seed(18);
        for _ in 0..1000 {
            assert!(!rng.chance_permille(0));
            assert!(rng.chance_permille(1_000));
            assert!(
                rng.chance_permille(5_000),
                "past certainty is still certain"
            );
        }
    }

    /// A proc quoted at 200 permille has to fire about a fifth of the time, or the
    /// balance numbers in `EnchantType::proc_permille` mean nothing. 10 000 draws
    /// at 20% average 2000; a correct implementation stays well inside ±10%.
    #[test]
    fn chance_permille_fires_about_as_often_as_it_promises() {
        let mut rng = Rng::from_seed(19);
        let fired = (0..10_000).filter(|_| rng.chance_permille(200)).count();
        assert!((1800..=2200).contains(&fired), "200 permille fired {fired}");
    }
}
