//! Preview of the graphical session's font: a mock writing screen and WrOS's
//! one-cell drawings beside Unifont's own. Writes a binary PPM to stdout.
//! `cargo run -p wros-graphics --example unifont_preview > preview.ppm`
use ratatui::{
    Terminal,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io::{self, Write};
use wros_graphics::TextSize;

const BG: Color = Color::Rgb(26, 27, 38);
const TEXT: Color = Color::Rgb(192, 202, 245);
const DIM: Color = Color::Rgb(86, 95, 137);
const ACCENT: Color = Color::Rgb(187, 154, 247);
const WARN: Color = Color::Rgb(247, 118, 142);
const SEL: Color = Color::Rgb(41, 46, 66);

const NARROW: &str = "⌖⏯ⓘ★⚠✎✦✧✱✲✶✺✿❀❚❡❦❧";

fn main() -> io::Result<()> {
    let (cols, rows) = (100u16, 34u16);
    let mut terminal = Terminal::new(TextSize::Writing.raster(cols, rows)).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            f.render_widget(Block::default().style(Style::default().bg(BG)), area);
            let [bar, main, glyphs, bottom] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(9),
                Constraint::Length(1),
            ])
            .areas(area);

            // Menu bar: titles with their letter underlined, File lit.
            let mut spans = vec![Span::raw(" ")];
            for (i, title) in [
                "WrOS", "File", "Edit", "View", "Book", "Tools", "Settings", "Help",
            ]
            .iter()
            .enumerate()
            {
                let base = if i == 1 {
                    Style::default()
                        .fg(ACCENT)
                        .bg(SEL)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                spans.push(Span::styled(" ", base));
                spans.push(Span::styled(
                    &title[..1],
                    base.add_modifier(Modifier::UNDERLINED),
                ));
                spans.push(Span::styled(&title[1..], base));
                spans.push(Span::styled(" ", base));
            }
            let right = "✓ saved  ⇅ 14:02 ";
            let used: usize = spans.iter().map(|s| s.width()).sum();
            spans.push(Span::raw(
                " ".repeat(bar.width as usize - used - right.chars().count()),
            ));
            spans.push(Span::styled(right, Style::default().fg(DIM)));
            f.render_widget(Paragraph::new(Line::from(spans)), bar);

            let [outline, page] =
                Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(main);
            let tree: Vec<Line> = [
                ("❡ Manuscript", TEXT, false),
                ("  ▾ Part One", TEXT, false),
                ("    ▸ Chapter 1", ACCENT, true),
                ("      The lighthouse", TEXT, false),
                ("      Storm warning", TEXT, false),
                ("✎ Drafts", TEXT, false),
                ("❧ Research", TEXT, false),
                ("ⓘ Notes", TEXT, false),
                ("⌖ Places", TEXT, false),
            ]
            .into_iter()
            .map(|(t, c, sel)| {
                let s = Style::default().fg(c);
                Line::styled(t, if sel { s.bg(SEL) } else { s })
            })
            .collect();
            f.render_widget(
                Paragraph::new(tree).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(DIM))
                        .title(Span::styled(
                            " Outline ",
                            Style::default().fg(DIM).add_modifier(Modifier::BOLD),
                        )),
                ),
                outline,
            );
            let prose = vec![
                Line::from("The lamp had not been lit in eleven years, but Mara climbed the"),
                Line::from(vec![
                    Span::raw("stairs anyway, counting them aloud. "),
                    Span::styled(
                        "One hundred and twelve,",
                        Style::default().add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(" her"),
                ]),
                Line::from(vec![
                    Span::raw("father had said, "),
                    Span::styled(
                        "and not one more.",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Café, naïve, déjà vu — “quotes”."),
                ]),
                Line::from(""),
                Line::from("中文 日本語 한국어 · Ελληνικά · Русский · עברית · 0123456789"),
                Line::from("┌─┬─┐ ╔═╗ ░▒▓█ ▂▄▆█ ⣿⠿⠛⠉ ← ↑ → ↓ • … — ‹ ›"),
            ];
            f.render_widget(
                Paragraph::new(prose)
                    .style(Style::default().fg(TEXT))
                    .wrap(Wrap { trim: false })
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(ACCENT))
                            .title(Span::styled(
                                " Chapter 1 › The lighthouse ",
                                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                            )),
                    ),
                page,
            );
            // Dropdown over the page.
            let menu = Rect::new(7, 1, 30, 7);
            f.render_widget(ratatui::widgets::Clear, menu);
            let rows = vec![
                Line::styled(
                    "▸ Start a new book…        ",
                    Style::default().fg(ACCENT).bg(SEL),
                ),
                Line::styled("  Open another book…       ", Style::default().fg(TEXT)),
                Line::styled("  Export…                  ", Style::default().fg(TEXT)),
                Line::styled("  Back up now              ", Style::default().fg(TEXT)),
                Line::styled("  Sleep  (not here)        ", Style::default().fg(DIM)),
            ];
            f.render_widget(
                Paragraph::new(rows).style(Style::default().bg(BG)).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ACCENT)),
                ),
                menu,
            );

            // WrOS's one-cell drawings, beside Unifont's own two-cell ones.
            let key = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
            let mut lines = vec![Line::from(vec![
                Span::styled(" WrOS one cell:  ", Style::default().fg(DIM)),
                Span::styled(
                    NARROW.chars().map(|c| format!("{c} ")).collect::<String>(),
                    Style::default().fg(TEXT),
                ),
            ])];
            lines.push(Line::from(vec![
                Span::styled(" Scaled evenly:  ", Style::default().fg(DIM)),
                Span::raw("☆ ✔ ✚ ❖ ☎ ☂ ♛ ⚑ ⚙ ➜ ⬆ ⬟ ✈ ✉ (typed by a writer; no WrOS drawing)"),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(" Hints: ", Style::default().fg(DIM)),
                Span::styled("↵", key),
                Span::styled(" choose  ", Style::default().fg(DIM)),
                Span::styled("esc", key),
                Span::styled(" close  ", Style::default().fg(DIM)),
                Span::styled("?", key),
                Span::styled(" help", Style::default().fg(DIM)),
            ]));
            lines.push(Line::styled(
                " Italic description: Turn the book into a Word file, a PDF or an EPUB.",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ));
            lines.push(Line::styled(
                " Bold title: WrOS · Settings › System › Battery and power",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                " ⚠ 1 conflict   ● 2 not saved",
                Style::default().fg(WARN),
            ));
            f.render_widget(
                Paragraph::new(lines).style(Style::default().fg(TEXT)),
                glyphs,
            );

            let status = Line::from(vec![
                Span::styled(" 41,280", Style::default().fg(TEXT)),
                Span::styled(" / 90,000 ", Style::default().fg(DIM)),
                Span::styled("▓▓▓▓░░░░░░", Style::default().fg(ACCENT)),
                Span::styled("  today 1,204", Style::default().fg(ACCENT)),
                Span::styled("  ⏯ ❚❚ sprint 12:30", Style::default().fg(DIM)),
                Span::raw(" ".repeat(18)),
                Span::styled("▂▄▆ ♪ 60% ▮ 84% 14:07 ", Style::default().fg(DIM)),
            ]);
            f.render_widget(Paragraph::new(status), bottom);
        })
        .unwrap();
    let pixels = terminal.backend();
    let (w, h) = (pixels.get_pixmap_width(), pixels.get_pixmap_height());
    let mut out = io::stdout().lock();
    write!(out, "P6\n{w} {h}\n255\n")?;
    out.write_all(pixels.get_pixmap_data())?;
    Ok(())
}
