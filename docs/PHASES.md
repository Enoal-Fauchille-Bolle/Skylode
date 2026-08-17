# Skylode - Phases

The dependency-ordered build plan for **both crates**, phase by phase: `skylode-core`
under [Core](#core), `skylode-tui` under [Front-end](#front-end). This document states
each phase's objective and the ordering that binds it. What ships in the MVP lives in
[ROADMAP.md](ROADMAP.md); the *why* behind each rule lives in
[decisions/](decisions/) — PHASES.md stays focused on order and intent.

The two halves were arrived at differently, and saying so is the point of keeping them
in one file. The core's phases were **derived** from the design documents
([DESIGN.md](DESIGN.md), [MECHANICS.md](MECHANICS.md), [SYSTEMS.md](SYSTEMS.md),
[decisions/](decisions/), [ROADMAP.md](ROADMAP.md)) by diffing them against the code.
The front-end's were a build order for frames [UI.md](UI.md) already specified in full,
so they are listed as a sequence rather than as objectives.

## Core

**Status: phases 0 to 11 have shipped.** The sentence this paragraph once carried —
*"none of the dynamic mechanics exist yet: no tick, no block breaking, no RNG
draws, no costs, no save"* — described the diff that produced the plan, and every
one of those now exists. What is left in core is the last of phase 10's tunables —
the ones no harness exercises (proc rates, offline cap, dip magnitude, the XP curve)
— and the *deepening* of `GameState::validate` that phase 11 listed and deliberately
left: the cross-field plausibility checks that reconcile an inventory and a level
against the new counters. Those want balance bounds nothing has measured yet, and a
`validate` tightened on a guess refuses honest runs. Each phase's heading keeps the
objective it was written with; where a phase's implementation forced a decision the
objective did not anticipate, the decision is recorded under that heading rather than
rewritten into it.

Module names follow the code, which is singular and folds progression into `player`.
The list lives in [SYSTEMS.md](SYSTEMS.md#core-modules) and nowhere else. This
paragraph used to carry a second copy of it, plus a note that SYSTEMS.md's was stale —
a document annotating another document's rot instead of repairing it, which is the
habit this whole directory was reorganised to break.

### Hard ordering constraints

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

### Phase 0 - Reconcile code with the settled decisions

Bring the code back in line with decisions already recorded, before building on top
of it. Three reconciliations: make `PickaxeTier::base_power` a strictly monotone
curve so a tier jump is a short dip and never a permanent regression
([0017](decisions/0017-base-tier-speed-is-a-monotone-custom-curve.md)); separate the
two "compressed" concepts by name — a
**dense block** (`IronBlock`, `Cobblestone`, …) is a mineable, tougher grid cell,
whereas a **Compressed unit** is 100 raw, never mined, minted by hand (see
[MECHANICS.md](MECHANICS.md#compression)); and settle the filler-block rule so every
block drops something, making `Block::material` total and removing the unreachable
`None` branch every caller would otherwise carry.

### Phase 1 - Deterministic foundations

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

### Phase 2 - The mine model

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
load-bearing invariant, and
[0054](decisions/0054-richness-has-no-weight-cap-the-value-cell-weight-per.md)
reversed it, because both of the jobs it did are done elsewhere — the free,
reversible dial is the anti-brick,
and the geometric cost curve is the anti-runaway. Moving the richness dial re-rolls
the *remaining* cells while leaving broken ones broken: no free action may ever put a
broken block back.

### Phase 3 - The real mining-power formula

Turn mining power from a stub into the spec formula, which unlocks instamine and
therefore the endgame. Fold the Haste multiplier into `mining_power`
(`(base_tier + efficiency² + 1) × haste_multiplier`, the product of the permanent
Haste enchant and any temporary boost; see
[MECHANICS.md](MECHANICS.md#instamine)). Add the mining gate `can_mine(block)` on
pickaxe tier ≥ the block's minimum, wiring up the `Ord` on `PickaxeTier` that exists
but nothing calls. Apply Fortune to drops, capped at 10
([MECHANICS.md](MECHANICS.md#fortune)).

### Phase 4 - Enchant effects

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

### Phase 5 - Economy

Make upgrades cost something. Add the `economy` module with a geometric cost curve
`cost(n) = base × growth^n`, split into a Compressed part plus a raw remainder,
covering pickaxe upgrades, enchant upgrades, and both mine tracks — size and richness
— each paid in that mine's own material (see
[MECHANICS.md](MECHANICS.md#upgrade-costs)). Replace the currently free
`Pickaxe::upgrade` with a transactional purchase path that checks affordability,
debits, and returns `Result`, plus the affordability / buy-×N / buy-max queries the
Upgrades screen reads. Add temporary Haste (Redstone) boosts with a tick-based timer,
feeding the phase-3 multiplier.

### Phase 6 - Progression

Wire mining level to the two-axis gate. Cap the level at 50, stopping
`Player::add_experience` from climbing without bound. Add world unlocks at the
threshold levels (Nether, then End). The unlocked set is **derived from the level,
not stored**: it is a monotone function of state the save already holds, so a second
copy would only be an invariant to maintain by hand — and prestige, which resets the
mining level, re-locks the worlds for free rather than needing to clear anything.
The lock a mine reports names *what is owed* (a level, a tier, or both) rather than
a bare boolean, because the Mines screen prints the requirement.
Grant level-up rewards. A level-up pays exactly one **payout** — a world at 15 and
30, a bundle of ore everywhere else — plus garnishes on their own rhythms: a boost
**charge** every fifth level, Emerald every third. The schedule is a pure function of
the level with no randomness, because the Levels screen draws rungs the player has
not reached yet. The bundle shares the enchant fuel table rather than owning one, and
its budget is linear in the level (see
[MECHANICS.md](MECHANICS.md#level-up-rewards)).

Grant XP on break — **per block, before Fortune**, from `Block::xp_value`: Fortune
and Excavator act on the loot and must not touch XP, or one investment would advance
both axes and the two-axis gating would collapse into one (see
[MECHANICS.md](MECHANICS.md#progression-and-gating)).

### Phase 7 - The runtime core

The keystone. Introduce `GameState`, the missing aggregate that owns the player, the
mines, the mine the player is in, the boost reserve and any running boost, the RNG
state, and `last_seen`. **Not the prestige rank**: it lives on `Player`, beside the
level phase 8 resets it with — the same reason the unlocked worlds are not a field.
Add `tick(input)`, the fixed 20 tps step applying held-Space mining, the auto-miner,
boost timers, XP, and enchant procs — all drawn from the seeded RNG
(see [SYSTEMS.md](SYSTEMS.md#tick-loop)). The spatial procs themselves already exist
(phase 4); wiring them up means calling them after a break and putting the refill
**last** — the shipped order is on `GameState::tick`, which grew two more steps than
this objective anticipated — which moves the batch reset out of the break that
empties the grid and to the end of the step — a blast may empty the mine too, and a
refill in the middle would drop a full grid under the enchants that have not rolled
yet. Add a basic flat-rate auto-miner (tiers and purchases are post-MVP); it never
procs, being credited in closed form, and it **grants no experience** — levels open
worlds, so an absence must not (see [MECHANICS.md](MECHANICS.md#auto-miner)). Credit
offline accrual in **closed form** — `rate × elapsed`
capped at the offline cap, *not* a tick replay, since a flat rate makes a replay a
multiplication done the long way — and clamp a backward clock jump to 0. Core reads no
wall clock: the caller injects `now`, or core stops being deterministic (see
[MECHANICS.md](MECHANICS.md#offline-accrual)).

### Phase 8 - Prestige

Close the loop. Add the `prestige` module — the price of a rank and the multiplier it
grants, as pure functions — and put the reset itself on `GameState`, which is the only
thing that owns the nine fields it clears. The condition is the two halves
[MECHANICS.md](MECHANICS.md#prestige) names, checked in that order: the End's unlock
level first, then the Amethyst, through the same two-pass till every purchase in the
game pays through. The level goes first because Amethyst only drops in the End, so
quoting a price to a player thirty levels short of the ore answers the wrong question.

The deep reset takes the pickaxe, Efficiency, Fortune, every enchant, the inventory,
every mine's size and richness, and the mining level — plus three the design's list
does not name and that would otherwise survive by omission: the boost reserve, the
auto-miner's carries, and the mines left behind. It does **not** take the RNG, whose
*position* is run state — rewinding it would deal the player back an identical run —
nor `last_seen`, since prestiging is neither a save nor an absence.

The multiplier is an **integer in permille, applied once per swing, with the fraction
carried**. Applied per block it would truncate to nothing on a one-ore drop, which is
exactly the player who has just prestiged; the auto-miner instead takes it once on its
*rate*, where the microblock carries already there absorb the fraction and the online
and offline paths stay one multiplication. `Player::prestige` is finally a `u32` that
something increments.

### Phase 9 - Save (serialisation half only)

Make the state persistable, keeping the core pure. Add serde derives on every
persisted type, including the PRNG state; a versioned save with
`to_json` / `from_json`, testable without a filesystem; and a migration hook keyed on
the `version` field. The HMAC, the atomic write, the `.bak` recovery, and the clock
reading stay **outside** core (in `skylode-tui` or a dedicated crate), so core keeps
its "pure, no I/O, deterministic" contract and its tests never touch disk (see
[SYSTEMS.md](SYSTEMS.md#save-system)).

Four things the shape of the code decided, none of them optional once the first save
is written:

- **The version lives inside the signed payload**, not in the envelope
  [SYSTEMS.md](SYSTEMS.md#integrity-hmac) sketches. The MAC covers `data` alone, so a
  version outside it is the one field a tamperer can edit freely — and it is exactly
  the field that selects which migration runs.
- **The maps are ordered, not hashed.** A `HashMap`'s iteration order is unspecified,
  so the same run would write a different text on every save: no golden save could
  pin it, and no two saves could be diffed. `BTreeMap` makes "the same state writes
  the same bytes" a property of the type.
- **An `Item` is written as a word** — `"iron"`, `"compressed_iron"` — because JSON
  object keys must be strings and the inventory is keyed by item. The key table is
  separate from the display name, so the UI can reword "End Stone" without
  invalidating every file on disk.
- **A load validates before it returns.** Deserialisation writes private fields
  directly, so it is the one input that reaches the state without passing a single
  rule; a file describing a dial above its ceiling, a grid that is not the size it
  claims, or a level off the ladder is **refused**, never repaired. Clamping would
  hand the player a run that is not the one they saved, and the recovery screen's
  backup is seconds old.

The configuration the save carries stays a **type parameter**: the core transports the
front-end's preferences without ever learning what a palette is.

### Phase 10 - Balance

Tune the numbers against the now-deterministic engine. Write simulation tests of the
form "N ticks ⇒ this level, this inventory", made possible precisely by the phase-1
determinism, and use them to fix the final values of the tunables left open in
[ROADMAP.md](ROADMAP.md#where-this-stands).

### Phase 11 - The counters the Stats screen reads

The one core gap the front-end found that no earlier phase predicted, and it is
small: [UI.md](UI.md) §5.5 prints three figures nothing counts. Add them to
`GameState`, and note that they are **two lifetimes, not one** — `blocks_broken`
and `playtime` are totals that survive a prestige, while the run's own elapsed time
is cleared by it, exactly like the nine fields phase 8 already resets. The `This
run` panel beside them needs no state at all: every row is a pure predicate over
the run (a tier reached, a mine maxed, a level crossed), which is why the design
carries **no "ever achieved" bitset** and the save schema gains no such field.

**`blocks_broken` counts what the player broke, not what the auto-miner credited.**
The impact block and the cells a blast brought down, and nothing else. The helper
never walks the grid — it weights the expected composition and multiplies — so its
"blocks" are a closed-form quotient rather than cells that visibly fell, and a
counter mixing the two would answer neither question a player is asking of it.
Excluded, the figure means *swings*, which is what someone comparing it against
their own memory has in mind, and it stays reachable from the swing path alone. The
consequence to keep straight is that an absence adds nothing to it — exactly as an
absence adds nothing to `playtime`, which counts simulated tick time.

**No `SAVE_VERSION` bump, and the reason was a fact about the calendar rather than
about the schema.** A version bump exists to protect files already written, and at the
time the front-end wrote none — the whole disk half of
[phase 9](#phase-9---save-serialisation-half-only) was still owed. There was therefore
no v1 file in existence for a migration to carry forward, so writing one would have
meant shipping a step that could never run, plus a test describing a situation that
could not occur. **That window closed on 2026-08-04**, when TUI phase 8 gave `persist`
its caller: files exist now, so the next schema change *is* the first real bump, and it
carries a migration. `#[serde(default)]` was refused for a
sharper reason: a default of `0` for `blocks_broken` would be *false* of an older
file rather than merely absent, which is the one thing the
[`unclaimed` precedent](decisions/0099-a-level-up-announces-it-does-not-pay-the-reward-is.md)
was careful to establish it was not. The
golden save moves instead, which is exactly the signal it exists to give.

**What shipped, and the one thing that did not.** The three counters, their two
lifetimes, the exclusion of the auto-miner and the golden save are done, and
`validate` gained the single invariant the pair offers for free — a run cannot have
lasted longer than the save that holds it. The **cross-field plausibility work is
deliberately deferred**: reconciling an inventory or a level against `blocks_broken`
needs bounds on what a swing can be worth, and every one of those is a phase-10
tunable nothing has measured. A `validate` tightened against a guess refuses honest
saves, which is a worse failure than the tampering it would catch.

## Front-end

Ten phases, 0 to 9, shipped between 2026-07-18 and 2026-08-09. All of them are done.

**The ordering was a core constraint, and that constraint is gone.** The list was
written while the core stopped at static data, so its shape *was* that limit: the TUI
could reach static layout and no further, because everything past phase 2 needed
queries landing in core phases 5 to 7. Core phases 0 to 11 have since shipped, and no
core dependency is left anywhere in the list. What ordered the tail instead was the
front-end's own dependency — Settings needs a save, and a save needs a session that
can load one.

| Phase | What it is | Landed |
| --- | --- | --- |
| 0 | app shell, screen ring, overlay stack, `keymap` to `Action` | `d0ccc05`, 2026-07-18 |
| 1 | the palette and the cell widget | `1920e8e`, 2026-07-24 |
| 2 | static layout of all six screens, plus the pulled overlays | 2026-07-26 |
| 3 | the Mine screen driven by a real run | `3d2f98e`, 2026-07-28 |
| 4 | the Mines screen | `8356448`, 2026-07-28 |
| 5 | Inventory, compression, and the three-state refusal | `66f2aa6`, 2026-07-28 |
| 6 | Upgrades: three sub-tabs, the mark column, the dip modal | `44bde22`, 2026-07-29 |
| 7 | the tick loop, and everything that consumes `Vec<GameEvent>` | `0c273b4`, 2026-08-03 |
| 8 | persistence and the session state machine | 2026-08-04 |
| 9 | Settings, and the cross-cutting work it unblocked | 2026-08-09 |

The boost — bought on a fourth Upgrades sub-tab, fired with `b` — landed between 8 and
9 on 2026-08-05 and is deliberately **unnumbered**: numbering it would have renumbered
Settings in a list other documents already cited by number.

**Phase 3 is where a wireframe stopped being an oracle and became a reference.** Phase
2's acceptance test was exact — the running app matches the counted frame — and that
was the payoff for migrating those frames verbatim into [UI.md](UI.md). It stopped
working at phase 3, because a fresh run has no target, no boost and no enchants, so
the app draws states no frame was ever drawn for. Phase 4 turned the concession into
the method: four things the Mines frame drew did not survive a real run, and each was
recorded as a *departure* under the frame it departs from rather than quietly
implemented. Every `departures` subsection in [UI.md](UI.md) — there are eight — exists
because of this phase.

**Every core gap the front-end found had one shape:** a question the interface asks
that the rules had no *public* way to answer. `MineKind::ALL`, `Material::ALL` and
`EnchantType::ALL` were all `#[cfg(test)]` until a screen had to list twelve mines,
fifteen materials or seven enchant types — an enum cannot enumerate itself, and mines
are created lazily, so eleven of the twelve have no `Mine` to ask. The last gap of all
was the three counters [phase 11](#phase-11---the-counters-the-stats-screen-reads)
added.
