# Skylode - Systems

Technical systems that support the game: the save system, the tech stack, and the
architecture. For player-facing rules, see [MECHANICS.md](MECHANICS.md). For the
concept and gameplay loop, see [DESIGN.md](DESIGN.md).

## Save system

Fully preventing cheating in a single-player offline game is impossible: the save
and any key live on the player's machine. The goal is to make accidental
corruption unlikely and casual tampering harder. This is deterrence, not DRM. DRM
(technical measures that restrict how software is used or copied) is out of place
for a free solo game.

### Format

A single JSON file via `serde_json`. The state is one cohesive blob, so SQLite
would be over-engineering: there are no relational queries, no large datasets, and
no partial updates. JSON is simple and human-debuggable.

### Where the file lives

One file, at the platform's own data location, resolved through
`directories::ProjectDirs` — with the backup as `save.json.bak` beside it:

| Platform | Path |
| --- | --- |
| Linux | `~/.local/share/skylode/save.json` |
| macOS | `~/Library/Application Support/skylode/save.json` |
| Windows | `%APPDATA%\skylode\data\save.json` |

**The Windows row was wrong until the loader was written**, and it is corrected here
from `directories`' own source rather than from the shape the other two rows have.
`ProjectDirs::data_dir` appends a `data` component on Windows — roaming application
data conventionally holds several categories side by side, so the crate keeps
`config`, `data` and `cache` apart under one project folder — while Linux and macOS
name the category in the path above the project. The call is
`ProjectDirs::from("", "", "skylode")`: the empty qualifier and organisation are
dropped on macOS (a bundle identifier of just `skylode`) and ignored on Linux, so the
project folder is the bare application name on all three.

Only the Linux row has been *observed*; the other two are read off the library's
implementation. That is the trade
[0113](decisions/0113-the-save-lives-at-the-platform-s-own-data-location.md) already accepted when
it chose the library — the point of a crate that knows three platforms is not to
second-guess its conventions from one of them — and the extra component is a directory
name, not a rule.

