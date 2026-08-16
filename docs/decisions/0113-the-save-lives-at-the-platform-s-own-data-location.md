# 0113 — The save lives at the platform's own data location

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

**The save lives at the platform's own data location**, resolved through
`directories::ProjectDirs` — `~/.local/share/skylode/save.json` on Linux — and not in a
dot-directory under `$HOME`

## Why

Enoal's call, on the stated constraint *do not pollute the player's home*. This
**revises** [SYSTEMS.md](../SYSTEMS.md#where-the-file-lives)'s earlier "one file, one
path, no XDG handling", and revises only its second half: what that decision rejected
was *splitting* the game across several XDG categories — preferences under `~/.config`,
state under `~/.local/state`, save data under `~/.local/share` — and that split is
exactly what config-in-the-save exists to prevent. It stands untouched; there is still
one file and still no config file. What falls is the claim that the one file therefore
belongs in `~/.skylode/`, which is precisely the pollution the convention was invented
to stop. "No XDG handling" was an argument about **complexity**, and a library reduces
that complexity to a single call — while hand-rolling the lookup would be ten lines on
Linux and wrong on macOS and Windows, a partial reimplementation of a standard being
more fragile than none because it looks correct. `ProjectDirs::from` answering `None`
starts the game **without persistence and says so**, rather than refusing to launch.
