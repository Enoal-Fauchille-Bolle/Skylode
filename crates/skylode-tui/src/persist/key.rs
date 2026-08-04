//! The signing key, and the two steps taken to hide it.
//!
//! **The ceiling is structural and comes first.** The game runs on the player's
//! machine, so the binary necessarily contains everything needed to produce a valid
//! save. What `docs/DECISIONS.md` chooses is therefore an *effort level*, not a
//! guarantee: hiding moves the attack from one command needing no skill to reading
//! the program itself, which is where the trade's own rule of thumb puts save editing
//! into the *not worth it* basket for most players. It costs no dependency and
//! almost nothing at run time. Build-time injection via `env!` was considered and
//! rejected there — it keeps the key out of the repository but not out of the binary,
//! which is what the player receives, and it splits the key between debug and release
//! so a run played during development would not load in the shipped game.
//!
//! ## Why the mask is derived and not stored
//!
//! This module used to hold **two** 64-byte constants and XOR them together. The
//! plain key was genuinely absent from the binary — that much worked — but the two
//! halves were declared one after the other, so the compiler laid them out one after
//! the other in `.rodata`. That is enough to lose the key: slide a 128-byte window
//! over the whole file, XOR its two halves, and test each candidate against the `mac`
//! of any save the attacker already owns. Measured on this binary, that search found
//! the key in **1.9 seconds**, from thirty lines of script, with no debugger and no
//! disassembly. The secret was never the bytes; it was their *adjacency*.
//!
//! So there is no second array now. [`SEED`] is sixteen bytes, and the mask is
//! **computed** from it by two rounds of SHA-256 — a hash the optimiser will not
//! evaluate at compile time, over an input [`black_box`](core::hint::black_box) makes
//! opaque anyway. A window search has nothing to slide against: the only 64 bytes in
//! the binary are [`MASKED`], and the value that would turn them into the key exists
//! only while [`key`] is running. Reading the derivation out of the code is still
//! entirely possible — that is the structural ceiling above, and it is now what the
//! attack costs.
//!
//! **Changing the scheme did not change the key.** [`MASKED`] was recomputed so that
//! [`key`] returns exactly the bytes it returned before, which is why no save on any
//! disk was invalidated by this and why
//! `the_key_is_the_one_the_vector_was_signed_with` passed unchanged. That test, over
//! in [`envelope`](super::envelope), is also what pins the key from here: this module
//! deliberately contains no copy of the plain bytes to assert against, since writing
//! one into a test would put back in the repository exactly what [`MASKED`] exists to
//! keep out of it.
//!
//! ## Why the plain bytes are never a constant
//!
//! [`key`] is a plain `fn`, not a `const fn`, and it reads its constants through
//! [`black_box`](core::hint::black_box). Both are deliberate, and both defend the
//! same thing: **a compile-time reassembly would write the plain key into the
//! binary**, cancelling the hiding with the very line meant to carry it. A `const fn`
//! called in a `const` position is folded by the compiler; a plain one can still be
//! folded by the optimiser when its inputs are known — and `cargo build --release`
//! runs LTO. `black_box` is what makes the seed opaque to that fold, and
//! `#[inline(never)]` keeps the reassembly in one place rather than copied into every
//! call site.
//!
//! ## Why the key is 64 bytes
//!
//! Not a round number: it is HMAC-SHA256's **block size**, and the one length RFC
//! 2104 neither zero-pads nor pre-hashes. That has a consequence in this crate's
//! favour. [`KeyInit::new`](hmac::KeyInit::new) takes exactly a block-sized key and
//! is total, while `new_from_slice` takes any length and returns a
//! [`Result`] whose `Err` cannot happen — the [`hmac`] crate's own example writes
//! `.expect("HMAC can take key of any size")`, and `expect` is a lint here. Choosing
//! the cryptographically untransformed length is what removes a `Result` from the
//! save path rather than smuggling one past a lint.
//!
//! It is also exactly two SHA-256 outputs, which is why the derivation below is two
//! rounds and not a loop with a counter.
//!
//! ## What a test can prove here, and what it cannot
//!
//! The tests below assert that the stored constant is **not** the key, and that the
//! mask is derived rather than stored. They do **not** assert that the plain bytes
//! are absent from the binary, and no test in this repository can: a unit test runs
//! inside the process, and `cargo test` builds a *test* binary rather than the release
//! one a player receives. That check is a procedure, written up in `docs/SYSTEMS.md` —
//! and note that `strings` is the wrong tool for it, since a binary key contains no
//! printable run to find even when it is there in full.

use core::hint;

use sha2::{Digest, Sha256};

/// HMAC-SHA256's block size, and therefore this key's length. See the module doc for
/// why that is the number rather than a preference.
pub const KEY_LEN: usize = 64;

/// SHA-256's output length. Two of these make one [`KEY_LEN`].
const DIGEST_LEN: usize = 32;

