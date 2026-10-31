//! Revision-note workflow over the existing retained editor and guarded store.
use super::*;
use anyhow::{Result, ensure};
use grimoire_core::{
    annotations::AnnotationStatus as Status,
    diagnostics::Snapshot,
    review_notes::{self as model, Anchor, ReviewNotes},
    statefile::Store,
};
use ratatui::{
    Frame,
    style::Style,
    widgets::{Clear, Paragraph, Wrap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Queue,
    Detail,
    Comment,
    Reanchor,
}
#[derive(Clone)]
struct Binding {
    pane: Pane,
    epoch: (u64, u64),
    snapshot: Snapshot,
}
struct Draft {
    binding: Binding,
    anchor: Anchor,
    note: Option<u64>,
    comment: String,
}
pub struct Review {
    pub store: Option<Store<ReviewNotes>>,
    pub nav: wros_shell::Navigation<Page>,
    pub notice: String,
    pub all: bool,
    pub scroll: u16,
    selected: Option<u64>,
    binding: Option<Binding>,
    draft: Option<Draft>,
}
impl Review {
    pub fn load(root: &Path) -> Self {
        let (store, notice) = match ReviewNotes::load(root) {
            Ok(s) => (
                Some(s),
                "Select a passage in the editor, then n to add a note.".into(),
            ),
            Err(e) => (None, format!("Notes unavailable: {e:#}")),
        };
        Self {
            store,
            nav: wros_shell::Navigation::new(Page::Queue),
            notice,
            all: false,
            scroll: 0,
            selected: None,
            binding: None,
            draft: None,
        }
    }
    fn visible(&self) -> Vec<&model::Note> {
        self.store.as_ref().map_or_else(Vec::new, |s| {
            s.value
                .notes
                .iter()
                .filter(|n| self.all || n.current.status != Status::Resolved)
                .collect()
        })
    }
    fn note(&self) -> Result<&model::Note> {
        self.store
            .as_ref()
            .and_then(|s| s.value.notes.iter().find(|n| Some(n.id) == self.selected))
            .ok_or_else(|| anyhow::anyhow!("select a revision note first"))
    }
    fn store(&mut self) -> Result<&mut Store<ReviewNotes>> {
        let mut reason = self.notice.clone();
        if !reason.ends_with("original metadata preserved") {
            reason.push_str(" · original metadata preserved");
        }
        self.store.as_mut().ok_or_else(|| anyhow::anyhow!(reason))
    }
    pub fn typing(&self) -> bool {
        self.nav.current() == Page::Comment
    }
    pub fn has_draft(&self) -> bool {
        self.draft.is_some()
    }
}
fn plain(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}
impl App {
    pub(super) fn current_story_document_id(&self) -> Option<u64> {
        let (_, path) = self.check_document(self.views.active).ok()?;
        self.review_notes
            .store
            .as_ref()?
            .value
            .documents
            .iter()
            .find(|d| d.path == path && !d.needs_reconnect)
            .map(|d| d.id)
    }
    pub(super) fn story_document_id(&mut self, i: usize) -> Result<u64> {
        self.ensure_document_claims()?;
        let n = &self.project.nodes[i];
        ensure!(
            n.kind == Kind::Scene
                && !n.read_only
                && !n.parked
                && !n.never_read()
                && !self.project.in_trash(i)
                && n.disk.unavailable.is_none()
                && n.disk.pending.is_none(),
            "story source unavailable or awaiting reconciliation"
        );
        let root = fs::canonicalize(&self.project.root)?;
        let relative = n.path.strip_prefix(&self.project.root)?;
        ensure!(
            fs::canonicalize(&n.path)? == root.join(relative),
            "story source is a symlink alias"
        );
        ensure!(
            n.seen.is_some() && grimoire_core::sync::Fingerprint::read(&n.path) == n.seen,
            "story backing source changed; reconcile it first"
        );
        let path = relative
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non UTF-8 scene path"))?
            .to_owned();
        let store = self.review_notes.store()?;
        if let Some(document) = store.value.documents.iter().find(|d| d.path == path) {
            ensure!(
                !document.needs_reconnect,
                "document move needs explicit reconnection"
            );
            let id = document.id;
            store.verify()?;
            return Ok(id);
        }
        store.update(|notes| notes.register(&path))
    }
    pub(super) fn active_story_document_id(&mut self) -> Result<u64> {
        self.flush();
        let binding = self.review_binding()?;
        let i = self
            .open
            .ok_or_else(|| anyhow::anyhow!("open an original scene first"))?;
        let id = self.story_document_id(i)?;
        ensure!(self.review_matches(&binding), "story source changed");
        Ok(id)
    }
    pub(super) fn snapshot_before_story_restore(&mut self) -> Result<()> {
        self.flush();
        let i = self
            .open
            .ok_or_else(|| anyhow::anyhow!("open a scene first"))?;
        self.active_story_document_id()?;
        let n = &self.project.nodes[i];
        grimoire_core::history::snapshot(&self.project.root, &n.path, &n.file_text(), None)?;
        Ok(())
    }
    pub(super) fn mark_current_reading_sentence(
        &mut self,
        line: usize,
        start: usize,
        end: usize,
    ) -> Result<()> {
        let binding = self.review_binding()?;
        let a = binding
            .snapshot
            .byte_at(line, start)
            .ok_or_else(|| anyhow::anyhow!("sentence start unavailable"))?;
        let b = binding
            .snapshot
            .byte_at(line, end)
            .ok_or_else(|| anyhow::anyhow!("sentence end unavailable"))?;
        let anchor = Anchor::capture(binding.snapshot.text(), a, b)?;
        self.review_notes.store()?.update(|notes| {
            notes.add(
                &binding.snapshot.stamp.document,
                anchor,
                "Marked during read-aloud for revision.".into(),
            )
        })?;
        Ok(())
    }
    pub fn open_review_notes(&mut self) {
        self.overlay = Overlay::ReviewNotes;
        self.review_notes.nav.reset(Page::Queue);
        self.review_notes.draft = None;
        self.review_notes.binding = None;
        self.review_notes.scroll = 0;
    }
    fn review_binding(&mut self) -> Result<Binding> {
        self.ensure_document_claims()?;
        let pane = self.views.active;
        let (i, _) = self.check_document(pane)?;
        let node = &self.project.nodes[i];
        let canonical = fs::canonicalize(&node.path)?;
        ensure!(
            canonical
                == fs::canonicalize(&self.project.root)?
                    .join(node.path.strip_prefix(&self.project.root)?),
            "annotation source must not be a symlink alias"
        );
        ensure!(
            node.seen.is_some() && grimoire_core::sync::Fingerprint::read(&node.path) == node.seen,
            "backing source changed or is unreadable; reconcile it first"
        );
        Ok(Binding {
            pane,
            epoch: self.editor.revision_stamp(),
            snapshot: self.check_snapshot(pane, 0)?,
        })
    }
    fn review_matches(&self, b: &Binding) -> bool {
        self.views.active == b.pane
            && self.editor.revision_stamp() == b.epoch
            && self
                .check_document(b.pane)
                .is_ok_and(|(_, p)| p == b.snapshot.stamp.document)
    }
    fn verify_review_binding(&mut self, expected: &Binding) -> Result<Binding> {
        ensure!(
            self.review_matches(expected),
            "source revision changed; cancel or refresh before reviewing"
        );
        let now = self.review_binding()?;
        ensure!(
            now.snapshot.stamp == expected.snapshot.stamp,
            "source changed; refresh before reviewing"
        );
        self.review_notes.store()?.verify()?;
        Ok(now)
    }
    fn review_selection(&mut self, note: Option<u64>) -> Result<Draft> {
        self.review_notes.store()?.verify()?;
        let binding = self.review_binding()?;
        let ((line, col), (last, end)) = self
            .editor
            .selection()
            .ok_or_else(|| anyhow::anyhow!("select a passage in the editor first"))?;
        // Editor coordinates are Unicode scalar columns; anchors use UTF-8 bytes.
        let byte = |row: usize, column: usize| -> Result<usize> {
            binding
                .snapshot
                .byte_at(row, column)
                .ok_or_else(|| anyhow::anyhow!("invalid selection"))
        };
        let anchor = Anchor::capture(binding.snapshot.text(), byte(line, col)?, byte(last, end)?)?;
        Ok(Draft {
            binding,
            anchor,
            note,
            comment: String::new(),
        })
    }
    fn begin_review_comment(&mut self, editing: bool) -> Result<()> {
        let mut draft = if editing {
            let note = self.review_notes.note()?.clone();
            let binding = self.review_binding()?;
            let document = self
                .review_notes
                .store()?
                .value
                .document(note.current.document_id)?;
            ensure!(
                !document.needs_reconnect,
                "reconnect the interrupted document move before editing its notes"
            );
            let path = document.path.clone();
            ensure!(
                binding.snapshot.stamp.document == path,
                "open the note's source in the editor first"
            );
            Draft {
                binding,
                anchor: note.current.anchor,
                note: Some(note.id),
                comment: note.current.comment,
            }
        } else {
            self.review_selection(None)?
        };
        if !editing {
            draft.comment.clear();
        }
        self.review_notes.draft = Some(draft);
        self.review_notes.nav.open(Page::Comment);
        self.review_notes.scroll = 0;
        self.review_notes.notice =
            "Enter saves the comment. Esc cancels. Manuscript stays unchanged.".into();
        Ok(())
    }
    fn save_review_comment(&mut self) -> Result<()> {
        let draft = self
            .review_notes
            .draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no comment draft"))?;
        let (binding, anchor, note, comment) = (
            draft.binding.clone(),
            draft.anchor.clone(),
            draft.note,
            draft.comment.clone(),
        );
        self.verify_review_binding(&binding)?;
        let id = self.review_notes.store()?.update(|s| {
            if let Some(id) = note {
                s.revise(id, |v| {
                    v.comment = comment;
                    v.status = Status::Open;
                    v.reviewed_sha256 = None;
                })?;
                Ok(id)
            } else {
                s.add(&binding.snapshot.stamp.document, anchor, comment)
            }
        })?;
        self.review_notes.selected = Some(id);
        self.review_notes.draft = None;
        self.review_notes.nav.reset(Page::Queue);
        self.review_notes.nav.open(Page::Detail);
        self.review_notes.binding = Some(binding);
        self.review_notes.scroll = 0;
        self.review_notes.notice = "Comment saved locally; source and undo retained.".into();
        Ok(())
    }
    fn inspect_review_note(&mut self) -> Result<()> {
        let id = self
            .review_notes
            .visible()
            .get(self.review_notes.nav.selected())
            .map(|n| n.id)
            .ok_or_else(|| anyhow::anyhow!("no notes in this filter"))?;
        self.review_notes.store()?.verify()?;
        self.review_notes.selected = Some(id);
        self.review_notes.nav.open(Page::Detail);
        self.review_notes.scroll = 0;
        self.refresh_review_binding();
        Ok(())
    }
    fn refresh_review_binding(&mut self) {
        match self.review_binding() {
            Ok(b) => {
                self.review_notes.binding = Some(b);
                self.review_notes.notice =
                    "Review the quote and comment; g selects only an exact unique anchor.".into();
            }
            Err(e) => {
                self.review_notes.binding = None;
                self.review_notes.notice = format!("{e:#}");
            }
        }
    }
    fn review_location(&mut self) -> Result<(Binding, (usize, usize))> {
        let expected = self
            .review_notes
            .binding
            .clone()
            .ok_or_else(|| anyhow::anyhow!("refresh with r first"))?;
        let binding = self.verify_review_binding(&expected)?;
        let note = self.review_notes.note()?;
        ensure!(
            !self
                .review_notes
                .store
                .as_ref()
                .unwrap()
                .value
                .document(note.current.document_id)?
                .needs_reconnect,
            "interrupted document move; explicitly re-anchor to reconnect this identity"
        );
        let path = &self
            .review_notes
            .store
            .as_ref()
            .unwrap()
            .value
            .document(note.current.document_id)?
            .path;
        ensure!(
            *path == binding.snapshot.stamp.document,
            "open the recorded source in the editor, then refresh with r"
        );
        let span = note
            .current
            .anchor
            .locate(binding.snapshot.text())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "anchor changed or ambiguous; select a passage and explicitly re-anchor"
                )
            })?;
        Ok((binding, span))
    }
    fn goto_review_note(&mut self) -> Result<()> {
        let (binding, (start, end)) = self.review_location()?;
        let a = binding
            .snapshot
            .point(start)
            .ok_or_else(|| anyhow::anyhow!("invalid source start"))?;
        let b = binding
            .snapshot
            .point(end)
            .ok_or_else(|| anyhow::anyhow!("invalid source end"))?;
        self.editor.select(a, b);
        self.focus = self.editor_focus();
        self.overlay = Overlay::None;
        self.msg =
            "Revision passage selected; edit normally, with existing Undo and autosave.".into();
        Ok(())
    }
    fn set_review_status(&mut self, status: Status) -> Result<()> {
        let (binding, _) = self.review_location()?;
        let id = self.review_notes.note()?.id;
        self.review_notes.store()?.update(|s| {
            s.revise(id, |v| {
                v.status = status;
                v.reviewed_sha256 =
                    (status == Status::Resolved).then_some(binding.snapshot.stamp.sha256);
            })
        })?;
        self.review_notes.notice =
            format!("{} saved for the reviewed source revision.", status.label());
        Ok(())
    }
    fn begin_review_reanchor(&mut self) -> Result<()> {
        let id = self.review_notes.note()?.id;
        let draft = self.review_selection(Some(id))?;
        self.review_notes.draft = Some(draft);
        self.review_notes.nav.open(Page::Reanchor);
        self.review_notes.nav.select(0, 2);
        self.review_notes.notice = "Review old and new passages. Cancel is selected.".into();
        Ok(())
    }
    fn save_review_reanchor(&mut self) -> Result<()> {
        let draft = self
            .review_notes
            .draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no anchor draft"))?;
        let (binding, anchor, id) = (
            draft.binding.clone(),
            draft.anchor.clone(),
            draft.note.unwrap(),
        );
        self.verify_review_binding(&binding)?;
        let doc = self.review_notes.note()?.current.document_id;
        let document = self.review_notes.store()?.value.document(doc)?.clone();
        let old = document.path;
        let new = &binding.snapshot.stamp.document;
        let changed = anchor != self.review_notes.note()?.current.anchor;
        ensure!(
            changed || old != *new || document.needs_reconnect,
            "anchor is unchanged; nothing saved"
        );
        if old != *new && !document.needs_reconnect {
            match fs::symlink_metadata(self.project.root.join(&old)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => anyhow::bail!(
                    "old document still exists or cannot be verified; refusing an identity transfer"
                ),
            }
        }
        self.review_notes.store()?.update(|s| {
            s.relocate(doc, new)?;
            s.revise(id, |v| {
                v.anchor = anchor;
                v.status = Status::Open;
                v.reviewed_sha256 = None;
            })
        })?;
        self.review_notes.draft = None;
        self.review_notes.nav.back();
        self.review_notes.binding = Some(binding);
        self.review_notes.notice =
            "Anchor saved; previous anchor and comment retained in note history.".into();
        if changed {
            self.observe_deck(grimoire_core::tutorial::Event::AnnotationReanchored);
        }
        Ok(())
    }
    pub fn paste_review_notes(&mut self, text: &str) {
        if !self.review_notes.typing() {
            return;
        }
        if let Some(draft) = &mut self.review_notes.draft {
            let text: String = text
                .chars()
                .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            if draft.comment.len().saturating_add(text.len()) <= model::MAX_COMMENT {
                draft.comment.push_str(&text);
            } else {
                self.review_notes.notice =
                    "Comment limit: 4096 bytes; paste was not inserted.".into();
            }
        }
    }
    pub fn on_review_notes_key(&mut self, key: Key) {
        let page = self.review_notes.nav.current();
        if key == Key::Esc {
            self.review_notes.draft = None;
            self.review_notes.scroll = 0;
            if !self.review_notes.nav.back() {
                self.overlay = Overlay::None;
            }
            return;
        }
        let result = match (page, key) {
            (Page::Queue, Key::Down | Key::Char('j')) => {
                let n = self.review_notes.visible().len();
                self.review_notes.nav.move_by(1, n);
                Ok(())
            }
            (Page::Queue, Key::Up | Key::Char('k')) => {
                let n = self.review_notes.visible().len();
                self.review_notes.nav.move_by(-1, n);
                Ok(())
            }
            (Page::Queue, Key::Home) => {
                let n = self.review_notes.visible().len();
                self.review_notes.nav.select(0, n);
                Ok(())
            }
            (Page::Queue, Key::End) => {
                let n = self.review_notes.visible().len();
                self.review_notes.nav.select(n.saturating_sub(1), n);
                Ok(())
            }
            (Page::Queue, Key::Char('f')) => {
                self.review_notes.all = !self.review_notes.all;
                self.review_notes
                    .nav
                    .select(0, self.review_notes.visible().len());
                Ok(())
            }
            (Page::Queue, Key::Char('r')) => {
                self.review_notes = Review::load(&self.project.root);
                Ok(())
            }
            (Page::Queue, Key::Enter) => self.inspect_review_note(),
            (Page::Queue, Key::Char('n')) => self.begin_review_comment(false),
            (Page::Detail, Key::Char('e')) => self.begin_review_comment(true),
            (Page::Detail, Key::Char('a')) => self.begin_review_reanchor(),
            (Page::Detail, Key::Char('g')) => self.goto_review_note(),
            (Page::Detail, Key::Char('s')) => self.set_review_status(Status::InProgress),
            (Page::Detail, Key::Char('x')) => self.set_review_status(Status::Resolved),
            (Page::Detail, Key::Char('o')) => self.set_review_status(Status::Open),
            (Page::Detail, Key::Char('r')) => {
                self.refresh_review_binding();
                Ok(())
            }
            (Page::Detail | Page::Comment | Page::Reanchor, Key::PageDown) => {
                self.review_notes.scroll = self.review_notes.scroll.saturating_add(8);
                Ok(())
            }
            (Page::Detail | Page::Comment | Page::Reanchor, Key::PageUp) => {
                self.review_notes.scroll = self.review_notes.scroll.saturating_sub(8);
                Ok(())
            }
            (Page::Comment, Key::Enter) => self.save_review_comment(),
            (Page::Comment, Key::Backspace) => {
                if let Some(d) = &mut self.review_notes.draft {
                    d.comment.pop();
                }
                Ok(())
            }
            (Page::Comment, Key::Char(c)) => {
                self.paste_review_notes(&c.to_string());
                Ok(())
            }
            (Page::Reanchor, Key::Up | Key::Down) => {
                let n = 1 - self.review_notes.nav.selected().min(1);
                self.review_notes.nav.select(n, 2);
                Ok(())
            }
            (Page::Reanchor, Key::Enter) if self.review_notes.nav.selected() == 1 => {
                self.save_review_reanchor()
            }
            (Page::Reanchor, Key::Enter) => {
                self.review_notes.draft = None;
                self.review_notes.nav.back();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.review_notes.notice = format!("{e:#}");
        }
    }
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let r = &app.review_notes;
    f.render_widget(Clear, area);
    let page = r.nav.current();
    if page == Page::Queue {
        let rows: Vec<String> = r
            .visible()
            .iter()
            .map(|n| {
                let path = r
                    .store
                    .as_ref()
                    .and_then(|s| s.value.document(n.current.document_id).ok())
                    .map_or("Unavailable", |d| d.path.as_str());
                format!(
                    "#{} · {} · {}\n  {}",
                    n.id,
                    n.current.status.label(),
                    plain(&n.current.comment).replace('\n', " "),
                    plain(path)
                )
            })
            .collect();
        let rows = if rows.is_empty() {
            vec!["No notes in this filter. Select original text, then n to add a note.".into()]
        } else {
            rows
        };
        wros_shell::render_menu(
            f,
            area,
            wros_shell::Menu {
                title: if r.all {
                    "Revision notes · All"
                } else {
                    "Revision notes · Open"
                },
                rows: &rows,
                selected: r.nav.selected(),
                notice: &r.notice,
                hint: "↑↓ choose  Enter review  n new  f filter  r reload  Esc back  F1 help",
            },
            theme.into(),
        );
        return;
    }
    let hint = match page {
        Page::Detail => "g passage  e comment  a anchor  s start  x resolve  o reopen  r refresh",
        Page::Comment => "Enter save  Esc cancel  PgUp/PgDn scroll  F1 help",
        Page::Reanchor => "↑↓ choose  Enter confirm  Esc cancel  F1 help",
        Page::Queue => unreachable!(),
    };
    let body = wros_shell::render_panel(
        f,
        area,
        match page {
            Page::Comment => "Revision notes · Comment",
            Page::Reanchor => "Revision notes · Re-anchor",
            _ => "Revision notes · Review",
        },
        &r.notice,
        hint,
        theme.into(),
    );
    let mut lines = vec![];
    if page == Page::Comment {
        if let Some(d) = &r.draft {
            lines.push(format!("Passage: {}", plain(&d.anchor.quote)));
            lines.push(format!(
                "Comment ({} / 4096 bytes):\n{}",
                d.comment.len(),
                plain(&d.comment)
            ));
        }
    } else if let Ok(note) = r.note() {
        let v = &note.current;
        let path = r
            .store
            .as_ref()
            .and_then(|s| s.value.document(v.document_id).ok())
            .map_or("Unavailable", |d| d.path.as_str());
        lines.push(format!(
            "Note #{} · {} · Document #{}\n{}",
            note.id,
            v.status.label(),
            v.document_id,
            plain(path)
        ));
        if page == Page::Reanchor {
            if let Some(d) = &r.draft {
                lines.push(format!(
                    "{} Cancel\n{} Save reviewed anchor",
                    if r.nav.selected() == 0 { "▸" } else { " " },
                    if r.nav.selected() == 1 { "▸" } else { " " }
                ));
                lines.push(format!(
                    "From: {}\nTo: {}",
                    plain(&v.anchor.quote),
                    plain(&d.anchor.quote)
                ));
                lines.push(format!(
                    "Selected document: {}",
                    plain(&d.binding.snapshot.stamp.document)
                ));
                if path != d.binding.snapshot.stamp.document {
                    lines.push("Reconnect this document ID and ALL its notes. The old path must be missing or an owned move unresolved. Identities cannot merge.".into());
                }
                lines.push(
                    "The note reopens. Previous comment/anchor stays in metadata history.".into(),
                );
            }
        } else {
            let reconnect = r
                .store
                .as_ref()
                .and_then(|s| s.value.document(v.document_id).ok())
                .is_some_and(|d| d.needs_reconnect);
            let state = match r.binding.as_ref().filter(|b| app.review_matches(b)) {
                _ if reconnect => "Interrupted document move; explicitly re-anchor to reconnect",
                Some(b) if b.snapshot.stamp.document == path => {
                    if v.anchor.locate(b.snapshot.text()).is_some() {
                        "Exact unique passage available"
                    } else {
                        "Anchor changed or ambiguous; explicit re-anchor required"
                    }
                }
                _ => "Open the recorded source, then r to refresh",
            };
            lines.push(state.into());
            if let Some(hash) = &v.reviewed_sha256 {
                let current = r
                    .binding
                    .as_ref()
                    .filter(|b| app.review_matches(b) && b.snapshot.stamp.document == path);
                if current.is_none_or(|b| b.snapshot.stamp.sha256 != *hash) {
                    lines.push("Resolved on an older/unavailable revision; review again before relying on it.".into());
                }
            }
            lines.push(format!(
                "Passage:\n{}\n\nComment:\n{}",
                plain(&v.anchor.quote),
                plain(&v.comment)
            ));
            lines.push(format!(
                "{} prior versions retained. Esc returns; PgUp/PgDn scroll; F1 help.",
                note.previous.len()
            ));
        }
    }
    f.render_widget(
        Paragraph::new(lines.join("\n\n"))
            .wrap(Wrap { trim: false })
            .scroll((r.scroll, 0))
            .style(Style::default().fg(theme.text)),
        body,
    );
}
