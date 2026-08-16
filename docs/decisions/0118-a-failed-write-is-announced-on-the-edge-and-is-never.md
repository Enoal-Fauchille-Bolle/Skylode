# 0118 — A failed write is announced on the edge and is never fatal

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

**A failed write is announced on the edge and is never fatal**

## Why

A full disk fails every ten seconds; announcing each one would bury the game under
identical refusals, and refusing to play would be the opposite of what *"no continue
anyway"* protects — the run **in memory** is fine, it is the disk that is not. So a
`bool` tracks whether writing works and the toast fires on a *change*: once when it
breaks, once when it works again. The case then repairs itself without relaunching.
