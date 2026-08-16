# 0107 — The value stipple is unconditional in both colour modes

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**The value stipple is unconditional in both colour modes; there is no dedicated
accessibility setting and no `Glyphs` setting**

## Why

The common-vs-value distinction is what the Mine screen exists for, so making it depend
on a setting would make it optional. Always on, it is redundant at 256 colours and
essential at 16 — which is what redundancy is supposed to look like. The 16-colour
fallback therefore becomes "one colour per *mine*": one rendering model with a channel
switched off, not a second code path. The `Glyphs` row, written as an ASCII fallback,
has no work left — ratatui ships no ASCII border set and Unicode was settled — and a
setting is what you add when a preference is genuinely contested, not a way of declining
to decide.
