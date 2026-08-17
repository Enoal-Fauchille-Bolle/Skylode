# Decisions

Every settled decision and every rejected idea, one record each. This directory
replaced a single 154-row table whose lines averaged 524 characters and peaked at
2 028 — a shape that put the ledger beyond `git diff` and therefore beyond review.

**A record is immutable.** When a decision is revisited, the replacement is written
as a new record that cites the old one, and the old one keeps its argument intact. A
log whose losing arguments have been overwritten looks authoritative and is not.
Where a decision was instead *refined in place* before this directory existed, the
record carries an **Amended** field counting the revisions and an `## Amendments`
section giving each one and its reason, so the refinement is visible rather than
folded into the original argument.

## scope

| # | Decision | Status |
| --- | --- | --- |
| [0003](0003-no-skyblock-islands-pvp-or-multiplayer-at-mvp.md) | No skyblock islands, PvP, or multiplayer at MVP | accepted |
| [0004](0004-no-bosses-or-combat.md) | No bosses or combat | accepted |
| [0129](0129-paid-ranks.md) | Paid ranks | rejected |
| [0133](0133-pvp-multiplayer.md) | PvP / multiplayer | rejected |
| [0134](0134-skyblock-island-building.md) | Skyblock island building | rejected |
| [0138](0138-gambling-push-your-luck-end.md) | Gambling / push-your-luck End | rejected |
| [0139](0139-production-chain-bottleneck-system.md) | Production-chain / bottleneck system | rejected |
| [0142](0142-bosses-combat-encounters.md) | Bosses / combat encounters | rejected |

## project

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-solo-rust-tui-ratatui-single-offline-binary.md) | Solo Rust TUI (ratatui), single offline binary | accepted |
| [0079](0079-name-skylode.md) | Name: Skylode | accepted |
| [0080](0080-lore-none-for-now.md) | Lore: none for now | accepted |
| [0081](0081-publish-both-crates-to-crates-io-the-front-end-as-the.md) | Publish both crates to crates.io | accepted |
| [0082](0082-docs-rs-documents-skylode-core-s-private-items.md) | docs.rs documents skylode-core's private items, deliberately | accepted |
| [0087](0087-language-english-no-i18n.md) | Language: English, no i18n | accepted |
| [0112](0112-the-dev-menu-is-gated-by-cfg-debug-assertions-plus-a.md) | The dev menu is gated by #[cfg(debug_assertions)] plus a SKYLODE_DEV… | accepted |
| [0135](0135-web-fullstack-front-end.md) | Web fullstack front-end | rejected |
| [0136](0136-bevy-2d-game.md) | Bevy 2D game | rejected |
| [0137](0137-pure-api-game-spacetraders-style.md) | Pure-API game (SpaceTraders-style) | rejected |

## progression

| # | Decision | Status |
| --- | --- | --- |
| [0005](0005-two-axis-gating-mining-level-opens-worlds-pickaxe-tier.md) | Two-axis gating: mining level opens worlds, pickaxe tier opens mines | accepted |
| [0006](0006-mining-xp-level-system-cap-50.md) | Mining XP / level system, cap 50 | accepted |
| [0007](0007-level-up-rewards-a-bundle-of-ore-plus-a-boost-charge.md) | Level-up rewards: a bundle of ore | accepted · amended once |
| [0033](0033-worlds-and-their-functions-overworld-bases-lapis.md) | Worlds and their functions | accepted |
| [0059](0059-xp-is-a-property-of-the-block-granted-before-fortune-a.md) | XP is a property of the block, granted before Fortune | accepted · amended once |
| [0064](0064-post-instamine-progression-mine-size-spatial-enchants.md) | Post-instamine progression | accepted |
| [0077](0077-no-daily-login-rewards.md) | No daily login rewards | accepted |
| [0098](0098-a-level-up-s-payout-is-exactly-one-thing-loot-or-a.md) | A level-up's payout is exactly one thing | accepted · amended once |
| [0099](0099-a-level-up-announces-it-does-not-pay-the-reward-is.md) | A level-up announces | accepted |
| [0140](0140-daily-login-rewards.md) | Daily login rewards | rejected |
| [0156](0156-the-opening-is-a-wooden-pickaxe-in-the-stone-mine.md) | The opening is a Wooden pickaxe in the Stone mine | accepted |

