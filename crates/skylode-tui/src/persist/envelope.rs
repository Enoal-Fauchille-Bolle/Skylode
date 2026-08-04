//! The envelope the payload travels in: `{"data": "…", "mac": "…"}`.
//!
//! `docs/SYSTEMS.md` §*Integrity* settles the shape; this module is the whole of it,
//! and it touches no filesystem — which is what lets every test below run without one.
//!
//! ## `data` is an escaped **string**, not a nested object
//!
//! The load-bearing decision, and it is about bytes rather than taste. A MAC covers
//! an exact sequence of bytes. Nested, `data` would come back from the parser as a
//! tree, and recomputing the MAC would mean **re-serialising** it — which serde_json
//! does not promise to do byte-identically: key order, number formatting and escape
//! choices are all free. A perfectly valid save would then fail its own signature,
//! intermittently and for no visible reason. As a string, the parser hands back the
//! payload unchanged and there is nothing to rebuild.
//!
//! The cost is accepted knowingly: the file reads as `{"data":"{\"version\":1,…`,
//! which spends most of what "human-debuggable" bought. `jq -r .data save.json`
//! restores it in one command, and the alternative is a bug that only appears on some
//! saves.
//!
//! A consequence worth naming so it is not mistaken for a hole: the MAC is taken over
//! the payload **unescaped**, so a file re-escaped differently — `A` for `A` —
//! yields the same payload, the same MAC and the same run. That is agreement, not a
//! forgery.
//!
//! ## Why the version is not out here
//!
//! It is inside the payload, where [`save`](skylode_core::save) put it. Out in the
//! envelope it would be the one field a tamperer could edit freely, and it is
//! precisely the field that decides which migration runs. Nothing stops the envelope
//! from *repeating* it later as a routing hint; hoisting it out of the signature is
//! the move that could not be undone.

use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;

use super::{PersistError, key};

/// SHA-256's output length, and therefore the MAC's.
const MAC_LEN: usize = 32;

/// What is actually on disk. Read-only: [`seal`] assembles the text by hand so that
/// writing an envelope cannot fail — see there.
#[derive(Deserialize)]
struct Envelope {
    /// The payload [`save::to_json`](skylode_core::save::to_json) produced, verbatim.
    data: String,
    /// [`sign`]'s answer over `data`, lowercase hexadecimal.
    mac: String,
}

