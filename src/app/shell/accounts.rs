//! Local accounts, Google Drive and backup pages.

use super::navigation::{links, page_label};
use super::{App, Choice, Entry, Key, Route};
use crate::palette::Action;
use zeroize::Zeroizing;

/// Change the writer's own password (#121). The three fields stay hidden and
/// are cleared as soon as they are sent or the form closes.
pub(crate) struct PasswordForm {
    current: Zeroizing<String>,
    new: Zeroizing<String>,
    repeat: Zeroizing<String>,
    pub(super) selected: usize,
}
impl PasswordForm {
    pub(super) const ACTIONS: usize = 5;
    pub(super) fn new() -> PasswordForm {
        PasswordForm {
            current: Zeroizing::new(String::new()),
            new: Zeroizing::new(String::new()),
            repeat: Zeroizing::new(String::new()),
            selected: 0,
        }
    }
    fn field(&mut self) -> Option<&mut Zeroizing<String>> {
        match self.selected {
            0 => Some(&mut self.current),
            1 => Some(&mut self.new),
            2 => Some(&mut self.repeat),
            _ => None,
        }
    }
    fn clear(&mut self) {
        for field in [&mut self.current, &mut self.new, &mut self.repeat] {
            *field = Zeroizing::new(String::new());
        }
        self.selected = 0;
    }
    pub(crate) fn rows(&self) -> Vec<String> {
        let shown = |s: &str| if s.is_empty() { "" } else { "[hidden]" };
        vec![
            format!("Current password: {}", shown(&self.current)),
            format!("New password: {}", shown(&self.new)),
            format!("Repeat new password: {}", shown(&self.repeat)),
            "Cancel and return to local accounts".into(),
            "Change password".into(),
            "This changes the password you sign in and unlock with. The disk password at start-up stays the same."
                .into(),
            "Input stays hidden. Escape discards this form.".into(),
        ]
    }
}

/// When the newest listed snapshot was taken, and what is still queued.
fn backup_overview(status: &crate::backup_service::Status) -> String {
    let newest = match status.snapshots.iter().map(|s| s.time.as_str()).max() {
        Some(time) => format!(
            "Newest snapshot: {}",
            time.get(..16).unwrap_or(time).replace('T', " ")
        ),
        None => "No snapshots listed yet; refresh snapshots to check".into(),
    };
    match status.queued_backups.len() {
        0 => newest,
        1 => format!("{newest} · 1 queued backup waiting"),
        n => format!("{newest} · {n} queued backups waiting"),
    }
}

