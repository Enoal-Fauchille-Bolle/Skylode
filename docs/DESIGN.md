# Skylode - Design

The vision document: concept, scope, the core gameplay loop, and the screen
layout. This is the entry point to the design. Detailed rules live in the sibling
documents:

- [MECHANICS.md](MECHANICS.md): mining, worlds, pickaxe, enchants, auto-miner,
  offline, prestige.
- [SYSTEMS.md](SYSTEMS.md): save system, tech stack, architecture.
- [ROADMAP.md](ROADMAP.md): MVP scope, post-MVP, open questions.
- [PHASES.md](PHASES.md): the dependency-ordered build plan for the core, phase by
  phase.
- [DECISIONS.md](DECISIONS.md): settled decisions and rejected ideas.

**Status:** pre-MVP design.

## Concept

Skylode is a solo, terminal-based (TUI) idle/incremental mining game written in
Rust, inspired by PikaNetwork's SkyMines gamemode.

It reinterprets the SkyMines "tycoon mining" loop as a single-player, offline TUI.
Start with a Wooden pickaxe, mine ore cubes, spend the ores to upgrade the pickaxe
(tier plus Efficiency plus Fortune) and to grow your mines, level up your mining to
unlock new worlds with new materials and functions, enchant the pickaxe with each
world's enchant material, and eventually prestige for permanent multipliers.

There is no PvP, no multiplayer, no skyblock island building, no paid ranks, and
no money. The economy runs on ores directly.

## Inspiration and scope

Source: PikaNetwork SkyMines. We keep the core loop and diverge wherever
multiplayer or monetization assumptions do not fit a solo offline game.

Kept from SkyMines:

- Mine, upgrade gear with ores, reach harder ores, repeat.
- Ore cubes that regenerate. SkyMines is not a prison mode: mines are regenerating
  blocks, so there is no depth axis.
- Pickaxe tiers plus Efficiency plus Fortune.
- The tension where a new tier mines slower than the maxed previous tier.
- Worlds: Overworld, Nether, End.
- Ores with distinct functions (haste, fortune, enchants).
- Compressed materials as a higher-denomination unit in upgrade costs.

Added (not in SkyMines):

- Prestige.
- Offline progression (idle accrual).

Dropped or changed items are recorded in [DECISIONS.md](DECISIONS.md).

## Core gameplay loop

1. Mine the selected ore cube with the pickaxe (hold Space). Ores and mining XP
   accumulate.
2. Ores pile up in the inventory (Fortune multiplies yield).
3. Spend ores to upgrade the pickaxe (Efficiency inside a tier, then jump tier),
   and to **grow** and **enrich** the mine — its two tracks, both paid in that
   mine's own material. Size scales the spatial enchants; richness raises the
   weight of the mine's valuable cell (Iron Ore to Iron Block, End Stone to
   Amethyst), so each cell is worth more.
4. A tier jump unlocks harder mines but temporarily lowers mining speed (the
   "dip", a deliberate decision point).
5. Mining levels up. New levels unlock new worlds (Nether, then End) with new
   materials and functions (speed, fortune, enchants).
6. Enchant the pickaxe with each world's enchant material (Lapis, then Quartz,
   then Amethyst), which raises the enchant ceiling per world.
7. At the End, farm Amethyst and prestige to reset progression for a permanent
   multiplier.

Engagement comes from decisions (when to jump tier, what to upgrade, which mine to
grow, when to prestige), not from combat. The two gating axes (level opens worlds,
pickaxe tier opens mines) and the detailed rules behind each step are in
[MECHANICS.md](MECHANICS.md).

## Screens

The interface is keyboard-driven, with one responsibility per screen, switched via
Tab or number keys:

- **Mine:** active mining (hold Space). The current cube is shown as a 2D grid
  (each block `##`, the targeted cell highlighted with a crack glyph), with a
  status strip below the grid (break-progress bar, mining power, active boost, and
  the mining-level / XP bar).
- **Mines:** choose the world and cube. Worlds are gated by mining level and mines
  by pickaxe tier; shows unlocked versus locked, and each mine's size and richness.
  This is also where the **richness dial** lives — drawn only on the three mines
  where it is a real choice (Quartz, Obsidian, End), and hidden on the nine where
  its only sensible position is the top.
- **Inventory:** ores and amounts (raw and Compressed, no stack limit).
- **Upgrades:** the full pickaxe roadmap (every tier and Efficiency/Fortune step
  with its composite cost, current position highlighted, future steps visible for
  planning), plus enchant levels and both mine tracks (size and richness).
  Quality-of-life: affordability markers, buy times N, buy max.
- **Stats:** progression, mining level, prestige rank, milestones.
