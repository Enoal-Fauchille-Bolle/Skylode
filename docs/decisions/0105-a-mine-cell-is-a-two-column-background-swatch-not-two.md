# 0105 — A mine cell is a two-column background swatch

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**A mine cell is a two-column *background swatch*, not two coloured characters**; the
glyph channel carries the value stipple (`░░`) and the break progression (`.:#`)

## Why

Colour discrimination rises with area, and a `#` is mostly the gaps between its strokes.
Three problems fall together: a dark material stops competing with the terminal
background (all 24 swatches hold `L* >= 12`), a broken cell becomes the *absence* of a
swatch and so is maximally contrasted, and `#` is freed — which is what makes
[MECHANICS.md](../MECHANICS.md#break-feedback)'s `.:#` ordering correct as written,
dissolving rather than settling the reversal once proposed for it. See
[UI.md](../UI.md).
