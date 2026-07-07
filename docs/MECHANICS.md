# Skylode - Mechanics

Detailed rules for the game systems that face the player: mining, worlds and
materials, pickaxe progression, enchants, the auto-miner, offline progression,
and prestige. For the high-level concept and gameplay loop, see
[DESIGN.md](DESIGN.md). For technical systems (save format, tech stack), see
[SYSTEMS.md](SYSTEMS.md).

## Mining model

### Ticks

The simulation advances in fixed discrete steps called ticks, at 20 ticks per
second. This matches Minecraft, so break-time formulas port over one to one and
we reuse existing balance values instead of re-deriving them. Rendering is
decoupled from the tick and redraws on change at roughly 30 fps.

Ticks drive break progress, timers (boosts, cube regeneration), and the basic
auto-miner. A fixed tick rate is chosen for determinism and testability: a
balance pass can be validated by simulating N ticks reproducibly (see the seeded
PRNG note below).

### Breaking a block

Breaking is progressive. Each block has a fixed `hardness`. The pickaxe has a
`mining_power`, computed as in Minecraft:

```text
mining_power = (base_tier + efficiency_bonus) * haste_multiplier

  base_tier        = Minecraft tool speed (see pickaxes.rs)
  efficiency_bonus = efficiency^2 + 1        (additive)
  haste_multiplier = product of haste sources (multiplicative:
                     permanent Haste enchant * temporary Redstone boost)
```

Each tick, `break_progress += mining_power`. When `break_progress >= hardness`,
the block breaks, yields its drop times Fortune, and `break_progress` resets to
0. Efficiency (additive) and Haste (multiplicative) act on different math layers,
so they stack without conflict.

### One block at a time

There is a single `break_progress` counter. The targeted block is a random
remaining cell of the mine grid. This is cosmetic: mines are mono-material, so
which cell is targeted does not affect yield. On break, the next random cell is
picked.

### Instamine

When `mining_power >= hardness`, a block breaks in a single tick. This is the
endgame goal. Past instamine, the visual focus shifts from the break bar (nothing
left to animate) to the grid emptying and enchant procs.

### Fortune

Fortune multiplies the drop count per broken block.

### Randomness

All randomness (enchant procs, Excavator drops) draws from a seeded PRNG whose
state lives in the save file (`rand`, for example `StdRng::seed_from_u64`), not
from OS entropy. This keeps ticks, offline replay, and `#[test]` balance runs
reproducible while enchants still fire in real bursts.

## Mine representation and interaction

### Active-continuous mining

The player holds Space to mine. This is not spam-tapping and not Melvor-style
idle. While Space is held, `break_progress += mining_power` each tick; releasing
stops it. Idle accrual comes only from the auto-miner (see below), which is a
separate system.

### The grid is the model

The 2D grid is the model, not just a view. The core stores the position of every
block (for example a `20x10` bitset), because the spatial enchants (Explosive,
Jackhammer, Nuke) need the geometry. `block_count` is derived (remaining cells)
and `capacity = W * H`. The core owns the grid and the TUI renders it (see the
core/TUI split in [SYSTEMS.md](SYSTEMS.md)).

### Grid size

Fixed for the MVP at `20x10 = 200` blocks. Each block renders on 2 character
columns (`##`) for a roughly square look, since terminal cells are about 1:2,
giving 40x10 characters. Grid size is a game constant, decoupled from terminal
size, so window size cannot change balance. The minimum terminal is about 80x24;
smaller shows an "enlarge your terminal" screen. There are no mine-size upgrades
at MVP: they would raise the minimum terminal size and break the rest of the UI.
Per-world growth is parked for post-MVP.

### Content

Mines are mono-material at MVP. Parked for post-MVP: seeding a mine with richer
blocks (for example diamond blocks among diamond ores), which would revive a true
Vein Miner that follows connected same-type blocks.

### Batch reset

The mine depletes to 0, then fully and instantly refills. This matches SkyMines,
where a cube regenerates as a whole. The 0 threshold is a tunable if the tail
ever drags.

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

Worlds are the progression spine. They replace depth, which does not exist in
SkyMines. Each new world unlocks materials that open new functions, not just
bigger numbers.

- **Overworld:** Stone, Coal, Iron, Gold, Redstone, Diamond, Emerald
- **Nether:** Ancient Debris, Obsidian, Crying Obsidian
- **End:** Amethyst (plus more, to be decided)

