use super::Deck;
use crate::{app::App, theme};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) const LIVE_INSTALL_CHOICES: [&str; 2] = ["Install WrOS to disk", "Try it first"];

impl Deck {
    pub(super) fn take_live_install_prompt(&mut self) -> bool {
        if !self.live_session || self.live_install_prompt_shown {
            return false;
        }
        self.live_install_prompt_shown = true;
        // Remember the prompt across app restarts while the runtime directory exists.
        // Without a writable runtime directory, remember it in this app session.
        let already_shown = self.enable_system
            && std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|dir| !dir.is_empty())
                .is_some_and(|dir| {
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(PathBuf::from(dir).join("wros-live-install-prompt"))
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists)
                });
        !already_shown
    }
}

impl App {
    pub(crate) fn sync_boot_theme_if_changed(&mut self, path: &Path) {
        if self.deck.boot_settings_available
            && fs::read_to_string(path).ok().as_deref() != Some(&theme::to_config(&self.theme))
        {
            self.sync_boot_theme();
        }
    }

    pub(crate) fn sync_boot_theme(&mut self) {
        if self.deck.boot_settings_available {
            self.deck.pending_boot_theme = Some(theme::to_config(&self.theme));
        }
    }

    pub(in crate::app) fn toggle_graphical_session(&mut self) {
        let Some(settings) = self.deck.session_settings.clone() else {
            return;
        };
        let on = !self.deck.graphical_chosen;
        let message = match crate::session::choose(&settings, on) {
            Ok(()) => {
                self.deck.graphical_chosen = on;
                if on {
                    "Graphical session on. It starts at your next login."
                } else {
                    "Graphical session off. The console session starts at your next login."
                }
                .to_string()
            }
            Err(error) => format!("Couldn't save the session choice: {error}"),
        };
        self.deck.note = message;
        self.deck.shell.notices.push(&self.deck.note);
    }

    pub(crate) fn request_live_installer(&mut self) {
        if !self.deck.live_session {
            self.deck
                .shell
                .notices
                .push("The live installer runs only from a WrOS live session.");
            return;
        }
        self.deck.shell.backup.tick();
        if !self.deck.installer_available || self.deck.busy || self.deck.shell.backup.job.is_some()
        {
            self.deck
                .shell
                .notices
                .push("Live installer is unavailable or a system/backup operation is pending.");
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck
                .shell
                .notices
                .push("Installer not started: unsaved buffers or source conflicts need attention.");
            return;
        }
        self.save_deck();
        self.deck.installer_requested = true;
    }
}