/// Browsing and copying from the app's Google Drive folder, at the end of the Google Drive page.
/// The Drive files page: one Drive folder at a time, folders first. A folder opens;
/// a file, or an ordinary folder, copies to a new place on this laptop. Books use Import.
pub(super) fn drive_rows(app: &App) -> Vec<Entry> {
    use crate::app::backup::FormKind;
    let backup = &app.deck.shell.backup;
    if !backup.status.google_connected {
        return vec![Entry::info(
            "Link Google Drive first, on the Google Drive page.",
        )];
    }
    let folder = backup.status.drive_folder.as_str();
    let inside = folder
        .strip_prefix("drive:")
        .unwrap_or("")
        .trim_matches('/');
    let mut rows = vec![Entry::info(if inside.is_empty() {
        "My Drive".to_string()
    } else {
        format!("My Drive › {}", inside.replace('/', " › "))
    })];
    if folder.is_empty() {
        if backup.job.is_some() {
            rows.push(Entry::info("Reading your Drive…"));
        } else {
            rows.push(Entry::new(
                "Retry loading My Drive",
                Choice::DriveFolder("drive:".into()),
            ));
        }
        return rows;
    }
    let holds_book = backup
        .status
        .drive_entries
        .iter()
        .any(|e| e.is_dir && e.name == "manuscript");
    if !inside.is_empty() {
        let up = inside.rsplit_once('/').map_or("", |(up, _)| up);
        rows.push(Entry::new(
            "Up one folder",
            Choice::DriveFolder(format!("drive:{up}")),
        ));
        if holds_book {
            rows.push(Entry::new(
                "Import this project…",
                Choice::DriveCopyBook(folder.to_string()),
            ));
        } else {
            rows.push(Entry::new(
                "Copy this whole folder to this laptop",
                Choice::BackupForm(FormKind::DriveMaterialize, folder.to_string()),
            ));
        }
    }
    for entry in &backup.status.drive_entries {
        let mut label = if entry.is_dir {
            format!("{}/", entry.name)
        } else if entry.google_doc {
            format!("{} · Google Doc", entry.name)
        } else {
            format!("{} · {}", entry.name, size_label(entry.size))
        };
        if !entry.local.is_empty() {
            label += if entry.cached {
                " · on this laptop"
            } else {
                " · copy on this laptop"
            };
        }
        let choice = if entry.is_dir {
            Choice::DriveFolder(entry.path.clone())
        } else {
            Choice::BackupForm(FormKind::DriveMaterialize, entry.path.clone())
        };
        rows.push(Entry::new(label, choice));
        let local = std::path::Path::new(&entry.local);
        if entry.is_dir && local.is_absolute() && crate::books::is_book(local) {
            rows.push(Entry::new(
                format!("Open the copy of {}", entry.name),
                Choice::Action(Action::OpenBookAt(local.to_path_buf())),
            ));
        }
    }
    if backup.status.drive_entries.is_empty() {
        rows.push(Entry::info("This folder is empty."));
    }
    rows.push(Entry::new(
        "Go to a Drive folder by name",
        Choice::BackupForm(FormKind::DriveBrowse, String::new()),
    ));
    rows.push(Entry::info(
        "Copies are checked against Google Drive. Your projects stay on this laptop.",
    ));
    rows
}

/// "12 KB", "1.2 MB".
fn size_label(bytes: u64) -> String {
    match bytes {
        b if b >= 1_048_576 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} bytes"),
    }
}

pub(super) fn change_password_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .password_form
        .as_ref()
        .map(|form| form.rows().into_iter().map(Entry::info).collect())
        .unwrap_or_default()
}

pub(super) fn local_accounts_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::new(
        "Refresh local accounts",
        Choice::Read(wros_sys::Request::ListLocalAccounts),
    )];
    if let Some(accounts) = &app.deck.shell.local_accounts {
        for account in accounts {
            rows.push(Entry::info(format!(
                "{}{}\nUID {} · Home: {}",
                account.username,
                if account.is_current {
                    " (this session)"
                } else {
                    ""
                },
                account.uid,
                account.home
            )));
        }
        if accounts.is_empty() {
            rows.push(Entry::info(
                "No local login accounts found in the supported UID range.",
            ));
        }
    } else {
        rows.push(Entry::info(
            "Local accounts unavailable; refresh to try again.",
        ));
    }
    rows.push(Entry::new("Change your password", Choice::PasswordOpen));
    rows.push(Entry::info(
        "Other accounts are added from the maintenance console (Ctrl+Alt+F2).",
    ));
    rows
}

