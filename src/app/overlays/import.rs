//! Bring a book in without changing its source or blocking the writer's keys.

use super::*;
use crate::backup_service::{self as service, Job, Progress};
use grimoire_core::book_copy::{self, MergeReport};
use grimoire_core::shelf::{free_destination, project_id_of, same_book, title_of};
use std::sync::mpsc::TryRecvError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportSource {
    // These sources are filled in by the following Import tasks.
    #[allow(dead_code)]
    Drive,
    #[allow(dead_code)]
    Usb {
        label: String,
        mount: PathBuf,
    },
    Laptop,
    #[allow(dead_code)]
    Draft,
}

impl ImportSource {
    fn label(&self) -> &str {
        match self {
            Self::Drive => "Google Drive",
            Self::Usb { label, .. } if !label.is_empty() => label,
            Self::Usb { .. } => "USB stick",
            Self::Laptop | Self::Draft => "this laptop",
        }
    }

    fn top(&self) -> PathBuf {
        let top = match self {
            Self::Usb { mount, .. } => mount.clone(),
            _ => grimoire_core::paths::home(),
        };
        fs::canonicalize(&top).unwrap_or(top)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Incoming {
    pub(crate) source: ImportSource,
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    path: PathBuf,
    name: String,
    book: bool,
}

#[derive(Debug)]
pub(crate) struct ImportDone {
    path: PathBuf,
    merge: Option<MergeReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportStage {
    Sources,
    Browse {
        source: ImportSource,
        dir: PathBuf,
        entries: Vec<Entry>,
    },
    Same {
        incoming: Incoming,
        local: PathBuf,
        choice: usize,
        // Each book's newest file date, read once: walking a book's folders on every
        // redraw is slow on the laptop's eMMC.
        newest_here: String,
        newest_incoming: String,
    },
    Copying {
        job: Job<ImportDone>,
        what: String,
    },
    Done(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportState {
    pub(crate) stage: ImportStage,
    // In Copying: 0 copies, 1/2 confirm keep/stop, 3 waits for cancellation,
    // and 4 allows the first progress frame before polling the result.
    pub(crate) sel: usize,
}

impl ImportState {
    pub(crate) fn new() -> Self {
        Self {
            stage: ImportStage::Sources,
            sel: 0,
        }
    }

    fn browse(source: ImportSource, dir: PathBuf) -> Self {
        let stage = match entries(&dir) {
            Ok(entries) => ImportStage::Browse {
                source,
                dir,
                entries,
            },
            Err(error) => ImportStage::Done(error),
        };
        Self { stage, sel: 0 }
    }
}

pub(crate) enum How {
    New,
    Replace(PathBuf),
    Merge(PathBuf),
    KeepBoth,
}

fn entries(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", books::pretty(dir)))?;
    let mut entries = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("Cannot read {}: {e}", books::pretty(dir)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|e| format!("Cannot read {name}: {e}"))?;
        if kind.is_dir() {
            let path = entry.path();
            entries.push(Entry {
                book: books::is_book(&path),
                path,
                name,
            });
        }
    }
    entries
        .sort_by_cached_key(|entry| (!entry.book, entry.name.to_lowercase(), entry.path.clone()));
    Ok(entries)
}

fn newest(root: &Path) -> String {
    let mut dirs = vec![root.to_path_buf()];
    let mut latest = None;
    while let Some(dir) = dirs.pop() {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        for entry in read.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                dirs.push(entry.path());
            } else if meta.is_file()
                && let Ok(time) = meta.modified()
            {
                latest = Some(latest.map_or(time, |old: std::time::SystemTime| old.max(time)));
            }
        }
    }
    latest.map_or_else(
        || "no readable file dates".into(),
        |time| {
            chrono::DateTime::<chrono::Local>::from(time)
                .format("%-d %b %Y %H:%M")
                .to_string()
        },
    )
}

impl App {
    pub(super) fn on_import_key(&mut self, key: Key) {
        let Overlay::Import(state) = &mut self.overlay else {
            return;
        };
        match &mut state.stage {
            ImportStage::Sources => match key {
                Key::Enter | Key::Char(' ') | Key::Right | Key::Char('l') => {
                    let source = ImportSource::Laptop;
                    let dir = source.top();
                    *state = ImportState::browse(source, dir);
                }
                Key::Esc => self.overlay = Overlay::Books { sel: 0 },
                _ => {
                    list_nav(key, &mut state.sel, 1, RING);
                }
            },
            ImportStage::Browse {
                source,
                dir,
                entries,
            } => {
                let up = *dir != source.top();
                if list_nav(key, &mut state.sel, entries.len() + usize::from(up), LIST) {
                    return;
                }
                match key {
                    Key::Esc => *state = ImportState::new(),
                    Key::Enter | Key::Char(' ') | Key::Right | Key::Char('l') => {
                        if let Some(entry) = entries.get(state.sel) {
                            if entry.book {
                                let incoming = Incoming {
                                    source: source.clone(),
                                    path: entry.path.clone(),
                                    name: entry.name.clone(),
                                    project_id: project_id_of(&entry.path),
                                };
                                self.pick_import(incoming);
                            } else {
                                *state = ImportState::browse(source.clone(), entry.path.clone());
                            }
                        } else if up && let Some(parent) = dir.parent() {
                            *state = ImportState::browse(source.clone(), parent.to_path_buf());
                        }
                    }
                    _ => {}
                }
            }
            ImportStage::Same {
                incoming,
                local,
                choice,
                ..
            } => {
                if list_nav(key, choice, 4, RING) {
                    return;
                }
                match key {
                    Key::Esc => *state = ImportState::new(),
                    Key::Enter | Key::Char(' ') => {
                        let how = match *choice {
                            0 => How::Replace(local.clone()),
                            1 => How::Merge(local.clone()),
                            2 => How::KeepBoth,
                            _ => {
                                *state = ImportState::new();
                                return;
                            }
                        };
                        let incoming = incoming.clone();
                        self.start_import(incoming, how);
                    }
                    _ => {}
                }
            }
            ImportStage::Copying { job, .. } => {
                if state.sel == 4 {
                    state.sel = 0;
                }
                match (state.sel, key) {
                    (0, Key::Esc) => state.sel = 1,
                    (1 | 2, Key::Esc) | (1, Key::Enter) => state.sel = 0,
                    (1 | 2, Key::Left | Key::Right | Key::Tab | Key::BackTab) => {
                        state.sel = 3 - state.sel;
                    }
                    (2, Key::Enter) => {
                        job.cancel();
                        state.sel = 3;
                    }
                    _ => {}
                }
            }
            ImportStage::Done(_) => match key {
                Key::Enter | Key::Esc => self.overlay = Overlay::None,
                _ => {}
            },
        }
    }

    fn import_done(&mut self, text: String) {
        self.msg = text.clone();
        self.overlay = Overlay::Import(ImportState {
            stage: ImportStage::Done(text),
            sel: 0,
        });
    }

    fn pick_import(&mut self, incoming: Incoming) {
        let here = fs::canonicalize(&self.project.root).unwrap_or(self.project.root.clone());
        let path = fs::canonicalize(&incoming.path).unwrap_or(incoming.path.clone());
        if path == here || same_book(&here, &incoming.name, incoming.project_id.as_deref()) {
            self.import_done("This book is already open here.".into());
            return;
        }
        for row in self.shelf_rows() {
            if let ShelfRow::Book(book, _) = row
                && same_book(&book.root, &incoming.name, incoming.project_id.as_deref())
            {
                let newest_here = newest(&book.root);
                let newest_incoming = newest(&incoming.path);
                self.overlay = Overlay::Import(ImportState {
                    stage: ImportStage::Same {
                        incoming,
                        local: book.root,
                        choice: 3,
                        newest_here,
                        newest_incoming,
                    },
                    sel: 0,
                });
                return;
            }
        }
        self.start_import(incoming, How::New);
    }

    pub(crate) fn start_import(&mut self, incoming: Incoming, how: How) {
        let what = format!(
            "Copying {} from {}…",
            title_of(&incoming.path),
            incoming.source.label()
        );
        let documents = books::documents();
        let now = chrono::Local::now().naive_local();
        let work = service::spawn_work("import", move |cancel, shared| {
            let run = || -> anyhow::Result<ImportDone> {
                let mut progress = |p: book_copy::Progress| {
                    if let Ok(mut shared) = shared.lock() {
                        *shared = Some(Progress {
                            phase: format!("{} files · {} bytes copied", p.files, p.bytes),
                            percent: None,
                            offer: None,
                        });
                    }
                };
                match how {
                    How::New | How::KeepBoth => {
                        fs::create_dir_all(&documents)?;
                        let path = free_destination(&documents, &incoming.name);
                        book_copy::copy_book(&incoming.path, &path, cancel, &mut progress)?;
                        Ok(ImportDone { path, merge: None })
                    }
                    How::Replace(path) => {
                        book_copy::replace_with_copy(
                            &incoming.path,
                            &path,
                            now.date(),
                            cancel,
                            &mut progress,
                        )?;
                        Ok(ImportDone { path, merge: None })
                    }
                    How::Merge(path) => {
                        let report = book_copy::merge_into(
                            &incoming.path,
                            &path,
                            incoming.source.label(),
                            now,
                            cancel,
                        )?;
                        Ok(ImportDone {
                            path,
                            merge: Some(report),
                        })
                    }
                }
            };
            run().map_err(|e| format!("Book import failed: {e:#}"))
        });
        match work {
            Ok(job) => {
                self.overlay = Overlay::Import(ImportState {
                    stage: ImportStage::Copying { job, what },
                    sel: 4,
                });
            }
            Err(error) => self.import_done(error),
        }
    }

    /// Keep the progress screen until the worker has finished checking or cleaning up.
    pub(crate) fn tick_import(&mut self) {
        let Overlay::Import(state) = &mut self.overlay else {
            return;
        };
        let ImportStage::Copying { job, .. } = &state.stage else {
            return;
        };
        if state.sel == 4 {
            state.sel = 0;
            return;
        }
        // A result must not dismiss the writer's cancellation question.
        if matches!(state.sel, 1 | 2) {
            return;
        }
        let stopped = state.sel == 3;
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The import worker stopped.".into()),
        };
        // The worker has ended; dropping its handle no longer waits for file work.
        self.overlay = Overlay::None;
        match result {
            Ok(done) => {
                self.shelf_cache.take();
                let mut text = format!(
                    "Copied to {} and checked. Opening it.",
                    books::pretty(&done.path)
                );
                if let Some(report) = &done.merge
                    && !report.kept_local.is_empty()
                {
                    let kept: Vec<String> = report
                        .kept_local
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    text = format!("Kept local: {}. {text}", kept.join(", "));
                }
                if self.open_book(&done.path) {
                    self.import_done(text.clone());
                    if done.merge.is_some() {
                        self.open_conflicts();
                        self.msg = text;
                    }
                } else {
                    self.import_done(format!(
                        "Copied to {} and checked. {}",
                        books::pretty(&done.path),
                        self.msg
                    ));
                }
            }
            // The writer chose Stop copying: the copy's own error is no news to them.
            Err(_) if stopped => self.import_done("Import stopped. Nothing was kept.".into()),
            Err(error) => self.import_done(error),
        }
    }
}

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Import(state) = &app.overlay else {
        return;
    };
    let box_area = centred(area, 92.min(area.width.saturating_sub(4)), 20);
    f.render_widget(Clear, box_area);
    let block = pane_block("IMPORT A BOOK", true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let width = inner.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    let mut sel = state.sel;
    let mut hint = " ↑↓ move   ↵ choose   esc back";
    let mut say = |text: String| {
        for row in wrap_words(&text, width) {
            lines.push(Line::from(Span::styled(
                format!(" {row}"),
                Style::default().fg(t.text),
            )));
        }
    };
    match &state.stage {
        ImportStage::Sources => rows.push("A folder on this laptop".into()),
        ImportStage::Browse {
            source,
            dir,
            entries,
        } => {
            say(format!(
                "Import from {} · {}",
                source.label(),
                books::pretty(dir)
            ));
            rows = entries
                .iter()
                .map(|entry| {
                    format!(
                        "{} · {}",
                        truncate(&entry.name, width.saturating_sub(12)),
                        if entry.book { "book" } else { "folder" }
                    )
                })
                .collect();
            if *dir != source.top() {
                rows.push("Up one folder".into());
            }
        }
        ImportStage::Same {
            incoming,
            local,
            choice,
            newest_here,
            newest_incoming,
        } => {
            say("This book is already on the shelf. Choose what to keep.".into());
            say(format!("Here: {}", books::pretty(local)));
            say(format!("Newest here: {newest_here}"));
            say(format!("Incoming: {}", books::pretty(&incoming.path)));
            say(format!("Newest incoming: {newest_incoming}"));
            rows = ["Replace", "Merge", "Keep both", "Cancel"]
                .map(str::to_string)
                .to_vec();
            sel = *choice;
        }
        ImportStage::Copying { job, what } => {
            say(what.clone());
            say(job.status_line());
            hint = " esc stop copying";
            if matches!(state.sel, 1 | 2) {
                say("Stop copying? Nothing will be kept.".into());
                rows = vec!["Keep copying".into(), "Stop copying".into()];
                sel = state.sel - 1;
                hint = " ←→ choose   ↵ confirm   esc keep copying";
            } else if state.sel == 3 {
                say("Stopping and removing the unfinished copy…".into());
                hint = " please wait for cleanup";
            }
        }
        ImportStage::Done(text) => {
            say(text.clone());
            hint = " ↵ or esc close";
        }
    }
    lines.push(Line::from(""));
    let room = (inner.height as usize)
        .saturating_sub(lines.len() + 2)
        .max(1);
    let start = (sel + 1).saturating_sub(room);
    for (i, row) in rows.iter().enumerate().skip(start).take(room) {
        let on = i == sel;
        let text = format!(" {} {}", if on { "▸" } else { " " }, row);
        lines.push(Line::from(Span::styled(
            truncate(&text, inner.width as usize),
            if on {
                Style::default().fg(t.accent).bg(t.sel)
            } else {
                Style::default().fg(t.text)
            },
        )));
    }
    lines.push(Line::from(""));
    lines.push(hint_line(hint, t));
    f.render_widget(Paragraph::new(lines), inner);
}
