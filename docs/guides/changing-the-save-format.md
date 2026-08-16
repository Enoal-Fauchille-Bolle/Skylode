# Changing the save format

Adding a field to saved state, or moving one. This is the guide with a trap in it, and
the trap has already cost one bug.

## First: does this need a bump at all?

`SAVE_VERSION` is **3**, in `crates/skylode-core/src/save.rs`.

| Change | Bump? |
| --- | --- |
| a new enum variant that is only ever a map key (a mine, a material) | no — an absent key is a state the player has not reached |
| a new field on saved state | **yes** |
| a field renamed, retyped, or moved between structs | **yes** |
| a field removed | **yes** |
| anything in `Config` | yes — config lives *inside* the save ([0091](../decisions/0091-config-lives-in-the-save-there-is-no-separate-config.md)) |

The argument that "no files exist yet" expired the day the front-end started writing
them, and it has already cost two bumps. If you are unsure, bump: a spurious version is
a no-op migration, a missing one is a save that will not open.

## 1. Add the field

Ordinary `serde` on the type that owns it. Two rules the format depends on and that are
easy to break without noticing:

- **Every map is a `BTreeMap`.** The same run must write the same bytes, or the golden
  save becomes noise and the autosave dirties itself every tick.
- **`Item` is a hand-written word key** (`"compressed_iron"`), deliberately kept apart
  from `Material::name` so the UI can reword a display name without rewriting saves.

## 2. Bump `SAVE_VERSION` and write the migration step

Add the step to `migrate()`. It takes and returns the document, because that is the
shape a chain wants: a v1 file read by a v4 build travels 1 → 2 → 3 → 4, and no step has
to know about more than its own successor.

**Grandfather; do not `serde(default)`.** This is the rule, and the `2 → 3` bump is why
it is not a style preference. `granted_ticks` is the total a running boost was granted,
and the gauge is a fraction of it. `serde(default)` would give a running boost a total
of `0` — which `Boost::validate` refuses. The shortcut turns an honest mid-boost save
into a *damaged* one.

The same reasoning shaped `1 → 2`. `auto_raw_credited` records what the auto-miner has
paid over the save's life, and a v1 file simply does not contain it. Both obvious
defaults are wrong: `0` makes an honest week-long absence look impossible and refuses
the file; `u64::MAX` switches the audit off forever. So the migration **grandfathers the
present** — whatever ore the file holds is accepted as earned, and everything from that
load on is audited against real counters.

**Watch the `==` versus `<=`.** `grandfather_auto_total` runs on `== 1`, because a v1
file that has just been through it now *has* the field and asking again would overwrite
a computed value. `grandfather_boost_total` runs on `<= 2`, because neither v1 nor v2
ever wrote it. Chained steps make each condition's range load-bearing.

## 3. The trap: a field nested inside an optional one

**`the_written_shape_is_pinned` is not the test that protects you.**

The golden save is a run written out and compared byte for byte. But `granted_ticks`
lives inside `active_boost`, which is an `Option` — so a run that never fired a charge
writes a **byte-identical document**, and the golden save cannot see the change at all.
The `2 → 3` bump exposed this.

When a new field is nested inside an optional one, the golden save is blind to it. Write
a **migration test**: a hand-built document at the old version, run through `from_json`,
asserted on the far side.

## 4. Validate

`from_json` **validates before returning**, and that is not belt-and-braces:
deserialisation writes private fields directly, so it is the only input that reaches
state without passing a rule.

Each type owns its own `validate`. **Refuse, never clamp** — a clamped save is a save
whose numbers silently stopped being the player's.

Note the deliberately missing half: the cross-field plausibility checks phase 11 listed
and left. They wait on the unmeasured tunables, because a `validate` tightened against a
guess refuses honest runs. See [ROADMAP.md](../ROADMAP.md#where-this-stands).

## 5. What lives outside the core

`save` holds serialisation and migration only, **never I/O**. The disk half — the atomic
write, the `.bak`, the HMAC, the clock — is the front-end's, in
`crates/skylode-tui/src/persist/`.

Two consequences. The HMAC covers the whole save including config
([0092](../decisions/0092-the-hmac-covers-the-whole-save-config-included-no-hand.md)), so
a player who wants a different palette must never need to touch the file — Settings has
to expose every config field and no game-state field, or the tamper warning fires on an
honest player. And the version lives **inside** the signed payload, because it selects
the migration and so must not be editable.

## 6. Verify

```sh
cargo test -p skylode-core save
cargo test -p skylode-core the_written_shape_is_pinned
cargo test --workspace
```

If `the_written_shape_is_pinned` fails, the question is not "what are the new bytes?"
but **what just happened to every save on disk**. Same for the RNG's
`the_sequence_is_pinned_to_a_golden_vector`: the draw order is a contract, and moving it
rerolls every existing run.

Then load a real save. Keep a copy of one written by the previous build, put it back in
place, and open it — a migration that compiles is not a migration that works.
