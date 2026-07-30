//! The dev menu — nine rows that reach the doors `skylode_core::game::dev` opens.
//!
//! **Deliberately absent from `docs/UI.md`.** That document specifies the *game's*
//! interface, and this is not part of it: it ships in no release build, no wireframe
//! counts its columns, and a player never sees it. `docs/DEV-MENU.md` is where it is
//! written down instead.
//!
//! **What it does not contain is the interesting half.** There is no pickaxe row, no
//! enchant row and no mine-upgrade row, though the core has a free door for each —
//! because the [`free_upgrades`](DevState#structfield.free_upgrades) toggle makes the *Upgrades
//! screen itself* free, ladders, `✓` marks, dip modal and all. Rebuilding those three
//! ladders in a box would be a second, worse copy of the screen the toggle lets you
//! use for real, and a dev tool whose job is to exercise the interface must not route
//! around it. Every row that survived is one the Upgrades screen cannot do at all.
//!
//! The two ladders below are **value ladders, not free numbers**: this menu adjusts
//! with `←/→` like every other list in the crate rather than growing the crate's first
//! text field. A spinner cannot be wrong, and the amounts a dev actually wants are
//! powers of ten (`compression`'s note, applied to a row that has no ceiling to clamp
//! against).

use std::time::Duration;

use ratatui::{Frame, layout::Rect};
use skylode_core::{material::Material, tunables::LEVEL_CAP};

use crate::{
    cursor::{step_in, step_index},
    format::grouped,
};

/// The amounts the `Amount` row offers, in powers of ten.
const AMOUNTS: [u32; 7] = [1, 10, 100, 1_000, 10_000, 100_000, 1_000_000];

/// The absences the `Skip time` row offers, as `(seconds, label)`.
///
/// It stops at seven days because [`OFFLINE_CAP`] does: a longer skip would test the
/// cap's clamp and nothing else, and the cap has its own test in the core.
///
/// [`OFFLINE_CAP`]: skylode_core::tunables::OFFLINE_CAP
const SKIPS: [(u64, &str); 6] = [
    (60, "1 m"),
    (600, "10 m"),
    (3_600, "1 h"),
    (21_600, "6 h"),
    (86_400, "24 h"),
    (604_800, "7 d"),
];

/// The highest rank the prestige row dials to.
///
/// A dial bound and not a rule: [`GameState::dev_set_prestige`] takes any [`u32`], and
/// the rank is unbounded in the rules on purpose. Twenty is where the multiplier stops
/// being a thing anyone needs to *look* at.
///
/// [`GameState::dev_set_prestige`]: skylode_core::game::GameState::dev_set_prestige
const MAX_PRESTIGE_DIAL: u32 = 20;

/// The highest number of charges the boost row grants at once.
const MAX_CHARGE_DIAL: u32 = 99;

/// One line of the menu.
///
/// A closed enum walked through [`ROWS`], for the reason every other cursor in this
/// crate is a typed value: a row that is not there cannot be pointed at. It carries no
/// data — the values live in [`DevState`], one field each, because two rows read the
/// same `amount` and a per-row payload would need two copies of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevRow {
    /// Whether the Upgrades screen charges for anything.
    FreeUpgrades,
    /// The figure the two give rows and the experience row spend.
    Amount,
    /// Which pile `Enter` credits.
    Material,
    /// Credit every pile at once.
    Everything,
    /// Credit experience, filing the levels it crosses.
    Experience,
    /// Where to put the mining level.
    Level,
    /// Where to put the prestige rank.
    Prestige,
    /// How many boost charges to add.
    Charges,
    /// How much absence the next resume should credit.
    SkipTime,
}

/// The rows, in the order they are drawn.
pub const ROWS: [DevRow; 9] = [
    DevRow::FreeUpgrades,
    DevRow::Amount,
    DevRow::Material,
    DevRow::Everything,
    DevRow::Experience,
    DevRow::Level,
    DevRow::Prestige,
    DevRow::Charges,
    DevRow::SkipTime,
];

impl DevRow {
    /// The row's name, as drawn.
    fn label(self) -> &'static str {
        match self {
            Self::FreeUpgrades => "Free upgrades",
            Self::Amount => "Amount",
            Self::Material => "Give material",
            Self::Everything => "Give everything",
            Self::Experience => "Give experience",
            Self::Level => "Mining level",
            Self::Prestige => "Prestige rank",
            Self::Charges => "Boost charges",
            Self::SkipTime => "Skip time",
        }
    }

    /// Whether `←/→` has anything to move on this row.
    ///
    /// The two give-everything rows read [`DevRow::Amount`] rather than carrying a
    /// figure of their own, so their `◄ ►` arrows would be a control that does nothing
    /// — and an arrow that does nothing is worse than none, because it invites the
    /// press that proves it.
    fn adjustable(self) -> bool {
        !matches!(self, Self::Everything | Self::Experience)
    }
}

