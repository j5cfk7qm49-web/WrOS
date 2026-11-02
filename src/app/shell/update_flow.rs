//! Offline update flow in Settings: find media, review, confirm, stage, then
//! restart or cancel; plus recovery status and repair.
//!
//! Every system change here sits behind its own default-Cancel screen. Replies
//! are never assumed: after cancel, repair or staging the recovery status is
//! read again and the screens show what the system reported.
use super::*;
use std::time::SystemTime;
use writingdeck_sys::{PowerAction, Request, UpdatePackage, UpdateReview, UpdateState};

/// Screens that belong to the update flow. Leaving the flow pops back past all
/// of them to wherever the writer started (Updates panel or Recovery view).
const FLOW_ROUTES: [Route; 5] = [
    Route::UpdateBundleForm,
    Route::UpdateReview,
    Route::UpdateConfirm,
    Route::UpdateStaged,
    Route::RepairConfirm,
];

/// State of one pass through find, review, confirm and stage.
#[derive(Default)]
pub struct UpdateFlow {
    /// The authenticated review the writer is looking at.
    pub review: Option<writingdeck_sys::UpdateReview>,
    /// Folder the review came from; staging re-verifies this same folder.
    pub dir: Option<String>,
    /// Review identity being re-verified after a refused or failed stage.
    pub recheck: Option<String>,
    /// The error the failed stage reported, shown if the update did not change.
    pub stage_error: Option<String>,
    /// Why the writer was returned to the review screen.
    pub changed: Option<String>,
    /// The helper's acknowledgement from this session's stage.
    pub staged: Option<writingdeck_sys::UpdateStaged>,
    /// Text of the last helper result or failure, kept visible after readback.
    pub last_result: Option<String>,
    /// What the WrOS update server reported last, when, and which version was announced.
    pub online: Option<writingdeck_sys::OnlineUpdate>,
    /// When to ask the update server next. Wall-clock time, so a suspended laptop
    /// still checks once a day of real time.
    pub online_next: Option<SystemTime>,
    pub online_alerted: Option<String>,
}

/// After login, give Wi-Fi time to connect before the first check.
const ONLINE_FIRST_CHECK: Duration = Duration::from_secs(90);
/// How often a running installed session asks the update server again.
const ONLINE_CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Offline, a wrong clock or a lapsed server signature: try again sooner.
const ONLINE_RETRY: Duration = Duration::from_secs(60 * 60);

/// The Updates panel's description of the last online check.
fn online_line(online: Option<&writingdeck_sys::OnlineUpdate>) -> String {
    let Some(online) = online else {
        return "Not checked for WrOS updates yet".into();
    };
    match (online.status.as_str(), &online.version, &online.installed) {
        ("available", Some(version), _) => format!("WrOS {version} is available"),
        ("current", _, Some(installed)) => format!("WrOS is up to date ({installed})"),
        ("current", _, None) => "WrOS is up to date".into(),
        ("clock", _, _) => format!(
            "Couldn't check for WrOS updates: this computer's clock may be wrong (it reads {}).",
            online.clock
        ),
        ("unreachable", _, _) => "Couldn't reach the WrOS update server".into(),
        ("unpublished", _, _) => "No WrOS updates have been published yet".into(),
        ("expired", _, _) => {
            "The WrOS update server's signature has expired; WrOS will check again later".into()
        }
        _ => "The WrOS update server's files didn't verify; nothing was downloaded".into(),
    }
}

/// Backup state relative to a review that recommends one.
enum BackupGate {
    NotNeeded,
    Unconfigured,
    Fresh(String),
    Missing,
    Unknown,
}

fn info(text: impl Into<String>) -> Entry {
    Entry::info(text)
}

fn package_line(p: &UpdatePackage) -> String {
    match (p.action.as_str(), &p.from_version) {
        ("install", _) => format!("Install {}: not installed → {}", p.name, p.to_version),
        ("reinstall", Some(from)) => {
            format!("Reinstall {}: {} → {}", p.name, from, p.to_version)
        }
        (_, Some(from)) => format!("Upgrade {}: {} → {}", p.name, from, p.to_version),
        (_, None) => format!("Change {}: → {}", p.name, p.to_version),
    }
}

