# Skylode - Roadmap

What is in the first playable version, and what is deliberately deferred. For the
rationale behind any of it, see [decisions/](decisions/); for the order the two crates
were built in, [PHASES.md](PHASES.md).

## Where this stands

The MVP list below is **delivered** — the game is playable, pre-1.0, and every phase of
both build plans has shipped. What separates the tree from `1.0.0` is the tail of the
balance work, and it is three items:

- **The tunables no harness reaches.** Enchant proc rates and cooldowns (the reference
  players barely buy the spatials), the offline cap and the auto-miner rate (an
  active-play harness never idles), the dip magnitude, and the XP curve — which the
  pacing band of [0030](decisions/0030-the-pacing-target-for-a-first-prestige-is-a-band-1-h.md)
  constrains *implicitly*, since the speedrun floor is XP-gated, but which nothing
  isolates. These are genuinely unmeasured rather than merely unwritten.

  **Three of them already have a direction, from playing rather than from a harness**:
  levelling reads as *too easy*, mining as *too fast*, and Lapis as *too expensive*. A
  harness cannot produce those readings — it measures elapsed hours, not whether an hour
  felt earned — so they are the starting point for this pass, not a competing opinion.
  Note that the first two pull against the measured band: shortening the XP curve or
  slowing the swing both lengthen a run, and the band is guarded at both ends.
- **The level-up bundle pays in the prestige currency, and nothing has measured how
  much of a rank that finances.** The budget is linear in the level, so **65.9 % of it
  lands on levels 31 to 50** — all of them past the End's unlock — and half of every
  bundle is the world's enchant material
  ([0012](decisions/0012-the-level-up-bundle-shares-the-enchant-fuel-table.md)), which
  in the End is Amethyst. About **4 050 of the 12 290 raw items** a full climb is
  granted are therefore the very currency a rank is bought with
  ([0066](decisions/0066-prestige-currency-amethyst-condition-a-fully-realised.md)).
  Whether that is a third of a first rank or a tenth of one is unknown, and the harness
  cannot answer it: it counts Amethyst in the bank without separating what was mined
  from what was given.
- **The deepening of `GameState::validate`** that phase 11 listed and deliberately
  left: the cross-field plausibility checks reconciling an inventory and a level against
  the counters. It waits on the tunables above, because a `validate` tightened against a
  guess refuses honest saves.

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
- **Sixteen states, not six screens.** The six tabs are `Screen::ALL` (Mine, Mines,
  Inventory, Upgrades, Stats, Levels). Beside them sit three non-game stages —
  title, save recovery, offline summary (`session::Stage`, whose fourth node *is* the
  game) — six modals (`overlay::Modal` minus the debug-only dev menu: Help, Settings,
  compression dialog, the dip, prestige preview, prestige confirm), and the
  terminal-too-small screen. Most were specified elsewhere and listed nowhere. Plus a
  cross-cutting toast component for the announcements no screen owned.

## Post-MVP (parked)

- **Achievements.** A list of one-off markers, the last of which is *reach prestige
  rank 10* — the game's finish line, and deliberately a marker rather than a gate. The
  threshold is a free choice for whoever builds this list, not a constraint the curve
  imposes; see [0155](decisions/0155-the-win-condition-is-an-achievement-at-prestige-rank-10.md).
- Full auto-miner system (tiers, "managers").
- Prestige meta-upgrade tree (spend a prestige currency on permanent perks).
- Daily quests (not daily login rewards).
- Skill / research tree (global multipliers).
- Richer End content and enchant variety.
- Special shop (Emerald currency).
- **An unbounded sink for a finished mine's ore.** Once a mine's two tracks are maxed,
  an Overworld ore has nowhere left to go; enchant fuel was costed out as a fix and
  could not carry it. See
  [0042](decisions/0042-old-mines-stay-relevant-as-enchant-fuel.md), which records the
  problem as open rather than solved.
- Further future: multiplayer / self-host.
