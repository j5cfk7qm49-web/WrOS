//! A public-footnote snapshot beside the retained original editor.
use super::*;
use grimoire_core::footnote_context;
use ratatui::widgets::Wrap;

#[derive(Debug, Clone, PartialEq)]
pub struct Reader {
    pub rows: Vec<FootnoteRow>,
    pub selected: usize,
    pub scroll: u16,
    pub max_scroll: std::cell::Cell<u16>,
    pub definition: String,
    pub references: usize,
    pub back: Option<(usize, Vec<FootnoteRow>)>,
}
fn key(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn public_display(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}
impl App {
    pub fn open_footnote_at_caret(&mut self) {
        let result = (|| -> anyhow::Result<FootnoteRow> {
            anyhow::ensure!(self.focus.is_editor(), "focus the original editor first");
            self.flush();
            self.ensure_document_claims()?;
            let (i, _) = self.check_document(self.views.active)?;
            let text = self.editor.text();
            let line = self
                .editor
                .lines
                .get(self.editor.cy)
                .ok_or_else(|| anyhow::anyhow!("caret unavailable"))?;
            anyhow::ensure!(self.editor.cx <= line.chars().count(), "caret unavailable");
            let offset = self.editor.lines[..self.editor.cy]
                .iter()
                .map(|s| s.len() + 1)
                .sum::<usize>()
                + line
                    .chars()
                    .take(self.editor.cx)
                    .map(char::len_utf8)
                    .sum::<usize>();
            let context = footnote_context::at_caret(&text, offset)?;
            let node = &self.project.nodes[i];
            let rows =
                super::footnotes::rows_for_scene(node.path.clone(), node.title.clone(), &text);
            let matching: Vec<_> = rows
                .into_iter()
                .filter(|r| key(&r.label) == context.key)
                .collect();
            matching
                .iter()
                .find(|r| {
                    r.line == self.editor.cy && r.col <= self.editor.cx && self.editor.cx <= r.end
                })
                .or_else(|| matching.first())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("footnote unavailable"))
        })();
        match result {
            Ok(row) => self.open_footnote_reader(row, false),
            Err(e) => self.msg = format!("Footnote context unavailable: {e:#}"),
        }
    }
    pub(super) fn open_footnote_reader(&mut self, row: FootnoteRow, from_browser: bool) {
        let result = (|| -> anyhow::Result<Reader> {
            let i = self.verify_footnote_source(&row)?;
            let node = &self.project.nodes[i];
            let context = footnote_context::for_label(&node.body, &row.label)?;
            let rows: Vec<_> =
                super::footnotes::rows_for_scene(row.scene.clone(), row.place.clone(), &node.body)
                    .into_iter()
                    .filter(|r| key(&r.label) == context.key)
                    .collect();
            let selected = rows
                .iter()
                .position(|r| r.line == row.line && r.col == row.col)
                .unwrap_or(0);
            let back = if from_browser {
                if let Overlay::Footnotes { sel, rows } = &self.overlay {
                    Some((*sel, rows.clone()))
                } else {
                    None
                }
            } else {
                None
            };
            Ok(Reader {
                rows,
                selected,
                scroll: 0,
                max_scroll: std::cell::Cell::new(0),
                definition: public_display(&context.definition),
                references: context.references,
                back,
            })
        })();
        match result {
            Ok(reader) => {
                self.overlay = Overlay::FootnoteReader(Box::new(reader));
                self.msg = "Public definition snapshot · edits stay in original Markdown".into();
            }
            Err(e) => self.msg = format!("Footnote context unavailable: {e:#}"),
        }
    }
    pub(super) fn on_footnote_reader_key(&mut self, key: Key) {
        let Overlay::FootnoteReader(reader) = &mut self.overlay else {
            return;
        };
        match key {
            Key::Esc => {
                self.overlay = match reader.back.take() {
                    Some((sel, rows)) => Overlay::Footnotes { sel, rows },
                    None => Overlay::None,
                };
            }
            Key::Down | Key::Char('j') => {
                reader.scroll = reader.scroll.saturating_add(1).min(reader.max_scroll.get())
            }
            Key::Up | Key::Char('k') => reader.scroll = reader.scroll.saturating_sub(1),
            Key::PageDown => {
                reader.scroll = reader.scroll.saturating_add(8).min(reader.max_scroll.get())
            }
            Key::PageUp => reader.scroll = reader.scroll.saturating_sub(8),
            Key::Home => reader.scroll = 0,
            Key::End => reader.scroll = reader.max_scroll.get(),
            Key::Left | Key::Right => {
                let mut nav = wros_shell::Navigation::new(());
                nav.select(reader.selected, reader.rows.len());
                nav.move_by(if key == Key::Right { 1 } else { -1 }, reader.rows.len());
                reader.selected = nav.selected();
                self.msg = if reader.references == 0 {
                    "Snapshot has no public references · d requires a unique definition".into()
                } else {
                    format!(
                        "Snapshot reference {} / {} selected · r opens original source",
                        reader.selected + 1,
                        reader.references
                    )
                };
            }
            Key::Enter | Key::Char('d' | 'r') => {
                let row = reader.rows.get(reader.selected).cloned();
                let references = reader.references;
                if let Some(row) = row {
                    let result = if key == Key::Char('r') && references == 0 {
                        Err(anyhow::anyhow!("this definition has no reference"))
                    } else {
                        self.goto_footnote(&row, key != Key::Char('r'))
                    };
                    if let Err(e) = result {
                        self.msg = format!(
                            "Footnote navigation refused: {e:#} · close and reopen after source reconciliation"
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
pub(super) fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let Overlay::FootnoteReader(reader) = &app.overlay else {
        return;
    };
    let Some(row) = reader.rows.get(reader.selected) else {
        return;
    };
    // The original writing remains visible above the common shell panel.
    let dock = Rect {
        y: area.y + area.height / 2,
        height: area.height - area.height / 2,
        ..area
    };
    f.render_widget(Clear, dock);
    let body = wros_shell::render_panel(
        f,
        dock,
        "Footnote context · Source Markdown",
        &app.msg,
        "↑↓ scroll  ←→ reference  d definition  r reference  Esc back  F1 help",
        theme.into(),
    );
    let text = format!(
        "[^{}] · {} · reference {} / {}\n{} · line {}\n\n{}",
        public_display(&row.label),
        row.status,
        if reader.references == 0 {
            0
        } else {
            reader.selected + 1
        },
        reader.references,
        public_display(&row.place),
        row.line + 1,
        reader.definition
    );
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.text));
    // Use the pinned renderer's own wrapping calculation, not an approximate
    // character/word count, so End and PageDown keep the final text visible.
    let max = paragraph
        .line_count(body.width)
        .saturating_sub(body.height as usize)
        .min(u16::MAX as usize) as u16;
    reader.max_scroll.set(max);
    f.render_widget(paragraph.scroll((reader.scroll.min(max), 0)), body);
}