## economy

| # | Decision | Status |
| --- | --- | --- |
| [0002](0002-economy-ores-only-no-currency.md) | Economy: ores only, no currency | accepted |
| [0013](0013-a-bundle-is-credited-entirely-in-raw-items-never-pre.md) | A bundle is credited entirely in raw items | accepted |
| [0023](0023-composite-upgrade-costs-compressed-plus-raw.md) | Composite upgrade costs (Compressed plus raw) | accepted |
| [0024](0024-compression-ratio-1-compressed-ore-100-raw.md) | Compression ratio: 1 Compressed ore = 100 raw | accepted |
| [0025](0025-compression-is-a-manual-player-action-free-and.md) | Compression is a manual player action | accepted |
| [0026](0026-costs-are-paid-in-the-denomination-they-are-quoted-in.md) | Costs are paid in the denomination they are quoted in | accepted |
| [0027](0027-a-dense-block-and-a-compressed-unit-are-different.md) | A dense block and a Compressed unit are different things, and both stay | accepted |
| [0028](0028-cost-curve-shape-geometric-growth-constants-tuned-at.md) | Cost curve shape: geometric growth, constants tuned at implementation | accepted |
| [0029](0029-each-upgrade-track-carries-its-own-base-and-growth.md) | Each upgrade track carries its own base and growth | accepted · amended once (phase 10) |
| [0031](0031-single-ore-quality-common-uncommon-removed.md) | Single ore quality (common/uncommon removed) | accepted |
| [0130](0130-money-currency-economy.md) | Money / currency economy | rejected |
| [0131](0131-common-uncommon-ore-qualities.md) | Common / uncommon ore qualities | rejected |
| [0132](0132-compression-as-inventory-management.md) | Compression as inventory management | withdrawn |

## pickaxe

| # | Decision | Status |
| --- | --- | --- |
| [0014](0014-pickaxe-efficiency-0-5-per-tier-dip-on-tier-jump-then.md) | Pickaxe: Efficiency 0..=5 per tier, dip on tier jump, then prestige | accepted |
| [0015](0015-a-tier-jump-is-paid-in-the-tier-being-left-at-that.md) | A tier jump is paid in the tier being left | accepted · amended once (phase 10) |
| [0016](0016-netherite-efficiency-0-15-the-enhancement-6-15-on-its.md) | Netherite Efficiency 0..=15 | accepted · amended once (phase 10) |
| [0017](0017-base-tier-speed-is-a-monotone-custom-curve.md) | base_tier speed is a monotone custom curve | accepted |
| [0018](0018-break-time-is-ceil-30-hardness-mining-power-minecraft.md) | Break time is ceil(30 * hardness / mining_power) | accepted |
| [0019](0019-efficiency-s-level-1-is-earned-from-level-1-not-level-0.md) | Efficiency's level² + 1 is earned from level 1, not level 0 | accepted |
| [0021](0021-obsidian-crying-obsidian-and-the-enchant-materials-are.md) | Obsidian / Crying Obsidian and the enchant materials are post-tier… | accepted |
| [0022](0022-netherite-efficiency-6-15-is-paid-in-obsidian-crying.md) | Netherite Efficiency 6..=15 is paid in Obsidian + Crying Obsidian | accepted · amended once |
| [0036](0036-the-end-s-ore-gates-behind-netherite-the-nether-s.md) | The End's ore gates behind Netherite, the Nether's behind Diamond | accepted · amended once |
| [0039](0039-efficiency-stays-capped-by-the-tier-every-other.md) | Efficiency stays capped by the tier | accepted · amended once |

## enchants

