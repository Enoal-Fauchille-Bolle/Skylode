//! The two prestige modals: the preview, then the typed confirm (UI.md §6.8–6.9).
//!
//! **The preview is two columns because the reset is a trade.** The left column is
//! deliberately brutal — the deep reset is the point, and a preview that soft-pedals
//! it sets up the one complaint that cannot be undone. It is drawn unaffordable in the
//! common case, which is why the honest last line names the progression still owed
//! rather than a price for an ore that only drops past those gates.
//!
//! **The confirm is the one place in the game that asks for typing.** Everything
//! else is a spinner or a menu because a keystroke cannot be wrong; here a keystroke
//! *being* possible is the point — it must break the training that nothing is final.
//!
//! Both draw from [`PrestigeView`] and never from the run, which is the dip modal's
//! rule and is load-bearing here for a second reason: the Stats panel the player
//! opened this from quotes the same five figures, and a box that re-derived them could
//! contradict what they had just read.

use ratatui::{Frame, layout::Rect};
use skylode_core::economy::Affordability;

use crate::{
    format::{denominations, holding, justified, multiplier, prestige_rank, roman},
    view::PrestigeView,
};

/// The word §6.9 asks for, and the cap on what the field will hold.
///
/// Public because [`App`](crate::app::App) both compares against it and refuses to
/// grow the field past it: one spelling, so a confirm that accepted a word the box
/// never asked for is unwritable.
pub const CONFIRM_WORD: &str = "PRESTIGE";

/// How wide the field is drawn, from the frame's own `> ____________`.
///
/// Four columns longer than [`CONFIRM_WORD`], so a completed word still sits inside a
/// visible field rather than filling it exactly — the frame draws it that way, and a
/// field that ends where the word ends reads as a word with nowhere left to go.
const FIELD_WIDTH: usize = 12;

/// Where the preview's right-hand column starts.
///
/// The frame reaches it by spelling out the spaces (`Rank  II  →  III` then twelve of
/// them), which only works while every figure keeps the width it was drawn at. A rank
/// is `0` before the first prestige and `1 200` long after, and a price is now two
/// denominations rather than one total, so the column is a **pad** here.
///
/// **One column for both rows, where the frame drew two.** It sits six columns right of
/// the frame's, because the widest left-hand entry is now
/// `Cost  65 Compressed + 40 Amethyst` — and having `Multiplier` and `Held` start
/// together is worth more than either matching a count taken before the price was
/// split. Recorded in `docs/UI.md` §6.8.1.
///
/// One column *past* that widest entry, so the flush-right `✓`/`✗` keeps a gap from the
/// figure beside it. A purse absurd enough to close that gap — a million raw — meets
/// [`justified`]'s overflow rather than a panic, which is the same graceful end every
/// counted width in this crate has.
const COLUMN: usize = 35;

/// The writable width inside the 68-column box: two borders and two padding columns.
///
/// Named because the flush-right mark is placed against it, and a mark that drifted
/// off the edge would be the one glyph on the line the player is looking for.
const BODY_WIDTH: usize = 64;

