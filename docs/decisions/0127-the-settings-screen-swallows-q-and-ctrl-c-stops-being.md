# 0127 — The Settings screen swallows q, and Ctrl-C stops being the same gesture

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**The Settings screen swallows `q`, and `Ctrl-C` stops being the same gesture**

## Why

TUI phase 9, on Enoal's review of the shipped screen. One frame was answering one key
two ways depending on the door it was opened through: from a game `q` never arrives,
because a modal is offered every key first, while from the title it reached the menu
vocabulary and ended the process. A modal captures *every* key — that is what modal
means — so the letter is swallowed on both doors and getting out is `Esc` then `q`, two
gestures that each say what they do. **The exception is `Ctrl-C`**, and keeping it
forced a real change: the menu resolver had mapped `q` and `Ctrl-C` to a single gesture,
on the argument that a screen with no game behind it has no third thing quitting could
mean. Settings is that third thing, so the two decode to different gestures now — one a
frame may capture, one nothing may. That is the split the in-game resolver has always
drawn between *back to the title* and *end the process*; it simply reached the menus for
the first time.
