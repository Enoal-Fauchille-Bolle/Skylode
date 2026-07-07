# Skylode - Roadmap

Scope for the first playable version and what is deliberately deferred. For the
rationale behind these choices, see [DECISIONS.md](DECISIONS.md).

## MVP

- Core mining loop (per-block break, instamine path).
- Ores to pickaxe upgrades (tiers, Efficiency, Fortune) with composite costs.
  - Full upgrade-roadmap screen.
- Worlds and materials with their functions.
- Haste boosts (Redstone).
- One basic auto-miner with offline accrual.
- Prestige.
- Save system: JSON, 10-second autosave, atomic write, versioning, HMAC
  integrity, `.bak` recovery, clock handling.
- Five screens (Mine, Mines, Inventory, Upgrades, Stats).

## Post-MVP (parked)

- Full auto-miner system (tiers, "managers").
- Daily quests (not daily login rewards).
- Skill / research tree (global multipliers).
- Richer End enchant variety.
- Mixed-content mines and a true Vein Miner.
- Per-world mine growth.
- Further future: multiplayer / self-host.

## Open questions

- **Win condition:** endless prestige loop, or a defined end goal (for example,
  reach instamine on Netherite)?
- **Starting state:** confirm Wooden pickaxe mining Stone as the opening.
- **Upgrade naming convention:** mirror PikaNetwork (enchant, level, material) or
  use our own.
- **Tunables (decided at implementation time):** offline cap (7 days), dip
  magnitude, cost curves, autosave interval (10 seconds), enchant proc rates and
  cooldowns, grid capacity (200), batch-reset threshold (0).
