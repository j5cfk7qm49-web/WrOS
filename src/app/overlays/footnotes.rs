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
    pub definition: Option<(usize, usize)>,
    pub source_sha256: String,
}

pub(super) fn rows_for_scene(scene: PathBuf, place: String, body: &str) -> Vec<FootnoteRow> {
    let analysis = analyze_footnotes(body);
    // One bounded preview per definition: thousands of references to a large
    // definition must not clone its entire content into every list row.
    let previews: HashMap<_, String> = analysis
        .definitions
        .iter()
        .map(|(key, definition)| {
            (
                key.as_str(),
                notes::strip_notes(&definition.content)
                    .trim()
                    .chars()
                    .take(256)
                    .collect(),
            )
        })
        .collect();
    let source_sha256 = grimoire_core::diagnostics::hex_digest(body.as_bytes());
    let definition_point = |line: usize| {
        let line = line.saturating_sub(1);
        let text = body.lines().nth(line).unwrap_or("");
        let col = text.find("]:").map_or(0, |at| {
            let after = &text[at + 2..];
            text[..at + 2].chars().count() + after.chars().take_while(|c| c.is_whitespace()).count()
        });
        (line, col)
    };
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
        let preview = previews
            .get(key.as_str())
            .cloned()
            .unwrap_or_else(|| "No definition in this scene".to_string());
        rows.push(FootnoteRow {
            scene: scene.clone(),
            place: place.clone(),
            label: reference.label.clone(),
            preview,
            status,
            line,
            col,
            end: col + body[reference.offset..reference.end_offset].chars().count(),
            definition: (status == "defined")
                .then(|| definition_point(analysis.definitions[&key].line_number)),
            source_sha256: source_sha256.clone(),
        });
    }
    for key in &analysis.unused_definitions {
        if let Some(definition) = analysis.definitions.get(key) {
            let duplicate = analysis.duplicate_definitions.contains(key);
            rows.push(FootnoteRow {
                scene: scene.clone(),
                place: place.clone(),
                label: definition.label.clone(),
                preview: previews[key.as_str()].clone(),
                status: if duplicate { "duplicate" } else { "unused" },
                line: definition_point(definition.line_number).0,
                col: definition_point(definition.line_number).1,
                end: definition_point(definition.line_number).1,
                definition: (!duplicate).then(|| definition_point(definition.line_number)),
                source_sha256: source_sha256.clone(),
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
        let mut bytes = 0usize;
        for i in revision::scenes_in_order(&self.project) {
            let n = &self.project.nodes[i];
            bytes = bytes.saturating_add(n.body.len());
            if n.body.len() > 2 * 1024 * 1024
                || bytes > 16 * 1024 * 1024
                || n.body.matches("[^").count() > 8192
            {
                self.msg = "footnote browser limit reached (2 MiB/scene, 16 MiB/book, 8192 markers/scene); source retained".into();
                return;
            }
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
        self.msg = "Original source · manuscript Word/PDF notes available; EPUB/paperback notes remain guarded".into();
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
            Key::Char('v') => {
                let row = rows.get(*sel).cloned();
                if let Some(row) = row {
                    self.open_footnote_reader(row, true);
                }
            }
            Key::Enter | Key::Char('d') => {
                let row = rows.get(*sel).cloned();
                if let Some(row) = row
                    && let Err(e) = self.goto_footnote(&row, key == Key::Char('d'))
                {
                    self.msg = format!(
                        "Footnote jump refused: {e:#} · Esc, reconcile source, reopen navigator"
                    );
                }
            }
            Key::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }

    pub(super) fn goto_footnote(
        &mut self,
        row: &FootnoteRow,
        definition: bool,
    ) -> anyhow::Result<()> {
        use anyhow::ensure;
        let (line, col, end) = if definition {
            let (line, col) = row
                .definition
                .ok_or_else(|| anyhow::anyhow!("no unique definition"))?;
            (line, col, col)
        } else {
            (row.line, row.col, row.end)
        };
        let i = self.verify_footnote_source(row)?;
        if self.open != Some(i) {
            self.open_scene(i);
        }
        // Opening can fail or focus an existing owner. Never select a retained
        // unrelated editor using this row's old coordinates.
        let (opened, _) = self.check_document(self.views.active)?;
        ensure!(
            opened == i && self.project.nodes[opened].path == row.scene,
            "source did not open"
        );
        ensure!(
            grimoire_core::diagnostics::hex_digest(self.editor.text().as_bytes())
                == row.source_sha256,
            "editor revision changed"
        );
        let node = &self.project.nodes[opened];
        ensure!(
            grimoire_core::sync::Fingerprint::read(&node.path) == node.seen,
            "backing source changed"
        );
        self.reveal(i);
        self.editor.select((line, col), (line, end));
        self.focus = self.editor_focus();
        self.overlay = Overlay::None;
        self.msg = format!(
            "footnote [^{}] · {} · edit original Markdown with normal Undo",
            row.label, row.status
        );
        Ok(())
    }

    pub(super) fn verify_footnote_source(&mut self, row: &FootnoteRow) -> anyhow::Result<usize> {
        use anyhow::ensure;
        self.flush();
        self.ensure_document_claims()?;
        let i = self
            .project
            .nodes
            .iter()
            .position(|n| n.path == row.scene)
            .ok_or_else(|| anyhow::anyhow!("source moved or disappeared"))?;
        let node = &self.project.nodes[i];
        ensure!(
            !node.read_only
                && !node.parked
                && !node.never_read()
                && !self.project.in_trash(i)
                && node.disk.pending.is_none()
                && node.disk.unavailable.is_none(),
            "source is unavailable or unresolved"
        );
        ensure!(
            node.seen.is_some() && grimoire_core::sync::Fingerprint::read(&node.path) == node.seen,
            "backing source changed"
        );
        ensure!(
            grimoire_core::diagnostics::hex_digest(node.body.as_bytes()) == row.source_sha256,
            "source revision changed"
        );
        Ok(i)
    }
}

pub(super) fn draw_footnotes(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Footnotes { sel, rows } = &app.overlay else {
        return;
    };
    f.render_widget(Clear, area);
    let labels: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "[^{}] · {} · {}\n  {}",
                row.label,
                row.status,
                row.place,
                row.preview.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect();
    wros_shell::render_menu(
        f,
        area,
        wros_shell::Menu {
            title: "Footnotes · Original Markdown",
            rows: &labels,
            selected: *sel,
            notice: &app.msg,
            hint: "↑↓ choose  v read  Enter reference  d definition  Esc back  F1 help",
        },
        t.into(),
    );
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
        let rows = rows_for_scene(
            PathBuf::from("scene.md"),
            "Scene".into(),
            "[^one]: First.\n[^ONE]: Second.\n",
        );
        assert_eq!(rows[0].status, "duplicate");
        assert_eq!(
            rows[0].definition, None,
            "unused duplicates cannot choose a definition"
        );
    }

    #[test]
    fn repeated_references_keep_bounded_public_previews() {
        let body = format!(
            "Text[^one] again[^one].\n\n[^one]: Public %% private hidden %% {}\n",
            "original ".repeat(300)
        );
        let rows = rows_for_scene(PathBuf::from("scene.md"), "Scene".into(), &body);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].preview.chars().count(), 256);
        assert_eq!(rows[0].preview, rows[1].preview);
        assert!(!rows[0].preview.contains("private hidden"));
    }
}
