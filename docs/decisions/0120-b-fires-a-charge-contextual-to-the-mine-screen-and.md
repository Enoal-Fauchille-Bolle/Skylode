# 0120 — b fires a charge, contextual to the Mine screen and printed in its…

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`b` fires a charge, contextual to the Mine screen and printed in its footer**

## Why

The window is thirty seconds, so a charge fired from the Inventory table is thirty
seconds spent looking at a table; and Mine is the only screen that draws the gauge the
boost appears on. A global `b` was the alternative — it would fire straight after a
purchase without changing screen — and it would have to follow `q` and `s` in being
advertised nowhere but Help, for a key that does nothing visible on five screens out of
six. The footer slot is the one freed when `q` came off it, which is the same rule
paying for both: a footer shows the bindings the *screen* owns. It is also the only
place the key is named for a player who was granted a charge by a level-up and has never
opened the shop. [UI.md](../UI.md#9-the-keymap) §9.
