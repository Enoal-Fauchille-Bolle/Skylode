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
  **Nether at level 15, End at level 30** (tunable). Each level-up also drops a
  small reward (see below).
- **Pickaxe tier unlocks mines and sets speed.** Inside an unlocked world, each
  individual mine is gated by the pickaxe tier that can break its ore (based on
  Minecraft's tool rules, see [worlds and materials](#worlds-and-materials)). The
  pickaxe tier plus Efficiency also determines mining speed.

The two axes interlock: mining yields ores (upgrade the pickaxe, which opens more
mines and mines faster) and XP (which opens the next world). Neither axis alone
carries progression.

### Level-up rewards

Each level-up drops a bundle of **ores or Compressed ore** (scaled to the level)
plus a **temporary boost** (a short Haste window). Level-ups never gate content by
themselves; only the world thresholds do. The reward keeps early levels
satisfying and gives a reason to keep the XP bar moving.

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
  efficiency_bonus = efficiency^2 + 1        (additive)
  haste_multiplier = product of haste sources (multiplicative:
                     permanent Haste enchant * temporary Redstone boost)
```

`mining_power` is a floating-point value so multiplicative haste can be
fractional. Each tick, `break_progress += mining_power`. When
`break_progress >= hardness`, the block breaks, yields its drop times Fortune, and
`break_progress` resets to 0. Efficiency (additive) and Haste (multiplicative) act
on different math layers, so they stack without conflict.

### One block at a time

There is a single `break_progress` counter. The targeted block is a random
remaining cell of the mine grid. On break, the next random cell is picked. In a
mixed-content mine the targeted cell's material decides the drop (see
[mixed content](#mixed-content)).

### Instamine

When `mining_power >= hardness`, a block breaks in a single tick. This is reached
in the endgame with Netherite Efficiency at its cap plus the Haste enchant. Past
instamine, single-target speed saturates at one block per tick, so the endgame
levers shift (see [post-instamine progression](#post-instamine-progression)).

### Fortune

Fortune multiplies the drop count per broken block. It is capped at **10**: past
that point ore is abundant enough that more Fortune adds nothing, so the player
moves to other levers.

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

### Mixed content

A mine can hold more than one material, with rarity weights (for example Obsidian
common with Crying Obsidian rare, or End Stone common with Amethyst rare). The
targeted cell's material decides its drop. A true Vein Miner (following connected
same-type blocks) is **not** planned: it was dropped rather than parked.

### Batch reset

The mine depletes to 0, then fully and instantly refills. This matches SkyMines,
where a cube regenerates as a whole. The 0 threshold is a tunable if the tail ever
drags.

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
| Overworld | Redstone | speed: temporary Haste boosts. Later: auto-miner fuel |
| Overworld | Emerald | Fortune upgrades. Later: currency of a special shop (to be decided) |
| Overworld | Lapis | enchant material (Overworld tier of enchants). True to Minecraft, where lapis is the enchanting currency |
| Nether | Ancient Debris | Netherite tier upgrades |
| Nether | Obsidian, Crying Obsidian | pickaxe enhancement past Netherite. Same mine (Obsidian common, Crying rare) |
| Nether | Quartz | enchant material (Nether tier of enchants) |
| End | End Stone, Amethyst | mixed mine (End Stone common, Amethyst rare). Amethyst is the top enchant material and the prestige currency |

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
- The top tier keeps climbing: **Netherite Efficiency goes 0 to 15** without a
  reset (15 is Pika's instamine point). Past Efficiency 15, the Haste enchant and
  Redstone boosts push the last blocks to instamine.
- Past Netherite, Obsidian and Crying Obsidian gate further enhancement
  (progression-gated, not paid).
- Beyond the upgrade ceiling: prestige.

### Mine gating table

Which pickaxe tier can open which mine follows Minecraft's tool rules. The current
mapping lives in `materials.rs` (`min_pickaxe_tier`): Stone/Coal need Wooden, Iron
needs Stone, Gold/Redstone/Diamond/Emerald need Iron, Lapis needs Stone, Ancient
Debris/Obsidian/Crying Obsidian need Diamond, Quartz and Amethyst are soft (Wooden)
but sit behind their world's XP gate. The End's rich role is carried by Amethyst
requiring late-game reach, not a high tier.

## Enchants

Five special enchants change *how* you mine, not just the numbers. They are
acquired by levels, and the enchant material differs per world, which caps the
enchant level available in each dimension:

- **Overworld enchants use Lapis** (lowest level cap).
- **Nether enchants use Quartz** (higher cap).
- **End enchants use Amethyst** (maximum cap).

All five enchants are available as soon as you can enchant (Overworld, once Lapis
is reachable); progressing to a new world only raises the **level cap**. Every
enchant upgrade costs the world's enchant material **plus a mix of raw ores from
the earlier mines**, which keeps old mines useful as permanent enchant fuel long
after their tier is passed.

Spatial enchants (each radiates from the impact cell; higher level means bigger
effect):

- **Explosive:** breaks a compact blob or square around the impact.
- **Jackhammer:** breaks a full row, then a band of `k` rows at high levels.
- **Nuke:** breaks the whole mine, on a long cooldown (higher level shortens the
  cooldown).

Non-spatial enchants (qualitative on another axis):

- **Excavator:** chance to drop `Compressed <ore>` or Emerald directly instead of
  raw ore.
- **Haste:** permanent mining-speed multiplier `x(1 + 0.2 * level)`, multiplicative
  and distinct from additive Efficiency. This is the endgame instamine lever.

Dropped or merged: Drill (a column dominated by the row), Laser (merged into
Jackhammer), a true Vein Miner (needs mixed-content mines, and mixed content now
exists but Vein Miner was still dropped), and Lucky-Strike / Overclock (variance
and gambling feel, rejected). See [DECISIONS.md](DECISIONS.md).

### Enchant parameters (to tune at implementation)

Each enchant needs, as named tunables: cost per level, the per-world level cap,
the effect scaling per level (blob radius, row band `k`, Haste factor, Excavator
proc rate), and for Nuke the cooldown curve. Values are set and balanced during
implementation, not fixed here.

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
button. It is also cheap to clear, one free action, so it is a beat and not a
wall. The UI is expected to tell the two failures apart — "compress first" and "go
mine more" are different messages, and only one of them is bad news.

- **Cost curve shape:** costs grow geometrically per step (`cost(n) = base *
  growth^n`), split across a Compressed part and a raw remainder. The base and
  growth constants are tunables set at implementation time, not fixed here.
- On PikaNetwork, upgrades are identified by Minecraft enchantment, level, and
  material (for example "Efficiency V Stone Pickaxe"). Our naming convention is to
  be decided.

## Auto-miner and offline progression

### Auto-miner

For the MVP there is a single basic auto-miner: a flat passive mining rate, not a
system (no purchases, no tiers). The full auto-miner system (tiers, like idle-game
"managers") is post-MVP.

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
- **Condition:** reach the End (level 30) and accumulate enough Amethyst.
- **Deep reset:** pickaxe (back to Wooden), Efficiency, Fortune, ore inventory,
  mine sizes, enchant levels, and the mining level (XP) all reset to the start.
- **Persists:** the prestige rank and its permanent global multiplier (ore yield,
  mining speed, and XP gain). The multiplier scale per rank is a tunable.
- **Gate role:** the prestige rank may also gate late content, replacing paid
  ranks. Whether prestige is a pure endless loop or leads to a defined win
  condition is an open question (see [ROADMAP.md](ROADMAP.md)).

A meta-upgrade tree spent from a prestige currency is possible but parked
post-MVP; the MVP ships the flat global multiplier only.
