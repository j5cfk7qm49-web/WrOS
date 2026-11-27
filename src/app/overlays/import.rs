//! Bring a book in without changing its source or blocking the writer's keys.

use super::*;
use crate::backup_service::{self as service, DriveBooks, Job, Progress, Request, Status};
use crate::sync_service;
use grimoire_core::book_copy::{self, MergeReport};
use grimoire_core::import;
use grimoire_core::projects_shelf::{free_destination, project_id_of, same_book, title_of};
use std::sync::mpsc::TryRecvError;
use wros_sys::{ImportStick, Request as SystemRequest};

const HISTORY_PROGRESS: &str = "Bringing in the project's history…";
const SYNC_PROGRESS: &str = "Linking the project with Google Drive…";

#[derive(Default)]
pub(crate) struct ImportMedia {
    sticks: Vec<ImportStick>,
    pending: Option<SystemRequest>,
    refresh: bool,
    open: bool,
    pub(crate) mounted: bool,
    pub(crate) waiting: bool,
    release: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportSource {
    Drive,
    Usb { label: String, mount: PathBuf },
    Laptop,
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
            Self::Drive => return PathBuf::from("drive:WrOS"),
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
    draft: bool,
    drive: Option<service::DriveBookEntry>,
}

#[derive(Debug)]
pub(crate) struct ImportDone {
    path: PathBuf,
    merge: Option<MergeReport>,
    sync: bool,
    sync_failed: bool,
    history_notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportStage {
    Sources,
    MountingUsb {
        device: String,
    },
    ReadingDrive {
        dir: PathBuf,
        job: Job<DriveBooks>,
        selected: Option<PathBuf>,
    },
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
    open_on_enter: Option<PathBuf>,
    // In Copying: 0 copies, 1/2 confirm keep/stop, 3 waits for cancellation.
    // In Copying and ReadingDrive: 4 allows a progress frame before polling.
    pub(crate) sel: usize,
}

impl ImportState {
    pub(crate) fn new() -> Self {
        Self {
            stage: ImportStage::Sources,
            open_on_enter: None,
            sel: 0,
        }
    }

    fn browse(source: ImportSource, dir: PathBuf) -> Self {
        let stage = match entries(&dir, source == ImportSource::Draft) {
            Ok(entries) => ImportStage::Browse {
                source,
                dir,
                entries,
            },
            Err(error) => ImportStage::Done(error),
        };
        Self {
            stage,
            open_on_enter: None,
            sel: 0,
        }
    }
}

pub(crate) enum How {
    New,
    Replace(PathBuf),
    Merge(PathBuf),
    KeepBoth,
}

fn entries(dir: &Path, drafts: bool) -> Result<Vec<Entry>, String> {
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
        let path = entry.path();
        let draft = drafts && kind.is_file() && import::is_draft(&path);
        if kind.is_dir() || draft {
            entries.push(Entry {
                book: !drafts && books::is_book(&path),
                draft,
                path,
                name,
                drive: None,
            });
        }
    }
    entries.sort_by_cached_key(|entry| {
        (
            entry.draft,
            !entry.book,
            entry.name.to_lowercase(),
            entry.path.clone(),
        )
    });
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

/// Only one book on this laptop may own a Drive folder (the design's Keep both rule). A copy
/// from a laptop folder or a USB stick leaves the original's sync link behind.
fn drop_sync_identity(book: &Path) -> anyhow::Result<()> {
    let link = book.join(".grimoire/sync.json");
    if fs::symlink_metadata(&link).is_ok() {
        fs::remove_file(&link)?;
    }
    Ok(())
}

/// After Replace, the book now in use keeps syncing: it keeps the link it came with (a Drive
/// marker's), or takes over the replaced book's, and the replaced book is left without one.
fn settle_replaced_identity(new: &Path, old: &Path) -> anyhow::Result<()> {
    let (new_link, old_link) = (
        new.join(".grimoire/sync.json"),
        old.join(".grimoire/sync.json"),
    );
    if fs::symlink_metadata(&old_link).is_err() {
        return Ok(());
    }
    if fs::symlink_metadata(&new_link).is_ok() {
        fs::remove_file(&old_link)?;
    } else {
        fs::create_dir_all(new.join(".grimoire"))?;
        fs::rename(&old_link, &new_link)?;
    }
    Ok(())
}

fn import_error(incoming: &Incoming, error: anyhow::Error) -> String {
    if matches!(&incoming.source, ImportSource::Usb { .. }) && !books::is_book(&incoming.path) {
        "The USB stick was removed.".into()
    } else {
        format!("Project import failed: {error:#}")
    }
}

fn finishing_drive_import(job: &Job<ImportDone>) -> bool {
    job.phase_is(SYNC_PROGRESS) || job.phase_is(HISTORY_PROGRESS)
}

impl App {
    pub(crate) fn open_import(&mut self) {
        self.import_media.sticks.clear();
        self.import_media.error = None;
        self.import_media.pending = None;
        self.import_media.refresh = true;
        self.overlay = Overlay::Import(ImportState::new());
    }

