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
//! Phase 6 took that up: the Upgrades sub-tab and one row cursor per sub-tab live
//! here, and [`UpgradeTab`] moved here from [`view`](crate::view) with them. *Which
//! sub-tab is showing* is the module header's own example of what a cursor is, and
//! leaving the type in the read model would have this module import the thing it
//! feeds.

use skylode_core::{economy, enchant::EnchantType, material::Material, mine_kind::MineKind};

/// Which sub-tab of the Upgrades screen is showing (UI.md §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeTab {
    /// The pickaxe ladder — a single linear roadmap, no rung skippable.
    Pickaxe,
    /// The six enchant tracks, each at its frontier.
    Enchants,
    /// The twelve mines' size and richness tracks.
    Mines,
}

impl UpgradeTab {
    /// The three, in the order the sub-tab bar prints them.
    pub const ALL: [Self; 3] = [Self::Pickaxe, Self::Enchants, Self::Mines];

    /// The next sub-tab, **wrapping** past the last back to the first.
    ///
    /// Wrapping, where every list on these screens clamps — and the distinction is
    /// the same one [`step_in`] is built around. The three sub-tabs are equivalent
    /// destinations, exactly like the six-screen ring: there is no progression order
    /// between the pickaxe and the mines, so passing the end of them is not a jump
    /// across anything.
    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|&tab| tab == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// The previous sub-tab, wrapping the other way.
    pub fn prev(self) -> Self {
        let index = Self::ALL.iter().position(|&tab| tab == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Which of a mine's two paid tracks a row of the Mines sub-tab is about.
///
/// **Front-end, and deliberately so.** The core has no such type: it has
/// [`GameState::buy_mine_size`](skylode_core::game::GameState) and
/// `buy_mine_richness`, two methods, and nothing that needs to *name* the choice
/// between them. What needs a name is a **row** — the sub-tab lists each mine twice
/// (`docs/UI.md` §5.4.2) — and a row identity belongs to whatever draws rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineTrack {
    /// The grid's size level.
    Size,
    /// The richness *ceiling* — never the free dial, which lives on the Mines screen.
    Richness,
}

impl MineTrack {
    /// Both tracks, in the order each mine's two rows are drawn.
    pub const ALL: [Self; 2] = [Self::Size, Self::Richness];
}

/// The twenty-four rows of the Mines sub-tab: every mine, both of its tracks.
///
/// Built rather than stored, because it is a *product* of two closed sets and there
/// is nothing per-run in it. Handing it to [`step_in`] is what lets the Mines sub-tab
/// reuse the one stepping rule instead of doing index arithmetic over a pair.
pub fn mine_tracks() -> Vec<(MineKind, MineTrack)> {
    MineKind::ALL
        .into_iter()
        .flat_map(|kind| MineTrack::ALL.map(|track| (kind, track)))
        .collect()
}

/// The six rows of the Enchants sub-tab: every enchant the shop actually sells.
///
/// **Filtered on the price, not on the variant.** `Efficiency` is absent because it
/// is a pickaxe upgrade priced on the ladder, and
/// [`economy::enchant_cost`] already says so by answering [`None`] for it — so this
/// reads that rule rather than restating it. The `World` passed decides which
/// *material* a price is quoted in and never whether there is a price at all, which
/// is why any of the three does here.
pub fn enchant_tracks() -> Vec<EnchantType> {
    EnchantType::ALL
        .into_iter()
        .filter(|&kind| economy::enchant_cost(kind, 0).is_some())
        .collect()
}

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
    /// Which of the three Upgrades sub-tabs is showing.
    pub upgrade_tab: UpgradeTab,
    /// Which rung of the pickaxe roadmap the Pickaxe sub-tab points at.
    ///
    /// **The one index-shaped cursor in the crate, and it is not an inconsistency.**
    /// The other three point at values of closed sets — a [`MineKind`], a
    /// [`Material`], an [`EnchantType`] — so they cannot name a row that is not
    /// there. The ladder is a *generated* list
    /// ([`upgrade::ladder`](skylode_core::upgrade::ladder)) whose rungs are
    /// `(tier, efficiency)` pairs the core does not hand out as a type, so there is no
    /// value to point at; and the pair itself would be worse, since it can name a rung
    /// no ladder holds (`Wooden` Efficiency 12). An index is wrapped back into the
    /// ladder on every step, which is the guarantee the typed cursors get for free.
    pub pickaxe_rung: usize,
    /// Which of the six enchant tracks the Enchants sub-tab points at.
    pub enchant: EnchantType,
    /// Which mine and which of its two paid tracks the Mines sub-tab points at.
    pub mine_track: (MineKind, MineTrack),
    /// Which rung of the Levels roadmap is selected, as a **level** and not an index.
    ///
    /// The ladder is `1..=LEVEL_CAP` — a closed, contiguous set — so the value and the
    /// index differ only by one, and the value is the one that matters: a claim is
    /// addressed by level. This is what keeps [`pickaxe_rung`](Cursors::pickaxe_rung)
    /// the crate's *only* index cursor, which is what its own note claims.
    ///
    /// Seeded from the run, like [`mine`](Cursors::mine) and unlike
    /// [`material`](Cursors::material), by the same test: the run answers *"what level
    /// am I"*, so opening anywhere else would point the roadmap at a rung the player
    /// is not on. `Home` is what puts it back after scrolling.
    pub level: u32,
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
    ///
    /// **`pickaxe_rung` joined the seeded half, and `enchant` the invented one**, by
    /// the same test: does the run answer the question the cursor asks? *"Where am I
    /// on the ladder"* it answers — that is [`position`](skylode_core::upgrade::position)
    /// — so opening anywhere else would point at a rung the player is not on. *"Which
    /// enchant am I looking at"* it does not, so that list opens at its first row.
    ///
    /// Takes the rung already computed rather than a `&GameState`, keeping this
    /// module free of the aggregate: a cursor is told where the player is, it does not
    /// go and look.
    pub fn new(mine: MineKind, pickaxe_rung: usize, level: u32) -> Self {
        Self {
            mine,
            // `Material::ALL`'s first entry rather than `Material::Stone` spelled
            // out: the table's top row is whatever the core lists first, and reading
            // it from the table is what keeps the cursor on a row that exists if the
            // order ever changes.
            material: Material::ALL[0],
            // The bar's first name, for the reason above.
            upgrade_tab: UpgradeTab::ALL[0],
            pickaxe_rung,
            // Never `None` in practice — Fortune is sold in every world — but read off
            // the list rather than named, exactly like `Material::ALL[0]` above, so the
            // opening row follows the list if the shop's order ever changes.
            enchant: enchant_tracks()
                .first()
                .copied()
                .unwrap_or(EnchantType::Fortune),
            // The Mines sub-tab opens on the mine the player is standing in, which is
            // the same fact `mine` above is seeded from — two lists about the twelve
            // mines should not disagree about where the player is.
            mine_track: (mine, MineTrack::ALL[0]),
            // The third seeded cursor, for `mine`'s reason: the run knows what level
            // the player is, so the roadmap opens on their own rung.
            level,
        }
    }
}

/// Steps an **index** by `delta`, **wrapping** around a list of `len` rows.
///
/// The arithmetic half of [`step_in`], split out for the one cursor that has no value
/// to point at ([`Cursors::pickaxe_rung`]). Sharing it is what keeps *every list
/// wraps* from being right for the typed cursors and wrong for the index one.
///
/// **[`isize::rem_euclid`] and not `%`.** Rust's remainder keeps the sign of the
/// dividend, so `-1 % 46` is `-1` and not `45` — a `↑` on the first row would produce
/// a negative index rather than the last one. `rem_euclid` is the mathematical modulo,
/// always non-negative for a positive divisor, and it is what lets one expression
/// serve both directions instead of a branch per sign.
///
/// It also absorbs a `current` past the end of the list: `50` in a list of `46` reads
/// as row `4` rather than panicking. Unreachable through the real callers, which step
/// through here every time, and cheaper to have than to argue about.
///
/// A `len` of zero answers `0`: there is no row to land on, the callers all slice with
/// this afterwards, and `rem_euclid` by zero would panic.
pub fn step_index(len: usize, current: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    // Signed throughout, then back: `current - 1` on a `usize` at row zero would wrap
    // to `usize::MAX`. `saturating_add` because `delta` is caller-supplied and an
    // overflow here would panic a keypress in a debug build.
    let next = (current as isize)
        .saturating_add(delta)
        .rem_euclid(len as isize);
    usize::try_from(next).unwrap_or(0)
}

/// Steps `current` by `delta` along `list`, **wrapping past either end**.
///
/// **Every list in the crate wraps, and so do the two rings.** That is one rule, not
/// the two this used to hold: the lists clamped, on the argument that a `↑` on the
/// first of twelve mines landing on the End mine would be a jump across the whole
/// game. The argument does not survive contact — a cursor only *highlights*, the
/// player sees where they land, and stopping dead at an end is a keypress that reports
/// nothing. Every purchase still goes through its own `Enter`, so nothing a wrap can
/// reach is anything a wrap can spend.
///
/// **What still stops at its ends is everything that is not a list**: the richness dial
/// (a cursor on a bought ceiling), the compression spinner (a quantity in `1..=pile`)
/// and the dip modal's caret (two options, where a held key must not put `Buy it` one
/// repeat away from `Not yet` — `docs/UI.md` §6.7). None of the three comes through
/// here; each owns its own arithmetic, which is what keeps this function's rule
/// unqualified.
///
/// Generic over the element because the cursors are **typed values and not indices**,
/// and the arithmetic is identical for each: find where the value sits, move, wrap back
/// into the list. Written once so the wrap cannot be right on one list and wrong on the
/// other. Rust *monomorphises* this — it emits one specialised copy per element type at
/// compile time — so sharing it costs nothing at runtime.
///
/// Two totality guards, both unreachable through the real callers and both here because
/// this crate's lints leave no `unwrap` to spend on "cannot happen": an empty `list` has
/// nowhere to step, and a `current` the list does not hold reads as position zero rather
/// than refusing.
pub fn step_in<T: Copy + PartialEq>(list: &[T], current: T, delta: isize) -> T {
    if list.is_empty() {
        return current;
    }
    let index = list.iter().position(|&item| item == current).unwrap_or(0);
    let next = step_index(list.len(), index, delta);
    list.get(next).copied().unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_opens_pointing_at_the_mine_it_is_standing_in() {
        assert_eq!(
            Cursors::new(MineKind::Obsidian, 0, 1).mine,
            MineKind::Obsidian
        );
    }

    /// The other cursor has nothing in the run to be seeded from, so it opens on the
    /// table's first row — and *on the table's* first row, read from the core, not on
    /// a variant named here.
    #[test]
    fn the_material_cursor_opens_on_the_first_row_of_the_table() {
        assert_eq!(
            Cursors::new(MineKind::Stone, 0, 1).material,
            Material::ALL[0]
        );
    }

    #[test]
    fn a_step_moves_one_row_in_either_direction() {
        assert_eq!(step_in(&Material::ALL, Material::Iron, 1), Material::Gold);
        assert_eq!(step_in(&Material::ALL, Material::Iron, -1), Material::Coal);
    }

    /// The rule the tab ring was once alone in following: passing an end comes out the
    /// other side. Asserted on both cursors, since the shared helper is only worth
    /// having if both get it.
    #[test]
    fn a_list_wraps_at_both_ends_rather_than_stopping() {
        let materials = &Material::ALL;
        assert_eq!(step_in(materials, Material::Stone, -1), Material::Amethyst);
        assert_eq!(step_in(materials, Material::Amethyst, 1), Material::Stone);

        let mines = &MineKind::ALL;
        assert_eq!(step_in(mines, MineKind::Stone, -1), MineKind::Amethyst);
        assert_eq!(step_in(mines, MineKind::Amethyst, 1), MineKind::Stone);
    }

    /// The index cursor's own wrap, and the two guards under it. A list with no rows is
    /// unreachable through the ladder — it is forty-six long — but the helper is generic
    /// over a length, and answering `0` is what keeps a caller slicing with the result
    /// from panicking where `rem_euclid` would divide by zero.
    #[test]
    fn stepping_an_index_wraps_and_survives_an_empty_list() {
        assert_eq!(step_index(46, 0, -1), 45, "the first row did not wrap");
        assert_eq!(step_index(46, 45, 1), 0, "the last row did not wrap");
        assert_eq!(step_index(46, 20, 1), 21);
        assert_eq!(step_index(0, 5, 1), 0);
        // A position past the end folds back in rather than panicking — the guard the
        // old clamp gave for free, kept deliberately.
        assert_eq!(step_index(46, 50, 0), 4);
    }

    /// The three sub-tabs are a **ring**, like every list on the screens they belong to
    /// — they were the exception, and [`step_in`]'s own doc no longer draws the
    /// distinction. What this still pins is that they come back where they started.
    #[test]
    fn the_sub_tabs_wrap_in_both_directions() {
        assert_eq!(UpgradeTab::Pickaxe.prev(), UpgradeTab::Mines);
        assert_eq!(UpgradeTab::Mines.next(), UpgradeTab::Pickaxe);
        for tab in UpgradeTab::ALL {
            assert_eq!(tab.next().prev(), tab, "{tab:?} did not come back");
        }
    }

    /// The Enchants sub-tab lists what the **shop sells**, which is every enchant but
    /// Efficiency — and it says so by reading the price rather than by naming the
    /// exception a second time.
    #[test]
    fn the_enchant_rows_are_the_six_the_shop_prices() {
        let tracks = enchant_tracks();
        assert_eq!(tracks.len(), 6);
        assert!(!tracks.contains(&EnchantType::Efficiency));
        assert_eq!(tracks.first(), Some(&EnchantType::Fortune));
    }

    /// Each mine contributes both of its rows, and they are adjacent — which is what
    /// makes `↓` walk a mine before moving to the next one (UI.md §5.4.2).
    #[test]
    fn the_mine_rows_are_every_mine_twice_in_a_row() {
        let rows = mine_tracks();
        assert_eq!(rows.len(), MineKind::ALL.len() * 2);
        assert_eq!(rows.first(), Some(&(MineKind::Stone, MineTrack::Size)));
        assert_eq!(rows.get(1), Some(&(MineKind::Stone, MineTrack::Richness)));
        assert_eq!(rows.get(2), Some(&(MineKind::Coal, MineTrack::Size)));
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