/// The menu's own state: which row, and the value each one is dialled to.
///
/// **It lives beside [`App::modal`](crate::app::App) rather than inside
/// [`Modal::Dev`](crate::overlay::Modal), which is the opposite of the choice the
/// compression dialog makes** — and the reason is the same rule read the other way.
/// That dialog's count belongs *to* one opening of one box, so storing it outside would
/// make "a count of 12 with no dialog open" writable. These values are the opposite:
/// dialling a million and then closing the box to look at the Inventory is the workflow,
/// and a state that reset on `Esc` would make the menu unusable for the thing it is for.
/// It also keeps [`Modal`](crate::overlay::Modal) [`Copy`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevState {
    /// The row the `▸` is on.
    pub row: DevRow,
    /// Whether the Upgrades screen buys through the free doors.
    pub free_upgrades: bool,
    /// Which pile the material row credits.
    pub material: Material,
    /// Index into [`AMOUNTS`].
    amount: usize,
    /// The level the level row would set.
    pub level: u32,
    /// The rank the prestige row would set.
    pub prestige: u32,
    /// How many charges the boost row would add.
    pub charges: u32,
    /// Index into [`SKIPS`].
    skip: usize,
}

impl Default for DevState {
    /// The menu as it first opens: nothing on, a thousand of the first pile.
    ///
    /// **`free_upgrades` opens `false`**, for the reason the dip modal's caret opens on
    /// `Not yet`: a mode that changes what every purchase costs must not be something a
    /// session acquires by being started.
    ///
    /// Hand-written rather than derived because three of the eight fields have no
    /// meaningful zero — a level of 0 is outside the ladder, an amount index of 0 is a
    /// menu that gives one ore at a time, and a charge count of 0 grants nothing.
    fn default() -> Self {
        Self {
            row: ROWS[0],
            free_upgrades: false,
            material: Material::ALL[0],
            amount: 3,
            level: 1,
            prestige: 0,
            charges: 1,
            skip: 2,
        }
    }
}

impl DevState {
    /// The figure the give rows spend.
    pub fn amount(&self) -> u32 {
        AMOUNTS.get(self.amount).copied().unwrap_or(1)
    }

    /// The absence the skip row would credit.
    pub fn skip(&self) -> Duration {
        Duration::from_secs(SKIPS.get(self.skip).map_or(0, |&(seconds, _)| seconds))
    }

    /// How that absence is written on the row.
    ///
    /// The label and not a formatter over the [`Duration`]: the ladder already names
    /// its own rungs, and the toast that reports a skip must say the words the row the
    /// player pressed said. A `Duration` printer would be a second vocabulary for six
    /// fixed values.
    pub fn skip_label(&self) -> &'static str {
        SKIPS.get(self.skip).map_or("", |&(_, label)| label)
    }

    /// Moves the `▸` by `delta`, stopping at the ends.
    ///
    /// [`step_in`] rather than a match, so this list clamps by the same
    /// implementation every other list in the crate does — the rule that only the tab
    /// ring wraps is written once.
    pub fn step_row(&mut self, delta: isize) {
        self.row = step_in(&ROWS, self.row, delta);
    }

    /// Turns the value under the cursor by `delta`.
    ///
    /// Every arm clamps rather than wrapping, and the two index rows go through
    /// [`step_in`] on their ladder for it. The bounds are the ladders' own where one
    /// exists ([`LEVEL_CAP`], [`Material::ALL`]) and this module's where the rules have
    /// none ([`MAX_PRESTIGE_DIAL`], [`MAX_CHARGE_DIAL`]).
    pub fn adjust(&mut self, delta: isize) {
        match self.row {
            // Any nudge flips it: a two-state row has no direction, and requiring `→`
            // for on and `←` for off would be a rule the frame cannot show.
            DevRow::FreeUpgrades => self.free_upgrades = !self.free_upgrades,
            DevRow::Amount => self.amount = step_index(AMOUNTS.len(), self.amount, delta),
            DevRow::Material => self.material = step_in(&Material::ALL, self.material, delta),
            DevRow::Level => self.level = dial(self.level, delta, 1, LEVEL_CAP),
            DevRow::Prestige => self.prestige = dial(self.prestige, delta, 0, MAX_PRESTIGE_DIAL),
            DevRow::Charges => self.charges = dial(self.charges, delta, 1, MAX_CHARGE_DIAL),
            DevRow::SkipTime => self.skip = step_index(SKIPS.len(), self.skip, delta),
            DevRow::Everything | DevRow::Experience => {}
        }
    }

    /// What the row prints to the right of its label.
    ///
    /// **The arrows are added by [`DevRow::adjustable`] and not by each arm**, so a row
    /// that draws a spinner is exactly a row `←/→` moves. Written the other way, the two
    /// facts would be stated twice and the second one to be edited would be the lie.
    fn value(&self, row: DevRow) -> String {
        let amount = grouped(self.amount());
        let value = match row {
            DevRow::FreeUpgrades => (if self.free_upgrades { "on" } else { "off" }).to_owned(),
            DevRow::Amount => amount,
            DevRow::Material => self.material.name().to_owned(),
            DevRow::Everything => format!("{amount} of each"),
            DevRow::Experience => format!("{amount} xp"),
            DevRow::Level => self.level.to_string(),
            DevRow::Prestige => self.prestige.to_string(),
            DevRow::Charges => self.charges.to_string(),
            DevRow::SkipTime => self.skip_label().to_owned(),
        };
        if row.adjustable() {
            format!("◄ {value} ►")
        } else {
            value
        }
    }
}

