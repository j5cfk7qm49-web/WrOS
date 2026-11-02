//! Native setup, encrypted snapshots and restore-to-new-folder controls.
use super::*;
use crate::backup_service::{self as service, QueuedBackup, Request, Secret, Status};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    NewLocal,
    NewDrive,
    Connect,
    Restore,
    ExportKey,
    Google,
    GoogleFile,
}
/// What the file form looks for on a mounted `WROS-UPDATE` stick.
pub const GOOGLE_TOKEN_FILE: &str = "wros-drive-token.json";
pub struct Form {
    pub kind: FormKind,
    fields: [Zeroizing<String>; 2],
    pub selected: usize,
    snapshot: String,
}
impl Form {
    pub(crate) fn field_count(&self) -> usize {
        if self.kind == FormKind::Connect { 2 } else { 1 }
    }
    fn secret(&self) -> bool {
        self.kind == FormKind::Google
    }
    pub fn title(&self) -> &'static str {
        match self.kind {
            FormKind::NewLocal => "New encrypted backup on a local drive",
            FormKind::NewDrive => "New encrypted backup on Google Drive",
            FormKind::Connect => "Connect an existing encrypted repository",
            FormKind::Restore => "Restore snapshot into a NEW folder",
            FormKind::ExportKey => "Export recovery key to a safe separate place",
            FormKind::Google => "Connect Google Drive from a trusted computer",
            FormKind::GoogleFile => "Connect Google Drive from a file on a USB stick",
        }
    }
    pub fn rows(&self) -> Vec<String> {
        let labels: &[&str] = match self.kind {
            FormKind::NewLocal => &["New repository folder (absolute path)"],
            FormKind::NewDrive => &["Google Drive destination (drive:Folder)"],
            FormKind::Connect => &[
                "Repository (absolute path or drive:Folder)",
                "Recovery key file",
            ],
            FormKind::Restore => &["New folder (absolute path)"],
            FormKind::ExportKey => &["New recovery key file (absolute path)"],
            FormKind::Google => &["Paste authorization JSON (hidden)"],
            FormKind::GoogleFile => &["Authorization file (absolute path)"],
        };
        let mut rows = labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                format!(
                    "{label}: {}",
                    if self.secret() && !self.fields[i].is_empty() {
                        "[hidden]"
                    } else {
                        self.fields[i].as_str()
                    }
                )
            })
            .collect::<Vec<_>>();
        rows.push("Confirm and continue".into());
        if self.kind == FormKind::Google {
            rows.push("On a trusted computer: rclone authorize drive".into());
            rows.push("Approve in your browser; paste the returned JSON here.".into());
            rows.push("Stored privately outside manuscripts. Escape discards it.".into());
        } else if self.kind == FormKind::GoogleFile {
            rows.push("On a trusted computer: rclone authorize drive, then save only".into());
            rows.push(format!(
                "the returned JSON as {GOOGLE_TOKEN_FILE} on a stick labelled WROS-UPDATE."
            ));
            rows.push(
                "The stick is mounted read-only and the path filled in; or type a path.".into(),
            );
            rows.push("Delete the file from the stick once Google Drive is connected.".into());
        } else if matches!(self.kind, FormKind::NewLocal | FormKind::NewDrive) {
            rows.push("Creates an encrypted repository and a private recovery key.".into());
            rows.push("Export the key separately; losing it makes backups unreadable.".into());
        } else if self.kind == FormKind::Restore {
            rows.push("Existing folders are refused. Every restored file is hash checked.".into());
        }
        rows
    }
}

#[derive(Default)]
pub struct Backups {
    state: PathBuf,
    pub status: Status,
    pub form: Option<Form>,
    pub notice: String,
    pub job: Option<service::Job>,
    pub sync: SyncState,
    /// True once a backup-service reply has been read since `status_ok` was
    /// last cleared. The update gate clears it before asking for fresh status
    /// so a stale or never-loaded status can never be mistaken for "no backups".
    pub status_ok: bool,
}
/// Readable Google Drive sync of the open book.
#[derive(Default)]
pub struct SyncState {
    pub job: Option<service::Job<crate::sync_service::Reply>>,
    /// The operation the running job performs, so its reply is handled right.
    pub running: Option<crate::sync_service::Request>,
    pub last: Option<Result<crate::sync_service::Reply, String>>,
    /// When the next sync is due; `None` until the open book is first seen.
    pub next: Option<std::time::Instant>,
    /// Whether the book had unsaved words at the last tick (a save ends that).
    pub saw_dirty: bool,
    /// Test seam: a stand-in helper and its environment.
    pub script: Option<PathBuf>,
    pub env: Vec<(&'static str, PathBuf)>,
}
/// Every few minutes while sync is on, and shortly after each save.
pub const SYNC_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);
pub const SYNC_AFTER_SAVE: std::time::Duration = std::time::Duration::from_secs(10);

