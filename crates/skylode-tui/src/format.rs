//! Text formatting shared across screens.
//!
//! [`grouped`] enforces the cross-cutting rule that numbers are **exact, with a
//! thousands separator, and never abbreviated** (UI.md §5.6): `1 240`, `418 297`,
//! never `1.2k`. [`justified`] lays a label and a value on one row with the value
//! flush to the right — the shape a roadmap row, a stat line and a table row all
//! repeat. [`xp_progress`] is the one reading that has a *non-numeric* answer, and it
//! is here so all three screens that print it give the same one. [`roman`] and
//! [`rung_label`] name a pickaxe rung, which both the Upgrades roadmap and the toast
//! announcing a purchase have to do identically. Keeping them together means the
//! grouping and the alignment cannot drift between the XP gauge and the inventory
//! table.

use std::time::Duration;

use skylode_core::{
    pickaxe::PickaxeTier,
    tunables::{RAW_PER_COMPRESSED, TICKS_PER_SECOND},
};

/// The glyph a truncated line ends on, and the one column [`truncate`] must keep for
/// it.
const ELLIPSIS: char = '…';

/// Groups `n` into space-separated thousands: `1240` becomes `"1 240"`.
///
/// A **plain space**, not a comma or a dot, because the wireframes are drawn that
/// way (`1 240 / 2 300`) and because the two punctuation choices are the two that
/// collide with a decimal point in the locales this reader lives between. The
/// separator is the ASCII space so the rendered row matches the frame byte for
/// byte, which is what the layout tests assert against.
pub fn grouped(n: u32) -> String {
    grouped_u64(u64::from(n))
}