/// Turns a plain counter by `delta`, clamped into `low..=high`.
///
/// Signed arithmetic and then a clamp, for [`crate::cursor::step_index`]'s reason: a
/// `u32` at its floor decremented directly wraps to [`u32::MAX`], which on the level
/// row would read as "the ladder has four billion rungs" rather than as "you are at
/// the bottom".
fn dial(current: u32, delta: isize, low: u32, high: u32) -> u32 {
    let next = i64::from(current) + delta as i64;
    let clamped = next.clamp(i64::from(low), i64::from(high));
    u32::try_from(clamped).unwrap_or(low)
}

/// The column each row's value starts at, counted from the label.
const VALUE_COLUMN: usize = 18;

/// Draws the menu.
///
/// Its width and height are counted from its contents rather than taken from a
/// wireframe — there is no wireframe. The nine rows plus a blank line and a key hint
/// fit inside the 80×24 budget [`too_small`](crate::overlay::too_small) already
/// guarantees, so the box never needs to scroll and this module needs no scrollbar.
pub fn render(frame: &mut Frame, area: Rect, dev: &DevState) {
    let width = VALUE_COLUMN;
    let mut lines: Vec<String> = vec![String::new()];
    for row in ROWS {
        // The `▸` is a mark, so `theme::marked` gives it ACCENT without this module
        // naming a colour — the same route every list row in the crate takes.
        let mark = if row == dev.row { "▸" } else { " " };
        let label = row.label();
        let value = dev.value(row);
        lines.push(format!(" {mark} {label:<width$}{value}"));
    }
    lines.push(String::new());
    lines.push(" ↑↓ row   ←→ value   Enter apply   Esc close".to_owned());

    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::modal(frame, area, 52, 14, " Dev menu ", &borrowed);
}

/// The columns the marker is drawn into, at the right end of the tab row.
///
/// **Measured, not chosen.** The six tab titles fill exactly 76 of the 80 columns the
/// layout guarantees, and their last ink is the `s` of `6 Levels` — so a marker has four
/// columns to live in and must leave the first of them blank or it reads as part of that
/// tab. `DEV FREE` was the first draft and it ate three of its letters; `FREE` still
/// abutted it.
///
/// Two rules follow, tested apart: the marker is drawn into a rect this wide, so a longer
/// one is *clipped* rather than allowed to overwrite a title; and [`MARKER`] is short
/// enough that it never comes to that.
pub const MARKER_COLUMNS: usize = 4;

/// The marker the tab bar carries while the dev menu is available.
///
/// **One word for both states, and the state is the colour.** Three columns is what the
/// tab row leaves (see [`MARKER_COLUMNS`]), and there is no three-letter word for *"every
/// upgrade is free"* that a reader would recognise — a truncation would be worse than no
/// distinction at all. So the persistent signal is
/// [`theme::REFUSED`](crate::theme::REFUSED) against
/// [`theme::MUTED`](crate::theme::MUTED), and it is not the only one: turning the toggle
/// raises a toast, and the row itself reads `◄ on ►` for as long as the menu is open.
/// Three channels, and the one purchase the mode actually changes announces itself.
///
/// **What the marker is for is the build, not the mode.** Its job on the other five
/// screens is to say *this session can cheat*; the mode's own consequences are visible
/// where they happen.
pub const MARKER: &str = "DEV";

