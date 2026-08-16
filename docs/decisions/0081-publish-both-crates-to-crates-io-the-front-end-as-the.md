# 0081 — Publish both crates to crates.io

**Status:** accepted
**Tags:** project
**Supersedes:** —
**Superseded by:** —

## Decision

Publish both crates to crates.io: the front-end as the installable game, `skylode-core`
as a library in its own right

## Why

Extends "single offline binary" rather than replacing it: `cargo install` is a
distribution channel for the same offline binary, and the rules being consumable on
their own is what the core/TUI boundary was for.

**The package keeps the name `skylode-tui`, and that is the interesting half.**
Publishing makes the *package* name public, and the argument that had justified
`skylode-tui` — it names a place in the workspace, the player only ever types the
binary's name — dies at that moment. It was replaced rather than patched: `skylode` is
the whole game, this package holds only the front-end, so naming it `skylode` would be a
false claim about its contents. The install line pays one hyphen — `cargo install
skylode-tui`, installing a binary called `skylode` — and the source tree stays honest.
Cargo's own ambiguity is the root of it: a package is both a source unit and the unit
`cargo install` delivers, and here the two want different names.

**No registry token lives in this repository.** The `publish-crate` job authenticates by
**Trusted Publishing**: the runner exchanges its OIDC identity for a credential that
lives thirty minutes, so there is nothing stored for anyone to leak. The first push had
to be manual, because crates.io requires an existing version before a repository can be
linked as a trusted publisher.

