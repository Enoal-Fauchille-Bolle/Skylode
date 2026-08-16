# 0083 — UI wireframes are ASCII monospace, not diagrams; flows are Mermaid

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

UI wireframes are ASCII monospace, not diagrams; flows are Mermaid

## Why

The deliverable is a grid of characters with a hard 80×24 budget, so the only wireframe
that can *prove* the content fits is one drawn in the same units. A diagram of a
terminal is a drawing of a lie: it renders an elegant panel that turns out to need 34
columns for `6 Compressed Iron + 50 Iron ✓`. Diagrams keep what ASCII does badly — the
navigation graph and the state machine — as Mermaid, in the same file.
