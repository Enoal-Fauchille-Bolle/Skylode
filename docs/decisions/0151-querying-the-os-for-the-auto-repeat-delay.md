# 0151 — Querying the OS for the auto-repeat delay

**Status:** rejected
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Querying the OS for the auto-repeat delay

## Why

Not hard — **impossible from a TUI**. `/dev/input` and the `KDSKBMODE` ioctl need root
and see the whole system with no window focus; X11 needs a running X server and `ssh
-X`, and would link X11 into a terminal game; over plain SSH the release "doesn't get
communicated at all" (zero bytes); Wayland sends `repeat_info` on `wl_keyboard`, which
the *terminal* receives and the TUI cannot see. The repeat is generated where the keys
are, and over SSH that is not where the program runs. A tty only knows "data in, data
out".
