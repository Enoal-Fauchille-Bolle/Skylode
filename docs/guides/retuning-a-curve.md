# Retuning a curve

Changing a price, a slope, or any balance dial. The edit is one line; the work is
knowing what it moves.

## 1. Find the dial

Two questions, in order — they are the ones `tunables`' own module docs pose, and they
decide where a number lives:

**Is it a dial, or a design fact?** A dial is a number the balance pass is invited to
turn. A design fact is settled and lives with the thing it describes:
`TICKS_PER_HARDNESS` (30) and `Block::hardness` are Minecraft's values kept 1:1,
`RAW_PER_DENSE_BLOCK` (9) and Efficiency's `level² + 1` are settled with their reasons.
Filing one of those under a module called *tunables* would not describe it — it would
**invite someone to turn it**.

**Is it keyed by an enum variant?** If yes it is a `match` in that enum's module, not a
constant: `World::enchant_cap`, `PickaxeTier::efficiency_cap`,
`PickaxeTier::base_power`, `World::unlock_level`, `mine::MINE_SIZES`. That shape is also
the only one that turns a *new* variant into a compile error rather than a silent
default.

Keyed by nothing, and it is in `crates/skylode-core/src/tunables.rs`.

## 2. Know which half of the game you are moving

Curves here are `cost(n) = base * growth^n`, and the rule from
[0029](../decisions/0029-each-upgrade-track-carries-its-own-base-and-growth.md) is worth
having in hand before you type:

> **The base governs the early game and the slope governs the late one**, since
> `base * growth^0` is the base whatever the slope.

So raising a slope to make the game harder leaves the opening untouched and inflates
only the end. And each track owns its slope for a reason — one slope cannot price a
nine-step track and a fifteen-step one, which is why the Netherite enhancement was given
`1.10` while tier jumps kept `1.45`. Changing a shared slope moves every track that
reads it.

## 3. Regenerate the atlas

```sh
cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
```

[BALANCE.md](../BALANCE.md) is generated, never edited. `git diff docs/BALANCE.md` is
the fastest reading of what your one-line change actually did — it prints the whole
price list, so a slope that moved one column and flattened another shows up as a diff
rather than as a surprise in play.

## 4. Run the gates, and read them properly

Four tests measure pacing, and they are in `crates/skylode-core/src/game.rs`:

| Test | What it holds |
| --- | --- |
| `the_first_prestige_lands_inside_the_pacing_window` | the floor, ~1 h — the speedrunner |
| `the_completionist_ceiling_stays_inside_its_window` | the ceiling, ~2.3 h |
| `the_prestige_loop_settles_instead_of_walling` | the ladder's shape across ten ranks |
| `one_climb_still_banks_about_what_the_price_is_aimed_at` | that `AMETHYST_PER_CLIMB` is still true |

```sh
cargo test -p skylode-core prestige
cargo test -p skylode-core pacing
```

**One test per edge, because a band held at one end is not held**
([0030](../decisions/0030-the-pacing-target-for-a-first-prestige-is-a-band-1-h.md)). Two
readings that catch people out:

- **The floor is XP-gated, not cost-gated.** The speedrunner finishes its pickaxe before
  level 50 and then waits, so raising prices barely moves the floor. If you changed a
  price and the floor did not move, that is correct, not a broken test.
- **`AMETHYST_PER_CLIMB` (5 000) is measured, not chosen**, and it can go stale silently
  in the dangerous direction: if a climb starts banking *more* than 5 000, prestige
  becomes free. That is what the fourth test is for.

## 5. The tunables nothing measures

Four dials are not covered by any harness, and the honest position is that changing them
is unguarded: **enchant proc rates and cooldowns** (the reference players barely buy the
spatials), **the offline cap and the auto-miner rate** (an active-play harness never
idles), **the dip magnitude**, and **the XP curve** — which the pacing band constrains
implicitly, since the floor is XP-gated, but which nothing isolates.

These are the last item on [ROADMAP.md](../ROADMAP.md#where-this-stands). If you are
touching one, you are doing the measuring, not confirming it.

## 6. Record it if it is a decision

A retune inside an agreed shape is a commit. A change to the *shape* — a new track, a
different curve form, a cap moving — is a decision, and gets a numbered record in
[decisions/](../decisions/). The test is simple: would someone six months from now ask
"why is this 1.45?" and need more than `git log` to answer.

## 7. Verify

```sh
cargo test --workspace
cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
bash scripts/check-docs.sh
```

Then play it. `SKYLODE_DEV=1 cargo run -p skylode-tui`, `` ` `` for the dev menu, and
the time skip will put you where the change bites.
