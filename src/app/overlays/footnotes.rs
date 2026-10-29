//! Read-only, offline footnote inspection for the active editor buffer.
//! This view never writes or reloads the manuscript, and is not an exporter.

use super::*;
use grimoire_core::footnotes::analyze_footnotes;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FootnoteRow {
    pub label: String,
    pub references: usize,
    pub first_reference_line: Option<usize>,
    pub definition_line: Option<usize>,
    pub excerpt: String,
    pub duplicate: bool,
}

impl FootnoteRow {
    fn status(&self) -> &'static str {
        if self.duplicate {
            "DUPLICATE"
        } else if self.definition_line.is_none() {
            "MISSING"
        } else if self.references == 0 {
            "UNUSED"
        } else {
            "OK"
        }
    }
}

fn normalized(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Build a bounded, deterministic snapshot. The parser, not a regular
/// expression over raw manuscript text, decides which references are visible.
fn inspect(markdown: &str) -> Result<Vec<FootnoteRow>, &'static str> {
    if markdown.len() > 2 * 1024 * 1024 {
        return Err("Footnote inspector: scene exceeds the 2 MiB preview limit.");
    }
    let analysis = analyze_footnotes(markdown);
    let mut found = BTreeMap::<String, FootnoteRow>::new();

    for reference in analysis.references {
        let key = normalized(&reference.label);
        let line = markdown[..reference.offset]
            .bytes()
            .filter(|&b| b == b'\n')
            .count() + 1;
        let row = found.entry(key.clone()).or_insert_with(|| FootnoteRow {
            label: key,
            references: 0,
            first_reference_line: Some(line),
            definition_line: None,
            excerpt: String::new(),
            duplicate: false,
        });
        row.references += 1;
        if row.first_reference_line.is_none() {
            row.first_reference_line = Some(line);
        }
    }

    for (key, definition) in analysis.definitions {
        let row = found.entry(key.clone()).or_insert_with(|| FootnoteRow {
            label: key,
            references: 0,
            first_reference_line: None,
            definition_line: None,
            excerpt: String::new(),
            duplicate: false,
        });
        row.definition_line = Some(definition.line_number);
        row.excerpt = definition.content.split_whitespace().take(60).collect::<Vec<_>>().join(" ");
    }

    for key in analysis.duplicate_definitions {
        if let Some(row) = found.get_mut(&key) {
            row.duplicate = true;
        }
    }

    if found.len() > 256 {
        return Err("Footnote inspector: more than 256 distinct notes; no partial preview.");
    }
    Ok(found.into_values().collect())
}

impl App {
    pub fn open_footnotes(&mut self) {
        let Some(_) = self.open else {
            self.msg = "Open a scene to inspect its footnotes.".into();
            return;
        };
        // Read the editor, not the project cache: unsaved and secondary-pane
        // changes must be visible immediately without triggering a disk save.
        let text = self.editor.text();
        match inspect(&text) {
            Ok(rows) => {
                self.overlay = Overlay::Footnotes {
                    title: self.open_title(),
                    rows,
                    sel: 0,
                };
            }
            Err(message) => self.msg = message.into(),
        }
    }

    pub(super) fn on_footnotes_key(&mut self, key: Key) {
        let Overlay::Footnotes { rows, sel, .. } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, rows.len(), LIST) {
            return;
        }
        if matches!(key, Key::Esc | Key::Enter | Key::Char('q')) {
            self.overlay = Overlay::None;
        }
    }
}