pub(super) fn google_account_rows(app: &App) -> Vec<Entry> {
    let backup = &app.deck.shell.backup;
    let mut rows = vec![Entry::info(if backup.status.google_connected {
        "Google Drive credentials saved privately on this device"
    } else {
        "Google Drive is not connected"
    })];
    if backup.status.phone_linking {
        rows.push(Entry::new(
            if backup.status.google_connected {
                "Relink Google Drive with your phone"
            } else {
                "Link Google Drive with your phone"
            },
            Choice::LinkPhone,
        ));
        if backup.status.google_connected && backup.status.google_shared_client {
            rows.push(Entry::info(
                "Drive uses rclone's shared Google access, which Google often slows down. Relink with your phone to use WrOS's own.",
            ));
        }
    }
    if backup.status.google_connected {
        if !backup.status.google_state.is_empty() {
            rows.push(Entry::info(format!(
                "Credential state: {}{}",
                backup.status.google_state,
                if backup.status.google_verified_at.is_empty() {
                    String::new()
                } else {
                    format!(" · verified {}", backup.status.google_verified_at)
                }
            )));
        }
        if !backup.status.google_expiry.is_empty() {
            rows.push(Entry::info(format!(
                "Token expiry: {}",
                backup.status.google_expiry
            )));
        }
        rows.push(Entry::new(
            "Verify Google Drive connection",
            Choice::BackupRequest(crate::backup_service::Request::CheckGoogle),
        ));
        rows.push(Entry::new(
            "Reauthorize Google Drive",
            Choice::BackupForm(crate::app::backup::FormKind::Google, String::new()),
        ));
        rows.push(Entry::new(
            "Reauthorize Google Drive from a file on a USB stick",
            Choice::BackupForm(crate::app::backup::FormKind::GoogleFile, String::new()),
        ));
        rows.push(Entry::new(
            "Disconnect Google Drive and remove local credentials",
            Choice::Open(Route::GoogleDisconnectConfirm),
        ));
    } else {
        rows.push(Entry::new(
            "Connect Google Drive",
            Choice::BackupForm(crate::app::backup::FormKind::Google, String::new()),
        ));
        rows.push(Entry::new(
            "Connect Google Drive from a file on a USB stick",
            Choice::BackupForm(crate::app::backup::FormKind::GoogleFile, String::new()),
        ));
    }
    let folder = &backup.status.drive_folder;
    rows.push(Entry::new(
        page_label(Route::GoogleDrive),
        Choice::DriveFolder(if folder.is_empty() {
            "drive:".into()
        } else {
            folder.clone()
        }),
    ));
    rows
}

pub(super) fn link_phone_rows(app: &App) -> Vec<Entry> {
    let backup = &app.deck.shell.backup;
    let mut rows = Vec::new();
    match &backup.job {
        Some(job) => {
            rows.push(Entry::info(job.status_line()));
            rows.push(Entry::new("Cancel linking", Choice::BackupCancel));
        }
        None => rows.push(Entry::info(if backup.status.google_connected {
            "Google Drive is linked. Escape returns to Settings."
        } else {
            "Linking stopped. Choose Link Google Drive with your phone to start again."
        })),
    }
    rows
}

pub(super) fn google_disconnect_rows() -> Vec<Entry> {
    vec![
        Entry::new(
            "Cancel and keep saved Google Drive credentials",
            Choice::GoogleDisconnectResolve(false),
        ),
        Entry::new(
            "Disconnect and erase local Google Drive credentials",
            Choice::GoogleDisconnectResolve(true),
        ),
        Entry::info("Local manuscripts, recovery keys and encrypted snapshots are preserved."),
        Entry::info("Revoke access in your Google account settings to end the provider grant."),
    ]
}

pub(super) fn sync_rows(app: &App) -> Vec<Entry> {
    vec![
        Entry::info("Readable Google Drive sync is set per project, in Settings › Backups."),
        Entry::info(if app.sync_enabled() {
            "This project syncs with Google Drive; Backups shows its latest result."
        } else {
            "This project is not synced; its words stay on this computer and in encrypted \
         backups."
        }),
    ]
}

