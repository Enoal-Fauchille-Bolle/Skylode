# Skylode documentation

Routed by what you came here to do, not by what each file is called. The titles are
alphabetical accidents; the questions below are not.

## I want to…

| …then read | |
| --- | --- |
| **understand what the game is** | [DESIGN.md](DESIGN.md) — the concept, the loop, what each screen is for |
| **know the exact rule** | [MECHANICS.md](MECHANICS.md) — mining, worlds, pickaxe, enchants, prestige |
| **know the order a swing resolves in** | the rustdoc on `GameState::resolve_swing` — five steps, three of them fixed |
| **know the exact number** | [BALANCE.md](BALANCE.md) — every price, generated from the code |
| **know what is on screen** | [UI.md](UI.md) — screens, overlays, keys, counted frames |
| **know how it is built** | [SYSTEMS.md](SYSTEMS.md) — the save format, the tick loop, the module map |
| **know *why* it is like that** | [decisions/](decisions/) — one numbered record per decision |
| **change something** | [guides/](guides/) — the recipe for each kind of change |
| **reach a state a test cannot play to** | [DEV-MENU.md](DEV-MENU.md) |
| **know what is left** | [ROADMAP.md](ROADMAP.md) — MVP scope and what is deferred |
| **know how it got here** | [PHASES.md](PHASES.md) — the build order, all of it shipped |

Building, testing and committing are not here: they are in
[CONTRIBUTING.md](../CONTRIBUTING.md), because they are the same questions whether or
not you ever open this directory.

## One fact, one home

The rule this directory is organised around, and the one to keep it organised:

| Authoritative on | Lives in |
| --- | --- |
| numbers, execution order, invariants | **the rustdoc**, beside the code it describes |
| what the game must do | **`docs/`** |
| why, and what was rejected | **`docs/decisions/`** |
| how to make a change | **`docs/guides/`** |
| what is left to do | **GitHub issues** |

Everything else links. A fact restated in a second place is a fact that will be wrong
in one of them, and there is no way to tell which — the swing order was once written in
eight files in three different truncations, none of which announced itself as partial.

`scripts/check-docs.sh` enforces the mechanical half of this — links, anchors, Rust
names, line width, and the decision records' own numbering. It cannot read prose, so it
cannot tell you a paragraph has stopped being true. That part is still on the reader.
