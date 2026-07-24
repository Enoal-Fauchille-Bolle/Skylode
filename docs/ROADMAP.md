# Skylode - Roadmap

Scope for the first playable version and what is deliberately deferred. For the
rationale behind these choices, see [DECISIONS.md](DECISIONS.md); for the order the
core is built in, see [PHASES.md](PHASES.md).

## MVP

- Core mining loop (per-block break, instamine path).
- Two-axis progression: mining level (XP) opens worlds; pickaxe tier opens mines.
  - Mining XP / level system (cap 50, world unlocks, level-up rewards).
- Ores to pickaxe upgrades (tiers, Efficiency 0..=5, Netherite 0..=15, Fortune to
  10) with composite costs.
  - Full upgrade-roadmap screen.
- Three worlds and their materials, including the per-dimension enchant materials
  (Lapis, Quartz, Amethyst).
- Five special enchants (Explosive, Jackhammer, Nuke, Excavator, Haste), leveled
  per dimension.
- Mixed-content mines (Obsidian + Crying, End Stone + Amethyst) — which is
  [richness](MECHANICS.md#mine-richness) at level 0.
- Per-mine size, 3x3 to 20x10, upgraded with the mine's own ore.
- Per-mine richness: the weight of the mine's valuable cell (Iron Ore to Iron
  Block, End Stone to Amethyst). Bought as a permanent ceiling, dialled freely
  below it. The dial is only shown on the two mines where it is a real choice.
- Haste boosts (Redstone).
- One basic auto-miner with offline accrual.
- Prestige (Amethyst cost, deep reset, permanent global multiplier).
- Save system: JSON, 10-second autosave, atomic write, versioning, HMAC
  integrity, `.bak` recovery, clock handling.
- Fifteen states, not five screens: the five above (Mine, Mines, Inventory,
  Upgrades, Stats) plus nine that were specified elsewhere and listed nowhere —
  main menu, terminal-too-small, save recovery, offline summary, level-up loot,
  compression dialog, prestige preview, prestige confirm, Settings — plus a
  cross-cutting toast component for the announcements no screen owned.

## Post-MVP (parked)

- **Achievements.** A list of one-off markers, the last of which is *reach prestige rank
  10* — the game's finish line, and deliberately a **marker rather than a gate**: it
  states that a run is complete without ending the session or capping the ladder. This is
  the answer the [win-condition question](#open-questions) was waiting on, so the two move
  together — and phase 10 has since done the price work that makes "marker rather than
  gate" mean something: the ladder no longer walls after rank 10, so continuing past the
  marker keeps returning ~1 h runs of the whole game. The threshold is therefore a free
  choice for whoever builds this list, not a constraint the curve imposes.
- Full auto-miner system (tiers, "managers").
- Prestige meta-upgrade tree (spend a prestige currency on permanent perks).
- Daily quests (not daily login rewards).
- Skill / research tree (global multipliers).
- Richer End content and enchant variety.
- Special shop (Emerald currency).
- Publish to crates.io (the game via `cargo install`, and `skylode-core` as a
  library). Two things block it today: neither manifest declares `description` or
  `license`, which crates.io rejects on upload; and `skylode-tui` depends on
  `skylode-core` by `path` alone, which cannot be published — a path dependency
  needs a `version` for the registry to resolve it.
- Further future: multiplayer / self-host.

## Open questions

- **Win condition:** **settled and implemented** (phase 10, second pass); only the
  achievement list that surfaces it is still post-MVP work. Neither a hard end nor an
  unmarked loop: an **achievement at prestige rank 10** marks a finish line without closing
  the game, and play continues past it. The prestige price was reshaped to make continuing
  worth doing — it is now `one climb's Amethyst income + a surcharge growing in a straight
  line`, where it was a curve that doubled per rank against a multiplier that only added.
  The measured ladder settles instead of walling: the climb accelerates ×2.1 across ten
  ranks, the Amethyst phase lengthens from ~20 to ~34 minutes, and the run they add up to
  goes 1.34 h → 1.03 h and stays there. **Rank 10 lands at ~11.5 h** for the speedrunner
  and ~15.9 h for the completionist, against ~9.6 h under the old curve — longer, but ten
  full runs of the game rather than six runs and four stretches of watching a counter. The
  threshold is now a free dial: past rank 10 each further rank costs about another hour,
  predictably, where the old curve made rank 12 unreachable in any reasonable session. See
  [DECISIONS.md](DECISIONS.md) and [MECHANICS.md](MECHANICS.md#prestige).
- **Starting state:** confirm Wooden pickaxe mining Stone as the opening.
- **End signature ore naming:** Amethyst carries the End's rich-ore role; confirm
  whether it needs a distinct name.
- ~~**Upgrade naming convention**~~ — **settled**: mirror PikaNetwork, with Roman
  numerals ("Diamond Pickaxe Efficiency XV"). See [DECISIONS.md](DECISIONS.md).
- ~~**Enchant level caps per dimension**~~ — **settled**: one ceiling per world
  shared by all five special enchants (3 / 6 / 10), not one per enchant. The cap
  gates how much may be invested; each enchant's own scaling decides what that buys.
  Lives in `World::enchant_cap`. The values stay open to balance, but their *order*
  does not. See [DECISIONS.md](DECISIONS.md).
- ~~**Cost-curve constants and mine-size upgrade costs**~~ — **settled by the phase-10
  pacing pass**: a per-track slope (size 1.55, richness 1.35, tier jumps and Efficiency
  1.45, the Netherite enhancement 1.10, enchants 1.25 on a base ten times the others),
  chosen against a measured target rather than by feel. A first prestige now lands in a
  **~1 h to ~2.3 h** band, measured by two reference players and guarded in the test gate
  by `the_first_prestige_lands_inside_the_pacing_window`. See [DECISIONS.md](DECISIONS.md).
  The values remain open to a *deliberate* retune — what is settled is that changing them
  now fails a test instead of passing unnoticed.
- ~~**Prestige multiplier scale and cost curve**~~ — **settled by the phase-10 prestige
  ladder**: `+20 %` per rank, additive, against a cost that doubles. Chosen against a
  measured curve rather than a feel — successive runs take 1.00, 0.68, 0.52, 0.41, 0.29,
  0.22 h and then climb back to 3.48 h by rank 10, so the loop rewards the first six
  ranks and prices the rest. Guarded by
  `the_prestige_loop_accelerates_then_turns_back_up`.
- **Tunables (decided at implementation time):** XP curve and world-unlock levels
  (15, 30), offline cap (7 days), dip magnitude, compression
  ratio (100), autosave interval (10 seconds),
  enchant proc rates and cooldowns,
  batch-reset threshold (0), `HOLD_WINDOW` (1100 ms — revisit only if
  playtest finds the stop latency perceptible) and the accessibility toggle's
  inactivity cutoff (15 s), and for richness: the number of levels, the
  weight curve `value_weight(level)` — with **no cap**, since
  [DECISIONS.md](DECISIONS.md) reversed the strict-sub-100% bound it once
  called an invariant (the free dial is the anti-brick, the geometric cost
  curve is the anti-runaway), leaving only the weaker rule that the two
  weights are never both zero — and how fast the cost mix shifts from the
  common material to the rare one.
