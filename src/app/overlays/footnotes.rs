//! Read-only footnote navigator for the original Markdown manuscript.
//! Preview and jump never copy editorial text into a separate editable store.

use super::*;
use grimoire_core::footnotes::analyze_footnotes;
use grimoire_core::notes;
use grimoire_core::revision;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteRow {
    pub scene: PathBuf,
    pub place: String,
    pub label: String,
    pub preview: String,
    pub status: &'static str,
    pub line: usize,
    pub col: usize,
    pub end: usize,
}

fn rows_for_scene(scene: PathBuf, place: String, body: &str) -> Vec<FootnoteRow> {
    let analysis = analyze_footnotes(body);
    let mut rows = Vec::new();
    for reference in &analysis.references {
        // The parser records UTF-8 source byte offsets, not display columns.
        // Convert the offset into the editor's character-based line/column.
        let prefix = &body[..reference.offset];
        let line = prefix.bytes().filter(|&b| b == b'\n').count();
        let col = prefix.rsplit('\n').next().unwrap_or("").chars().count();
        let key = reference
            .label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let status = if analysis.duplicate_definitions.contains(&key) {
            "duplicate"
        } else if analysis.missing_definitions.contains(&key) {
            "missing"
        } else {
            "defined"
        };
        let preview = analysis.definitions.get(&key).map_or_else(
            || "No definition in this scene".to_string(),
            |definition| notes::strip_notes(&definition.content).trim().to_string(),
        );
        rows.push(FootnoteRow {
            scene: scene.clone(),
            place: place.clone(),
            label: reference.label.clone(),
            preview,
            status,
            line,
            col,
            end: col + reference.label.chars().count() + 3,
        });
    }
    for key in &analysis.unused_definitions {
        if let Some(definition) = analysis.definitions.get(key) {
            rows.push(FootnoteRow {
                scene: scene.clone(),
                place: place.clone(),
                label: definition.label.clone(),
                preview: notes::strip_notes(&definition.content).trim().to_string(),
                status: "unused",
                line: definition.line_number.saturating_sub(1),
                col: 0,
                end: 0,
            });
        }
    }
    rows.sort_by_key(|row| (row.line, row.col));
    rows
}

impl App {
    pub fn open_footnotes(&mut self) {
        self.flush();
        let mut rows = Vec::new();
        for i in revision::scenes_in_order(&self.project) {
            let n = &self.project.nodes[i];
            let place = grimoire_core::search::place_of(&self.project, &self.parents, i);
            rows.extend(rows_for_scene(n.path.clone(), place, &n.body));
            if rows.len() > 2048 {
                self.msg = "too many footnotes to list · manuscript unchanged".into();
                return;
            }
        }
        if rows.is_empty() {
            self.msg = "no public Markdown footnotes in this book yet · use [^name] and [^name]: definition".into();
            return;
        }
        self.overlay = Overlay::Footnotes { sel: 0, rows };
    }

    pub(super) fn on_footnotes_key(&mut self, key: Key) {
        let Overlay::Footnotes { sel, rows } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, rows.len(), LIST) {
            return;
        }
        match key {
            Key::Enter => {
                let row = rows.get(*sel).cloned();
                self.overlay = Overlay::None;
                if let Some(row) = row {
                    self.go_to_hit(&row.scene, row.line, row.col, row.end);
                    self.msg = format!("footnote [^{}] · {}", row.label, row.status);
                }
            }
            Key::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }
}

pub(super) fn draw_footnotes(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Footnotes { sel, rows } = &app.overlay else {
        return;
    };
    let box_area = centred(area, 94, 19);
    f.render_widget(Clear, box_area);
    let heading = format!(
        "FOOTNOTES · {} reference(s) / unused definition(s)",
        rows.len()
    );
    let block = pane_block(&heading, true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let dim = Style::default().fg(t.dim);
    let width = inner.width as usize;
    let max_rows = (inner.height as usize).saturating_sub(7).min(10);
    let sel = (*sel).min(rows.len().saturating_sub(1));
    let start = sel.saturating_sub(max_rows.saturating_sub(1));
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "  ↑↓ select · Enter jumps to source · Esc returns to writing",
        dim,
    )));
    lines.push(Line::from(""));
    for (i, row) in rows.iter().enumerate().skip(start).take(max_rows) {
        let on = i == sel;
        let label = format!("[^{}]", row.label);
        let place = truncate(&row.place, width / 3);
        let middle = truncate(
            &format!("{label}  {}  ·  {place}", row.status),
            width.saturating_sub(4),
        );
        lines.push(
            Line::from(vec![
                Span::styled(
                    if on { " ▸ " } else { "   " },
                    Style::default().fg(t.accent),
                ),
                Span::styled(
                    middle,
                    Style::default().fg(if on { t.accent } else { t.text }),
                ),
            ])
            .style(if on {
                Style::default().bg(t.sel)
            } else {
                Style::default()
            }),
        );
    }
    while lines.len() < max_rows + 3 {
        lines.push(Line::from(""));
    }
    if let Some(row) = rows.get(sel) {
        lines.push(Line::from(Span::styled(
            format!("  [^{}] · {}", row.label, row.status),
            Style::default().fg(t.accent),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                truncate(
                    &row.preview.split_whitespace().collect::<Vec<_>>().join(" "),
                    width.saturating_sub(4)
                )
            ),
            Style::default().fg(t.text),
        )));
    }
    lines.push(Line::from(""));
    lines.push(hint_line(
        " View/jump only · DOCX/PDF page-footnote export is not implemented",
        t,
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footnote_navigator_tracks_unicode_columns_and_missing_definitions() {
        let body = "λ[^good] and [^missing]\n\n[^good]: First definition.\n";
        let rows = rows_for_scene(PathBuf::from("scene.md"), "Chapter › Scene".into(), body);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].line, 0);
        assert_eq!(rows[0].col, 1, "Unicode character, not UTF-8 byte");
        assert_eq!(rows[0].status, "defined");
        assert!(rows[0].preview.contains("First definition."));
        assert_eq!(rows[1].status, "missing");
    }

    #[test]
    fn footnote_navigator_ignores_code_links_private_notes_and_shows_unused_definitions() {
        let body = "Text `[^code]` \\[^escape] [link](https://example.invalid/[^url]).\n%% [^private]: hidden %%\n\n[^orphan]: Unused note.\n";
        let rows = rows_for_scene(PathBuf::from("scene.md"), "Scene".into(), body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "orphan");
        assert_eq!(rows[0].status, "unused");
        assert_eq!(rows[0].line, 3);
    }

    #[test]
    fn duplicate_footnote_labels_get_an_explicit_review_status() {
        let body = "Text[^one].\n\n[^one]: First.\n[^ONE]: Second.\n";
        let rows = rows_for_scene(PathBuf::from("scene.md"), "Scene".into(), body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "duplicate");
    }
}
