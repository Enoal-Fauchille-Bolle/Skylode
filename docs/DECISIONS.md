# Skylode - Decisions

An append-only ledger of settled design decisions and rejected ideas. Each entry
records the verdict and a short reason. For the full detail of any accepted
decision, follow the link into [DESIGN.md](DESIGN.md), [MECHANICS.md](MECHANICS.md),
or [SYSTEMS.md](SYSTEMS.md) rather than repeating it here.

## Accepted

| Decision | Reason |
| --- | --- |
| Solo Rust TUI (`ratatui`), single offline binary | Chosen over web, pure-API, and Bevy 2D. Best fit for learning Rust, offline shareability, and scope control. |
| Economy: ores only, no currency | Simplicity and theme. |
| No skyblock islands, PvP, or multiplayer at MVP | Out of scope for a solo MVP. |
| No bosses or combat | Engagement is economic, not combat. |
| Pickaxe: Efficiency 0..5 per tier, dip on tier jump, Netherite climbs past 5, then prestige | Faithful to the source loop. See [MECHANICS.md](MECHANICS.md#pickaxe-progression). |
| Obsidian / Crying Obsidian and Amethyst are post-Netherite enhancements, not full tiers | Keeps the tier list clean. |
| Composite upgrade costs (compressed plus raw) | Readability. Compression is a denomination, not inventory management. |
| Prestige: yes | Adds an endgame reset lever absent from SkyMines. |
| Single ore quality (common/uncommon removed) | Extra complexity for little MVP value. |
| Dirt removed; game starts at Stone | Dirt had no compelling function. |
| End = richest final mine plus enchant workshop (amethyst spent on enchants) | Combines the two useful End ideas. |
| Auto-miner: one basic miner at MVP, full system post-MVP | Scope control. |
| Mining interaction: active-continuous (hold Space) | Not spam, not Melvor idle. Idle comes only from the auto-miner. See [MECHANICS.md](MECHANICS.md#active-continuous-mining). |
| Mine is a 2D grid, and the grid is the model | Spatial enchants need block positions. `block_count` derived, `capacity = W * H`. |
| Grid: fixed 20x10 = 200, 2 char-columns per block, min terminal ~80x24 | Size is a game constant, decoupled from terminal size. No mine-size upgrades at MVP. |
| Mono-material mines at MVP | Mixed-content mines and a true Vein Miner parked post-MVP. |
| Batch reset: deplete to 0, then full instant refill | Matches SkyMines cube regeneration. |
| Amethyst enchants (by level): Explosive, Jackhammer, Nuke, Excavator, Haste | Trimmed set for a uniform 2D grid. See [MECHANICS.md](MECHANICS.md#enchants). |
| Haste enchant = permanent multiplier | Multiplicative, distinct from additive Efficiency, so no conflict. |
| Tick rate: 20 per second | Minecraft one-to-one; rendering decoupled; offline replayed. |
| Seeded PRNG in the save | Reproducible ticks and tests while keeping real enchant bursts. |
| Offline accrual: yes, cap 7 days, 100% rate, clamp backward and log, wall clock | See [MECHANICS.md](MECHANICS.md#offline-accrual). |
| No daily login rewards | Daily quests considered post-MVP instead. |
| Save: JSON, 10s autosave (if dirty) plus transactions plus exit, atomic write, `.bak`, HMAC, versioning | See [SYSTEMS.md](SYSTEMS.md#save-system). |
| Name: Skylode | Settled. |
| Lore: none for now | Four candidate directions parked: last miner, station operator, mining drone, prospector. |

## Rejected

| Idea | Reason |
| --- | --- |
| Paid ranks | Monetization gate, meaningless offline. Replaced by progression gates. |
| Money / currency economy | Chose ore-only for simplicity and theme. |
| Common / uncommon ore qualities | Extra complexity for little MVP value; not in the referenced era. |
| Compression as inventory management | No stack limit, so it would be pure friction. Kept only as a cost denomination. |
| PvP / multiplayer | Out of scope for a solo MVP. |
| Skyblock island building | Not the part of SkyMines we want. |
| Web fullstack front-end | TUI fits learning Rust, offline shareability, and scope control better. |
| Bevy 2D game | Same reason as above. |
| Pure-API game (SpaceTraders-style) | Leans on automation and multiplayer appeal; chose the directly-playable TUI. |
| Gambling / push-your-luck End | Judged not adapted to the game. |
| Production-chain / bottleneck system | Kept the loop simple (mine to inventory) for MVP. |
| Daily login rewards | Retention dark pattern; offline accrual already rewards returning. |
| Dirt | No compelling function. |
| Bosses / combat encounters | Out of scope; engagement comes from economic decisions. |
| Drill and Laser enchants | Drill is a column dominated by the row; Laser merged into Jackhammer. |
| Lucky-Strike / Overclock enchants | Variance and gambling feel. |
