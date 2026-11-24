//! Books: starting a new one, bringing in a draft written elsewhere, and
//! switching to another. The book that's open is saved and put away first,
//! just as quitting would, and the music, the Pomodoro and the look carry
//! across.

use grimoire_core::import;
use grimoire_core::shelf::{find_books, project_id_of, title_of};

use super::*;

/// What a typed folder would do, said before ↵ is pressed.
enum Where {
    Nothing,
    /// Free, or an empty folder: a new book goes here.
    Free(PathBuf),
    /// Already a book.
    Book(PathBuf),
    /// A folder with other things in it, or a file that isn't a draft.
    Taken(PathBuf),
    /// A Word, Markdown or text file to bring in as a new book.
    Draft(PathBuf),
}

fn look(beside: &Path, typed: &str) -> Where {
    let Some(p) = books::typed_path(beside, typed) else {
        return Where::Nothing;
    };
    if books::is_book(&p) {
        Where::Book(p)
    } else if import::is_draft(&p) {
        Where::Draft(p)
    } else if p.is_file() || fs::read_dir(&p).is_ok_and(|mut d| d.next().is_some()) {
        Where::Taken(p)
    } else {
        Where::Free(p)
    }
}

/// Typing into a one-line box: printable keys add, Backspace takes away.
fn edit(buf: &mut String, key: Key) {
    match key {
        Key::Char(c) if !c.is_control() && buf.chars().count() < 120 => buf.push(c),
        Key::Backspace => {
            buf.pop();
        }
        _ => {}
    }
}

impl App {
    /// Documents books, newest opened first then by title, followed by recent
    /// books elsewhere. Import and new-book rows finish the shelf.
    pub fn shelf_rows(&self) -> Vec<ShelfRow> {
        if let Some((at, rows)) = self.shelf_cache.borrow().as_ref()
            && at.elapsed() < std::time::Duration::from_secs(2)
        {
            return rows.clone();
        }
        let rows = self.read_shelf_rows();
        *self.shelf_cache.borrow_mut() = Some((Instant::now(), rows.clone()));
        rows
    }

    fn read_shelf_rows(&self) -> Vec<ShelfRow> {
        let documents = books::documents();
        let documents = fs::canonicalize(&documents).unwrap_or(documents);
        let here = fs::canonicalize(&self.project.root).unwrap_or(self.project.root.clone());
        let recent: Vec<PathBuf> = self
            .state_file
            .as_ref()
            .map(|state| books::recent(state))
            .unwrap_or_default()
            .into_iter()
            .map(|root| fs::canonicalize(&root).unwrap_or(root))
            .collect();
        let book = |root: PathBuf| ShelfBook {
            title: title_of(&root),
            project_id: project_id_of(&root),
            root,
        };
        let mut found = find_books(&documents, 3, 500);
        if here.starts_with(&documents) && !found.iter().any(|book| book.root == here) {
            found.push(book(here.clone()));
        }
        found.sort_by_cached_key(|book| {
            (
                recent
                    .iter()
                    .position(|root| *root == book.root)
                    .unwrap_or(usize::MAX),
                book.title.to_lowercase(),
                book.root.clone(),
            )
        });
        let mut rows: Vec<ShelfRow> = found
            .into_iter()
            .map(|book| {
                let open = book.root == here;
                ShelfRow::Book(book, open)
            })
            .collect();
        for root in recent.iter().chain(std::iter::once(&here)) {
            if !root.starts_with(&documents)
                && !rows
                    .iter()
                    .any(|row| matches!(row, ShelfRow::Book(book, _) if book.root == *root))
            {
                rows.push(ShelfRow::Book(book(root.clone()), *root == here));
            }
        }
        rows.push(ShelfRow::Import);
        rows.push(ShelfRow::NewBook);
        rows
    }

    /// Other shelf books are also available by name in the command palette.
    pub fn other_books(&self) -> Vec<PathBuf> {
        self.shelf_rows()
            .into_iter()
            .filter_map(|row| match row {
                ShelfRow::Book(book, false) => Some(book.root),
                _ => None,
            })
            .collect()
    }