    pub(crate) fn open_drive_import(&mut self, remote: String) {
        if !self.deck.shell.backup.status.google_connected {
            return;
        }
        let remote = remote.trim_end_matches('/');
        let parent = remote
            .rsplit_once('/')
            .map_or("drive:", |(parent, _)| parent);
        self.open_import();
        self.browse_import(ImportSource::Drive, PathBuf::from(parent));
        if let Overlay::Import(ImportState {
            stage: ImportStage::ReadingDrive { selected, .. },
            ..
        }) = &mut self.overlay
        {
            *selected = Some(PathBuf::from(remote));
        }
    }

    /// Read and split a draft just as before, then open its new book in Documents.
    pub(crate) fn import_draft(&mut self, file: &Path) {
        let draft = match import::read(file) {
            Ok(d) => d,
            Err(e) => {
                self.import_done(format!("{e:#}"));
                return;
            }
        };
        let name = file.file_stem().unwrap_or_default().to_string_lossy();
        let root = free_destination(&books::documents(), &name);
        if let Err(e) = import::make_book(&root, &draft) {
            self.import_done(format!("couldn't bring it in: {e:#}"));
            return;
        }
        self.overlay = Overlay::None;
        if self.open_book(&root) {
            self.msg = format!(
                "brought in {}: {}{} · it lives in {}",
                draft.title,
                books::tally(&draft),
                if draft.before.trim().is_empty() {
                    ""
                } else {
                    " · what came before the first chapter is in Notes"
                },
                books::pretty(&root)
            );
        }
    }