fn update_state(app: &App) -> Option<UpdateState> {
    app.deck.shell.recovery.as_ref().map(|r| r.update_state)
}

fn confirmed_staged(app: &App) -> bool {
    update_state(app) == Some(UpdateState::Staged)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Rows of System › Updates after the shared refresh row.
pub(super) fn updates_panel(app: &App) -> Vec<Entry> {
    let online = app.deck.shell.update_flow.online.as_ref();
    let mut rows = vec![info(online_line(online))];
    if let Some(writingdeck_sys::OnlineUpdate {
        status,
        version: Some(version),
        ..
    }) = online
        && status == "available"
    {
        rows.push(Entry::new(
            format!("Update WrOS to {version}"),
            Choice::UpdateOnlineFetch,
        ));
    }
    rows.push(Entry::new(
        "Check for WrOS updates",
        Choice::UpdateOnlineCheck,
    ));
    rows.extend([
        Entry::new(
            "Preview cached package plan",
            Choice::View(Route::UpdatesView, Request::CheckUpdates),
        ),
        Entry::new(
            "Find update media · disc or drive labelled WROS-UPDATE",
            Choice::UpdateFindMedia,
        ),
        Entry::new(
            "Review a signed update bundle by path",
            Choice::UpdateBundleOpen,
        ),
    ]);
    if confirmed_staged(app) {
        rows.push(Entry::new(
            "An update is staged · restart to install it, or cancel it",
            Choice::UpdateStagedOpen,
        ));
    }
    rows.push(Entry::new(
        "Recovery status, repair and staged update",
        Choice::View(Route::RecoveryView, Request::RecoveryStatus),
    ));
    if let Some(state) = update_state(app) {
        rows.push(info(format!("Update state: {}", state.label())));
    }
    if let Some(result) = &app.deck.shell.update_flow.last_result {
        rows.push(info(format!("Last update result: {result}")));
    }
    rows.push(info(
        "WrOS updates come signed from the WrOS update server or on update media.",
    ));
    rows.push(info(
        "You review who signed it and every package change before anything is staged.",
    ));
    rows
}

/// Rows of System › Recovery after the shared refresh row.
pub(super) fn recovery_panel() -> Vec<Entry> {
    vec![
        Entry::new(
            "Recovery status, repair and staged update",
            Choice::View(Route::RecoveryView, Request::RecoveryStatus),
        ),
        info("ext4 has no snapshot rollback. Keep an encrypted backup before updating."),
    ]
}

pub(super) fn review_rows(app: &App) -> Vec<Entry> {
    let flow = &app.deck.shell.update_flow;
    let Some(review) = &flow.review else {
        return vec![
            Entry::new("Back · nothing is under review", Choice::UpdateAbandon),
            info("No update is loaded. Go back and find media or enter a path."),
        ];
    };
    let mut rows = vec![Entry::new(
        "Back · do not install anything",
        Choice::UpdateAbandon,
    )];
    if review.packages.is_empty() && review.removals.is_empty() {
        rows.push(info(
            "This bundle changes no packages, so there is nothing to stage.",
        ));
    } else {
        rows.push(Entry::new(
            "Continue to confirmation · nothing is installed yet",
            Choice::UpdateContinue,
        ));
    }
    if let Some(changed) = &flow.changed {
        rows.push(info(format!("Changed since your last review: {changed}")));
    }
    rows.push(info(format!("Signed by key {}", review.signer)));
    rows.push(info(format!(
        "Release {} · dated {}",
        review.suite, review.date
    )));
    rows.push(info(format!("Valid until {}", review.valid_until)));
    rows.push(info(format!(
        "{} · {}",
        plural(review.packages.len(), "package change", "package changes"),
        plural(review.removals.len(), "removal", "removals")
    )));
    rows.extend(review.packages.iter().map(|p| info(package_line(p))));
    rows.extend(
        review
            .removals
            .iter()
            .map(|name| info(format!("Remove {name}"))),
    );
    if review.backup_recommended {
        rows.push(info(
            "A fresh backup is recommended first; ext4 has no snapshot rollback.",
        ));
    }
    rows
}

fn backup_gate(app: &App, review: &UpdateReview) -> BackupGate {
    let backup = &app.deck.shell.backup;
    if !review.backup_recommended {
        BackupGate::NotNeeded
    } else if !backup.status_ok {
        BackupGate::Unknown
    } else if !backup.status.configured {
        BackupGate::Unconfigured
    } else if let Some(taken) = backup.fresh_snapshot(&app.project.root, chrono::Utc::now()) {
        BackupGate::Fresh(taken)
    } else {
        BackupGate::Missing
    }
}

fn gate_lines(app: &App, review: &UpdateReview) -> Vec<String> {
    let mut lines = vec![if app.project.dirty_count() == 0
        && !app.tree_stale
        && app.conflicts().is_empty()
    {
        "Writing: every buffer is saved and there are no source conflicts.".to_string()
    } else {
        "Writing: unsaved changes or source conflicts must be resolved before staging.".to_string()
    }];
    lines.push(match backup_gate(app, review) {
        BackupGate::NotNeeded => "Backup: this update does not ask for one.".to_string(),
        BackupGate::Unconfigured => {
            "Backup: none is set up. Without one, a failed update cannot be undone; ext4 has no rollback."
                .to_string()
        }
        BackupGate::Fresh(taken) => format!("Backup: fresh, taken {taken}."),
        BackupGate::Missing => format!(
            "Backup: none in the last {} hours. Create one in Settings › Accounts › Backups first.",
            super::super::backup::FRESH_BACKUP_HOURS
        ),
        BackupGate::Unknown => {
            "Backup: status not read yet. Choose Stage again in a moment.".to_string()
        }
    });
    if app.deck.shell.backup.job.is_some() {
        lines.push("A backup operation is running; staging waits until it ends.".to_string());
    }
    lines
}

pub(super) fn confirm_rows(app: &App) -> Vec<Entry> {
    let flow = &app.deck.shell.update_flow;
    let Some(review) = &flow.review else {
        return vec![
            Entry::new("Cancel · nothing is under review", Choice::UpdateAbandon),
            info("No update is loaded. Go back and review a bundle."),
        ];
    };
    let mut rows = vec![
        Entry::new("Cancel · do not install this update", Choice::UpdateAbandon),
        Entry::new(
            "Stage this update · it installs at your next restart",
            Choice::UpdateStage,
        ),
        info(format!(
            "{} signed by key {}, valid until {}.",
            plural(review.packages.len(), "package change", "package changes"),
            review.signer,
            review.valid_until
        )),
        info(
            "Staging installs nothing yet. You choose when to restart, and can cancel until then.",
        ),
    ];
    rows.extend(gate_lines(app, review).into_iter().map(info));
    rows
}

pub(super) fn staged_rows(app: &App) -> Vec<Entry> {
    let flow = &app.deck.shell.update_flow;
    let mut rows = vec![
        Entry::new("Not now · leave the update staged", Choice::UpdateAbandon),
        Entry::new(
            "Restart now to install the staged update",
            Choice::UpdateRestart,
        ),
        Entry::new("Cancel staged update", Choice::UpdateCancelStaged),
    ];
    match &app.deck.shell.recovery {
        Some(recovery) => {
            rows.push(info(format!(
                "System reports: update {}",
                recovery.update_state.label()
            )));
            if !recovery.staged_packages.is_empty() {
                rows.push(info(format!(
                    "Staged packages: {}",
                    recovery.staged_packages.join(", ")
                )));
            }
            if recovery.update_state != UpdateState::Staged {
                rows.push(info(
                    "The system does not report a staged update, so restarting would not install one.",
                ));
            }
        }
        None => rows.push(info(
            "Reading the staged state from the system; restart is offered once it is confirmed.",
        )),
    }
    if let Some(staged) = &flow.staged {
        rows.push(info(format!(
            "Helper acknowledged {} (transaction {}).",
            plural(staged.packages, "package", "packages"),
            staged.transaction_id
        )));
    }
    if let Some(result) = &flow.last_result {
        rows.push(info(format!("Last result: {result}")));
    }
    rows.push(info(
        "Restarting applies the update before you can write again. Do not power off while it runs.",
    ));
    rows.push(info(
        "Cancel works only until you restart; once applying begins there is no cancel.",
    ));
    rows
}

pub(super) fn repair_rows() -> Vec<Entry> {
    vec![
        Entry::new("Cancel · do not repair", Choice::UpdateAbandon),
        Entry::new(
            "Repair now from retained verified packages",
            Choice::RepairRun,
        ),
        info(
            "Repair finishes interrupted package steps using only archives WrOS already verified and kept.",
        ),
        info("No network is used and your writing is not touched."),
        info("If repair fails, start WrOS rescue from the install media."),
    ]
}

pub(super) fn recovery_rows(app: &App) -> Vec<Entry> {
    let flow = &app.deck.shell.update_flow;
    let mut rows = vec![Entry::new(
        "Refresh recovery status",
        Choice::Read(Request::RecoveryStatus),
    )];
    if let Some(result) = &flow.last_result {
        rows.push(info(format!("Last result: {result}")));
    }
    let Some(recovery) = &app.deck.shell.recovery else {
        rows.push(info(
            "Recovery status was not read. Refresh to try again; nothing has been assumed.",
        ));
        rows.push(Entry::new(
            "Encrypted project backups and restore",
            Choice::Open(Route::Backups),
        ));
        return rows;
    };
    rows.push(info(format!(
        "Update state: {}",
        recovery.update_state.label()
    )));
    match recovery.update_state {
        UpdateState::Staged => {
            rows.push(Entry::new(
                "Manage staged update · restart to install it",
                Choice::UpdateStagedOpen,
            ));
            rows.push(Entry::new(
                "Cancel staged update",
                Choice::UpdateCancelStaged,
            ));
        }
        UpdateState::Applying => rows.push(info(
            "An update is being applied. It cannot be cancelled; wait for it to finish, then refresh.",
        )),
        _ => {
            let needed = recovery.interrupted_dpkg
                || !recovery.unconfigured_packages.is_empty()
                || matches!(
                    recovery.update_state,
                    UpdateState::Failed | UpdateState::Interrupted
                );
            rows.push(Entry::new(
                if needed {
                    "Repair interrupted update or packages…"
                } else {
                    "Run repair check · nothing looks broken…"
                },
                Choice::RepairOpen,
            ));
        }
    }
    if recovery.staged_update && recovery.update_state != UpdateState::Staged {
        rows.push(info(
            "A system-update marker exists that WrOS did not stage; WrOS leaves it alone.",
        ));
    }
    rows.push(info(&recovery.status));
    rows.push(info(format!(
        "Root filesystem: {} · Device: {} · Separate /home: {}",
        recovery.root_filesystem,
        recovery.root_device,
        if recovery.separate_home { "yes" } else { "no" }
    )));
    rows.push(info(format!(
        "Automatic filesystem snapshot rollback: {}",
        if recovery.rollback_capable {
            "available"
        } else {
            "unavailable"
        }
    )));
    rows.push(info(&recovery.rollback_strategy));
    if recovery.interrupted_dpkg {
        rows.push(info(if recovery.unconfigured_packages.is_empty() {
            "Interrupted dpkg work: queued package triggers".to_string()
        } else {
            format!(
                "Interrupted dpkg packages: {}",
                recovery.unconfigured_packages.join(", ")
            )
        }));
    }
    if !recovery.staged_packages.is_empty() {
        rows.push(info(format!(
            "Staged offline update: {}",
            recovery.staged_packages.join(", ")
        )));
    }
    rows.push(Entry::new(
        "Encrypted project backups and restore",
        Choice::Open(Route::Backups),
    ));
    rows
}

impl App {
    fn in_settings(&self) -> bool {
        matches!(self.overlay, Overlay::Deck(writingdeck::Page::Settings))
    }
    fn in_update_flow(&self) -> bool {
        self.in_settings() && FLOW_ROUTES.contains(&self.deck.shell.nav.current())
    }
    fn update_notice(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.deck.note = text.clone();
        self.deck.shell.notices.push(&text);
    }
    /// The same save and conflict gate the installer and power paths use.
    fn writing_is_safe(&mut self) -> bool {
        self.commit_saves()
            && self.project.dirty_count() == 0
            && !self.tree_stale
            && self.conflicts().is_empty()
    }
    /// Forget the cached recovery status and read it again from the system.
    pub(crate) fn refresh_recovery(&mut self) {
        self.deck.shell.recovery = None;
        if !self.deck.busy {
            self.deck.request(Request::RecoveryStatus);
        }
    }
    /// Pop out of every update screen and drop the review that was open.
    fn leave_update_flow(&mut self) {
        for _ in 0..FLOW_ROUTES.len() {
            if !FLOW_ROUTES.contains(&self.deck.shell.nav.current()) || !self.deck.shell.nav.back()
            {
                break;
            }
        }
        self.deck.shell.update_form = None;
        let flow = &mut self.deck.shell.update_flow;
        flow.review = None;
        flow.dir = None;
        flow.recheck = None;
        flow.stage_error = None;
        flow.changed = None;
    }
    pub(super) fn update_abandon(&mut self) {
        self.leave_update_flow();
    }
    /// After login and then daily, in an installed appliance session only.
    pub(crate) fn tick_online_updates(&mut self) {
        if !self.deck.online_updates {
            return;
        }
        let now = SystemTime::now();
        let Some(next) = self.deck.shell.update_flow.online_next else {
            self.deck.shell.update_flow.online_next = Some(now + ONLINE_FIRST_CHECK);
            return;
        };
        // A pending keyboard or display preview owns the system worker until it is confirmed.
        let previewing = self.deck.preview.is_some() || self.deck.shell.warm_preview.is_some();
        if now >= next && !self.deck.busy && !previewing {
            self.deck.shell.update_flow.online_next = Some(now + ONLINE_CHECK_EVERY);
            self.deck.request(Request::CheckOnlineUpdates);
        }
    }
    pub(super) fn update_online_check(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        self.deck.shell.update_flow.online_next = Some(SystemTime::now() + ONLINE_CHECK_EVERY);
        self.deck.request(Request::CheckOnlineUpdates);
    }
    pub(super) fn update_online_fetch(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        self.deck.shell.update_flow.last_result = None;
        self.update_notice("Downloading the WrOS update and checking its signature…");
        self.deck.request(Request::FetchOnlineUpdate);
    }
    pub(crate) fn on_online_update(&mut self, online: writingdeck_sys::OnlineUpdate) {
        let flow = &mut self.deck.shell.update_flow;
        if online.status == "available"
            && let Some(version) = &online.version
            && flow.online_alerted.as_ref() != Some(version)
        {
            flow.online_alerted = Some(version.clone());
            let alert = format!("WrOS {version} is available · Settings › Updates");
            self.deck.shell.notices.push(&alert);
            self.msg = alert;
        }
        if matches!(
            online.status.as_str(),
            "unreachable" | "unpublished" | "clock" | "expired"
        ) {
            self.deck.shell.update_flow.online_next = Some(SystemTime::now() + ONLINE_RETRY);
        }
        self.deck.note = online_line(Some(&online));
        self.deck.shell.update_flow.online = Some(online);
    }
    pub(crate) fn on_online_update_fetched(&mut self, path: String) {
        // Downloaded and verified; the ordinary review shows who signed it and what changes.
        self.deck
            .request(Request::ReviewUpdateBundle { bundle_dir: path });
    }
    pub(super) fn update_find_media(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        self.deck.shell.update_flow.last_result = None;
        self.deck.request(Request::FindUpdateMedia);
    }
    pub(super) fn update_continue(&mut self) {
        let Some(review) = self.deck.shell.update_flow.review.clone() else {
            return;
        };
        if review.packages.is_empty() && review.removals.is_empty() {
            return;
        }
        // Refresh the backup status so the confirmation shows what exists now.
        self.deck.shell.backup.status_ok = false;
        self.deck
            .shell
            .backup
            .request(crate::backup_service::Request::Status);
        self.deck.shell.nav.open(Route::UpdateConfirm);
        self.deck.shell.nav.select(0, 4);
    }
    pub(super) fn update_stage(&mut self) {
        let flow = &self.deck.shell.update_flow;
        let (Some(review), Some(dir)) = (flow.review.clone(), flow.dir.clone()) else {
            self.update_notice("No reviewed update to stage. Go back and review a bundle.");
            return;
        };
        if self.deck.busy {
            self.update_notice(
                "A system request is still running. Wait for it to finish, then choose Stage again.",
            );
            return;
        }
        self.deck.shell.backup.tick();
        if self.deck.shell.backup.job.is_some() {
            self.update_notice(
                "A backup operation is running. Wait for it to finish, then choose Stage again.",
            );
            return;
        }
        if !self.writing_is_safe() {
            self.update_notice(
                "Not staged: unsaved buffers or source conflicts need attention. Return to writing, save and resolve them, then try again.",
            );
            return;
        }
        match backup_gate(self, &review) {
            BackupGate::NotNeeded | BackupGate::Unconfigured | BackupGate::Fresh(_) => {}
            BackupGate::Missing => {
                self.update_notice(format!(
                    "Not staged: no backup of this project in the last {} hours. Create one in Settings › Accounts › Backups, then come back.",
                    super::super::backup::FRESH_BACKUP_HOURS
                ));
                return;
            }
            BackupGate::Unknown => {
                self.deck.shell.backup.status_ok = false;
                self.deck
                    .shell
                    .backup
                    .request(crate::backup_service::Request::Status);
                self.update_notice(
                    "Not staged yet: the backup status was not available. Checking now; choose Stage again in a moment.",
                );
                return;
            }
        }
        self.save_deck();
        self.deck.shell.update_flow.last_result = None;
        self.deck.shell.update_flow.changed = None;
        self.deck.request(Request::StageUpdates {
            bundle_dir: dir,
            review_id: review.review_id,
        });
    }
    pub(super) fn update_open_staged(&mut self) {
        self.deck.shell.nav.open(Route::UpdateStaged);
        self.deck.shell.nav.select(0, 3);
    }
    pub(super) fn update_restart(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        if !confirmed_staged(self) {
            self.update_notice(
                "Not restarting: the system has not confirmed a staged update. Refresh Recovery status first.",
            );
            return;
        }
        self.deck.shell.backup.tick();
        if self.deck.shell.backup.job.is_some() {
            self.update_notice("Not restarting: a backup operation is still running.");
            return;
        }
        if !self.writing_is_safe() {
            self.update_notice(
                "Not restarting: unsaved buffers or source conflicts need attention. Return to writing to review them.",
            );
            return;
        }
        self.save_deck();
        self.deck.shell.update_flow.last_result = None;
        self.deck.request(Request::Power(PowerAction::Restart));
    }
    pub(super) fn update_cancel_staged(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        self.deck.shell.update_flow.last_result = None;
        self.deck.request(Request::CancelStagedUpdates);
    }
    pub(super) fn repair_open(&mut self) {
        self.deck.shell.nav.open(Route::RepairConfirm);
        self.deck.shell.nav.select(0, 2);
    }
    pub(super) fn repair_run(&mut self) {
        if self.deck.busy {
            self.update_notice("A system request is still running. Wait for it, then try again.");
            return;
        }
        self.deck.shell.update_flow.last_result = None;
        self.deck.shell.nav.back();
        self.deck.request(Request::RecoverUpdates);
    }

    // ---- replies, called from the system-worker result loop -------------------

    pub(crate) fn on_update_media(&mut self, media: Option<String>) {
        if self.on_google_file_media(Ok(media.clone())) {
            return;
        }
        match media {
            None => {
                let text = "No update media found. Insert the disc or drive labelled WROS-UPDATE, then try again, or enter a folder path.";
                self.deck.shell.update_flow.last_result = Some(text.into());
                self.update_notice(text);
            }
            Some(path) => {
                // Mounting is only half the job; the media is not trusted until
                // it has been reviewed.
                if self.in_settings()
                    && self.deck.shell.nav.current() == Route::Panel(Panel::Updates)
                {
                    self.deck
                        .request(Request::ReviewUpdateBundle { bundle_dir: path });
                } else {
                    self.update_notice(format!(
                        "Update media mounted read-only at {path}. Open Settings › System › Updates to review it."
                    ));
                }
            }
        }
    }
    pub(crate) fn on_update_review(&mut self, request: &Request, review: UpdateReview) {
        let Request::ReviewUpdateBundle { bundle_dir } = request else {
            return;
        };
        let flow = &mut self.deck.shell.update_flow;
        if let Some(previous) = flow.recheck.take() {
            // A stage was refused. Say whether the signed content really changed.
            let stage_error = flow.stage_error.take().unwrap_or_default();
            let unchanged = previous == review.review_id;
            if unchanged {
                let text = format!(
                    "Update not staged: {stage_error}. The update itself is unchanged and nothing was installed; you can try again."
                );
                self.deck.shell.update_flow.last_result = Some(text.clone());
                self.update_notice(text);
            } else {
                let text = "the signed content on the update media is different from what you reviewed (a different set of package changes). Nothing was staged. Read the new list before continuing."
                    .to_string();
                flow.review = Some(review);
                flow.dir = Some(bundle_dir.clone());
                flow.changed = Some(text.clone());
                flow.last_result = None;
                self.update_notice(format!("Update changed: {text}"));
                // Return to the review screen, dropping the stale confirmation.
                if self.in_settings() {
                    while self.deck.shell.nav.current() != Route::UpdateReview
                        && FLOW_ROUTES.contains(&self.deck.shell.nav.current())
                        && self.deck.shell.nav.back()
                    {}
                    if self.deck.shell.nav.current() != Route::UpdateReview {
                        self.deck.shell.nav.open(Route::UpdateReview);
                    }
                    self.deck.shell.nav.select(0, 2);
                }
            }
            self.refresh_recovery();
            return;
        }
        let text = format!(
            "Reviewed: signed by key {}, {}. Nothing is installed yet.",
            review.signer,
            plural(review.packages.len(), "package change", "package changes")
        );
        flow.review = Some(review);
        flow.dir = Some(bundle_dir.clone());
        flow.changed = None;
        let route = self.deck.shell.nav.current();
        if self.in_settings()
            && matches!(
                route,
                Route::UpdateBundleForm | Route::Panel(Panel::Updates)
            )
        {
            if route == Route::UpdateBundleForm {
                self.deck.shell.update_form = None;
                self.deck.shell.nav.back();
            }
            self.deck.shell.nav.open(Route::UpdateReview);
            self.deck.shell.nav.select(0, 2);
        }
        self.update_notice(text);
    }
    pub(crate) fn on_update_staged(&mut self, staged: writingdeck_sys::UpdateStaged) {
        let text = if staged.restart_required {
            format!(
                "Staged {} (transaction {}). Restart to install it; you can cancel until then.",
                plural(staged.packages, "package", "packages"),
                staged.transaction_id
            )
        } else {
            format!(
                "Staged {} (transaction {}). The helper says no restart is required; check Recovery status.",
                plural(staged.packages, "package", "packages"),
                staged.transaction_id
            )
        };
        let showing = self.in_update_flow();
        let flow = &mut self.deck.shell.update_flow;
        flow.staged = Some(staged);
        flow.last_result = Some(text.clone());
        self.update_notice(text);
        // Read back what the system now reports before offering a restart.
        self.refresh_recovery();
        if showing {
            self.leave_update_flow();
            self.update_open_staged();
        } else {
            self.deck.shell.update_flow.review = None;
            self.deck.shell.update_flow.dir = None;
        }
    }
    /// Cancel and repair succeeded: show the helper's words, then re-read.
    pub(crate) fn on_update_message(&mut self, request: &Request, message: &str) {
        match request {
            Request::CancelStagedUpdates => {
                self.deck.shell.update_flow.staged = None;
                self.deck.shell.update_flow.last_result = Some(message.to_string());
                self.refresh_recovery();
                if self.in_settings() && self.deck.shell.nav.current() == Route::UpdateStaged {
                    self.leave_update_flow();
                }
            }
            Request::RecoverUpdates => {
                self.deck.shell.update_flow.last_result = Some(message.to_string());
                self.refresh_recovery();
            }
            Request::Power(PowerAction::Restart) if confirmed_staged(self) => {
                self.deck.shell.update_flow.last_result = Some(format!(
                    "Restart requested: {message} The update installs while the computer restarts."
                ));
            }
            _ => {}
        }
    }
    /// Truthful, actionable text for a failed update request. Returns true when
    /// the request was an update request and has been handled.
    pub(crate) fn on_update_error(&mut self, request: &Request, error: &str) -> bool {
        match request {
            Request::CheckOnlineUpdates => {
                // Silent: the Updates panel says the server could not be reached.
                let online = writingdeck_sys::OnlineUpdate {
                    status: "unreachable".into(),
                    version: None,
                    installed: None,
                    clock: String::new(),
                };
                let flow = &mut self.deck.shell.update_flow;
                flow.online_next = Some(SystemTime::now() + ONLINE_RETRY);
                self.deck.note = online_line(Some(&online));
                self.deck.shell.update_flow.online = Some(online);
            }
            Request::FetchOnlineUpdate if error.contains("already up to date") => {
                // apt or another session installed it meanwhile: nothing to fetch, nothing wrong.
                let flow = &mut self.deck.shell.update_flow;
                if let Some(online) = &mut flow.online {
                    online.status = "current".into();
                    online.installed = online.version.take();
                }
                let text = online_line(flow.online.as_ref());
                flow.last_result = Some(text.clone());
                self.update_notice(text);
            }
            Request::FetchOnlineUpdate => {
                let text = format!(
                    "WrOS update not downloaded: {error}. Nothing was installed. Check the network, then try again."
                );
                self.deck.shell.update_flow.last_result = Some(text.clone());
                self.update_notice(text);
            }
            Request::FindUpdateMedia => {
                if !self.on_google_file_media(Err(error)) {
                    let text = format!(
                        "Could not look for update media: {error}. Nothing was mounted. You can still enter a folder path."
                    );
                    self.deck.shell.update_flow.last_result = Some(text.clone());
                    self.update_notice(text);
                }
            }
            Request::ReviewUpdateBundle { .. } => {
                let flow = &mut self.deck.shell.update_flow;
                if flow.recheck.take().is_some() {
                    let stage_error = flow.stage_error.take().unwrap_or_default();
                    let text = format!(
                        "Update not staged: {stage_error}. The bundle can no longer be verified ({error}). Check that the update media is still inserted."
                    );
                    flow.last_result = Some(text.clone());
                    self.leave_update_flow();
                    self.update_notice(text);
                    self.refresh_recovery();
                } else {
                    let text = format!(
                        "Update not reviewed: {error}. Nothing was changed. Check that the folder holds a complete WrOS update signed by the WrOS key, then try again."
                    );
                    flow.last_result = Some(text.clone());
                    self.update_notice(text);
                }
            }
            Request::StageUpdates {
                bundle_dir,
                review_id,
            } => {
                let flow = &mut self.deck.shell.update_flow;
                flow.stage_error = Some(error.to_string());
                flow.recheck = Some(review_id.clone());
                self.update_notice(format!(
                    "Update not staged: {error}. Nothing was installed. Checking whether the update changed…"
                ));
                self.deck.request(Request::ReviewUpdateBundle {
                    bundle_dir: bundle_dir.clone(),
                });
                if !self.deck.busy {
                    // The check could not even be sent; do not leave it armed.
                    let flow = &mut self.deck.shell.update_flow;
                    flow.recheck = None;
                    let text = format!(
                        "Update not staged: {error}. Nothing was installed. Go back and review the update again."
                    );
                    flow.stage_error = None;
                    flow.last_result = Some(text.clone());
                    self.update_notice(text);
                }
            }
            Request::CancelStagedUpdates => {
                let text = format!(
                    "The staged update was not cancelled: {error}. Refresh Recovery status to see whether it is still staged."
                );
                self.deck.shell.update_flow.last_result = Some(text.clone());
                self.refresh_recovery();
                self.update_notice(text);
            }
            Request::RecoverUpdates => {
                let text = format!(
                    "Repair did not finish: {error}. Your writing is untouched. Refresh status and try again, or start WrOS rescue from the install media."
                );
                self.deck.shell.update_flow.last_result = Some(text.clone());
                self.refresh_recovery();
                self.update_notice(text);
            }
            _ => return false,
        }
        true
    }
}
