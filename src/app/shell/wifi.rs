//! Wi-Fi scan, connection and saved-profile pages.

use super::{App, Choice, Entry, Key, Route};
use zeroize::Zeroizing;

pub(in crate::app) const WIFI_SCAN_FAILED: &str =
    "Couldn't scan for Wi-Fi. Make sure Wi-Fi is on, then try again.";

pub(crate) struct WifiForm {
    pub network: wros_sys::WifiNetwork,
    pub password: Zeroizing<String>,
    pub(super) selected: usize,
}
impl WifiForm {
    fn requires_password(&self) -> bool {
        !self.network.security.is_empty() && self.network.security != "--"
    }
    pub(super) fn action_count(&self) -> usize {
        if self.requires_password() {
            if self.network.in_use { 4 } else { 3 }
        } else if self.network.in_use {
            3
        } else {
            2
        }
    }
    fn rows(&self) -> Vec<String> {
        if self.requires_password() {
            let mut rows = vec![
                format!(
                    "Password (hidden): {}",
                    if self.password.is_empty() {
                        ""
                    } else {
                        "[hidden]"
                    }
                ),
                "Cancel and return to Wi-Fi networks".into(),
                format!("Connect to {}", self.network.ssid),
            ];
            if self.network.in_use {
                rows.push("Disconnect current Wi-Fi connection".into());
            }
            rows.push(format!(
                "Network: {} · Signal: {}% · Security: {}",
                self.network.ssid, self.network.signal, self.network.security
            ));
            rows.push(
                "WPA password: 8–63 bytes, or a 64-digit hexadecimal key. Input stays hidden."
                    .into(),
            );
            rows.push("Before Connect, Escape discards this form and password.".into());
            rows
        } else {
            let mut rows = vec![
                "Cancel and return to Wi-Fi networks".into(),
                format!("Connect to open network {}", self.network.ssid),
            ];
            if self.network.in_use {
                rows.push("Disconnect current Wi-Fi connection".into());
            }
            rows.push(format!(
                "Network: {} · Signal: {}% · Security: open",
                self.network.ssid, self.network.signal
            ));
            rows.push("Open networks do not encrypt wireless traffic.".into());
            rows
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SavedWifiAction {
    Cancel,
    Connect,
    Forget,
}

pub(super) fn network_panel_rows(app: &App, rows: &mut Vec<Entry>) {
    let status = app.deck.snapshot.as_ref();
    rows.extend([
        Entry::info(format!(
            "Connection: {}",
            status
                .and_then(|s| s.network.as_deref())
                .unwrap_or("unknown; refresh status")
        )),
        Entry::new("Enable Wi-Fi radio", Choice::System(1)),
        Entry::new("Disable Wi-Fi radio", Choice::System(2)),
        Entry::new(
            "Scan Wi-Fi networks",
            Choice::View(Route::WifiNetworks, wros_sys::Request::ScanWifi),
        ),
        Entry::new(
            "Saved Wi-Fi profiles",
            Choice::View(Route::SavedWifi, wros_sys::Request::ListSavedWifi),
        ),
        Entry::new("Disconnect active Wi-Fi", Choice::WifiDisconnect),
        Entry::info("Select a network in Scan Wi-Fi networks to connect or enter credentials."),
    ]);
}

pub(super) fn network_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::new(
        "Refresh scan",
        Choice::Read(wros_sys::Request::ScanWifi),
    )];
    if let Some(networks) = &app.deck.shell.wifi_networks {
        for network in networks {
            rows.push(Entry::new(
                format!(
                    "{}{}\nSignal: {}% · Security: {}",
                    if network.in_use { "Connected: " } else { "" },
                    network.ssid,
                    network.signal,
                    network.security
                ),
                Choice::WifiSelect(network.clone()),
            ));
        }
        if networks.is_empty() {
            rows.push(Entry::info("No visible named Wi-Fi networks returned."));
        }
    } else if app.deck.busy {
        rows.push(Entry::info("Looking for Wi-Fi networks…"));
    } else {
        rows.push(Entry::info(WIFI_SCAN_FAILED));
    }
    rows.push(Entry::info(
        "Select a network above to connect or enter protected credentials.",
    ));
    rows
}

pub(super) fn connect_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .wifi_form
        .as_ref()
        .map(|form| form.rows().into_iter().map(Entry::info).collect())
        .unwrap_or_default()
}