    pub(super) fn on_new_book_key(&mut self, key: Key) {
        let Overlay::NewBook { buf, shape } = &mut self.overlay else {
            return;
        };
        let all = project::Template::ALL;
        let at = all.iter().position(|t| t == shape).unwrap_or(0);
        match key {
            Key::Enter => {
                let (typed, shape) = (buf.clone(), *shape);
                self.create_book(&typed, shape);
            }
            // Tab steps through the shapes; the name stays as typed.
            Key::Tab => *shape = all[(at + 1) % all.len()],
            Key::BackTab => *shape = all[(at + all.len() - 1) % all.len()],
            Key::Esc => self.overlay = Overlay::None,
            k => edit(buf, k),
        }
    }

    pub(super) fn on_books_key(&mut self, key: Key) {
        let rows = self.shelf_rows();
        let Overlay::Books { sel } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, rows.len(), RING) {
            return;
        }
        match key {
            Key::Enter | Key::Char(' ') | Key::Right | Key::Char('l') => match rows.get(*sel) {
                Some(ShelfRow::Book(book, _)) => {
                    let root = book.root.clone();
                    self.overlay = Overlay::None;
                    let _ = self.open_book(&root);
                }
                Some(ShelfRow::Import) => {
                    if self.background {
                        self.deck
                            .shell
                            .backup
                            .request(crate::backup_service::Request::Status);
                    }
                    self.open_import();
                }
                Some(ShelfRow::NewBook) => {
                    self.overlay = Overlay::NewBook {
                        buf: String::new(),
                        shape: project::Template::Novel,
                    };
                }
                None => {}
            },
            Key::Char('x') | Key::Delete => {
                let at = *sel;
                match rows.get(at) {
                    Some(ShelfRow::Book(book, false)) => {
                        self.overlay = Overlay::DeleteBook {
                            root: book.root.clone(),
                            typed: String::new(),
                            yes: false,
                            back: at,
                        };
                    }
                    Some(ShelfRow::Book(_, true)) => {
                        self.msg =
                            "that's the project you have open · open another project first".into();
                    }
                    _ => {}
                }
            }
            Key::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }

    pub(super) fn on_delete_book_key(&mut self, key: Key) {
        let Overlay::DeleteBook {
            root,
            typed,
            yes,
            back,
        } = &mut self.overlay
        else {
            return;
        };
        let demo = grimoire_core::tutorial::is_demo(root);
        match key {
            Key::Esc => {
                self.overlay = Overlay::Books { sel: *back };
            }
            Key::Left | Key::Right | Key::Tab | Key::BackTab => *yes = !*yes,
            Key::Enter if !*yes => self.overlay = Overlay::Books { sel: *back },
            Key::Enter => {
                if !demo && typed.trim() != books::title_of_folder(root) {
                    self.msg =
                        format!("type {} exactly to delete it", books::title_of_folder(root));
                    return;
                }
                let (root, back) = (root.clone(), *back);
                match self.delete_book(&root) {
                    Ok(()) => {
                        self.shelf_cache.take();
                        self.msg = format!(
                            "deleted {} ({}) from this laptop",
                            books::title_of_folder(&root),
                            books::pretty(&root)
                        );
                        let left = self.shelf_rows().len();
                        self.overlay = Overlay::Books {
                            sel: back.min(left - 1),
                        };
                    }
                    Err(why) => {
                        self.msg = why;
                        self.overlay = Overlay::Books { sel: back };
                    }
                }
            }
            Key::Backspace if !demo => {
                typed.pop();
            }
            Key::Char(c) if !demo && typed.chars().count() < 120 => typed.push(c),
            _ => {}
        }
    }