/// Draws the prestige preview for `view` (UI.md §6.8).
pub fn render_preview(frame: &mut Frame, area: Rect, view: &PrestigeView) {
    let material = view.material.name();
    let mark = if view.lock.is_open() && view.verdict == Affordability::Affordable {
        "✓"
    } else {
        "✗"
    };

    // Both figures are variable-width — a rank can be `0`, `III` or `1 200`, and a
    // price is now split into two denominations — so the second column is placed by
    // padding rather than by counting spaces into the string. The frame's own column
    // is preserved for the figures it was drawn at, and nothing overruns for the ones
    // it was not.
    let ranks = format!(
        " Rank  {}  →  {}",
        prestige_rank(view.rank),
        prestige_rank(view.rank.saturating_add(1)),
    );
    let header = format!(
        "{ranks:<COLUMN$}Multiplier  {}  →  {}",
        multiplier(view.multiplier_permille),
        multiplier(view.next_multiplier_permille),
    );
    // `Cost` and `Held` in the *same* two denominations, because the whole question on
    // this line is whether one fits the other. They come from different places on
    // purpose: the price is a total the till splits, the purse is two counts read off
    // the inventory — see `format::holding` for why the second must not be computed
    // like the first.
    let costs = format!(" Cost  {} {material}", denominations(view.cost));
    let price = justified(
        &format!(
            "{costs:<COLUMN$}Held  {}",
            holding(view.held_compressed, view.held_raw)
        ),
        mark,
        BODY_WIDTH,
    );

    // The two columns. The left one is what the reset takes, deepest loss first; the
    // right one is fixed, because what survives a prestige is not a property of the
    // run — it is the rank, its multiplier and the preferences, always those three.
    let mut losses = vec![format!("{} Pickaxe → Wooden", view.tier.name())];
    // Dropped at zero rather than printed as `Efficiency 0 → 0`: an enchant at level 0
    // is one the player does not own, so listing it bills them for a loss they cannot
    // take. Same rule `NOTHING` follows on the Mine screen's Fortune line.
    if view.efficiency > 0 {
        losses.push(format!("Efficiency {} → 0", roman(view.efficiency)));
    }
    if view.fortune > 0 {
        losses.push(format!("Fortune {} → 0", roman(view.fortune)));
    }
    if view.other_enchants > 0 {
        losses.push(format!("All {} enchants → 0", view.other_enchants));
    }
    losses.push(format!("Mining level {} → 1", view.level));
    losses.push("Every mine's size and richness".to_owned());
    losses.push("Your entire inventory".to_owned());
    let keeps = [
        "Prestige rank",
        "The global multiplier",
        "Your settings",
        "",
        "",
        "",
        "",
    ];

    let mut lines = vec![
        String::new(),
        header,
        price,
        String::new(),
        " You lose                          You keep".to_owned(),
        " ────────────────────────────      ────────────────────────".to_owned(),
    ];
    for row in 0..losses.len().max(keeps.len()) {
        let left = losses.get(row).map_or("", String::as_str);
        let right = keeps.get(row).copied().unwrap_or("");
        // The right column starts at the counted column 35 of the frame; a left entry
        // that overran it would push its neighbour rather than being truncated, which
        // is the ratatui behaviour the box's fixed width already tolerates.
        lines.push(format!(" {left:<33} {right}"));
    }
    lines.push(String::new());
    lines.extend(closing_lines(view));

    super::modal(
        frame,
        area,
        68,
        18,
        " Prestige ",
        &lines.iter().map(String::as_str).collect::<Vec<_>>(),
    );
}

/// The box's last sentence: why this cannot be bought yet, or nothing.
///
/// **Two sources, and never both.** While a gate is shut the price is the wrong
/// answer — Amethyst only drops in the End, so quoting it to a player short of the
/// level answers a question they did not ask (§6.8). Once both gates are open the
/// lock has nothing left to say, and the line is free for what the till would refuse:
/// missing ore, or the value held in the wrong denomination.
///
/// The wording of a shortfall is deliberately *not* shared with
/// [`announce`](crate::announce) or with the purchase toasts: those word an
/// [`Affordability`] in one line for a three-second window, where this box has two
/// lines and no deadline.
fn closing_lines(view: &PrestigeView) -> Vec<String> {
    let material = view.material.name();
    if !view.lock.is_open() {
        // Two gates, so at most two clauses — built from the lock's two `Option`s, and
        // a third sentence would have to come from somewhere the lock does not report.
        let mut clauses = Vec::new();
        if let Some(level) = view.lock.missing_level() {
            clauses.push(format!("Lv {} of {level}", view.level));
        }
        if let Some(tier) = view.lock.missing_tier() {
            clauses.push(format!(
                "a {} pickaxe short of {}",
                view.tier.name(),
                tier.name()
            ));
        }
        return vec![
            format!(" You are {} —", clauses.join(" and ")),
            format!(" and {material} only drops past the level."),
        ];
    }
    match &view.verdict {
        Affordability::Affordable => vec![
            format!(" You hold the {material}. This cannot be undone."),
            String::new(),
        ],
        // The value is there in the wrong shape: one trip to `3 Inventory` fixes it,
        // and saying "go mining" here would send the player somewhere that cannot help.
        //
        // **The key is named in the line, which is §8.4's own rule.** The purchase
        // toasts end in `· c to go` for the same reason: `c` is dead until something is
        // refused, so the sentence that identifies the problem is the only place its fix
        // can be advertised. Here it has to be — a modal captures the keyboard, so a
        // player who has read this box has no footer left to read.
        Affordability::CompressFirst(_) => vec![
            format!(" You hold the {material} in the wrong denomination —"),
            " press  c  to go and convert it.".to_owned(),
        ],
        Affordability::Insufficient(_) => vec![
            format!(
                " You are {} {material} short — the End's richness dial is what",
                denominations(view.cost.saturating_sub(view.held))
            ),
            " turns a run into a rank.".to_owned(),
        ],
    }
}

