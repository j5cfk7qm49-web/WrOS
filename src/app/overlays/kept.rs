//! Where this project is kept: the folder it is, what's in it, whether there's
//! a copy of it anywhere, and the things to do about that. A project is a
//! folder of plain files, which is easy to lose track of; this is the one
//! place that says where it is and makes it one file when one file is wanted.

use super::*;
use grimoire_core::shelf;

/// What can be done from the box, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeptRow {
    OpenFolder,
    SaveCopy,
    ShowCopies,
    MoveHome,
}

impl KeptRow {
    fn label(self) -> &'static str {
        match self {
            KeptRow::OpenFolder => "Open the folder",
            KeptRow::SaveCopy => "Save a copy: the whole project as one file",
            KeptRow::ShowCopies => "Show the copies",
            KeptRow::MoveHome => "Move it to your projects folder",
        }
    }

    fn key(self) -> char {
        match self {
            KeptRow::OpenFolder => 'o',
            KeptRow::SaveCopy => 's',
            KeptRow::ShowCopies => 'c',
            KeptRow::MoveHome => 'm',
        }
    }
}

impl App {
    /// Is this project in the projects folder already?
    fn kept_at_home(&self) -> bool {
        let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let root = real(&self.project.root);
        let home = real(&self.books_home);
        root.parent() == Some(home.as_path())
    }

    pub(crate) fn kept_rows(&self) -> Vec<KeptRow> {
        let mut rows = vec![KeptRow::OpenFolder, KeptRow::SaveCopy];
        if shelf::copies_dir(&self.books_home).is_dir() {
            rows.push(KeptRow::ShowCopies);
        }
        if !self.kept_at_home() {
            rows.push(KeptRow::MoveHome);
        }
        rows
    }

    pub fn open_kept(&mut self) {
        let facts = shelf::facts(&self.project.root);
        self.overlay = Overlay::Kept {
            sel: 0,
            files: facts.files,
            bytes: facts.bytes,
        };
    }

    pub(super) fn on_kept_key(&mut self, key: Key) {
        let rows = self.kept_rows();
        let Overlay::Kept { sel, .. } = &mut self.overlay else {
            return;
        };
        if list_nav(key, sel, rows.len(), LIST) {
            return;
        }
        let chosen = match key {
            Key::Enter | Key::Char(' ') => rows.get(*sel).copied(),
            Key::Char(c) => rows
                .iter()
                .copied()
                .find(|r| r.key() == c.to_ascii_lowercase()),
            Key::Esc => {
                self.overlay = Overlay::None;
                return;
            }
            _ => None,
        };
        match chosen {
            Some(KeptRow::OpenFolder) => {
                let root = self.project.root.clone();
                self.show_in_file_manager(&root, "the project's folder");
            }
            Some(KeptRow::SaveCopy) => self.save_copy(),
            Some(KeptRow::ShowCopies) => {
                let dir = shelf::copies_dir(&self.books_home);
                self.show_in_file_manager(&dir, "your copies");
            }
            Some(KeptRow::MoveHome) => self.move_book_home(),
            None => {}
        }
    }

    fn show_in_file_manager(&mut self, dir: &Path, what: &str) {
        self.msg = match super::about::open_url(&dir.to_string_lossy()) {
            Ok(()) => format!("opened {what}"),
            Err(_) => format!("couldn't open it here · it's {}", books::pretty(dir)),
        };
    }

    /// The whole project as one file, in the copies folder, named for the project
    /// and today. Everything is saved first, so the copy is the project as it
    /// stands.
    pub fn save_copy(&mut self) {
        if !self.commit_saves() {
            return;
        }
        if let SaveState::Waiting(why) = &self.save_state {
            self.msg = format!("Save is waiting: {why}; copy not made");
            return;
        }
        let into = shelf::copies_dir(&self.books_home);
        let today = chrono::Local::now().date_naive();
        self.msg =
            match shelf::save_copy(&self.project.root, &self.project.meta.title, &into, today) {
                Ok(file) => format!(
                    "saved a copy: {} · in {}",
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    books::pretty(&into)
                ),
                Err(e) => format!("couldn't save a copy: {e:#}"),
            };
    }

    /// Put the project in the projects folder and carry on writing in it there.
    pub fn move_book_home(&mut self) {
        if self.kept_at_home() {
            self.msg = "it's already in your projects folder".into();
            return;
        }
        if self.deck.shell.backup.job.is_some() {
            self.msg = "a backup is running · move the project when it finishes".into();
            return;
        }
        // Everything written and put away first, as when the project is closed.
        if !self.try_quit() {
            self.msg = format!("{} · so the project stays where it is", self.msg);
            return;
        }
        let from = self.project.root.clone();
        let home = self.books_home.clone();
        let dest = match shelf::move_to(&from, &home) {
            Ok(d) => d,
            Err(e) => {
                self.msg = format!("{e:#}");
                return;
            }
        };
        self.overlay = Overlay::None;
        match self.load_book(&dest) {
            Ok(()) => {
                self.msg = format!("moved · this project now lives in {}", books::pretty(&dest));
            }
            Err(why) => {
                // It moved but won't open from there: put it back, so the
                // project that's open is still a project that's there.
                self.msg = match grimoire_core::book_copy::rename_new(&dest, &from) {
                    Ok(()) => format!("{why} · so it's back where it was"),
                    Err(e) => format!("{why} · it is now in {} ({e:#})", books::pretty(&dest)),
                };
            }
        }
    }
}