/// [`grouped`], for a count that outgrows a [`u32`].
///
/// The one figure in the game that does: [`OfflineReport::blocks`] is a rate times an
/// absence, and seven days of it at a raised auto-miner rate passes four billion. The
/// two share an implementation rather than a convention, so the separator cannot be a
/// space in one reading of a number and a comma in the other.
///
/// [`OfflineReport::blocks`]: skylode_core::game::OfflineReport
pub fn grouped_u64(n: u64) -> String {
    // Built from the least-significant digit up — grouping counts from the right,
    // so the string is assembled reversed and flipped once at the end rather than
    // repeatedly measuring how far the current digit sits from the decimal point.
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (count, ch) in digits.chars().rev().enumerate() {
        // A separator every third digit, but never a leading one: `count == 0` is
        // the first digit and `count % 3 == 0` there would prepend a stray space.
        if count > 0 && count % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// A span of simulated ticks as `14h 22m`, or `22m` under the hour.
///
/// **Ticks in, because ticks are what the core counts.** Playtime advances inside
/// `tick` and is stored as a count of them, which is what makes it deterministic; the
/// division belongs here rather than in the core, where a remainder would become a
/// second thing a save can be wrong about.
///
/// **Hours never roll over into days**, unlike the offline summary's *"6d 4h away"*.
/// The two answer different questions: an absence is a span the player waited out and
/// reads best in its largest unit, while playtime is an achievement being totalled —
/// `47h 10m` is the figure someone is proud of, and `1d 23h` reads as a timer.
///
/// Seconds are dropped rather than rounded. A figure that ticks over while being read
/// invites the eye to the one digit that matters least.
pub fn duration_hm(ticks: u64) -> String {
    let minutes = ticks / TICKS_PER_SECOND / 60;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if hours == 0 {
        return format!("{minutes}m");
    }
    // Two digits on the minutes only when there are hours in front of them, so a
    // column of figures lines up (`14h 22m`, `3h 07m`) without `07m` ever standing
    // alone and reading as a stopwatch.
    format!("{hours}h {minutes:02}m")
}

/// A countdown of simulated ticks as whole seconds, **rounded up**.
///
/// **The fourth duration in this module, and the only one that counts *down*.** The
/// other three total something that has already happened, where rounding either way is
/// a cosmetic choice. This one labels a boost that is still running, and rounding
/// *down* would print `0s` for the last twenty ticks of a live boost — a gauge saying
/// the thing is over while it is still multiplying. `div_ceil` makes the readout hit
/// zero exactly when the boost does.
///
/// **Widened, divided, narrowed.** [`TICKS_PER_SECOND`] is a `u64` because the offline
/// accrual multiplies by it across days, while a tick countdown is a `u32`; dividing in
/// the wider type keeps the conversion total, where a cast either way would be the
/// compiler taking the programmer's word for it. The narrowing back cannot fail — a
/// `u32` of ticks over twenty is a smaller number — and is written as a fallback rather
/// than an `unwrap` only because this crate's lints leave none to spend.
pub fn boost_seconds(ticks: u32) -> u32 {
    u32::try_from(u64::from(ticks).div_ceil(TICKS_PER_SECOND)).unwrap_or(u32::MAX)
}

/// A span of wall-clock time as `6d 4h`, `6h 12m`, `12m` or `45s`.
///
/// **The third duration format in this module, and none of the three is a special
/// case of another.** [`duration_hm`] totals a *playtime* and therefore never rolls
/// hours into days — `47h 10m` is a figure to be proud of and `1d 23h` reads as a
/// timer. [`age`] labels a row in a log and gives one unit, because every column it
/// spends is a column taken from the sentence. This one describes an **absence**: a
/// span the player lived through and reads best in its largest unit, with the next one
/// down for the part that would otherwise be rounded away.
///
/// Two units where there are two to give. Under a minute it says seconds, because
/// [`GameState::resume`](skylode_core::game::GameState::resume) can produce a short
/// span that still paid for a block, and `0m` would be a screen announcing nothing.
///
/// [`Duration`] and not ticks, unlike [`duration_hm`]: an absence is measured against
/// the wall clock and never simulated, so there are no ticks to divide.
pub fn span(elapsed: Duration) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;

    let seconds = elapsed.as_secs();
    match seconds {
        s if s >= DAY => format!("{}d {}h", s / DAY, s % DAY / HOUR),
        // Two digits on the minutes only behind an hour, so a column lines up
        // (`14h 22m`, `3h 07m`) without `07m` ever standing alone.
        s if s >= HOUR => format!("{}h {:02}m", s / HOUR, s % HOUR / MINUTE),
        s if s >= MINUTE => format!("{}m", s / MINUTE),
        s => format!("{s}s"),
    }
}

/// How long ago something was said, in the shortest honest unit: `12s`, `4m`, `3h`.
///
/// **One unit and never two**, which is where this parts company with
/// [`duration_hm`]: it is a column in a log of one-line entries, and every column
/// spent on precision is a column taken from the sentence. `3h` is all the answer a
/// reader of a history wants, and the row below it is already older.
///
/// Days are the last step and do not roll further. The buffer is a session's, and a
/// session long enough to need weeks has already lost its oldest entries to
/// [`HISTORY_CAP`](crate::toast::HISTORY_CAP).
pub fn age(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    match seconds {
        s if s < MINUTE => format!("{s}s"),
        s if s < HOUR => format!("{}m", s / MINUTE),
        s if s < DAY => format!("{}h", s / HOUR),
        s => format!("{}d", s / DAY),
    }
}

/// `text` cut to `width` columns, ending on an `…` when anything was dropped.
///
/// **The frames under-count what the game actually says.** `docs/UI.md` §5.5 draws its
/// history with invented, abbreviated lines (`+80 A. Debris`); the real sentence
/// [`announce::of`](crate::announce::of) produces is `Level 23 — +115 Quartz, +80
/// Ancient Debris — claim on 6`, which is fifty-five columns against the forty-seven a
/// 80-column terminal leaves. Left alone, ratatui clips it flush against the border,
/// so a cut word and a word that merely ends there look identical.
///
/// The `…` is what separates them, and it costs a column: a line that just fits keeps
/// all `width` columns, and one that does not keeps `width - 1` and the glyph.
///
/// Columns are counted in `chars` for [`justified`]'s reason — the sentences carry
/// `—`, `×` and `…`, each one column wide and several bytes long.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push(ELLIPSIS);
    out
}

