//! Conflict copies — Grimoire's own and every sync app's — listed, and each
//! settled with both versions side by side: take the copy, keep the scene,
//! or keep both. Nothing is deleted; every choice undoes with Ctrl-Z.

use super::*;
use grimoire_core::merge::{self, Choice, Piece};
use grimoire_core::project::Settle;

/// The three ways to settle one, in the order they're offered.
const CHOICES: [Settle; 3] = [Settle::TakeCopy, Settle::KeepOriginal, Settle::KeepBoth];

impl App {
    /// Every conflict copy in the book (the trash's don't count).
    pub fn conflicts(&self) -> Vec<usize> {
        (0..self.project.nodes.len())
            .filter(|&i| self.project.nodes[i].copy_of.is_some() && !self.project.in_trash(i))
            .collect()
    }

    /// The scene a copy is a copy of, if it's in the tree.
    pub(crate) fn original_of(&self, copy: usize) -> Option<usize> {
        let n = &self.project.nodes[copy];
        let of = n.copy_of.as_ref()?;
        let want = n.path.with_file_name(format!("{}.md", of.original));
        self.project.nodes.iter().position(|m| m.path == want)
    }

    pub fn open_conflicts(&mut self) {
        self.flush();
        if self.conflicts().is_empty() {
            self.msg = "no conflicts — every scene has one version".into();
            return;
        }
        self.overlay = Overlay::Conflicts { sel: 0 };
    }

    pub(super) fn on_conflicts_key(&mut self, key: Key) {
        let list = self.conflicts();
        let Overlay::Conflicts { sel } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, list.len(), LIST) {
            return;
        }
        match key {
            Key::Enter => {
                if let Some(&copy) = list.get(*sel) {
                    self.show_conflict(copy);
                }
            }
            Key::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }

    /// Open the scene, show the copy beside it, and ask.
    fn show_conflict(&mut self, copy: usize) {
        let path = self.project.nodes[copy].path.clone();
        if let Some(orig) = self.original_of(copy) {
            if self.open != Some(orig) {
                self.reveal(orig);
                self.open_scene(orig);
            }
            if self.side_room {
                self.close_codex();
                self.beside = Some(BesidePane {
                    path: path.clone(),
                    scroll: 0,
                });
            }
        } else {
            // Its original is gone: show the copy itself.
            self.reveal(copy);
            self.open_scene(copy);
        }
        self.overlay = Overlay::Settle { copy: path, sel: 0 };
    }

