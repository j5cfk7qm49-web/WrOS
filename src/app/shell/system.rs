//! System status, storage, time and sound settings.

use super::{App, Choice, Entry, Key, Panel, Route, display, power, update_flow, wifi};

pub(in crate::app) struct ClockForm {
    pub(super) datetime: String,
    pub(super) selected: usize,
}
impl ClockForm {
    fn rows(&self) -> Vec<String> {
        vec![
            format!(
                "Local date and time (YYYY-MM-DD HH:MM[:SS]): {}",
                self.datetime
            ),
            "Cancel and keep current local clock".into(),
            "Review manual clock change · default-Cancel confirmation next".into(),
            "Sets local system time via org.wros.clock with network time (NTP) kept off.".into(),
            "Existing writing records keep their recorded timestamps.".into(),
        ]
    }
}

pub(super) fn panel_rows(app: &App, panel: Panel) -> Vec<Entry> {
    let mut rows = vec![Entry::new("Refresh local system status", Choice::System(0))];
    match panel {
        Panel::Network => wifi::network_panel_rows(app, &mut rows),
        Panel::Display => display::display_panel_rows(app, &mut rows),
        Panel::Power => power::power_panel_rows(app, &mut rows),
        Panel::Storage => storage_panel_rows(app, &mut rows),
        Panel::DateTime => date_time_panel_rows(app, &mut rows),
        Panel::Updates => rows.extend(update_flow::updates_panel(app)),
        Panel::Recovery => rows.extend(update_flow::recovery_panel()),
    }
    rows
}

fn storage_panel_rows(app: &App, rows: &mut Vec<Entry>) {
    let status = app.deck.snapshot.as_ref();
    rows.push(Entry::new(
        "Mounted filesystems",
        Choice::View(
            Route::StorageDevicesView,
            wros_sys::Request::ListStorageDevices,
        ),
    ));
    rows.push(Entry::info(format!(
        "Available local storage: {}",
        status
            .and_then(|s| s.available_bytes)
            .map_or("unknown".into(), |n| format!("{} MiB", n / (1024 * 1024)))
    )));
    if app.deck.live_session && app.deck.installer_available {
        rows.push(Entry::new(
            "Install WrOS to disk · encrypted live installer",
            Choice::InstallerOpen,
        ));
    } else if app.deck.live_session {
        rows.push(Entry::info(
            "Live installer unavailable on this console or image",
        ));
    } else {
        rows.push(Entry::info(
            "System installed · live installer runs only from live media",
        ));
    }
}

fn date_time_panel_rows(app: &App, rows: &mut Vec<Entry>) {
    let status = app.deck.snapshot.as_ref();
    rows.extend([
        Entry::info(
            match (
                status.and_then(|s| s.network_time),
                status.and_then(|s| s.time_synchronized),
            ) {
                (Some(true), Some(true)) => "Network time: on · synchronized",
                (Some(true), _) => "Network time: on · not yet synchronized",
                (Some(false), _) => "Network time: off",
                (None, _) => "Network time: unknown",
            },
        ),
        Entry::info(format!(
            "Timezone: {}",
            status
                .and_then(|s| s.timezone.as_deref())
                .unwrap_or("unknown")
        )),
        Entry::new("Choose timezone", Choice::TimezoneOpen),
        Entry::new(
            "Set the clock by hand (turns network time off)",
            Choice::ClockOpen,
        ),
    ]);
    if status.and_then(|s| s.network_time) == Some(false) {
        rows.push(Entry::new(
            "Use network time again (sets the clock automatically)",
            Choice::Read(wros_sys::Request::UseNetworkTime),
        ));
    }
}

pub(super) fn storage_devices_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::new(
        "Refresh mounted filesystems",
        Choice::Read(wros_sys::Request::ListStorageDevices),
    )];
    if let Some(devices) = &app.deck.shell.storage_devices {
        for device in devices {
            rows.push(Entry::info(format!(
                "{}\n{}\n{} MiB free of {} MiB · {}% used",
                device.mountpoint,
                device.filesystem,
                device.available_bytes / 1_048_576,
                device.total_bytes / 1_048_576,
                device.use_percent
            )));
        }
        if devices.is_empty() {
            rows.push(Entry::info("No supported filesystems returned."));
        }
    } else {
        rows.push(Entry::info(
            "Mounted filesystems unavailable; refresh to try again.",
        ));
    }
    rows
}

pub(super) fn installer_confirm_rows() -> Vec<Entry> {
    vec![
        Entry::new(
            "Cancel and remain in the live writing session",
            Choice::InstallerResolve(false),
        ),
        Entry::new(
            "Start encrypted WrOS installer on console tty7",
            Choice::InstallerResolve(true),
        ),
        Entry::info("Retires writing input on tty1 while the WrOS installer runs on tty7."),
        Entry::info(
            "Disk writes remain default-No inside the installer until you explicitly confirm.",
        ),
    ]
}