/// A line of exactly `width` columns with `left` at the start and `right` flush to
/// the end — the shape a roadmap row, a stat line and a table row all repeat.
///
/// Counts columns in `chars`, not bytes: marks like `▸●` and the `·` separator are
/// multi-byte but one column wide, and byte lengths would over-pad every row that
/// carries one. If `left` and `right` together already exceed `width`, the pad is
/// zero rather than negative — the row overflows visibly instead of panicking on a
/// subtraction the way a `usize` underflow would.
pub fn justified(left: &str, right: &str, width: usize) -> String {
    let used = left.chars().count() + right.chars().count();
    let pad = width.saturating_sub(used);
    let mut out = String::with_capacity(left.len() + pad + right.len());
    out.push_str(left);
    for _ in 0..pad {
        out.push(' ');
    }
    out.push_str(right);
    out
}

/// A raw total in the denominations a price is quoted in, the material unnamed:
/// `1 Compressed`, `6 Compressed + 50`, `40`, `0`.
///
/// **Here so that a refusal and the price it refuses cannot split the same number two
/// ways.** The Upgrades panes quote a price through
/// [`CostLine::requirements`](skylode_core::economy::CostLine::requirements), which
/// drops a denomination that rounds to nothing; a toast built from
/// [`Affordability::Insufficient`](skylode_core::economy::Affordability::Insufficient)
/// reads its shortfall in **raw**, because the core's first pass asks *"is the ore
/// there at all"* and that question has no denomination. Both are right, and side by
/// side they contradicted each other — the pane said `1 Compressed Stone` over a toast
/// saying `100 Stone`. This is the one place the split is spelled for the front-end,
/// and it repeats `requirements`' rule rather than inventing a second one.
///
/// **The material is deliberately absent.** Every caller has already named it —
/// `Not enough Stone — …` — and a second `Stone` inside the number would read as a
/// different pile.
///
/// A total under [`RAW_PER_COMPRESSED`] is bare, including zero: `0 held` is what a
/// penniless player holds, and `0 Compressed + 0` states it twice.
pub fn denominations(total: u32) -> String {
    let compressed = total / RAW_PER_COMPRESSED;
    let raw = total % RAW_PER_COMPRESSED;
    match (compressed, raw) {
        (0, _) => grouped(raw),
        (_, 0) => format!("{} Compressed", grouped(compressed)),
        _ => format!("{} Compressed + {}", grouped(compressed), grouped(raw)),
    }
}

/// What the player **actually holds**, in the two denominations they hold it in.
///
/// **Not [`denominations`], and confusing the two is a lie about the purse.** That one
/// takes a raw *total* and splits it the way a price is quoted, which is right for a
/// price — a `Cost` is computed from a total and owed in the split. A holding is not
/// computed from anything: the player has some Compressed units and some raw items, and
/// [`Inventory::raw_value`] adds them up into a value that no longer says which. Passing
/// that value through `denominations` reports 20 000 raw Amethyst as `200 Compressed`,
/// which is the exact opposite of the truth and hides the very refusal — *you hold the
/// value in the wrong denomination* — that the line is there to explain.
///
/// **Both parts are kept, where `denominations` drops a zero**, for the same reason: the
/// line exists to be compared against a price quoted in both, so `0 Compressed + 20 000`
/// is the whole answer and `20 000` is half of it. An empty purse is the one exception —
/// `0 Compressed + 0` states nothing twice.
///
/// [`Inventory::raw_value`]: skylode_core::inventory::Inventory::raw_value
pub fn holding(compressed: u32, raw: u32) -> String {
    if compressed == 0 && raw == 0 {
        return "0".to_owned();
    }
    format!("{} Compressed + {}", grouped(compressed), grouped(raw))
}

/// Roman numerals `I`..=`XV` — exactly the range an Efficiency level can take, since
/// [`PickaxeTier::efficiency_cap`] tops out at 15 on Netherite.
const ROMAN: [&str; 15] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
];