    /// Remove a book that isn't open, folder and all. It is first renamed to a
    /// hidden name beside itself, so a half-finished delete never looks like a book.
    fn delete_book(&mut self, root: &Path) -> Result<(), String> {
        let shown = books::pretty(root);
        if self.deck.shell.backup.job.is_some() {
            return Err("a backup is running · delete the project when it finishes".into());
        }
        let meta = fs::symlink_metadata(root).map_err(|_| format!("{shown} is already gone"))?;
        if meta.file_type().is_symlink() || !meta.is_dir() || !books::is_book(root) {
            return Err(format!(
                "{shown} isn't a project folder, so it was left alone"
            ));
        }
        let real = fs::canonicalize(root).map_err(|e| format!("couldn't delete {shown}: {e}"))?;
        let here = fs::canonicalize(&self.project.root).unwrap_or(self.project.root.clone());
        if here.starts_with(&real) {
            return Err("that's the project you have open · open another project first".into());
        }
        let home = fs::canonicalize(grimoire_core::paths::home()).ok();
        let (Some(parent), Some(name)) = (real.parent(), real.file_name()) else {
            return Err(format!("{shown} can't be deleted from here"));
        };
        if home.as_ref().is_some_and(|h| h.starts_with(&real)) {
            return Err(format!(
                "{shown} holds your home folder, so it was left alone"
            ));
        }
        let hidden = parent.join(format!(
            ".deleting-{}-{}",
            name.to_string_lossy(),
            std::process::id()
        ));
        fs::rename(&real, &hidden).map_err(|e| format!("couldn't delete {shown}: {e}"))?;
        if let Some(state) = &self.state_file {
            books::forget(state, root);
            books::forget(state, &real);
        }
        // A queued backup of a book that no longer exists would stop the whole queue.
        let project = real.to_string_lossy().into_owned();
        let backup = &mut self.deck.shell.backup;
        if backup
            .status
            .queued_backups
            .iter()
            .any(|q| q.project == project)
        {
            backup.request(crate::backup_service::Request::Forget { project });
        }
        fs::remove_dir_all(&hidden).map_err(|e| {
            format!(
                "{shown} is no longer a project, but part of it is still in {}: {e}",
                books::pretty(&hidden)
            )
        })
    }

    /// Start a whole new book from what was typed, and switch to it.
    fn create_book(&mut self, typed: &str, shape: project::Template) {
        let root = match look(&self.project.root, typed) {
            Where::Nothing => return,
            Where::Taken(p) => {
                self.msg = format!(
                    "{} already has other things in it · try another name",
                    books::pretty(&p)
                );
                return;
            }
            Where::Book(p) => {
                self.overlay = Overlay::None;
                if self.open_book(&p) {
                    self.msg = format!(
                        "there was already a project at {}, so here it is",
                        books::pretty(&p)
                    );
                }
                return;
            }
            Where::Draft(file) => {
                self.import_draft(&file);
                return;
            }
            Where::Free(p) => p,
        };
        if let Err(e) = fs::create_dir_all(&root)
            .map_err(anyhow::Error::from)
            .and_then(|()| shape.scaffold(&root, None))
        {
            self.msg = format!(
                "couldn't start a project at {}: {e:#}",
                books::pretty(&root)
            );
            return;
        }
        self.observe_deck(grimoire_core::tutorial::Event::NovelCreated);
        let completed_lesson_3 = self.deck.progress.completed.contains(&3);
        self.overlay = Overlay::None;
        if self.open_book(&root) {
            if completed_lesson_3 {
                self.deck.progress.completed.insert(3);
                if let Ok(mut store) = grimoire_core::tutorial::Store::load(&self.project.root) {
                    store.progress.completed.insert(3);
                    let _ = store.save();
                }
            }
            self.msg = if shape == project::Template::Blank {
                format!(
                    "{} is ready (blank) · press n to start a scene",
                    self.project.meta.title
                )
            } else {
                format!(
                    "{} is ready ({}) · this first page shows how a project is laid out \
                     · Tab to read it, Esc for the menu",
                    self.project.meta.title,
                    shape.name().to_lowercase()
                )
            };
        }
    }

    /// Put this book away and open the one at `root` in its place. Anything
    /// that stops it (a save that can't happen, a folder that isn't a book)
    /// leaves this book open and says why. True if it switched.
    pub fn open_book(&mut self, root: &Path) -> bool {
        match self.switch_book(root) {
            Ok(switched) => switched,
            Err(why) => {
                self.msg = why;
                false
            }
        }
    }

