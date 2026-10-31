//! Native setup, encrypted snapshots and restore-to-new-folder controls.
use super::*;
use crate::backup_service::{self as service, Request, Secret, Status};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    NewLocal,
    NewDrive,
    Connect,
    Restore,
    ExportKey,
    Google,
}
pub struct Form {
    pub kind: FormKind,
    fields: [Zeroizing<String>; 2],
    pub selected: usize,
    snapshot: String,
}
impl Form {
    fn field_count(&self) -> usize {
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
}
impl Backups {
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
    pub fn tick(&mut self) {
        let result = self.job.as_ref().map(|job| job.result.try_recv());
        match result {
            Some(Ok(Ok(status))) => {
                self.notice = status.notice.clone();
                self.status = status;
                self.job = None;
            }
            Some(Ok(Err(error))) => {
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
                };
                self.deck.shell.nav.back();
                self.deck.shell.backup.request(request);
            }
            _ => {}
        }
    }
}