/// `level` as a Roman numeral, or `"?"` past the table.
///
/// The fallback is unreachable in play — [`ROMAN`] spans every level any cap allows —
/// but it exists so the lookup is total: this crate's lints forbid the `unwrap` that
/// would be the alternative, and a panic while drawing a frame is the worst way for a
/// front-end to report that a cap moved.
///
/// **Here rather than in [`view`](crate::view)**, where it lived until the Upgrades
/// ladder needed it: a toast naming a bought rung is written by
/// [`app`](crate::app), which holds no read model, so a numeral private to the
/// projection would have had to be duplicated or made public from the wrong module.
pub fn roman(level: u8) -> &'static str {
    // `level - 1` cannot underflow: zero is filtered here rather than by the callers,
    // each of which would otherwise repeat the guard.
    if level == 0 {
        return "?";
    }
    ROMAN.get(usize::from(level) - 1).copied().unwrap_or("?")
}

/// A prestige rank, as the Stats panel and the two prestige modals print it.
///
/// **Not [`roman`], and the difference is the range.** An enchant level is bounded by a
/// cap the tables spell out, so `ROMAN` spans it and a value past the table is a bug.
/// A prestige rank is deliberately *unbounded* — `prestige::multiplier_permille`
/// documents that a player who keeps trading runs in keeps meeting runs of much the
/// same length — so `roman`'s `"?"` would not be reporting a moved cap, it would be
/// refusing to name a rank the player legitimately holds. Past the numerals the arabic
/// number is the honest answer.
///
/// **Rank `0` is the common case**, not an edge one: it is every run before the first
/// prestige, and `roman(0)` answers `"?"` because an enchant at level 0 is one the
/// player does not own. A rank of 0 *is* owned — it is where everyone starts — so it
/// prints as `0`.
pub fn prestige_rank(rank: u32) -> String {
    match u8::try_from(rank) {
        Ok(0) => "0".to_owned(),
        // Roman while the table covers it; the numeral is what §5.5 and §6.8 draw.
        Ok(small) if usize::from(small) <= ROMAN.len() => roman(small).to_owned(),
        _ => grouped(rank),
    }
}

/// A permille multiplier as the frames quote it: `1200` becomes `"×1.20"`.
///
/// Two decimals, always, because the three figures that share a line in §6.8
/// (`×1.20  →  ×1.30`) have to align on the point — and because the step between two
/// ranks is `PRESTIGE_MULT_PER_RANK_PERMILLE`, which is finer than one decimal can
/// show. The division is done in integers and printed with a literal `.`, rather than
/// through a float, so the string cannot pick up a rounding artefact on its way to the
/// screen.
pub fn multiplier(permille: u32) -> String {
    // Permille to hundredths: the tens digit is dropped rather than rounded, which is
    // exact for every multiplier the rank curve produces (they are all multiples of 10).
    let hundredths = permille / 10;
    format!("×{}.{:02}", hundredths / 100, hundredths % 100)
}

/// The number a mine-track level is **shown** as: the core counts from 0, the player
/// counts from 1.
///
/// A mine's size and richness levels are *indexes into a table of ten rungs*
/// (`MINE_SIZES`, and richness `0..=9`), which is why the core numbers them from zero
/// — and why the player must not see them that way. A level of `0` there does not mean
/// "absent": the mine exists, it is 3×3, and the player owns the first rung of ten. So
/// `Size level 0` reads as a mine that has not started, when what it describes is a
/// mine at the bottom of a ladder it is standing on. Enchants are the opposite case and
/// deliberately untouched: their level `0` really is *not owned*, so `roman(0)`
/// answering `?` and the roadmap printing `none` are both right as they stand.
///
/// **One function rather than a `+ 1` at each print site**, because the sites are
/// nine and spread over four screens, an announcement and a refusal label: a screen
/// that forgot the shift would not fail a build, it would quietly claim a rung the
/// player does not own is the one they are standing on.
///
/// The return is never 0, which is what lets the dial's bar divide by
/// `shown_rung(richness_max)` — the top rung's number *is* the number of rungs.
pub fn shown_rung(level: u32) -> u32 {
    level.saturating_add(1)
}

