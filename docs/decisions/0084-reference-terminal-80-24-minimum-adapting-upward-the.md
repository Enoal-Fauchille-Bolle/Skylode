# 0084 — Reference terminal: 80×24 minimum

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Reference terminal: 80×24 minimum, adapting upward. **The grid is fixed, the chrome
flexes**

## Why

The mine grid is already a game constant *"decoupled from terminal size, so the window
size cannot change balance"* ([MECHANICS.md](../MECHANICS.md#mine-size)) — so it is a
fixed `Length(42)` (40 columns of `##` plus borders), never a proportion. All adaptivity
therefore belongs to the panels *around* it, which dissolves the responsive question
rather than answering it.
