//! The disk password screen: the logo, the prompt, a dot per key, and what the
//! last try did. The password itself is never drawn.

use crate::keys::Key;
use crate::logo::Logo;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};
use wros_shell::grimoire_theme::Theme;
use zeroize::Zeroizing;

/// Longer than any disk password a writer types; more is ignored.
pub const MAX_CHARS: usize = 512;

/// Where a key leaves the screen.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Typing,
    /// Enter: the password is ready.
    Done,
    /// Esc: use Debian's text prompt instead.
    Text,
}

pub struct Unlock {
    password: Zeroizing<String>,
    /// Tries made before this one (cryptsetup's `CRYPTTAB_TRIED`).
    pub tried: u32,
    /// Tries allowed (`CRYPTTAB_OPTION_tries`, 3 by default).
    pub tries: u32,
}

impl Unlock {
    pub fn new(tried: u32, tries: u32) -> Self {
        Self {
            // Room for the longest password up front, so it never moves and leaves a copy behind.
            password: Zeroizing::new(String::with_capacity(MAX_CHARS * 4)),
            tried,
            tries,
        }
    }

    /// From cryptsetup's environment: `CRYPTTAB_TRIED` and `CRYPTTAB_OPTION_tries`.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let number = |name: &str| get(name).and_then(|v| v.trim().parse::<u32>().ok());
        Self::new(
            number("CRYPTTAB_TRIED").unwrap_or(0),
            number("CRYPTTAB_OPTION_tries")
                .filter(|&n| n > 0)
                .unwrap_or(3),
        )
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Char(c) if self.password.chars().count() < MAX_CHARS => self.password.push(c),
            Key::Char(_) => {}
            Key::Backspace => {
                self.password.pop();
            }
            Key::Clear => self.password.clear(),
            Key::Enter => return Outcome::Done,
            Key::Esc => return Outcome::Text,
        }
        Outcome::Typing
    }

    /// The typed password; only `main` reads it, to hand it to cryptsetup.
    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn draw(&self, frame: &mut Frame, theme: &Theme) {
        let area = frame.area();
        let text = Style::default().fg(theme.text);
        let dim = Style::default().fg(theme.dim);
        let lines = vec![
            Line::styled(
                "Type your disk password, then press Enter",
                text.add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                dots(self.password.chars().count(), area.width),
                text.fg(theme.accent),
            ),
            Line::styled(self.message(), Style::default().fg(theme.warn)),
            Line::from(""),
            Line::styled("Esc: plain text prompt", dim),
        ];
        let logo_height = Logo::height_for(Rect {
            height: area.height.saturating_sub(lines.len() as u16 + 1),
            ..area
        });
        let used = logo_height + 1 + lines.len() as u16;
        let [_, logo, _, prompt, _] = Layout::vertical([
            Constraint::Length(area.height.saturating_sub(used) / 2),
            Constraint::Length(logo_height),
            Constraint::Length(1),
            Constraint::Length(lines.len() as u16),
            Constraint::Min(0),
        ])
        .areas(area);
        frame.render_widget(Logo { theme }, logo);
        frame.render_widget(Paragraph::new(lines).centered(), prompt);
    }

    fn message(&self) -> String {
        if self.tried == 0 {
            return String::new();
        }
        format!(
            "That password didn't unlock the disk. Try {} of {}.",
            (self.tried + 1).min(self.tries.max(1)),
            self.tries.max(1)
        )
    }
}

/// One dot per character, as many as fit on a line.
fn dots(count: usize, width: u16) -> String {
    let room = usize::from(width.saturating_sub(4)) / 2;
    "* ".repeat(count.min(room)).trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Decoder;
    use ratatui::{Terminal, backend::TestBackend};
    use wros_shell::grimoire_theme::default_theme;

    fn screen(unlock: &Unlock, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let theme = default_theme();
        terminal.draw(|f| unlock.draw(f, &theme)).unwrap();
        let buf = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn typed(unlock: &mut Unlock, bytes: &[u8]) -> Outcome {
        let mut decoder = Decoder::default();
        let mut last = Outcome::Typing;
        for &b in bytes {
            if let Some(key) = decoder.feed(b) {
                last = unlock.key(key);
            }
        }
        last
    }

    #[test]
    fn typing_editing_and_enter_give_the_exact_password() {
        let mut unlock = Unlock::new(0, 3);
        assert_eq!(
            typed(&mut unlock, "Wrong ünïcode".as_bytes()),
            Outcome::Typing
        );
        typed(&mut unlock, b"\x15");
        assert_eq!(unlock.password(), "");
        typed(&mut unlock, "pässwortX\x7f".as_bytes());
        assert_eq!(typed(&mut unlock, b"\x1b[D\r"), Outcome::Done);
        assert_eq!(unlock.password(), "pässwort");
    }

    #[test]
    fn esc_asks_for_the_text_prompt() {
        let mut unlock = Unlock::new(0, 3);
        typed(&mut unlock, b"abc");
        let mut decoder = Decoder::default();
        decoder.feed(0x1b);
        assert_eq!(unlock.key(decoder.timeout().unwrap()), Outcome::Text);
    }

    #[test]
    fn the_password_is_never_drawn_at_any_console_size() {
        let mut unlock = Unlock::new(0, 3);
        for c in "Secret-Colemak-ärst".chars() {
            unlock.key(Key::Char(c));
            for (w, h) in [(80, 25), (100, 37), (128, 48), (170, 48), (40, 12)] {
                let shown = screen(&unlock, w, h);
                assert!(
                    !shown.contains("Secret") && !shown.contains("ärst"),
                    "{shown}"
                );
            }
        }
        assert!(screen(&unlock, 170, 48).contains("* * *"));
    }

    #[test]
    fn a_wrong_try_says_so_and_counts() {
        let unlock = Unlock::from_env(|name| match name {
            "CRYPTTAB_TRIED" => Some("1".into()),
            _ => None,
        });
        assert!(
            screen(&unlock, 100, 37).contains("That password didn't unlock the disk. Try 2 of 3.")
        );
        assert!(!screen(&Unlock::new(0, 3), 100, 37).contains("didn't unlock"));
        let odd = Unlock::from_env(|_| Some("junk".into()));
        assert_eq!((odd.tried, odd.tries), (0, 3));
    }

    #[test]
    fn the_screen_at_console_sizes() {
        let unlock = Unlock::new(1, 3);
        let mut typing = unlock;
        for c in "four".chars() {
            typing.key(Key::Char(c));
        }
        insta::assert_snapshot!("unlock_170x48", screen(&typing, 170, 48));
        insta::assert_snapshot!("unlock_80x25", screen(&typing, 80, 25));
    }

    #[test]
    fn a_long_password_is_capped_and_its_dots_fit() {
        let mut unlock = Unlock::new(0, 3);
        for _ in 0..(MAX_CHARS + 50) {
            unlock.key(Key::Char('x'));
        }
        assert_eq!(unlock.password().chars().count(), MAX_CHARS);
        assert!(
            screen(&unlock, 80, 25)
                .lines()
                .all(|l| l.chars().count() <= 80)
        );
    }
}
