//! Test builds only (feature `test-driver`): publish the available controls over
//! a private taria socket so screen tests choose them by name. The socket is
//! bound only when `WROS_TEST_DRIVER_SOCKET` names one.

use crate::app::shell::{Route, driver_page};
use crate::app::{App, Key, Overlay};
use crate::palette::Sub;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use taria_ratatui::taria::{Action, AgentInput, Node, Role};
use taria_ratatui::{InputStatus, TariaLayer};

#[cfg(not(debug_assertions))]
compile_error!("test-driver is for test builds only; WrOS packages never include it");

pub struct Driver(TariaLayer, &'static str);

impl Driver {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("WROS_TEST_DRIVER_SOCKET")?;
        TariaLayer::bind_at("wros", std::path::PathBuf::from(path))
            .ok()
            .map(|layer| Self(layer, "unknown"))
    }

    pub fn input_backend(&mut self, source: &'static str) {
        self.1 = source;
    }

    #[cfg(target_os = "linux")]
    pub fn publish_lock_handoff(&mut self) {
        self.0
            .publish([Node::new("lock-handoff", Role::Text).label("Writing session lock handoff")]);
    }

    #[cfg(target_os = "linux")]
    pub fn reopen_after_lock(self) -> Option<Self> {
        let source = self.1;
        // Retire the listener, including any input it received during the
        // handoff. A new connection can act only on the resumed writer.
        drop(self);
        let mut driver = Self::from_env()?;
        driver.input_backend(source);
        Some(driver)
    }

    pub fn publish(&mut self, app: &App) {
        let mut nodes = vec![
            Node::new("system-busy", Role::Text)
                .label("System request pending")
                .value(app.deck.busy.to_string()),
            Node::new("system-notice", Role::Text)
                .label("System notice")
                .value(app.deck.shell.notices.latest().unwrap_or("")),
            Node::new("input-backend", Role::Text)
                .label("Input backend")
                .value(self.1),
            Node::new("open-settings", Role::Button)
                .label("Open Settings")
                .action(Action::Activate),
            Node::new("status", Role::Text).label("Status").value(
                app.deck
                    .battery
                    .showing(std::time::Instant::now())
                    .unwrap_or(&app.msg)
                    .to_string(),
            ),
        ];
        if app.overlay == Overlay::None {
            nodes.push(button("open-menu", "Open menu"));
        }
        if let Some((title, rows)) = settings(app) {
            let selected = app.deck.shell.nav.selected();
            nodes.push(Node::new("page", Role::List).label(title).children(
                rows.into_iter().enumerate().map(|(i, (label, choosable))| {
                    let row = Node::new(format!("row-{i}"), Role::ListItem)
                        .label(label)
                        .focused(i == selected);
                    if choosable {
                        row.action(Action::Activate)
                    } else {
                        row
                    }
                }),
            ));
        }
        if let Some((id, label, selected)) = menu(&app.overlay) {
            nodes.push(
                Node::new(id, Role::List).label(label).children(
                    app.menu_rows()
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, _))| {
                            Node::new(format!("{id}/{label}"), Role::ListItem)
                                .label(label)
                                .focused(i == selected)
                                .action(Action::Activate)
                        }),
                ),
            );
        }
        if let Some((id, title, accept, _)) = dialog(&app.overlay) {
            nodes.push(Node::new(id, Role::Dialog).label(title).children([
                button(format!("{id}-accept"), accept),
                button(format!("{id}-cancel"), "Cancel"),
            ]));
        }
        self.0.publish(nodes);
    }

    /// Applies what the test sent: a row chosen by name, or keys and text routed
    /// as a person's would be, through `key`.
    pub fn drain(&self, app: &mut App, mut key: impl FnMut(&mut App, KeyEvent)) {
        self.0.drain(|input| {
            if app.deck.lock_requested || app.deck.power_pending {
                return InputStatus::Ignored;
            }
            match input {
                AgentInput::Act {
                    node,
                    action: Action::Activate,
                    ..
                } => activate(app, &node.0, &mut key),
                AgentInput::Key { key: name, .. } => match taria_ratatui::to_crossterm_key(&name) {
                    Some(event) => {
                        key(app, event);
                        InputStatus::Delivered
                    }
                    None => InputStatus::Ignored,
                },
                AgentInput::Text { text, .. } => {
                    for c in text.chars() {
                        key(app, KeyCode::Char(c).into());
                        if app.deck.lock_requested || app.deck.power_pending {
                            break;
                        }
                    }
                    InputStatus::Delivered
                }
                _ => InputStatus::Ignored,
            }
        });
    }
}