/// A rung of the pickaxe ladder, named the way `docs/UI.md` §5.4 lists it:
/// `Netherite Pickaxe` for a tier jump, `Diamond Eff IV` for an Efficiency level.
///
/// **The tier's own word plus a suffix**, which is what
/// [`PickaxeTier::name`](skylode_core::pickaxe::PickaxeTier::name) returning the bare
/// material is for: the roadmap writes `Pickaxe` once per tier and never on the thirty
/// Efficiency rungs between.
///
/// A rung at Efficiency 0 is the tier itself — the jump, or the bare pickaxe a run
/// starts with. Both read the same, and correctly: what the row names is *arriving at
/// that tier*, and the two differ only in whether anybody paid for it.
pub fn rung_label(tier: PickaxeTier, efficiency: u8) -> String {
    if efficiency == 0 {
        format!("{} Pickaxe", tier.name())
    } else {
        format!("{} Eff {}", tier.name(), roman(efficiency))
    }
}

/// The XP readout: `1 240 / 2 300`, or [`MAXED`] once there is no next level.
///
/// `to_next` is an [`Option`] because
/// [`Player::experience_to_next_level`](skylode_core::player::Player::experience_to_next_level)
/// is: at [`LEVEL_CAP`](skylode_core::tunables::LEVEL_CAP) there is no rung left to
/// price, and the core says so in the type rather than with a `0` sentinel. Written
/// once here because **three screens print this same reading** — the Mine gauge,
/// the Stats Progression panel and the Levels title — and three independent
/// `map_or`s would be three chances to word the capped case differently, or to
/// divide by the zero a sentinel would have handed them.
pub fn xp_progress(xp: u32, to_next: Option<u32>) -> String {
    match to_next {
        Some(needed) => format!("{} / {}", grouped(xp), grouped(needed)),
        None => MAXED.to_owned(),
    }
}

/// What a track with nothing left to sell reads.
///
/// The same word the Upgrades Mines rows already use for a maxed size or richness
/// track, reused rather than reinvented: a player who has seen `Stone  Size
/// maxed` should read `Lv 50  maxed` as the same statement about a different
/// ladder.
pub const MAXED: &str = "maxed";

