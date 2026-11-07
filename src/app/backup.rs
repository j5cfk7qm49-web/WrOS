//! Native setup, encrypted snapshots and restore-to-new-folder controls.
use super::*;
use crate::backup_service::{self as service, DriveEntry, QueuedBackup, Request, Secret, Status};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    NewLocal,
    NewDrive,
    Connect,
    Restore,
    RestoreWithSettings,
    ExportKey,
    Google,
    GoogleFile,
    DriveBrowse,
    DriveMaterialize,
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
            FormKind::RestoreWithSettings => "Restore snapshot and your settings into a NEW folder",
            FormKind::ExportKey => "Export recovery key to a safe separate place",
            FormKind::Google => "Connect Google Drive from a trusted computer",
            FormKind::GoogleFile => "Connect Google Drive from a file on a USB stick",
            FormKind::DriveBrowse => "Go to a Google Drive folder",
            FormKind::DriveMaterialize => "Copy from Google Drive to this laptop",
        }
    }
    /// What submitting the filled-in form asks the backup helper to do.
    pub fn request(mut self, project: &Path) -> Request {
        let project = || Some(project.to_string_lossy().into_owned());
        match self.kind {
            FormKind::NewLocal | FormKind::NewDrive => Request::Init {
                repository: self.fields[0].to_string(),
            },
            FormKind::Connect => Request::Connect {
                repository: self.fields[0].to_string(),
                key: self.fields[1].to_string(),
            },
            FormKind::Restore | FormKind::RestoreWithSettings => Request::Restore {
                settings: self.kind == FormKind::RestoreWithSettings,
                snapshot: self.snapshot,
                destination: self.fields[0].to_string(),
                project: project(),
            },
            FormKind::ExportKey => Request::ExportKey {
                destination: self.fields[0].to_string(),
            },
            FormKind::Google => Request::ConnectGoogle {
                authorization: Secret(std::mem::take(&mut self.fields[0])),
            },
            FormKind::GoogleFile => Request::ConnectGoogleFile {
                path: self.fields[0].to_string(),
            },
            FormKind::DriveBrowse => Request::DriveList {
                folder: self.fields[0].to_string(),
            },
            FormKind::DriveMaterialize => Request::DriveMaterialize {
                remote: self.fields[0].to_string(),
                destination: self.fields[1].to_string(),
                project: project(),
            },
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
            FormKind::Restore | FormKind::RestoreWithSettings => &["New folder (absolute path)"],
            FormKind::ExportKey => &["New recovery key file (absolute path)"],
            FormKind::Google => &["Paste authorization JSON (hidden)"],
            FormKind::GoogleFile => &["Authorization file (absolute path)"],
            FormKind::DriveBrowse => &["Google Drive folder (drive:Novels/Drafts)"],
            FormKind::DriveMaterialize => &[
                "From Google Drive",
                "To a new folder or file on this laptop",
            ],
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
        } else if matches!(self.kind, FormKind::Restore | FormKind::RestoreWithSettings) {
            rows.push("Existing folders are refused. Every restored file is hash checked.".into());
            if self.kind == FormKind::RestoreWithSettings {
                rows.push("Your current settings are kept in a dated folder first.".into());
            }
        } else if self.kind == FormKind::DriveBrowse {
            rows.push("drive: alone is the top of your Drive.".into());
        } else if self.kind == FormKind::DriveMaterialize {
            rows.push("Nothing on this laptop is written over; the place must be new.".into());
            rows.push("The copy is checked against Google Drive before it is kept.".into());
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
    /// Automatic backups of the open book, and whether the running job is one.
    pub auto: AutoBackup,
    pub auto_running: bool,
    /// What the running job is, when a tutorial lesson counts its outcome.
    pub(crate) running: Option<Finished>,
    /// The last such operation that finished well, until the app reads it.
    pub finished: Option<Finished>,
}

/// A finished backup operation a tutorial lesson can count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finished {
    Restore,
    /// Linking ended with Drive connected.
    LinkPhone,
}