/// "416 KB", "2.1 MB".
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1_048_576 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} bytes"),
    }
}

pub(super) fn draw_kept(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Kept { sel, files, bytes } = &app.overlay else {
        return;
    };
    let rows = app.kept_rows();
    let width = 66.min(area.width.saturating_sub(2)).max(30);
    let box_area = centred(area, width, rows.len() as u16 + 14);
    f.render_widget(Clear, box_area);
    let block = pane_block("WHERE THIS PROJECT IS KEPT", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let iw = inner.width as usize;
    let dim = Style::default().fg(t.dim);
    let text = Style::default().fg(t.text);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!(
                " {}",
                truncate(&app.project.meta.title, iw.saturating_sub(2))
            ),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(" {}", short_path(&app.project.root, iw.saturating_sub(2))),
            text,
        )),
        Line::from(""),
    ];
    let what = format!(
        "A folder of plain text files: {} file{}, {}. Every scene is its own file, so any editor can read it.",
        thousands(*files),
        if *files == 1 { "" } else { "s" },
        size(*bytes)
    );
    for l in wrap_to(&what, iw.saturating_sub(2)) {
        lines.push(Line::from(Span::styled(format!(" {l}"), dim)));
    }
    // Is it anywhere but here?
    let synced = if app.sync_enabled() {
        ("Synced with Google Drive.".to_string(), dim)
    } else if let Some(client) = app.cloud {
        (format!("Synced by {}.", client.label()), dim)
    } else {
        (
            "Local to this laptop.".to_string(),
            Style::default().fg(t.warn),
        )
    };
    lines.push(Line::from(Span::styled(format!(" {}", synced.0), synced.1)));
    let copies = shelf::copies_dir(&app.books_home);
    let root = app
        .project
        .root
        .canonicalize()
        .unwrap_or_else(|_| app.project.root.clone());
    let backup = app
        .deck
        .shell
        .backup
        .status
        .snapshots
        .iter()
        .filter(|snapshot| Path::new(&snapshot._path) == root)
        .filter_map(|snapshot| chrono::DateTime::parse_from_rfc3339(&snapshot.time).ok())
        .max();
    let (last, style) = if let Some(when) = backup {
        (
            format!(
                " Last backup: {}.",
                when_label(when.with_timezone(&chrono::Local)).to_lowercase()
            ),
            dim,
        )
    } else if let Some((_, when)) =
        shelf::newest_copy(&copies, &app.project.meta.title, &app.project.root)
    {
        (
            format!(
                " Last copy saved: {}.",
                when_label(when.into()).to_lowercase()
            ),
            dim,
        )
    } else {
        (
            " No copy has been saved yet.".to_string(),
            Style::default().fg(t.warn),
        )
    };
    lines.push(Line::from(Span::styled(last, style)));
    lines.push(Line::from(""));

    let sel = (*sel).min(rows.len().saturating_sub(1));
    for (i, row) in rows.iter().enumerate() {
        let on = i == sel;
        let label = row.label();
        let pad = iw.saturating_sub(3 + label.chars().count() + 2);
        let l = Line::from(vec![
            Span::styled(
                if on { " ▸ " } else { "   " },
                Style::default().fg(t.accent),
            ),
            Span::styled(
                label,
                Style::default().fg(if on { t.accent } else { t.text }),
            ),
            Span::raw(" ".repeat(pad)),
            Span::styled(row.key().to_string(), Style::default().fg(t.sun)),
            Span::raw(" "),
        ]);
        lines.push(if on {
            l.style(Style::default().bg(t.sel))
        } else {
            l
        });
    }
    lines.push(Line::from(""));
    let says = match rows.get(sel) {
        Some(KeptRow::OpenFolder) => "Shows the project's folder in your file manager.".to_string(),
        Some(KeptRow::SaveCopy) => format!(
            "One .zip with the project files, named for the project and today, in {}. Keep it somewhere safe or send it. WrOS and Grimoire can open it.",
            books::pretty(&copies)
        ),
        Some(KeptRow::ShowCopies) => {
            format!("Shows {} in your file manager.", books::pretty(&copies))
        }
        Some(KeptRow::MoveHome) => format!(
            "Puts this project in {}, with your other projects. Adds a number if the name is taken.",
            books::pretty(&app.books_home)
        ),
        None => String::new(),
    };
    let mut said = wrap_to(&says, iw.saturating_sub(2));
    said.truncate(3);
    while said.len() < 3 {
        said.push(String::new());
    }
    for l in said {
        lines.push(Line::from(Span::styled(
            format!(" {l}"),
            Style::default().fg(t.dim).add_modifier(Modifier::ITALIC),
        )));
    }
    while lines.len() < (inner.height as usize).saturating_sub(1) {
        lines.push(Line::from(""));
    }
    lines.truncate((inner.height as usize).saturating_sub(1));
    lines.push(hint_line(" j/k move   ↵ do it   esc close", t));
    f.render_widget(Paragraph::new(lines), inner);
}

/// Break `text` into lines no wider than `width`, at spaces.
fn wrap_to(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// `p` as ~/…, cut down from the left to `width` characters.
fn short_path(p: &Path, width: usize) -> String {
    let s = books::pretty(p);
    let n = s.chars().count();
    if n <= width {
        s
    } else {
        format!("…{}", s.chars().skip(n + 1 - width).collect::<String>())
    }
}
