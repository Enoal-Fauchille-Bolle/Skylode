# 0093 — Screens shown before the save is trusted render with hardcoded defaults

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

Screens shown before the save is trusted render with hardcoded defaults

## Why

Main menu, terminal-too-small and save recovery cannot read config, because config is
inside a save that is missing (fresh install) or has just failed its HMAC. Reading
settings from a save you have decided not to trust is a contradiction, and the recovery
screen is the first thing some players ever see.