| # | Decision | Status |
| --- | --- | --- |
| [0012](0012-the-level-up-bundle-shares-the-enchant-fuel-table.md) | The level-up bundle shares the enchant fuel table rather than owning one | accepted |
| [0020](0020-fortune-capped-at-10-but-reached-progressively-3-6-10.md) | Fortune capped at 10, but reached progressively: 3 / 6 / 10 by world | accepted · amended once |
| [0034](0034-new-enchant-materials-lapis-overworld-quartz-nether.md) | New enchant materials: Lapis (Overworld), Quartz (Nether) | accepted |
| [0037](0037-enchant-material-differs-per-dimension-and-caps.md) | Enchant material differs per dimension and caps enchant level there | accepted |
| [0038](0038-the-per-world-enchant-cap-is-one-number-shared-by-all.md) | The per-world enchant cap is one number shared by all five specials (3… | accepted |
| [0040](0040-permanent-upgrades-alone-never-instamine-ancient.md) | Permanent upgrades alone never instamine Ancient Debris or Obsidian | accepted |
| [0041](0041-enchant-cost-world-material-two-ores-of-the-current.md) | Enchant cost = world material + two ores of the current progression tier | accepted · amended once |
| [0042](0042-old-mines-stay-relevant-as-enchant-fuel.md) | Old mines stay relevant as enchant fuel | withdrawn |
| [0062](0062-five-special-enchants-explosive-jackhammer-nuke.md) | Five special enchants: Explosive, Jackhammer, Nuke, Excavator, Haste | accepted |
| [0063](0063-haste-enchant-permanent-multiplier.md) | Haste enchant = permanent multiplier | accepted |
| [0100](0100-the-four-triggered-special-enchants-explosive.md) | The four triggered special enchants (Explosive, Jackhammer, Nuke, Excavator) | accepted · amended once |
| [0101](0101-excavator-substitutes-one-compressed-mined-material.md) | Excavator substitutes one Compressed <mined material> for the block's… | accepted |
| [0102](0102-excavator-rolls-once-per-swing-on-the-impact-block.md) | Excavator rolls once per swing, on the impact block only | accepted |
| [0103](0103-excavator-resolves-in-enchant-not-on-mine-and-draws.md) | Excavator resolves in enchant | accepted |
| [0104](0104-explosive-is-a-chebyshev-square-up-to-3x3-5x5-7x7-by.md) | Explosive is a Chebyshev square (up to 3x3 / 5x5 / 7x7 by level band) | accepted |
| [0143](0143-drill-and-laser-enchants.md) | Drill and Laser enchants | rejected |
| [0144](0144-lucky-strike-overclock-enchants.md) | Lucky-Strike / Overclock enchants | rejected |
| [0145](0145-true-vein-miner.md) | True Vein Miner | rejected |

## mines

| # | Decision | Status |
| --- | --- | --- |
| [0032](0032-dirt-removed-game-starts-at-stone.md) | Dirt removed; game starts at Stone | accepted |
| [0035](0035-amethyst-moved-to-the-end-end-is-a-mixed-mine-end.md) | Amethyst moved to the End | accepted |
| [0046](0046-mine-is-a-2d-grid-and-the-grid-is-the-model.md) | Mine is a 2D grid, and the grid is the model | accepted |
| [0047](0047-mine-size-is-per-mine-3x3-to-20x10-max-upgraded-with.md) | Mine size is per-mine | accepted · amended twice |
| [0048](0048-mixed-content-mines-allowed-at-mvp.md) | Mixed-content mines allowed at MVP | accepted |
| [0049](0049-the-dial-changes-a-mine-s-mining-speed-and-not-always.md) | The dial changes a mine's mining speed | accepted · amended once (phase 10) |
| [0050](0050-mine-richness-a-second-per-mine-upgrade-track-the.md) | Mine richness: a second per-mine upgrade track | accepted |
| [0051](0051-an-overworld-ore-mine-s-common-cell-is-the-ore-itself.md) | An Overworld ore mine's common cell is the ore itself | accepted |
| [0052](0052-richness-buy-the-ceiling-set-the-dial-freely-below-it.md) | Richness: buy the ceiling, set the dial freely below it | accepted · amended once |
| [0053](0053-richness-has-two-flavours-and-the-asymmetry-is-accepted.md) | Richness has two flavours, and the asymmetry is accepted | accepted |
| [0054](0054-richness-has-no-weight-cap-the-value-cell-weight-per.md) | Richness has no weight cap | accepted |
| [0055](0055-mine-upgrades-size-and-richness-are-paid-in-that-mine.md) | Mine upgrades (size and richness) are paid in that mine's own material | accepted · amended once |
| [0056](0056-mines-persist-no-free-action-may-ever-put-a-broken.md) | Mines persist; no free action may ever put a broken block back | accepted |
| [0057](0057-the-free-geometric-re-roll-is-knowingly-left-open-at.md) | The free geometric re-roll is knowingly left open at MVP | accepted |
| [0060](0060-every-block-drops-something-each-world-s-filler-drops.md) | Every block drops something | accepted |
| [0061](0061-batch-reset-deplete-to-0-then-full-instant-refill.md) | Batch reset: deplete to 0, then full instant refill | accepted |
| [0141](0141-dirt.md) | Dirt | rejected |
| [0157](0157-amethyst-keeps-its-name.md) | Amethyst keeps its name | accepted |

## prestige

| # | Decision | Status |
| --- | --- | --- |
| [0030](0030-the-pacing-target-for-a-first-prestige-is-a-band-1-h.md) | The pacing target for a first prestige is a band | accepted · amended twice (both downward) |
| [0058](0058-prestige-also-resets-mine-richness.md) | Prestige also resets mine richness | accepted |
| [0065](0065-prestige-yes.md) | Prestige: yes | accepted |
| [0066](0066-prestige-currency-amethyst-condition-a-fully-realised.md) | Prestige currency: Amethyst | accepted · amended twice |
| [0067](0067-prestige-is-a-deep-reset-including-xp-keeping-only.md) | Prestige is a deep reset (including XP) | accepted · amended once (phase 10, second pass) |
| [0068](0068-the-prestige-multiplier-no-longer-applies-to-mining.md) | The prestige multiplier no longer applies to mining speed | accepted |
| [0069](0069-the-prestige-price-is-a-sum-one-climb-s-amethyst.md) | The prestige price is a sum | accepted |
| [0070](0070-the-prestige-loop-is-endless-by-design-and-the-price.md) | The prestige loop is endless by design | accepted |
| [0071](0071-the-prestige-reset-also-takes-the-boost-reserve-the.md) | The prestige reset also takes the boost reserve | accepted |
| [0072](0072-the-prestige-multiplier-is-an-integer-in-permille.md) | The prestige multiplier is an integer in permille | accepted |
| [0110](0110-prestige-opens-with-p-from-stats-the-typed-prestige.md) | Prestige opens with p from Stats; the typed PRESTIGE confirm stays | accepted |
| [0155](0155-the-win-condition-is-an-achievement-at-prestige-rank-10.md) | The win condition is an achievement at prestige rank 10 | accepted |

## auto-miner

| # | Decision | Status |
| --- | --- | --- |
| [0008](0008-the-auto-miner-pays-ore-and-never-xp-and-it-runs-at.md) | The auto-miner pays ore and never XP | accepted |
| [0009](0009-the-auto-miner-never-walks-the-grid-online-or-offline.md) | The auto-miner never walks the grid, online or offline | accepted |
| [0043](0043-auto-miner-one-basic-miner-at-mvp-full-system-post-mvp.md) | Auto-miner: one basic miner at MVP, full system post-MVP | accepted |
| [0073](0073-the-auto-miner-takes-the-multiplier-once-on-its-rate.md) | The auto-miner takes the multiplier once | accepted · amended once (phase 10, second pass) |
| [0076](0076-offline-accrual-yes-cap-7-days-100-rate-clamp-backward.md) | Offline accrual: yes | accepted |

## boost

| # | Decision | Status |
| --- | --- | --- |
| [0010](0010-firing-a-boost-charge-while-one-is-running-stacks-the.md) | Firing a boost charge while one is running stacks the duration | accepted |
| [0011](0011-a-boost-is-granted-as-a-charge-held-in-reserve-not-as.md) | A boost is granted as a charge held in reserve, not as a running boost | accepted |

## runtime

| # | Decision | Status |
| --- | --- | --- |
| [0044](0044-mining-interaction-active-continuous-hold-space.md) | Mining interaction: active-continuous (hold Space) | accepted |
| [0045](0045-releasing-the-mine-key-forfeits-the-block-in-progress.md) | Releasing the mine key forfeits the block in progress | accepted |
| [0074](0074-tick-rate-20-per-second.md) | Tick rate: 20 per second | accepted |

## save

| # | Decision | Status |
| --- | --- | --- |
| [0075](0075-seeded-prng-in-the-save.md) | Seeded PRNG in the save | accepted |
| [0078](0078-save-json-10s-autosave-if-dirty-plus-transactions-plus.md) | Save: JSON, 10s autosave (if dirty) plus transactions plus exit, atomic… | accepted |
| [0091](0091-config-lives-in-the-save-there-is-no-separate-config.md) | Config lives in the save; there is no separate config file | accepted |
| [0092](0092-the-hmac-covers-the-whole-save-config-included-no-hand.md) | The HMAC covers the whole save | accepted |
| [0093](0093-screens-shown-before-the-save-is-trusted-render-with.md) | Screens shown before the save is trusted render with hardcoded defaults | accepted |
| [0097](0097-save-recovery-refuses-a-save-that-fails-its-checksum.md) | Save recovery refuses a save that fails its checksum | accepted |
| [0113](0113-the-save-lives-at-the-platform-s-own-data-location.md) | The save lives at the platform's own data location | accepted |
| [0114](0114-the-hmac-key-is-obfuscated-in-the-binary-build-time.md) | The HMAC key is obfuscated in the binary | accepted |
| [0116](0116-continuing-from-the-backup-is-announced-with-a-toast.md) | Continuing from the backup is announced with a toast, not a frame | accepted |
| [0118](0118-a-failed-write-is-announced-on-the-edge-and-is-never.md) | A failed write is announced on the edge and is never fatal | accepted |
| [0149](0149-a-separate-config-file-xdg-config-skylode.md) | A separate config file (XDG ~/.config/skylode/) | rejected |

## ui

| # | Decision | Status |
| --- | --- | --- |
| [0083](0083-ui-wireframes-are-ascii-monospace-not-diagrams-flows.md) | UI wireframes are ASCII monospace, not diagrams; flows are Mermaid | accepted |
| [0084](0084-reference-terminal-80-24-minimum-adapting-upward-the.md) | Reference terminal: 80×24 minimum | accepted |
| [0085](0085-scope-is-15-states-plus-a-toast-component-not-5-screens.md) | Scope is 15 states plus a toast component, not 5 screens | accepted |
| [0086](0086-event-announcements-ephemeral-toasts-plus-full-history.md) | Event announcements: ephemeral toasts plus full history in Stats | accepted |
| [0088](0088-upgrade-naming-minecraft-pika-style-with-roman.md) | Upgrade naming: Minecraft/Pika style with Roman numerals | accepted |
| [0089](0089-colour-a-256-colour-palette-degrading-to-16-in-settings.md) | Colour: a 256-colour palette, degrading to 16 in Settings | accepted |
| [0090](0090-numbers-are-exact-with-separators-1-234-567-never.md) | Numbers are exact, with separators (1 234 567); never abbreviated | accepted |
| [0094](0094-mining-input-two-layers-the-kitty-keyboard-protocol.md) | Mining input: two layers | accepted |
| [0095](0095-hold-window-1100-ms-and-the-false-positive-is-the-one.md) | HOLD_WINDOW = 1100 ms, and the false positive is the one we eat | accepted |
| [0096](0096-an-accessibility-toggle-space-starts-stops-with-a-15-s.md) | An accessibility toggle | accepted · amended once (TUI phase 9) |
| [0105](0105-a-mine-cell-is-a-two-column-background-swatch-not-two.md) | A mine cell is a two-column background swatch | accepted |
| [0106](0106-the-palette-is-24-entries-one-per-block-variant-and.md) | The palette is 24 entries | accepted |
| [0107](0107-the-value-stipple-is-unconditional-in-both-colour.md) | The value stipple is unconditional in both colour modes | accepted |
| [0108](0108-spatial-procs-are-rendered-as-a-two-beat-flash-200-ms.md) | Spatial procs are rendered as a two-beat flash (~200 ms) | accepted |
| [0109](0109-is-global-and-printed-in-every-footer-s-and-q-are.md) | ? is global and printed in every footer | accepted |
| [0111](0111-no-next-affordable-upgrade-hint-at-mvp-the-role-goes.md) | No "next affordable upgrade" hint at MVP | accepted |
| [0115](0115-q-in-a-game-returns-to-the-title-only-ctrl-c-ends-the.md) | q in a game returns to the title | accepted |
| [0117](0117-new-game-asks-for-confirmation-and-only-where-there-is.md) | New game asks for confirmation, and only where there is a run to lose | accepted |
| [0119](0119-a-boost-charge-is-bought-on-a-fourth-upgrades-sub-tab.md) | A boost charge is bought on a fourth Upgrades sub-tab | accepted |
| [0120](0120-b-fires-a-charge-contextual-to-the-mine-screen-and.md) | b fires a charge, contextual to the Mine screen and printed in its… | accepted |
| [0121](0121-firing-onto-a-running-boost-asks-no-confirmation-and-m.md) | Firing onto a running boost asks no confirmation | accepted |
| [0122](0122-a-preference-changed-on-the-title-is-carried-by-the.md) | A preference changed on the title is carried by the Splash | accepted |
| [0123](0123-settings-does-not-pause-the-game-exactly-as-help-does.md) | Settings does not pause the game, exactly as Help does not | accepted |
| [0124](0124-press-to-start-is-a-latch-flipped-on-the-rising-edge.md) | Press to start is a latch flipped on the rising edge of the hold… | accepted |
| [0125](0125-mining-happens-on-the-mine-screen-and-nowhere-else-in.md) | Mining happens on the Mine screen and nowhere else, in both input modes | accepted |
| [0126](0126-the-press-to-start-latch-puts-itself-down-after-15.md) | The Press to start latch puts itself down after 15 minutes with no key… | accepted |
| [0127](0127-the-settings-screen-swallows-q-and-ctrl-c-stops-being.md) | The Settings screen swallows q, and Ctrl-C stops being the same gesture | accepted |
| [0128](0128-r-restores-the-setting-under-the-cursor-and-there-is.md) | r restores the setting under the cursor, and there is no reset-all | accepted |
| [0146](0146-diagram-tool-wireframes-excalidraw-for-screen-layouts.md) | Diagram-tool wireframes (Excalidraw) for screen layouts | rejected |
| [0147](0147-truecolor-24-bit.md) | Truecolor (24-bit) | rejected |
| [0148](0148-abbreviated-numbers-1-23m.md) | Abbreviated numbers (1.23M) | rejected |
| [0150](0150-measuring-the-auto-repeat-delay-to-calibrate-hold.md) | Measuring the auto-repeat delay to calibrate HOLD_WINDOW | rejected |
| [0151](0151-querying-the-os-for-the-auto-repeat-delay.md) | Querying the OS for the auto-repeat delay | rejected |
| [0152](0152-auto-detecting-a-player-whose-auto-repeat-is-disabled.md) | Auto-detecting a player whose auto-repeat is disabled | rejected |
| [0153](0153-an-unbounded-accessibility-toggle-no-inactivity-cutoff.md) | An unbounded accessibility toggle (no inactivity cutoff) | rejected |
| [0154](0154-requiring-the-kitty-keyboard-protocol.md) | Requiring the kitty keyboard protocol | rejected |
