# Skylode - Roadmap

Scope for the first playable version and what is deliberately deferred. For the
rationale behind these choices, see [DECISIONS.md](DECISIONS.md).

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
- Mixed-content mines (Obsidian + Crying, End Stone + Amethyst).
- Per-mine size, 3x3 to 20x10, upgraded with the mine's own ore.
- Haste boosts (Redstone).
- One basic auto-miner with offline accrual.
- Prestige (Amethyst cost, deep reset, permanent global multiplier).
- Save system: JSON, 10-second autosave, atomic write, versioning, HMAC
  integrity, `.bak` recovery, clock handling.
- Five screens (Mine, Mines, Inventory, Upgrades, Stats).

## Post-MVP (parked)

- Full auto-miner system (tiers, "managers").
- Prestige meta-upgrade tree (spend a prestige currency on permanent perks).
- Daily quests (not daily login rewards).
- Skill / research tree (global multipliers).
- Richer End content and enchant variety.
- Special shop (Emerald currency).
- Further future: multiplayer / self-host.

## Open questions

- **Win condition:** endless prestige loop, or a defined end goal (for example,
  reach instamine on Netherite, or a prestige rank)?
- **Starting state:** confirm Wooden pickaxe mining Stone as the opening.
- **End signature ore naming:** Amethyst carries the End's rich-ore role; confirm
  whether it needs a distinct name.
- **Upgrade naming convention:** mirror PikaNetwork (enchant, level, material) or
  use our own.
- **Tunables (decided at implementation time):** XP curve and world-unlock levels
  (15, 30), offline cap (7 days), dip magnitude, cost-curve constants, compression
  ratio (100), autosave interval (10 seconds), enchant level caps per dimension,
  enchant proc rates and cooldowns, mine-size upgrade costs, prestige multiplier
  scale, batch-reset threshold (0).
