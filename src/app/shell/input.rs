//! Keyboard and pointer input for the Settings menu.

#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::pages::{entries, kinds};
use super::{
    App, Choice, ClockForm, Key, KeyboardWizardForm, Overlay, PasswordForm, Route, SavedWifiAction,
    UpdateBundleForm, music, wros,
};
use crate::app::Tab;
use crate::geometry::to_u16_i;
use crate::palette::Action;

impl App {
    pub(crate) fn on_settings_key(&mut self, key: Key) {
        if self.deck.shell.nav.current() == Route::BackupForm {
            self.on_backup_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::WifiConnect {
            self.on_wifi_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::ChangePassword {
            self.on_password_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::MusicServerForm {
            self.on_music_server_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::UpdateBundleForm {
            self.on_update_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::ManualClock {
            self.on_clock_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::KeyboardWizard {
            self.on_keyboard_wizard_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::Timezones {
            let query = &mut self.deck.shell.timezone_query;
            let changed = match key {
                Key::Char(c)
                    if query.len() < 64 && (c.is_ascii_alphanumeric() || " /_+-".contains(c)) =>
                {
                    query.push(c);
                    true
                }
                Key::Backspace => query.pop().is_some(),
                _ => false,
            };
            if changed {
                self.deck.shell.nav.select(1, entries(self).len());
                return;
            }
        }
        if self.deck.shell.nav.current() == Route::WarmPreview
            && matches!(key, Key::Esc | Key::Left)
        {
            self.resolve_warm_preview(false);
            return;
        }
        if self.deck.shell.nav.current() == Route::LinkPhone && key == Key::Esc {
            // Leaving the code stops linking: the listener closes and the sign-in is dropped.
            if self.deck.shell.backup.job.take().is_some() {
                self.deck.shell.backup.notice = "Linking cancelled; nothing was changed.".into();
            }
            self.deck.shell.nav.back();
            return;
        }
        let rows = entries(self);
        let kinds = kinds(self, &rows);
        let nav = &mut self.deck.shell.nav;
        match key {
            Key::Up => nav.move_among(-1, &kinds, rows.len()),
            Key::Down => nav.move_among(1, &kinds, rows.len()),
            Key::PageUp => nav.move_among(-10, &kinds, rows.len()),
            Key::PageDown => nav.move_among(10, &kinds, rows.len()),
            Key::Home => {
                nav.select(0, rows.len());
                nav.settle(&kinds, rows.len());
            }
            Key::End => {
                nav.select(rows.len().saturating_sub(1), rows.len());
                nav.settle(&kinds, rows.len());
            }
            Key::Esc | Key::Left => {
                self.deck.shell.warm_wait = false;
                self.deck.shell.wifi_form = None;
                self.deck.shell.password_form = None;
                self.deck.shell.wifi_saved_choice = None;
                self.deck.shell.update_form = None;
                self.deck.shell.clock_form = None;
                self.deck.shell.clock_choice = None;
                self.deck.shell.keyboard_wizard = None;
                if nav.current() == Route::UpdateReview {
                    // Backing out of a review drops it; it must be authenticated again.
                    let flow = &mut self.deck.shell.update_flow;
                    flow.review = None;
                    flow.dir = None;
                    flow.changed = None;
                }
                if !nav.back() {
                    self.deck.shell.save();
                    self.overlay = Overlay::None;
                }
            }
            Key::Enter | Key::Right => {
                nav.select(nav.selected(), rows.len());
                nav.settle(&kinds, rows.len());
                let Some(row) = rows.into_iter().nth(nav.selected()) else {
                    return;
                };
                match row.choice {
                    Choice::BackupRequest(crate::backup_service::Request::RunQueue) => {
                        self.request_run_backup_queue()
                    }
                    Choice::BackupRequest(request) => self.deck.shell.backup.request(request),
                    Choice::BackupForm(kind, snapshot) => self.open_backup_form(kind, snapshot),
                    Choice::DriveFolder(folder) => {
                        if self.deck.shell.nav.current() != Route::GoogleDrive {
                            self.deck.shell.nav.open(Route::GoogleDrive);
                        }
                        if self.deck.shell.backup.status.google_connected {
                            self.deck
                                .shell
                                .backup
                                .request(crate::backup_service::Request::DriveList { folder });
                        }
                    }
                    Choice::DriveCopyBook(remote) => self.open_drive_import(remote),
                    Choice::YoutubePlayToggle => self.toggle_youtube_play(),
                    Choice::BackupNow => self.request_project_backup(),
                    Choice::BackupCommonplace => self.request_commonplace_backup(),
                    Choice::SyncSwitch(on) => self.request_sync_switch(on),
                    Choice::SyncNow => self.request_sync_run(false),
                    Choice::SyncConfirmDeletes => self.request_sync_run(true),
                    Choice::BackupQueueNow => self.request_project_backup_queue(),
                    Choice::LinkPhone => {
                        self.deck.shell.nav.open(Route::LinkPhone);
                        self.deck
                            .shell
                            .backup
                            .request(crate::backup_service::Request::LinkPhone);
                    }
                    Choice::BackupCancel => {
                        self.deck.shell.backup.job = None;
                        self.deck.shell.backup.notice = "Cancelling; refresh snapshots before retrying. Completed snapshots remain recoverable.".into();
                    }
                    Choice::GoogleDisconnectResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::GoogleDisconnectResolve(true) => {
                        self.deck.shell.backup.tick();
                        if self.deck.shell.backup.job.is_some() {
                            self.deck.shell.backup.notice = "Wait for the current backup operation, then confirm disconnect again.".into();
                            return;
                        }
                        self.deck.shell.nav.back();
                        self.deck
                            .shell
                            .backup
                            .request(crate::backup_service::Request::DisconnectGoogle);
                    }
                    Choice::WifiSelect(network) if !self.deck.busy => {
                        self.open_wifi_network(network);
                    }
                    Choice::WifiDisconnect if !self.deck.busy => {
                        self.deck.request(wros_sys::Request::DisconnectWifi);
                    }
                    Choice::SavedWifiSelect(profile) if !self.deck.busy => {
                        self.deck.shell.wifi_saved_choice = Some(profile);
                        self.deck.shell.nav.open(Route::SavedWifiProfile);
                        self.deck.shell.nav.select(0, 5);
                    }
                    Choice::SavedWifiResolve(SavedWifiAction::Cancel) => {
                        self.deck.shell.wifi_saved_choice = None;
                        self.deck.shell.nav.back();
                    }
                    Choice::SavedWifiResolve(action) if !self.deck.busy => {
                        if let Some(profile) = self.deck.shell.wifi_saved_choice.take() {
                            self.deck.shell.nav.back();
                            match action {
                                SavedWifiAction::Cancel => {}
                                SavedWifiAction::Connect => self
                                    .deck
                                    .request(wros_sys::Request::ActivateSavedWifi(profile.uuid)),
                                SavedWifiAction::Forget => self
                                    .deck
                                    .request(wros_sys::Request::ForgetWifi(profile.uuid)),
                            }
                        }
                    }
                    Choice::PasswordOpen if !self.deck.busy => {
                        self.deck.shell.password_form = Some(PasswordForm::new());
                        self.deck.shell.nav.open(Route::ChangePassword);
                    }
                    Choice::UpdateBundleOpen if !self.deck.busy => {
                        self.deck.shell.update_form = Some(UpdateBundleForm {
                            bundle_dir: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.nav.open(Route::UpdateBundleForm);
                    }
                    Choice::WifiSelect(_)
                    | Choice::WifiDisconnect
                    | Choice::SavedWifiSelect(_)
                    | Choice::SavedWifiResolve(_)
                    | Choice::PasswordOpen
                    | Choice::UpdateBundleOpen => {}
                    Choice::UpdateFindMedia => self.update_find_media(),
                    Choice::UpdateOnlineCheck => self.update_online_check(),
                    Choice::UpdateOnlineFetch => self.update_online_fetch(),
                    Choice::UpdateContinue => self.update_continue(),
                    Choice::UpdateStage => self.update_stage(),
                    Choice::UpdateBackupThenStage(restart) => {
                        self.update_backup_then_stage(restart)
                    }
                    Choice::UpdateAbandon => self.update_abandon(),
                    Choice::UpdateStagedOpen => self.update_open_staged(),
                    Choice::UpdateRestart => self.update_restart(),
                    Choice::UpdateCancelStaged => self.update_cancel_staged(),
                    Choice::UpdateApplyNow => self.update_apply_now(),
                    Choice::UpdateGoBackOpen => self.update_go_back_open(),
                    Choice::UpdateGoBack => self.update_go_back(),
                    Choice::RepairOpen => self.repair_open(),
                    Choice::RepairRun => self.repair_run(),
                    Choice::Lock => self.request_console_lock(),
                    Choice::RestartApp => self.restart_writing_app(),
                    Choice::LidClose => self.cycle_lid_close(),
                    Choice::GraphicalSession => self.toggle_graphical_session(),
                    Choice::BootScreen(retry) => {
                        if !self.deck.busy
                            && let Some(status) = &self.deck.boot_status
                        {
                            self.deck.request(wros_sys::Request::BootScreen {
                                enabled: if retry {
                                    status.enabled
                                } else {
                                    !status.enabled
                                },
                                theme: crate::theme::to_config(&self.theme),
                            });
                        }
                    }
                    // Asking is safe while a request runs (opening Storage refreshes it, so
                    // the WrOS menu's Install always arrives busy); starting waits below.
                    Choice::InstallerOpen => {
                        self.deck.shell.nav.open(Route::InstallerConfirm);
                        self.deck.shell.nav.select(0, 4);
                    }
                    Choice::InstallerResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::InstallerResolve(true) if !self.deck.busy => {
                        self.deck.shell.nav.back();
                        self.request_live_installer();
                    }
                    Choice::InstallerResolve(true) => {}
                    Choice::TimezoneOpen if !self.deck.busy => {
                        self.deck.shell.timezones = None;
                        self.deck.shell.timezone_query.clear();
                        self.deck.shell.timezone_choice = None;
                        self.deck.shell.nav.open(Route::Timezones);
                        self.deck.request(wros_sys::Request::ListTimezones);
                    }
                    Choice::TimezoneChoose(zone) if !self.deck.busy => {
                        self.deck.shell.timezone_choice = Some(zone);
                        self.deck.shell.nav.open(Route::TimezoneConfirm);
                        self.deck.shell.nav.select(0, 3);
                    }
                    Choice::TimezoneResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::TimezoneResolve(true) if !self.deck.busy => {
                        if let Some(zone) = self.deck.shell.timezone_choice.clone()
                            && self
                                .deck
                                .shell
                                .timezones
                                .as_ref()
                                .is_some_and(|zones| zones.contains(&zone))
                        {
                            self.deck.request(wros_sys::Request::SetTimezone(zone));
                        }
                    }
                    Choice::TimezoneOpen
                    | Choice::TimezoneChoose(_)
                    | Choice::TimezoneResolve(true) => {}
                    Choice::ClockOpen if !self.deck.busy => {
                        self.deck.shell.clock_form = Some(ClockForm {
                            datetime: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.nav.open(Route::ManualClock);
                    }
                    Choice::ClockResolve(false) => {
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.nav.back();
                    }
                    Choice::ClockResolve(true) if !self.deck.busy => {
                        if let Some(dt) = self.deck.shell.clock_choice.clone() {
                            self.deck.request(wros_sys::Request::SetLocalClock(dt));
                        }
                    }
                    Choice::ClockOpen | Choice::ClockResolve(true) => {}
                    Choice::SoundVolume(delta) if !self.deck.busy => {
                        if let Some(audio) = &self.deck.shell.audio_status
                            && audio.available
                            && let Some(current) = audio.volume_percent
                        {
                            let next = u8::try_from((i16::from(current) + delta).clamp(0, 100))
                                .unwrap_or(100);
                            self.deck.request(wros_sys::Request::SetVolume(next));
                        } else {
                            self.deck.note =
                                "No supported sound output is available; volume unchanged.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
                    }
                    Choice::SoundMuteToggle if !self.deck.busy => {
                        if let Some(audio) = &self.deck.shell.audio_status
                            && audio.available
                        {
                            self.deck.request(wros_sys::Request::SetMute(!audio.muted));
                        } else {
                            self.deck.note =
                                "No supported sound output is available; mute unchanged.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
                    }
                    Choice::SoundVolume(_) | Choice::SoundMuteToggle => {}
                    Choice::KeyboardWizardOpen if !self.deck.busy => {
                        self.deck.shell.keyboard_wizard = Some(KeyboardWizardForm {
                            base_index: 0,
                            alias_name: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.nav.open(Route::KeyboardWizard);
                    }
                    Choice::KeyboardWizardOpen => {}
                    Choice::Open(route) => self.open_route(route),
                    Choice::View(route, request) if !self.deck.busy => {
                        self.deck.shell.nav.open(route);
                        self.clear_system_view(&request);
                        self.deck.request(request);
                    }
                    Choice::Read(request) if !self.deck.busy => {
                        self.clear_system_view(&request);
                        self.deck.request(request);
                    }
                    Choice::View(..) | Choice::Read(_) => {}
                    Choice::Information => self.deck.shell.notices.push(&row.label),
                    Choice::WarmAdjust(kelvin, strength) if !self.deck.busy => {
                        let choice = &mut self.deck.shell.warmth;
                        choice.kelvin = to_u16_i(
                            (i32::from(choice.kelvin) + i32::from(kelvin)).clamp(1000, 6500),
                        );
                        choice.strength =
                            u8::try_from((i16::from(choice.strength) + strength).clamp(0, 100))
                                .unwrap_or(100);
                    }
                    Choice::WarmToggle if !self.deck.busy => {
                        self.deck.shell.warmth.enabled = !self.deck.shell.warmth.enabled
                    }
                    Choice::WarmStatus if !self.deck.busy => {
                        self.deck.request(wros_sys::Request::DisplayStatus)
                    }
                    Choice::WarmApply if !self.deck.busy => {
                        if self
                            .deck
                            .shell
                            .display
                            .as_ref()
                            .is_some_and(|s| s.available)
                        {
                            self.deck
                                .request(wros_sys::Request::DisplayPreview(self.deck.shell.warmth));
                            self.deck.shell.warm_wait = self.deck.busy;
                        } else {
                            self.deck.shell.notices.push(
                                "Display support is unavailable or unchecked; colors unchanged.",
                            );
                        }
                    }
                    Choice::WarmResolve(keep) if !self.deck.busy => self.resolve_warm_preview(keep),
                    Choice::WarmAdjust(..)
                    | Choice::WarmToggle
                    | Choice::WarmStatus
                    | Choice::WarmApply
                    | Choice::WarmResolve(_) => {}
                    Choice::Action(Action::MusicSource)
                        if self.deck.shell.nav.current() == Route::MusicPage =>
                    {
                        let wros = self.wros_music;
                        let sources: Vec<_> = music::Source::ALL
                            .into_iter()
                            .filter(|source| source.available(wros))
                            .collect();
                        let index = sources
                            .iter()
                            .position(|&source| source == self.music.source)
                            .unwrap_or(0);
                        self.select_music_source(sources[(index + 1) % sources.len()], false);
                    }
                    Choice::Action(action) => {
                        self.deck.shell.external = true;
                        let search_music = action == Action::MusicPlayer
                            && self.deck.shell.nav.current() == Route::MusicPage
                            && self.music.source == music::Source::YouTubeMusic
                            && music::ytm_helper_path().is_some();
                        self.run_action(action);
                        if search_music
                            && let Overlay::Player {
                                tab,
                                follow,
                                typing,
                                ..
                            } = &mut self.overlay
                        {
                            *tab = Tab::Search;
                            *follow = false;
                            *typing = true;
                        }
                        self.return_to_settings();
                    }
                    Choice::System(sel) => {
                        self.deck.shell.external = true;
                        self.overlay = Overlay::Deck(wros::Page::System { sel });
                        self.on_deck_key(Key::Enter);
                        self.return_to_settings();
                    }
                }
            }
            _ => {}
        }
    }
}

impl App {
    /// Forms take their own keys; a click doesn't choose a row in them.
    fn settings_form_open(&self) -> bool {
        matches!(
            self.deck.shell.nav.current(),
            Route::BackupForm
                | Route::WifiConnect
                | Route::ChangePassword
                | Route::MusicServerForm
                | Route::UpdateBundleForm
                | Route::ManualClock
                | Route::KeyboardWizard
        )
    }

    fn settings_row_at(&self, x: u16, y: u16) -> Option<usize> {
        self.settings_hits
            .borrow()
            .iter()
            .find(|(r, _)| x >= r.x && x < r.right() && y == r.y)
            .map(|(_, i)| *i)
    }

    /// A click with Settings up: a row opens (or runs), as Enter on it would;
    /// outside the box closes Settings, unless a form is being filled in.
    /// True if Settings took the click.
    pub(in crate::app) fn settings_click(&mut self, x: u16, y: u16) -> bool {
        if self.overlay != Overlay::Deck(wros::Page::Settings) {
            return false;
        }
        let area = self.settings_area.get();
        let inside = x >= area.x && x < area.right() && y >= area.y && y < area.bottom();
        if self.settings_form_open() {
            return true;
        }
        if !inside {
            for _ in 0..12 {
                if self.overlay != Overlay::Deck(wros::Page::Settings) {
                    break;
                }
                self.on_settings_key(Key::Esc);
            }
            return true;
        }
        if let Some(i) = self.settings_row_at(x, y) {
            let rows = entries(self);
            let kinds = kinds(self, &rows);
            if kinds
                .get(i)
                .is_some_and(|k| *k != wros_shell::RowKind::Info)
            {
                self.deck.shell.nav.select(i, rows.len());
                self.on_settings_key(Key::Enter);
            }
        }
        true
    }

    /// The pointer over a Settings row lights it.
    pub(crate) fn settings_hover(&mut self, x: u16, y: u16) {
        if self.overlay != Overlay::Deck(wros::Page::Settings) || self.settings_form_open() {
            return;
        }
        if let Some(i) = self.settings_row_at(x, y) {
            let rows = entries(self);
            let kinds = kinds(self, &rows);
            if kinds
                .get(i)
                .is_some_and(|k| *k != wros_shell::RowKind::Info)
            {
                self.deck.shell.nav.select(i, rows.len());
            }
        }
    }

    /// The wheel over Settings moves its highlight, as ↑↓ do. True if it did.
    pub(in crate::app) fn settings_wheel(&mut self, down: bool) -> bool {
        if self.overlay != Overlay::Deck(wros::Page::Settings) {
            return false;
        }
        self.on_settings_key(if down { Key::Down } else { Key::Up });
        true
    }
}