pub(super) fn draw_footnotes(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Footnotes { title, rows, sel } = &app.overlay else {
        return;
    };
    let width = area.width.saturating_sub(4).min(94);
    let height = area.height.saturating_sub(4).min(25);
    if width < 10 || height < 7 {
        return;
    }
    let box_area = centred(area, width, height);
    f.render_widget(Clear, box_area);
    let block = pane_block("FOOTNOTES · READ-ONLY PREVIEW", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let room = inner.width as usize;
    let dim = Style::default().fg(t.dim);
    let mut lines = Vec::<Line>::new();
    lines.push(Line::from(Span::styled(
        format!(" {}", truncate(title, room.saturating_sub(2))),
        Style::default().fg(t.accent),
    )));
    lines.push(Line::from(format!(
        " {} distinct note(s) · {} reference(s) · snapshot of open scene",
        rows.len(),
        rows.iter().map(|r| r.references).sum::<usize>()
    )));
    lines.push(Line::from(""));
    if rows.is_empty() {
        lines.push(Line::from(" No Markdown footnotes in this scene."));
        lines.push(Line::from(" Use [^source] in prose, then [^source]: text below."));
    } else {
        let display_count = (inner.height as usize).saturating_sub(10).max(1);
        let start = sel.saturating_sub(display_count.saturating_sub(1));
        let end = (start + display_count).min(rows.len());
        for (i, row) in rows.iter().enumerate().take(end).skip(start) {
            let on = i == *sel;
            let text = format!(
                " {} {:<9} [^{}] · {} ref(s)",
                if on { "▸" } else { " " },
                row.status(),
                row.label,
                row.references
            );
            lines.push(Line::from(Span::styled(
                truncate(&text, room.saturating_sub(1)),
                Style::default()
                    .fg(if row.status() == "OK" { t.text } else { t.warn })
                    .bg(if on { t.sel } else { t.bg }),
            )));
        }
        lines.push(Line::from(""));
        let current = &rows[(*sel).min(rows.len() - 1)];
        lines.push(Line::from(Span::styled(
            format!(
                " [^{}] · {}",
                truncate(&current.label, room.saturating_sub(21)),
                current.status()
            ),
            Style::default().fg(t.accent),
        )));
        lines.push(Line::from(format!(
            " First reference: {} · Definition: {}",
            current.first_reference_line.map_or("—".into(), |n| n.to_string()),
            current.definition_line.map_or("—".into(), |n| n.to_string())
        )));
        if current.definition_line.is_some() {
            lines.push(Line::from(Span::styled(
                format!(" {}", truncate(&current.excerpt, room.saturating_sub(2))),
                dim,
            )));
        } else {
            lines.push(Line::from(Span::styled(" Add a matching definition in this scene.", dim)));
        }
    }
    while lines.len() < (inner.height as usize).saturating_sub(1) {
        lines.push(Line::from(""));
    }
    lines.push(hint_line(" ↑↓ select · PgUp/PgDn move · ↵/esc close · reopen to refresh", t));
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_missing_unused_duplicate_and_multiline_unicode_notes() {
        let text = "A λ claim[^Source], again[^source], and missing[^gone].\n\n\
                    [^source]: A careful explanation.\n\n    Continued λ text.\n\n\
                    [^source]: An ambiguous duplicate.\n\n\
                    [^orphan]: Never referenced.";
        let rows = inspect(text).unwrap();
        assert_eq!(rows.len(), 3);
        let source = rows.iter().find(|r| r.label == "source").unwrap();
        assert_eq!(source.references, 2);
        assert!(source.duplicate);
        assert_eq!(source.first_reference_line, Some(1));
        assert_eq!(source.status(), "DUPLICATE");
        assert!(source.excerpt.contains("Continued λ text."));
        assert_eq!(rows.iter().find(|r| r.label == "gone").unwrap().status(), "MISSING");
        assert_eq!(rows.iter().find(|r| r.label == "orphan").unwrap().status(), "UNUSED");
    }

    #[test]
    fn invisible_markup_does_not_create_notes_and_limit_refuses_partial_preview() {
        let text = "%% [^hidden]: secret %%\n\n`[^inline]` \\[^escaped] [link](https://example.invalid/[^url])";
        // The same context rules as the maintained Markdown parser apply.
        assert!(inspect(text).unwrap().is_empty());
        assert!(inspect(&"x".repeat(2 * 1024 * 1024 + 1)).is_err());
    }
}
