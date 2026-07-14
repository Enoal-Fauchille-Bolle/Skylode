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
| Composite upgrade costs (Compressed plus raw) | Readability: `6 Compressed Iron + 50 Iron` reads better than `650 Iron`, and the player can check it in their head. |
| Compression ratio: 1 Compressed ore = 100 raw | Round, readable, matches the doc cost examples. |
| Compression is a manual player action, free and lossless both ways (100 raw <-> 1 Compressed) | Revises the earlier "denomination, not inventory management" call: a Compressed unit is real inventory state, not a display format. Free and reversible so it can never soft-lock a run. See [MECHANICS.md](MECHANICS.md#compression). |
| Costs are paid in the denomination they are quoted in: 650 raw Iron does *not* buy `6 Compressed Iron + 50 Iron` | Makes compressing a step in the upgrade path instead of a cosmetic button. The refusal costs one action to clear, since compression is free, so the friction is a beat, not a wall. |
| A *dense block* and a *Compressed unit* are different things, and both stay | A dense block (`IronBlock`, `Cobblestone`) is a mineable, tougher grid cell dropping 9 raw. A Compressed unit is minted by the player, worth 100 raw, and is never mined nor a `Block`. Nine versus a hundred, mined versus minted. |
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
| **Mine richness**: a second per-mine upgrade track, the weight of the mine's valuable cell | Absorbs "mixed content" rather than adding to it — richness 0 *is* the mixed mine. It is also the only path into the game for the dense blocks, which are otherwise decoration. Distinct from size because they multiply different sub-systems: size scales the spatial enchants, richness scales Fortune. See [MECHANICS.md](MECHANICS.md#mine-richness). |
| A mine is one common cell plus one cell of value — pure ore, not filler-plus-veins | Rejects the Minecraft-style "iron mine is mostly Stone with veins". That reading would reopen "pickaxe tier opens mines" (a wooden pickaxe could break 85% of the iron mine's cells) and make "a mine funds its own growth" ambiguous (paid in iron, or in the stone it mostly produces?). The pure mine is faithful to SkyMines and uniform across all three worlds. |
| Richness: buy the ceiling, set the dial freely below it | The compression rule again, for the same reason: one free action always puts a run in the shape the current goal wants, so a purchase can slow a run down but never strand it. |
| Richness has two flavours, and the asymmetry is accepted | Where the valuable cell is the dense form of the same material (9 mines), enriching is pure gain and the dial has one sensible position — the UI hides it. Where it is a different material (Obsidian, End), enriching is a substitution and the dial is a real choice. The rules stay uniform; only the UI branches. |
| Richness cap: the valuable cell's weight stays strictly below 100% | An **invariant, not a tunable**, doing double duty: anti-brick (the End mine never stops dropping the End Stone that pays to grow it) and anti-runaway (bounded production gain against an unbounded cost curve). |
| Mine upgrades (size and richness) are paid in that mine's own material; on the two two-material mines the richness mix shifts from common toward rare as it climbs | Trades an arithmetic brake ("enriching dries up the currency that buys enrichment") for a decisional one: high richness competes directly with prestige for Amethyst. See [MECHANICS.md](MECHANICS.md#mine-upgrade-costs). |
| Mines persist; **no free action may ever put a broken block back** | One rule covering two doors. Regenerating a mine on entry, or on a dial move, would be a free batch reset — break the 4 Amethyst out of 200, leave, return to a full grid, repeat. Depleting the mine *is* the price of the refill. |
| The free geometric re-roll is knowingly left open at MVP | Wiggling the dial re-rolls the layout of the remaining cells, which a patient player could use to line the valuable ones up under an Explosive. Single-player, offline, no leaderboard: the only person cheated is the cheater. Closes by deferring the dial to the next regeneration if it ever bites. |
| Prestige also resets mine richness | Second track of the same object, same currency. Keeping it would make the first prestige nearly painless on mines, and re-walking the progression is the point. |
| **XP is granted per item the block *contained*, before Fortune** — 1 per ore cell, 9 per dense cell | Keeps the two progression axes independent. Fortune multiplies loot, not experience; if it multiplied both, one investment would advance both axes and "neither axis alone carries progression" would stop being true. Richness still speeds levelling, since a dense cell contains nine. Excavator substitutes the loot and likewise grants no extra XP. |
| Every block drops something; each world's filler drops its own material (Stone, Netherrack, End Stone) | The filler is the block the player breaks most often, so one that paid nothing would make the bulk of their swings a tax. The three worlds now agree on the rule, and `Block::material` is total — no `None` branch for a case that cannot happen. |
| Batch reset: deplete to 0, then full instant refill | Matches SkyMines cube regeneration. |
| Five special enchants: Explosive, Jackhammer, Nuke, Excavator, Haste | Trimmed set for a uniform 2D grid. See [MECHANICS.md](MECHANICS.md#enchants). |
| Haste enchant = permanent multiplier | Multiplicative, distinct from additive Efficiency, so no conflict. |
| Post-instamine progression: mine size + spatial enchants + Fortune + ore value + prestige | Single-target speed saturates at instamine; throughput and value take over. See [MECHANICS.md](MECHANICS.md#post-instamine-progression). |
| Prestige: yes | The endgame reset loop, absent from SkyMines. See [MECHANICS.md](MECHANICS.md#prestige). |
| Prestige currency: Amethyst; condition: reach the End and accumulate it | Ties the last dimension to the reset loop; Amethyst is dual-use (enchants or prestige). |
| Prestige is a deep reset (including XP), keeping only prestige rank and its global multiplier | Re-walking the progression is the point; the multiplier makes it fast. |
| Tick rate: 20 per second | Minecraft one-to-one; rendering decoupled. Drives the interactive session only — offline is credited in closed form, not replayed. |
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
| ~~Compression as inventory management~~ | **Reversed** — see Accepted. Originally rejected as pure friction, on the grounds that unlimited stacks left it nothing to solve. Reinstated as a manual, free, reversible action, with costs quoted and paid in an exact denomination: the point is not storage, it is a beat in the upgrade path. |
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
