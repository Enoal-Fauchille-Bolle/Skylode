# 0090 — Numbers are exact, with separators (1 234 567); never abbreviated

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Numbers are exact, with separators (`1 234 567`); never abbreviated

## Why

The composite denomination exists *for readability* and so the player "can check it in
their head" — an abbreviated `1.23M` destroys exactly that, since `I have 1.2M, the cost
is 1.23M, can I?` is unanswerable. Compression by 100 already keeps the printed numbers
small.