    pub(crate) fn on_import_media(&mut self, request: &SystemRequest, sticks: Vec<ImportStick>) {
        self.import_media.waiting = false;
        match request {
            SystemRequest::ListImportMedia => {
                self.import_media.mounted |= sticks.iter().any(|stick| stick.mount.is_some());
                self.import_media.sticks = sticks;
            }
            SystemRequest::MountImportMedia(device) => {
                self.import_media.mounted = true;
                if matches!(
                    &self.overlay,
                    Overlay::Import(ImportState {
                        stage: ImportStage::MountingUsb { device: chosen },
                        ..
                    }) if chosen == device
                ) && let Some(stick) = sticks.into_iter().next()
                    && let Some(mount) = stick.mount
                {
                    self.import_media.release = false;
                    let source = ImportSource::Usb {
                        label: stick.label,
                        mount: PathBuf::from(mount),
                    };
                    let dir = source.top();
                    self.browse_import(source, dir);
                } else {
                    // Esc or another screen won the race with the mount worker.
                    self.import_media.release = true;
                }
            }
            SystemRequest::UnmountImportMedia => {
                self.import_media.mounted = false;
                self.import_media.release = false;
                if let Overlay::Import(ImportState {
                    stage: ImportStage::MountingUsb { device },
                    ..
                }) = &self.overlay
                {
                    self.import_media.pending =
                        Some(SystemRequest::MountImportMedia(device.clone()));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_import_media_error(&mut self, request: &SystemRequest, error: &str) {
        match request {
            SystemRequest::ListImportMedia => {
                self.import_media.waiting = false;
                self.import_media.error = Some(format!("Cannot list USB sticks: {error}"));
            }
            SystemRequest::MountImportMedia(_) => {
                self.import_media.waiting = false;
                self.import_media.mounted = true;
                self.import_media.release = true;
                if matches!(self.overlay, Overlay::Import(_)) {
                    self.import_done(format!("Cannot mount the USB stick: {error}"));
                }
            }
            SystemRequest::UnmountImportMedia => {
                self.import_media.waiting = false;
                self.import_media.mounted = true;
                self.import_media.release = false;
                self.msg = format!("Cannot unmount the USB stick: {error}");
                if matches!(self.overlay, Overlay::Import(_)) {
                    self.import_done(self.msg.clone());
                }
            }
            _ => {}
        }
    }

    pub(super) fn on_import_key(&mut self, key: Key) {
        let Overlay::Import(state) = &mut self.overlay else {
            return;
        };
        match &mut state.stage {
            ImportStage::Sources => match key {
                Key::Enter | Key::Char(' ') | Key::Right | Key::Char('l') => {
                    let source = if state.sel == 0 {
                        if !self.deck.shell.backup.status.google_connected {
                            self.open_settings_page(crate::app::shell::Route::GoogleAccount);
                            return;
                        }
                        ImportSource::Drive
                    } else if let Some(stick) = self.import_media.sticks.get(state.sel - 1) {
                        let device = stick.device.clone();
                        self.import_media.pending = Some(if self.import_media.mounted {
                            SystemRequest::UnmountImportMedia
                        } else {
                            SystemRequest::MountImportMedia(device.clone())
                        });
                        state.stage = ImportStage::MountingUsb { device };
                        state.sel = 0;
                        return;
                    } else if state.sel == self.import_media.sticks.len() + 1 {
                        ImportSource::Laptop
                    } else {
                        ImportSource::Draft
                    };
                    let dir = source.top();
                    self.browse_import(source, dir);
                }
                Key::Esc => self.overlay = Overlay::Books { sel: 0 },
                _ => {
                    list_nav(
                        key,
                        &mut state.sel,
                        self.import_media.sticks.len() + 3,
                        RING,
                    );
                }
            },
            ImportStage::MountingUsb { .. } => {
                if key == Key::Esc {
                    *state = ImportState::new();
                    self.import_media.pending = None;
                    self.import_media.release = true;
                }
            }
            ImportStage::ReadingDrive { .. } => {
                if key == Key::Esc {
                    *state = ImportState::new();
                }
            }
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
                            if entry.draft {
                                let file = entry.path.clone();
                                self.import_draft(&file);
                            } else if entry.book {
                                let incoming = Incoming {
                                    source: source.clone(),
                                    path: entry.path.clone(),
                                    name: entry.name.clone(),
                                    project_id: entry.drive.as_ref().map_or_else(
                                        || project_id_of(&entry.path),
                                        |drive| drive.project_id.clone(),
                                    ),
                                };
                                self.pick_import(incoming);
                            } else {
                                let source = source.clone();
                                let dir = entry.path.clone();
                                self.browse_import(source, dir);
                            }
                        } else if up && let Some(parent) = dir.parent() {
                            let source = source.clone();
                            let dir = parent.to_path_buf();
                            self.browse_import(source, dir);
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
                if finishing_drive_import(job) {
                    return;
                }
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
                Key::Enter => {
                    let open = state.open_on_enter.take();
                    self.overlay = Overlay::None;
                    if let Some(path) = open
                        && path != self.project.root
                        && !self.open_book(&path)
                    {
                        self.import_done(self.msg.clone());
                    }
                }
                Key::Esc => self.overlay = Overlay::None,
                _ => {}
            },
        }
    }

    fn browse_import(&mut self, source: ImportSource, dir: PathBuf) {
        if source != ImportSource::Drive {
            self.overlay = Overlay::Import(ImportState::browse(source, dir));
            return;
        }
        let request = Request::DriveBooks {
            folder: dir.to_string_lossy().into_owned(),
        };
        let backup = &self.deck.shell.backup;
        let state = backup.state.clone();
        let script = backup.script.clone();
        match service::spawn_work("drive-books", move |cancel, progress| {
            service::execute_in(&request, cancel, Some(state), Some(progress), script)
        }) {
            Ok(job) => {
                self.overlay = Overlay::Import(ImportState {
                    stage: ImportStage::ReadingDrive {
                        dir,
                        job,
                        selected: None,
                    },
                    open_on_enter: None,
                    sel: 4,
                });
            }
            Err(error) => self.import_done(error),
        }
    }

    fn import_done(&mut self, text: String) {
        self.msg = text.clone();
        self.overlay = Overlay::Import(ImportState {
            stage: ImportStage::Done(text),
            open_on_enter: None,
            sel: 0,
        });
    }

    fn pick_import(&mut self, incoming: Incoming) {
        let drive = incoming.source == ImportSource::Drive;
        let rows = self.shelf_rows();
        if drive {
            let folder = incoming.path.to_string_lossy();
            let folder = folder.strip_prefix("drive:WrOS/");
            for row in &rows {
                if let ShelfRow::Book(book, _) = row
                    && fs::symlink_metadata(book.root.join(".grimoire"))
                        .is_ok_and(|meta| meta.is_dir())
                    && fs::symlink_metadata(book.root.join(".grimoire/sync.json"))
                        .is_ok_and(|meta| meta.is_file())
                    && let Ok(bytes) = fs::read(book.root.join(".grimoire/sync.json"))
                    && let Ok(identity) = serde_json::from_slice::<serde_json::Value>(&bytes)
                    && folder.is_some()
                    && identity
                        .get("drive_folder")
                        .and_then(serde_json::Value::as_str)
                        == folder
                {
                    self.import_done(format!(
                        "{} is already here and syncs with this Drive folder.",
                        book.title
                    ));
                    if let Overlay::Import(state) = &mut self.overlay {
                        state.open_on_enter = Some(book.root.clone());
                    }
                    return;
                }
            }
        }
        let here = fs::canonicalize(&self.project.root).unwrap_or(self.project.root.clone());
        if (!drive && fs::canonicalize(&incoming.path).unwrap_or(incoming.path.clone()) == here)
            || same_book(&here, &incoming.name, incoming.project_id.as_deref())
        {
            self.import_done("This project is already open here.".into());
            return;
        }
        for row in rows {
            if let ShelfRow::Book(book, _) = row
                && same_book(&book.root, &incoming.name, incoming.project_id.as_deref())
            {
                let newest_here = newest(&book.root);
                let newest_incoming = if drive {
                    "not available from Drive".into()
                } else {
                    newest(&incoming.path)
                };
                self.overlay = Overlay::Import(ImportState {
                    stage: ImportStage::Same {
                        incoming,
                        local: book.root,
                        choice: 3,
                        newest_here,
                        newest_incoming,
                    },
                    open_on_enter: None,
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
            if incoming.source == ImportSource::Drive {
                incoming.name.clone()
            } else {
                title_of(&incoming.path)
            },
            incoming.source.label()
        );
        let documents = books::documents();
        let now = chrono::Local::now().naive_local();
        let project = self.project.root.to_string_lossy().into_owned();
        let backup = &self.deck.shell.backup;
        let state = backup.state.clone();
        let script = backup.script.clone();
        let sync_script = backup.sync.script.clone();
        let sync_env = backup.sync.env.clone();
        let work = service::spawn_work("import", move |cancel, shared| {
            let run = || -> anyhow::Result<ImportDone> {
                let drive = incoming.source == ImportSource::Drive;
                fs::create_dir_all(&documents)?;
                // Replace and Merge need a checked source beside the destination; the
                // temporary directory removes it on success, failure or cancellation.
                let stage = if drive && matches!(&how, How::Replace(_) | How::Merge(_)) {
                    Some(
                        tempfile::Builder::new()
                            .prefix(".wros-import-drive-")
                            .tempdir_in(&documents)?,
                    )
                } else {
                    None
                };
                let materialized = if drive {
                    let path = stage.as_ref().map_or_else(
                        || free_destination(&documents, &incoming.name),
                        |stage| stage.path().join(&incoming.name),
                    );
                    let request = Request::DriveMaterialize {
                        remote: incoming.path.to_string_lossy().into_owned(),
                        destination: path.to_string_lossy().into_owned(),
                        project: Some(project),
                    };
                    let _: Status = service::execute_in(
                        &request,
                        cancel,
                        Some(state),
                        Some(shared.clone()),
                        script,
                    )
                    .map_err(anyhow::Error::msg)?;
                    Some(path)
                } else {
                    None
                };
                let source = materialized.as_deref().unwrap_or(&incoming.path);
                let mut progress = |p: book_copy::Progress| {
                    if let Ok(mut shared) = shared.lock() {
                        *shared = Some(Progress {
                            phase: format!("{} files · {} bytes copied", p.files, p.bytes),
                            percent: None,
                            offer: None,
                        });
                    }
                };
                let copy = || -> anyhow::Result<ImportDone> {
                    // The checked copy's marker is current, including books beyond the
                    // listing's 50-marker cap. Never enable from an old listing alone.
                    let marker = source.join(".wros-sync.json");
                    let project_id = if drive
                        && !matches!(&how, How::KeepBoth | How::Merge(_))
                        && marker.exists()
                    {
                        anyhow::ensure!(
                            fs::metadata(&marker)?.len() <= 2 * 1024 * 1024,
                            "Drive's sync marker is too large."
                        );
                        let value: serde_json::Value = serde_json::from_slice(&fs::read(&marker)?)?;
                        let id = value.get("project_id").and_then(serde_json::Value::as_str);
                        let id = id.filter(|id| {
                            id.len() == 36
                                && id.bytes().enumerate().all(|(i, byte)| {
                                    if matches!(i, 8 | 13 | 18 | 23) {
                                        byte == b'-'
                                    } else {
                                        byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
                                    }
                                })
                        });
                        Some(
                            id.ok_or_else(|| anyhow::anyhow!("Drive's sync marker is invalid."))?
                                .to_string(),
                        )
                    } else {
                        None
                    };
                    let sync = project_id.is_some();
                    if drive {
                        anyhow::ensure!(
                            !cancel.load(std::sync::atomic::Ordering::Relaxed),
                            "Import stopped."
                        );
                        anyhow::ensure!(
                            books::is_book(source),
                            "Drive folder is no longer a project."
                        );
                        let identity = source.join(".grimoire/sync.json");
                        if sync {
                            let remote = incoming.path.to_string_lossy();
                            let folder = remote.strip_prefix("drive:WrOS/").filter(|folder| {
                                !folder.is_empty()
                                    && !folder.contains('/')
                                    && !folder.contains('\\')
                            });
                            let folder = folder.ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Synced projects must be directly inside Drive's WrOS folder."
                                )
                            })?;
                            fs::create_dir_all(source.join(".grimoire"))?;
                            fs::write(
                                &identity,
                                serde_json::to_vec(&serde_json::json!({
                                    "version": 1,
                                    "project_id": project_id,
                                    "drive_folder": folder,
                                }))?,
                            )?;
                        } else if identity.exists() {
                            // Keep both and ordinary copies must not inherit a stale link.
                            fs::remove_file(identity)?;
                        }
                    }
                    // Drive's history is brought in only for a book that is new on this laptop:
                    // merging into a book already here keeps that book's own timeline.
                    let fresh = !matches!(how, How::Merge(_));
                    let (path, merge) = match how {
                        How::New | How::KeepBoth => {
                            let path = if drive {
                                source.to_path_buf()
                            } else {
                                let path = free_destination(&documents, &incoming.name);
                                book_copy::copy_book(source, &path, cancel, &mut progress)?;
                                drop_sync_identity(&path)?;
                                path
                            };
                            (path, None)
                        }
                        How::Replace(path) => {
                            let replaced = book_copy::replace_with_copy(
                                source,
                                &path,
                                now.date(),
                                cancel,
                                &mut progress,
                            )?;
                            if !drive {
                                drop_sync_identity(&path)?;
                            }
                            settle_replaced_identity(&path, &replaced)?;
                            (path, None)
                        }
                        How::Merge(path) => {
                            let report = book_copy::merge_into(
                                source,
                                &path,
                                incoming.source.label(),
                                now,
                                cancel,
                            )?;
                            (path, Some(report))
                        }
                    };
                    let show_phase = |phase: &str| {
                        if let Ok(mut shared) = shared.lock() {
                            *shared = Some(Progress {
                                phase: phase.into(),
                                percent: None,
                                offer: None,
                            });
                        }
                    };
                    let sync_failed = if sync && fresh {
                        show_phase(SYNC_PROGRESS);
                        // Adopt the checked files as a sync base before the writer can edit them.
                        let project = path.to_string_lossy().into_owned();
                        let request = sync_service::Request::sync(project, false, false);
                        !sync_service::run_blocking::<sync_service::Reply>(
                            &request,
                            sync_script,
                            &sync_env,
                            cancel,
                        )
                        .is_ok_and(|reply| reply.status == "synced")
                    } else {
                        false
                    };
                    // Only this source's marker can identify the right Drive history.
                    let history_notice = if sync && fresh && crate::installed_session() {
                        show_phase(HISTORY_PROGRESS);
                        timeline::import_history(&path, sync_service::run_timeline)
                    } else {
                        None
                    };
                    Ok(ImportDone {
                        path,
                        merge,
                        sync,
                        sync_failed,
                        history_notice,
                    })
                };
                let result = copy();
                if result.is_err()
                    && stage.is_none()
                    && let Some(path) = materialized
                {
                    let _ = if path.is_dir() {
                        fs::remove_dir_all(path)
                    } else {
                        fs::remove_file(path)
                    };
                }
                result
            };
            run().map_err(|error| import_error(&incoming, error))
        });
        match work {
            Ok(job) => {
                self.overlay = Overlay::Import(ImportState {
                    stage: ImportStage::Copying { job, what },
                    open_on_enter: None,
                    sel: 4,
                });
            }
            Err(error) => self.import_done(error),
        }
    }

    /// Keep the progress screen until the worker has finished checking or cleaning up.
    pub(crate) fn tick_import(&mut self) {
        let importing = matches!(self.overlay, Overlay::Import(_));
        if !importing {
            if self.import_media.open {
                self.import_media.release = true;
            }
            self.import_media.pending = None;
            self.import_media.refresh = false;
        }
        self.import_media.open = importing;
        // Copying cannot be left by a key. If another screen replaces it, Job's
        // drop waits for cancellation and cleanup before this tick can unmount.
        let copying = matches!(
            self.overlay,
            Overlay::Import(ImportState {
                stage: ImportStage::Copying { .. },
                ..
            })
        );
        if !self.deck.busy && !self.import_media.waiting && !copying {
            if self.import_media.mounted && self.import_media.release {
                self.import_media.pending = Some(SystemRequest::UnmountImportMedia);
            }
            if self.import_media.pending.is_none() && self.import_media.refresh {
                self.import_media.pending = Some(SystemRequest::ListImportMedia);
                self.import_media.refresh = false;
            }
            if let Some(request) = self.import_media.pending.take() {
                self.deck.request(request);
                self.import_media.waiting = self.deck.busy;
            }
        }
        let Overlay::Import(state) = &mut self.overlay else {
            return;
        };
        if let ImportStage::ReadingDrive { dir, job, selected } = &state.stage {
            if state.sel == 4 {
                state.sel = 0;
                return;
            }
            let result = match job.result.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err("The Drive listing worker stopped.".into()),
            };
            match result {
                Ok(reply) => {
                    let entries: Vec<Entry> = reply
                        .entries
                        .into_iter()
                        .map(|entry| Entry {
                            path: PathBuf::from(&entry.path),
                            name: entry.name.clone(),
                            book: entry.book,
                            draft: false,
                            drive: Some(entry),
                        })
                        .collect();
                    state.sel = selected
                        .as_ref()
                        .and_then(|path| entries.iter().position(|entry| entry.path == *path))
                        .unwrap_or(0);
                    state.stage = ImportStage::Browse {
                        source: ImportSource::Drive,
                        dir: dir.clone(),
                        entries,
                    };
                }
                Err(error) => self.import_done(error),
            }
            return;
        }
        let ImportStage::Copying { job, .. } = &state.stage else {
            return;
        };
        if state.sel == 4 {
            state.sel = 0;
            return;
        }
        if finishing_drive_import(job) {
            state.sel = 0;
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
        self.import_media.release = true;
        match result {
            Ok(done) => {
                self.shelf_cache.take();
                let mut text = format!("Copied to {} and checked.", books::pretty(&done.path));
                let opened = self.open_book(&done.path);
                if opened {
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
                    text.push_str(" Opening it.");
                    if done.sync {
                        self.request_sync_switch(true);
                    }
                } else {
                    text.push(' ');
                    text.push_str(&self.msg);
                }
                if done.sync_failed {
                    text.push_str(
                        " The first sync didn't finish; it runs again when the project is open.",
                    );
                }
                if let Some(notice) = done.history_notice {
                    text.push(' ');
                    text.push_str(&notice);
                }
                self.import_done(text.clone());
                if opened && done.merge.is_some() {
                    self.open_conflicts();
                    self.msg = text;
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
    let block = pane_block("IMPORT A PROJECT", true, t);
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
        ImportStage::Sources => {
            rows.push(if app.deck.shell.backup.status.google_connected {
                "Google Drive".into()
            } else {
                "Google Drive · link it in Settings first".into()
            });
            rows.extend(app.import_media.sticks.iter().map(|stick| {
                if stick.label.is_empty() {
                    "USB stick".into()
                } else {
                    format!("USB stick \"{}\"", stick.label)
                }
            }));
            rows.push("A folder on this laptop".into());
            rows.push("A draft (Word, Markdown, text)".into());
            if let Some(error) = &app.import_media.error {
                say(error.clone());
            }
        }
        ImportStage::MountingUsb { .. } => say("Opening the USB stick…".into()),
        ImportStage::ReadingDrive { job, .. } => {
            say("Reading Google Drive…".into());
            say(job.status_line());
        }
        ImportStage::Browse {
            source,
            dir,
            entries,
        } => {
            say(format!(
                "Import from {} · {}",
                source.label(),
                if *source == ImportSource::Drive {
                    dir.to_string_lossy().trim_start_matches("drive:").into()
                } else {
                    books::pretty(dir)
                }
            ));
            rows = entries
                .iter()
                .map(|entry| {
                    let synced = entry.drive.as_ref().is_some_and(|drive| drive.synced);
                    format!(
                        "{} · {}{}",
                        truncate(
                            &entry.name,
                            width.saturating_sub(if synced { 20 } else { 12 })
                        ),
                        if entry.draft {
                            "draft"
                        } else if entry.book {
                            "project"
                        } else {
                            "folder"
                        },
                        if synced { " · synced" } else { "" }
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
            say("This project is already on the shelf. Choose what to keep.".into());
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
            if finishing_drive_import(job) {
                hint = " please wait";
            } else if matches!(state.sel, 1 | 2) {
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
            hint = if state.open_on_enter.is_some() {
                " ↵ open project   esc close"
            } else {
                " ↵ or esc close"
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn a_usb_stick_removed_during_copy_names_the_cause_and_leaves_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("stick/Book");
        let documents = temp.path().join("Documents");
        fs::create_dir_all(source.join("manuscript")).unwrap();
        fs::create_dir(&documents).unwrap();
        fs::write(source.join("manuscript/Scene.md"), "Words from USB.").unwrap();
        let incoming = Incoming {
            source: ImportSource::Usb {
                label: "BOOKS".into(),
                mount: source.parent().unwrap().to_path_buf(),
            },
            path: source.clone(),
            name: "Book".into(),
            project_id: None,
        };
        let mut removed = false;
        let error = book_copy::copy_book(
            &source,
            &documents.join("Book"),
            &AtomicBool::new(false),
            &mut |_| {
                if !removed {
                    // The open file can finish reading, but the mounted tree is gone
                    // before verification, just as when a stick is pulled out.
                    fs::rename(source.parent().unwrap(), temp.path().join("pulled")).unwrap();
                    removed = true;
                }
            },
        )
        .unwrap_err();
        assert!(removed);
        assert_eq!(import_error(&incoming, error), "The USB stick was removed.");
        assert_eq!(fs::read_dir(&documents).unwrap().count(), 0);
    }

    #[test]
    fn other_copy_errors_keep_their_cause() {
        let temp = tempfile::tempdir().unwrap();
        let incoming = Incoming {
            source: ImportSource::Laptop,
            path: temp.path().join("missing"),
            name: "Book".into(),
            project_id: None,
        };
        assert_eq!(
            import_error(&incoming, anyhow::anyhow!("disk is full")),
            "Project import failed: disk is full"
        );
    }

    fn identity(book: &Path, id: &str) {
        fs::create_dir_all(book.join(".grimoire")).unwrap();
        fs::write(book.join(".grimoire/sync.json"), id).unwrap();
    }

    #[test]
    fn a_copied_book_never_brings_another_books_sync_link() {
        let temp = tempfile::tempdir().unwrap();
        let book = temp.path().join("Book");
        identity(&book, "the original's");
        drop_sync_identity(&book).unwrap();
        assert!(!book.join(".grimoire/sync.json").exists());
        // A book without one is left as it is.
        drop_sync_identity(&book).unwrap();
        drop_sync_identity(&temp.path().join("No state")).unwrap();
    }

    #[test]
    fn after_replace_only_the_book_in_use_owns_the_drive_folder() {
        let temp = tempfile::tempdir().unwrap();
        let (new, old) = (
            temp.path().join("Book"),
            temp.path().join("Book (replaced)"),
        );
        fs::create_dir_all(&new).unwrap();
        // The replaced book's link moves to the book now in use.
        identity(&old, "local");
        settle_replaced_identity(&new, &old).unwrap();
        assert_eq!(fs::read(new.join(".grimoire/sync.json")).unwrap(), b"local");
        assert!(!old.join(".grimoire/sync.json").exists());
        // A link the new copy already has (from Drive's marker) wins; the old one goes.
        identity(&old, "stale");
        identity(&new, "from Drive");
        settle_replaced_identity(&new, &old).unwrap();
        assert_eq!(
            fs::read(new.join(".grimoire/sync.json")).unwrap(),
            b"from Drive"
        );
        assert!(!old.join(".grimoire/sync.json").exists());
    }
}