    fn switch_book(&mut self, root: &Path) -> Result<bool, String> {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let here = fs::canonicalize(&self.project.root).unwrap_or(self.project.root.clone());
        if root == here {
            self.msg = "that's the project you have open".into();
            return Ok(false);
        }
        if !books::is_book(&root) {
            return Err(format!(
                "{} isn't a project: there's no manuscript folder in it",
                books::pretty(&root)
            ));
        }
        if !self.try_quit() {
            return Err(format!("{} · so this project stays open for now", self.msg));
        }
        let stranded = project::restore_stranded(&root);
        let updated = project::upgrade(&root).unwrap_or_else(|e| {
            vec![format!(
                "not all of it could be brought up to date yet ({e:#})"
            )]
        });
        let cant = |e: anyhow::Error| format!("couldn't open {}: {e:#}", books::pretty(&root));
        let project = Project::load(&root).map_err(cant)?;
        let setup = Setup {
            // Inert: the music already playing comes across instead.
            music: music::Config::default(),
            theme: self.theme.clone(),
            settings: Settings {
                spellcheck: self.spell_on,
                icons: self.icons_on,
                line_width: self.line_width,
                typewriter: self.typewriter,
                spelling: self.spell_lang.key().into(),
                ..Settings::default()
            },
            background: self.background,
        };
        let mut next = App::with(project, setup).map_err(cant)?;
        next.brain = std::mem::take(&mut self.brain);
        if let (Some(next), Some(previous)) = (&mut next.timeline, &self.timeline) {
            next.carry_notice(previous);
        }
        if self.import_media.mounted || self.import_media.waiting {
            next.deck.carry_import_worker(&mut self.deck);
        }
        next.import_media = std::mem::take(&mut self.import_media);
        next.deck.shell.backup.sync.script = self.deck.shell.backup.sync.script.take();
        next.deck.shell.backup.sync.env = std::mem::take(&mut self.deck.shell.backup.sync.env);
        std::mem::swap(&mut next.music, &mut self.music);
        std::mem::swap(&mut next.viz, &mut self.viz);
        std::mem::swap(&mut next.pomo, &mut self.pomo);
        next.pane_mode = self.pane_mode;
        next.scene_visible = self.scene_visible;
        next.wros_music = self.wros_music;
        next.super_keys = self.super_keys;
        next.screen = self.screen;
        next.frame = self.frame;
        next.state_file = self.state_file.take();
        if self.speller.is_some() {
            next.speller = self.speller.take();
            next.speller_rx = None;
        }
        let left = std::mem::replace(self, next).project.root;
        if let Some(state) = &self.state_file {
            // The one left behind goes on the list too, to come back to.
            books::remember(state, &left);
            books::remember(state, &root);
        }
        if !stranded.is_empty() {
            self.msg = App::stranded_note(&stranded);
        } else if !updated.is_empty() {
            self.msg = format!("project updated: {}", updated.join(" · "));
        } else if self.msg.is_empty() {
            self.msg = format!("opened {}", self.project.meta.title);
        } else {
            self.msg = format!("opened {} · {}", self.project.meta.title, self.msg);
        }
        Ok(true)
    }
}

/// `p` as ~/…, cut down from the left to `width` characters.
fn short_path(p: &Path, width: usize) -> String {
    let s = books::pretty(p);
    let n = s.chars().count();
    if n <= width {
        return s;
    }
    let tail: String = s.chars().skip(n + 1 - width).collect();
    format!("…{tail}")
}