/// When an automatic backup of the open book is due. Times are passed in, so the rules are
/// testable without a clock: after saved changes not yet in a verified backup, once the
/// writer has paused for `AUTO_PAUSE`, at most once per `AUTO_EVERY`, never with unsaved
/// words; failures retry after 15 minutes, an hour, then every four hours.
#[derive(Default)]
pub struct AutoBackup {
    /// Saves seen in this session, and how many of them a verified backup covers.
    saves: u64,
    backed_up: u64,
    /// The save count a running backup started from; saves after it still need a backup.
    started_at: Option<u64>,
    saw_dirty: bool,
    last_activity: Option<std::time::Instant>,
    last_run: Option<std::time::Instant>,
    failures: u32,
    pub retry_at: Option<std::time::Instant>,
    failing_since: Option<std::time::SystemTime>,
    pub failure_noticed: bool,
}
pub const AUTO_PAUSE: std::time::Duration = std::time::Duration::from_secs(10 * 60);
pub const AUTO_EVERY: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const AUTO_RETRY: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(15 * 60),
    std::time::Duration::from_secs(60 * 60),
    std::time::Duration::from_secs(4 * 60 * 60),
];
const AUTO_FAILING_NOTICE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

impl AutoBackup {
    /// Called every tick: typing is activity, and the end of unsaved words is a save.
    pub fn observe(&mut self, now: std::time::Instant, dirty: bool) {
        if dirty {
            self.last_activity = Some(now);
        } else if self.saw_dirty {
            self.saves += 1;
            self.last_activity = Some(now);
        }
        self.saw_dirty = dirty;
    }
    pub fn due(&self, now: std::time::Instant) -> bool {
        self.saves > self.backed_up
            && self.started_at.is_none()
            && !self.saw_dirty
            && self.last_activity.is_some_and(|t| now >= t + AUTO_PAUSE)
            && self.last_run.is_none_or(|t| now >= t + AUTO_EVERY)
            && self.retry_at.is_none_or(|t| now >= t)
    }
    pub fn start(&mut self, now: std::time::Instant) {
        self.started_at = Some(self.saves);
        self.last_run = Some(now);
    }
    pub fn finish(&mut self, ok: bool, now: std::time::Instant, wall: std::time::SystemTime) {
        let started = self.started_at.take().unwrap_or(self.backed_up);
        if ok {
            self.backed_up = started;
            self.failures = 0;
            self.retry_at = None;
            self.failing_since = None;
            self.failure_noticed = false;
        } else {
            self.failures += 1;
            let step = (self.failures as usize - 1).min(AUTO_RETRY.len() - 1);
            self.retry_at = Some(now + AUTO_RETRY[step]);
            self.last_run = None; // a retry is not held back by the hourly limit
            self.failing_since.get_or_insert(wall);
        }
    }
    pub fn failing_for_a_day(&self, wall: std::time::SystemTime) -> bool {
        self.failing_since
            .and_then(|since| wall.duration_since(since).ok())
            .is_some_and(|age| age >= AUTO_FAILING_NOTICE)
    }
    pub fn failing(&self) -> bool {
        self.failures > 0
    }
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
    /// Files a sync parked Drive's version beside, until they are settled.
    pub pending: Vec<crate::sync_service::Pending>,
    /// Parked copies already announced as needing a merge.
    pub announced: Vec<String>,
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
        let kind = match &request {
            Request::Restore { .. } => Some(Finished::Restore),
            Request::LinkPhone => Some(Finished::LinkPhone),
            _ => None,
        };
        match service::spawn(request, self.state.clone()) {
            Ok(job) => {
                self.job = Some(job);
                self.running = kind;
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
        let browser_path = self.state.join("drive-browser.json");
        if !browser_path.is_symlink()
            && let Ok(text) = fs::read_to_string(&browser_path)
            && let Ok(bdata) = serde_json::from_str::<serde_json::Value>(&text)
        {
            if let Some(folder) = bdata.get("folder").and_then(|v| v.as_str()) {
                self.status.drive_folder = folder.to_string();
            }
            if let Some(entries) = bdata.get("entries")
                && let Ok(parsed) = serde_json::from_value::<Vec<DriveEntry>>(entries.clone())
            {
                self.status.drive_entries = parsed;
            }
        }
    }
    fn finish_auto(&mut self, ok: bool) {
        if std::mem::take(&mut self.auto_running) {
            self.auto
                .finish(ok, std::time::Instant::now(), std::time::SystemTime::now());
        }
    }
    /// Backups are set up for this book once a repository exists and holds a verified
    /// snapshot of exactly this folder (read from the helper's own records, offline).
    pub fn set_up_for(&self, project: &Path) -> bool {
        let Ok(root) = project.canonicalize() else {
            return false;
        };
        self.state.join("repository.json").is_file()
            && fs::read_to_string(self.state.join("latest.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|latest| latest.get(root.to_string_lossy().as_ref()).is_some())
    }
    /// One line on this book's backups, for the Backups screen.
    pub fn auto_line(&self, project: &Path) -> String {
        if self.auto_running
            && let Some(job) = &self.job
        {
            return job.status_line().replacen("Working:", "Backing up:", 1);
        }
        if self.auto.failing() {
            let retry = self
                .auto
                .retry_at
                .map(|at| at.saturating_duration_since(std::time::Instant::now()))
                .map(|wait| {
                    (chrono::Local::now() + chrono::Duration::from_std(wait).unwrap_or_default())
                        .format("%H:%M")
                        .to_string()
                })
                .unwrap_or_else(|| "soon".into());
            return format!("Automatic backup did not finish · trying again at {retry}");
        }
        let root = project
            .canonicalize()
            .unwrap_or_else(|_| project.to_path_buf());
        let newest = self
            .status
            .snapshots
            .iter()
            .filter(|s| Path::new(&s._path) == root && !s.restore_checked_at.is_empty())
            .map(|s| s.time.clone())
            .max()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok());
        match newest {
            Some(time) => {
                let time = time.with_timezone(&chrono::Local);
                let when = if time.date_naive() == chrono::Local::now().date_naive() {
                    time.format("%H:%M").to_string()
                } else {
                    time.format("%-d %b %H:%M").to_string()
                };
                format!("Backed up {when} · verified · backs up again after you save and pause")
            }
            None if self.set_up_for(project) => {
                "Backs up automatically after you save and pause for 10 minutes".into()
            }
            None => "Automatic backups start once this book has a verified backup".into(),
        }
    }
    pub fn tick(&mut self) {
        let result = self.job.as_ref().map(|job| job.result.try_recv());
        match result {
            Some(Ok(Ok(status))) => {
                self.finished = self
                    .running
                    .take()
                    .filter(|kind| *kind != Finished::LinkPhone || status.google_connected);
                self.notice = status.notice.clone();
                self.status = status;
                self.status_ok = true;
                self.job = None;
                self.finish_auto(true);
            }
            Some(Ok(Err(error))) => {
                self.running = None;
                self.sync_local_metadata();
                self.notice = if self.auto_running {
                    format!("Automatic backup did not finish: {error}")
                } else {
                    error
                };
                self.job = None;
                self.finish_auto(false);
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.running = None;
                self.notice = "Backup worker stopped; refresh before retrying.".into();
                self.job = None;
                self.finish_auto(false);
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
            FormKind::Restore | FormKind::RestoreWithSettings => self
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
            FormKind::DriveBrowse => {
                if self.deck.shell.backup.status.drive_folder.is_empty() {
                    "drive:".into()
                } else {
                    self.deck.shell.backup.status.drive_folder.clone()
                }
            }
            FormKind::DriveMaterialize => {
                if snapshot.is_empty() {
                    "drive:".into()
                } else {
                    snapshot.clone()
                }
            }
            _ => String::new(),
        };
        let second = if kind == FormKind::DriveMaterialize && !snapshot.is_empty() {
            // Beside the open book, under the item's own name, numbered if that is taken.
            let inside = snapshot.strip_prefix("drive:").unwrap_or(&snapshot);
            let item_name = inside.trim_matches('/').rsplit('/').next().unwrap_or("");
            let item_name = if item_name.is_empty() {
                "Drive"
            } else {
                item_name
            };
            let dir = self.project.root.parent().unwrap_or(Path::new("/tmp"));
            let mut path = dir.join(format!("From Drive - {item_name}"));
            let mut n = 2;
            while path.exists() || path.is_symlink() {
                path = dir.join(format!("From Drive - {item_name} {n}"));
                n += 1;
            }
            path.to_string_lossy().into_owned()
        } else {
            String::new()
        };
        self.deck.shell.backup.form = Some(Form {
            kind,
            fields: [Zeroizing::new(first), Zeroizing::new(second)],
            selected: 0,
            snapshot,
        });
        self.deck.shell.nav.open(super::shell::Route::BackupForm);
        if kind == FormKind::GoogleFile {
            // The update helper's read-only WROS-UPDATE mount; its reply fills in the path.
            self.deck.request(wros_sys::Request::FindUpdateMedia);
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
                    if let Ok(reply) = &result {
                        sync.pending = reply.pending.clone();
                        if reply.downloaded + reply.deleted > 0 || !reply.pending.is_empty() {
                            self.tree_stale = true;
                        }
                    }
                    let synced = matches!(&result, Ok(reply) if reply.status == "synced");
                    let sync = &mut self.deck.shell.backup.sync;
                    sync.last = Some(result);
                    if synced {
                        self.observe_deck(grimoire_core::tutorial::Event::SyncCompleted);
                    }
                }
            }
            return;
        }
        self.merge_pending();
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
    /// Merge each parked Drive copy whose changes do not clash, once the book is
    /// re-read and saved; announce the ones that clash. Copies settled
    /// elsewhere drop out.
    fn merge_pending(&mut self) {
        if self.deck.shell.backup.sync.pending.is_empty()
            || self.tree_stale
            || self.project.dirty_count() > 0
        {
            return;
        }
        let root = self.project.root.clone();
        self.deck
            .shell
            .backup
            .sync
            .pending
            .retain(|p| root.join(&p.parked).exists());
        for pending in self.deck.shell.backup.sync.pending.clone() {
            let copy = root.join(&pending.parked);
            let Some(ci) = self.project.nodes.iter().position(|n| n.path == copy) else {
                continue; // not in the tree yet
            };
            let Some(base) = pending
                .merge_base
                .as_ref()
                .and_then(|b| fs::read_to_string(b).ok())
            else {
                continue; // no shared version: settled by hand in Conflicts
            };
            let (Ok(mine), Ok(theirs)) = (
                fs::read_to_string(root.join(&pending.path)),
                fs::read_to_string(&copy),
            ) else {
                continue;
            };
            let pieces = grimoire_core::merge::three_way(&base, &mine, &theirs);
            if grimoire_core::merge::clashes(&pieces) == 0 {
                let text = grimoire_core::merge::assemble(&pieces, &[]);
                self.finish_merge(copy, text, true);
            } else if !self
                .deck
                .shell
                .backup
                .sync
                .announced
                .contains(&pending.parked)
            {
                let title = self.original_of(ci).map_or_else(
                    || self.project.nodes[ci].title.clone(),
                    |o| self.project.nodes[o].title.clone(),
                );
                self.msg = format!(
                    "{title} needs merging: WrOS and Drive changed the same paragraph · settle it in Conflicts"
                );
                self.deck.shell.backup.sync.announced.push(pending.parked);
            }
        }
    }
    /// Start an automatic backup of the open book when one is due (see `AutoBackup`).
    pub(crate) fn tick_auto_backup(&mut self) {
        let now = std::time::Instant::now();
        let dirty = self.project.dirty_count() > 0;
        let auto = &mut self.deck.shell.backup.auto;
        auto.observe(now, dirty);
        if auto.failing_for_a_day(std::time::SystemTime::now()) && !auto.failure_noticed {
            auto.failure_noticed = true;
            self.msg =
                "Automatic backups have not finished for a day · see Settings › Backups".into();
        }
        let backup = &self.deck.shell.backup;
        if dirty || backup.job.is_some() || !backup.auto.due(now) || self.tree_stale {
            return;
        }
        if !self.conflicts().is_empty() || !self.deck.shell.backup.set_up_for(&self.project.root) {
            return;
        }
        let project = self.project.root.to_string_lossy().into_owned();
        let backup = &mut self.deck.shell.backup;
        backup.auto.start(now);
        backup.auto_running = true;
        backup.request(Request::AutoBackup { project });
        if backup.job.is_none() {
            backup.finish_auto(false);
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
                let form = self.deck.shell.backup.form.take().unwrap();
                let request = form.request(&self.project.root);
                self.deck.shell.nav.back();
                self.deck.shell.backup.request(request);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod auto_backup_tests {
    use super::{AUTO_EVERY, AUTO_PAUSE, AutoBackup};
    use std::time::{Duration, Instant, SystemTime};

    const MIN: Duration = Duration::from_secs(60);

    /// The writer types, saves at `t0`, then pauses.
    fn saved(t0: Instant) -> AutoBackup {
        let mut auto = AutoBackup::default();
        auto.observe(t0, true);
        auto.observe(t0, false);
        auto
    }

    #[test]
    fn a_backup_waits_for_a_ten_minute_pause_after_a_save() {
        let t0 = Instant::now();
        let mut auto = AutoBackup::default();
        auto.observe(t0, false);
        assert!(
            !auto.due(t0 + AUTO_PAUSE * 6),
            "nothing saved, nothing to back up"
        );
        let auto = saved(t0);
        assert!(!auto.due(t0 + 9 * MIN));
        assert!(auto.due(t0 + AUTO_PAUSE));
    }

    #[test]
    fn typing_again_restarts_the_pause_and_unsaved_words_never_back_up() {
        let t0 = Instant::now();
        let mut auto = saved(t0);
        auto.observe(t0 + 8 * MIN, true);
        assert!(
            !auto.due(t0 + 12 * MIN),
            "the writer is typing: words are unsaved"
        );
        auto.observe(t0 + 13 * MIN, false);
        assert!(!auto.due(t0 + 20 * MIN));
        assert!(auto.due(t0 + 23 * MIN));
    }

    #[test]
    fn at_most_once_an_hour_and_saves_made_during_a_run_are_not_lost() {
        let t0 = Instant::now();
        let mut auto = saved(t0);
        let start = t0 + AUTO_PAUSE;
        auto.start(start);
        assert!(!auto.due(start), "one run at a time");
        auto.observe(start + MIN, true);
        auto.observe(start + MIN, false);
        auto.finish(true, start + 2 * MIN, SystemTime::now());
        assert!(!auto.due(start + 30 * MIN), "at most once an hour");
        assert!(
            auto.due(start + AUTO_EVERY),
            "the save made during the run still needs a backup"
        );
        auto.start(start + AUTO_EVERY);
        auto.finish(true, start + AUTO_EVERY, SystemTime::now());
        assert!(
            !auto.due(start + AUTO_EVERY * 5),
            "everything saved is backed up"
        );
    }

    #[test]
    fn failures_back_off_fifteen_minutes_an_hour_then_four_hours() {
        let t0 = Instant::now();
        let wall = SystemTime::now();
        let mut auto = saved(t0);
        let mut at = t0 + AUTO_PAUSE;
        for wait in [15 * MIN, 60 * MIN, 240 * MIN, 240 * MIN] {
            assert!(auto.due(at));
            auto.start(at);
            auto.finish(false, at, wall);
            assert!(!auto.due(at + wait - MIN));
            at += wait;
        }
        assert!(!auto.failing_for_a_day(wall + Duration::from_secs(23 * 3600)));
        assert!(auto.failing_for_a_day(wall + Duration::from_secs(25 * 3600)));
        auto.start(at);
        auto.finish(true, at, wall);
        assert!(!auto.failing_for_a_day(wall + Duration::from_secs(48 * 3600)));
    }
}
