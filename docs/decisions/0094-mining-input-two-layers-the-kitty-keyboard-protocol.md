# 0094 — Mining input: two layers

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Mining input: two layers — the kitty keyboard protocol where it exists, a fixed 1100 ms
window everywhere else

## Why

A terminal sends **nothing** when a key is released: the legacy encoding is "one key =
its character", and a character has no duration, so *hold Space* is not expressible. The
kitty protocol adds a real `release` event type and is the exact path, but it needs
**`REPORT_EVENT_TYPES` *and* `REPORT_ALL_KEYS_AS_ESCAPE_CODES`** — Space produces text,
so without the second flag it is sent as raw `0x20` and carries no event type at all.
Everywhere else, only OS auto-repeat is observable, so `space_held = (now −
last_space_event) < HOLD_WINDOW`. Windows reports release natively through the Console
API, without kitty. See [SYSTEMS.md](../SYSTEMS.md#keyboard-input).
