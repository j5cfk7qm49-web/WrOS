//! The parts every WrOS box is drawn with: the editor's panes and menus,
//! Settings, login and the installer. One border, one bold title, one shaded
//! highlight row with `▸`, one way of lighting the keys in a hint.
use crate::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

/// The marker on the highlighted row of every list.
pub const MARKER: &str = "▸ ";

/// A key in a hint: accent and bold.
pub fn key_style(t: Theme) -> Style {
    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
}

/// The highlighted row of a list or menu: shaded, in the accent.
pub fn highlight(t: Theme) -> Style {
    Style::default().fg(t.accent).bg(t.sel)
}

/// A pane or box: the theme's border, accent when it has focus, and a bold title.
pub fn pane_block(title: &str, focused: bool, t: Theme) -> Block<'_> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { t.accent } else { t.border }));
    if title.is_empty() {
        return block;
    }
    block.title(Span::styled(
        format!(" {title} "),
        Style::default()
            .fg(if focused { t.accent } else { t.dim })
            .add_modifier(Modifier::BOLD),
    ))
}

/// A hint line like " y delete   esc keep it", its keys lit and the rest dim.
pub fn hint_spans(text: &str, t: Theme) -> Vec<Span<'static>> {
    let dim = Style::default().fg(t.dim);
    hint_parts(text)
        .into_iter()
        .map(|(part, key)| Span::styled(part, if key { key_style(t) } else { dim }))
        .collect()
}

/// Split a hint into (text, is it a key). Hints are separated by two or more
/// spaces or " · ", and each starts with its key — "↵ choose", "PgUp PgDn
/// scroll", "any other key cancels" — unless it's typing ("type a name"),
/// which has none.
pub fn hint_parts(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let gap = if rest.starts_with(" · ") {
            " · ".len()
        } else {
            rest.len() - rest.trim_start_matches(' ').len()
        };
        if gap > 0 {
            out.push((rest[..gap].to_string(), false));
            rest = &rest[gap..];
            continue;
        }
        let end = [rest.find("  "), rest.find(" · ")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(rest.len());
        let hint = &rest[..end];
        let key = if hint == "type" || hint.starts_with("type ") {
            0
        } else {
            ["any other key", "any key", "PgUp PgDn", "[ ]"]
                .into_iter()
                .find(|k| hint.starts_with(k))
                .map_or_else(|| hint.find(' ').unwrap_or(hint.len()), str::len)
        };
        if key > 0 {
            out.push((hint[..key].to_string(), true));
        }
        if key < hint.len() {
            out.push((hint[key..].to_string(), false));
        }
        rest = &rest[end..];
    }
    out
}

/// A menu-bar title, " File ", with the letter at `letter` (a char index)
/// underlined; lit when its menu is the one chosen.
pub fn underlined_title(label: &str, letter: usize, lit: bool, t: Theme) -> Vec<Span<'static>> {
    let base = if lit {
        highlight(t).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t.text)
    };
    let mut spans = vec![Span::styled(" ", base)];
    for (i, c) in label.chars().enumerate() {
        let style = if i == letter {
            base.add_modifier(Modifier::UNDERLINED)
        } else {
            base
        };
        spans.push(Span::styled(c.to_string(), style));
    }
    spans.push(Span::styled(" ", base));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn theme() -> Theme {
        Theme {
            text: Color::White,
            accent: Color::Yellow,
            dim: Color::Gray,
            border: Color::Blue,
            sel: Color::DarkGray,
            warn: Color::Red,
        }
    }

    #[test]
    fn hint_spans_light_only_the_keys() {
        let keys = |text: &str| -> Vec<String> {
            let parts = hint_parts(text);
            assert_eq!(
                parts.iter().map(|(p, _)| p.as_str()).collect::<String>(),
                text
            );
            parts
                .into_iter()
                .filter(|(_, k)| *k)
                .map(|(p, _)| p)
                .collect()
        };
        assert_eq!(keys(" y delete   esc keep it"), ["y", "esc"]);
        assert_eq!(keys(" type a name   ↵ create   esc cancel"), ["↵", "esc"]);
        assert_eq!(
            keys("↑↓ pick a version   PgUp PgDn scroll   ↵ restore it"),
            ["↑↓", "PgUp PgDn", "↵"]
        );
        assert_eq!(
            keys("↑↓ scroll   any other key: back to sessions"),
            ["↑↓", "any other key"]
        );
        assert_eq!(keys(" type hex · ↵ next · esc save & close"), ["↵", "esc"]);
        assert_eq!(
            keys(" space pause  ←→ seek 10s  [ ] prev/next  +/- volume"),
            ["space", "←→", "[ ]", "+/-"]
        );
        assert_eq!(
            keys("   ↵ next  ↑ previous  Tab replace  ^F whole book"),
            ["↵", "↑", "Tab", "^F"]
        );
        let spans = hint_spans(" y delete", theme());
        assert_eq!(spans[1].style, key_style(theme()));
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn a_titled_pane_has_a_bold_title_and_lights_its_border_with_focus() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        pane_block("Settings", true, theme()).render(buf.area, &mut buf);
        assert_eq!(buf[(2, 0)].symbol(), "S");
        assert!(buf[(2, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(0, 1)].fg, Color::Yellow);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        pane_block("Settings", false, theme()).render(buf.area, &mut buf);
        assert_eq!(buf[(0, 1)].fg, Color::Blue);
    }

    #[test]
    fn a_menu_title_underlines_its_letter() {
        let spans = underlined_title("File", 0, true, theme());
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, " File ");
        assert!(spans[1].style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(!spans[2].style.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(spans[1].style.bg, Some(Color::DarkGray));
        assert_eq!(
            underlined_title("File", 0, false, theme())[1].style.bg,
            None
        );
    }
}
