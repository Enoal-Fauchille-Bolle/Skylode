//! Custom widgets that paint the buffer directly.
//!
//! Empty for now. The one this module exists for is the **mine grid**
//! (UI-EN.md §7): a fixed 42-column panel of coloured two-character cells, which
//! is *not* a `Canvas` — `Canvas` plots braille dot shapes, and the grid is
//! coloured `##` cells on a character lattice. It is a short `Widget`
//! implementation writing into the [`Buffer`] directly, which is the right shape
//! for it: the core owns the geometry, the TUI only paints what it is told.
//!
//! Anything reusable across screens that ratatui does not already ship belongs
//! here; anything ratatui *does* ship (`List`, `Table`, `Tabs`, `LineGauge`)
//! should be used as-is rather than re-implemented.
//!
//! [`Buffer`]: ratatui::buffer::Buffer