/// Wraps a payload in a signed envelope.
///
/// **Returns a [`String`] and not a [`Result`]**, which is worth a sentence because
/// the obvious implementation cannot. `serde_json::to_string` over a struct of two
/// [`String`]s is infallible in fact and fallible in signature, and a save path that
/// carries an error case nobody can reach is a match arm every caller has to invent a
/// message for. [`Value`]'s [`Display`](std::fmt::Display) does the one part that
/// genuinely needs a serialiser — escaping the payload into a JSON string — and hands
/// back a [`String`] directly. The `mac` needs no escaping at all: hexadecimal is
/// already a JSON-safe alphabet.
pub fn seal(payload: &str) -> String {
    let data = Value::String(payload.to_owned()).to_string();
    let mac = to_hex(&sign(payload.as_bytes()));
    format!(r#"{{"data":{data},"mac":"{mac}"}}"#)
}

/// Unwraps an envelope and checks its seal, returning the payload untouched.
///
/// **The two refusals are the two questions a recovery screen asks separately.**
/// [`Damaged`](PersistError::Damaged) means *this is not an envelope* — unparseable,
/// truncated mid-file, a `mac` that is not 64 hex digits — and nothing can be said
/// about what it once held. [`Tampered`](PersistError::Tampered) means the envelope is
/// intact and the signature is not, which is the only outcome that offers the backup.
pub fn open(text: &str) -> Result<String, PersistError> {
    let envelope: Envelope = serde_json::from_str(text).map_err(|_| PersistError::Damaged)?;
    let expected = from_hex(&envelope.mac).ok_or(PersistError::Damaged)?;

    if !verify(envelope.data.as_bytes(), &expected) {
        return Err(PersistError::Tampered);
    }
    Ok(envelope.data)
}

/// The keyed hash of a payload.
///
/// Takes the key by value from [`key::key`] on every call rather than holding it in a
/// `static`: a `static` would be the plain bytes sitting in the binary's data
/// section, which is exactly what the masking exists to avoid. Sixty-four XORs per
/// save, twice a minute.
fn sign(payload: &[u8]) -> [u8; MAC_LEN] {
    let mut mac = <Hmac<Sha256>>::new(&key::key().into());
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

/// Whether `expected` is the signature of `payload`.
///
/// Through [`verify_slice`](Mac::verify_slice) rather than an `==` over [`sign`]'s
/// answer, so the comparison is constant-time. The threat model here does not need
/// that — an attacker holds the file and can take all day — but it costs nothing and
/// spares a hand-rolled comparison over secret-adjacent bytes.
fn verify(payload: &[u8], expected: &[u8]) -> bool {
    let mut mac = <Hmac<Sha256>>::new(&key::key().into());
    mac.update(payload);
    mac.verify_slice(expected).is_ok()
}

/// Bytes as lowercase hexadecimal.
///
/// Hand-rolled rather than a dependency: two characters per byte over thirty-two
/// bytes is a `fold`, and a crate for a `for` loop is a crate to audit, update and
/// explain forever.
fn to_hex(bytes: &[u8; MAC_LEN]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(MAC_LEN * 2), |mut text, byte| {
            text.push_str(&format!("{byte:02x}"));
            text
        })
}

/// The inverse, refusing anything that is not exactly what [`to_hex`] writes.
///
/// **Lowercase only, and exactly [`MAC_LEN`] bytes.** Accepting uppercase would be
/// generosity toward a file this program did not write, and every such file is one
/// [`open`] is about to refuse anyway. The strictness also closes a small hole in the
/// obvious implementation: [`u8::from_str_radix`] accepts a leading `+`, so `"+f"`
/// would decode as fifteen from a pair that is not hexadecimal at all.
fn from_hex(text: &str) -> Option<[u8; MAC_LEN]> {
    let digits = text.as_bytes();
    if digits.len() != MAC_LEN * 2 {
        return None;
    }

    let mut bytes = [0; MAC_LEN];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks_exact(2)) {
        *byte = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(bytes)
}

/// One hexadecimal digit's value, or nothing if it is not one.
fn digit(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload with the two things that make escaping matter — a quote and a
    /// backslash — so the round trip below is not passing on plain text alone.
    const PAYLOAD: &str = r#"{"version":1,"note":"a \"quoted\" \\ run"}"#;

    /// Flips the low bit of the byte at `at`, which is the smallest change a file can
    /// suffer and the one a checksum exists to catch.
    fn flip(text: &str, at: usize) -> String {
        let mut bytes = text.as_bytes().to_vec();
        bytes[at] ^= 1;
        match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => unreachable!("flipping a low bit of ASCII stays UTF-8: {error}"),
        }
    }

    /// Where `needle` starts in `text`.
    ///
    /// The two flip tests aim by name rather than by offset, because the offsets that
    /// look obvious are the escapes: flipping the low bit of the `\` in `\"` turns the
    /// field into a *shorter* JSON string, which is [`PersistError::Damaged`] and not
    /// the corruption these tests are about.
    fn at(text: &str, needle: &str) -> usize {
        match text.find(needle) {
            Some(at) => at,
            None => unreachable!("{needle} should be in {text}"),
        }
    }

    #[test]
    fn a_sealed_payload_opens_to_exactly_what_went_in() {
        match open(&seal(PAYLOAD)) {
            Ok(payload) => assert_eq!(payload, PAYLOAD),
            Err(error) => unreachable!("a sealed payload should open: {error}"),
        }
    }

    /// The escaping is real, not incidental: the payload's own quotes must survive as
    /// content rather than closing the field they are inside.
    #[test]
    fn the_payload_is_a_string_and_not_a_nested_object() {
        let sealed = seal(PAYLOAD);
        assert!(
            sealed.starts_with(r#"{"data":"{\"version\":1,"#),
            "{sealed}"
        );
        assert!(!sealed.starts_with(r#"{"data":{"#), "{sealed}");
    }

    #[test]
    fn one_flipped_byte_in_the_payload_breaks_the_seal() {
        let sealed = seal(PAYLOAD);
        let sealed = flip(&sealed, at(&sealed, "quoted"));
        assert!(matches!(open(&sealed), Err(PersistError::Tampered)));
    }

    /// The other half, and it is a different failure: the payload is untouched and
    /// the signature is the thing that moved.
    #[test]
    fn one_flipped_byte_in_the_mac_breaks_the_seal() {
        let sealed = seal(PAYLOAD);
        // The low bit of a hex digit stays a hex digit — `a` to `b`, `0` to `1` — so
        // what this produces is a wrong signature and not a malformed one, which is
        // what separates the two refusals.
        let at = at(&sealed, r#""mac":""#) + r#""mac":""#.len();
        assert!(matches!(
            open(&flip(&sealed, at)),
            Err(PersistError::Tampered)
        ));
    }

    /// Everything an envelope can be wrong *about itself* is one answer, because no
    /// screen can offer a different action for any of them.
    #[test]
    fn anything_that_is_not_an_envelope_is_damaged() {
        let sealed = seal(PAYLOAD);
        let not_envelopes = [
            String::from("not json at all"),
            String::from("{}"),
            String::from(r#"{"data":"x"}"#),
            String::from(r#"{"mac":"00"}"#),
            // A file cut in the middle: what an interrupted write would leave, if
            // the write were not atomic.
            sealed[..sealed.len() / 2].to_owned(),
            // A mac of the right alphabet and the wrong length.
            sealed.replace(r#""mac":""#, r#""mac":"ff"#),
        ];

        for text in not_envelopes {
            assert!(
                matches!(open(&text), Err(PersistError::Damaged)),
                "should be damaged: {text}"
            );
        }
    }

    /// Uppercase is refused deliberately, and a `+` is the case the obvious
    /// implementation would have let through.
    #[test]
    fn a_mac_that_is_not_lowercase_hexadecimal_is_damaged() {
        for mac in [
            "FF".repeat(MAC_LEN),
            "+f".repeat(MAC_LEN),
            "zz".repeat(MAC_LEN),
        ] {
            assert_eq!(from_hex(&mac), None, "{mac}");
        }
        assert_eq!(from_hex(""), None);
        assert_eq!(from_hex(&"ab".repeat(MAC_LEN - 1)), None);
    }

    #[test]
    fn every_byte_survives_the_hexadecimal() {
        for byte in u8::MIN..=u8::MAX {
            let bytes = [byte; MAC_LEN];
            assert_eq!(from_hex(&to_hex(&bytes)), Some(bytes));
        }
    }

    /// **The known-answer vector.** A fixed payload signs to one fixed string, which
    /// pins the key, the algorithm and the encoding together in a single assertion.
    ///
    /// If it fails, the question is not "what is the new hexadecimal?" but "what did
    /// we just do to every save ever written?" — a signature has no migration, so a
    /// changed key is a wiped disk.
    #[test]
    fn the_key_is_the_one_the_vector_was_signed_with() {
        assert_eq!(
            to_hex(&sign(b"skylode")),
            "76d542976f8cf1b632d88fd10f544717b647d0dc3f07eaef7c87e7c42109e51b"
        );
    }
}