pub(super) fn backup_rows(app: &App) -> Vec<Entry> {
    use crate::app::backup::FormKind;
    use crate::backup_service::Request;
    let backup = &app.deck.shell.backup;
    let mut rows = vec![
        Entry::info(backup.auto_line(&app.project.root)),
        Entry::new(
            "Create encrypted backup of this saved project",
            Choice::BackupNow,
        ),
        Entry::new("Refresh snapshots", Choice::BackupRequest(Request::List)),
    ];
    if app.commonplace.store_exists() {
        rows.push(Entry::new(
            "Create encrypted backup of Commonplace",
            Choice::BackupCommonplace,
        ));
    }
    rows.extend(links(Route::Backups));
    // Keep action positions stable when an asynchronous job finishes.
    rows.insert(
        0,
        if backup.job.is_some() {
            Entry::new("Cancel current backup operation", Choice::BackupCancel)
        } else {
            Entry::info("No backup operation is running")
        },
    );
    // Always present, so positions stay stable here too.
    rows.insert(
        1,
        Entry::info(match &backup.job {
            Some(job) => job.status_line(),
            None => backup_overview(&backup.status),
        }),
    );
    if backup.status.configured {
        rows.push(Entry::info(format!(
            "Destination: {}",
            backup.status.repository
        )));
        rows.push(Entry::info(format!(
            "Private recovery key: {}",
            backup.status.recovery_key
        )));
    } else {
        rows.push(Entry::info(
            "No repository configured. Choose a local drive or connect Google Drive.",
        ));
    }
    rows.push(Entry::info(if backup.status.google_connected {
        "Google Drive connection saved privately"
    } else {
        "Google Drive is not connected"
    }));
    if backup.status.google_connected {
        if app.sync_enabled() {
            rows.push(Entry::new("Sync now", Choice::SyncNow));
            if let Some(deletes) = backup.sync.confirm_deletes() {
                rows.push(Entry::new(
                    format!("Sync and delete {deletes} files"),
                    Choice::SyncConfirmDeletes,
                ));
            }
            rows.push(Entry::info(backup.sync.line()));
            rows.push(Entry::new(
                "Stop syncing this project",
                Choice::SyncSwitch(false),
            ));
        } else {
            rows.push(Entry::new(
                "Sync this project with Google Drive (readable files)",
                Choice::SyncSwitch(true),
            ));
            rows.push(Entry::info(
                "Readable copies in Drive are not encrypted: anyone with access to your Google account can read them.",
            ));
        }
    }
    rows.push(Entry::info(
        "Select a snapshot below to restore into a NEW folder.",
    ));
    rows.push(Entry::info(
        "Keep local backups on another device; folders can share the same disk.",
    ));
    // The fixed notice line is clipped at 80 columns. Keep the complete
    // result, including recovery paths, available in the scrollable body.
    let detail: Vec<char> = backup
        .notice
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let width = usize::from(app.screen.0.saturating_sub(8)).max(8) / 2;
    for chunk in detail.chunks(width) {
        rows.push(Entry::info(chunk.iter().collect::<String>()));
    }
    for snapshot in &backup.status.snapshots {
        rows.push(Entry::new(
            format!(
                "{} · {} · {}\n{}",
                snapshot.project,
                snapshot.time,
                snapshot.id.get(..8).unwrap_or("unknown"),
                if snapshot.restore_checked_at.is_empty() {
                    "Restore has not been checked on this device".into()
                } else {
                    format!("Restore checked: {}", snapshot.restore_checked_at)
                }
            ),
            Choice::BackupForm(FormKind::Restore, snapshot.id.clone()),
        ));
        if snapshot.has_profile {
            rows.push(Entry::new(
                "  ↳ Restore with your settings",
                Choice::BackupForm(FormKind::RestoreWithSettings, snapshot.id.clone()),
            ));
        }
    }
    for snapshot in &backup.status.brain_snapshots {
        rows.push(Entry::new(
            format!(
                "Restore Commonplace · {} · {}",
                snapshot.time,
                snapshot.id.get(..8).unwrap_or("unknown")
            ),
            Choice::BackupForm(FormKind::RestoreCommonplace, snapshot.id.clone()),
        ));
    }
    rows
}

pub(super) fn backup_destination_rows(app: &App) -> Vec<Entry> {
    use crate::app::backup::FormKind;
    let mut rows = vec![
        Entry::new(
            "New backup on a local drive",
            Choice::BackupForm(FormKind::NewLocal, String::new()),
        ),
        Entry::new(
            "New backup on Google Drive",
            Choice::BackupForm(FormKind::NewDrive, String::new()),
        ),
        Entry::new(
            "Connect existing repository and recovery key",
            Choice::BackupForm(FormKind::Connect, String::new()),
        ),
    ];
    if !app.deck.shell.backup.status.google_connected {
        rows.push(Entry::new(
            "Link Google Drive first",
            Choice::Open(Route::GoogleAccount),
        ));
    }
    rows.push(Entry::info(
        "Keep local backups on another device; folders can share the same disk.",
    ));
    rows
}

