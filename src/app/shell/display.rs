//! Display and keyboard settings, including their previews.

use super::{App, Choice, Entry, Key, Overlay, Route, wros};
use std::time::Instant;

pub(super) struct KeyboardWizardForm {
    pub(super) base_index: usize,
    pub(super) alias_name: String,
    pub(super) selected: usize,
}
impl KeyboardWizardForm {
    fn rows(&self, custom_maps: &[wros_sys::keyboard::CustomMapRegistration]) -> Vec<String> {
        let base = wros_sys::keyboard::Layout::ALL[self.base_index].clone();
        let mut rows = vec![
            format!("Base layout: {} · {}", base.label(), base.description()),
            format!("Registered alias name (optional): {}", self.alias_name),
            "Cycle base layout (QWERTY / Colemak / Colemak-DH)".into(),
            "Preview base layout · 15-second hardware-level rollback".into(),
            "Register local alias for this base layout".into(),
            "Cancel and return to Interface settings".into(),
            "Arbitrary keymap compilation stays disabled; custom maps alias verified layouts."
                .into(),
        ];
        for reg in custom_maps {
            rows.push(format!(
                "Registered alias: {} → {}",
                reg.name,
                reg.base.id()
            ));
        }
        rows
    }
}

pub(super) fn display_panel_rows(app: &App, rows: &mut Vec<Entry>) {
    let status = app.deck.snapshot.as_ref();
    rows.extend([
        Entry::info(format!(
            "Brightness: {}",
            status
                .and_then(|s| s.brightness_percent)
                .map_or("unavailable".into(), |p| format!("{p}%"))
        )),
        Entry::new("Brightness -10%", Choice::System(5)),
        Entry::new("Brightness +10%", Choice::System(6)),
        if app.warm_screen_available() {
            Entry::new(
                "Warm screen · blue light filter",
                Choice::Open(Route::WarmScreen),
            )
        } else {
            Entry::info("Warm screen · unavailable in this build")
        },
    ]);
    if app.deck.session_settings.is_some() {
        rows.extend([
            Entry::new(
                format!(
                    "Graphical session: {} · from the next login",
                    if app.deck.graphical_chosen {
                        "on"
                    } else {
                        "off"
                    }
                ),
                Choice::GraphicalSession,
            ),
            Entry::info(
                "Graphical writing starts by default. Plain text is available for recovery.",
            ),
        ]);
    }
    if app.deck.boot_settings_available {
        if let Some(status) = &app.deck.boot_status {
            rows.push(Entry::new(
                format!(
                    "Start screen: {}",
                    if status.enabled { "WrOS" } else { "plain text" }
                ),
                Choice::BootScreen(false),
            ));
            rows.push(if status.failed && !status.pending {
                Entry::new("Prepare the start screen again", Choice::BootScreen(true))
            } else {
                Entry::info(status.preparation())
            });
        } else {
            rows.push(Entry::info("Start screen: refresh to read the choice."));
        }
    }
}

pub(super) fn warm_screen_rows(app: &App) -> Vec<Entry> {
    let shell = &app.deck.shell;
    let choice = shell.warmth;
    let status = shell
        .display
        .as_ref()
        .map_or("Not checked; check display support", |s| s.message.as_str());
    vec![
        Entry::new(
            "Check display support and saved settings",
            Choice::WarmStatus,
        ),
        Entry::info(status),
        Entry::info(
            shell
                .display
                .as_ref()
                .map_or("Display support: unknown".into(), |s| {
                    format!(
                        "Display support: {} · saved preference {}",
                        if s.available {
                            "available"
                        } else {
                            "unavailable"
                        },
                        if s.applied {
                            if crate::session::composited() {
                                "accepted for this session"
                            } else {
                                "matches display"
                            }
                        } else {
                            "not applied"
                        }
                    )
                }),
        ),
        Entry::new(
            format!("Filter: {}", if choice.enabled { "on" } else { "off" }),
            Choice::WarmToggle,
        ),
        Entry::new(
            format!("Temperature: {} K · warmer -100 K", choice.kelvin),
            Choice::WarmAdjust(-100, 0),
        ),
        Entry::new("Temperature: cooler +100 K", Choice::WarmAdjust(100, 0)),
        Entry::new(
            format!("Strength: {}% · decrease 10%", choice.strength),
            Choice::WarmAdjust(0, -10),
        ),
        Entry::new("Strength: increase 10%", Choice::WarmAdjust(0, 10)),
        Entry::new("Preview choice · keep within 15 seconds", Choice::WarmApply),
        Entry::info("1000–6500 K; 2200 K is very warm. Changes apply only after Preview."),
        Entry::info("Off restores the original color curves. Unsupported drivers stay unchanged."),
    ]
}