**This revises an earlier "no XDG handling"**, which was written against a
different question. What that decision rejected — and still rejects — is
*splitting* the game across several XDG categories: preferences under `~/.config`,
run state under `~/.local/state`, save data under `~/.local/share`. That split is
exactly what [config in the save](#config-in-the-save) exists to prevent, and it is
untouched: there is still one file, and still no separate config file.

What changes is where that one file goes. A dot-directory in `$HOME` is precisely
what the convention exists to prevent, so *"do not handle XDG"* and *"do not
pollute the player's home"* pull in opposite directions. The second wins, because
the first was a statement about **complexity**, and a library reduces that
complexity to a single call. Hand-rolling the lookup would be ten lines on Linux
and wrong on the other two platforms — and a partial reimplementation of a standard
is more fragile than none, because it looks correct.

`ProjectDirs::from` answers `None` where the platform cannot say — no home
directory at all, rare but real inside a container. That case **starts the game
without persistence and says so**, rather than refusing to launch: a player who
cannot save should still be able to play.

### Saved state

The `data` blob (see [Integrity](#integrity-hmac) below) serializes one cohesive
game-state struct. The fields, derived from the mechanics:

- `version`: schema version, for migrations. **Inside the blob**, not beside it —
  see [Integrity](#integrity-hmac).
- `rng`: the seeded PRNG state — a *position in a sequence*, not just the seed, so a
  reloaded run continues its dice rather than rerolling them.
- `last_seen`: wall-clock time of the last write, for offline accrual. Written as
  whole seconds since the Unix epoch, and a clock set before 1970 clamps to it
  rather than failing the write: losing a run to a wrong clock is the one outcome a
  save system must not have.
- `player`: the pickaxe (tier plus each enchant's level), the inventory, the mining
  level and banked XP, the prestige rank, and the XP carry. The **unlocked worlds
  are not a field**: they are derived from the level, a monotone function of state
  already stored, so a second copy would only be an invariant to maintain by hand —
  and prestige, which resets the level, re-locks them for free. The prestige
  *multiplier* is not a field either, for the same reason: it is a function of the
  rank.
- `inventory`: a map from item to count, written with **word keys** — `"iron"`,
  `"compressed_iron"`. JSON object keys must be strings, and the key table is
  deliberately separate from the display name so the UI can reword "End Stone"
  without invalidating every file on disk.
- `mine` and `visited`: the mine the player is in, and every mine they have entered
  and left — each with its size, its richness level and dial, and its grid, holes
  included. Two fields rather than a map plus a selected key, so "the mine in front
  of the player" is a value that is always there rather than a lookup that can miss.
  A mine never visited has no state worth storing: its grid is a function of its
  kind and the generator.
- `boost_charges` and `active_boost`: the **reserve of unspent charges** (a count —
  every boost in the game is identical, so nothing else distinguishes them), plus
  any running boost and its remaining timer. The reserve is a field of its own
  because level-up grants charges the player has not fired: dropping it would make a
  reload eat every charge earned and not yet spent. A running boost carries **two**
  timers, not one — what it has left and what it was granted — because charges stack
  by addition and a gauge needs the second to be a fraction of anything. See
  [UI.md](UI.md#51-mine) §5.1 for what the pair draws, and `SAVE_VERSION` 3 below for
  what it cost.
- the **carries**: the auto-miner's unpaid fractions of a common and a value cell,
  and the prestige multiplier's unpaid fraction of each item. They look like
  bookkeeping and are not: dropping them turns a fractional rate into a floor, which
  at a low enough rate is zero forever.
- `config`: the player's *preferences* — colour palette (256 or the 16-colour
  fallback), ASCII-only glyphs, mining input mode, number format. **Not** game
  state, but it lives here anyway: see below. The core carries it as a **type
  parameter** and never learns what it is — the front-end gets its own type back.

All the maps in the file are **ordered**, not hashed, so the same run always writes
the same bytes: a text that varies from write to write can be neither pinned by a
test nor diffed against another save.

### A load validates before it returns

Deserialization writes private fields directly, so it is the one input that reaches
the game state without passing a single rule. A file that parses and still describes
a run the rules could not produce — a richness dial above the ceiling bought for it,
a grid that is not the size its level claims, a level off the ladder, an inventory
entry at zero, a boost that is a *slow* — is **refused, never repaired**. Clamping
would hand the player a run that is not the one they saved, and the recovery screen's
backup is seconds old.

The per-field half of this is aimed at a bug in a migration we write rather than at a
player with a text editor; the HMAC below is what turns away the plain hand-edit. The
cross-field half is aimed squarely at the case *between* those two — the tamperer who
has the key and re-signs — and it is the only layer that keeps working after the key is
out.

#### The cross-field audit

Every check above reads **one field**. A level inside the ladder, a carry under its
denominator, a dial under its ceiling — each is a fact about a number on its own, and a
file satisfying all of them can still describe a player holding a billion Diamond after
forty minutes. That is exactly what a tamperer who has found the key produces, so
`GameState::validate` closes with three checks that compare fields *against each other*:

| Check | Reads | Bounded by |
| --- | --- | --- |
| blocks against time | `blocks_broken`, `playtime` | one tick breaks at most one full grid |
| the ladder against blocks | level, prestige rank, `blocks_broken` | experience comes only from broken blocks |
| the purse against everything | inventory, `blocks_broken`, `auto_raw_credited` | mining, the reward ladder, the auto-miner |

**Every bound is a deliberate over-estimate.** A save an honest player wrote and this
refuses is a run lost to a guess — the worst outcome the save system has — while a cheat
that squeaks under a ceiling ten thousand times too generous had to be coherent across
the whole document to get there. So each ceiling is derived from a constant the rules
already enforce (the best block in the game, the biggest grid, the full reward ladder at
every rank) and rounded the generous way at every step. Nothing here is measured from
play, and nothing here should ever be tightened to what a *typical* run does.

**`auto_raw_credited` is a counter added for this**, and it is the only one of the three
that could not be derived. The auto-miner pays out during absences that leave no mark on
`playtime` — an absence is credited in closed form, never replayed — and the number of
absences a save has lived through is unbounded, so without a running total the only
sound ceiling on the player's ore would be "however many seven-day windows they might
have slept through", which is no ceiling at all. It took `SAVE_VERSION` to 2, and the
`1 → 2` migration **grandfathers** rather than defaulting to zero: a version-1 file
cannot say what its auto-miner paid out, and a `0` would be false in the direction that
accuses an honest save.

**`SAVE_VERSION` 3 is `granted_ticks`**, the second timer on a running boost, and its
`2 → 3` migration grandfathers by the same doctrine: a version-2 boost is given a total
equal to what it has left, so the gauge reopens full and drains over whatever remains.
The two rejected values are rejected the same way — the field is a *missing fact* and
not an *absence*, so nothing may default it. Zero is the sharper case here, because it
does not merely mislead: `Boost::validate` refuses a boost holding more time than it was
granted, so a `serde(default)` would turn every mid-boost save into a **damaged** one.
The thirty-second constant fails for the same reason on any stack. And `serde` could not
have helped even in principle, since a default that had to equal a *sibling* field is
one serde never gives a way to write.

The bump is also the case that shows the two versions are not the same kind of change.
`auto_raw_credited` altered the document every save writes; `granted_ticks` lives inside
`active_boost`, so a run that has fired no charge writes a version-3 file byte-identical
to the version-2 one but for the number at the front. **The golden save cannot see it** —
its fixture has no boost running — which is why the migration's own tests, rather than
the pinned document, are what stand between the bump and a file nothing checks.

**What it does not catch, and cannot.** A tamperer holding the key can raise the
counters alongside the purse and satisfy all three; nothing inside a file can prove a
file. What the audit buys is that the lie must now be told consistently in four places
instead of one. Measured against the real thing: a save inflated to a billion of every
material is refused, and so is one whose `blocks_broken` was erased, while a prestige
rank raised from 0 to 10 slips through *on a mature run* (11 869 blocks broken pays for
1.7 M experience against the 1.35 M eleven climbs demand) and is refused on a younger
one. That is the over-estimate working as designed, not a check failing.

### Config in the save

There is deliberately **no separate config file**. One file, one path — see
[Where the file lives](#where-the-file-lives) for which path, and for what that
sentence used to claim about XDG and no longer does.
Prestige does not touch the file — only the player deleting it does — so
preferences survive a run. One cost is accepted knowingly: deleting the save loses
the preferences.

**The second cost turned out not to be one.** This paragraph used to read that adding
a config field bumps `version` and needs a migration like any other schema change.
It does not have to: a new preference may carry `serde(default)`, and here — unlike
the core's play counters — the default is *honest*. A preference missing from an older
file means the player never expressed one, which is exactly what the default says. A
default may stand in for an **absence**; what it may not stand in for is a fact we
failed to record, which is why the core's `blocks_broken` refused the same treatment.

The front-end's `Config` was made serialisable without a `SAVE_VERSION` bump for the
other half of that argument: a bump protects files already written, and until the
loader shipped there were none.

The consequence that matters is about the [HMAC](#integrity-hmac), which covers the
whole file, config included. **No hand-editing is tolerated, and Settings is the
only path to change a preference.** That is only tenable under one rule:

> The Settings screen exposes **every config field, and no game-state field**.

Both halves are load-bearing. Exposing every config field means nobody ever needs
to open the file to change a colour, so the tamper warning never fires on a
cosmetic edit — the HMAC's false positive goes to zero. Exposing no game-state
field means a player editing their Amethyst count still has to touch the file, and
still trips it — the HMAC's true positive is untouched.

It also forces a bootstrap rule. Config is inside a save that may be **missing**
(fresh install) or **untrusted** (HMAC mismatch), so the screens that run before
the save is validated — main menu, "terminal too small", and the recovery screen
below — **render with hardcoded defaults**. Reading preferences out of a save you
have just decided not to trust is a contradiction, and the recovery screen is the
first thing some players ever see.

### Save cadence

- Autosave every 10 seconds while a run is up.
- On important transactions (upgrade, prestige).
- On graceful exit — `q` back to the title, `Ctrl-C`, and a dead event source.
- On **opening** a run, before the player has pressed anything.
- Update `last_seen` on every write, so offline accrual stays correct (see
  [MECHANICS.md](MECHANICS.md#offline-accrual)).

**The `dirty` flag this section used to ask for is deliberately not there**, and the
reason is a mechanic that arrived after it was written. The auto-miner credits on
*every* tick, so "has the state changed since the last write" is a `bool` that cannot
be false while a game is up — a field that lies about what it is for. What the flag was
meant to save is instead structural: the title screen, the recovery frames and the
offline summary hold no run, so the loop over them never reaches this clock.

**Opening a run writes it, and that is not belt-and-braces.** A `New game` abandoned in
its first seconds would otherwise leave a title with nothing to continue; and on the
`Continue` path `GameState::resume` has just credited an absence *in memory*, so a
crash before the first cadence would measure the next absence from the old mark and pay
for the same hours twice.

**A failed write is not fatal, and it is announced on the edge.** The run in memory is
fine, and throwing it away would be the opposite of what *"no continue anyway"*
protects. A full disk fails every ten seconds, so the toast fires when the answer
*changes* — once when saving breaks, once when it works again — and the case repairs
itself without relaunching.

**Moving `last_seen` belongs to the write, not to the caller**, and the ordering is
why. `persist::save` calls `GameState::touch(now)` and *then* serialises; a caller
that touched afterwards would write the previous mark and have the next absence
measured from a moment that has already been paid for. The clock is **injected** —
`save(slots, state, config, now)` — so this module reads no clock of its own, the
same rule the core follows and for the same reason: a wall-clock read buried inside a
write is a dependency no test can choose.

### Integrity (HMAC)

An HMAC is a keyed hash. On save: serialize the state to text, compute
`mac = HMAC-SHA256(key, text)`, and write:

```json
{ "data": "<serialized state, version included>", "mac": "<hmac hex>" }
```

**`data` is a JSON *string* holding escaped JSON — not a nested object**, and that is
a requirement rather than a formatting choice. A MAC covers an exact sequence of
bytes. Nested, `data` would come back from the parser as a tree, and recomputing the
MAC would mean **re-serialising** it — which `serde_json` does not promise to do
byte-identically: key order, number formatting and escape choices are all free. A
perfectly valid save would then fail its own signature, intermittently. As a string,
the parser hands the payload back unchanged and there is nothing to rebuild.

The cost is accepted knowingly: the file reads as `{"data":"{\"version\":1,…`, which
spends most of what "human-debuggable" bought above. `jq -r .data save.json` restores
it in one command, and the alternative is a bug that appears on some saves and not
others.

One consequence, so it is not mistaken for a hole: the MAC is taken over the payload
**unescaped**, so a file re-escaped differently — `A` where we wrote `A` — yields
the same payload, the same MAC and the same run. That is two spellings of one
document, not a forgery.

The `mac` is 64 lowercase hexadecimal digits, and the reader accepts nothing else —
no uppercase, no other length. Encoding and decoding are ~15 lines in `persist`
rather than a `hex` dependency.

**The version is inside `data`, not beside it.** The MAC covers `data` alone, so a
version in the envelope would be the one field a tamperer could edit freely — and it
is precisely the field that decides which migration runs. Signed, it cannot be used
to steer the loader. Nothing stops the envelope from *repeating* it later as a
routing hint; hoisting it out of the signature is the move that could not be undone.

The key is embedded in the binary. On load: recompute the HMAC over `data` and
compare to the stored `mac`. A match means intact; a mismatch means modified or
corrupted. This is tamper detection, not prevention: the embedded key is
extractable. It catches hand-editing and corruption, not determined cheating.

**The key is stored obfuscated, and that is the whole of the hardening.** It is
held masked — each byte XOR-ed against a pattern — and reassembled at run time, so
the plain bytes never appear in the binary. This is the one step worth taking: it
moves the attack from a single command needing no skill to reading the program, which
is where the trade's own rule of thumb puts save editing into the *not worth the
effort* basket for most players.

**The mask is derived, not stored, and that came from a measurement.** The first
version of this held *two* 64-byte constants and XOR-ed them. The plain key was
genuinely absent from the binary — that half worked — but the two arrays were declared
one after the other and the compiler laid them out one after the other in `.rodata`.
That is enough to lose the key without a debugger at all: slide a 128-byte window over
the whole file, XOR its two halves, and test each candidate against the `mac` of any
save you already own. Measured against the shipped binary, that search found the key in
**1.9 seconds** from thirty lines of script. The secret was never the bytes; it was
their adjacency.

So there is no second array. A 16-byte seed is grown into the 64-byte mask by two
chained SHA-256 rounds at run time, and a window search has nothing to slide against.
The rewrite deliberately **did not change what `key()` returns** — the stored constant
was recomputed so the output is byte-identical — because a new key is a wiped disk for
every save in existence and there is no migration for a signature.

**Two implementation details that are the whole of whether it works.**

*Reassembly must not happen at compile time.* A `const fn` called in a `const`
position is folded by the compiler, which would write the plain key into the
binary — cancelling the masking with the very line meant to carry it. Even a plain
function is a pure function of two known arrays, and `cargo build --release` runs
LTO, so the optimiser could fold it too. `persist::key` is therefore an ordinary
`fn`, marked `#[inline(never)]`, reading both constants through
`core::hint::black_box`.

*The key is 64 bytes, which is HMAC-SHA256's block size* — the one length RFC 2104
neither zero-pads nor pre-hashes. That choice also removes a `Result` from the save
path: `KeyInit::new` takes exactly a block-sized key and cannot fail, while
`new_from_slice` takes any length and returns an error that cannot happen. The
correct cryptographic length and the total signature are the same length.

### Checking the key is really hidden

**This is a procedure, not a test, and the repository says so rather than faking
one.** A unit test runs inside the process and cannot inspect its own `.rodata`; and
`cargo test` builds a *test* binary, not the release one a player receives. What the
tests do assert is that the stored constant is not the key, and a known-answer vector
pinning the key against accidental change.

```sh
cargo build --release
python3 - <<'PY'
key = bytes.fromhex("…the 64 bytes…")          # from crates/skylode-tui/src/persist/key.rs
print(open("target/release/skylode","rb").read().count(key))   # must print 0
PY
```

Two things that look like they should work and do not:

- **`strings` is the wrong tool.** A binary key contains no printable run, so
  `strings` would miss it even when it is there in full. The search has to be over raw
  bytes.
- **`grep -f keyfile` is the wrong tool too.** `grep` treats each *line* of the
  pattern file as a separate pattern, so a key containing a `0x0a` byte — this one
  does, at offset 51 — is silently split into two shorter patterns and the check
  reports a false pass.

And one caveat that decides *when* the check means anything: while `persist` had no
caller, the module was dead-code-eliminated from **both** profiles, so the constants
were absent because the code was absent — a pass that proved nothing.

**Run on 2026-08-04, once the session machine called `persist::save`**, which is the
first time it meant anything. The result, on a 1.7 MB release binary:

| Searched for | Occurrences | What it says |
| --- | --- | --- |
| the reassembled key | **0** | `black_box` and `#[inline(never)]` held against LTO |
| `MASKED` | 1 | the reassembly is *in* the binary — the check is not passing by absence |
| `MASK` | 1 | likewise |

The middle two rows are the point. A count of zero on all three would mean the
optimiser had removed the code, and the check would be measuring nothing. And
`key[51] == 0x0a` is confirmed, which is why `grep -f` splits the pattern there and
reports a false pass.

**Re-run on 2026-08-04 after the mask was moved to a derivation**, which added a
fourth row and is the one that now matters most:

| Searched for | Occurrences | What it says |
| --- | --- | --- |
| the reassembled key | **0** | unchanged: the key itself is still absent |
| `MASKED` | 1 | the one constant left, and the check still is not passing by absence |
| `SEED` | 1 | likewise |
| **the derived mask** | **0** | LTO did *not* fold the two SHA-256 rounds |

The last row is the whole claim. If the optimiser had evaluated the hash at compile
time it would have written the mask into `.rodata` beside `MASKED`, restoring the
adjacent pair the derivation exists to remove — and the window search would work again.
It does not: re-run against the hardened binary, that search reports *not found* after
scanning all 1.7 MB.

**One consequence worth writing down, found while walking the state machine:** a save
*from the future* cannot be forged from outside the binary. Raising `version` inside
the payload invalidates the MAC, so the loader answers `Tampered` and not
`FromTheFuture` — that path is reachable only by something holding the key, which is
why the test fixture for it lives inside `persist` itself.

**Build-time injection was considered and rejected.** Reading the key from an
environment variable at compile time (`env!`) keeps it out of the repository, but
not out of the binary — and the binary is what the player receives. Rust's own
guidance is to use `env!` for a secret **only when the binary is not
distributed**, which is the opposite of this game; and the repository is meant to
go public, so the reassembly method is readable either way. It would also split
the key between debug and release builds, so a run played during development would
not load in the shipped game, and a forgotten variable would fail a release build.
The cost is daily and the gain is near zero.

**The second layer is [validation](#a-load-validates-before-it-returns), and it is
the one that keeps paying.** A tamperer who extracts the key and re-signs the file
still has to produce a state that satisfies every rule the types enforce, which is
a far duller job than editing a number. Unlike the key, that layer also serves the
honest player: disk corruption fails it in exactly the same way. Deepening it with
[cross-field plausibility checks](#the-cross-field-audit) was therefore worth more than
any further work on hiding the key — and both were done on 2026-08-04, in that order of
importance.

### Robustness and recovery

- **Atomic writes:** write to a temp file **in the save's own directory**, then
  `rename`, so a crash mid-write cannot corrupt the save. The directory matters: a
  temp file elsewhere makes the last step a copy across filesystems rather than a
  rename, and a copy is divisible.
- **Backup:** keep the last known-good save as `.bak` (free thanks to atomic
  writes).
- **Schema versioning:** the `version` field enables safe migrations.
- **On integrity failure:** do not crash or punish. Inform the player the save
  looks modified or corrupted, and offer to restore the `.bak` or start a new
  game. Treat it first as corruption recovery, not anti-cheat enforcement.

#### What a write actually does, in order

1. `create_dir_all` — the one thing the loader *makes*, since a fresh install has no
   `~/.local/share/skylode` yet.
2. The sealed text into a temporary file in that same directory.
3. `sync_all`. Without it the operating system is free to let the rename reach the
   disk before the bytes do, and a power cut would leave a correctly-named empty file.
   It costs one flush of a few kilobytes, twice a minute. **The directory itself is
   not fsynced** — that needs a Unix-only `File::open(dir).sync_all()` and is a no-op
   or an error elsewhere — so the residual window is a power cut between the rename
   and the directory entry reaching the platter. Documented rather than closed.
4. Two renames: the old primary becomes the `.bak`, then the temporary becomes the
   primary. Each is indivisible, so **the primary is never a half-written file** — it
   is only ever the target of a rename.

**Atomicity is asserted by construction, not by a test.** No unit test can prove a
negative about the *timing* of a crash. Three tests stand in for it: a save leaves the
directory holding exactly the expected files and no leftover temporary; a file
truncated by hand is refused rather than half-read; and a write that fails leaves the
previous save intact and loadable.

**The rotation is blind.** The primary is not re-read and re-verified before it
becomes the `.bak`. Checking would cost a read plus an HMAC on every autosave to
defend against a player editing their save while the game is running — which that same
player defeats by editing it while the game is closed.

**A crash between the two renames leaves a good `.bak` and no primary.** This is the
one window the design keeps, and it has a consequence for the session state machine:
*a missing primary is only a fresh install when the backup is missing too*. See
[UI.md](UI.md) §8.3, whose `no save` edge is worded for it.

#### What the loader refuses, and what the `.bak` answers

`persist` loads **one** slot and reports; it never falls back to the backup on its
own, never deletes a file and never repairs a value. Restoring the backup is a
keypress in [UI.md](UI.md) §8.3, so the choice belongs to the player.

| What is wrong | Answer | Does the `.bak` help? |
| --- | --- | --- |
| no file | not a failure — a fresh install | — |
| the bytes cannot be reached (permissions, a directory that is a file) | `Io` | **No** — both files share a directory |
| not an envelope: not JSON, not UTF-8, truncated, bad `mac` encoding | `Damaged` | Yes |
| envelope intact, signature does not match | `Tampered` | Yes — the main case |
| written by a newer build | `FromTheFuture` | **No** — the backup is from the future too |
| signed, and still a run the rules could not produce | `Rejected` | Yes |

Two causes share an answer exactly when they share a **screen**. That is why every way
an envelope can be malformed is one `Damaged` — unparseable, truncated and badly
encoded are three diagnoses of one sentence — and why `FromTheFuture` is lifted out of
the core's own error rather than left inside `Rejected`: it is the only refusal whose
answer is *"update the game"* rather than *"restore the backup"*.

**Five refusals, four troubles, three frames.** The session machine groups them once
more on the way to the screen: `Damaged`, `Tampered` and `Rejected` are one frame with
the backup offered; `Io` and *"the backup failed too"* share a frame with nothing left
to offer, differing only in their first sentence; and `FromTheFuture` has a frame of
its own. `Io` had to be given that sentence rather than reusing the checksum one — a
player with a permission problem must not be told their file was edited.

**"Start a new game" costs the backup, and the frames now say so.** They used to
promise *"the current save is kept"*, which is true only until the new run's first
write: that write rotates the broken file into the backup slot, and the good backup
goes with it. A sentence that stops being true after ten seconds of play is worse than
no sentence, so the row reads *"the backup goes with it"*.

**A save from the future is not offered a new game at all.** Every other refusal here
is a broken file; that one is a *good* file this build is too old to read, so starting
over would let the older build write over a run the player made with a newer one. Its
frame offers `Quit` and says to update — see [UI.md](UI.md) §8.3.

## Tech stack

- Language: Rust.
- TUI: `ratatui` and `crossterm` (event loop and rendering).
- Serialization: `serde` and `serde_json` (save file).
- Time: `std::time::SystemTime`.
- RNG: `rand` plus `rand_chacha`, a seeded PRNG whose state lives in the save, for
  deterministic ticks. Specifically `ChaCha8Rng`, **not** `StdRng`: `rand` does not
  guarantee `StdRng`'s algorithm across releases, and an algorithm that changes
  under a save that stores a position in its sequence turns every existing run into
  a different one. `rand_chacha` guarantees reproducibility; that guarantee is the
  whole reason it is here. Both crates are taken with `default-features = false`,
  which strips OS entropy out of the core entirely.
- Atomic file write: `tempfile` (temp plus persist/rename).
- Integrity: `sha2` and `hmac`.
- Save location: `directories` (`ProjectDirs`), so the one file lands where each
  platform keeps application data rather than in a dot-directory under `$HOME`.
  See [Where the file lives](#where-the-file-lives).
- Distribution: a single static binary (`cargo build --release`), cross-platform
  in the terminal.

## Architecture

Game rules live in a `core` crate (`skylode-core`), decoupled from the TUI
(`skylode-tui`). This keeps the rules testable (deterministic ticks, `#[test]`)
and leaves the door open for other front-ends later. The core owns the game state,
including the mine grid; the TUI only renders it and forwards input.

### Tick loop

The core advances on a fixed timestep of 20 ticks per second (see
[MECHANICS.md](MECHANICS.md#ticks)). One `tick(input)` call applies the held-Space
mining, the auto-miner, timers (boosts, cube regeneration), XP accrual, and enchant
procs, all from the seeded PRNG so a run is reproducible. Rendering is decoupled:
the TUI redraws on change at roughly 30 fps, reading the core state without
driving it.

The swing inside one tick resolves in a fixed order, and **that order is stated in the
code, on `GameState::tick`** — it is an execution contract rather than a design
intention, so it belongs where a reader can check it against the function that keeps it.
What matters here is the step that is easy to get wrong: the batch reset is **last**. It
cannot fire on the break that empties the grid, because a blast can empty it too, and
the enchants that have not rolled yet would be handed a fresh full grid to blast on the
balance sheet of one swing.

`tick` **returns what happened** (`Vec<GameEvent>`) rather than only mutating. A
front-end that had to diff the state between frames to notice an Excavator proc
would be guessing: it misses two procs landing in the same tick, and it cannot tell
a `+1 Compressed Iron` earned from one the player minted by hand. Six mechanics owe
the player an announcement, one buffer feeds both the toast and the Stats history,
and only the inside of the tick can fill it. Events carry data and never
presentation — no colours, no durations, no instants — so the toast's window and the
proc flash's decay stay on the front-end's side of the determinism boundary.

Offline time is **not** replayed tick by tick. The MVP auto-miner is a flat passive
rate (see [MECHANICS.md](MECHANICS.md#auto-miner)), so what it produces over an
absence is a multiplication, not a simulation — and stepping 432 000 ticks to apply
one is work no player waits for and no test needs. Credit it in closed form on
launch, from the capped elapsed time (see
[offline accrual](MECHANICS.md#offline-accrual)). The tick loop drives the
*interactive* session only.

### Keyboard input

`tick(input)` takes an `Input` carrying `space_held: bool` — a struct rather than a
bare bool because the tick's inputs are a set that grows, and each one added to a
positional signature is a call site that keeps compiling with its arguments swapped.
Producing that bool is the TUI's job, and
it is harder than it looks, because **a terminal sends nothing when a key is
released**. The legacy encoding is "one key = its character", inherited from
teletypes where a key *was* a character and a character has no duration. The
release is not lost in transit: it is never encoded. A tty only knows "data stream
in, data stream out". So *hold Space* — the interaction
[0044](decisions/0044-mining-interaction-active-continuous-hold-space.md) settles on — is
not expressible by default.

Two layers, and the second is the one that runs on most machines:

**Layer 1 — exact.** Call `crossterm::terminal::supports_keyboard_enhancement()`
at startup (note: in `terminal`, not `event`; it round-trips a query to the
terminal, so it must run before the event loop). If supported, push the kitty
keyboard protocol flags and read real `Press` / `Release`; pop them on exit. The
flags must be **both**:

```rust
KeyboardEnhancementFlags::REPORT_EVENT_TYPES
    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
```

`REPORT_EVENT_TYPES` alone is silently useless here. The protocol sends
text-producing keys as raw UTF-8, and Space produces text — so it arrives as `0x20`
with no event-type field at all. The second flag is what forces Space through the
`CSI 32 ; 1 : 3 u` path where a release can exist. Windows needs neither: crossterm
reads the Console API there, which carries a key-down flag natively.

**Layer 2 — the window.** Everywhere else, including every VTE terminal (Ptyxis,
gnome-terminal, Console, Tilix), only OS auto-repeat is observable, and an
auto-repeated `0x20` is byte-identical to a fresh press:

```text
space_held = (now - last_space_event) < HOLD_WINDOW    // HOLD_WINDOW = 1100 ms
```

That is the whole mechanism: one subtraction, one comparison. No measurement, no
calibration, no persisted state — see
[0150](decisions/0150-measuring-the-auto-repeat-delay-to-calibrate-hold.md),
[0151](decisions/0151-querying-the-os-for-the-auto-repeat-delay.md) and
[0152](decisions/0152-auto-detecting-a-player-whose-auto-repeat-is-disabled.md) for why each
of those was tried and rejected. The 1100 is not a preference: the window must exceed
the largest initial auto-repeat delay a user setting can produce (Windows caps at
1000 ms), or mining cuts out during the gap and resumes, hitching on every hold.
Since the initial delay and the repeat interval differ, no single timeout avoids
both false positives and false negatives, so the design picks: up to 1.1 s of
over-mining after release, which is invisible against a 7-day offline cap.

The accessibility toggle is the same mechanism with two constants — a 15 000 ms
window extended by any key, plus Space cutting it explicitly.

**This does not weaken the core's determinism.** The contract is `tick(input)`: the
core is *given* `space_held`, it never infers it, so "same inputs, same outputs"
holds. What is not reproducible is the *session* — the same physical gesture can
produce different tick sequences on two machines. That is already true of any human
input, and is called out here only because determinism is load-bearing elsewhere in
this document.

### Core modules

The core is split by concern, each unit testable in isolation. **Names are the code's**
— singular, and with progression folded into `player`. This list sketched plural modules
(`worlds`, `pickaxes`) plus a separate `progression` until 2026-08-16; where a sketch and
a module disagree, the module wins, and the sketch is the thing to fix.

- `world`, `material`, `block`: the static data (which ores, their world, hardness, and
  minimum pickaxe tier), plus the **per-dimension enchant ceiling** the five special
  enchants and Fortune share (`World::enchant_cap`) — one number per world, and a
  rule of the world rather than of any enchant.
- `pickaxe`: tiers, Efficiency, Fortune, enchant levels, and `mining_power`. Owns
  **Efficiency's** ceiling (`PickaxeTier::efficiency_cap`), the one keyed by the tier.
- `mine`, `mine_kind`: the grid model, mixed content, break progress, batch reset, size
  and richness; and which of the twelve canonical mines a grid is, with its block pool.
- `player`: mining XP and level, world unlocks, prestige rank, and the two-axis gating.
  There is no `progression` module — it would have held one struct's fields.
- `enchant`: the seven enchants and their effects, the blast shapes and proc curves,
  plus `max_level` — the dispatch that picks whichever ceiling applies.
- `inventory`: what the player holds, and manual compression.
- `economy`: costs (composite Compressed plus raw) and the purchases that spend them.
- `boost`, `reward`, `upgrade`: the timed multiplier, the level-up bundles filed against
  a level, and the upgrade tracks.
- `prestige`: the arithmetic **only** — the multiplier and the carry. The reset itself is
  `GameState::prestige`, because it clears nine of that struct's fields.
- `game`: `GameState` and `tick`, the run in progress. It states the swing's order.
- `rng`: the seeded source of every draw, and the only module naming `rand`.
- `tunables`: the balance dials keyed by nothing.
- `error`: `CoreError` — what the rules refuse.
- `save`: serialization and migration **only**. The HMAC, the atomic write, the
  `.bak` recovery and the clock reading live outside the core, or it would stop
  being the pure, I/O-free library the rest of this section describes.

The TUI (`skylode-tui`) holds the six screens (Mine, Mines, Inventory, Upgrades, Stats,
Levels) and the overlays, reads core state to render, and forwards keyboard input.
