# Contributing to Skylode

Thanks for your interest in Skylode. The game is **playable and pre-1.0**: the
mining loop, the six screens, the enchants, prestige and the save system all work.
What stands between here and `1.0.0` is the tail of the balance work — see
[docs/ROADMAP.md](docs/ROADMAP.md) for the scope, and the § *Versioning and
releases* below for what that version number promises. Code contributions are
welcome, but please open an issue to discuss before starting a large change, so
effort stays aligned with the roadmap.

## Prerequisites

- A stable Rust toolchain (the crates use edition 2024).
- `cargo` (bundled with Rust via [rustup](https://rustup.rs)).

## Build, run, and test

```sh
cargo build --release   # build the workspace
cargo test              # run the test suite
cargo run -p skylode-tui  # play it: Space mines, q quits
```

A debug build also carries a **dev menu** — ore out of nothing, free upgrades, a time
skip — behind two gates: `SKYLODE_DEV=1 cargo run -p skylode-tui`, then `` ` ``. It is
compiled out of `--release` entirely. See [docs/DEV-MENU.md](docs/DEV-MENU.md).

## Scripts

Five tools live in [scripts/](scripts/). Each argues its own purpose in its header;
what follows is when you would reach for one.

```sh
scripts/check-docs.sh        # the documentation's linter — links, anchors, Rust names,
                             # line width, and the decision records' numbering. Runs in CI.
scripts/coverage.sh          # the workspace's coverage, as a browsable per-line report
scripts/diff-coverage.sh     # coverage of what *this branch* changed (needs diff-cover)
scripts/kitty-test.sh        # does this terminal speak the kitty keyboard protocol?
scripts/unsaved-game.sh      # run the game where it cannot find or create a save (Linux)
```

**`diff-coverage.sh` is the one to run before a large commit.** The workspace figure
sits near 99 %, which makes it useless as a review signal — a hundred new uncovered
lines move it by less than a point — and the 94 % gate in CI only refuses a collapse.
Scoring the diff alone answers the question a reviewer actually has. It needs
`diff-cover`, a Python tool: `pipx install diff-cover`, or
`pip install --user --break-system-packages diff-cover` on a distribution that marks its
Python externally-managed. Compare against another branch with `COMPARE_BRANCH=…`.

`kitty-test.sh` answers a question that comes up as *"why does `Space` fire twice for
me?"*. The mine key is two layers — a hold inferred from a time window everywhere, and a
real key release where the protocol reports one — so the terminal's answer decides which
path you are debugging. `unsaved-game.sh` reaches the no-persistence branch: the title
screen's permanent warning, and an autosave that fails on the edge without being fatal.

## Code style

- Format with `cargo fmt` before committing.
- Keep the workspace lint-clean with `cargo clippy`. The workspace denies the
  `correctness` and `suspicious` lint groups and warns on `unwrap`, `expect`, and
  `panic` (see `[workspace.lints.clippy]` in `Cargo.toml`). Prefer returning
  errors over panicking.
- Game rules belong in `skylode-core` (kept deterministic and testable); the TUI
  in `skylode-tui` only renders and forwards input. See
  [docs/SYSTEMS.md](docs/SYSTEMS.md).
- Public items carry rustdoc that explains *why* a formula or a visibility is
  shaped the way it is — the constraint, not the mechanics. `PickaxeTier::base_power`
  and `Block::drop_amount` are the density to match.
- Keep that rustdoc honest. A comment arguing for behaviour the code no longer
  has is worse than no comment: the next reader trusts it and stops reading the
  code.
- Tests are inline — one `#[cfg(test)] mod tests` per module. There is no
  `tests/` directory, and the coverage tool is configured on the assumption that
  there is not.
- Test names are sentences, not labels
  (`a_fresh_pickaxe_is_wooden_and_unenchanted`). A failing test should read as
  the claim it just disproved.

## Commit messages

This project follows [Conventional Commits](https://www.conventionalcommits.org):

```text
type(scope)!: subject
```

- The subject is one line of 72 characters at most, imperative mood, lowercase,
  with no trailing period (for example `feat(core): add fortune multiplier`). It
  may end with a closing parenthesis when it names code that way
  (`refactor(core): shut the free upgrade paths behind pub(crate)`), as long as
  the parentheses it opens are the ones it closes.
- A body is optional; separate it from the subject with a blank line.
- The scope is optional; use it when the affected area is obvious (`core`, `tui`,
  `docs`).
- The `!` is optional and marks a breaking change; it goes after the scope, just
  before the colon (`feat(core)!: make every block drop its own material`).
- Types: `feat`, `fix`, `docs`, `refactor`, `chore`, `style`, `test`, `build`,
  `ci`. The list is closed: the `commit-msg` hook rejects anything else.

## Versioning and releases

One version covers the whole workspace. It lives in `[workspace.package]` at the
repository root, and both crates inherit it with `version.workspace = true`. That
is a claim about the product rather than about the code: the two crates always ship
together, inside one binary, to one player. If `skylode-core` is ever consumed by
something other than this front-end, its version becomes an API contract and has to
move on its own.

Releases are **annotated** tags named `vX.Y.Z` (`git tag -a v0.2.0 -m "…"`). The
tag is the only thing that starts a release; nothing else does. Tags that are not
versions (backup markers, for example) go without the `v` prefix, which is what
keeps the two kinds apart.

### What a tag does

Pushing one runs [`release.yml`](.github/workflows/release.yml), which re-runs the
whole check suite and then, only if it passes, produces four things:

- **Three archives** — Linux x86_64, Windows x86_64, macOS arm64 — each carrying the
  `skylode` binary, `README.md` and `LICENSE`, plus a single `SHA256SUMS`.
- **A build provenance attestation** per archive, signed with the job's short-lived
  OIDC identity. It answers what `SHA256SUMS` cannot: that file is published by the
  same account to the same page, so it catches a corrupted download and says nothing
  about a substituted one. Verify with
  `gh attestation verify <archive> --repo Enoal-Fauchille-Bolle/Skylode`.
- **Both crates on crates.io** — `skylode-core` and `skylode-tui` — published by
  Trusted Publishing rather than a stored token, so no registry credential exists in
  this repository to leak. The job asks the registry about each package separately
  and publishes only the ones that are missing: re-tagging is the ordinary way a
  version is already there, and a workspace-wide publish refuses outright when it is.
  One thing a tag cannot do, though: **a crate's first version has to be uploaded by
  hand**, because crates.io only lets a Trusted Publisher be declared for a crate that
  already exists. Use a token scoped to `publish-new`, then revoke it.
- **A release body**, taken from `.github/release-notes/<tag>.md` when that file
  exists and generated by `git-cliff` from the commit history otherwise.

Two things to do by hand before tagging, in this order:

1. Bump **both** the version in `[workspace.package]` *and* the `version` on
   `skylode-tui`'s path dependency on `skylode-core`. They are two places for one
   number; forgetting the second is caught immediately, because `cargo check` then
   refuses to resolve `skylode-core = "^<old>"` against the new version.
2. Decide whether this release wants a hand-written body. Prose is worth it when the
   release speaks to a player; a plumbing release reads better generated.

### What the three numbers promise

Skylode is a game, not a library, so its public contract is not a set of `pub fn`
signatures. It is what a player relies on: **their save file opens**, **the keys do
what they did**, and **the run they are in the middle of still makes sense**. So the
question to ask of any release is the one a player would ask — *if I update without
doing anything, do I lose something?*

- **Breaking** — a save from the previous version cannot be loaded, a binding
  changed under the player's fingers, or progress they had banked is gone.
- **Added** — new content or a new capability, with everything that worked still
  working.
- **Fixed** — corrections only; nothing moved.

Note that "breaking" is about what the player experiences, not about what the code
changed. A save-format change shipped **with a migration** is not breaking: the file
still opens. A change that leaves every signature intact but desynchronises existing
saves — a different RNG draw order, say — *is* breaking, and no compiler will say so.

### Which number moves

While the version is below `1.0.0`, **Cargo treats the minor as the breaking axis**
(`0.2.3` resolves as `>=0.2.3, <0.3.0`), which differs from upstream SemVer's "no
guarantees before 1.0". So, pre-1.0:

| Change | Bump |
| --- | --- |
| Breaking, or added | `0.MINOR.0` |
| Fixed | `0.x.PATCH` |

Against the commit types above: `feat` and any `!` take the minor; `fix`,
`refactor` and `style` take the patch; `docs`, `chore`, `test`, `build` and `ci` do
not justify a release on their own. Lower components reset to zero — after `0.4.7`,
a feature gives `0.5.0`, not `0.5.7`.

**That mapping is a first filter, not the answer**, because it reads the type and
the contract above is about the player. The question that settles it is whether the
change reaches **their binary or their save**, and the scope usually tells you:
`fix(ci)`, `fix(test)` and a fix inside the dev menu all take the patch by the line
above and are worth *no release at all*. The dev menu is the clearest of the three —
it is `#[cfg(debug_assertions)]` throughout, so `cargo build --release` does not
compile it, and the change cannot reach a player even in principle.

The rule runs the other way too, which is the half that is easy to miss. A
`build(deps)` bump justifies no release on its own — until it closes an advisory in
a crate that ships inside the binary. At that point somebody downloading an archive
is running the vulnerable code, and the release is the entire point. The commit type
says no; the player says yes; the player wins.

`1.0.0` is reserved for one condition, and it is checkable rather than a matter of
taste: **the MVP list in [docs/ROADMAP.md](docs/ROADMAP.md) is complete**. Tagging it
is a promise that breaking anything afterwards costs a `2.0.0`.

### `SAVE_VERSION` is a separate number

`skylode_core::save::SAVE_VERSION` is a migration selector, not a version of the
game. It advances only when the on-disk document changes shape, for reasons that
have nothing to do with the product's maturity — it reached `3` across two releases
nobody shipped. Tying the two together would force a format bump for nothing, or
make a bugfix release lie about the format. The relationship belongs in the release
notes as prose (*"`SAVE_VERSION` 3 → 4, migrated on load"*), never in the numbers.

## Git hooks

The hooks in [.githooks/](.githooks/) check the rules above so a broken commit
never reaches the history. Git does not version the hooks path, so install them
once per clone:

```sh
.githooks/setup-hooks.sh     # macOS, Linux, WSL
.githooks/setup-hooks.ps1    # Windows, PowerShell
```

Both scripts do the same thing: `git config core.hooksPath .githooks`, which
points git at the versioned hooks instead of the local `.git/hooks/`. Undo it
with `git config --unset core.hooksPath`.

- **`pre-commit`** runs `cargo fmt --check`, `cargo check`, and `cargo clippy -D
  warnings` over the workspace, and refuses the commit on the first failure. It
  is the slow one, since it compiles.
- **`commit-msg`** validates the commit message against the rules above: type
  from the closed list, optional lowercase scope, optional breaking-change `!`,
  subject in the imperative mood and 72 characters at most, no trailing period,
  balanced parentheses, body separated by a blank line.

Both hooks step aside where enforcing them would only get in the way:

- **On a `dev/*` branch**, both bypass every check. Work in progress stays cheap
  there; rewrite the history before merging into `main`.
- **On the messages git writes itself** (merges, reverts, `fixup!`/`squash!`),
  `commit-msg` bypasses validation. They cannot follow the convention and are not
  typed by hand. This is also why there is no `merge` type: a merge is already
  identified by its two parents, not by its message.

Skipping a hook for a single commit is `git commit --no-verify`. Reach for it
rarely, and never on `main`.

## Design context

Before proposing gameplay or systems changes, read the design documents in
[docs/](docs/), especially [DECISIONS.md](docs/DECISIONS.md), which records what
has already been settled or rejected and why.

To reach a state past the first hour of a run without playing to it, use the dev menu
([docs/DEV-MENU.md](docs/DEV-MENU.md)).