/// Draws the typed prestige confirm for `view`, with `typed` in its field (UI.md §6.9).
pub fn render_confirm(frame: &mut Frame, area: Rect, view: &PrestigeView, typed: &str) {
    let rank = prestige_rank(view.rank.saturating_add(1));
    let title = format!(" Prestige {rank} ");
    // The field: what was typed, then underscores for the rest of the drawn width. The
    // count is over `chars` and not bytes — a stray multibyte keystroke must cost one
    // column, or the box's own border moves.
    let filled = typed.chars().count();
    let field = format!("{typed}{}", "_".repeat(FIELD_WIDTH.saturating_sub(filled)));

    super::modal(
        frame,
        area,
        56,
        11,
        &title,
        &[
            "",
            " This cannot be undone.",
            "",
            &format!(
                " {} {}  →  rank {rank}  ({})",
                denominations(view.cost),
                view.material.name(),
                multiplier(view.next_multiplier_permille),
            ),
            " Everything else resets.",
            "",
            &format!(" Type  {CONFIRM_WORD}  to confirm:"),
            &format!(" > {field}"),
        ],
    );
}

#[cfg(test)]
mod tests {
    use skylode_core::{material::Material, pickaxe::PickaxeTier};

    use super::*;
    use crate::view::View;

    /// The §6.8 fixture: rank II, mid-climb, holding nothing.
    fn locked() -> PrestigeView {
        View::sample().prestige
    }

    /// The same trade with both gates open and the price met.
    ///
    /// The purse is set in the **shape** the price is quoted in, not as a total: that
    /// is what `Affordable` means, and a fixture that held the value some other way
    /// would be describing a run the till refuses.
    fn ready() -> PrestigeView {
        PrestigeView {
            lock: skylode_core::prestige::lock(50, PickaxeTier::Netherite),
            tier: PickaxeTier::Netherite,
            level: 50,
            held: 6_540,
            held_compressed: 65,
            held_raw: 40,
            verdict: Affordability::Affordable,
            ..locked()
        }
    }

    #[test]
    fn the_preview_is_a_two_column_trade_drawn_unaffordable() {
        let view = locked();
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        assert!(
            frame.contains("You lose") && frame.contains("You keep"),
            "{frame}"
        );
        assert!(frame.contains("Diamond Pickaxe → Wooden"), "{frame}");
        assert!(frame.contains("The global multiplier"), "{frame}");
        assert!(frame.contains("Mining level 23 → 1"), "{frame}");
        // Unaffordable, so the honest last line names the progression owed.
        assert!(frame.contains("short of Netherite"), "{frame}");
        assert!(frame.contains("Lv 23 of 50"), "{frame}");
    }

    /// **The purse is read, never recomputed.** A player sitting on 20 000 raw Amethyst
    /// holds *no* Compressed units, and the value re-split as a price would claim 200 of
    /// them — the exact opposite of the truth, on the one line whose job is to explain
    /// why the trade is refused.
    #[test]
    fn a_purse_of_raw_ore_is_not_reported_as_compressed_units() {
        let view = PrestigeView {
            lock: skylode_core::prestige::lock(50, PickaxeTier::Netherite),
            tier: PickaxeTier::Netherite,
            level: 50,
            held: 20_000,
            held_compressed: 0,
            held_raw: 20_000,
            verdict: Affordability::CompressFirst(Vec::new()),
            ..locked()
        };
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        assert!(frame.contains("Held  0 Compressed + 20 000"), "{frame}");
        assert!(!frame.contains("200 Compressed"), "{frame}");
        // And the closing line names the loop that fixes it.
        assert!(frame.contains("wrong denomination"), "{frame}");
    }