    pub(super) fn on_settle_key(&mut self, key: Key) {
        let Overlay::Settle { copy, .. } = &self.overlay else {
            return;
        };
        let copy = copy.clone();
        let mergeable = self.merge_base_of(&copy).is_some();
        let Overlay::Settle { sel, .. } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, CHOICES.len() + usize::from(mergeable), RING) {
            return;
        }
        let pick = match key {
            Key::Enter => Some(*sel),
            Key::Char(c @ '1'..='4') => Some(c as usize - '1' as usize),
            _ => None,
        };
        match pick {
            Some(3) if mergeable => return self.open_merge(copy),
            Some(i) if i < CHOICES.len() => return self.settle_copy(copy, CHOICES[i]),
            _ => {}
        }
        if key == Key::Esc {
            self.close_beside_of(&copy);
            self.overlay = Overlay::Conflicts { sel: 0 };
        }
    }

    /// The private last-shared version a sync kept for this Drive copy, if any.
    pub(crate) fn merge_base_of(&self, copy: &Path) -> Option<PathBuf> {
        let root = &self.project.root;
        self.deck
            .shell
            .backup
            .sync
            .pending
            .iter()
            .find(|p| root.join(&p.parked) == copy)
            .and_then(|p| p.merge_base.as_ref().map(PathBuf::from))
    }

    /// Paragraph pieces of the scene against the Drive copy, from their shared base.
    fn merge_pieces(&self, copy: &Path) -> Option<Vec<Piece>> {
        let base = std::fs::read_to_string(self.merge_base_of(copy)?).ok()?;
        let ci = self.project.nodes.iter().position(|n| n.path == copy)?;
        let original = self.original_of(ci)?;
        let mine = std::fs::read_to_string(&self.project.nodes[original].path).ok()?;
        let theirs = std::fs::read_to_string(copy).ok()?;
        Some(merge::three_way(&base, &mine, &theirs))
    }

    fn open_merge(&mut self, copy: PathBuf) {
        // Build "Mine" from the saved editor buffers, rather than the older
        // disk text that settling would otherwise overwrite those words with.
        if !self.commit_saves() {
            return;
        }
        if self.project.dirty_count() != 0 || self.tree_stale {
            self.msg = "merge is waiting for saved words and source changes to be re-read".into();
            return;
        }
        let Some(pieces) = self.merge_pieces(&copy) else {
            self.msg = "this copy can't be merged; take it, keep the scene, or keep both".into();
            return;
        };
        if merge::clashes(&pieces) == 0 {
            let text = merge::assemble(&pieces, &[]);
            return self.finish_merge(copy, text, false);
        }
        self.overlay = Overlay::Merge {
            copy,
            pieces,
            choices: Vec::new(),
        };
    }

    pub(super) fn on_merge_key(&mut self, key: Key) {
        let Overlay::Merge {
            copy,
            pieces,
            choices,
        } = &mut self.overlay
        else {
            return;
        };
        let choice = match key {
            Key::Char('1') => Choice::Mine,
            Key::Char('2') => Choice::Theirs,
            Key::Char('3') => Choice::Both,
            Key::Esc => {
                let copy = copy.clone();
                self.overlay = Overlay::Settle { copy, sel: 0 };
                return;
            }
            _ => return,
        };
        choices.push(choice);
        if choices.len() == merge::clashes(pieces) {
            let text = merge::assemble(pieces, choices);
            let copy = copy.clone();
            self.finish_merge(copy, text, false);
        }
    }

    /// The merged text goes into the copy, then the copy is taken: the scene's
    /// own words go to its history first, and one Ctrl-Z undoes it all.
    pub(crate) fn finish_merge(&mut self, copy: PathBuf, text: String, automatic: bool) {
        if let Err(e) = std::fs::write(&copy, &text) {
            self.msg = format!("left both as they were: {e}");
            return;
        }
        let before = self.overlay.clone();
        self.settle_copy(copy.clone(), Settle::TakeCopy);
        if !copy.exists() {
            let title = self
                .project
                .nodes
                .iter()
                .position(|n| n.path.with_extension("") == copy.with_extension(""))
                .map(|i| self.project.nodes[i].title.clone());
            let root = self.project.root.clone();
            self.deck
                .shell
                .backup
                .sync
                .pending
                .retain(|p| root.join(&p.parked) != copy);
            let m = self.mod_label();
            self.msg = format!(
                "merged Drive's edits{} · {m}Z undoes",
                title.map_or_else(String::new, |t| format!(" into {t}"))
            );
        }
        if automatic {
            self.overlay = before;
        }
    }

    fn close_beside_of(&mut self, copy: &Path) {
        if self.beside.as_ref().is_some_and(|b| b.path == copy) {
            self.close_beside();
        }
    }

    /// Settle the copy at `copy` the way chosen, and make it one undo step.
    pub(crate) fn settle_copy(&mut self, copy: PathBuf, how: Settle) {
        self.flush();
        if !self.commit_saves() {
            return;
        }
        let Some(ci) = self.project.nodes.iter().position(|n| n.path == copy) else {
            self.overlay = Overlay::None;
            self.msg = "that copy isn't there any more".into();
            return;
        };
        let label = self.project.nodes[ci]
            .copy_of
            .as_ref()
            .map_or("copy", |c| c.source.label())
            .to_string();
        let original = self.original_of(ci);
        let title = original.map_or_else(
            || self.project.nodes[ci].title.clone(),
            |o| self.project.nodes[o].title.clone(),
        );
        let seen = original.and_then(|o| self.project.nodes[o].seen);
        let root = self.project.root.clone();
        self.close_beside_of(&copy);
        let done = match project::settle(&root, &copy, how, seen) {
            Ok(done) => done,
            Err(e) => {
                self.overlay = Overlay::None;
                self.msg = format!("left both as they were: {e}");
                return;
            }
        };
        self.close_if_trashed(&done.moves);
        self.follow_many(&done.moves);
        let m = self.mod_label();
        let (what, said) = match how {
            Settle::TakeCopy => (
                format!("take the {label} of {title}"),
                format!(
                    "{title} has the {label}'s words now — its own are in its history (H) · {m}Z undoes"
                ),
            ),
            Settle::KeepOriginal => (
                format!("keep {title} over its {label}"),
                format!("kept {title} as it was — the {label} is in the trash · {m}Z undoes"),
            ),
            Settle::KeepBoth => (
                format!("keep both versions of {title}"),
                format!(
                    "kept both — the {label} is a scene of its own, right after it · {m}Z undoes"
                ),
            ),
        };
        let kept = done.kept.clone();
        self.record(TreeStep::Settled {
            what,
            moves: done.moves,
            texts: done.texts,
            links: done.links,
        });
        if let Err(e) = self.reload_tree() {
            self.overlay = Overlay::None;
            self.msg = format!("settled, but couldn't re-read the tree: {e}");
            return;
        }
        if let Some(i) = kept.and_then(|k| self.project.nodes.iter().position(|n| n.path == k)) {
            self.reveal(i);
        }
        // More to settle: back to the list; otherwise done.
        self.overlay = if self.conflicts().is_empty() {
            Overlay::None
        } else {
            Overlay::Conflicts { sel: 0 }
        };
        self.msg = said;
    }
}

