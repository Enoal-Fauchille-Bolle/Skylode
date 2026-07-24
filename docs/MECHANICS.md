# Skylode - Mechanics

Detailed rules for the game systems that face the player: progression and gating,
mining, worlds and materials, pickaxe progression, enchants, the auto-miner,
offline progression, post-instamine endgame, and prestige. For the high-level
concept and gameplay loop, see [DESIGN.md](DESIGN.md). For technical systems (save
format, tech stack), see [SYSTEMS.md](SYSTEMS.md).

## Progression and gating

Progression runs on two independent axes:

- **Mining level (XP) unlocks worlds.** Mining grants XP. The level rises up to a
  cap of **50**. Reaching a level threshold opens a new world (dimension):
  **Nether at level 15, End at level 30** (tunable). Every level-up pays something
  (see below): a world at those two thresholds, a bundle of ore everywhere else.
- **Pickaxe tier unlocks mines and sets speed.** Inside an unlocked world, each
  individual mine is gated by the pickaxe tier that can break its ore (based on
  Minecraft's tool rules, see [worlds and materials](#worlds-and-materials)). The
  pickaxe tier plus Efficiency also determines mining speed.

The two axes interlock: mining yields ores (upgrade the pickaxe, which opens more
mines and mines faster) and XP (which opens the next world). Neither axis alone
carries progression.

### Level-up rewards

A level-up hands over exactly one **payout**, plus up to two **garnishes** that run
on their own rhythms and ignore which payout fired. Level-ups never gate content by
themselves; only the world thresholds do.

**The payout is a world or a bundle of ore, never both and never neither.** Levels
15 and 30 open a dimension and pay no ore: that world *is* the reward, and a bundle
on top of it would split the announcement in two and dilute the one that matters.
Every other level from 2 to 50 pays ore. Level 1 is where the player starts, so it
is never *reached* and pays nothing at all.

**The whole schedule is a pure function of the level, with no randomness.** The
Levels screen draws the entire 1→50 ladder in advance, including rungs the player
has not reached, so a reward that were drawn could not be shown before it fired —
and freezing the draw would mean carrying a PRNG state in the save for nothing.

#### The ore bundle

The bundle's budget is **`10 × level` raw items**, linear rather than geometric. It
is deliberately *not* on the [cost curve](#upgrade-costs): that curve is indexed by a
track's step (0 to 15 at most), and reading it at a mining level instead would run
the exponent to 50 and hand out more in one level-up than the most expensive purchase
in the game costs. Over a full run the bundles total about 3 % of everything the
player must buy — an opening hand, not an income.

**The bundle mirrors a price.** It quotes the same materials in the same proportions
as the [enchant cost](#enchants) of the matching rung — the world's enchant material
at **50 %**, and that rung's abundant and scarce fuel ores at **35 %** and **15 %** —
so the player recognises what they receive because it is the shape of what they
spend. Sharing that table rather than copying it is also what keeps the two from
drifting: re-balancing enchant fuel re-balances the rewards in the same edit.

| Levels | 50 % | 35 % | 15 % |
| --- | --- | --- | --- |
| 2–6 | Lapis | Stone | Coal |
| 7–10 | Lapis | Iron | Gold |
| 11–14 | Lapis | Gold | Diamond |
| 16–20 | Quartz | Netherrack | Ancient Debris |
| 21–25 | Quartz | Ancient Debris | Obsidian |
| 26–29 | Quartz | Obsidian | Crying Obsidian |
| 31–50 | End Stone and Amethyst, on the ramp below | | |

The End is **two lines, not three**, for the reason its enchant price is: the
dimension holds one mine whose rare cell already *is* the enchant material, so a
third line would be a second line of Amethyst. The two split on the same rare ramp
the End's prices climb — 25 % Amethyst at level 31 rising to 91 % at level 50 — so
the bundle keeps paying in the proportion the player is spending.

**The bundle is credited entirely in raw items**, however large it grows — `+455
Amethyst`, not `4 Compressed + 55`. Minting a [Compressed](#compression) unit is the
player's own move, free and reversible both ways, and a reward that arrived pre-split
would make it for them; the strict two-denomination rule is about *paying*, never about
receiving. What the Levels screen prints is therefore exactly what lands.

#### The garnishes

- **A boost charge every five levels** (5, 10, … 50 — ten in a run), **including at
  15 and 30**. The charge is not a running boost: it is held until the player fires
  it, and every boost in the game is identical, so the reserve is a count and nothing
  more. This is what makes crossing several levels at once — a lump of offline
  experience — safe: charges accumulate instead of burning down in an unwatched
  window. Redstone itself is never granted; the charges are what would have bought.
- **Emerald every three levels**, worth **25 % of the budget on top of it**, so those
  levels are visibly better rather than differently split. Not at 15 and 30: Emerald
  is ore, and those levels pay a world. Emerald earns its own rhythm because
  [Fortune](#fortune) is the one permanent purchase whose currency stops being mined
  once the Overworld is behind the player.

## Mining model

### Ticks

The simulation advances in fixed discrete steps called ticks, at 20 ticks per
second. This matches Minecraft, so break-time formulas port over one to one and
we reuse existing balance values instead of re-deriving them. Rendering is
decoupled from the tick and redraws on change at roughly 30 fps.

Ticks drive break progress, timers (boosts, cube regeneration), XP accrual, and
the basic auto-miner. A fixed tick rate is chosen for determinism and testability:
a balance pass can be validated by simulating N ticks reproducibly (see the seeded
PRNG note below).

### Breaking a block

Breaking is progressive. Each block has a fixed `hardness`. The pickaxe has a
`mining_power`, computed as in Minecraft:

```text
mining_power = (base_tier + efficiency_bonus) * haste_multiplier

  base_tier        = monotone per-tier speed (see pickaxes.rs)
  efficiency_bonus = efficiency^2 + 1 if efficiency > 0, else 0  (additive)
  haste_multiplier = product of haste sources (multiplicative:
                     permanent Haste enchant * temporary Redstone boost)
```

The `efficiency > 0` guard is Minecraft's (`Player.getDigSpeed`), and it is why
the first level of Efficiency is a discrete jump of `+2`: the `+ 1` rides along
with the first level rather than being a flat bonus every pickaxe collects. An
unenchanted Wooden pickaxe is worth 2, not 3.

`mining_power` is a floating-point value so multiplicative haste can be
fractional. Each tick, `break_progress += mining_power`. When
`break_progress >= hardness * 30`, the block breaks, yields its drop times the
[Fortune multiplier](#fortune), and `break_progress` resets to 0. Efficiency (additive) and Haste (multiplicative)
act on different math layers, so they stack without conflict.

The **30** is Minecraft's, and it is the conversion between two scales that are not
the same one: `getDestroyProgress` reads `dig_speed / hardness / 30` per tick and
breaks at `1.0`. Rearranged so the counter carries power instead of a fraction, a
block costs `hardness * 30` and takes:

```text
ticks = ceil(30 * hardness / mining_power)
```

which reproduces the wiki's break times exactly — Stone with a Wooden pickaxe in 23
ticks (1.15 s), Obsidian with a Diamond one in 188 (9.4 s). It is not a tunable: it
is what makes the 1:1 hardness table worth porting, since a hardness ported 1:1
whose break *times* are not is only a coincidence of notation. Minecraft's other
divisor — 100, for mining without the right tool — has no counterpart here: the
[mine gating table](#mine-gating-table) refuses a block below the required tier
outright rather than letting the player chip at it.

### One block at a time

There is a single `break_progress` counter. The targeted block is a random
remaining cell of the mine grid. On break, the next random cell is picked. Every
mine holds two kinds of cell, so the targeted cell's material always decides the
drop (see [mine richness](#mine-richness)).

### Instamine

When `mining_power >= hardness * 30`, a block breaks in a single tick. This is not a
single moment the endgame arrives at: **each block crosses its own threshold**, and
the hardness table spreads those thresholds from 12 (Netherrack) to 1500 (Obsidian).
Netherite at Efficiency 15 already one-shots the Overworld's ores and dense blocks;
the hardest two stay out of reach even with Haste at its cap, and only the temporary
Redstone boost closes that last gap (see below). Past instamine, single-target speed
saturates at one block per tick, so the endgame levers shift (see
[post-instamine progression](#post-instamine-progression)).

Instamine is **not a special case in the code**: a power at or above the threshold
simply satisfies the same check on its first tick. The saturation is what the
discarded leftover buys — progress resets to 0 rather than carrying the overshoot
into the next block, so no amount of power clears more than one cell per tick.

Netherite at Efficiency 15 is worth 235, which instamines the Overworld ores (90)
and the dense blocks (150) but not Ancient Debris (900) or Obsidian (1500): those
are what the Haste enchant and the Redstone boost are for — **the two together, not
one each**. Even Haste at its highest cap tops the pickaxe out at 705, so the last
two blocks stay out of reach of permanent upgrades *entirely*, and only the
temporary boost closes the gap. That is the staging this table is meant to have: a
ceiling the player cannot buy their way past, which is what leaves the boost a job
and the endgame a lever. The exact rungs are phase-10 balance, but that ordering is
not one of them — `mine`'s
`a_hasted_netherite_instamines_the_dense_blocks_but_not_the_obsidian` pins it.

### Fortune

Fortune multiplies the drop count per broken block by **`1 + level`**: an
un-fortuned pickaxe multiplies by 1 and takes exactly what the block contained, and
Fortune 10 is worth eleven times the block, not ten. It is capped at **10**: past
that point ore is abundant enough that more Fortune adds nothing, so the player
moves to other levers. That ceiling is **reached in three steps, not one** —
Fortune shares [`World::enchant_cap`](#enchants) with the five specials, so it is
capped at 3 in the Overworld, 6 in the Nether and 10 in the End. Fortune
multiplies the **loot** and only the loot — see XP, below.

The multiplier is **exact and drawn from nothing**. Minecraft rolls Fortune at
random, per block; in an idle game a draw that fires on every break is averaged flat
by the thousandth block, so the player never sees the variance — only the mean —
while the roll costs a PRNG draw per swing and, with it, reproducibility. The place
randomness earns its keep is a *rare, legible* event, which is exactly what the
Excavator proc and the spatial-enchant bursts are. Fortune is the steady lever and
states its own number.

It also applies to **every block equally**, including the dense forms and the
Obsidian and Ancient Debris of the endgame. Minecraft exempts anything that drops
itself, which would leave Fortune inert precisely where
[post-instamine progression](#post-instamine-progression) needs it, and would make
[richness](#mine-richness) shrink Fortune's reach rather than scale it. The 1:1
fidelity to Minecraft is kept for hardness only — see [DECISIONS.md](DECISIONS.md).

### XP

XP is a **property of the block, granted before Fortune** — `Block::xp_value`, not
what the player walks away with. Fortune and Excavator multiply or substitute the
loot; neither touches the experience.

That "before Fortune" is what holds the two progression axes apart. Levels open
worlds, ore opens pickaxes. If Fortune multiplied XP as well as loot, a single
investment would advance both axes at once, and *"neither axis alone carries
progression"* would quietly stop being true. Fortune is a yield lever, and stays
one.

**Every block a swing brings down pays, including the cells a blast takes.** That is
not the door Fortune was just refused through, and the difference is what falls:
Fortune multiplies the yield of *one* block, while Explosive, Jackhammer and Nuke
make *more blocks fall*. That is the same kind of gain as breaking faster with
Efficiency, which nobody expects to leave the level bar alone — an enchant that
cleared a grid and paid XP for one cell of it would be an ore purchase that *slows*
the level axis down. It also keeps the `base × (1 + 2w)` identity below true of a
grid however the procs happened to land, which is what phase 10 balances the twelve
bases against.

**XP is its own table, not the drop count.** It once was the drop count — one per
ore cell, nine per dense one — which had a consequence nobody chose: the three
endgame mines are exactly the three with no dense form, so their cells granted one
apiece while an Iron Block granted nine, and the Iron mine out-levelled the End.
Each mine now has a base that rises with the progression, from Stone at 1 to the End
at 24, and **a cell of value is worth three times its mine's common cell** — three
rather than nine, so the dial still moves the level bar without the crafting ratio
deciding the level curve.

That ordering is a *property* rather than a balance pass: a full grid is worth
`base × (1 + 2w)` for a dial weight `w`, so it stays proportional to the base at
every dial setting, and rising bases order the twelve mines at all settings at once.
It holds per grid, and per second once a boost is running. It does **not** hold per
second without one: Ancient Debris and Obsidian take 67 and 70 seconds a grid where
every other mine takes ten. That is the gap the [Redstone boost](#instamine) exists
to close, so the one regime where the order breaks is the one the player is meant to
spend Redstone on.

Enriching a mine still speeds up levelling as well as income — see
[mine richness](#mine-richness).

### Randomness

All randomness (enchant procs, Excavator drops, mixed-mine cell types) draws from
a seeded PRNG whose state lives in the save file — `rand_chacha::ChaCha8Rng`, seeded
via `seed_from_u64` — and never from OS entropy. This keeps ticks and `#[test]`
balance runs reproducible while enchants still fire in real bursts. `ChaCha8Rng`
rather than `StdRng` because only the former guarantees the same sequence across
`rand` releases; see [SYSTEMS.md](SYSTEMS.md#tech-stack).

## Mine representation and interaction

### Active-continuous mining

The player holds Space to mine. This is not spam-tapping and not Melvor-style
idle. While Space is held, `break_progress += mining_power` each tick; releasing
stops it. Idle accrual comes only from the auto-miner (see below), which is a
separate system.

### The grid is the model

The 2D grid is the model, not just a view. The core stores the position of every
block (a bitset per material), because the spatial enchants (Explosive,
Jackhammer, Nuke) need the geometry. `block_count` is derived (remaining cells)
and `capacity = W * H`. The core owns the grid and the TUI renders it (see the
core/TUI split in [SYSTEMS.md](SYSTEMS.md)).

### Mine size

Each mine has its **own size**, from **3x3** up to a maximum of **20x10 = 200**
cells. A mine's size is **upgraded by spending that mine's own ore**, so every
mine has a self-contained growth goal funded by what it produces. Each block
renders on 2 character columns (`##`) for a roughly square look, so a full 20x10
mine is 40x10 characters.

Grid size is a game constant per mine, decoupled from terminal size, so the window
size cannot change balance. The terminal is sized for the largest mine (20x10);
smaller mines occupy less of the same area. The minimum terminal is about **80x24**
(40x10 for the biggest grid, plus borders, the status strip, and margins); smaller
shows an "enlarge your terminal" screen.

Size is one of a mine's **two** upgrade tracks; the other is
[richness](#mine-richness). Both are throughput, which is exactly why they must
multiply *different* things or they would be two names for one number. Size is what
makes the **spatial enchants** scale — a Jackhammer clears a 20-wide row instead of
a 5-wide one, a Nuke clears 200 cells instead of 25. Richness is what makes each
cell worth more, so it scales with **Fortune**. Which track to buy first therefore
depends on which enchants the player has invested in, and that is a decision rather
than a division.

### Mine richness

A mine is a **common cell** and a **cell of value**, held in rarity weights. The
Iron mine is Iron Ore with Iron Block; the Obsidian mine is Obsidian with Crying
Obsidian; the End mine is End Stone with Amethyst. The targeted cell's material
decides its drop. **Richness is the weight of the cell of value**, and it is the
mine's second upgrade track.

This is not a system bolted on top of mixed content — it *is* mixed content, made
mutable. A mine at richness 0 is precisely the mixed mine as first specified;
buying richness shifts weight from the common cell to the valuable one. One
mechanic, one cost model, and it reaches every mine in the game, including the three
that have no dense form.

It is also the only way the **dense blocks** enter the game at all. `Iron Block`,
`Cobblestone` and their siblings are tougher cells worth nine raw (see
[compression](#compression)); with no richness track, no mine would ever contain
one, and the whole dense form would be decoration.

**Buy the ceiling, set the dial.** A richness *level* is bought: permanent,
geometric in cost, one-way. Below that ceiling the player moves a *dial* freely and
for free. This is the [compression](#compression) rule again, and it is here for the
same reason: whatever shape a run is in, one free action puts it in the shape the
player's current goal wants. A purchase may slow a run down; it must never be able
to strand it.

**Two flavours, deliberately.** Where the valuable cell is the *dense form of the
same material* — nine mines: Stone, Coal, Iron, Gold, Lapis, Redstone, Emerald,
Diamond, Ancient Debris — `Iron Ore` and `Iron Block` both drop Iron. Enriching
there is **pure gain**: at the current hardness values a dense cell pays nine for a
hardness of five, against one for a hardness of three, so it is worth roughly five
times as much iron per second. The dial has exactly one sensible position, the top,
and a dial with one position is not a dial: the UI does not draw it.

Where the valuable cell is a *different material* — three mines: Netherrack/Quartz
Ore, Obsidian/Crying Obsidian, End Stone/Amethyst — enriching is a **substitution**:
more of the rare, less of the common. There the dial is a genuine choice, and the UI
draws it. Two of the three are the endgame mines (Obsidian and End), where the
decision carries the most weight; the Quartz mine trades its own growth currency
(Netherrack) against the Nether's enchant material. The rules stay uniform across
all twelve; only the interface branches.

**The End mine is the sharpest case.** End Stone funds that mine's own growth;
Amethyst funds prestige and the End enchant cap. The dial arbitrates between them —
*grow, or harvest*. Since Amethyst is already dual-use, the End mine becomes a
three-way call on the scarcest resource in the game: cash out (prestige), power up
(enchant cap), or reinvest (richness).

**The dial also changes a mine's speed, and not always in the player's favour.** The
two cells of a mine can differ in hardness, so shifting weight between them shifts how
long a grid takes. Enriching the Quartz mine makes it **2.4x slower** (Netherrack 0.4
against Quartz Ore 3.0); enriching the End mine makes it **1.5x slower** (End Stone 10
against Amethyst 15, phase 10 — see [pickaxe progression](#pickaxe-progression) on why
the End is hardened); the Obsidian mine is unaffected, both its cells being 50. For the
Quartz mine this falls out of keeping Minecraft's hardness table 1:1; for the End it is
now designed. Either way it reads as a genuine trade — rarer ore, slower grid — so
enriching the End is a real cost, not the free speed bonus its soft-Amethyst version
used to hand out. It also puts the prestige currency behind the *slowest* cells, which
is what makes Netherite's Efficiency worth buying to farm it.

**The dial, not a weight cap, is what keeps a run from stranding.** An earlier
version of this design capped the valuable cell's weight strictly below 100% and
called it a load-bearing invariant — the reasoning being that a 100%-Amethyst End
mine would stop dropping the End Stone that pays to grow it, and brick the run. But
the dial is *free and reversible*: a player who over-enriched simply slides the
setting back down, refills mostly common cells, harvests the End Stone, and slides
back up. The rescue is always one free action away, so no weight cap is needed to
prevent a brick — the dial already does it. This is the same **no free action may
put a broken block back** rule at work: free to re-shape the *remaining* grid,
never free to un-break what is gone.

Two consequences follow, and neither depends on a strict-sub-100% cap:

- **It still cannot brick a run.** Whatever a mine is enriched to, richness setting
  0 is always reachable for free and is mostly common cells, so the growth currency
  is always one dial-move away. A purchase that hurts is a mistake; a purchase that
  strands is a bug — and the free dial is what rules the bug out.
- **It still cannot run away.** The production gain from richness is bounded (a
  finite value-cell weight, whatever it is), while the cost curve is geometric and
  unbounded, so the cost always wins in the end — including on the mines where
  richness is paid partly in the very material it produces (see
  [mine upgrade costs](#mine-upgrade-costs)).

The actual weight of the valuable cell per richness level is therefore an ordinary
**tunable**, not an invariant: the core computes it from a provisional formula
(`value_weight`), and phase 10 balance sets its final shape. The one structural rule
the core does enforce is far weaker — the two cell weights are never *both* zero, so
the composition always describes a valid distribution.

A true Vein Miner (following connected same-type blocks) is **not** planned: it was
dropped rather than parked.

### The canonical mines

Twelve mines, one per resource the economy needs. Each is a common cell plus a cell
of value; the world it sits in, the pickaxe tier that gates it, and the materials it
yields all follow from that pair (both cells of a mine always share a world and a
gating tier). This is the list the `MineKind` registry encodes.

| World | Mine | Common cell | Value cell | Flavour |
| --- | --- | --- | --- | --- |
| Overworld | Stone | Stone | Cobblestone | same material |
| Overworld | Coal | Coal Ore | Coal Block | same material |
| Overworld | Iron | Iron Ore | Iron Block | same material |
| Overworld | Gold | Gold Ore | Gold Block | same material |
| Overworld | Lapis | Lapis Ore | Lapis Block | same material |
| Overworld | Redstone | Redstone Ore | Redstone Block | same material |
| Overworld | Emerald | Emerald Ore | Emerald Block | same material |
| Overworld | Diamond | Diamond Ore | Diamond Block | same material |
| Nether | Quartz | Netherrack | Quartz Ore | two materials |
| Nether | Ancient Debris | Ancient Debris | Netherite Block | same material |
| Nether | Obsidian | Obsidian | Crying Obsidian | two materials |
| End | End | End Stone | Amethyst | two materials |

The eight Overworld mines are pure same-material (ore + dense form): an Overworld
ore mine never has filler as its common cell, or unlocking it would drop the player
into breaking mostly-valueless Stone (see [DECISIONS.md](DECISIONS.md)). The Nether
Quartz mine is the one place Netherrack — otherwise the sole material with no
economic function — earns a role: it is that mine's common cell and its growth
currency, with Quartz Ore as the value. Netherrack and Quartz gate behind Diamond,
with the rest of the Nether's mines, and behind the Nether's level-15 XP gate.

### Batch reset

The mine depletes to 0, then fully and instantly refills. This matches SkyMines,
where a cube regenerates as a whole. The 0 threshold is a tunable if the tail ever
drags.

### Mines persist

Every mine keeps its own grid, holes included. Leaving a mine and coming back finds
it exactly as it was left. Regenerating a mine on entry would hand out a free batch
reset: break the four Amethyst cells out of two hundred, leave, come back to a full
grid, break them again. Depleting the mine *is* the price of the refill, and
switching screens must not pay it for you.

The same rule governs the richness dial. Moving it re-rolls the composition of the
**remaining** cells at once — the player sees the change immediately — but it leaves
the holes exactly where they are. One rule covers both, and every free action added
later has to answer to it:

> **No free action may ever put a broken block back.**

What this leaves open is cosmetic rather than economic: a player can wiggle the dial
to re-roll the *geometry* of what remains, until the valuable cells happen to sit
under an Explosive. It costs nothing but patience. This is knowingly accepted for
the MVP — the game is single-player and offline, with no leaderboard, so the only
person a re-roller cheats is themselves — and it closes by deferring the dial to the
next regeneration if it ever proves to matter.

### Break feedback

A progress bar below the grid, in a status strip alongside mining power and the
active boost, is the stable readout. The targeted cell is highlighted and shows a
crack glyph (`.:#` progression by break percentage). This is mostly visible early
or on hard blocks (Obsidian and similar) and becomes irrelevant at instamine.

### Input

The mouse is not used to mine: that would break the idle, offline, and
determinism model. The keyboard selects the mine. The mouse may later serve menu
navigation only.

## Worlds and materials

Worlds are the progression spine, unlocked by mining level. Each world gives its
own set of functions, not just bigger numbers. Within a world, mines are gated by
pickaxe tier.

**Overworld** (from the start): the basics plus the first enchant material.
**Nether** (level 15): the top pickaxe tier, post-Netherite enhancement, and the
second enchant material.
**End** (level 30): the richest final mine and the prestige material.

| World | Material | Function |
| --- | --- | --- |
| Overworld | Stone, Coal, Iron, Gold, Diamond | pickaxe tier upgrades |
| Overworld | Redstone | speed: temporary Redstone boosts. Later: auto-miner fuel |
| Overworld | Emerald | Fortune upgrades. Later: currency of a special shop (to be decided) |
| Overworld | Lapis | enchant material (Overworld tier of enchants). True to Minecraft, where lapis is the enchanting currency |
| Nether | Ancient Debris | Netherite tier upgrades |
| Nether | Obsidian, Crying Obsidian | pickaxe enhancement past Netherite. Same mine (Obsidian common, Crying rare). The enhancement consumes both, so where the recipe wants a ratio of the two, the richness dial has an *optimum* rather than a maximum |
| Nether | Quartz | enchant material (Nether tier of enchants) |
| End | End Stone, Amethyst | one mine (End Stone common, Amethyst rare), and the sharpest [richness](#mine-richness) dial in the game. End Stone funds the mine's own growth; Amethyst is the top enchant material and the prestige currency |

Redstone (speed), Emerald (Fortune), and the enchant materials (Lapis, Quartz,
Amethyst) each own a distinct function, so no two ores are redundant.

## Pickaxe progression

- Tiers: Wooden -> Stone -> Iron -> Gold -> Diamond -> Netherite (top tier).
- `base_tier` speed follows a **monotone custom curve** (each tier is strictly
  faster than the last). The "Minecraft 1:1" principle applies to `hardness`, not
  to tier speed: Minecraft's own tool speeds are non-monotone (gold outruns
  diamond), which would make a whole tier a regression instead of a short dip.
- Within a tier, Efficiency goes from 0 to 5. Jumping to the next tier resets
  Efficiency and temporarily lowers mining speed (the "dip"). Tune the dip so it
  stays short and clearly worth taking.
- The top tier keeps climbing: **Netherite Efficiency goes 0 to 15** (15 is Pika's
  instamine point). Past Efficiency 15, the Haste enchant and Redstone boosts push
  the last blocks to instamine.
- That climb is **priced in two halves, each on its own reset of the curve**.
  Efficiency 1→5 is the ordinary tier upgrade, paid in Ancient Debris like the tier
  itself, on curve steps 0→4. Efficiency **6→15 is the post-Netherite enhancement**,
  and it *is paid* — in Obsidian **and** Crying Obsidian both, the two materials of
  the Obsidian mine — on its **own** curve steps 0→9, restarted from zero the way a
  tier jump restarts Efficiency for every other tier, and on its **own gentler slope**
  (`1.10`, not the `1.45` the tier jumps and Efficiency 1→5 share). Without that reset
  the fifteenth level read curve step 14, an Obsidian wall roughly six times its
  neighbours (phase 10 measured a reference speedrun spending ~37 h of a 39 h run on
  it alone); with it, the enhancement climbs its own short ladder. The separate slope
  is the second half of the same fix: the enhancement is a ten-step track only a
  completionist buys, so pricing it apart from the ≤5-step climbs every run makes is
  what let the pacing pass shrink it without touching the speedrun — bringing a
  max-everything run's ceiling from ~5.4 h down to ~2.3 h. The one-off dip in
  *price* at Eff 5→6 is the same kind a tier jump already makes. It is mostly the
  common Obsidian with a Crying share climbing from a quarter to the dial's own
  ceiling, so the enhancement consumes both and the Obsidian mine's
  [richness](#mine-richness) dial has a *moving* optimum ratio to farm toward (see
  [upgrade costs](#upgrade-costs)). The enhancement is folded into the Efficiency
  climb, not a separate track — **but it no longer gates prestige** (see
  [Prestige](#prestige)), so buying it is now optimisation, not obligation.
- Beyond the upgrade ceiling: prestige.

### Mine gating table

Which pickaxe tier can open which mine follows the two-axis gate. The mapping lives
in `block.rs` (`min_pickaxe_tier`): Stone/Coal need Wooden, Iron needs Stone,
Gold/Redstone/Diamond/Emerald need Iron, Lapis needs Stone, the **whole Nether** —
Ancient Debris/Obsidian/Crying Obsidian **and Quartz** — needs Diamond, and the
**End's End Stone/Amethyst needs Netherite**. Amethyst is the prestige currency and
the top enchant material, so gating it at the top of the ladder makes reaching it
proof of a full tier climb — and it is what finally gives Netherite a mine to open,
where the Overworld's ladder tops out at Iron.

## Enchants

Five special enchants change *how* you mine, not just the numbers. They are
acquired by levels, and the enchant material differs per world, which caps the
enchant level available in each dimension:

- **Overworld enchants use Lapis** (lowest level cap: **3**).
- **Nether enchants use Quartz** (higher cap: **6**).
- **End enchants use Amethyst** (maximum cap: **10**).

All five enchants are available as soon as you can enchant (Overworld, once Lapis
is reachable); progressing to a new world only raises the **level cap**. **Fortune
is capped by the world too** — its ceiling of 10 is unchanged, but it is reached
3 / 6 / 10 like the specials rather than being available in full from level 1.
Efficiency is the only enchant the world does not cap; the pickaxe tier does.

A special enchant's price is the world's enchant material **plus the two ores of
that level's rung** — an abundant one and a scarce one, drawn from the mines the
player is working *now*: Stone and Coal at level 1, Iron and Gold at 2, Gold and
Diamond at 3, then Netherrack and Ancient Debris, Ancient Debris and Obsidian,
Obsidian and Crying Obsidian. The pair is keyed by the **level**, never by where
the player stands, so level 1 costs Stone and Coal whether it is bought in the
first minute or from the End. The three lines share one total, 50 / 35 / 15.

The End is the exception its geography forces: one mine, whose rare cell *is* the
enchant material. Levels 7 to 10 therefore quote **two** lines, End Stone and
Amethyst, the Amethyst share climbing as the level does — which is what finally
gives End Stone a use beyond its own mine.

**Fortune costs Emerald and nothing else.** Its *price* is keyed to neither the
world nor the tier, so it has no "current rung's ore" to consume — but its *cap*
is the world's, like the specials'.

The cap is **one number per world, shared by all five specials and by Fortune** —
not a cap per `(enchant, world)` pair. It is the *gate*: how much the player may invest. What
the investment buys is the enchant's own effect scaling, below. Keeping the two
apart is what lets every world hand out the same budget while the five enchants
stay wildly different; an effect that grows too fast by level 10 is a fault in its
own curve, and capping that one enchant lower would fix a curve with the wrong
tool and leave the player an asymmetry to explain. In the code the cap is
therefore `World::enchant_cap`, a rule of the world, not of any enchant.

Efficiency alone sits outside this: it is capped by the **pickaxe tier** (5, or 15
at Netherite). The two groups — keyed by tier, keyed by world — are what keep the
two progression axes independent. If a world also raised Efficiency's ceiling, one
investment would advance both axes and the two-axis gate would collapse into one,
and Netherite's cap of 15 would be unreachable outside the End, deleting the final
tier's whole reward. Fortune is keyed by neither axis, so none of that argument
ever applied to it — it is world-capped with the specials, and the code's
`EnchantType::max_level` is a two-way dispatch to match.

Every **triggered** special enchant — Explosive, Jackhammer, Nuke, Excavator —
fires on a **random proc**, not on every break: a swing that lands a block rolls
once per enchant, and a higher level raises that roll's chance. This is the *rare,
legible burst* [Randomness](#randomness) reserves the PRNG for, the same role the
Excavator proc already played. The draw is seeded, so it is reproducible, and it
fires on **active mining only** — the auto-miner is a flat closed-form rate and
never procs, so the enchants pay out for playing, not for idling. Haste is the
exception: a passive permanent multiplier, always on, that does not proc.

Spatial enchants each radiate from the impact cell (the block just broken):

- **Explosive:** breaks a compact **square** (Chebyshev) around the impact. Level
  raises both the proc chance and the square's radius, in three bands aligned with
  the world caps — up to **3x3** in the Overworld (cap 3), **5x5** in the Nether
  (cap 6), **7x7** in the End (cap 10). Capped there so it never approaches Nuke's
  whole-grid clear.
- **Jackhammer:** breaks a **full row** — the mine's whole width, so *mine size* is
  what scales its reach. Level raises only the proc chance; the row is always one
  cell tall, which is what keeps it distinct from Explosive's square.
- **Nuke:** breaks the **whole mine**; its geometry never changes with level, only
  the proc chance does. **No cooldown** — clearing the grid is its own limiter,
  since a re-proc finds nothing to break until the batch reset refills.

A blast breaks whatever standing cells its shape covers, with **no tier check**: a
mine can never hold a cell its gating tier cannot break (`MineKind` guarantees it),
so a blast has no un-mineable cell to catch. The impact cell is already a hole by
the time the blast runs, and the shape clips itself at holes and edges alike — no
special case, no bounds check.

Non-spatial enchants (qualitative on another axis):

- **Excavator:** on a proc, substitutes **one `Compressed <ore>` of the mined
  material** for the block's whole raw drop. Its proc is the model the three spatials
  now follow, and it is the only thing in the game that mints a Compressed unit
  without paying its 100 raw.

  **Fortune does not multiply it.** *Substitutes* is meant at full strength: the proc
  replaces the drop rather than joining it, so a proc is worth a flat 100 raw whatever
  else the pickaxe carries. Composing them would put the rarest burst in the game
  under its largest multiplier — 11 Compressed, 1100 raw, from one swing at the caps —
  and a windfall that swings by a factor of eleven stops being legible to the player
  and starts dominating every balance number near it.

  It rolls **once per swing, on the block the player actually broke**, never on the
  cells a blast takes. A maxed Nuke drops two hundred cells in a tick; rolling each
  would make the number of draws per swing depend on a blast's geometry, and a PRNG
  sequence whose shape varies with the grid is one no golden vector can pin.

  An **Emerald** was the alternative half of an earlier draft ("a Compressed unit or
  an Emerald") and is dropped. It read as a premium currency once; in Skylode it is
  one Overworld material among eight, so on almost every mine it would have been the
  strictly worse side of a coin flip — an outcome the player suffers rather than
  understands.
- **Haste:** permanent mining-speed multiplier `x(1 + 0.2 * level)`, multiplicative
  and distinct from additive Efficiency. This is the endgame instamine lever, and —
  as above — the one special that does not proc.

Dropped or merged: Drill (a column dominated by the row), Laser (merged into
Jackhammer), a true Vein Miner (needs mixed-content mines, and mixed content now
exists but Vein Miner was still dropped), and Lucky-Strike / Overclock (variance
and gambling feel, rejected). See [DECISIONS.md](DECISIONS.md).

### Enchant parameters (to tune at implementation)

Each enchant needs, as named tunables: cost per level, its **proc-chance curve**
per level (Explosive, Jackhammer, Nuke and Excavator each proc, more often as they
climb), the Explosive **square-radius bands** (3x3 / 5x5 / 7x7), and the Haste
factor. There is **no Nuke cooldown curve** — Nuke is a proc like the others.
Values are set and balanced during implementation, not fixed here.

The per-world level cap is **no longer one of them**: it is set (3 / 6 / 10, above)
and shared by all five, so there is one ceiling per world rather than one per
enchant. The numbers themselves stay provisional until balance, but their *order*
is not — a world that raised no ceiling would leave its enchant material buying
nothing.

The **Haste factor is bounded above as well as below**, and the ceiling is the
easier one to cross by accident. Permanent upgrades alone must stay short of Ancient
Debris: `235 × (1 + 0.2 × 10) = 705`, under its instamine threshold of 900. Push the
factor to 0.3 and the maxed pickaxe reaches 940, takes Ancient Debris for good, and
leaves the Redstone boost with no work left to do — which is its whole reason to
exist. See [Instamine](#instamine).

## Compression

The inventory holds a material in two denominations: **raw**, as mined, and
**Compressed**, in bundles of 100. `1 Compressed Iron = 100 Iron`.

Compressing is a **player action**, not a display format. The inventory never
converts behind the player's back; they choose when to trade 100 raw for one
Compressed unit, and they can trade back. The conversion is **free and lossless in
both directions**, which is what keeps a run un-brickable: whatever shape a
player's stock is in, one action puts it in the shape a cost wants.

The one thing that mints a Compressed unit without paying 100 raw for it is the
**Excavator** enchant, which is exactly why it is worth having.

A Compressed unit is **not** a *dense block*. A dense block (`Iron Block`,
`Cobblestone`) is a grid cell you mine: tougher than the ore, and it drops 9 raw.
A Compressed unit is worth 100, is minted in the inventory, and no pickaxe ever
produces one. Nine versus a hundred, mined versus minted.

## Upgrade costs

Every pickaxe upgrade costs a mix of Compressed and raw ore, for example
`6 Compressed Iron + 50 Iron`. The denomination is there for readability: small
composite numbers read better than one large number like `650 Iron`.

**Costs are paid in the denomination they are quoted in.** A player sitting on 650
raw Iron cannot buy `6 Compressed Iron + 50 Iron` — the value is there, the
denomination is not, and they must compress first. That refusal is deliberate: it
is what makes compressing a step in the upgrade path rather than a cosmetic
button. Clearing it is one free action, but a *separate, deliberate* one — never
folded into the purchase — so the refusal keeps its teeth: it rewards the player
who keeps a denomination ready over the one who mints it at the point of sale.
The UI is expected to tell the two failures apart — "compress first" and "go
mine more" are different messages, and only one of them is bad news.

- **Cost curve shape:** costs grow geometrically per step (`cost(n) = base *
  growth^n`), split across a Compressed part and a raw remainder. The constants are
  tunables set at implementation time, not fixed here — but there is **one pair per
  track**, not one for the game. A slope compounds over however many steps a track
  has and is only meaningful against that track's own production growth: size takes a
  mine from 9 cells to 200 across nine steps, while Netherite's Efficiency runs
  fifteen steps and multiplies nothing. One slope for both leaves the short track
  free or the long one unaffordable.
- **The base governs the early game, the slope governs the late one.** Step zero
  costs the base whatever the slope, so raising a slope to make the game harder
  inflates only the endgame. This is why the enchant ladder carries a *high* base and
  the *gentlest* slope: its ten levels are split 3 / 3 / 4 across the worlds, and a
  steep curve would make the Overworld's three a rounding error, leaving the ores
  that fuel them barely demanded.
- **Most costs are a single material; several are a mix.** A price is a list of
  lines, one per material. The ordinary pickaxe upgrades and the nine same-material
  mine tracks quote a single line. Four prices quote more, and all four **share** one
  total rather than adding lines on top of it: a special enchant (world material plus
  its rung's two ores), both tracks of a two-material mine, Netherite's Efficiency
  6→15 (Obsidian plus Crying Obsidian), and the End's enchants.
- **Where a price mixes two materials, the rare share climbs with the step.** One
  ramp, ending at the same 91 % the richness dial reaches at its own ceiling — so the
  dial setting a recipe wants *moves up* as the player climbs it, and the mine's top
  rung is exactly what the track's last step asks for. Pinned at a fixed fraction, as
  Netherite's enhancement once was, the optimum never moves and most of that mine's
  richness track is not worth buying.
- On PikaNetwork, upgrades are identified by Minecraft enchantment, level, and
  material (for example "Efficiency V Stone Pickaxe"). Our naming convention is to
  be decided.

### Mine upgrade costs

Both mine tracks — [size](#mine-size) and [richness](#mine-richness) — are paid **in
that mine's own material**, on the same geometric curve, so every mine funds its own
growth out of what it produces.

On the three mines that hold two materials, **both tracks** cost a **mix that shifts
as it climbs**: mostly End Stone at the low levels, increasingly Amethyst at the
high ones. The cost curve therefore tracks the mine's own production curve — each
level is paid mostly in whatever the mine currently makes most of. Size shares that
shape with richness because "the mine's own material" means *both* of a two-material
mine's materials; reading it as the common cell alone left Crying Obsidian paying
for nothing but the pickaxe.

That shift trades one brake for a better one. Paying purely in the common material
would be self-limiting by arithmetic: enriching dries up the currency that buys
enrichment. Paying the top levels in the rare material instead puts richness in
**direct competition with prestige**, which is what the player is farming Amethyst
for in the first place. The brake stops being a mechanic and becomes a decision, and
a decision is worth more. The compounding this invites — Amethyst buys richness buys
Amethyst — is closed by the [richness cap](#mine-richness): a bounded production gain
against an unbounded cost curve.

The nine single-material mines have nothing to mix, but they show the same shape in
the other denomination: the opening steps are quoted mostly in raw items, and the
Compressed part grows to dominate as the track climbs — exactly as the pickaxe costs
read.

## Auto-miner and offline progression

### Auto-miner

For the MVP there is a single basic auto-miner: a flat passive mining rate, not a
system (no purchases, no tiers). The full auto-miner system (tiers, like idle-game
"managers") is post-MVP. It runs **at all times**, not only while the player is
idle: idle accrual *comes from* the auto-miner, which says where passive income
originates, not that the helper downs tools the moment the player picks theirs up.
One that did would tax playing actively.

**It pays ore, and never XP.** Levels open worlds and ore opens pickaxes, and
neither axis alone carries progression — so an auto-miner that granted experience
would open the Nether and the End to a player who was away for a week and never
mined. That collapses the two-axis gate into a clock. It is the same rule that keeps
[Fortune](#fortune) off the experience, applied to elapsed time instead of to an
upgrade, and it sits beside the one already settled for the
[triggered enchants](#enchants), which fire on active mining only because the
closed-form auto-miner cannot draw. Playing pays ore, XP and procs; being away pays
ore.

**It never walks the grid**, online or offline. It reads the *expected* composition
from the mine's [richness dial](#mine-richness) and multiplies, so an idle mine does
not visibly empty. That is the price of having one model instead of two, and what it
buys is that a player who watches for an hour and one who is away for an hour are
paid identically — which a grid-walking online path and a closed-form offline one
could not promise.

### Offline accrual

The game does not run in the background. Store `last_seen`; on start, compute
`elapsed = now - last_seen`, cap it, and credit what the basic auto-miner would have
produced over that span.

This is a **multiplication, not a replay**. The MVP auto-miner is a flat passive rate
(see above), so its output over an absence is `rate × elapsed` — there is nothing a
tick-by-tick simulation would discover that the closed form does not already give,
and 7 days of absence would mean stepping over 12 million ticks to find it out. The
seeded PRNG earns its keep on the interactive tick loop and in balance tests, not
here. Should the auto-miner ever gain state that compounds (post-MVP tiers), this is
the decision to revisit.

Two independent offline levers:

- **Cap:** the maximum duration counted is 7 days (tunable). A clock set far ahead
  yields at most 7 days.
- **Offline rate:** fixed at 100% of the online production rate (no reduction),
  combined with the 7-day cap.

### Clock handling

- Forward jump (potential cheat): bounded by the cap above.
- Backward jump (`now < last_seen`; legitimate causes include DST, timezone, and
  NTP correction): clamp `elapsed` to 0, do not penalize the player (no reset, no
  cheater flag), and log the event (stderr or log file) for the developer only.
- Use the wall clock (`SystemTime`), not a monotonic clock, because offline spans
  reboots. The cap and clamp handle its quirks.

## Post-instamine progression

At instamine, single-target speed saturates: a block breaks in one tick, so raw
speed cannot increase further. The endgame levers therefore shift to throughput
and value, not speed:

- **Mine size:** a bigger mine holds more blocks per cycle, and crucially it makes
  the **spatial enchants scale**. A Jackhammer clears a 20-wide row instead of a
  5-wide one; a Nuke clears 200 blocks instead of 25. Spatial enchants are the
  main throughput multiplier once instamine is reached.
- **[Mine richness](#mine-richness):** more value per cell. This lever *strengthens*
  at instamine, because the extra hardness of a dense cell stops costing anything —
  one block per tick whatever it is made of — so a dense cell becomes a flat nine
  for one. It also stacks multiplicatively with Fortune.
- **Fortune:** more drops per block, up to the cap of 10.
- **Ore value:** the End's Amethyst is the highest-value ore, mined toward
  prestige.
- **Prestige:** the reset loop for a permanent multiplier.

## Prestige

Prestige is a voluntary reset of the run in exchange for a permanent global
multiplier. It is not present in SkyMines; it is the endgame loop that gives
Skylode replayability after instamine, and it replaces the paid-rank gate of the
source.

- **Currency:** Amethyst, the rare ore of the End. The player farms the End, then
  spends Amethyst to prestige. Amethyst is dual-use (push the End enchant cap or
  prestige), which creates a real spending choice.
- **Condition:** a **fully realised run** — the mining level at its cap (50) and a
  Netherite pickaxe — plus enough Amethyst to pay. Reaching the End (level 30) is no
  longer sufficient: with the End's ore [gated behind Netherite](#mine-gating-table),
  the old condition left the shortest path to prestige an XP race to level 30 that
  never climbed a pickaxe tier (the balance harness measured that floor at ~2.6 h).
  Requiring the full climb puts the two-axis gate back on the prestige itself.
  **Efficiency 15 is deliberately *not* a condition** (phase 10): it was redundant
  with the Amethyst price, which already forces reaching and working the End, and it
  was the sole source of the mono-mine Obsidian grind — a reference speedrun spent
  ~37 h of a 39 h run on it. Dropping it makes Netherite's Efficiency 6→15 pure
  optimisation, and lets the speedrunner (skip it, farm Amethyst) and the
  completionist (max it) diverge. The progression gates are checked **before** the
  price, since Amethyst only drops past the level gate; `Player::prestige_lock`
  reports which are still shut, the shape `MineLock` takes for the mine gate.
- **Deep reset:** pickaxe (back to Wooden), Efficiency, Fortune, ore inventory,
  mine sizes, **mine richness**, enchant levels, and the mining level (XP) all reset
  to the start. Richness goes with size because it is the second track of the same
  object, funded in the same currency: keeping it would make the first prestige
  nearly painless on mines, and re-walking the progression is the point. Three more
  go with them, for the same reason rather than as an afterthought: the **boost
  reserve** (ore already converted, plus the charges the erased levels granted), the
  **auto-miner's carries**, and the **mines left behind** — a run that kept its
  visited mines would hold a richness-9 End grid its level-1 player is no longer
  allowed to enter.
- **Persists:** the prestige rank and its permanent global multiplier (ore yield,
  mining speed, and XP gain). The multiplier scale per rank is a tunable. The
  **seeded generator persists too**, and it is the one thing that must: its position
  is run state, so rewinding it with the rest would deal the player back an identical
  second run — same grids, same procs — which is the opposite of what re-walking the
  progression is for.
- **Shape of the multiplier:** additive per rank (`1 + k × rank`), quoted and applied
  as an **integer in permille**, never a float. Every yield in the game is a whole
  number, so a `×1.2` applied to a one-ore drop truncates to `×1.0` and the first rank
  is worth nothing through exactly the stretch it exists to shorten. Two rules make it
  survive: the multiplier is applied **once per swing over the swing's total**, not
  per block; and the **fraction is carried** to the next swing rather than dropped.
  The auto-miner is the exception that proves it — there the multiplier lands once on
  the *rate*, before the microblock split, where the carries that already exist absorb
  the fraction and the online and offline paths stay one multiplication rather than
  two.
- **Gate role:** the prestige rank may also gate late content, replacing paid
  ranks. Whether prestige is a pure endless loop or leads to a defined win
  condition is an open question (see [ROADMAP.md](ROADMAP.md)).

A meta-upgrade tree spent from a prestige currency is possible but parked
post-MVP; the MVP ships the flat global multiplier only.