/// What the mask is grown from. **Not the key, and not half of it**: on its own it is
/// sixteen bytes that XOR against nothing in the binary.
const SEED: [u8; 16] = [
    0xa1, 0xe3, 0xd0, 0x45, 0x26, 0xd6, 0x64, 0xf1, 0xdb, 0x51, 0xdd, 0xe5, 0xb0, 0x4f, 0x13, 0x74,
];

/// The key, XOR-ed against the mask [`mask`] derives. This is the only 64-byte
/// constant in the crate, which is the whole point — see the module doc.
const MASKED: [u8; KEY_LEN] = [
    0x06, 0x93, 0x7f, 0x28, 0x7b, 0x25, 0xaf, 0x11, 0x89, 0x9e, 0xd4, 0x26, 0xcf, 0x4a, 0x46, 0x47,
    0xe2, 0xec, 0xab, 0x14, 0xd4, 0x14, 0x97, 0x6b, 0x45, 0x09, 0x10, 0x38, 0x9e, 0x3f, 0xfb, 0xa5,
    0xa9, 0x77, 0x27, 0x87, 0xc2, 0x8c, 0x74, 0xfb, 0xc2, 0xd7, 0x12, 0x3c, 0x20, 0x77, 0xd1, 0x5d,
    0x0b, 0xeb, 0xc4, 0x0c, 0xa9, 0xa2, 0x33, 0x0c, 0x60, 0xa6, 0xbd, 0xb7, 0x10, 0x5a, 0x75, 0xe8,
];

/// Grows [`SEED`] into [`KEY_LEN`] bytes, at run time and never at compile time.
///
/// Two chained SHA-256 rounds rather than one hash of a longer input, because
/// [`KEY_LEN`] is two digests wide and chaining is the shape that stays right if the
/// length ever changes: each round consumes the previous one's output.
///
/// The seed goes through [`black_box`](hint::black_box) so the optimiser cannot treat
/// the whole chain as a known-input computation and fold it back down to the constant
/// it exists to avoid writing.
fn mask() -> [u8; KEY_LEN] {
    let first: [u8; DIGEST_LEN] = Sha256::digest(hint::black_box(SEED)).into();
    let second: [u8; DIGEST_LEN] = Sha256::digest(first).into();

    let mut mask = [0; KEY_LEN];
    mask[..DIGEST_LEN].copy_from_slice(&first);
    mask[DIGEST_LEN..].copy_from_slice(&second);
    mask
}

/// Rebuilds the key, at run time and never at compile time.
///
/// **Changing what this returns invalidates every save ever written**, which is why
/// `the_key_is_the_one_the_vector_was_signed_with` pins it: a new key is a new game
/// as far as the disk is concerned, and there is no migration for a signature. That
/// is also the constraint the derivation above was fitted to — [`MASKED`] was chosen
/// so this function's answer did not move when the scheme did.
#[inline(never)]
pub fn key() -> [u8; KEY_LEN] {
    let mut key = hint::black_box(MASKED);
    for (byte, mask) in key.iter_mut().zip(mask()) {
        *byte ^= mask;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one property of the hiding a test *can* see from inside the process: the
    /// array in the source is not the array the signing uses. It is a cheap guard
    /// against [`MASKED`] being "simplified" away by someone who reads it as the key
    /// itself.
    #[test]
    fn the_stored_constant_is_not_the_key() {
        assert_ne!(MASKED, key());
        assert_ne!(mask(), key());
    }

    /// **The property that replaced the second constant.** The mask must not be
    /// findable in the binary as itself, and the nearest thing a unit test can check
    /// is that it is not the one array that *is* in there. A future edit that
    /// "simplified" [`mask`] into a stored table would fail this and, more usefully,
    /// would fail to explain itself against the module doc.
    #[test]
    fn the_mask_is_derived_and_is_not_the_stored_constant() {
        assert_ne!(mask(), MASKED);
        assert_eq!(mask().len(), KEY_LEN);
        // Deterministic: the same seed grows the same mask, or a reload would not
        // open a file this build wrote a second earlier.
        assert_eq!(mask(), mask());
        // The seed is not a prefix of what it grows, which a one-round derivation
        // that simply padded would produce.
        assert_ne!(&mask()[..SEED.len()], &SEED[..]);
    }

    /// A key of the wrong length would still compile — [`KeyInit::new`] would simply
    /// stop matching — but a key that is *all one byte* would compile, run, and sign
    /// everything with something a guess could find. Neither is likely; both are
    /// cheap to rule out.
    ///
    /// [`KeyInit::new`]: hmac::KeyInit::new
    #[test]
    fn the_key_is_sixty_four_bytes_of_something() {
        let key = key();
        assert_eq!(key.len(), KEY_LEN);
        assert!(key.iter().any(|&byte| byte != key[0]), "{key:?}");
    }
}
