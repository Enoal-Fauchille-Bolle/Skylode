# Adding a mine

A thirteenth `MineKind`. Most of this list is written for you by the compiler; what is
here is the order to take it in, and the two places it will *not* stop you.

## 1. The blocks first, if they are new

A mine draws from two cells, and both are `Block` variants. If the mine reuses existing
blocks, skip to step 2.

Add the variant to `Block` in `crates/skylode-core/src/block.rs`, then run `cargo
check`. Every method on `Block` is an exhaustive `match`, so the compiler names them:
`material`, `name`, `hardness`, `world`, `min_pickaxe_tier`, `xp_value`, `drops`.

Two of those are not free choices:

- **`hardness` is Minecraft's number, unchanged.** That 1:1 fidelity is
  [0018](../decisions/0018-break-time-is-ceil-30-hardness-mining-power-minecraft.md), and
  it is what makes break *times* comparable to the source game. It is not a balance dial.
- **`drops` returns 9 for a dense form and 1 otherwise.** A dense block
  (`IronBlock`) is not the same thing as a Compressed unit
  ([0027](../decisions/0027-a-dense-block-and-a-compressed-unit-are-different.md)) —
  one is mined, the other is minted by hand.

`ALL_BLOCKS` in the same file is `#[cfg(test)]` and walks the table; add the variant
there too or the consistency tests will say so.

## 2. The mine

In `crates/skylode-core/src/mine_kind.rs`:

- Add the variant to `enum MineKind`.
- Extend `pub const ALL: [Self; 12]` to `13`. **The length is in the type**, so a
  forgotten entry and a duplicated one are both compile errors.
- `cargo check` now names `common_block`, `value_block`, `world`, `gating_tier`,
  `common_material`, `value_material` and `name`.
- Fix `all_mines_covers_every_variant` in the same file's test module. That test exists
  precisely for this moment: `ALL`'s length being in the type says nothing about the
  *enum*, so the exhaustive `match` inside the test is what refuses a thirteenth mine
  that no screen lists.

**`gating_tier` is the one with a rule behind it.** Both cells of a mine must gate
together — `common_and_value_share_a_gating_tier` asserts it — and the endgame ores sit
at the top of the ladder deliberately
([0036](../decisions/0036-the-end-s-ore-gates-behind-netherite-the-nether-s.md)): a rich
mine reachable with a starting pickaxe collapses the two-axis gate to one.

## 3. The colours

`crates/skylode-tui/src/palette.rs` holds `const PALETTE: [MinePalette; 12]`, one entry
per mine, and the same length-in-the-type trick applies.

The gate here is **pairwise, not global**
([0106](../decisions/0106-the-palette-is-24-entries-one-per-block-variant-and.md)): the
two cells of *this* mine must be distinguishable from each other, not from every other
block in the game. `every_pair_clears_the_contrast_gate` checks it. Hue follows
Minecraft, because a material is meant to be recognised rather than learned — the same
argument that keeps Amethyst's name
([0157](../decisions/0157-amethyst-keeps-its-name.md)).

Add the 16-colour fallback in the same entry. It is not optional: `ColourMode` is a
player preference and a missing fallback is a mine that is invisible to anyone who set
it.

## 4. What the compiler will *not* catch

Two things, and they are the reason this guide exists.

**The economy will price your mine without being asked.** `mine_size_cost` and
`mine_richness_cost` are keyed by `MineKind` and read the curve, so a new mine is
priced the moment it exists — plausibly, and quite possibly wrongly. Regenerate
[BALANCE.md](../BALANCE.md) and read the two new rows:

```sh
cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
```

If the mine has two materials, check that the sliding common-to-rare mix
([0055](../decisions/0055-mine-upgrades-size-and-richness-are-paid-in-that-mine.md))
reads sensibly at both ends of the track.

**The pacing band is a claim about a specific set of mines.** The two reference players
in `game.rs`'s balance harness walk the game as it is, so
`the_first_prestige_lands_inside_the_pacing_window` and
`the_completionist_ceiling_stays_inside_its_window` are now measuring a different game.
Run them. If the completionist's ceiling moved, that is real — a thirteenth mine is
thirteen more tracks for a completionist to max.

## 5. Saves

`MineKind` is a save key. A mine the player has never visited is absent from the map —
`current_mine()` is total because the mine in progress is a *field*, not a key — so
adding a variant does **not** invalidate existing saves and needs no `SAVE_VERSION`
bump. Removing or renaming one does; see
[changing-the-save-format.md](changing-the-save-format.md).

## 6. Verify

```sh
cargo test --workspace          # the table-consistency and balance tests
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-docs.sh      # if you touched docs/
SKYLODE_DEV=1 cargo run -p skylode-tui   # then ` to reach the mine without playing to it
```

The dev menu ([DEV-MENU.md](../DEV-MENU.md)) is how you look at the new mine without a
full climb: give yourself the tier and the level, then switch to it on the Mines screen.
