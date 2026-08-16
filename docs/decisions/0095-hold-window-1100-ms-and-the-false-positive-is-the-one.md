# 0095 — HOLD_WINDOW = 1100 ms, and the false positive is the one we eat

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

`HOLD_WINDOW` = 1100 ms, and the false positive is the one we eat

## Why

The window must exceed the largest initial auto-repeat delay a user setting can produce,
or mining cuts out during the gap and resumes — a visible hitch on every hold. Windows
caps at 1000 ms, the highest of the platforms. Below that, it breaks for someone; it is
not a preference. Since the initial delay (~500–660 ms) and the repeat interval (~30–40
ms) *differ*, no single timeout can avoid both false positives and false negatives — so
the choice is which to eat, and 1.1 s of over-mining is invisible in a game whose
offline cap is 7 days, while a stutter starting every hold is not. Tunable at phase 10;
the trigger to revisit is playtest finding the stop latency perceptible.
