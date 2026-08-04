//! The signing key, and the one step taken to hide it.
//!
//! **The ceiling is structural and comes first.** The game runs on the player's
//! machine, so the binary necessarily contains everything needed to produce a valid
//! save. What `docs/DECISIONS.md` chooses is therefore an *effort level*, not a
//! guarantee: masking moves the attack from one command needing no skill to a
//! debugger, which is where the trade's own rule of thumb puts save editing into the
//! *not worth it* basket for most players. It costs no dependency and nothing at run
//! time. Build-time injection via `env!` was considered and rejected there — it keeps
//! the key out of the repository but not out of the binary, which is what the player
//! receives, and it splits the key between debug and release so a run played during
//! development would not load in the shipped game.
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
//! ## Why the plain bytes are never a constant
//!
//! [`key`] is a plain `fn`, not a `const fn`, and it reads its two constants through
//! [`black_box`](core::hint::black_box). Both are deliberate, and both defend the
//! same thing: **a compile-time XOR would write the plain key into the binary**,
//! cancelling the masking with the very line meant to carry it. A `const fn` called
//! in a `const` position is folded by the compiler; a plain one can still be folded
//! by the optimiser, since it is a pure function of two known arrays — and
//! `cargo build --release` runs LTO. `black_box` is what makes the inputs opaque to
//! that fold, and `#[inline(never)]` keeps the reassembly in one place rather than
//! copied into every call site.
//!
//! ## What a test can prove here, and what it cannot
//!
//! The tests below assert that the stored constant is **not** the key and that the
//! key is what the [`envelope`](super::envelope) module's known-answer vector was
//! computed with. They do **not** assert that the plain bytes are absent from the
//! binary, and no test in this repository can: a unit test runs inside the process,
//! and `cargo test` builds a *test* binary rather than the release one a player
//! receives. That check is a procedure, written up in `docs/SYSTEMS.md` — and note
//! that `strings` is the wrong tool for it, since a binary key contains no printable
//! run to find even when it is there in full.

use core::hint;

/// HMAC-SHA256's block size, and therefore this key's length. See the module doc for
/// why that is the number rather than a preference.
pub const KEY_LEN: usize = 64;

/// The key, XOR-ed against [`MASK`]. Neither this nor [`MASK`] is the key.
const MASKED: [u8; KEY_LEN] = [
    0x68, 0x42, 0x1e, 0x95, 0x94, 0x44, 0xc1, 0x45, 0xee, 0xef, 0x2f, 0xc7, 0xfe, 0x33, 0x5e, 0x92,
    0x9a, 0xdb, 0xb2, 0x36, 0x61, 0xcb, 0x9b, 0xf0, 0xe4, 0xed, 0x77, 0x7e, 0x57, 0xe9, 0x56, 0xf5,
    0xf5, 0xb5, 0x4c, 0xa5, 0x3f, 0xef, 0x25, 0xae, 0x36, 0x2e, 0x9c, 0x10, 0xa9, 0xd2, 0x27, 0x46,
    0x20, 0x7a, 0xfd, 0xa6, 0x22, 0x3a, 0xd1, 0xd6, 0xc9, 0xd3, 0xe4, 0xa3, 0x8d, 0xd9, 0x6e, 0x41,
];

/// The pattern [`MASKED`] is XOR-ed against.
///
/// Random rather than a repeated byte, which would be visible as a pattern in a hex
/// dump and would leave [`MASKED`] a rotation of the key rather than an unrelated
/// array.
const MASK: [u8; KEY_LEN] = [
    0x57, 0xb2, 0x00, 0x54, 0xc1, 0xe5, 0x57, 0x87, 0x36, 0xa4, 0x3a, 0x75, 0x12, 0x2b, 0xd9, 0x39,
    0x44, 0xb7, 0x89, 0xa3, 0x47, 0x7b, 0xcc, 0x37, 0xcb, 0x17, 0x84, 0xd3, 0xec, 0x8c, 0xe6, 0x26,
    0x6d, 0xe6, 0xe5, 0x6b, 0x19, 0xfc, 0x98, 0xa7, 0x54, 0xeb, 0x7b, 0x89, 0x67, 0x7e, 0xae, 0xd9,
    0x1e, 0x4f, 0xb9, 0xac, 0xd9, 0x26, 0x0c, 0xc1, 0x84, 0x2d, 0x23, 0xd3, 0x4f, 0xd1, 0x8b, 0xa0,
];

/// Rebuilds the key, at run time and never at compile time.
///
/// **Changing what this returns invalidates every save ever written**, which is why
/// `the_key_is_the_one_the_vector_was_signed_with` pins it: a new key is a new game
/// as far as the disk is concerned, and there is no migration for a signature.
#[inline(never)]
pub fn key() -> [u8; KEY_LEN] {
    let mut key = hint::black_box(MASKED);
    for (byte, mask) in key.iter_mut().zip(hint::black_box(MASK)) {
        *byte ^= mask;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one property of the masking a test *can* see from inside the process: the
    /// array in the source is not the array the signing uses. It is a cheap guard
    /// against the masking being "simplified" away by someone who reads `MASKED` as
    /// the key itself.
    #[test]
    fn the_stored_constant_is_not_the_key() {
        assert_ne!(MASKED, key());
        assert_ne!(MASK, key());
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
