# CLAUDE.md

Guidance for Claude Code working in this repository. Tracked, so
`scripts/check-docs.sh` reads it like any other document — which is what stops a
shortcut in here quietly outliving the code it describes.

## What it is

A solo, terminal-based idle/incremental mining game in Rust, inspired by PikaNetwork's
SkyMines. **Playable and pre-1.0.** Two crates:

- **`skylode-core`** — the rules. Pure, deterministic, UI-agnostic, **no I/O**. Its only
  dependencies are `rand` and `rand_chacha`, and both are `default-features = false` so
  ambient OS entropy is *not compiled in*: the determinism contract is enforced by the
  compiler, not by discipline. Keep it that way. No wall-clock reads either — the caller
  injects `now`.
- **`skylode-tui`** — ratatui/crossterm. Renders core state and forwards input, nothing
  more. Raw input becomes a semantic action exactly once, in `keymap`, so `App::update`
  is testable without a terminal.

Keep that boundary: the core must stay testable without a terminal, and other
front-ends must remain possible.

## Commands

```sh
cargo run -p skylode-tui          # play it: Space mines, q backs out to the title
cargo test                        # the whole workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc-all -p skylode-core --open           # NOT `cargo doc` — see CONTRIBUTING.md
cargo tarpaulin --fail-under 94 --out Stdout   # coverage gate, run by hand
bash scripts/check-docs.sh                     # the documentation's linter
cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
```

`doc-all` is a `.cargo/config.toml` alias for `doc --document-private-items`; plain
`cargo doc` silently drops every link to a `pub(crate)` item, which is most of the
interesting ones. The `pre-commit` hook runs fmt, check, clippy and **doc with
`-D warnings`**; tarpaulin and `check-docs.sh` are deliberately *not* in it, so both are
conventions run by hand and gates run in CI. Both hooks bypass everything on a `dev/*`
branch. Install once per clone with `.githooks/setup-hooks.sh`.

## Conventions

- **Prefer errors over panics.** Rules that can refuse return `Result<_, CoreError>`.
  Workspace lints deny `correctness` and `suspicious`, warn on `unwrap`/`expect`/
  `panic`/`todo`/`unimplemented`. A **new crate needs `[lints] workspace = true`**.
- Rust edition 2024, stable toolchain, MSRV 1.88 (measured, and a CI job keeps it).
- **Tests are inline** — one `#[cfg(test)] mod tests` per module, no `tests/` directory,
  and the coverage tool is configured on that assumption. Test names are sentences
  (`a_fresh_pickaxe_is_wooden_and_unenchanted`).
- **Rustdoc explains the constraint, not the mechanics** — *why* a formula or a
  visibility is shaped that way. Match the density of `PickaxeTier::base_power` or
  `Block::drop_amount`. A comment arguing for behaviour the code no longer has is worse
  than none.
- Use `expect(dead_code, reason = "awaiting the phase-N …")`, naming the phase that will
  consume it, rather than making a half-finished path public.
- **Commits**: Conventional Commits, single line, ≤72 chars, imperative, lowercase, no
  trailing period. Nine types (`feat` `fix` `docs` `refactor` `chore` `style` `test`
  `build` `ci`) — no `perf`, no `revert`. The `commit-msg` hook enforces it.

## Citing the documentation

Two forms, and the linter checks both.

- **A decision** is cited as `docs/decisions/0069` — the number, never the filename.
  A record's slug comes from its title, so embedding one would make rewording a title
  break every citation of it. Markdown may use a full link instead; rustdoc may not,
  because a relative link would resolve from `target/doc`.
- **A section** is cited as `docs/UI.md` §5.2, and the document must be named on that
  line or the one above — the example here is a real citation for that reason. UI.md
  renumbered when the overlays became a chapter, so an old number often still
  resolves, to a *different* section.

## One fact, one home

The rule the documentation is organised around. Do not restate a fact in a second
place — link to it.

| Authoritative on | Lives in |
| --- | --- |
| numbers, execution order, invariants | the **rustdoc**, beside the code |
| what the game must do | `docs/` |
| why, and what was rejected | `docs/decisions/`, one numbered record each |
| how to make a change | `docs/guides/` |
| what is left to do | `docs/ROADMAP.md` |

**Start at [docs/README.md](docs/README.md)**, which routes by what you are trying to
do. Before proposing gameplay or systems changes, read
[docs/decisions/](docs/decisions/) — it records what is settled and what was explicitly
rejected, and cite a decision by number. The phases have shipped, so `docs/` now lags
`crates/` more often than it leads: where a document and the code disagree on a
**number or a behaviour**, the code is right and the document is stale — say so rather
than coding to the document. A disagreement about **intent** is a design question, not
a stale fact: open a decision record.
