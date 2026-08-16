# 0122 — A preference changed on the title is carried by the Splash

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**A preference changed on the title is carried by the `Splash`, not written to disk
there**

## Why

TUI phase 9. Settings is reachable before any run exists, so the preferences it turns
have nowhere to live: the config is *inside the save*, and on the title there is no save
open. The title therefore holds a `Config` of its own — filled from the loaded save, or
defaulted on a fresh install — and `Continue` and `New game` hand **it** to the run they
open, instead of re-reading the file's. It reaches the disk on that run's first
autosave. **The cost is accepted knowingly**: changing a setting and then quitting from
the title without playing loses it. The alternative was to write a save from the title,
which would mean a code path that persists config with no run behind it — a second
writer for one file, and one that would have to invent what to write for a player who
has never played. A preference is worth exactly one run's first ten seconds; a second
save writer is worth more than that.
