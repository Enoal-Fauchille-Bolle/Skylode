# 0149 — A separate config file (XDG ~/.config/skylode/)

**Status:** rejected
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

A separate config file (XDG `~/.config/skylode/`)

## Why

Considered so config would escape the save's HMAC. Rejected: one file is simpler, and
the HMAC objection dissolves once Settings exposes every config field, since nobody then
needs to hand-edit anything.
