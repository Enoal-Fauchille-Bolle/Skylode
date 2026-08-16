# 0089 — Colour: a 256-colour palette, degrading to 16 in Settings

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Colour: a 256-colour palette, degrading to 16 in Settings

## Why

Twelve mines × (common cell + value cell) is 24 materials to tell apart. Sixteen ANSI
colours cannot carry 24 without collisions (Lapis and Diamond become one blue); 256 is
near-universal and enough. The 16-colour mode is the fallback for poor terminals and for
colour blindness, and there the *glyph* carries the difference. Truecolor was rejected.
