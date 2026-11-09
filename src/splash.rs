//! The picture shown as the graphical writing session opens.

use ratatui::{
    Frame,
    crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind},
    layout::Rect,
};
use ratatui_splash_screen::{SplashConfig, SplashScreen};
use std::time::{Duration, Instant};

const SIZE: (u16, u16) = wros_shell::logo::FULL;
const SHOW_FOR: Duration = Duration::from_millis(180);

pub struct Splash {
    picture: SplashScreen,
    until: Option<Instant>,
}

impl Splash {
    pub fn new(graphical: bool, low_power: bool) -> Option<Self> {
        if !graphical || low_power {
            return None;
        }
        let picture = SplashScreen::new(SplashConfig {
            image_data: include_bytes!("../assets/wros-startup.png"),
            sha256sum: None,
            render_steps: 2, // A sharp first frame, then reuse the cached colours.
            use_colors: true,
        })
        .ok()?;
        Some(Self {
            picture,
            until: None,
        })
    }

    pub fn draw(&mut self, frame: &mut Frame, now: Instant) -> Option<Duration> {
        let area = frame.area();
        let (width, height) = SIZE;
        if area.width < width || area.height < height {
            return None;
        }
        let remaining = self
            .until
            .get_or_insert(now + SHOW_FOR)
            .saturating_duration_since(now);
        if remaining.is_zero() {
            return None;
        }
        let picture = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(&mut self.picture, picture);
        Some(remaining)
    }
}

pub fn dismisses(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        }) | Event::Paste(_)
            | Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(_),
                ..
            })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        crossterm::event::{KeyCode, KeyModifiers, MouseButton},
    };

    #[test]
    fn the_graphical_picture_fits_and_finishes_on_time() {
        let mut splash = Splash::new(true, false).expect("graphical picture");
        let (width, height) = SIZE;
        let mut terminal = Terminal::new(TestBackend::new(width + 4, height + 4)).unwrap();
        let now = Instant::now();
        let frame = terminal
            .draw(|frame| assert_eq!(splash.draw(frame, now), Some(SHOW_FOR)))
            .unwrap();
        assert_eq!(frame.buffer[(0, 0)].symbol(), " ");
        let border = &frame.buffer[(2, 2)];
        let letter = &frame.buffer[(4, 3)];
        let paper = &frame.buffer[(3, 3)];
        for cell in [border, letter, paper] {
            assert!(matches!(
                cell.symbol().chars().next(),
                Some('\u{2801}'..='\u{28ff}')
            ));
        }
        assert_ne!(border.fg, paper.fg);
        assert_ne!(letter.fg, paper.fg);
        terminal
            .draw(|frame| assert_eq!(splash.draw(frame, now + SHOW_FOR), None))
            .unwrap();
    }

    #[test]
    fn the_picture_yields_to_low_power_console_and_small_graphical_screens() {
        assert!(Splash::new(false, false).is_none());
        assert!(Splash::new(true, true).is_none());
        let mut splash = Splash::new(true, false).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(SIZE.0 - 1, SIZE.1)).unwrap();
        let frame = terminal
            .draw(|frame| assert_eq!(splash.draw(frame, Instant::now()), None))
            .unwrap();
        assert!(frame.buffer.content.iter().all(|cell| cell.symbol() == " "));
    }

    #[test]
    fn a_writer_can_skip_the_picture_without_waiting_for_key_release() {
        assert!(dismisses(&Event::Key(KeyEvent::from(KeyCode::Char('a')))));
        assert!(dismisses(&Event::Paste("Early text".into())));
        assert!(dismisses(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })));
        assert!(!dismisses(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))));
        assert!(!dismisses(&Event::Resize(90, 30)));
    }
}
