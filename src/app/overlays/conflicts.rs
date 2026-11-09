//! Conflict copies — Grimoire's own and every sync app's — listed, and each
//! settled with both versions side by side: take the copy, keep the scene,
//! or keep both. Nothing is deleted; every choice undoes with Ctrl-Z.

use super::*;
use grimoire_core::merge::{self, Choice, Piece};
use grimoire_core::project::Settle;
use grimoire_core::tutorial::Event;

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
            copy: Some(copy),
            pieces,
            choices: Vec::new(),
            side: 0,
            scroll: [0, 0],
        };
    }

    pub(crate) fn open_merge_practice(&mut self) {
        let pieces = merge::three_way(
            "Mara reached the gate.\n",
            "Mara opened the gate.\n",
            "Mara locked the gate.\n",
        );
        self.overlay = Overlay::Merge {
            copy: None,
            pieces,
            choices: Vec::new(),
            side: 0,
            scroll: [0, 0],
        };
    }

    pub(super) fn on_merge_key(&mut self, key: Key) {
        let Overlay::Merge {
            copy,
            pieces,
            choices,
            side,
            scroll,
        } = &mut self.overlay
        else {
            return;
        };
        // How far the chosen version can scroll, and a page of it, as last drawn.
        let (pane, max) = self.merge_panes.get()[*side];
        let page = usize::from(pane.height.max(2) - 1);
        let at = &mut scroll[*side];
        let choice = match key {
            Key::Char('1') => Choice::Mine,
            Key::Char('2') => Choice::Theirs,
            Key::Char('3') => Choice::Both,
            Key::Tab | Key::BackTab => return *side ^= 1,
            Key::Up => return *at = at.saturating_sub(1),
            Key::Down => return *at = (*at + 1).min(max),
            Key::PageUp => return *at = at.saturating_sub(page),
            Key::PageDown => return *at = (*at + page).min(max),
            Key::Home => return *at = 0,
            Key::End => return *at = max,
            Key::Esc => {
                self.overlay = match copy.clone() {
                    Some(copy) => Overlay::Settle { copy, sel: 0 },
                    None => Overlay::Deck(wros::Page::Lesson { id: 49 }),
                };
                return;
            }
            _ => return,
        };
        choices.push(choice);
        *side = 0;
        *scroll = [0, 0];
        if choices.len() == merge::clashes(pieces) {
            let text = merge::assemble(pieces, choices);
            if let Some(copy) = copy.clone() {
                self.finish_merge(copy, text, false);
            } else {
                self.overlay = Overlay::Deck(wros::Page::Lesson { id: 49 });
                self.observe_deck(Event::MergePractised);
                self.msg = format!(
                    "Practice result: {}",
                    text.split_whitespace().collect::<Vec<_>>().join(" ")
                );
            }
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

/// Both versions of a clash, each wrapped in full and scrolled on its own, with
/// the words that differ between them marked.
pub(super) fn draw_merge(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Merge {
        copy,
        pieces,
        choices,
        side,
        scroll,
        ..
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
    let box_area = centred(area, 84, area.height.saturating_sub(4).min(32));
    f.render_widget(Clear, box_area);
    let title = if copy.is_some() {
        "MERGE"
    } else {
        "MERGE PRACTICE"
    };
    let block = pane_block(title, true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let iw = inner.width as usize;
    // Title, blank, two headings, blank and hints around the two versions.
    let room = ((inner.height as usize).saturating_sub(6) / 2).max(1);
    let row = |y: usize, h: usize| {
        Rect {
            x: inner.x,
            y: inner.y.saturating_add(y as u16),
            width: inner.width,
            height: h as u16,
        }
        .intersection(inner)
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(
                " Clash {} of {total}: WrOS and Drive changed this paragraph differently.",
                choices.len() + 1
            ),
            Style::default().fg(t.text),
        )),
        row(0, 1),
    );
    let plain = Style::default().fg(t.text);
    let marked = Style::default()
        .fg(t.warn)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let words = marked_words(&clash.mine, &clash.theirs);
    let mut panes = [(Rect::default(), 0); 2];
    let mut y = 2;
    for (i, name) in ["1  Yours", "2  Drive's"].into_iter().enumerate() {
        let wrapped = wrap_marked(&words[i], iw.saturating_sub(5), plain, marked);
        let max = wrapped.len().saturating_sub(room);
        let top = scroll[i].min(max);
        let mut head = format!("{} {name}", if *side == i { "▸" } else { " " });
        if max > 0 {
            head += &format!(
                "    lines {}–{} of {}",
                top + 1,
                (top + room).min(wrapped.len()),
                wrapped.len()
            );
        }
        let mut style = Style::default().fg(t.accent);
        if *side == i {
            style = style.add_modifier(Modifier::BOLD);
        }
        f.render_widget(Paragraph::new(Span::styled(head, style)), row(y, 1));
        let pane = row(y + 1, room);
        let shown: Vec<Line> = wrapped
            .into_iter()
            .skip(top)
            .take(room)
            .map(|mut line| {
                line.spans.insert(0, Span::raw("    "));
                line
            })
            .collect();
        f.render_widget(Paragraph::new(shown), pane);
        panes[i] = (pane, max);
        y += 1 + room;
    }
    app.merge_panes.set(panes);
    let hints = if panes.iter().any(|p| p.1 > 0) {
        " 1 yours  2 Drive's  3 both, yours first  tab ↑↓ scroll  esc back"
    } else {
        " 1 yours   2 Drive's   3 both, yours first   esc back"
    };
    f.render_widget(Paragraph::new(hint_line(hints, t)), row(y + 1, 1));
}

/// Each version's words, marked where they differ from the other version.
fn marked_words(mine: &str, theirs: &str) -> [Vec<(String, bool)>; 2] {
    use grimoire_core::history::{Piece as Diff, diff};
    let mut out = [Vec::new(), Vec::new()];
    for piece in diff(mine, theirs) {
        match piece {
            Diff::Same(s) => {
                out[0].push((s.clone(), false));
                out[1].push((s, false));
            }
            Diff::Removed(s) => out[0].push((s, true)),
            Diff::Added(s) => out[1].push((s, true)),
        }
    }
    out
}

/// Word-wrap runs of text at most `width` columns wide, marked runs in their
/// own style. Lines break at spaces and at the text's own line ends; only a
/// word wider than the whole line is cut.
fn wrap_marked(
    runs: &[(String, bool)],
    width: usize,
    plain: Style,
    marked: Style,
) -> Vec<Line<'static>> {
    let width = width.max(4);
    // Words, each made of styled pieces and starting with its space; None is a line end.
    let mut words: Vec<Option<Vec<(String, Style)>>> = vec![Some(Vec::new())];
    for (text, changed) in runs {
        let style = if *changed { marked } else { plain };
        for ch in text.chars() {
            match ch {
                '\n' => words.extend([None, Some(Vec::new())]),
                ' ' | '\t' => words.push(Some(vec![(" ".into(), style)])),
                _ => {
                    let Some(Some(word)) = words.last_mut() else {
                        continue;
                    };
                    match word.last_mut() {
                        Some((piece, s)) if *s == style => piece.push(ch),
                        _ => word.push((ch.to_string(), style)),
                    }
                }
            }
        }
    }
    let mut lines = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for word in words {
        let Some(mut word) = word else {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
            continue;
        };
        let len: usize = word.iter().map(|(p, _)| p.chars().count()).sum();
        if used > 0 && used + len > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        // A line never starts with the space between two words.
        if used == 0
            && let Some((first, _)) = word.first_mut()
            && first.starts_with(' ')
        {
            first.remove(0);
        }
        for (mut piece, style) in word {
            while used + piece.chars().count() > width {
                let head: String = piece.chars().take(width - used).collect();
                piece = piece.chars().skip(width - used).collect();
                spans.push(Span::styled(head, style));
                lines.push(Line::from(std::mem::take(&mut spans)));
                used = 0;
            }
            used += piece.chars().count();
            if !piece.is_empty() {
                spans.push(Span::styled(piece, style));
            }
        }
    }
    if !spans.is_empty() || lines.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
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