pub(super) fn timezone_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::info(format!(
        "Filter: {} · type a city or region",
        app.deck.shell.timezone_query
    ))];
    if let Some(zones) = &app.deck.shell.timezones {
        let query = app
            .deck
            .shell
            .timezone_query
            .to_ascii_lowercase()
            .replace(' ', "_");
        rows.extend(
            zones
                .iter()
                .filter(|zone| zone.to_ascii_lowercase().contains(&query))
                .map(|zone| Entry::new(zone, Choice::TimezoneChoose(zone.clone()))),
        );
        if rows.len() == 1 {
            rows.push(Entry::info(
                "No matching timezones; Backspace edits the filter",
            ));
        }
    } else {
        rows.push(Entry::info(
            "Timezone list unavailable; return and try again",
        ));
    }
    rows
}

pub(super) fn timezone_confirm_rows(app: &App) -> Vec<Entry> {
    vec![
        Entry::new("Cancel timezone change", Choice::TimezoneResolve(false)),
        Entry::new(
            format!(
                "Use {}",
                app.deck
                    .shell
                    .timezone_choice
                    .as_deref()
                    .unwrap_or("unavailable")
            ),
            Choice::TimezoneResolve(true),
        ),
        Entry::info(
            "Changes future local dates. Existing writing records keep their recorded dates.",
        ),
    ]
}

pub(super) fn clock_form_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .clock_form
        .as_ref()
        .map(|form| form.rows().into_iter().map(Entry::info).collect())
        .unwrap_or_default()
}

pub(super) fn clock_confirm_rows(app: &App) -> Vec<Entry> {
    vec![
        Entry::new("Cancel manual clock change", Choice::ClockResolve(false)),
        Entry::new(
            format!(
                "Set local clock to {}",
                app.deck
                    .shell
                    .clock_choice
                    .as_deref()
                    .unwrap_or("unavailable")
            ),
            Choice::ClockResolve(true),
        ),
        Entry::info(
            "Network time stays off until you switch it back on. Existing writing records keep their recorded dates.",
        ),
    ]
}

pub(super) fn sound_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::new(
        "Refresh sound status",
        Choice::Read(wros_sys::Request::AudioStatus),
    )];
    if let Some(audio) = &app.deck.shell.audio_status {
        rows.push(Entry::info(format!(
            "Output: {} · Volume: {} · Muted: {}",
            if audio.available {
                "available"
            } else {
                "unavailable"
            },
            audio
                .volume_percent
                .map_or("unavailable".into(), |v| format!("{v}%")),
            if audio.muted { "yes" } else { "no" }
        )));
        rows.push(Entry::info(&audio.message));
        if audio.available {
            rows.push(Entry::new("Volume -10%", Choice::SoundVolume(-10)));
            rows.push(Entry::new("Volume +10%", Choice::SoundVolume(10)));
            rows.push(Entry::new(
                if audio.muted {
                    "Unmute sound output"
                } else {
                    "Mute sound output"
                },
                Choice::SoundMuteToggle,
            ));
        }
    } else {
        rows.push(Entry::info(
            "Sound status unavailable; refresh to check local mixer.",
        ));
    }
    rows.push(Entry::info(
        "Writing never depends on sound; a computer without a sound card says so here.",
    ));
    rows
}

impl App {
    pub(super) fn clear_system_view(&mut self, request: &wros_sys::Request) {
        use wros_sys::Request;
        match request {
            Request::ScanWifi => self.deck.shell.wifi_networks = None,
            Request::ListSavedWifi => self.deck.shell.wifi_saved = None,
            Request::ListLocalAccounts => self.deck.shell.local_accounts = None,
            Request::ListStorageDevices => self.deck.shell.storage_devices = None,
            Request::CheckUpdates => self.deck.shell.updates = None,
            Request::RecoveryStatus => self.deck.shell.recovery = None,
            Request::GetGoogleStatus => self.deck.shell.google_status = None,
            Request::AudioStatus => self.deck.shell.audio_status = None,
            _ => {}
        }
    }

    pub(crate) fn paste_clock_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.clock_form else {
            return;
        };
        if form.selected != 0 {
            return;
        }
        if form.datetime.len() + text.len() <= 32 && !text.chars().any(char::is_control) {
            form.datetime.push_str(text);
        }
    }

    pub(super) fn on_clock_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.clock_form else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.clock_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % 3,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    2
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if form.selected == 0 => {
                form.datetime.pop();
            }
            Key::Delete if form.selected == 0 => {
                form.datetime.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 0 => {
                self.paste_clock_form(&c.to_string());
            }
            Key::Enter if form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter if form.selected == 1 => {
                self.deck.shell.clock_form = None;
                self.deck.shell.nav.back();
            }
            Key::Enter if form.selected == 2 => {
                let candidate = form.datetime.trim().to_string();
                match wros_sys::timezone::validate_local_datetime(&candidate) {
                    Ok(normalized) => {
                        self.deck.shell.clock_choice = Some(normalized);
                        self.deck.shell.nav.open(Route::ManualClockConfirm);
                        self.deck.shell.nav.select(0, 3);
                    }
                    Err(err) => {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                }
            }
            _ => {}
        }
    }
}
