//! The dip modal (UI.md §6.7).
//!
//! Fires **only on a net regression** — a pickaxe chain that crosses a tier jump
//! and ends below the power it started at — never on an ordinary Efficiency step, or
//! it would be a modal nobody reads. It is **not a warning**: the dip is a chosen
//! decision point, so the frame states the trade in ticks per block (a number the
//! player can feel, unlike raw power) and offers the deal, `Not yet` the default.
//!
//! **It draws from the same [`PickaxeDetail`] the pane behind it draws from**, rather
//! than from the run. The player has just read those numbers in the detail pane; a
//! modal that re-derived them from `GameState` could disagree with the box it was
//! opened from, and the one disagreement that matters — the power before and after —
//! is the whole content of the decision.

use ratatui::{Frame, layout::Rect};

use crate::{format::roman, view::PickaxeDetail};

/// Draws the dip modal for `detail`, with the caret on `Buy it` when `buy`.
///
/// A pickaxe detail with no [`dip`](PickaxeDetail::dip) draws nothing at all: the
/// modal is only ever opened on one, and painting an empty box over the screen would
/// be worse than the missing confirmation.
pub fn render(frame: &mut Frame, area: Rect, detail: &PickaxeDetail, buy: bool) {
    let Some(dip) = &detail.dip else {
        return;
    };

    let mut lines = vec![String::new(), buying(detail)];
    if let Some((cap, _)) = detail.ceiling {
        lines.push(format!(" This resets Efficiency {} to 0.", roman(cap)));
    }
    lines.push(String::new());
    // Read off `power`, which every rung carries, rather than off `dip`, which only a
    // regression does: the two numbers are the same fact whether or not it is bad news,
    // and duplicating them onto the dip would let the modal and the pane behind it drift.
    let power = &detail.power;
    lines.push(format!(
        " Mining power      {:.1}   →   {:.1}",
        power.before, power.after
    ));
    lines.push(format!(
        " {}    {}  →  {} per block",
        power.block.name(),
        ticks(power.ticks_before),
        ticks(power.ticks_after)
    ));
    lines.push(String::new());
    match &dip.repaid_at {
        Some(repaid) => {
            lines.push(format!(
                " You get it back at {} ({:.1}),",
                repaid.rung, repaid.power
            ));
            lines.push(format!(" {} later.", purchases(repaid.rungs_later)));
        }
        // Netherite at its cap is the only rung with nothing past it, so a dip that is
        // never repaid is the end of the ladder rather than a projection failure. It
        // still has to say so: silence here would read as "repaid immediately".
        None => lines.push(" There is no rung past it to earn it back.".to_owned()),
    }
    lines.push(String::new());
    lines.push(choices(buy));

    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::modal(
        frame,
        area,
        64,
        u16::try_from(borrowed.len().saturating_add(2)).unwrap_or(u16::MAX),
        &format!(" {} ", detail.title),
        &borrowed,
    );
}

/// The opening sentence: what is being bought, named where naming it fits.
///
/// **Three shapes for one sentence**, because the frame's own — *"Buying Diamond
/// Efficiency V, then the tier jump"* — is written for a chain of exactly two, which
/// is the commonest case and not the only one. `M` can aim at a chain of nine, and
/// listing nine rung names in a modal would bury the two numbers it exists to show.
fn buying(detail: &PickaxeDetail) -> String {
    match detail.chain.as_slice() {
        [] | [_] => " Buying the tier jump.".to_owned(),
        [first, _] => format!(" Buying {first}, then the tier jump."),
        rungs => format!(" Buying {} rungs, ending in the tier jump.", rungs.len()),
    }
}

/// `27 ticks`, or `—` for a block this pickaxe cannot break at all.
fn ticks(count: Option<u32>) -> String {
    count.map_or_else(|| crate::view::NOTHING.to_owned(), |n| format!("{n} ticks"))
}

/// `five purchases` — pluralised, since a dip repaid one rung on is the interesting
/// case and `1 purchases` would undercut a sentence meant to reassure.
fn purchases(count: usize) -> String {
    if count == 1 {
        "1 purchase".to_owned()
    } else {
        format!("{count} purchases")
    }
}

/// The two options, with the caret on the focused one.
///
/// The caret moves rather than the options: their order is the frame's, and a list
/// that reordered itself around the focus would make `←`/`→` mean nothing.
fn choices(buy: bool) -> String {
    let (left, right) = if buy { ("▸", " ") } else { (" ", "▸") };
    format!(" {left}  Buy it       {right} n  Not yet")
}

#[cfg(test)]
mod tests {
    use skylode_core::{
        block::Block,
        material::{Item, Material},
    };

    use super::*;
    use crate::view::{DipDetail, Mark, PowerDetail, PriceLine, Repaid};

