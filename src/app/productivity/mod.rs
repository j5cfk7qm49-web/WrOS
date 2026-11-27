//! Real operation accounting with ordinary Markdown still the only prose.
//! Draw reads cached state. No timer infers writing from an open window.

mod accounting;
mod draw;
mod input;
mod rows;
mod settings;
mod state;

pub use accounting::source_editor;
pub(super) use draw::{calendar_grid, draw};
pub use state::{Input, Page, Productivity};
