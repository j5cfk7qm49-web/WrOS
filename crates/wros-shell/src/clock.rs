//! A clock supplied by the caller. This module never reads time or input.

use crate::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Paragraph, Widget},
};
use tui_big_text::{BigText, PixelSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    hour: u8,
    minute: u8,
}

impl Time {
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then_some(Self { hour, minute })
    }

    pub fn label(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

pub struct Clock {
    pub time: Time,
    pub theme: Theme,
}

impl Clock {
    pub const WIDTH: u16 = 40;
    pub const HEIGHT: u16 = 9;
}

impl Widget for Clock {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let label = self.time.label();
        Paragraph::new(label.clone())
            .centered()
            .style(Style::default().fg(self.theme.dim))
            .render(
                Rect {
                    height: area.height.min(1),
                    ..area
                },
                buf,
            );
        // The installed console font has full blocks, but not the half blocks
        // used by compressed pixel modes (#105). Keep the label on small areas.
        if area.width >= Self::WIDTH && area.height >= Self::HEIGHT {
            BigText::builder()
                .pixel_size(PixelSize::Full)
                .style(Style::default().fg(self.theme.accent))
                .lines(vec![label.into()])
                .centered()
                .build()
                .render(
                    Rect::new(area.x, area.y + 1, area.width, Self::HEIGHT - 1),
                    buf,
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_has_a_readable_label_and_large_console_safe_digits() {
        let area = Rect::new(0, 0, 40, 9);
        let mut buf = Buffer::empty(area);
        Clock {
            time: Time::new(23, 45).unwrap(),
            theme: Theme::default(),
        }
        .render(area, &mut buf);
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("23:45"));
        assert!(
            text.contains(['▀', '▄', '█']),
            "the large digits must be drawn"
        );
        assert!(
            buf.content
                .iter()
                .all(|c| { c.symbol().chars().all(|c| c.is_ascii() || c == '█') })
        );
    }

    #[test]
    fn small_and_offset_clocks_never_write_outside_their_area() {
        for width in 0..=44 {
            for height in 0..=11 {
                let area = Rect::new(3, 2, width, height);
                let mut buf =
                    Buffer::filled(Rect::new(0, 0, 50, 16), ratatui::buffer::Cell::new("."));
                Clock {
                    time: Time::new(0, 7).unwrap(),
                    theme: Theme::default(),
                }
                .render(area, &mut buf);
                for y in 0..16 {
                    for x in 0..50 {
                        if !area.contains((x, y).into()) {
                            assert_eq!(buf[(x, y)].symbol(), ".");
                        }
                    }
                }
                if width >= 5 && height > 0 {
                    let text: String = buf.content.iter().map(|c| c.symbol()).collect();
                    assert!(text.contains("00:07"));
                }
            }
        }
    }

    #[test]
    fn invalid_times_cannot_reach_the_display() {
        assert!(Time::new(24, 0).is_none());
        assert!(Time::new(23, 60).is_none());
        assert_eq!(Time::new(0, 0).unwrap().label(), "00:00");
    }
}
