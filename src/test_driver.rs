//! Test builds only (feature `test-driver`): after each frame the app publishes the
//! Settings page and its rows over a private taria socket, so the screen tests in
//! `tests/driven.rs` choose rows by name instead of counting arrow presses. The
//! socket is bound only when `WROS_TEST_DRIVER_SOCKET` names one.

use crate::app::shell::{Route, driver_page};
use crate::app::{App, Key, Overlay};
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

    /// What is on screen: a way into Settings, the status line, and the open
    /// Settings page's rows.
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
        self.0.publish(nodes);
    }

    /// Applies what the test sent: a row chosen by name, or keys and text routed
    /// as a person's would be, through `key`.
    pub fn drain(
        &self,
        app: &mut App,
        mut key: impl FnMut(&mut App, ratatui::crossterm::event::KeyEvent),
    ) {
        self.0.drain(|input| match input {
            AgentInput::Act { node, .. } if node.0 == "open-settings" => {
                app.open_settings();
                InputStatus::Delivered
            }
            AgentInput::Act { node, .. } => {
                let Some(i) = node.0.strip_prefix("row-").and_then(|i| i.parse().ok()) else {
                    return InputStatus::Ignored;
                };
                let Some((_, rows)) = settings(app) else {
                    return InputStatus::Ignored;
                };
                if !rows.get(i).is_some_and(|(_, choosable)| *choosable) {
                    return InputStatus::Ignored;
                }
                app.deck.shell.nav.select(i, rows.len());
                app.on_settings_key(Key::Enter);
                InputStatus::Delivered
            }
            AgentInput::Key { key: name, .. } => match taria_ratatui::to_crossterm_key(&name) {
                Some(event) => {
                    key(app, event);
                    InputStatus::Delivered
                }
                None => InputStatus::Ignored,
            },
            AgentInput::Text { text, .. } => {
                for c in text.chars() {
                    key(
                        app,
                        ratatui::crossterm::event::KeyEvent::from(
                            ratatui::crossterm::event::KeyCode::Char(c),
                        ),
                    );
                }
                InputStatus::Delivered
            }
            _ => InputStatus::Ignored,
        });
    }
}

fn settings(app: &App) -> Option<(String, Vec<(String, bool)>)> {
    (app.overlay == Overlay::Deck(crate::app::wros::Page::Settings)
        && app.deck.shell.nav.current() != Route::BackupForm)
        .then(|| driver_page(app))
}
