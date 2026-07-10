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
| Two-axis gating: mining level opens worlds, pickaxe tier opens mines | Separates "how far you are" from "how strong you are". See [MECHANICS.md](MECHANICS.md#progression-and-gating). |
| Mining XP / level system, cap 50 | Drives world unlocks and level-up rewards. Nether at level 15, End at level 30 (tunable). |
| Level-up rewards: ores / Compressed ore + a temporary boost | Keeps early levels satisfying without gating content. |
| Pickaxe: Efficiency 0..=5 per tier, dip on tier jump, then prestige | Faithful to the source loop. See [MECHANICS.md](MECHANICS.md#pickaxe-progression). |
| Netherite Efficiency 0..=15 (no reset) | 15 is Pika's instamine point; the top tier keeps climbing past 5. |
| `base_tier` speed is a monotone custom curve | Minecraft's tool speeds are non-monotone (gold beats diamond); the 1:1 principle is kept only for hardness. See [MECHANICS.md](MECHANICS.md#pickaxe-progression). |
| Fortune capped at 10 | Past 10, ore is abundant enough that more Fortune is pointless (matches Pika). |
| Obsidian / Crying Obsidian and the enchant materials are post-tier enhancements, not full tiers | Keeps the tier list clean. |
| Composite upgrade costs (compressed plus raw) | Readability. Compression is a denomination, not inventory management. |
| Compression ratio: 1 Compressed ore = 100 raw | Round, readable, matches the doc cost examples. |
| Cost curve shape: geometric growth, constants tuned at implementation | Balanced in live play, not fixed up front. |
| Single ore quality (common/uncommon removed) | Extra complexity for little MVP value. |
| Dirt removed; game starts at Stone | Dirt had no compelling function. |
| Worlds and their functions: Overworld (bases + Lapis enchants), Nether (Netherite + enhancement + Quartz enchants), End (End Stone + Amethyst) | Each world owns distinct functions. See [MECHANICS.md](MECHANICS.md#worlds-and-materials). |
| New enchant materials: Lapis (Overworld), Quartz (Nether) | One enchant material per dimension caps enchant level per world and keeps a clear theme. Lapis is Minecraft's enchanting currency. |
| Amethyst moved to the End; End is a mixed mine (End Stone + rare Amethyst) | Gives the End a signature rich ore. Amethyst is the top enchant material and the prestige currency. |
| Enchant material differs per dimension and caps enchant level there | Reason to progress worlds; all five enchants available early, only the cap rises. See [MECHANICS.md](MECHANICS.md#enchants). |
| Enchant cost = world material + a mix of earlier mines' ores | Keeps old mines useful as permanent enchant fuel after their tier is passed. |
| Old mines stay relevant as enchant fuel | Solves "mines become useless once their tier is behind you". |
| Auto-miner: one basic miner at MVP, full system post-MVP | Scope control. |
| Mining interaction: active-continuous (hold Space) | Not spam, not Melvor idle. Idle comes only from the auto-miner. See [MECHANICS.md](MECHANICS.md#active-continuous-mining). |
| Mine is a 2D grid, and the grid is the model | Spatial enchants need block positions. `block_count` derived, `capacity = W * H`. |
| Mine size is per-mine, 3x3 to 20x10 max, upgraded with the mine's own ore | A self-funded growth goal per mine; the terminal is sized for the 20x10 max. Revises the earlier "fixed 20x10, no size upgrades" decision. |
| Mixed-content mines allowed at MVP | Obsidian + Crying, End Stone + Amethyst. The targeted cell's material decides the drop. |
| Batch reset: deplete to 0, then full instant refill | Matches SkyMines cube regeneration. |
| Five special enchants: Explosive, Jackhammer, Nuke, Excavator, Haste | Trimmed set for a uniform 2D grid. See [MECHANICS.md](MECHANICS.md#enchants). |
| Haste enchant = permanent multiplier | Multiplicative, distinct from additive Efficiency, so no conflict. |
| Post-instamine progression: mine size + spatial enchants + Fortune + ore value + prestige | Single-target speed saturates at instamine; throughput and value take over. See [MECHANICS.md](MECHANICS.md#post-instamine-progression). |
| Prestige: yes | The endgame reset loop, absent from SkyMines. See [MECHANICS.md](MECHANICS.md#prestige). |
| Prestige currency: Amethyst; condition: reach the End and accumulate it | Ties the last dimension to the reset loop; Amethyst is dual-use (enchants or prestige). |
| Prestige is a deep reset (including XP), keeping only prestige rank and its global multiplier | Re-walking the progression is the point; the multiplier makes it fast. |
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
| Paid ranks | Monetization gate, meaningless offline. Replaced by progression gates (level and prestige rank). |
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
| Lucky-Strike / Overclock enchants | Variance and gambling feel. (The word "overclock" is free to reuse later for an auto-miner speed feature; only the enchant is rejected.) |
| True Vein Miner | Even with mixed-content mines now in scope, following connected same-type blocks was dropped rather than built. |