impl SyncState {
    /// One line on where sync stands, for the Backups screen.
    pub fn line(&self) -> String {
        if let Some(job) = &self.job {
            return job.status_line().replacen("Working:", "Syncing:", 1);
        }
        match &self.last {
            None => "Not synced yet".into(),
            Some(Err(error)) => format!("Sync did not finish: {error}"),
            Some(Ok(reply)) => match reply.status.as_str() {
                "synced" => {
                    let when = reply
                        .synced_at
                        .as_deref()
                        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                        .unwrap_or_else(|| "just now".into());
                    let mut line = format!(
                        "Synced {when} · {} up, {} down, {} deleted",
                        reply.uploaded, reply.downloaded, reply.deleted
                    );
                    match reply.pending.len() {
                        0 => {}
                        1 => line.push_str(" · 1 file needs merging"),
                        n => line.push_str(&format!(" · {n} files need merging")),
                    }
                    line
                }
                "confirm" => format!("Stopped: this sync would delete {} files", reply.deletes),
                "offline" => "Waiting: Google Drive could not be reached".into(),
                _ => reply.message.clone(),
            },
        }
    }
    /// Deletions the last run stopped for, if it did.
    pub fn confirm_deletes(&self) -> Option<u32> {
        match &self.last {
            Some(Ok(reply)) if reply.status == "confirm" => Some(reply.deletes),
            _ => None,
        }
    }
}

/// How recent a project snapshot must be to count as a backup taken for an update.
pub const FRESH_BACKUP_HOURS: i64 = 24;
impl Backups {
    /// Time of the newest snapshot of exactly this project folder that is at
    /// most `FRESH_BACKUP_HOURS` old, judged against `now`.
    pub fn fresh_snapshot(
        &self,
        project: &Path,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<String> {
        self.status
            .snapshots
            .iter()
            .filter(|snapshot| Path::new(&snapshot._path) == project)
            .filter_map(|snapshot| {
                let taken = chrono::DateTime::parse_from_rfc3339(&snapshot.time).ok()?;
                let age = now.signed_duration_since(taken);
                (age >= chrono::Duration::hours(-1)
                    && age <= chrono::Duration::hours(FRESH_BACKUP_HOURS))
                .then(|| snapshot.time.clone())
            })
            .max()
    }
    pub fn new(root: &Path, background: bool) -> Self {
        Self {
            state: if background {
                grimoire_core::paths::data_dir().join("wros/backup")
            } else {
                root.join(".grimoire/backup-test-state")
            },
            ..Self::default()
        }
    }
    pub fn request(&mut self, request: Request) {
        self.tick();
        if self.job.is_some() {
            self.notice = "A backup operation is already running.".into();
            return;
        }
        match service::spawn(request, self.state.clone()) {
            Ok(job) => {
                self.job = Some(job);
                self.notice = "Backup operation running; Escape returns to writing.".into();
            }
            Err(error) => self.notice = error,
        }
    }
    fn sync_local_metadata(&mut self) {
        let account_path = self.state.join("google-account.json");
        if !account_path.is_symlink()
            && let Ok(text) = fs::read_to_string(&account_path)
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text)
        {
            if let Some(state) = meta.get("state").and_then(|v| v.as_str()) {
                self.status.google_state = state.to_string();
            }
            if let Some(verified) = meta.get("verified_at").and_then(|v| v.as_str()) {
                self.status.google_verified_at = verified.to_string();
            }
            if let Some(expiry) = meta.get("token_expiry").and_then(|v| v.as_str()) {
                self.status.google_expiry = expiry.to_string();
            }
        }
        let queue_path = self.state.join("queue.json");
        if !queue_path.is_symlink()
            && let Ok(text) = fs::read_to_string(&queue_path)
            && let Ok(qdata) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(items) = qdata.get("items")
            && let Ok(parsed) = serde_json::from_value::<Vec<QueuedBackup>>(items.clone())
        {
            self.status.queued_backups = parsed;
        }
    }
    pub fn tick(&mut self) {
        let result = self.job.as_ref().map(|job| job.result.try_recv());
        match result {
            Some(Ok(Ok(status))) => {
                self.notice = status.notice.clone();
                self.status = status;
                self.status_ok = true;
                self.job = None;
            }
            Some(Ok(Err(error))) => {
                self.sync_local_metadata();
                self.notice = error;
                self.job = None;
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.notice = "Backup worker stopped; refresh before retrying.".into();
                self.job = None;
            }
            _ => {}
        }
    }
}

