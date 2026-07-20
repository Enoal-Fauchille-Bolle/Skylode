# Skylode - Phases

The dependency-ordered build plan for `skylode-core`, phase by phase. The TUI is
out of scope here. These phases are *derived from* the design documents
([DESIGN.md](DESIGN.md), [MECHANICS.md](MECHANICS.md), [SYSTEMS.md](SYSTEMS.md),
[DECISIONS.md](DECISIONS.md), [ROADMAP.md](ROADMAP.md)) by diffing them against the
code as it stands: the static data (worlds, blocks, materials, tiers, enchant caps)
is largely in place and tested, while **none of the dynamic mechanics exist yet** —
no tick, no block breaking, no RNG draws, no costs, no save. This document states
each phase's *objective* and the ordering that binds them. What ships in the MVP
lives in [ROADMAP.md](ROADMAP.md); the *why* behind each rule lives in
[DECISIONS.md](DECISIONS.md) — PHASES.md stays focused on order and intent.

Module names follow the code, which is singular and folds progression into
`player` (`world`, `block`, `material`, `inventory`, `mine`, `pickaxe`, `enchant`,
`player`, `rng`, `error`). [SYSTEMS.md](SYSTEMS.md#core-modules) still lists the
older plural sketch (`worlds`, `pickaxes`, `progression`, …); where the two differ,
the code's names win.

## Hard ordering constraints

Only three dependencies are truly rigid. Everything else can be reshuffled.

1. **RNG before mine generation.** The PRNG state lives in the save so that ticks
   and `#[test]` balance runs are reproducible, and so a reloaded run continues its
   sequence rather than rerolling it. (Not for offline accrual — that is a
   closed-form multiplication and draws nothing.) Writing mine generation, the
   spatial enchants, or the auto-miner against ambient randomness first would mean
   rewriting every balance test later.
2. **Grid accessors before spatial enchants.** Explosive, Jackhammer and Nuke are
   geometry over the grid; they cannot exist before the grid can be read and mutated
   cell by cell.
3. **Everything before `GameState` / `tick`.** There is no struct that owns "the run
   in progress", so `tick(input)` has no `self` to live on. It is the keystone, and
   keystones go in last.

## Phase 0 - Reconcile code with the settled decisions

Bring the code back in line with decisions already recorded, before building on top
of it. Three reconciliations: make `PickaxeTier::base_power` a strictly monotone
curve so a tier jump is a short dip and never a permanent regression (see
[DECISIONS.md](DECISIONS.md)); separate the two "compressed" concepts by name — a
**dense block** (`IronBlock`, `Cobblestone`, …) is a mineable, tougher grid cell,
whereas a **Compressed unit** is 100 raw, never mined, minted by hand (see
[MECHANICS.md](MECHANICS.md#compression)); and settle the filler-block rule so every
block drops something, making `Block::material` total and removing the unreachable
`None` branch every caller would otherwise carry.

## Phase 1 - Deterministic foundations

Lay the primitives every later phase draws on. Introduce the `rng` module — a seeded
PRNG (`ChaCha8Rng`, chosen because `rand` makes no cross-release promise for
`StdRng` and the save stores a *position in a sequence*; see
[SYSTEMS.md](SYSTEMS.md#tech-stack) and
[MECHANICS.md](MECHANICS.md#randomness)) whose state is threaded explicitly as
`&mut Rng`, so no path can consume randomness without saying so in its signature.
Grow the `error` module so every rule that can refuse returns `Result` and a refusal
changes nothing — no partial debit, no half-applied upgrade — which is what the
phase-5 purchase path must be able to assume. Add the `tunables` module as the one
home for the constants [ROADMAP.md](ROADMAP.md) leaves open (world-unlock levels,
level cap, offline cap, compression ratio, cost-curve base and growth, autosave
interval).

## Phase 2 - The mine model

The largest gap, and where the RNG first draws. Give mines an identity (a kind /
registry: block pool, world, gating pickaxe tier, the material that pays for size
and richness), then replace the uniform grid with one drawn from **weights**, where
the weight of the valuable cell *is* the mine's richness — mixed content is richness
at level 0, not a separate feature (see
[MECHANICS.md](MECHANICS.md#mine-richness)). Add the grid accessors
(`remaining_count`, `capacity`, `is_empty`, `get`, cell removal) that later phases
read, keeping `block_count` derived and never stored
([MECHANICS.md](MECHANICS.md#the-grid-is-the-model)). Add progressive breaking (a
single `break_progress` accumulating `mining_power` until `hardness`), the instamine
path, and the batch reset that refills at zero remaining. **Invariant:** the two
cell weights are never *both* zero, so the composition always describes a valid
distribution — and that is the *only* structural rule the core enforces here. The
valuable cell's weight per level is an ordinary tunable (`value_weight`) that phase
10 sets: an earlier version of this design made a strict-sub-100% cap a
load-bearing invariant, and [DECISIONS.md](DECISIONS.md) reversed it, because both
of the jobs it did are done elsewhere — the free, reversible dial is the anti-brick,
and the geometric cost curve is the anti-runaway. Moving the richness dial re-rolls
the *remaining* cells while leaving broken ones broken: no free action may ever put a
broken block back.

## Phase 3 - The real mining-power formula

Turn mining power from a stub into the spec formula, which unlocks instamine and
therefore the endgame. Fold the Haste multiplier into `mining_power`
(`(base_tier + efficiency² + 1) × haste_multiplier`, the product of the permanent
Haste enchant and any temporary boost; see
[MECHANICS.md](MECHANICS.md#instamine)). Add the mining gate `can_mine(block)` on
pickaxe tier ≥ the block's minimum, wiring up the `Ord` on `PickaxeTier` that exists
but nothing calls. Apply Fortune to drops, capped at 10
([MECHANICS.md](MECHANICS.md#fortune)).

## Phase 4 - Enchant effects

Give the five enchants their effects, most of them geometry over the phase-2 grid.
Enchant level caps are per-world (Lapis < Quartz < Amethyst) — **done**: one shared
ceiling per world rather than one per enchant, because the cap gates how much may be
invested and each enchant's own scaling decides what that buys. The spatial enchants
are **done** as well: Explosive (a Chebyshev square around the impact, growing in
three bands aligned with the world caps, so a 7x7 is proof of the End), Jackhammer
(one full-width row, scaled by mine size rather than by level) and Nuke (the whole
grid, at any level). All three fire on a seeded **proc** whose frequency climbs with
the level, rolled in a fixed order that a save replays; Nuke has no cooldown, since
emptying the mine is its own limiter. They compute and break shapes but are **not
wired to a tick** — ordering a swing as impact → procs → refill is phase 7's.
Excavator closes the phase: a proc that substitutes one Compressed unit of the mined
material for the block's whole raw drop, unmultiplied by Fortune, rolled once per
swing on the impact block. It reshapes no cell, so it resolves in `enchant` rather
than on the mine, and draws **after** the three spatials — an order the two halves are
tested against rather than produced by a single loop (see
[MECHANICS.md](MECHANICS.md#enchants)).

## Phase 5 - Economy

Make upgrades cost something. Add the `economy` module with a geometric cost curve
`cost(n) = base × growth^n`, split into a Compressed part plus a raw remainder,
covering pickaxe upgrades, enchant upgrades, and both mine tracks — size and richness
— each paid in that mine's own material (see
[MECHANICS.md](MECHANICS.md#upgrade-costs)). Replace the currently free
`Pickaxe::upgrade` with a transactional purchase path that checks affordability,
debits, and returns `Result`, plus the affordability / buy-×N / buy-max queries the
Upgrades screen reads. Add temporary Haste (Redstone) boosts with a tick-based timer,
feeding the phase-3 multiplier.

## Phase 6 - Progression

Wire mining level to the two-axis gate. Cap the level at 50, stopping
`Player::add_experience` from climbing without bound. Add world unlocks at the
threshold levels (Nether, then End) and the set of unlocked worlds in the state.
Grant level-up rewards (an ore / Compressed bundle plus a short Haste window). Grant
XP on break — **per item the block *contained*, before Fortune** (1 for an ore cell,
9 for a dense one): Fortune and Excavator act on the loot and must not touch XP, or
one investment would advance both axes and the two-axis gating would collapse into
one (see [MECHANICS.md](MECHANICS.md#progression-and-gating)).

## Phase 7 - The runtime core

The keystone. Introduce `GameState`, the missing aggregate that owns player, mines
per world, selected mine, active boosts, RNG state, prestige rank, and `last_seen`.
Add `tick(input)`, the fixed 20 tps step applying held-Space mining, the auto-miner,
boost timers, XP, and enchant procs — all drawn from the seeded RNG
(see [SYSTEMS.md](SYSTEMS.md#tick-loop)). The spatial procs themselves already exist
(phase 4); wiring them up means calling them after a break and ordering the swing
**impact → procs → refill**, which moves the batch reset out of the break that
empties the grid and to the end of the step — a blast may empty the mine too, and a
refill in the middle would drop a full grid under the enchants that have not rolled
yet. Add a basic flat-rate auto-miner (tiers and purchases are post-MVP); it never
procs, being credited in closed form. Credit offline accrual in **closed form** — `rate × elapsed`
capped at the offline cap, *not* a tick replay, since a flat rate makes a replay a
multiplication done the long way — and clamp a backward clock jump to 0. Core reads no
wall clock: the caller injects `now`, or core stops being deterministic (see
[MECHANICS.md](MECHANICS.md#offline-accrual)).

## Phase 8 - Prestige

Add the `prestige` module: the condition (reach the End, accumulate enough Amethyst),
the deep reset (pickaxe, Efficiency, Fortune, enchants, inventory, mine sizes, mine
richness, XP), and the surviving rank with its permanent global multiplier on ore
yield, mining speed and XP gain. `Player::prestige` is a `u32` that nothing yet
increments (see [MECHANICS.md](MECHANICS.md#prestige)).

## Phase 9 - Save (serialisation half only)

Make the state persistable, keeping the core pure. Add serde derives on every
persisted type, including the PRNG state; a versioned save struct with
`to_json` / `from_json`, testable without a filesystem; and a migration hook keyed on
the `version` field. The HMAC, the atomic write, the `.bak` recovery, and the clock
reading stay **outside** core (in `skylode-tui` or a dedicated crate), so core keeps
its "pure, no I/O, deterministic" contract and its tests never touch disk (see
[SYSTEMS.md](SYSTEMS.md#save-system)).

## Phase 10 - Balance

Tune the numbers against the now-deterministic engine. Write simulation tests of the
form "N ticks ⇒ this level, this inventory", made possible precisely by the phase-1
determinism, and use them to fix the final values of the tunables left open in
[ROADMAP.md](ROADMAP.md#open-questions).
