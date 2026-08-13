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
- ~~Publish to crates.io.~~ **Pulled out of this list on 2026-08-12 — decided and
  wired.** Both blockers this entry named are gone, and one of them was never quite
  right: `description`, `license` and `repository` landed with the versioning work, and
  the missing `version` on the path dependency blocked publishing the **front-end**, not
  the library. `skylode-core` depends only on registry crates, so it was publishable as
  it stood — `cargo publish -p skylode-core --dry-run` succeeds on the tree that
  carried this paragraph.

  What exists now: a `publish-crate` job in `release.yml` that publishes the core on
  every tag, authenticated by **Trusted Publishing** rather than a stored token — the
  runner exchanges its OIDC identity for a credential that lives thirty minutes, so
  there is no `CARGO_REGISTRY_TOKEN` in this repository to leak. The first push is
  manual, because crates.io requires an existing version before a repository can be
  linked as a trusted publisher.

  Publishing the front-end as well — so that `cargo install` is a way to play — was
  settled the same week: **yes, and without renaming the package**. That second half
  is the interesting one. Publishing makes the *package* name public, and the argument
  that had justified `skylode-tui` (it names a place in the workspace; the player only
  ever types the binary's name) dies at that moment. It was replaced rather than
  patched: `skylode` is the whole game, this package holds only the front-end, so
  naming it `skylode` would be a false claim about its contents. The install line pays
  one hyphen — `cargo install skylode-tui`, installing a binary called `skylode` — and
  the source tree stays honest. Cargo's own ambiguity is the root of it: a package is
  both a source unit and the unit `cargo install` delivers, and here the two want
  different names.
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
- ~~**Starting state**~~ — **settled**: a Wooden pickaxe in the Stone mine is the
  opening, confirmed as written. It was never in doubt so much as never signed off,
  and it is what `Player::new` and `GameState::new` have built all along — the two
  reference players in the phase-10 harness start there, so the measured pacing band
  is a band about *this* opening and no other. Recording it closes the gap between a
  default nobody chose on purpose and one that has now been chosen.
- ~~**End signature ore naming**~~ — **settled**: Amethyst keeps its name. The
  question was whether the End's rich ore should stop borrowing a material Minecraft
  puts in the Overworld, and the answer follows the rule the palette already obeys —
  a material is meant to be **recognised, not learned**, which is why hue follows
  Minecraft rather than being invented. An invented name would add one mapping for
  the player to memorise and close no ambiguity, since nothing else in the game is
  called Amethyst; and the lore argument is one the game declines everywhere else,
  Ancient Debris, Obsidian and Quartz all being taken as they are. That Amethyst also
  serves as the prestige currency is a reason to keep it legible, not to rename it.
  Renaming would have been cheap — `Material::name` is a display name, deliberately
  kept apart from the save key — so this is a choice and not a constraint.
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
  ladder, then re-settled by its second pass**: `+10 %` per rank on ore yield and XP (and
  *not* on mining speed), against a price that is `one climb's Amethyst income + a
  surcharge growing `+20 %` per rank`. Both halves moved together and neither makes sense
  alone: the multiplier is the only lever on the climb, the surcharge the only lever on
  the Amethyst phase, and the ladder's shape is what the two do to each other.
  Measured rather than felt, and measured **per phase** — a total hides two halves moving
  in opposite directions. The climb accelerates ×2.1 across ten ranks, the Amethyst phase
  lengthens from ~20 to ~34 minutes, and the run they add up to goes 1.34 h → 1.03 h and
  settles there. Guarded by `the_prestige_loop_settles_instead_of_walling` and
  `one_climb_still_banks_about_what_the_price_is_aimed_at`.
  The first pass' figures — `+20 %` against a doubling price, giving a **U** that bottomed
  at 0.22 h by rank 6 and climbed to 3.48 h by rank 10 — are kept in
  [DECISIONS.md](DECISIONS.md) as the record of why this shape was rejected, not as a
  target that was missed.
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
