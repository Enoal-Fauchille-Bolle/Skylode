# Skylode - Balance

Every price in the game, in the denominations the till demands them in.
`1C` = one Compressed unit = 100 raw.

> **This file is generated. Do not edit it.**
>
> ```sh
> cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
> ```
>
> The figures come from calling the same `economy` and `prestige` functions
> the game charges the player with, so they cannot disagree with the game.
> A hand-kept predecessor of this table went stale the day phase 10 replaced
> one cost slope with four; this one cannot. The *rules* these prices
> implement live in [MECHANICS.md](MECHANICS.md), and the *reasons* behind
> them in [decisions/](decisions/).

## The parameters every price is generated from

Each track is a curve `cost(n) = base * growth^n`, so a few numbers
generate the several hundred below. Changing one moves a whole column.

| Parameter | Value | Track it shapes |
| --- | --- | --- |
| `COST_BASE` | 100 | every track but enchants |
| `SIZE_COST_GROWTH` | 1.55 | mine size |
| `RICHNESS_COST_GROWTH` | 1.35 | mine richness |
| `UPGRADE_COST_GROWTH` | 1.45 | tier jumps and Efficiency |
| `NETHERITE_ENHANCEMENT_COST_GROWTH` | 1.1 | Efficiency 6..=15 |
| `ENCHANT_COST_BASE` | 1 000 | enchants and Fortune |
| `ENCHANT_COST_GROWTH` | 1.25 | enchants and Fortune |
| `BOOST_COST` | 300 | one boost charge |
| `AMETHYST_PER_CLIMB` | 5 000 | prestige, measured not chosen |
| `PRESTIGE_SURCHARGE_BASE` | 1 100 | prestige |
| `PRESTIGE_SURCHARGE_PER_RANK_PERMILLE` | 200 | prestige |
| `PRESTIGE_MULT_PER_RANK_PERMILLE` | 100 | prestige |

## Pickaxe: tier jumps

Paid in the tier being **left**, at that tier's step on the curve —
leaving Gold costs Gold. The jump is the last thing a tier is for.

| Leaving | Price | Raw total |
| --- | --- | --- |
| Wooden | 6C+41 Stone | 641 |
| Stone | 9C+29 Coal | 929 |
| Iron | 13C+48 Iron | 1 348 |
| Gold | 19C+54 Gold | 1 954 |
| Diamond | 28C+33 Diamond | 2 833 |
| **Total** | | **7 705** |

## Pickaxe: Efficiency, per tier

Levels 1..=5 on every tier, priced on the shared `UPGRADE_COST_GROWTH`
slope and paid in the current tier's material. Netherite's 6..=15 is a
separate track and has its own section.

| Tier | 1 | 2 | 3 | 4 | 5 | Total |
| --- | --- | --- | --- | --- | --- | --- |
| Wooden | 1C Stone | 1C+45 Stone | 2C+10 Stone | 3C+5 Stone | 4C+42 Stone | **1 202** |
| Stone | 1C Coal | 1C+45 Coal | 2C+10 Coal | 3C+5 Coal | 4C+42 Coal | **1 202** |
| Iron | 1C Iron | 1C+45 Iron | 2C+10 Iron | 3C+5 Iron | 4C+42 Iron | **1 202** |
| Gold | 1C Gold | 1C+45 Gold | 2C+10 Gold | 3C+5 Gold | 4C+42 Gold | **1 202** |
| Diamond | 1C Diamond | 1C+45 Diamond | 2C+10 Diamond | 3C+5 Diamond | 4C+42 Diamond | **1 202** |
| Netherite | 1C Ancient Debris | 1C+45 Ancient Debris | 2C+10 Ancient Debris | 3C+5 Ancient Debris | 4C+42 Ancient Debris | **1 202** |

## Pickaxe: the Netherite enhancement, Efficiency 6..=15

Ten rungs on a slope of its own, paid in Obsidian and Crying Obsidian
on a sliding mix. It restarts the curve at step 0 rather than
continuing from 5, so the enhancement is priced as its own ladder.

| Efficiency | Price | Raw total |
| --- | --- | --- |
| 6 | 75 Obsidian + 25 Crying Obsidian | 100 |
| 7 | 75 Obsidian + 35 Crying Obsidian | 110 |
| 8 | 74 Obsidian + 47 Crying Obsidian | 121 |
| 9 | 71 Obsidian + 62 Crying Obsidian | 133 |
| 10 | 67 Obsidian + 79 Crying Obsidian | 146 |
| 11 | 62 Obsidian + 99 Crying Obsidian | 161 |
| 12 | 55 Obsidian + 1C+22 Crying Obsidian | 177 |
| 13 | 47 Obsidian + 1C+48 Crying Obsidian | 195 |
| 14 | 36 Obsidian + 1C+78 Crying Obsidian | 214 |
| 15 | 22 Obsidian + 2C+14 Crying Obsidian | 236 |
| **Total** | | **1 593** |

## Enchants: the five specials

One shared ceiling per world gates how far any of them may be taken:
3 at the Overworld, 6 at the Nether, 10 at the End.

All five are priced identically at a given level, so one table serves
them; the material shifts with the world whose cap the level sits under.

