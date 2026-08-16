# Guides

How to make a change, as a sequence of steps. Each guide exists because doing the thing
otherwise means reading three documents and the code to work out what it touches.

| Guide | When |
| --- | --- |
| [adding-a-mine.md](adding-a-mine.md) | a thirteenth `MineKind` |
| [retuning-a-curve.md](retuning-a-curve.md) | a price, a slope, any balance dial |
| [changing-the-save-format.md](changing-the-save-format.md) | a field added to, or moved in, saved state |
| [adding-a-screen-or-overlay.md](adding-a-screen-or-overlay.md) | a seventh tab or a new modal |

**The pattern all four share.** This codebase is built so that an incomplete change is
a *compile error* rather than a silent gap: enums are matched exhaustively, tables are
fixed-length arrays whose length lives in the type, and a half-finished path is marked
`expect(dead_code, reason = "awaiting the phase-N …")` so the lint fires when the phase
lands. So the honest first step in every guide below is the same one — **make the
smallest change and run `cargo check`**. The compiler will enumerate the rest of the
work for you, and it is more reliable than any list a document can keep.

What the compiler cannot tell you is what a change *means*: which test now measures the
wrong thing, which save on disk stops loading, which sentence in `docs/` has quietly
become false. That is what these guides are for.