/// The gauge fill for that same reading, in `0.0..=1.0`.
///
/// **Full, not empty, at the cap.** A capped player has earned every level there
/// is, so an empty bar would read as "no progress" for the one state that is total
/// progress. Paired with [`xp_progress`] here so the number and the bar cannot
/// disagree about what the cap looks like.
pub fn xp_ratio(xp: u32, to_next: Option<u32>) -> f64 {
    to_next.map_or(1.0, |needed| f64::from(xp) / f64::from(needed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The player counts the rungs they own; the core indexes the table they sit in.
    ///
    /// Both ends of both mine tracks are checked because both are load-bearing: the
    /// bottom rung must read `1` (a mine at size level 0 is a real 3×3 mine, not a
    /// missing one) and the top must read `10` (ten rungs, so the last one is the
    /// tenth and the dial's `10/10` is the count of them).
    #[test]
    fn a_rung_is_shown_counting_from_one() {
        assert_eq!(shown_rung(0), 1);
        assert_eq!(shown_rung(9), 10);
        // The saturating add is unreachable through the game — no track has `u32::MAX`
        // rungs — and is there so a corrupt or future level cannot panic mid-frame,
        // which is [`roman`]'s reason for answering `?` rather than indexing.
        assert_eq!(shown_rung(u32::MAX), u32::MAX);
    }

    #[test]
    fn a_number_below_a_thousand_is_left_untouched() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(9), "9");
        assert_eq!(grouped(680), "680");
    }

    #[test]
    fn a_thousand_gets_one_separator() {
        assert_eq!(grouped(1_240), "1 240");
        assert_eq!(grouped(2_300), "2 300");
    }

    #[test]
    fn separators_repeat_every_three_digits() {
        assert_eq!(grouped(418_297), "418 297");
        assert_eq!(grouped(1_000_000), "1 000 000");
    }

    #[test]
    fn the_boundary_at_exactly_a_thousand_groups_correctly() {
        // 1000 is the first value that groups: the separator sits after the 1, not
        // before it, which is the off-by-one `count > 0` guards against.
        assert_eq!(grouped(1_000), "1 000");
    }

    #[test]
    fn a_total_below_a_compressed_unit_is_quoted_bare() {
        // Including zero, which is what a penniless player holds: `0 Compressed + 0`
        // says the same thing twice and reads as two shortfalls.
        assert_eq!(denominations(0), "0");
        assert_eq!(denominations(40), "40");
        assert_eq!(denominations(99), "99");
    }

    #[test]
    fn a_whole_number_of_units_drops_the_raw_half() {
        // `CostLine::requirements`' own rule, repeated: a denomination that rounds to
        // nothing is not owed, so naming it would quote a payment nobody makes.
        assert_eq!(denominations(100), "1 Compressed");
        assert_eq!(denominations(1_000), "10 Compressed");
    }

    #[test]
    fn a_mixed_total_names_both_denominations_in_the_order_they_are_paid() {
        // The wireframes' own form, and the number that started this: a price of 650
        // is `6 Compressed + 50`, never the flat `650` the same value would make.
        assert_eq!(denominations(650), "6 Compressed + 50");
        assert_eq!(denominations(101), "1 Compressed + 1");
    }

    #[test]
    fn the_compressed_count_is_grouped_like_every_other_number() {
        // The cross-cutting rule of §5.6 reaches inside a price too: a six-figure
        // count of units is still read by a human.
        assert_eq!(denominations(1_240_000), "12 400 Compressed");
    }

    #[test]
    fn justified_puts_the_value_flush_right_and_fills_the_gap() {
        assert_eq!(justified("Lv", "50", 10), "Lv      50");
        assert_eq!(justified("XP", "1 240", 10), "XP   1 240");
    }

    #[test]
    fn justified_measures_multibyte_marks_as_one_column() {
        // `▸●` is four bytes but two columns: a byte count would pad two short.
        let line = justified("▸●", "x", 6);
        assert_eq!(line.chars().count(), 6, "padded by bytes, not columns");
        assert!(line.starts_with("▸●"));
        assert!(line.ends_with('x'));
    }

    #[test]
    fn justified_overflows_rather_than_panicking_when_too_narrow() {
        // Left and right already exceed the width: the pad floors at zero, and the
        // two simply abut instead of underflowing the `usize` subtraction.
        assert_eq!(justified("abcd", "efgh", 6), "abcdefgh");
    }

    #[test]
    fn xp_below_the_cap_reads_as_a_grouped_fraction() {
        assert_eq!(xp_progress(1_240, Some(2_300)), "1 240 / 2 300");
        assert_eq!(xp_ratio(1_150, Some(2_300)), 0.5);
    }

    #[test]
    fn xp_at_the_cap_reads_as_maxed_on_a_full_bar() {
        // The pairing is the point: a capped player has earned every level there
        // is, so the word and the bar must both say *finished* rather than the
        // word saying finished over an empty bar.
        assert_eq!(xp_progress(4_900, None), MAXED);
        assert_eq!(xp_ratio(4_900, None), 1.0);
    }

    /// Rank 0 is where every run starts, so it is the one this has to get right.
    #[test]
    fn a_prestige_rank_is_roman_but_starts_at_zero() {
        assert_eq!(prestige_rank(0), "0");
        assert_eq!(prestige_rank(1), "I");
        assert_eq!(prestige_rank(3), "III");
    }

    /// The rank is unbounded by design, so past the numerals the number is the answer
    /// — `roman`'s `?` would refuse to name a rank the player actually holds.
    #[test]
    fn a_rank_past_the_numerals_is_named_in_digits_rather_than_refused() {
        assert_eq!(prestige_rank(15), "XV");
        assert_eq!(prestige_rank(16), "16");
        assert_eq!(prestige_rank(1_200), "1 200");
    }

    #[test]
    fn a_multiplier_is_printed_to_two_decimals() {
        // Rank 0's exact `×1.00` is the identity `multiplier_permille` is built to
        // keep, and the one figure a run that has never prestiged prints.
        assert_eq!(multiplier(1_000), "×1.00");
        assert_eq!(multiplier(1_200), "×1.20");
        assert_eq!(multiplier(2_500), "×2.50");
    }

    /// Hours never roll into days, and the minutes take a leading zero only when
    /// there are hours in front of them to line up under.
    #[test]
    fn a_span_of_ticks_reads_as_hours_and_minutes() {
        let minutes = |n: u64| n * 60 * TICKS_PER_SECOND;

        assert_eq!(duration_hm(minutes(22)), "22m");
        assert_eq!(duration_hm(minutes(14 * 60 + 22)), "14h 22m");
        assert_eq!(duration_hm(minutes(3 * 60 + 7)), "3h 07m");
        // Two days of play is `47h`, not `1d 23h`: this figure is a total someone is
        // proud of, not a countdown.
        assert_eq!(duration_hm(minutes(47 * 60 + 10)), "47h 10m");
        assert_eq!(duration_hm(0), "0m");
    }

    /// An absence *does* roll into days, which is where it parts company with the
    /// playtime above: `9d 4h` is how long someone was away, and `220h` is not an
    /// answer anyone reads.
    #[test]
    fn an_absence_reads_in_its_two_largest_units() {
        assert_eq!(span(Duration::from_secs(6 * 3_600 + 12 * 60)), "6h 12m");
        assert_eq!(span(Duration::from_secs(3 * 3_600 + 7 * 60)), "3h 07m");
        assert_eq!(span(Duration::from_secs(9 * 86_400 + 4 * 3_600)), "9d 4h");
        assert_eq!(span(Duration::from_secs(12 * 60)), "12m");
        // Under the minute it says seconds rather than `0m`: a short absence that
        // still paid for a block gets a screen, and a screen announcing nothing would
        // read as a bug.
        assert_eq!(span(Duration::from_secs(45)), "45s");
        assert_eq!(span(Duration::ZERO), "0s");
    }

    /// Seconds are dropped rather than rounded, so a part-minute reads as the minute
    /// it is inside.
    #[test]
    fn a_span_of_ticks_drops_its_seconds() {
        assert_eq!(duration_hm(TICKS_PER_SECOND * 59), "0m");
        assert_eq!(duration_hm(TICKS_PER_SECOND * 61), "1m");
    }

    /// One unit and never two: the history's stamp column exists to be glanced at,
    /// and every column it spends is one the sentence does not get.
    #[test]
    fn an_age_reads_in_the_largest_unit_that_fits_and_only_that_one() {
        assert_eq!(age(0), "0s");
        assert_eq!(age(59), "59s");
        assert_eq!(age(60), "1m");
        assert_eq!(age(90), "1m");
        assert_eq!(age(59 * 60), "59m");
        assert_eq!(age(3_600), "1h");
        assert_eq!(age(23 * 3_600), "23h");
        assert_eq!(age(24 * 3_600), "1d");
        assert_eq!(age(9 * 24 * 3_600), "9d");
    }

    /// A line that fits keeps every column; one that does not gives a column back to
    /// the `…`, so a cut sentence cannot be mistaken for one that merely ends there.
    #[test]
    fn a_line_too_long_for_its_box_ends_on_an_ellipsis() {
        assert_eq!(truncate("Mine refilled", 20), "Mine refilled");
        assert_eq!(truncate("Mine refilled", 13), "Mine refilled");
        assert_eq!(truncate("Mine refilled", 12), "Mine refill…");
        assert_eq!(truncate("Mine refilled", 8), "Mine re…");
        // The result is exactly `width` columns, `…` included — the glyph replaces a
        // column rather than being appended past the box.
        assert_eq!(truncate("Mine refilled", 12).chars().count(), 12);
        assert_eq!(truncate("Mine refilled", 1), "…");
        assert_eq!(truncate("Mine refilled", 0), "");
    }

    /// Counted in columns and not bytes, like every other helper here: the real
    /// sentences carry `—` and `×`, which are one column and three bytes.
    #[test]
    fn truncating_counts_columns_and_not_bytes() {
        let line = "Explosive — 9 blocks";
        assert_eq!(line.chars().count(), 20);
        assert_eq!(truncate(line, 20), line);
        assert_eq!(truncate(line, 12), "Explosive —…");
        // The cut never lands inside a multi-byte glyph, which a byte slice would do
        // and would panic on.
        assert_eq!(truncate(line, 11).chars().count(), 11);
    }
}