pub(super) fn warm_preview_rows(app: &App) -> Vec<Entry> {
    vec![
        Entry::info(format!(
            "Temporary display preview · {} seconds remaining",
            app.deck
                .shell
                .warm_preview
                .as_ref()
                .map_or(0, |(_, end)| end
                    .saturating_duration_since(Instant::now())
                    .as_secs())
        )),
        Entry::new(
            "Cancel and restore previous colors",
            Choice::WarmResolve(false),
        ),
        Entry::new("Keep these display settings", Choice::WarmResolve(true)),
        Entry::info("The independent system timer restores unconfirmed changes if WrOS exits."),
    ]
}

pub(super) fn keyboard_rows() -> Vec<Entry> {
    vec![
        Entry::new(
            "Keyboard layout · QWERTY / Colemak / Colemak-DH",
            Choice::System(4),
        ),
        Entry::new(
            "First-run keyboard layout wizard · registered maps only",
            Choice::KeyboardWizardOpen,
        ),
        Entry::new("Keyboard shortcuts", Choice::System(11)),
    ]
}

pub(super) fn keyboard_wizard_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .keyboard_wizard
        .as_ref()
        .map(|form| {
            form.rows(&app.deck.shell.custom_keymaps)
                .into_iter()
                .map(Entry::info)
                .collect()
        })
        .unwrap_or_default()
}

impl App {
    pub(in crate::app) fn warm_screen_available(&self) -> bool {
        !self.deck.graphical_session
            || crate::session::native()
            || cfg!(all(target_os = "linux", feature = "compositor-warmth"))
    }

    pub(super) fn resolve_warm_preview(&mut self, keep: bool) {
        if self.deck.busy {
            return;
        }
        if let Some((token, _)) = self.deck.shell.warm_preview.take() {
            self.deck.request(if keep {
                wros_sys::Request::DisplayConfirm(token)
            } else {
                wros_sys::Request::DisplayCancel(token)
            });
        }
        self.deck.shell.nav.back();
    }

    pub(crate) fn paste_keyboard_wizard(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.keyboard_wizard else {
            return;
        };
        if form.selected != 1 {
            return;
        }
        if form.alias_name.len() + text.len() <= 32 && !text.chars().any(char::is_control) {
            form.alias_name.push_str(text);
        }
    }

    pub(super) fn on_keyboard_wizard_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.keyboard_wizard else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.keyboard_wizard = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % 6,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    5
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if form.selected == 1 => {
                form.alias_name.pop();
            }
            Key::Delete if form.selected == 1 => {
                form.alias_name.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 1 => {
                self.paste_keyboard_wizard(&c.to_string());
            }
            Key::Enter if form.selected == 0 || form.selected == 2 => {
                form.base_index = (form.base_index + 1) % wros_sys::keyboard::Layout::ALL.len();
            }
            Key::Enter if form.selected == 1 => {
                form.selected = 4;
            }
            Key::Enter if form.selected == 3 => {
                if self.deck.busy {
                    return;
                }
                let base = wros_sys::keyboard::Layout::ALL[form.base_index].clone();
                self.deck.shell.external = true;
                self.deck.request(wros_sys::Request::KeyboardPreview(base));
                if self.deck.busy {
                    self.overlay = Overlay::Deck(wros::Page::KeyboardWait);
                }
            }
            Key::Enter if form.selected == 4 => {
                let base = wros_sys::keyboard::Layout::ALL[form.base_index].clone();
                let name = form.alias_name.trim().to_string();
                match wros_sys::keyboard::CustomMapRegistration::new(&name, base.clone()) {
                    Ok(reg) => {
                        self.deck
                            .shell
                            .custom_keymaps
                            .retain(|existing| existing.name != reg.name);
                        self.deck.shell.custom_keymaps.push(reg);
                        self.deck.note =
                            format!("Registered keyboard alias '{name}' for {}.", base.id());
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                    Err(err) => {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                }
            }
            Key::Enter if form.selected == 5 => {
                self.deck.shell.keyboard_wizard = None;
                self.deck.shell.nav.back();
            }
            _ => {}
        }
    }
}