pub(super) fn draw_merge(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Merge {
        pieces, choices, ..
    } = &app.overlay
    else {
        return;
    };
    let total = merge::clashes(pieces);
    let Some(clash) = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Clash(c) => Some(c),
            Piece::Same(_) => None,
        })
        .nth(choices.len())
    else {
        return;
    };
    let box_area = centred(area, 84, area.height.saturating_sub(4).min(24));
    f.render_widget(Clear, box_area);
    let block = pane_block("MERGE", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let iw = inner.width as usize;
    let room = (inner.height as usize).saturating_sub(7) / 2;
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                " Clash {} of {total}: WrOS and Drive changed this paragraph differently.",
                choices.len() + 1
            ),
            Style::default().fg(t.text),
        )),
        Line::from(""),
        Line::from(Span::styled(" 1  Yours", Style::default().fg(t.accent))),
    ];
    for text in [&clash.mine, &clash.theirs] {
        for line in text.lines().take(room.max(1)) {
            lines.push(Line::from(Span::styled(
                format!("    {}", truncate(line, iw.saturating_sub(5))),
                Style::default().fg(t.text),
            )));
        }
        if std::ptr::eq(text, &clash.mine) {
            lines.push(Line::from(Span::styled(
                " 2  Drive's",
                Style::default().fg(t.accent),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(hint_line(
        " 1 yours   2 Drive's   3 both, yours first   esc back",
        t,
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

/// How much longer or shorter the copy is than its scene, in words.
fn difference(app: &App, copy: usize) -> String {
    let theirs = app.project.nodes[copy].words() as i64;
    let Some(orig) = app.original_of(copy) else {
        return "its scene is gone".into();
    };
    let ours = app.project.nodes[orig].words() as i64;
    match theirs - ours {
        0 => "same length".into(),
        d if d > 0 => format!("{d} word{} longer", if d == 1 { "" } else { "s" }),
        d => format!("{} word{} shorter", -d, if d == -1 { "" } else { "s" }),
    }
}

fn modified(app: &App, i: usize) -> String {
    std::fs::metadata(&app.project.nodes[i].path)
        .and_then(|m| m.modified())
        .map(|t| when_label(chrono::DateTime::<chrono::Local>::from(t)))
        .unwrap_or_default()
}

pub(super) fn draw_conflicts(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Conflicts { sel } = &app.overlay else {
        return;
    };
    let list = app.conflicts();
    let rows = list
        .len()
        .min(area.height.saturating_sub(10) as usize)
        .max(1);
    let box_area = centred(area, 78, rows as u16 + 8);
    f.render_widget(Clear, box_area);
    let block = pane_block("SETTLE CONFLICTS", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let iw = inner.width as usize;

    let mut lines = vec![
        Line::from(Span::styled(
            " Two devices wrote the same scene. Each copy below is the version",
            Style::default().fg(t.text),
        )),
        Line::from(Span::styled(
            " that lost — nothing is gone until you choose, and never after.",
            Style::default().fg(t.dim),
        )),
        Line::from(""),
    ];
    let start = (*sel + 1).saturating_sub(rows);
    for (k, &i) in list.iter().enumerate().skip(start).take(rows) {
        let on = k == *sel;
        let title = app.original_of(i).map_or_else(
            || app.project.nodes[i].title.clone(),
            |o| app.project.nodes[o].title.clone(),
        );
        let label = app.project.nodes[i]
            .copy_of
            .as_ref()
            .map_or("copy", |c| c.source.label());
        let detail = format!("{} · {}", modified(app, i), difference(app, i));
        let left = format!(" {} {title} — {label}", if on { "▸" } else { " " });
        let room = iw.saturating_sub(left.chars().count() + 2);
        lines.push(
            Line::from(vec![
                Span::styled(
                    left,
                    Style::default().fg(if on { t.accent } else { t.text }),
                ),
                Span::styled(
                    format!("  {}", truncate(&detail, room)),
                    Style::default().fg(t.dim),
                ),
            ])
            .style(if on {
                Style::default().bg(t.sel)
            } else {
                Style::default()
            }),
        );
    }
    lines.push(Line::from(""));
    lines.push(hint_line(" ↑↓ choose   ↵ see both   esc close", t));
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_settle(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Settle { copy, sel } = &app.overlay else {
        return;
    };
    let Some(ci) = app.project.nodes.iter().position(|n| &n.path == copy) else {
        return;
    };
    let label = app.project.nodes[ci]
        .copy_of
        .as_ref()
        .map_or("copy", |c| c.source.label());
    let title = app.original_of(ci).map_or_else(
        || app.project.nodes[ci].title.clone(),
        |o| app.project.nodes[o].title.clone(),
    );
    // Low on the screen, so both versions stay readable above it.
    let w = area.width.saturating_sub(4).min(84);
    let h = 11u16.min(area.height);
    let box_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.bottom().saturating_sub(h + 1),
        w,
        h,
    );
    f.render_widget(Clear, box_area);
    let block = pane_block("SETTLE", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let iw = inner.width as usize;
    let side = app.beside.as_ref().is_some_and(|b| &b.path == copy);
    let intro = if side {
        format!(
            " {title} on the left, the {label} on the right ({}).",
            difference(app, ci)
        )
    } else {
        format!(" The {label} of {title} ({}).", difference(app, ci))
    };
    let mut choices = vec![
        (
            format!("Take the {label}"),
            "the scene's own words go to its history",
        ),
        (
            "Keep the scene as it is".to_string(),
            "the copy goes to the trash",
        ),
        (
            "Keep both".to_string(),
            "the copy becomes a scene of its own, right after it",
        ),
    ];
    if app.merge_base_of(copy).is_some() {
        choices.push((
            "Merge paragraph by paragraph".to_string(),
            "keep each side's changes; choose where both changed",
        ));
    }
    let mut lines = vec![
        Line::from(Span::styled(
            truncate(&intro, iw),
            Style::default().fg(t.text),
        )),
        Line::from(""),
    ];
    for (k, (what, why)) in choices.iter().enumerate() {
        let on = k == *sel;
        let lead = format!(" {} {}  {what}", if on { "▸" } else { " " }, k + 1);
        let room = iw.saturating_sub(lead.chars().count() + 3);
        lines.push(
            Line::from(vec![
                Span::styled(
                    lead,
                    Style::default().fg(if on { t.accent } else { t.text }),
                ),
                Span::styled(
                    format!(" — {}", truncate(why, room)),
                    Style::default().fg(t.dim),
                ),
            ])
            .style(if on {
                Style::default().bg(t.sel)
            } else {
                Style::default()
            }),
        );
    }
    lines.push(Line::from(""));
    let m = app.mod_label();
    lines.push(hint_line(
        &format!(" ↑↓ choose   ↵ do it   esc back   {m}Z undoes it after"),
        t,
    ));
    f.render_widget(Paragraph::new(lines), inner);
}
