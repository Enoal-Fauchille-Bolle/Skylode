//! The Mine screen — active mining (UI.md §5.1).
//!
//! **Phase 1, not phase 2.** What is here is the grid and nothing else: the real
//! screen puts it in a fixed 42-column panel with a `Haul` strip above, Pickaxe
//! and Mine panels beside it and three `LineGauge` rows below, and that layout is
//! the next phase's work. Drawing the grid now is what makes a badly chosen
//! swatch visible — which is not something a `Buffer` assertion can tell you.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::Rect,
    widgets::{Block, BorderType, Borders},
};

use crate::{action::Action, view::View, widget::MineGrid};

/// Draws the grid, centred in a panel titled with the mine's name.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let block = Block::default()
        .title(format!(" {} ", view.mine_name))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    // `inner` before `render_widget`, because rendering consumes the block: the
    // grid has to be told the area *inside* the border or it would paint over it.
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut grid = MineGrid::new(view.mine_kind, &view.grid).mode(view.colour_mode);
    // Chained conditionally rather than passed as an `Option`: the widget models
    // "nothing is being dug" as the absence of the call, so that a break ratio
    // cannot exist without a cell for it to be about.
    if let Some(cell) = view.target {
        grid = grid.target(cell, view.break_ratio);
    }
    frame.render_widget(grid, inner);
}

/// No contextual bindings yet; `Space` (mine) arrives with the tick in phase 7.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
