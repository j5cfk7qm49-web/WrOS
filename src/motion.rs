//! Short, colour-only transitions for the writing session.

use crate::theme::Theme;
use ratatui::{buffer::Buffer, style::Color};
use std::time::{Duration, Instant};
use tachyonfx::{CellFilter, Effect, Interpolation, fx};

const WELCOME: Duration = Duration::from_millis(210);
const HIGHLIGHT: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    Row(usize),
    Lesson(u8),
}

impl Highlight {
    pub fn from_overlay(overlay: &crate::app::Overlay) -> Option<Self> {
        use crate::app::{Overlay, wros::Page};
        match overlay {
            Overlay::Deck(Page::Tutorial { sel }) => Some(Self::Row(*sel)),
            Overlay::Deck(Page::Lesson { id }) => Some(Self::Lesson(*id)),
            _ => None,
        }
    }
}

struct Fade {
    effect: Effect,
    remaining: Duration,
    previous: Option<Instant>,
}

impl Fade {
    fn new(from: Color, length: Duration, filter: CellFilter) -> Self {
        Self {
            effect: fx::fade_from_fg(from, (length, Interpolation::SineOut)).with_filter(filter),
            remaining: length,
            previous: None,
        }
    }

    fn paint(&mut self, buffer: &mut Buffer, now: Instant) -> bool {
        let step = self
            .previous
            .replace(now)
            .map_or(Duration::ZERO, |previous| {
                now.saturating_duration_since(previous)
            });
        self.remaining = self.remaining.saturating_sub(step);
        if self.remaining.is_zero() {
            return false;
        }
        self.effect.process(step, buffer, buffer.area);
        true
    }
}

#[derive(Default)]
pub struct Motion {
    started: bool,
    highlight: Option<Highlight>,
    fade: Option<Fade>,
}

impl Motion {
    pub fn paint(
        &mut self,
        buffer: &mut Buffer,
        graphical: bool,
        low_power: bool,
        highlight: Option<Highlight>,
        theme: &Theme,
        now: Instant,
    ) -> bool {
        let welcome = !std::mem::replace(&mut self.started, true);
        let changed = std::mem::replace(&mut self.highlight, highlight) != highlight;
        if !graphical || low_power {
            self.fade = None;
            return false;
        }
        if changed {
            self.fade = highlight
                .map(|_| Fade::new(theme.text, HIGHLIGHT, CellFilter::FgColor(theme.accent)));
        } else if welcome {
            self.fade = Some(Fade::new(theme.dim, WELCOME, CellFilter::Text));
        }
        self.fade.take_if(|fade| !fade.paint(buffer, now));
        self.fade.is_some()
    }
}

/// Only called after writing input has been retired and before vlock starts.
#[cfg(target_os = "linux")]
pub fn before_lock(terminal: &mut crate::display::Terminal, theme: &Theme) {
    use ratatui::{
        style::Style,
        widgets::{Clear, Paragraph},
    };
    let mut fade = Fade::new(theme.dim, HIGHLIGHT, CellFilter::Text);
    loop {
        let now = Instant::now();
        let mut animating = false;
        let drawn = terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            let row = ratatui::layout::Rect {
                y: area.y + area.height / 2,
                height: area.height.min(1),
                ..area
            };
            frame.render_widget(
                Paragraph::new("Locking…")
                    .centered()
                    .style(Style::default().fg(theme.text)),
                row,
            );
            animating = fade.paint(frame.buffer_mut(), now);
        });
        if drawn.is_err() || !animating {
            break;
        }
        std::thread::sleep(crate::FAST_TICK.saturating_sub(now.elapsed()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{layout::Rect, style::Style};

    fn writing(theme: &Theme) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 16, 2));
        buffer.set_string(0, 0, "Scene", Style::default().fg(theme.text));
        buffer.set_string(0, 1, "Lesson", Style::default().fg(theme.accent));
        buffer
    }

    #[test]
    fn the_graphical_welcome_fades_colours_and_leaves_every_glyph_in_place() {
        let theme = crate::theme::default_theme();
        let original = writing(&theme);
        let mut motion = Motion::default();
        let now = Instant::now();
        let mut first = original.clone();
        assert!(motion.paint(&mut first, true, false, None, &theme, now));
        assert_ne!(first[(0, 0)].fg, original[(0, 0)].fg);
        assert!(
            first
                .content
                .iter()
                .zip(&original.content)
                .all(|(a, b)| a.symbol() == b.symbol() && a.bg == b.bg)
        );

        let mut halfway = original.clone();
        assert!(motion.paint(
            &mut halfway,
            true,
            false,
            None,
            &theme,
            now + Duration::from_millis(80)
        ));
        assert_ne!(halfway[(0, 0)].fg, first[(0, 0)].fg);
        assert_ne!(halfway[(0, 0)].fg, original[(0, 0)].fg);

        let mut finished = original.clone();
        assert!(!motion.paint(
            &mut finished,
            true,
            false,
            None,
            &theme,
            now + Duration::from_millis(500)
        ));
        assert_eq!(finished, original);
    }

    #[test]
    fn console_and_low_power_frames_are_static_and_low_power_cancels_an_active_fade() {
        let theme = crate::theme::default_theme();
        let original = writing(&theme);
        let now = Instant::now();
        for (graphical, low_power) in [(false, false), (true, true)] {
            let mut buffer = original.clone();
            assert!(!Motion::default().paint(
                &mut buffer,
                graphical,
                low_power,
                Some(Highlight::Row(0)),
                &theme,
                now
            ));
            assert_eq!(buffer, original);
        }
        let mut motion = Motion::default();
        assert!(motion.paint(&mut original.clone(), true, false, None, &theme, now));
        let mut buffer = original.clone();
        assert!(!motion.paint(&mut buffer, true, true, None, &theme, now));
        assert!(!motion.paint(&mut buffer, true, false, None, &theme, now));
        assert_eq!(buffer, original);
    }

    #[test]
    fn moving_the_tutorial_highlight_fades_accent_cells_and_leaving_cancels_it() {
        let theme = crate::theme::default_theme();
        let original = writing(&theme);
        let mut motion = Motion::default();
        let now = Instant::now();
        for selection in [Highlight::Row(0), Highlight::Row(1), Highlight::Lesson(2)] {
            let mut buffer = original.clone();
            assert!(motion.paint(&mut buffer, true, false, Some(selection), &theme, now));
            assert_eq!(buffer[(0, 0)], original[(0, 0)]);
            assert_eq!(buffer[(0, 1)].symbol(), original[(0, 1)].symbol());
            assert_ne!(buffer[(0, 1)].fg, original[(0, 1)].fg);
        }
        let mut buffer = original.clone();
        assert!(!motion.paint(&mut buffer, true, false, None, &theme, now));
        assert_eq!(buffer, original);
    }
}