pub(super) fn saved_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![Entry::new(
        "Refresh saved profiles",
        Choice::Read(wros_sys::Request::ListSavedWifi),
    )];
    if let Some(profiles) = &app.deck.shell.wifi_saved {
        for profile in profiles {
            rows.push(Entry::new(
                format!("{}\nUUID: {}", profile.name, profile.uuid),
                Choice::SavedWifiSelect(profile.clone()),
            ));
        }
        if profiles.is_empty() {
            rows.push(Entry::info(
                "No saved Wi-Fi profiles are visible to this session.",
            ));
        }
    } else {
        rows.push(Entry::info(
            "Saved profiles unavailable; refresh to try again.",
        ));
    }
    rows.push(Entry::info(
        "Select a saved profile to connect, reconnect or remove it by UUID.",
    ));
    rows
}

pub(super) fn saved_profile_rows(app: &App) -> Vec<Entry> {
    let name = app
        .deck
        .shell
        .wifi_saved_choice
        .as_ref()
        .map_or("unavailable", |p| p.name.as_str());
    let uuid = app
        .deck
        .shell
        .wifi_saved_choice
        .as_ref()
        .map_or("unavailable", |p| p.uuid.as_str());
    vec![
        Entry::new(
            "Cancel and keep saved profile unchanged",
            Choice::SavedWifiResolve(SavedWifiAction::Cancel),
        ),
        Entry::new(
            format!("Connect or reconnect {name}"),
            Choice::SavedWifiResolve(SavedWifiAction::Connect),
        ),
        Entry::new(
            format!("Forget saved profile {name}"),
            Choice::SavedWifiResolve(SavedWifiAction::Forget),
        ),
        Entry::info(format!("Profile: {name} · UUID: {uuid}")),
        Entry::info("Operations target this exact 36-character NetworkManager UUID."),
    ]
}

impl App {
    pub(in crate::app) fn open_wifi_network(&mut self, network: wros_sys::WifiNetwork) {
        if !network.security.is_empty()
            && network.security != "--"
            && !network
                .security
                .split_whitespace()
                .all(|kind| matches!(kind, "WPA1" | "WPA2" | "WPA3"))
        {
            self.deck.note = "This network needs enrollment beyond a personal WPA password; \
                              use a supported saved profile or another network."
                .into();
            self.deck.shell.notices.push(&self.deck.note);
            return;
        }
        self.deck.shell.wifi_form = Some(WifiForm {
            network,
            password: Zeroizing::new(String::new()),
            selected: 0,
        });
        self.deck.shell.nav.open(Route::WifiConnect);
    }

    pub(crate) fn paste_wifi_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.wifi_form else {
            return;
        };
        if !form.requires_password() || form.selected != 0 {
            return;
        }
        if form.password.len() + text.len() <= 64 && !text.chars().any(char::is_control) {
            form.password.push_str(text);
        }
    }

    pub(super) fn on_wifi_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.wifi_form else {
            return;
        };
        let count = form.action_count();
        let requires_password = form.requires_password();
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.wifi_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % count,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    count.saturating_sub(1)
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if requires_password && form.selected == 0 => {
                form.password.pop();
            }
            Key::Delete if requires_password && form.selected == 0 => {
                form.password.clear();
            }
            Key::Char(c) if !c.is_control() && requires_password && form.selected == 0 => {
                self.paste_wifi_form(&c.to_string());
            }
            Key::Enter if requires_password && form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter => {
                let selected = form.selected;
                let cancel_idx = if requires_password { 1 } else { 0 };
                let connect_idx = if requires_password { 2 } else { 1 };
                let disconnect_idx = if requires_password { 3 } else { 2 };
                if selected == cancel_idx {
                    self.deck.shell.wifi_form = None;
                    self.deck.shell.nav.back();
                    return;
                }
                if self.deck.busy {
                    return;
                }
                if selected == disconnect_idx && form.network.in_use {
                    self.deck.request(wros_sys::Request::DisconnectWifi);
                    return;
                }
                if selected == connect_idx {
                    let ssid = form.network.ssid.clone();
                    if let Err(err) = wros_sys::system_views::validate_wifi_ssid(&ssid) {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                        return;
                    }
                    if requires_password {
                        if let Err(err) =
                            wros_sys::system_views::validate_wifi_password(&form.password)
                        {
                            self.deck.note = format!("Unavailable: {err}");
                            self.deck.shell.notices.push(&self.deck.note);
                            return;
                        }
                        let secret = wros_sys::WifiSecret::new(form.password.as_str());
                        form.password = Zeroizing::new(String::new());
                        self.deck.request(wros_sys::Request::ConnectWifi {
                            ssid,
                            password: Some(secret),
                        });
                    } else {
                        self.deck.request(wros_sys::Request::ConnectWifi {
                            ssid,
                            password: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}