/// The most columns [`MARKER`] may use — one less than the rect, so the blank that
/// separates it from `6 Levels` survives.
const MARKER_TEXT_COLUMNS: usize = MARKER_COLUMNS - 1;

/// The bound above, enforced at **compile time** rather than by a test.
///
/// The same device `tunables` uses for the two world-unlock thresholds' ordering: a
/// widened marker is a build error, and does not have to be caught by someone reading
/// `6 Levels DE` in a failing frame. `len` counts bytes where the rect counts columns,
/// which errs the safe way — a multi-byte glyph is refused a column earlier than it needs
/// to be.
const _: () = assert!(MARKER.len() <= MARKER_TEXT_COLUMNS);

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(dev: &DevState) -> String {
        crate::overlay::render_to_string(|frame, area| render(frame, area, dev))
    }

    #[test]
    fn the_menu_opens_with_nothing_switched_on() {
        let dev = DevState::default();
        assert!(!dev.free_upgrades, "a fresh session was already free");
        assert_eq!(dev.row, DevRow::FreeUpgrades);
        assert_eq!(dev.amount(), 1_000);
        assert_eq!(dev.skip(), Duration::from_secs(3_600));
        assert_eq!(dev.level, 1);
    }

    #[test]
    fn the_cursor_clamps_at_both_ends_of_the_list() {
        let mut dev = DevState::default();
        dev.step_row(-1);
        assert_eq!(dev.row, ROWS[0], "the cursor walked off the top");

        for _ in 0..ROWS.len() * 2 {
            dev.step_row(1);
        }
        assert_eq!(
            dev.row,
            DevRow::SkipTime,
            "the cursor walked off the bottom"
        );
    }

    #[test]
    fn the_amount_ladder_climbs_in_powers_of_ten_and_stops() {
        let mut dev = DevState {
            row: DevRow::Amount,
            ..DevState::default()
        };

        dev.adjust(1);
        assert_eq!(dev.amount(), 10_000);
        for _ in 0..10 {
            dev.adjust(1);
        }
        assert_eq!(
            dev.amount(),
            1_000_000,
            "the ladder did not stop at the top"
        );
        for _ in 0..10 {
            dev.adjust(-1);
        }
        assert_eq!(dev.amount(), 1, "the ladder did not stop at the bottom");
    }

    #[test]
    fn either_arrow_flips_the_free_upgrades_row() {
        let mut dev = DevState::default();
        dev.adjust(-1);
        assert!(dev.free_upgrades);
        dev.adjust(-1);
        assert!(!dev.free_upgrades);
    }

    #[test]
    fn the_level_dial_stops_inside_the_ladder_the_game_has() {
        let mut dev = DevState {
            row: DevRow::Level,
            ..DevState::default()
        };

        dev.adjust(-1);
        assert_eq!(dev.level, 1, "the dial went below the first level");

        for _ in 0..LEVEL_CAP + 5 {
            dev.adjust(1);
        }
        assert_eq!(dev.level, LEVEL_CAP, "the dial went past the cap");
    }

    #[test]
    fn the_two_derived_rows_have_no_value_to_turn() {
        let mut dev = DevState::default();
        for row in [DevRow::Everything, DevRow::Experience] {
            dev.row = row;
            let before = dev.clone();
            dev.adjust(1);
            assert_eq!(dev, before, "{row:?} moved something");
            assert!(!row.adjustable());
        }
    }

    #[test]
    fn the_material_row_walks_all_fifteen_piles() {
        let mut dev = DevState {
            row: DevRow::Material,
            ..DevState::default()
        };
        for _ in 0..Material::ALL.len() * 2 {
            dev.adjust(1);
        }
        assert_eq!(dev.material, Material::ALL[Material::ALL.len() - 1]);
    }

    #[test]
    fn the_frame_names_every_row_and_marks_the_one_under_the_cursor() {
        let dev = DevState {
            row: DevRow::Level,
            ..DevState::default()
        };
        let frame = draw(&dev);

        for row in ROWS {
            assert!(
                frame.contains(row.label()),
                "{} is missing\n{frame}",
                row.label()
            );
        }
        assert!(frame.contains("▸ Mining level"), "{frame}");
        assert!(frame.contains("Free upgrades     ◄ off ►"), "{frame}");
        assert!(frame.contains("Enter apply"), "{frame}");
    }

    #[test]
    fn the_derived_rows_print_the_amount_they_would_spend() {
        let dev = DevState::default();
        let frame = draw(&dev);
        assert!(frame.contains("Give everything   1 000 of each"), "{frame}");
        assert!(frame.contains("Give experience   1 000 xp"), "{frame}");
    }
}