impl App {
    pub fn open_backup_form(&mut self, kind: FormKind, snapshot: String) {
        if self.deck.shell.backup.job.is_some() {
            return;
        }
        let first = match kind {
            FormKind::NewDrive => "drive:WrOS-Backups".into(),
            FormKind::Restore => self
                .project
                .root
                .parent()
                .unwrap_or(Path::new("/tmp"))
                .join(format!(
                    "{}-restored-{}",
                    self.project
                        .root
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    &snapshot[..snapshot.len().min(8)]
                ))
                .to_string_lossy()
                .into_owned(),
            FormKind::Connect => self.deck.shell.backup.status.repository.clone(),
            _ => String::new(),
        };
        self.deck.shell.backup.form = Some(Form {
            kind,
            fields: [Zeroizing::new(first), Zeroizing::new(String::new())],
            selected: 0,
            snapshot,
        });
        self.deck.shell.nav.open(super::shell::Route::BackupForm);
        if kind == FormKind::GoogleFile {
            // The update helper's read-only WROS-UPDATE mount; its reply fills in the path.
            self.deck.request(writingdeck_sys::Request::FindUpdateMedia);
        }
    }
    /// The update helper's reply while the file form is open. Returns false when the form is
    /// not open, so the update flow handles the reply instead.
    pub(crate) fn on_google_file_media(&mut self, media: Result<Option<String>, &str>) -> bool {
        let open = self.deck.shell.nav.current() == super::shell::Route::BackupForm;
        let Some(form) = self
            .deck
            .shell
            .backup
            .form
            .as_mut()
            .filter(|f| open && f.kind == FormKind::GoogleFile)
        else {
            return false;
        };
        self.deck.shell.backup.notice = match media {
            Ok(Some(path)) => {
                if form.fields[0].is_empty() {
                    form.fields[0].push_str(&format!("{path}/{GOOGLE_TOKEN_FILE}"));
                }
                format!("Stick mounted read-only at {path}. Check the path, then confirm.")
            }
            Ok(None) => "No stick labelled WROS-UPDATE was found. Type the file's path.".into(),
            Err(error) => {
                format!("Could not look for a WROS-UPDATE stick: {error}. Type the file's path.")
            }
        };
        true
    }
    /// Sync is on for the open book once `wros-sync enable` recorded its identity.
    pub(crate) fn sync_enabled(&self) -> bool {
        self.project.root.join(".grimoire/sync.json").is_file()
    }
    pub(crate) fn request_sync(&mut self, request: crate::sync_service::Request) {
        let sync = &mut self.deck.shell.backup.sync;
        if sync.job.is_some() {
            return;
        }
        let Some(script) = sync.script.clone().or_else(crate::sync_service::script) else {
            sync.last = Some(Err(
                "the WrOS Google Drive sync helper is not installed".into()
            ));
            return;
        };
        match crate::sync_service::spawn(request.clone(), script, sync.env.clone()) {
            Ok(job) => {
                sync.job = Some(job);
                sync.running = Some(request);
            }
            Err(error) => sync.last = Some(Err(error)),
        }
    }
    pub(crate) fn request_sync_run(&mut self, allow_deletes: bool) {
        let project = self.project.root.to_string_lossy().into_owned();
        self.request_sync(crate::sync_service::Request::Sync {
            project,
            allow_deletes,
        });
    }
    pub(crate) fn request_sync_switch(&mut self, on: bool) {
        let project = self.project.root.to_string_lossy().into_owned();
        self.request_sync(if on {
            crate::sync_service::Request::Enable { project }
        } else {
            crate::sync_service::Request::Disable { project }
        });
    }
    /// Collect a finished run, and start one when due: on opening a book that
    /// syncs, shortly after a save, and every few minutes. Never with unsaved words.
    pub(crate) fn tick_sync(&mut self) {
        use crate::sync_service::Request as Sync;
        let sync = &mut self.deck.shell.backup.sync;
        let finished = sync
            .job
            .as_ref()
            .and_then(|job| match job.result.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("the sync worker stopped".to_string()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = finished {
            sync.job = None;
            match (sync.running.take(), result) {
                (Some(Sync::Enable { .. }), Ok(_)) => self.request_sync_run(false),
                (Some(Sync::Disable { .. }), result) => {
                    sync.next = None;
                    sync.last = Some(result);
                }
                (_, result) => {
                    sync.next = Some(std::time::Instant::now() + SYNC_EVERY);
                    sync.last = Some(result);
                }
            }
            return;
        }
        if !self.sync_enabled() {
            return;
        }
        let dirty = self.project.dirty_count() > 0;
        let sync = &mut self.deck.shell.backup.sync;
        let now = std::time::Instant::now();
        if sync.saw_dirty && !dirty {
            let soon = now + SYNC_AFTER_SAVE;
            sync.next = Some(sync.next.map_or(soon, |next| next.min(soon)));
        }
        sync.saw_dirty = dirty;
        let next = *sync.next.get_or_insert(now);
        if !dirty && sync.job.is_none() && now >= next {
            self.request_sync_run(false);
        }
    }
    pub fn request_project_backup(&mut self) {
        if self.deck.shell.backup.job.is_some() {
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck.shell.backup.notice =
                "Backup not started: save buffers and resolve source conflicts first.".into();
            return;
        }
        self.save_deck();
        self.deck.shell.backup.request(Request::Backup {
            project: self.project.root.to_string_lossy().into_owned(),
        });
    }
    pub fn request_project_backup_queue(&mut self) {
        if self.deck.shell.backup.job.is_some() {
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck.shell.backup.notice =
                "Backup queue not started: save buffers and resolve source conflicts first.".into();
            return;
        }
        self.save_deck();
        self.deck.shell.backup.request(Request::Queue {
            project: self.project.root.to_string_lossy().into_owned(),
        });
    }
    pub fn request_run_backup_queue(&mut self) {
        if self.deck.shell.backup.job.is_some() {
            return;
        }
        // The queue can contain this open book. Apply the ordinary backup save
        // and conflict gate before allowing its saved files to be transmitted.
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck.shell.backup.notice =
                "Backup queue not started: save buffers and resolve source conflicts first.".into();
            return;
        }
        self.save_deck();
        self.deck.shell.backup.request(Request::RunQueue);
    }
    pub fn paste_backup_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.backup.form else {
            return;
        };
        if form.selected >= form.field_count() {
            return;
        }
        let field = &mut form.fields[form.selected];
        let limit = if form.kind == FormKind::Google {
            32768
        } else {
            4096
        };
        if field.len() + text.len() <= limit && !text.contains('\0') {
            if form.kind == FormKind::Google {
                field.push_str(text.trim());
            } else if !text.chars().any(char::is_control) {
                field.push_str(text);
            }
        }
    }
    pub fn on_backup_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.backup.form else {
            return;
        };
        let count = form.field_count();
        match key {
            Key::Esc => {
                self.deck.shell.backup.form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % (count + 1),
            Key::Up => {
                form.selected = if form.selected == 0 {
                    count
                } else {
                    form.selected - 1
                }
            }
            Key::Backspace if form.selected < count => {
                form.fields[form.selected].pop();
            }
            Key::Delete if form.selected < count => {
                form.fields[form.selected].clear();
            }
            Key::Char(c) if !c.is_control() && form.selected < count => {
                self.paste_backup_form(&c.to_string())
            }
            Key::Enter if form.selected < count => form.selected += 1,
            Key::Enter => {
                let mut form = self.deck.shell.backup.form.take().unwrap();
                let request = match form.kind {
                    FormKind::NewLocal | FormKind::NewDrive => Request::Init {
                        repository: form.fields[0].to_string(),
                    },
                    FormKind::Connect => Request::Connect {
                        repository: form.fields[0].to_string(),
                        key: form.fields[1].to_string(),
                    },
                    FormKind::Restore => Request::Restore {
                        snapshot: form.snapshot,
                        destination: form.fields[0].to_string(),
                        project: Some(self.project.root.to_string_lossy().into_owned()),
                    },
                    FormKind::ExportKey => Request::ExportKey {
                        destination: form.fields[0].to_string(),
                    },
                    FormKind::Google => Request::ConnectGoogle {
                        authorization: Secret(std::mem::take(&mut form.fields[0])),
                    },
                    FormKind::GoogleFile => Request::ConnectGoogleFile {
                        path: form.fields[0].to_string(),
                    },
                };
                self.deck.shell.nav.back();
                self.deck.shell.backup.request(request);
            }
            _ => {}
        }
    }
}