/// One line of typing, with where it will go (or what's there) under it,
/// and the keys that work.
fn typed_box(
    f: &mut Frame,
    area: Rect,
    t: &Theme,
    (title, lead): (&str, &str),
    buf: &str,
    (below, keys): (Line, &str),
    extra: Vec<Line>,
) {
    let box_area = centred(area, 64, 9 + extra.len() as u16);
    f.render_widget(Clear, box_area);
    let block = pane_block(title, true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    // A long path shows its end, where the typing is.
    let room = (inner.width as usize).saturating_sub(5);
    let n = buf.chars().count();
    let shown: String = if n > room {
        format!("…{}", buf.chars().skip(n + 1 - room).collect::<String>())
    } else {
        buf.to_string()
    };
    let lines = vec![
        Line::from(Span::styled(format!(" {lead}"), Style::default().fg(t.dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" ▸ ", Style::default().fg(t.accent)),
            Span::styled(shown, Style::default().fg(t.text)),
            Span::styled("█", Style::default().fg(t.accent)),
        ]),
        Line::from(""),
        below,
    ]
    .into_iter()
    .chain(extra)
    .chain([Line::from(""), hint_line(keys, t)])
    .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_new_book(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::NewBook { buf, shape } = &app.overlay else {
        return;
    };
    let dim = Style::default().fg(t.dim);
    let found = look(&app.project.root, buf);
    // The shapes, one chosen, and what the chosen one starts with; a draft
    // brings its own shape, so they step aside for one.
    let mut extra = Vec::new();
    if !matches!(found, Where::Draft(_) | Where::Book(_)) {
        extra.push(Line::from(""));
        use project::Template::{Blank, Essay, Novel, ShortStory};
        for (group, shapes) in [
            ("Fiction", &[Novel, ShortStory][..]),
            ("Non-fiction", &[Essay][..]),
            ("", &[Blank][..]),
        ] {
            let mut row = vec![Span::styled(format!(" {group:<12} "), dim)];
            for template in shapes {
                let on = *template == *shape;
                row.push(Span::styled(
                    format!("{} {}   ", if on { "●" } else { "○" }, template.name()),
                    if on {
                        Style::default().fg(t.accent)
                    } else {
                        dim
                    },
                ));
            }
            extra.push(Line::from(row));
        }
        let mut line = String::new();
        let mut said = Vec::new();
        for word in shape.about().split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > 58 {
                said.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        said.push(line);
        for l in said {
            extra.push(Line::from(Span::styled(
                format!(" {l}"),
                Style::default().fg(t.dim).add_modifier(Modifier::ITALIC),
            )));
        }
    }
    let under = match found {
        Where::Nothing => (Line::from(""), " type a name   tab shape   esc cancel"),
        Where::Free(p) => (
            Line::from(vec![
                Span::styled(" it'll live in ", dim),
                Span::styled(short_path(&p, 46), Style::default().fg(t.accent)),
            ]),
            " ↵ start it   tab shape   esc cancel",
        ),
        Where::Book(p) => (
            Line::from(Span::styled(
                format!(" there's already a project there: {}", books::title(&p)),
                dim,
            )),
            " ↵ open that one   esc cancel",
        ),
        Where::Taken(_) => (
            Line::from(Span::styled(
                " that folder is already there, and has other things in it",
                Style::default().fg(t.warn),
            )),
            " try another name   esc cancel",
        ),
        Where::Draft(p) => draft_line(&p, t),
    };
    typed_box(
        f,
        area,
        t,
        (
            "NEW PROJECT",
            "What's it called? Or a draft's path, to bring it in.",
        ),
        buf,
        under,
        extra,
    );
}

/// A draft that's there to bring in, by its file name.
fn draft_line<'a>(p: &Path, t: &Theme) -> (Line<'a>, &'static str) {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    (
        Line::from(vec![
            Span::styled(" ✓ ", Style::default().fg(t.accent)),
            Span::styled(
                name.chars().take(52).collect::<String>(),
                Style::default().fg(t.text),
            ),
        ]),
        " ↵ bring it in as a new project   esc cancel",
    )
}

pub(super) fn draw_books(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Books { sel } = &app.overlay else {
        return;
    };
    let rows = app.shelf_rows();
    let can_delete = matches!(rows.get(*sel), Some(ShelfRow::Book(_, false)));
    let rows: Vec<(String, String, bool)> = rows
        .iter()
        .map(|row| match row {
            ShelfRow::Book(book, open) => {
                let name = if grimoire_core::tutorial::is_demo(&book.root) {
                    format!("{} · tutorial demo", books::title_of_folder(&book.root))
                } else {
                    book.title.clone()
                };
                (name, books::pretty(&book.root), *open)
            }
            ShelfRow::Import => ("Import a project…".into(), String::new(), false),
            ShelfRow::NewBook => ("New project…".into(), String::new(), false),
        })
        .collect();
    let width = 112.min(area.width.saturating_sub(4));
    let box_area = centred(area, width, rows.len() as u16 + 4);
    f.render_widget(Clear, box_area);
    let block = pane_block("YOUR PROJECTS", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let mut lines: Vec<Line> = Vec::new();
    let room = (inner.height as usize).saturating_sub(2).max(1);
    let start = (*sel + 1).saturating_sub(room);
    let name_w = (inner.width as usize).saturating_sub(4) / 2;
    for (i, (name, path, open)) in rows.iter().enumerate().skip(start).take(room) {
        let on = i == *sel;
        let suffix = if *open { " · open now" } else { "" };
        let name: String = name
            .chars()
            .take(name_w.saturating_sub(suffix.chars().count()))
            .chain(suffix.chars())
            .collect();
        lines.push(
            Line::from(vec![
                Span::styled(
                    if on { " ▸ " } else { "   " },
                    Style::default().fg(if on { t.accent } else { t.dim }),
                ),
                Span::styled(
                    format!("{name:<w$} ", w = name_w),
                    Style::default().fg(if on { t.accent } else { t.text }),
                ),
                Span::styled(path.clone(), Style::default().fg(t.dim)),
            ])
            .style(if on {
                Style::default().bg(t.sel)
            } else {
                Style::default()
            }),
        );
    }
    lines.push(Line::from(""));
    lines.push(hint_line(
        if can_delete {
            " j/k move   ↵ open   x delete   esc close"
        } else {
            " j/k move   ↵ open   esc close"
        },
        t,
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_delete_book(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::DeleteBook {
        root, typed, yes, ..
    } = &app.overlay
    else {
        return;
    };
    let demo = grimoire_core::tutorial::is_demo(root);
    let folder = books::title_of_folder(root);
    let width = 64u16.min(area.width.saturating_sub(2)).max(30);
    let text_w = width.saturating_sub(4) as usize;
    let dim = Style::default().fg(t.dim);
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!(
            " {}{}",
            books::title(root),
            if demo { " · tutorial demo" } else { "" }
        ),
        Style::default().fg(t.text),
    ))];
    lines.push(Line::from(Span::styled(
        format!(" {}", books::pretty(root)),
        dim,
    )));
    lines.push(Line::from(""));
    for row in crate::ui::wrap_words(
        "This removes the project's folder and everything in it from this laptop. \
         It can't be undone here. Backups and Google Drive copies are kept.",
        text_w,
    ) {
        lines.push(Line::from(Span::styled(format!(" {row}"), dim)));
    }
    if !demo {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" Type {folder} to delete it:"),
            Style::default().fg(t.text),
        )));
        lines.push(Line::from(Span::styled(
            format!(" {typed}▏"),
            Style::default().fg(t.accent),
        )));
    }
    lines.push(Line::from(""));
    let pick = |on: bool, label: &str| {
        Span::styled(
            if on {
                format!("[{label}]")
            } else {
                format!(" {label} ")
            },
            if on {
                Style::default().fg(t.accent).bg(t.sel)
            } else {
                Style::default().fg(t.text)
            },
        )
    };
    lines.push(Line::from(vec![
        Span::raw(" "),
        pick(!*yes, "Keep it"),
        Span::raw("    "),
        pick(*yes, "Delete it"),
    ]));
    lines.push(Line::from(""));
    lines.push(hint_line(" ←→ choose   ↵ confirm   esc keep it", t));
    let box_area = centred(area, width, lines.len() as u16 + 2);
    f.render_widget(Clear, box_area);
    let block = pane_block("DELETE THIS PROJECT?", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    f.render_widget(Paragraph::new(lines), inner);
}
