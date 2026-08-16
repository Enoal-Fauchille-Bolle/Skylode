# Adding a screen or an overlay

A seventh tab, or a new modal. The front-end has one organising rule and everything
here follows from it: **raw input becomes a semantic action exactly once**, in `keymap`,
so `App::update` is a pure function of `(state, Action)` and the whole app is testable
without a terminal.

## The chain

```
crossterm KeyEvent
  → keymap::resolve(&App, key) -> Option<Action>      decode, once
  → App::update(&mut self, action: Action)            the reducer
  → View::from_state(state, config, cursors, …)       the read model
  → Screen::render(self, frame, area, &view)          draw
```

Each arrow is a narrowing, and each is enforced by the compiler. `Screen::render` is
handed a `&View` **and nothing else** — no `GameState`, no `Config` — so anything a
screen needs must first become a field of `View`.

## Adding a tab

1. **`screen/mod.rs`** — add the variant to `enum Screen` and extend `pub const ALL:
   [Self; 6]`. That array *is* the tab order and the `1`..`6` mapping, so all three move
   together and its length is in the type.
2. **A module** under `screen/`, with a `render` arm.
3. **`view.rs`** — add whatever the screen reads to `View`, and fill it in
   `from_state`. There is **no `..Self::sample()`** left in this codebase: the compiler
   is exhaustive over `View` again, so a field added there breaks `from_state` until
   someone decides where it comes from. That is deliberate — do not reintroduce a
   fallback.
4. **`cursor.rs`** — if the screen has a list. Every list cursor lives in `Cursors`,
   which is what lets `Esc` be cheap: leaving a screen loses nothing.
5. **`action.rs`** — only if the screen needs a *new* gesture.

**Reuse the list gestures.** `Action::CursorUp`, not `MinesCursorUp`. This is forced
rather than chosen: `keymap` decodes a key with no access to the run, so it *cannot*
produce a `SelectMine(kind)` — it does not know where the cursor is. Which screen a
gesture lands on is resolved in `update`, where the state is. The payoff is that
Inventory, Upgrades and Levels share five actions instead of adding five apiece.

## Adding a modal

1. **`overlay/mod.rs`** — a variant on `enum Modal`, and a module under `overlay/`.
2. **Put the modal's own state *in the variant*.** `Compress { material, direction,
   units }`, `Settings { row }`. Two facts that only mean anything together are stored
   together, so *"a count of 12 with no dialog open"* is a state nobody can write down.
   Keep the payload `Copy` so `Modal` stays `Copy`.
3. **`keymap`'s rule 2 gives a modal first refusal on every key.** You get `Esc` and the
   menu gestures for free; that layering is also why the first `Esc` closes the box and
   only the second is decoded to `Action::ToMine`.
4. An overlay is **not** a `Screen`, so it is not handed a `&View`. It takes what it
   needs as parameters — including preferences like `number_format`.

## The four things that bite

**A preference is copied into the `View`, not read from `Config` by the screen.**
`colour_mode`, `number_format` and `sub_tab_keys` are `View` fields. And
`format::grouped(n, format)` takes the format **explicitly, with no default** — that
signature is what made the compiler enumerate every call site when the thousands
separator landed. A default would have let the forgotten ones print spaces at a player
who asked for commas.

**If your state pauses the tick, clear the flash.** The proc flash's beat is resolved in
`sync_view`, not in `render` — unlike a toast, whose expiry is asked at draw time. A
state that stops the tick freezes a live flash mid-beat. (`docs/UI.md` §7.1.)

**Do not touch the mine key's release branch.** `keymap` carries a release branch
*above* even the modal capture, and it has to: on a terminal that reports key releases
(the kitty protocol), every binding fires twice without it. It looks like dead weight
and is not.

**`q` is not quit.** It returns to the title; only `Ctrl-C` ends the process
([0115](../decisions/0115-q-in-a-game-returns-to-the-title-only-ctrl-c-ends-the.md)). A
new modal that swallows `q` must say what it does with it — Settings does, deliberately.

## Testing it

`App::run` is generic over ratatui's `Backend` and over the `event::Events` trait, so a
test drives the **real loop** with a scripted event list against `TestBackend`. There is
no reason for a new screen to be untested: everything the loop does is reachable without
a tty.

The two lines that genuinely cannot be covered are `main` and the crossterm polling
thread in `EventHandler::new` — both need a real terminal, `event::poll` fails outside
one and takes the thread with it. They are left in the coverage denominator rather than
excluded.

## Verify

```sh
cargo test -p skylode-tui
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p skylode-tui                  # actually look at it
```

Check it at **80×24**, the reference terminal. The grid is fixed and the chrome flexes
([0084](../decisions/0084-reference-terminal-80-24-minimum-adapting-upward-the.md)); a
screen that only fits your window is a screen that does not fit.
