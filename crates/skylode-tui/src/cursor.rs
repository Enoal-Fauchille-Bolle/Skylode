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
/// reuse the one clamping rule instead of doing index arithmetic over a pair.
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
    /// no ladder holds (`Wooden` Efficiency 12). An index is clamped into the ladder
    /// on every step, which is the guarantee the typed cursors get for free.
    pub pickaxe_rung: usize,
    /// Which of the six enchant tracks the Enchants sub-tab points at.
    pub enchant: EnchantType,
    /// Which mine and which of its two paid tracks the Mines sub-tab points at.
    pub mine_track: (MineKind, MineTrack),
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
    pub fn new(mine: MineKind, pickaxe_rung: usize) -> Self {
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
        }
    }
}

/// Steps an **index** by `delta`, clamped into a list of `len` rows.
///
/// The arithmetic half of [`step_in`], split out for the one cursor that has no value
/// to point at ([`Cursors::pickaxe_rung`]). Sharing it is what keeps *lists clamp*
/// from being right for the typed cursors and wrong for the index one — the failure
/// mode being a `↑` on the first rung that wrapped to a maxed Netherite pickaxe.
///
/// A `len` of zero answers `0`: there is no row to land on, and the callers all slice
/// with this afterwards.
pub fn step_index(len: usize, current: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    // Signed throughout, then clamped back: `current - 1` on a `usize` at row zero
    // would wrap to `usize::MAX`.
    let next = (current as isize + delta).clamp(0, len as isize - 1);
    usize::try_from(next).unwrap_or(0)
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
    let next = step_index(list.len(), index, delta);
    list.get(next).copied().unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_opens_pointing_at_the_mine_it_is_standing_in() {
        assert_eq!(Cursors::new(MineKind::Obsidian, 0).mine, MineKind::Obsidian);
    }

    /// The other cursor has nothing in the run to be seeded from, so it opens on the
    /// table's first row — and *on the table's* first row, read from the core, not on
    /// a variant named here.
    #[test]
    fn the_material_cursor_opens_on_the_first_row_of_the_table() {
        assert_eq!(Cursors::new(MineKind::Stone, 0).material, Material::ALL[0]);
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

    /// The index cursor's own clamp, and the guard under it. A list with no rows is
    /// unreachable through the ladder — it is forty-six long — but the helper is
    /// generic over a length, and answering `0` is what keeps a caller slicing with
    /// the result from panicking.
    #[test]
    fn stepping_an_index_clamps_and_survives_an_empty_list() {
        assert_eq!(step_index(46, 0, -1), 0, "the first row wrapped");
        assert_eq!(step_index(46, 45, 1), 45, "the last row wrapped");
        assert_eq!(step_index(46, 20, 1), 21);
        assert_eq!(step_index(0, 5, 1), 0);
    }

    /// The three sub-tabs are a **ring**, unlike every list on the screens they belong
    /// to — the same distinction [`step_in`]'s own doc draws for the tab bar.
    #[test]
    fn the_sub_tabs_wrap_where_the_lists_clamp() {
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
