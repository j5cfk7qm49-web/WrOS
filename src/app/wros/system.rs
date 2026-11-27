use super::{Deck, Page};
use crate::app::{App, Overlay, backup, shell};
use grimoire_core::tutorial;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};
use wros_sys::{Backend, PowerAction, Reply, Request, Snapshot};

pub(super) struct Worker {
    tx: SyncSender<Request>,
    rx: Receiver<(Request, Result<Reply, String>)>,
    simulated: bool,
}
impl Worker {
    pub(super) fn system() -> Self {
        #[cfg(all(target_os = "linux", feature = "compositor-warmth"))]
        if crate::session::composited() {
            return Self::spawn(crate::compositor_warmth::backend());
        }
        Self::spawn(Box::new(wros_sys::linux::Linux::default()))
    }
    fn spawn(mut backend: Box<dyn Backend>) -> Self {
        let simulated = backend.simulated();
        let (tx, requests) = mpsc::sync_channel::<Request>(1);
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let result = backend
                    .request(request.clone())
                    .map_err(|e| format!("{e:#}"));
                if results.send((request, result)).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx, simulated }
    }
}

impl Deck {
    pub(in crate::app) fn simulated(&self) -> bool {
        self.worker.as_ref().is_some_and(|worker| worker.simulated)
            || self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.simulated)
    }
    /// While a system request runs: what it is doing, and for how long once it takes a
    /// while (#167). Shown in place of the last notice until the reply arrives.
    pub(in crate::app) fn busy_line(&self) -> String {
        let Some((what, since)) = &self.running else {
            return "Waiting for the system…".into();
        };
        let what = match &self.fetch_progress {
            Some(progress) => progress.line(),
            None => what.clone(),
        };
        match wros_sys::elapsed_label(since.elapsed()) {
            Some(time) => format!("{what} · {time}"),
            None => what,
        }
    }
    /// While an online update downloads, read the engine's progress about once a second.
    fn tick_fetch_progress(&mut self) {
        let fetching = self.busy
            && self
                .running
                .as_ref()
                .is_some_and(|(what, _)| *what == Request::FetchOnlineUpdate.activity());
        if !fetching
            || self
                .fetch_read
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.fetch_read = Some(std::time::Instant::now());
        self.fetch_progress = wros_sys::FetchProgress::read();
    }

    #[expect(
        clippy::unwrap_used,
        reason = "the system worker is initialized before dispatch"
    )]
    pub(crate) fn request(&mut self, request: Request) {
        if self.busy {
            self.note = format!("Still busy, try again in a moment: {}", self.busy_line());
            return;
        }
        if self.worker.is_none() {
            if !self.enable_system {
                self.note = "System operations disabled in this test session.".into();
                return;
            }
            self.worker = Some(Worker::system());
        }
        let powering_off = matches!(
            request,
            Request::Power(PowerAction::Shutdown | PowerAction::Restart)
        );
        let activity = request.activity();
        match self.worker.as_ref().unwrap().tx.try_send(request) {
            Ok(()) => {
                self.power_pending |= powering_off;
                self.busy = true;
                self.running = Some((activity, std::time::Instant::now()));
                self.note = self.busy_line();
            }
            Err(e) => self.note = format!("System worker unavailable: {e}"),
        }
    }
    #[cfg(test)]
    pub(crate) fn simulate(&mut self) {
        self.worker = Some(Worker::spawn(Box::new(wros_sys::Simulated::laptop())));
    }
    #[cfg(test)]
    pub(crate) fn use_test_backend(&mut self, backend: Box<dyn Backend>) {
        self.worker = Some(Worker::spawn(backend));
    }

    pub(in crate::app) fn carry_import_worker(&mut self, previous: &mut Self) {
        std::mem::swap(&mut self.worker, &mut previous.worker);
        std::mem::swap(&mut self.busy, &mut previous.busy);
    }
}