| Level | Price | Raw total |
| --- | --- | --- |
| 1 | 5C Lapis + 3C+50 Stone + 1C+50 Coal | 1 000 |
| 2 | 6C+26 Lapis + 4C+37 Iron + 1C+87 Gold | 1 250 |
| 3 | 7C+82 Lapis + 5C+47 Gold + 2C+34 Diamond | 1 563 |
| 4 | 9C+78 Quartz + 6C+83 Netherrack + 2C+92 Ancient Debris | 1 953 |
| 5 | 12C+21 Quartz + 8C+54 Ancient Debris + 3C+66 Obsidian | 2 441 |
| 6 | 15C+27 Quartz + 10C+68 Obsidian + 4C+57 Crying Obsidian | 3 052 |
| 7 | 28C+62 End Stone + 9C+53 Amethyst | 3 815 |
| 8 | 25C+28 End Stone + 22C+40 Amethyst | 4 768 |
| 9 | 18C+48 End Stone + 41C+12 Amethyst | 5 960 |
| 10 | 6C+71 End Stone + 67C+80 Amethyst | 7 451 |
| **Total per enchant** | | **33 253** |

## Fortune

Capped by the world like the specials, but priced in a single material.

| Level | Price | Raw total |
| --- | --- | --- |
| 1 | 10C Emerald | 1 000 |
| 2 | 12C+50 Emerald | 1 250 |
| 3 | 15C+63 Emerald | 1 563 |
| 4 | 19C+53 Emerald | 1 953 |
| 5 | 24C+41 Emerald | 2 441 |
| 6 | 30C+52 Emerald | 3 052 |
| 7 | 38C+15 Emerald | 3 815 |
| 8 | 47C+68 Emerald | 4 768 |
| 9 | 59C+60 Emerald | 5 960 |
| 10 | 74C+51 Emerald | 7 451 |
| **Total** | | **33 253** |

## Mine size, per mine

Nine purchases take a mine from level 0 to level 9. Paid in the mine's
own material — on the three two-material mines, in both, on a mix that
slides toward the rare one as the track climbs.

| Mine | First step | Last step | Track total |
| --- | --- | --- | --- |
| Stone | 1C Stone | 33C+32 Stone | 9 207 |
| Coal | 1C Coal | 33C+32 Coal | 9 207 |
| Iron | 1C Iron | 33C+32 Iron | 9 207 |
| Gold | 1C Gold | 33C+32 Gold | 9 207 |
| Lapis | 1C Lapis | 33C+32 Lapis | 9 207 |
| Redstone | 1C Redstone | 33C+32 Redstone | 9 207 |
| Emerald | 1C Emerald | 33C+32 Emerald | 9 207 |
| Diamond | 1C Diamond | 33C+32 Diamond | 9 207 |
| Quartz | 1C Netherrack | 6C+40 Netherrack + 26C+92 Quartz | 9 207 |
| Ancient Debris | 1C Ancient Debris | 33C+32 Ancient Debris | 9 207 |
| Obsidian | 1C Obsidian | 6C+40 Obsidian + 26C+92 Crying Obsidian | 9 207 |
| End | 1C End Stone | 6C+40 End Stone + 26C+92 Amethyst | 9 207 |

## Mine richness, per mine

Nine purchases take a mine from level 0 to level 9. Paid in the mine's
own material — on the three two-material mines, in both, on a mix that
slides toward the rare one as the track climbs.

| Mine | First step | Last step | Track total |
| --- | --- | --- | --- |
| Stone | 1C Stone | 11C+3 Stone | 3 968 |
| Coal | 1C Coal | 11C+3 Coal | 3 968 |
| Iron | 1C Iron | 11C+3 Iron | 3 968 |
| Gold | 1C Gold | 11C+3 Gold | 3 968 |
| Lapis | 1C Lapis | 11C+3 Lapis | 3 968 |
| Redstone | 1C Redstone | 11C+3 Redstone | 3 968 |
| Emerald | 1C Emerald | 11C+3 Emerald | 3 968 |
| Diamond | 1C Diamond | 11C+3 Diamond | 3 968 |
| Quartz | 1C Netherrack | 2C+12 Netherrack + 8C+91 Quartz | 3 968 |
| Ancient Debris | 1C Ancient Debris | 11C+3 Ancient Debris | 3 968 |
| Obsidian | 1C Obsidian | 2C+12 Obsidian + 8C+91 Crying Obsidian | 3 968 |
| End | 1C End Stone | 2C+12 End Stone + 8C+91 Amethyst | 3 968 |

## Boost

One charge costs **3C Redstone**, flat and repeatable with no ceiling. It runs for
30 seconds at x2.5, and firing onto a running boost stacks the duration.

The price is `3 * COST_BASE` rather than a literal, so a rebalance of
the curve moves it instead of leaving it behind — which is the exact
mistake that produced the original imbalance.

## Prestige

The price is a **sum**, not a step on a geometric curve: one climb's
Amethyst income plus a surcharge growing in a straight line. The
multiplier applies to ore yield and XP, and deliberately not to speed.

| Rank reached | Price | Multiplier |
| --- | --- | --- |
| 1 | 61C Amethyst | x1.100 |
| 2 | 63C+20 Amethyst | x1.200 |
| 3 | 65C+40 Amethyst | x1.300 |
| 4 | 67C+60 Amethyst | x1.400 |
| 5 | 69C+80 Amethyst | x1.500 |
| 6 | 72C Amethyst | x1.600 |
| 7 | 74C+20 Amethyst | x1.700 |
| 8 | 76C+40 Amethyst | x1.800 |
| 9 | 78C+60 Amethyst | x1.900 |
| 10 | 80C+80 Amethyst | x2.000 |
| 11 | 83C Amethyst | x2.100 |
| 12 | 85C+20 Amethyst | x2.200 |

---

Generated by `crates/skylode-core/examples/balance_atlas.rs`.
