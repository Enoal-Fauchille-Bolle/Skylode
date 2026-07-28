//! Where the player is *pointing*, as opposed to where they are.
//!
//! A cursor is front-end state and nothing else: which row of a list is
//! highlighted, which sub-tab is showing. None of it belongs in `skylode-core` —
//! a list selection has no business reaching a save file, and the rules must stay
//! answerable without a screen.
//!
//! **A module of its own, rather than a field on [`App`](crate::app::App)**, and the
//! reason is a dependency and not tidiness. Both `app` and [`view`](crate::view)
//! need this type: `app` owns it and moves it, `view` reads it to project the run
//! into what the screens draw. Declaring it in `app` would make `view` import the
//! application — legal in Rust, since modules of one crate may refer to each other
//! in a cycle, but it points the dependency backwards: the read model would then
//! know about the loop that drives it.
//!
//! Phases 6 and 7 add their own fields here (the Upgrades sub-tab and its row)
//! rather than growing [`View::from_state`](crate::view::View::from_state) one
//! parameter per phase.

use skylode_core::{material::Material, mine_kind::MineKind};

/// Every list cursor the front-end owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursors {
    /// Which of the twelve mines the Mines screen highlights and describes.
    ///
    /// A [`MineKind`] and not an index into a row list, so it cannot point at a row
    /// that no longer exists — the twelve are a fixed set, and the screen's own
    /// grouping into worlds is a rendering detail this must not depend on.
    ///
    /// **Distinct from the mine the player is standing in**, which is the run's and
    /// lives in `GameState`. The two start equal and part company the moment `↑` is
    /// pressed; the list marks them `▸` and `●` precisely so the difference is
    /// visible.
    pub mine: MineKind,
    /// Which of the fifteen materials the Inventory table highlights, and whose
    /// holdings the Compress panel details.
    ///
    /// A [`Material`] and not an index, for the reason [`mine`](Cursors::mine) is a
    /// [`MineKind`]: the set is closed and fixed, so a typed cursor cannot point at a
    /// row that is not there. It is also what the compression dialog is opened
    /// *about* — `c` converts the material under this cursor, so a stale index would
    /// convert the wrong pile rather than merely drawing the wrong `▸`.
    pub material: Material,
}

impl Cursors {
    /// The cursors a session opens with, given the mine the run is standing in.
    ///
    /// **Only the mine is seeded from the run, and the asymmetry is the run's, not a
    /// shortcut.** A `GameState` answers *"which mine am I standing in"* — so
    /// defaulting that one to [`MineKind::Stone`] would open the Mines screen
    /// somewhere the player is not. Nothing answers *"which material am I looking
    /// at"*: a material is not a place, and there is no held one to read. So the
    /// Inventory list opens at its first row, which is the honest answer rather than
    /// an invented one.
    ///
    /// There is no [`Default`] for the same reason as before: it would have to invent
    /// the mine, which the caller already holds.
    pub fn new(mine: MineKind) -> Self {
        Self {
            mine,
            // `Material::ALL`'s first entry rather than `Material::Stone` spelled
            // out: the table's top row is whatever the core lists first, and reading
            // it from the table is what keeps the cursor on a row that exists if the
            // order ever changes.
            material: Material::ALL[0],
        }
    }
}

/// Steps `current` by `delta` along `list`, **stopping at the ends rather than
/// wrapping**.
///
/// **Lists clamp, rings wrap**, and the distinction is a design rule rather than a
/// preference. The tab ring wraps because it is six equivalent destinations; these
/// lists are progression orders — twelve mines under three world headers, fifteen
/// materials grouped by world — so a `↑` on the first row that landed on the last
/// would be a jump across the whole game.
///
/// Generic over the element because both cursors are **typed values and not
/// indices**, and the arithmetic is identical for each: find where the value sits,
/// move, clamp back into the list. Written once so the clamp cannot be right on one
/// list and wrong on the other. Rust *monomorphises* this — it emits one specialised
/// copy per element type at compile time — so sharing it costs nothing at runtime.
///
/// Two totality guards, both unreachable through the two real callers and both here
/// because this crate's lints leave no `unwrap` to spend on "cannot happen":
/// an empty `list` has nowhere to step, and a `current` the list does not hold reads
/// as position zero rather than refusing.
pub fn step_in<T: Copy + PartialEq>(list: &[T], current: T, delta: isize) -> T {
    if list.is_empty() {
        return current;
    }
    let index = list.iter().position(|&item| item == current).unwrap_or(0);
    // Signed arithmetic, then clamped back: `index - 1` on a `usize` at row zero
    // would wrap to `usize::MAX` and index far past the end.
    let next = (index as isize + delta).clamp(0, list.len() as isize - 1);
    list.get(next as usize).copied().unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_opens_pointing_at_the_mine_it_is_standing_in() {
        assert_eq!(Cursors::new(MineKind::Obsidian).mine, MineKind::Obsidian);
    }

    /// The other cursor has nothing in the run to be seeded from, so it opens on the
    /// table's first row — and *on the table's* first row, read from the core, not on
    /// a variant named here.
    #[test]
    fn the_material_cursor_opens_on_the_first_row_of_the_table() {
        assert_eq!(Cursors::new(MineKind::Stone).material, Material::ALL[0]);
    }

    #[test]
    fn a_step_moves_one_row_in_either_direction() {
        assert_eq!(step_in(&Material::ALL, Material::Iron, 1), Material::Gold);
        assert_eq!(step_in(&Material::ALL, Material::Iron, -1), Material::Coal);
    }

    /// The rule the tab ring does *not* follow: a list stops at its ends. Asserted on
    /// both cursors, since the shared helper is only worth having if both get it.
    #[test]
    fn a_list_clamps_at_both_ends_rather_than_wrapping() {
        let materials = &Material::ALL;
        assert_eq!(step_in(materials, Material::Stone, -1), Material::Stone);
        assert_eq!(
            step_in(materials, Material::Amethyst, 1),
            Material::Amethyst
        );

        let mines = &MineKind::ALL;
        assert_eq!(step_in(mines, MineKind::Stone, -1), MineKind::Stone);
        assert_eq!(step_in(mines, MineKind::Amethyst, 1), MineKind::Amethyst);
    }

    /// The two totality guards. Neither is reachable through a real cursor — both
    /// lists are non-empty constants and both cursor types are closed sets — but the
    /// helper is generic and says so rather than panicking a keypress.
    #[test]
    fn stepping_is_total_on_an_empty_list_and_an_unlisted_value() {
        assert_eq!(step_in(&[], MineKind::Iron, 1), MineKind::Iron);
        // A value the list does not hold reads as position zero, so a step forward
        // lands on the second row rather than refusing.
        assert_eq!(
            step_in(&[MineKind::Stone, MineKind::Coal], MineKind::Iron, 1),
            MineKind::Coal
        );
    }
}
