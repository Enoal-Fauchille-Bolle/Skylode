# 0097 — Save recovery refuses a save that fails its checksum

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

Save recovery refuses a save that fails its checksum: no *Continue anyway*, and no
exception even when the `.bak` also fails

## Why

Loading data that failed its HMAC is exactly what a hand-editor needs, so refusing it is
a real if partial protection — and partial is not worthless. The counter-argument (it
punishes the innocent whose save is genuinely corrupt) dies on the save cadence:
autosave every 10 s plus transactions plus exit means the `.bak` is seconds old, so the
innocent loses seconds, not a run. Reopening the hatch only when the `.bak` is missing
would rebuild the whole hole with one extra step in front of it. Overrides
[SYSTEMS.md](../SYSTEMS.md#robustness-and-recovery)'s earlier "continue anyway at their
own risk".
