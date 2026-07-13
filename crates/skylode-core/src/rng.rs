//! The seeded, replayable source of randomness.
//!
//! Every draw in the game — which cell a mixed mine puts an ore in, whether an
//! enchant procs — comes from here, and from nowhere else. The point is not the
//! randomness itself but its *replayability*: a run seeded with the same number
//! produces the same sequence of draws, on any machine, forever. That is what
//! makes `#[test]` balance runs ("N ticks yields this level, this inventory")
//! possible at all, and what keeps a saved run one continuous history rather
//! than a fresh roll of the dice at every load.

use rand::Rng as _;
use rand::distr::Distribution as _;
use rand::distr::weighted::WeightedIndex;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng as _;
use std::num::NonZeroU32;

/// The game's random-number generator: a seeded PRNG whose state is part of the
/// run.
///
/// Wraps [`ChaCha8Rng`] rather than `rand`'s `StdRng`, and the choice is load-
/// bearing: `StdRng` is explicitly *not* guaranteed to keep its algorithm across
/// `rand` releases, while `rand_chacha` guarantees reproducibility. Since the
/// generator's position is destined for the save file, an algorithm that may
/// silently change underneath us would turn every existing save into a different
/// run. This wrapper is also the only place in the core that names a `rand` type,
/// so the dependency cannot leak into the rules.
#[derive(Debug, Clone)]
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "awaiting the phase-2 mine generation")
    )]
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "awaiting the phase-4 enchant procs")
    )]
    pub(crate) fn chance(&mut self, p: f64) -> bool {
        if p.is_nan() {
            return false;
        }
        self.0.random_bool(p.clamp(0.0, 1.0))
    }

    /// Draws an index into `weights`, each index in proportion to its weight.
    ///
    /// This is how a mixed mine decides that a cell is Crying Obsidian rather
    /// than Obsidian. `None` when the weights cannot describe a distribution —
    /// empty, or summing to zero — which is a caller's mistake to fix, not a
    /// value to guess at.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "awaiting the phase-2 mine generation")
    )]
    pub(crate) fn weighted(&mut self, weights: &[u32]) -> Option<usize> {
        WeightedIndex::new(weights)
            .ok()
            .map(|distribution| distribution.sample(&mut self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    /// memory, without serde or a filesystem: phase 9 only has to persist what
    /// this test restores by hand.
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
}