pub(super) fn backup_key_rows(app: &App) -> Vec<Entry> {
    use crate::app::backup::FormKind;
    use crate::backup_service::Request;
    let backup = &app.deck.shell.backup;
    let mut rows = vec![
        Entry::new(
            "Export recovery key",
            Choice::BackupForm(FormKind::ExportKey, String::new()),
        ),
        Entry::new(
            "Recover an interrupted repository lock",
            Choice::BackupRequest(Request::Unlock),
        ),
    ];
    if backup.status.configured {
        rows.push(Entry::info(format!(
            "Private recovery key: {}",
            backup.status.recovery_key
        )));
    }
    rows
}

pub(super) fn backup_queue_rows(app: &App) -> Vec<Entry> {
    use crate::backup_service::Request;
    let backup = &app.deck.shell.backup;
    let mut rows = vec![
        Entry::new(
            "Queue saved project for offline backup",
            Choice::BackupQueueNow,
        ),
        Entry::new(
            "Run queued backups and retry interrupted jobs",
            Choice::BackupRequest(Request::RunQueue),
        ),
    ];
    if backup.status.queued_backups.is_empty() {
        rows.push(Entry::info("Nothing is waiting."));
    } else {
        rows.push(Entry::new(
            "Clear pending backup queue",
            Choice::BackupRequest(Request::ClearQueue),
        ));
        rows.push(Entry::info(format!(
            "Queued offline backups: {} pending",
            backup.status.queued_backups.len()
        )));
        for item in &backup.status.queued_backups {
            rows.push(Entry::info(format!(
                "Queued: {} · attempts {}{}",
                item.label,
                item.attempts,
                if item.last_error.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", item.last_error)
                }
            )));
        }
    }
    rows
}

pub(super) fn backup_form_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .backup
        .form
        .as_ref()
        .map(|form| form.rows().into_iter().map(Entry::info).collect())
        .unwrap_or_default()
}

impl App {
    pub(crate) fn paste_password_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.password_form else {
            return;
        };
        if let Some(field) = form.field()
            && field.len() + text.len() < 512
            && !text.chars().any(char::is_control)
        {
            field.push_str(text);
        }
    }

    pub(super) fn on_password_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.password_form else {
            return;
        };
        let count = PasswordForm::ACTIONS;
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.password_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % count,
            Key::Up => form.selected = (form.selected + count - 1) % count,
            Key::Backspace => {
                if let Some(field) = form.field() {
                    field.pop();
                }
            }
            Key::Delete => {
                if let Some(field) = form.field() {
                    *field = Zeroizing::new(String::new());
                }
            }
            Key::Char(c) if !c.is_control() && form.selected < 3 => {
                self.paste_password_form(&c.to_string());
            }
            Key::Enter if form.selected < 3 => form.selected += 1,
            Key::Enter if form.selected == 3 => {
                self.deck.shell.password_form = None;
                self.deck.shell.nav.back();
            }
            Key::Enter => {
                if self.deck.busy {
                    return;
                }
                let problem = if form.current.is_empty() || form.new.is_empty() {
                    Some("Type your current password and a new one.".to_string())
                } else if form.new.as_str() != form.repeat.as_str() {
                    Some("The new password and its repeat differ.".to_string())
                } else if form.new.as_str() == form.current.as_str() {
                    Some("The new password is the same as the current one.".to_string())
                } else {
                    wros_sys::system_views::validate_login_password(&form.new)
                        .and_then(|()| {
                            wros_sys::system_views::validate_login_password(&form.current)
                        })
                        .err()
                        .map(|e| e.to_string())
                };
                if let Some(problem) = problem {
                    form.repeat = Zeroizing::new(String::new());
                    form.selected = if form.current.is_empty() { 0 } else { 1 };
                    self.deck.note = problem;
                    self.deck.shell.notices.push(&self.deck.note);
                    return;
                }
                let current = wros_sys::WifiSecret::new(form.current.as_str());
                let new = wros_sys::WifiSecret::new(form.new.as_str());
                form.clear();
                self.deck.request(wros_sys::Request::ChangePassword {
                    current: Some(current),
                    new: Some(new),
                });
            }
            _ => {}
        }
    }
}