    /// The departure from the frame, asserted rather than left to a doc note: the
    /// price is quoted in the denominations it is actually paid in.
    #[test]
    fn the_price_is_quoted_in_both_denominations() {
        let view = locked();
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        assert!(frame.contains("65 Compressed + 40 Amethyst"), "{frame}");
        assert!(!frame.contains("6 540 Amethyst"), "{frame}");
    }

    /// A rank-0 run is the common case, and `roman(0)` would have printed `?`.
    #[test]
    fn a_run_that_has_never_prestiged_reads_rank_zero() {
        let view = PrestigeView {
            rank: 0,
            multiplier_permille: 1_000,
            next_multiplier_permille: 1_100,
            efficiency: 0,
            fortune: 0,
            other_enchants: 0,
            tier: PickaxeTier::Wooden,
            level: 1,
            ..locked()
        };
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        assert!(frame.contains("Rank  0  →  I"), "{frame}");
        assert!(frame.contains("×1.00  →  ×1.10"), "{frame}");
        // Nothing owned, so nothing is billed as a loss.
        assert!(!frame.contains("Efficiency"), "{frame}");
        assert!(!frame.contains("enchants → 0"), "{frame}");
    }

    /// Once the gates are open the lock has nothing to say, and the line the §6.8
    /// frame reserved for it carries the till's refusal instead.
    #[test]
    fn an_open_lock_hands_the_closing_line_to_the_price() {
        let short = PrestigeView {
            held: 0,
            verdict: Affordability::Insufficient(Vec::new()),
            ..ready()
        };
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &short));
        assert!(
            frame.contains("65 Compressed + 40 Amethyst short"),
            "{frame}"
        );
        assert!(!frame.contains("short of Netherite"), "{frame}");

        let misshaped = PrestigeView {
            verdict: Affordability::CompressFirst(Vec::new()),
            ..ready()
        };
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &misshaped));
        assert!(frame.contains("wrong denomination"), "{frame}");
    }

    #[test]
    fn an_affordable_preview_is_marked_and_says_so() {
        let view = ready();
        let frame = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        assert!(frame.contains("✓"), "{frame}");
        assert!(frame.contains("This cannot be undone."), "{frame}");
    }

    #[test]
    fn the_confirm_asks_for_the_typed_word() {
        let view = ready();
        let frame = crate::overlay::render_to_string(|f, a| render_confirm(f, a, &view, ""));
        assert!(frame.contains("This cannot be undone."), "{frame}");
        assert!(frame.contains("Type  PRESTIGE  to confirm:"), "{frame}");
        assert!(frame.contains("> ____________"), "{frame}");
        assert!(frame.contains("Prestige III"), "{frame}");
    }

    /// The field echoes what was typed, mistakes included — §6.9's whole argument.
    #[test]
    fn the_field_shows_the_letters_as_they_are_typed() {
        let view = ready();
        let frame = crate::overlay::render_to_string(|f, a| render_confirm(f, a, &view, "PREZ"));
        assert!(frame.contains("> PREZ________"), "{frame}");
        let frame =
            crate::overlay::render_to_string(|f, a| render_confirm(f, a, &view, CONFIRM_WORD));
        assert!(frame.contains("> PRESTIGE____"), "{frame}");
    }

    #[test]
    fn the_confirm_quotes_the_same_price_as_the_preview() {
        let view = ready();
        let preview = crate::overlay::render_to_string(|f, a| render_preview(f, a, &view));
        let confirm = crate::overlay::render_to_string(|f, a| render_confirm(f, a, &view, ""));
        let price = format!("{} {}", denominations(view.cost), Material::Amethyst.name());
        assert!(preview.contains(&price) && confirm.contains(&price));
    }
}
