# 0114 — The HMAC key is obfuscated in the binary

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

**The HMAC key is obfuscated in the binary; build-time injection is rejected, and the
effort goes into validation instead**

## Why

The ceiling is structural and worth stating before the trade-off: the game runs on the
player's machine, so the binary necessarily contains everything needed to produce a
valid save. What is chosen is therefore an *effort level*, not a guarantee. Obfuscation
— the key held masked and reassembled by a `const fn` — is the one step that pays,
moving the attack from a single command needing no skill to a debugger, which is the
threshold the trade's own practice puts at "not worth it" for most players; it costs ~40
isolated lines, no dependency and nothing at run time. **Injection at build time via
`env!` was rejected on three counts**: it keeps the key out of the repository but not
out of the binary, which is what the player receives; Rust's own guidance is to use
`env!` for a secret only when the binary is *not* distributed, the opposite of this
game; and the repository is meant to go public, so the reassembly method is readable
regardless. Its costs, by contrast, are daily — a key split between debug and release
means a run played during development will not load in the shipped game, and a forgotten
variable fails a release build. The remaining effort goes to
[validation](../SYSTEMS.md#a-load-validates-before-it-returns), the only layer that
survives the key being extracted *and* the only one that also serves the honest player,
since disk corruption fails it identically. Consistent with
[0057](0057-the-free-geometric-re-roll-is-knowingly-left-open-at.md) and
[0112](0112-the-dev-menu-is-gated-by-cfg-debug-assertions-plus-a.md): single-player,
offline, no leaderboard — a cheat harms nobody but its author.