    /// The §6.7 frame's own chain: a maxed Diamond pickaxe one rung from Netherite.
    fn a_dip() -> PickaxeDetail {
        PickaxeDetail {
            title: "Netherite Pickaxe".to_owned(),
            crosses_tier_jump: true,
            chain: vec![
                "Diamond Efficiency V".to_owned(),
                "Netherite Pickaxe".to_owned(),
            ],
            mark: Mark::Affordable,
            costs: vec![
                PriceLine {
                    item: Item::Compressed(Material::AncientDebris),
                    needed: 4,
                    held: 4,
                    mark: Mark::Affordable,
                },
                PriceLine {
                    item: Item::Raw(Material::AncientDebris),
                    needed: 60,
                    held: 60,
                    mark: Mark::Affordable,
                },
            ],
            power: PowerDetail {
                before: 34.0,
                after: 9.0,
                block: Block::AncientDebris,
                ticks_before: Some(27),
                ticks_after: Some(100),
            },
            dip: Some(DipDetail {
                repaid_at: Some(Repaid {
                    rung: "Netherite Efficiency V".to_owned(),
                    power: 35.0,
                    rungs_later: 5,
                }),
            }),
            unlocks: Vec::new(),
            // A dip is a purchase, and a purchase is what an owned rung has none of.
            owned: None,
            ceiling: Some((5, 15)),
        }
    }

    fn frame_of(detail: &PickaxeDetail, buy: bool) -> String {
        crate::overlay::render_to_string(|frame, area| render(frame, area, detail, buy))
    }

    #[test]
    fn it_states_the_trade_in_ticks_and_offers_the_deal() {
        let frame = frame_of(&a_dip(), false);
        assert!(
            frame.contains("Buying Diamond Efficiency V, then the tier jump."),
            "{frame}"
        );
        assert!(frame.contains("This resets Efficiency V to 0."), "{frame}");
        // The dip in ticks per block, not only in power.
        assert!(
            frame.contains("Mining power      34.0   →   9.0"),
            "{frame}"
        );
        assert!(
            frame.contains("27 ticks  →  100 ticks per block"),
            "{frame}"
        );
        assert!(
            frame.contains("You get it back at Netherite Efficiency V (35.0),"),
            "{frame}"
        );
        assert!(frame.contains("5 purchases later."), "{frame}");
    }

    /// **The caret opens on `Not yet`**, against the §6.7 wireframe and with its prose:
    /// a modal that only appears on the one purchase in the game that costs power must
    /// not put the reflex key on taking it.
    #[test]
    fn the_caret_opens_on_the_safe_option_and_moves_to_the_other() {
        let not_yet = frame_of(&a_dip(), false);
        let row = not_yet
            .lines()
            .find(|line| line.contains("Buy it"))
            .unwrap_or_default();
        assert!(row.contains("▸ n  Not yet"), "{row:?}");

        let buying = frame_of(&a_dip(), true);
        let row = buying
            .lines()
            .find(|line| line.contains("Buy it"))
            .unwrap_or_default();
        assert!(row.contains("▸  Buy it"), "{row:?}");
    }

    /// `1 purchases` would undercut the one sentence in the box meant to reassure.
    #[test]
    fn a_dip_repaid_on_the_very_next_rung_says_purchase_in_the_singular() {
        let detail = PickaxeDetail {
            dip: Some(DipDetail {
                repaid_at: Some(Repaid {
                    rung: "Netherite Eff I".to_owned(),
                    power: 35.0,
                    rungs_later: 1,
                }),
            }),
            ..a_dip()
        };
        let frame = frame_of(&detail, false);
        assert!(frame.contains("1 purchase later."), "{frame}");
    }

    #[test]
    fn a_longer_chain_counts_its_rungs_instead_of_naming_them() {
        // `M` can aim at nine rungs at once, and nine names would bury the two numbers
        // the modal exists to show.
        let detail = PickaxeDetail {
            chain: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            ..a_dip()
        };
        let frame = frame_of(&detail, false);
        assert!(
            frame.contains("Buying 3 rungs, ending in the tier jump."),
            "{frame}"
        );
    }

    #[test]
    fn a_chain_of_one_is_the_jump_itself() {
        let detail = PickaxeDetail {
            chain: vec!["Netherite Pickaxe".to_owned()],
            ..a_dip()
        };
        let frame = frame_of(&detail, false);
        assert!(frame.contains("Buying the tier jump."), "{frame}");
    }

    /// The last rung of the ladder: nothing past it can earn the power back, and the
    /// modal has to say so rather than leave the sentence out.
    #[test]
    fn a_dip_with_nothing_past_it_says_so() {
        let detail = PickaxeDetail {
            dip: Some(DipDetail { repaid_at: None }),
            power: PowerDetail {
                ticks_before: None,
                ticks_after: None,
                ..a_dip().power
            },
            ceiling: None,
            ..a_dip()
        };
        let frame = frame_of(&detail, false);
        assert!(
            frame.contains("There is no rung past it to earn it back."),
            "{frame}"
        );
        // A block this pickaxe cannot break at all reads `—`, never `0 ticks`.
        assert!(frame.contains("—  →  — per block"), "{frame}");
        assert!(!frame.contains("resets Efficiency"), "{frame}");
    }

    /// A detail with no dip draws nothing: the modal is only opened on one, and an
    /// empty box over the screen would be worse than no confirmation at all.
    #[test]
    fn a_pickaxe_with_no_dip_draws_no_box() {
        let detail = PickaxeDetail {
            dip: None,
            ..a_dip()
        };
        assert!(frame_of(&detail, false).trim().is_empty());
    }
}