fn button(id: impl Into<String>, label: &str) -> Node {
    Node::new(id, Role::Button)
        .label(label)
        .action(Action::Activate)
}

fn activate(app: &mut App, id: &str, key: &mut impl FnMut(&mut App, KeyEvent)) -> InputStatus {
    match id {
        "open-settings" => app.open_settings(),
        "open-menu" if app.overlay == Overlay::None => app.open_menu(),
        _ => {
            if let Some((scope, _, _)) = menu(&app.overlay)
                && let Some(label) = id.strip_prefix(scope).and_then(|id| id.strip_prefix('/'))
                && let Some(index) = app.menu_rows().iter().position(|row| row.0 == label)
                && let Overlay::Menu { sel } | Overlay::Sub { sel, .. } = &mut app.overlay
            {
                *sel = index;
                key(app, KeyCode::Enter.into());
            } else if let Some((scope, _, _, accept)) = dialog(&app.overlay)
                && let Some(action) = id.strip_prefix(scope)
            {
                let code = match action {
                    "-accept" => accept,
                    "-cancel" => KeyCode::Esc,
                    _ => return InputStatus::Ignored,
                };
                key(app, code.into());
            } else if let Some(index) = id
                .strip_prefix("row-")
                .and_then(|i| i.parse::<usize>().ok())
                && let Some((_, rows)) = settings(app)
                && rows.get(index).is_some_and(|(_, choosable)| *choosable)
            {
                app.deck.shell.nav.select(index, rows.len());
                app.on_settings_key(Key::Enter);
            } else {
                return InputStatus::Ignored;
            }
        }
    }
    InputStatus::Delivered
}

fn menu(overlay: &Overlay) -> Option<(&'static str, &'static str, usize)> {
    let (id, label, selected) = match overlay {
        Overlay::Menu { sel } => ("menu-main", "Grimoire", sel),
        Overlay::Sub { sub, sel } => match sub {
            Sub::Book => ("menu-book", "This book", sel),
            Sub::Writing => ("menu-writing", "Writing tools", sel),
            Sub::Productivity => ("menu-productivity", "Productivity", sel),
            Sub::Settings => ("menu-settings", "Settings", sel),
            Sub::Help => ("menu-help", "Help & about", sel),
        },
        _ => return None,
    };
    Some((id, label, *selected))
}

// Form values and manuscript names are deliberately absent from this view.
fn dialog(overlay: &Overlay) -> Option<(&'static str, &'static str, &'static str, KeyCode)> {
    Some(match overlay {
        Overlay::NewBook { .. } => (
            "dialog-new-book",
            "Start a new book",
            "Create",
            KeyCode::Enter,
        ),
        Overlay::Create { .. } => (
            "dialog-create",
            "Name the new item",
            "Create",
            KeyCode::Enter,
        ),
        Overlay::Rename { .. } => ("dialog-rename", "Rename an item", "Rename", KeyCode::Enter),
        Overlay::Confirm { .. } => (
            "dialog-delete",
            "Confirm deletion",
            "Delete",
            KeyCode::Char('y'),
        ),
        _ => return None,
    })
}

fn settings(app: &App) -> Option<(String, Vec<(String, bool)>)> {
    (app.overlay == Overlay::Deck(crate::app::wros::Page::Settings)
        && app.deck.shell.nav.current() != Route::BackupForm)
        .then(|| driver_page(app))
}