| Material | Function |
| --- | --- |
| Stone, Coal, Iron, Gold, Diamond | Pickaxe tier upgrades |
| Redstone | Speed: haste boosts (temporary mining speed). Later: auto-miner fuel/energy, overclock |
| Emerald | Fortune upgrades. Later: currency of a special shop (to be decided) |
| Ancient Debris | Top pickaxe tier upgrades |
| Obsidian, Crying Obsidian | Pickaxe enhancement past Netherite (progression-gated) |
| Amethyst | Special pickaxe enchants: Explosive, Jackhammer, Nuke, Excavator, Haste (see below) |

## Pickaxe progression

- Tiers: Wooden -> Stone -> Iron -> Gold -> Diamond -> Netherite (top tier).
- Within a tier, Efficiency goes from 0 to 5. Jumping to the next tier resets
  Efficiency and temporarily lowers mining speed. Tune this dip so it stays short
  and clearly worth taking: SkyMines shows a failure case (a roughly 10-hour
  Efficiency-0 grind) to avoid.
- The top tier keeps climbing past Efficiency 5 toward instamine.
- Past Netherite, Obsidian and Crying Obsidian gate further enhancement
  (progression-gated, not paid).
- Beyond the upgrade ceiling: prestige.

### Enchants

Amethyst applies special enchants, acquired by levels as in Minecraft (cost
scales per level). On a uniform, mono-material 2D grid the classic prison enchants
collapse geometrically: a "layer" is a row, a "vein blob" is a square, and a long
row dominates a short column. The set is therefore trimmed to 5 distinct enchants.

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
  and distinct from additive Efficiency. This is the endgame instamine lever. It
  is a pure number enchant and does not count toward anti-monotony variety.

Dropped or merged: Drill (a column dominated by the row), Laser (merged into
Jackhammer), a true Vein Miner (needs mixed-content mines, parked post-MVP), and
Lucky-Strike / Overclock (variance and gambling feel, rejected). See
[DECISIONS.md](DECISIONS.md).

### Upgrade costs (composite: compressed plus raw)

Every pickaxe upgrade costs a mix of compressed blocks and raw ore, for example
`6 Compressed Iron + 50 Iron`. Compression here is a denomination for
readability: small composite numbers read better than one large number like
`650 Iron`. It is not inventory management, since stacks are unlimited. A
compressed block is just a fixed bundle of raw ore.

- The denomination is named `Compressed <ore>` (for example Compressed Iron).
- On PikaNetwork, upgrades have no abstract "tier 1/2" names; they are identified
  by Minecraft enchantment, level, and material (for example "Efficiency V Stone
  Pickaxe"). Our naming convention is to be decided.

### Upgrade surface (avoiding monotony)

Without armor, sword, or bow, the upgrade targets are: pickaxe tier, Efficiency,
Fortune, amethyst enchants (qualitative abilities), the auto-miner, haste boosts,
and prestige. The anti-monotony lever is the spatial enchants: Explosive and
Jackhammer change how you mine (bursts and cleared rows), not just a flat
multiplier. A small set of upgrade types is fine as long as pacing (new worlds,
the tier-jump decision, prestige) is good. The enchant and research surface can
be expanded post-MVP if playtesting feels thin.

## Auto-miner and offline progression

### Auto-miner

For the MVP there is a single basic auto-miner: a flat passive mining rate, not a
system (no purchases, no tiers). The full auto-miner system (tiers, like idle-game
"managers") is post-MVP.

### Offline accrual

The game does not run in the background. On launch it replays elapsed time. Store
`last_seen`; on start, compute `elapsed = now - last_seen`, simulate the basic
auto-miner over `elapsed` (capped), and credit the result.

Two independent offline levers:

- **Cap:** the maximum duration counted is 7 days (tunable). A clock set far
  ahead yields at most 7 days.
- **Offline rate:** fixed at 100% of the online production rate (no reduction),
  combined with the 7-day cap.

### Clock handling

- Forward jump (potential cheat): bounded by the cap above.
- Backward jump (`now < last_seen`; legitimate causes include DST, timezone, and
  NTP correction): clamp `elapsed` to 0, do not penalize the player (no reset, no
  cheater flag), and log the event (stderr or log file) for the developer only.
- Use the wall clock (`SystemTime`), not a monotonic clock, because offline spans
  reboots. The cap and clamp handle its quirks.

## Prestige

Reset progression for a permanent multiplier. This is not present in SkyMines.
Curves and unlocks are to be decided. The prestige level may also serve as the
progression gate that replaces paid ranks.
