# 0128 — r restores the setting under the cursor, and there is no reset-all

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`r` restores the setting under the cursor, and there is no reset-all**

## Why

TUI phase 9, Enoal's call. It is the only destructive gesture on the one screen with no
confirmation, and every ladder there is a closed enum short enough to walk back by hand
in at most three presses — so a global reset would buy two keystrokes at the price of
the only key that can undo work the player meant to keep. The default is read out of
`Config::default()` rather than written down a second time, since which value is default
is already declared once, in the `#[default]` attribute on each preference's enum. The
key is printed in the footer: an undo nobody can see is an undo nobody uses.