impl App {
    pub(crate) fn open_system_settings(&mut self) {
        self.deck.system_parent = None;
        self.overlay = Overlay::Deck(Page::System { sel: 0 });
        self.observe_deck(tutorial::Event::SystemOpened);
        if self.deck.snapshot.is_none() {
            self.deck.request(Request::Refresh);
        }
    }
    pub(super) fn close_system_subpage(&mut self) {
        self.overlay = self
            .deck
            .system_parent
            .take()
            .map_or(Overlay::None, |sel| Overlay::Deck(Page::System { sel }));
    }

    /// New readings from the gauges' thread; a change of light (its key) shows
    /// the light for a moment.
    pub(crate) fn set_gauges(
        &mut self,
        mut gauges: crate::gauges::Gauges,
        now: std::time::Instant,
    ) {
        if self.deck.gauges.light.is_some() && gauges.light != self.deck.gauges.light {
            self.deck.light_flash = Some(now);
        }
        gauges.wifi_radio = gauges.wifi_radio.or(self.deck.gauges.wifi_radio);
        self.deck.gauges = gauges;
    }

    pub(crate) fn tick_deck(&mut self) {
        self.tick_source_catalogue();
        self.deck.shell.backup.tick();
        self.tick_music_setup();
        let now = chrono::Local::now();
        self.deck.clock = now.format("%H:%M").to_string();
        self.tick_timeline(Instant::now(), now);
        self.deck.backup_mark = self.read_backup_mark();
        match self.deck.shell.backup.finished.take() {
            Some(backup::Finished::Restore) => self.observe_deck(tutorial::Event::BackupRestored),
            Some(backup::Finished::LinkPhone) => self.observe_deck(tutorial::Event::PhoneLinked),
            Some(backup::Finished::DriveList)
                if self.overlay == Overlay::Deck(Page::Settings)
                    && self.deck.shell.nav.current() == shell::Route::GoogleDrive =>
            {
                self.observe_deck(tutorial::Event::DriveFolderListed)
            }
            Some(backup::Finished::DriveList) => {}
            None => {}
        }
        self.tick_update_chain();
        self.tick_power_wait();
        self.deck.tick_fetch_progress();
        self.tick_productivity();
        self.tick_online_updates();
        self.tick_update_health();
        if !self.deck.power_pending {
            self.tick_read();
            self.tick_sync();
            self.tick_commonplace_backup();
            self.tick_auto_backup();
        }
        let result = self.deck.worker.as_ref().and_then(|w| w.rx.try_recv().ok());
        if let Some((request, result)) = result {
            self.deck.busy = false;
            self.deck.running = None;
            self.deck.fetch_progress = None;
            self.deck.fetch_read = None;
            if matches!(request, Request::Power(_)) && (result.is_err() || self.deck.simulated()) {
                self.deck.power_pending = false;
            }
            match result {
                Ok(Reply::Snapshot(s)) => {
                    self.deck.note = if s.simulated {
                        "SIMULATED system status; not hardware verified"
                    } else {
                        "System status refreshed locally."
                    }
                    .into();
                    // Settings just changed or read these: the bar shows them now.
                    if s.battery_percent.is_some() {
                        self.deck.gauges.battery = s.battery_percent;
                        self.deck.gauges.charging = s.charging.unwrap_or(false);
                    }
                    if s.brightness_percent.is_some() {
                        self.deck.gauges.light = s.brightness_percent;
                    }
                    self.deck.snapshot = Some(s);
                    if self.deck.boot_settings_available
                        && self.deck.shell.nav.current()
                            == shell::Route::Panel(shell::Panel::Display)
                    {
                        self.deck.request(Request::BootStatus);
                    }
                }
                Ok(Reply::BootStatus(status)) => {
                    self.deck.note = status.preparation().into();
                    let saved = matches!(request, Request::BootScreen { enabled, .. } if enabled == status.enabled)
                        && !status.failed;
                    self.deck.boot_status = Some(status);
                    if saved {
                        self.observe_deck(tutorial::Event::StartScreenSaved);
                    }
                }
                Ok(Reply::Message(message)) => {
                    self.on_update_message(&request, &message);
                    if request != Request::ReportUpdateReady {
                        self.deck.note = message;
                    }
                    if let Request::Wifi(on) = request {
                        self.deck.gauges.wifi_radio = Some(on);
                        if !on {
                            self.deck.gauges.wifi = Some(crate::gauges::Wifi::Off);
                        }
                        self.replace_wifi_networks(None);
                        self.deck.widget_wifi_scan_pending = on;
                    }
                    if let Request::Brightness(percent) = request {
                        self.deck.gauges.light = Some(percent);
                        if let Some(snapshot) = &mut self.deck.snapshot {
                            snapshot.brightness_percent = Some(percent);
                        }
                    }
                    if matches!(request, Request::KeyboardConfirm(_)) {
                        self.observe_deck(tutorial::Event::LayoutConfirmed);
                    }
                    if matches!(request, Request::DisplayConfirm(_)) {
                        self.observe_deck(tutorial::Event::WarmthKept);
                    }
                    if matches!(
                        request,
                        Request::DisplayConfirm(_) | Request::DisplayCancel(_)
                    ) {
                        self.deck.request(Request::DisplayStatus);
                    }
                    if matches!(request, Request::ChangePassword { .. }) {
                        self.deck.shell.password_form = None;
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == shell::Route::ChangePassword
                        {
                            self.deck.shell.nav.back();
                        }
                    }
                    if let Request::ConnectWifi { ssid, .. } = &request {
                        self.deck.shell.wifi_form = None;
                        if let Some(networks) = &mut self.deck.shell.wifi_networks {
                            for network in networks {
                                network.in_use = &network.ssid == ssid;
                            }
                        }
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == shell::Route::WifiConnect
                        {
                            self.deck.shell.nav.back();
                        }
                    }
                    if matches!(request, Request::DisconnectWifi) {
                        if let Some(networks) = &mut self.deck.shell.wifi_networks {
                            for network in networks {
                                network.in_use = false;
                            }
                        }
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == shell::Route::WifiConnect
                        {
                            self.deck.shell.wifi_form = None;
                            self.deck.shell.nav.back();
                        }
                    }
                    if let Request::ForgetWifi(uuid) = &request
                        && let Some(saved) = &mut self.deck.shell.wifi_saved
                    {
                        saved.retain(|s| &s.uuid != uuid);
                    }
                    if let Request::ActivateSavedWifi(uuid) = &request
                        && let Some(profile) = self
                            .deck
                            .shell
                            .wifi_saved
                            .as_ref()
                            .and_then(|s| s.iter().find(|p| &p.uuid == uuid))
                            .cloned()
                        && let Some(networks) = &mut self.deck.shell.wifi_networks
                    {
                        for network in networks {
                            network.in_use = network.ssid == profile.name;
                        }
                    }
                }
                Ok(Reply::UpdateReview(review)) => {
                    self.on_update_review(&request, review);
                }
                Ok(Reply::UpdateStaged(staged)) => {
                    self.on_update_staged(staged);
                }
                Ok(Reply::UpdateApplied(applied)) => {
                    self.on_update_applied(applied);
                }
                Ok(Reply::ImportMedia(sticks)) => {
                    self.on_import_media(&request, sticks);
                }
                Ok(Reply::UpdateMedia(media)) => {
                    self.on_update_media(media);
                }
                Ok(Reply::OnlineUpdate(online)) => {
                    self.on_online_update(online);
                }
                Ok(Reply::OnlineUpdateFetched(path)) => {
                    self.on_online_update_fetched(path);
                }
                Ok(Reply::WifiScan(networks)) => {
                    self.deck.note = format!(
                        "{} named Wi-Fi networks returned. Select a network to connect.",
                        networks.len()
                    );
                    self.replace_wifi_networks(Some(networks));
                    self.observe_deck(tutorial::Event::WifiScanned);
                }
                Ok(Reply::SavedWifi(profiles)) => {
                    self.deck.note = format!(
                        "{} saved Wi-Fi profiles visible to this session.",
                        profiles.len()
                    );
                    self.deck.shell.wifi_saved = Some(profiles);
                }
                Ok(Reply::LocalAccounts(accounts)) => {
                    self.deck.note = "Local account list loaded; no account was changed.".into();
                    self.deck.shell.local_accounts = Some(accounts);
                }
                Ok(Reply::StorageDevices(devices)) => {
                    self.deck.note = "Mounted filesystem usage loaded locally.".into();
                    self.deck.shell.storage_devices = Some(devices);
                }
                Ok(Reply::Updates(plan)) => {
                    self.deck.note = plan.status.clone();
                    self.deck.shell.updates = Some(plan);
                }
                Ok(Reply::Recovery(info)) => {
                    // A recent helper result stays on screen; the status read
                    // back after it is shown in the Recovery and Updates views.
                    self.deck.note = self
                        .deck
                        .shell
                        .update_flow
                        .last_result
                        .clone()
                        .unwrap_or_else(|| info.status.clone());
                    self.deck.shell.recovery = Some(info);
                    self.on_recovery_for_chain();
                    self.on_recovery_for_health();
                }
                Ok(Reply::GoogleStatus(status)) => {
                    self.deck.note = status.status_message.clone();
                    self.deck.shell.google_status = Some(status);
                }
                Ok(Reply::GoogleDriveFiles(files)) => {
                    self.deck.note =
                        "Development service reply; Drive enrollment is not implemented.".into();
                    self.deck.shell.drive_files = Some(files);
                }
                Ok(Reply::Backups(backups)) => {
                    self.deck.note =
                        "Development service reply; verified backup is not configured.".into();
                    self.deck.shell.backups = Some(backups);
                }
                Ok(Reply::Timezones(zones)) => {
                    self.deck.note = "Local timezone catalog loaded. Type to filter.".into();
                    self.deck.shell.timezones = Some(zones);
                }
                Ok(Reply::Timezone(zone)) => {
                    self.deck.note = format!(
                        "Timezone verified: {zone}{}.",
                        if self.deck.snapshot.as_ref().is_some_and(|s| s.simulated) {
                            " (simulated)"
                        } else {
                            ""
                        }
                    );
                    self.deck
                        .snapshot
                        .get_or_insert_with(Snapshot::default)
                        .timezone = Some(zone);
                    if matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == shell::Route::TimezoneConfirm
                    {
                        self.deck.shell.timezone_choice = None;
                        self.deck.shell.nav.back();
                        self.deck.shell.nav.back();
                    }
                }
                Ok(Reply::LocalClock(time)) => {
                    self.deck.note = format!(
                        "Clock set to {time}; network time is off until you switch it back on{}.",
                        if self.deck.snapshot.as_ref().is_some_and(|s| s.simulated) {
                            " (simulated)"
                        } else {
                            ""
                        }
                    );
                    self.deck
                        .snapshot
                        .get_or_insert_with(Snapshot::default)
                        .network_time = Some(false);
                    if matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == shell::Route::ManualClockConfirm
                    {
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.clock_form = None;
                        self.deck.shell.nav.back();
                        self.deck.shell.nav.back();
                    }
                }
                Ok(Reply::NetworkTime) => {
                    self.deck.note =
                        "Network time is on; the clock sets itself whenever WrOS is online.".into();
                    let snapshot = self.deck.snapshot.get_or_insert_with(Snapshot::default);
                    snapshot.network_time = Some(true);
                    snapshot.time_synchronized = None;
                }
                Ok(Reply::Audio(audio)) => {
                    if audio.available {
                        self.deck.gauges.volume = audio.volume_percent;
                        self.deck.gauges.muted = audio.muted;
                    }
                    self.deck.note = audio.message.clone();
                    self.deck.shell.audio_status = Some(audio);
                }
                Ok(Reply::DisplayStatus(status)) => {
                    self.deck.shell.warmth = status.warmth;
                    self.deck.note = status.message.clone();
                    self.deck.shell.display = Some(status);
                }
                Ok(Reply::DisplayPreview { token, seconds }) => {
                    if self.deck.shell.warm_wait
                        && matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == shell::Route::WarmScreen
                    {
                        self.deck.shell.warm_preview =
                            Some((token, Instant::now() + Duration::from_secs(seconds.min(15))));
                        self.deck.shell.nav.open(shell::Route::WarmPreview);
                        self.deck.note = "Temporary warmth applied. Choose Keep or Cancel; automatic rollback is armed.".into();
                    } else {
                        self.deck.request(Request::DisplayCancel(token));
                    }
                    self.deck.shell.warm_wait = false;
                }
                Ok(Reply::KeyboardPreview { token, seconds }) => {
                    if let Request::KeyboardPreview(layout) = request {
                        if matches!(self.overlay, Overlay::Deck(Page::KeyboardWait)) {
                            self.deck.preview = Some((
                                token,
                                layout,
                                Instant::now() + Duration::from_secs(seconds.min(15)),
                            ));
                            self.overlay = Overlay::Deck(Page::KeyboardTest {
                                text: String::new(),
                            });
                            self.deck.note =
                                "Test the layout here. Enter keeps it; Esc reverts.".into();
                        } else {
                            self.deck.request(Request::KeyboardCancel(token));
                        }
                    }
                }
                Err(e) => {
                    self.on_import_media_error(&request, &e);
                    self.deck.shell.warm_wait = false;
                    if matches!(request, Request::DisplayStatus) {
                        self.deck.shell.display = None;
                    }
                    if matches!(request, Request::ConnectWifi { .. })
                        && let Some(form) = &mut self.deck.shell.wifi_form
                    {
                        form.password.clear();
                    }
                    if matches!(
                        request,
                        Request::BootStatus | Request::BootTheme(_) | Request::BootScreen { .. }
                    ) {
                        self.deck.note =
                            "Couldn't change the start screen. Your project is safe. Try again."
                                .into();
                    } else if matches!(request, Request::ScanWifi) {
                        self.deck.note = shell::WIFI_SCAN_FAILED.into();
                    } else if !self.on_update_error(&request, &e) {
                        self.deck.note = format!("Unavailable: {e}");
                    }
                    if matches!(self.overlay, Overlay::Widget { .. })
                        && matches!(
                            request,
                            Request::Wifi(_)
                                | Request::SetMute(_)
                                | Request::SetVolume(_)
                                | Request::Brightness(_)
                        )
                    {
                        self.msg = self.deck.note.clone();
                    }
                    if matches!(self.overlay, Overlay::Deck(Page::KeyboardWait)) {
                        self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                    }
                }
            }
            if self.deck.simulated() && !self.deck.note.contains("SIMULATED") {
                self.deck.note = format!("SIMULATED · {}", self.deck.note);
            }
            self.deck.shell.notices.push(&self.deck.note);
        }
        if !self.deck.busy
            && let Some(theme) = self.deck.pending_boot_theme.take()
        {
            self.deck.request(Request::BootTheme(theme));
        }
        if self
            .deck
            .shell
            .warm_preview
            .as_ref()
            .is_some_and(|(_, end)| {
                Instant::now() >= *end || !matches!(self.overlay, Overlay::Deck(Page::Settings))
            })
            && !self.deck.busy
            && let Some((token, _)) = self.deck.shell.warm_preview.take()
        {
            self.deck.request(Request::DisplayCancel(token));
            if self.deck.shell.nav.current() == shell::Route::WarmPreview {
                self.deck.shell.nav.back();
            }
        }
        if self
            .deck
            .preview
            .as_ref()
            .is_some_and(|(_, _, end)| Instant::now() >= *end)
            && let Some((token, _, _)) = self.deck.preview.take()
        {
            self.deck.request(Request::KeyboardCancel(token));
            if matches!(self.overlay, Overlay::Deck(Page::KeyboardTest { .. })) {
                self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
            }
        }
        self.run_pending_settings_row();
        self.run_widget_wifi_scan();
    }
}
