//! The picture shown as the graphical writing session opens.

use ratatui::{
    Frame,
    buffer::Buffer,
    crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind},
    layout::Rect,
    widgets::Widget,
};
use ratatui_splash_screen::{SplashConfig, SplashScreen};
use std::time::{Duration, Instant};

const SIZE: (u16, u16) = wros_shell::logo::FULL;
/// The picture fades in, then holds; a key, paste or click skips it at any point.
const FADE: Duration = Duration::from_millis(500);
const SHOW_FOR: Duration = Duration::from_millis(1500);
/// The picture's own fade: `ratatui-splash-screen` draws it darkened and blurred for the first
/// half of its steps, less so each time it is drawn, and sharp from the middle step on.
const STEPS: i32 = 14;
/// Draws from the first, darkest, to the first sharp one.
const FRAMES: u32 = STEPS as u32 / 2;

pub struct Splash {
    picture: SplashScreen,
    start: Option<Instant>,
    /// How many of the fade's steps have been drawn: each draw of the widget takes one more, so
    /// the fade is paced here by the clock, not by how often the screen happens to be drawn.
    rendered: u32,
    /// The latest step's cells, copied into every frame until the next step is due.
    cells: Buffer,
}

impl Splash {
    pub fn new(graphical: bool, low_power: bool) -> Option<Self> {
        if !graphical || low_power {
            return None;
        }
        let picture = SplashScreen::new(SplashConfig {
            image_data: include_bytes!("../assets/wros-startup.png"),
            sha256sum: None,
            render_steps: STEPS,
            use_colors: true,
        })
        .ok()?;
        let (width, height) = SIZE;
        Some(Self {
            picture,
            start: None,
            rendered: 0,
            cells: Buffer::empty(Rect::new(0, 0, width, height)),
        })
    }

    /// When the fade's `step`th draw (counting from 0) is due, from the start.
    fn due(step: u32) -> Duration {
        let nanos = FADE.as_nanos() * u128::from(step) / u128::from(FRAMES - 1);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    pub fn draw(&mut self, frame: &mut Frame, now: Instant) -> Option<Duration> {
        let area = frame.area();
        let (width, height) = SIZE;
        if area.width < width || area.height < height {
            return None;
        }
        let elapsed = now.saturating_duration_since(*self.start.get_or_insert(now));
        if elapsed >= SHOW_FOR {
            return None;
        }
        // Every step that is due by now, so a late frame catches up instead of running behind.
        while self.rendered < FRAMES && Self::due(self.rendered) <= elapsed {
            let mut cells = Buffer::empty(self.cells.area);
            (&mut self.picture).render(cells.area, &mut cells);
            self.cells = cells;
            self.rendered += 1;
        }
        let left = area.x + (area.width - width) / 2;
        let top = area.y + (area.height - height) / 2;
        let buffer = frame.buffer_mut();
        for y in 0..height {
            for x in 0..width {
                buffer[(left + x, top + y)] = self.cells[(x, y)].clone();
            }
        }
        Some(if self.rendered < FRAMES {
            Self::due(self.rendered).saturating_sub(elapsed)
        } else {
            SHOW_FOR - elapsed
        })
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
        let start = Instant::now();
        terminal
            .draw(|frame| assert!(splash.draw(frame, start).is_some()))
            .unwrap();
        // The sharp picture, once the fade is over.
        let frame = terminal
            .draw(|frame| assert_eq!(splash.draw(frame, start + FADE), Some(SHOW_FOR - FADE)))
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
            .draw(|frame| assert_eq!(splash.draw(frame, start + SHOW_FOR), None))
            .unwrap();
    }

    /// The picture's total brightness: the sum of every Braille cell's colour channels.
    fn brightness(buffer: &ratatui::buffer::Buffer) -> u32 {
        buffer
            .content
            .iter()
            .filter_map(|cell| match cell.fg {
                ratatui::style::Color::Rgb(r, g, b) => {
                    Some(u32::from(r) + u32::from(g) + u32::from(b))
                }
                _ => None,
            })
            .sum()
    }

    fn frame_at(splash: &mut Splash, at: Instant) -> (ratatui::buffer::Buffer, Option<Duration>) {
        let (width, height) = SIZE;
        let mut terminal = Terminal::new(TestBackend::new(width + 4, height + 4)).unwrap();
        let mut wait = None;
        let frame = terminal
            .draw(|frame| wait = splash.draw(frame, at))
            .unwrap();
        (frame.buffer.clone(), wait)
    }

    #[test]
    fn the_picture_fades_in_over_half_a_second_then_holds() {
        let mut splash = Splash::new(true, false).unwrap();
        let start = Instant::now();
        let (first, wait) = frame_at(&mut splash, start);
        // While fading, the loop is asked back for the next step, not for the whole showing.
        assert!(wait.unwrap() <= FADE / 4, "{wait:?}");
        let (middle, _) = frame_at(&mut splash, start + FADE / 2);
        let (sharp, wait) = frame_at(&mut splash, start + FADE);
        assert!(
            brightness(&first) < brightness(&middle),
            "the fade starts dark"
        );
        assert!(brightness(&middle) < brightness(&sharp), "and brightens");
        assert_eq!(wait, Some(SHOW_FOR - FADE), "then the sharp picture holds");
        let (held, _) = frame_at(&mut splash, start + FADE + Duration::from_millis(300));
        assert_eq!(held, sharp);
        assert_eq!(frame_at(&mut splash, start + SHOW_FOR).1, None);
    }

    #[test]
    fn the_fade_keeps_time_however_often_the_screen_is_drawn() {
        // Redraws for other reasons (a resize, a key the loop ignores) must not hurry the fade.
        // (Steps are compared, not cells: the library colours a cell from an unordered map.)
        let start = Instant::now();
        let mut redrawn = Splash::new(true, false).unwrap();
        for _ in 0..20 {
            frame_at(&mut redrawn, start);
        }
        assert_eq!(redrawn.rendered, 1);
        frame_at(&mut redrawn, start + FADE / 2);
        assert_eq!(redrawn.rendered, 4);
        // A late frame catches up instead of running behind.
        let mut late = Splash::new(true, false).unwrap();
        frame_at(&mut late, start);
        frame_at(&mut late, start + FADE);
        assert_eq!(late.rendered, FRAMES);
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
