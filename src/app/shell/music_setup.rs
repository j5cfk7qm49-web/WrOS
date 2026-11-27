use super::{App, Key, Route, music};
use crate::library::{Catalog, Jellyfin, Plex};
use anyhow::Result;
use std::{sync::mpsc, thread};
use zeroize::{Zeroize, Zeroizing};

const MAX_FIELD_BYTES: usize = 1024;

pub(super) struct MusicServerForm {
    server: String,
    // Plex uses a token and has no username field.
    username: Option<String>,
    secret: Zeroizing<String>,
    pub(super) selected: usize,
}

impl MusicServerForm {
    fn fields(&self) -> usize {
        2 + usize::from(self.username.is_some())
    }

    pub(super) fn actions(&self) -> usize {
        self.fields() + 1
    }

    fn field(&mut self) -> Option<&mut String> {
        match self.selected {
            0 => Some(&mut self.server),
            1 if self.username.is_some() => self.username.as_mut(),
            i if i < self.fields() => Some(&mut *self.secret),
            _ => None,
        }
    }

    pub(super) fn rows(&self) -> Vec<String> {
        let mut rows = vec![format!("Server URL · {}", self.server)];
        let hidden = "•".repeat(self.secret.chars().count());
        if let Some(username) = &self.username {
            rows.extend([
                format!("Username · {username}"),
                format!("Password · {hidden}"),
                "Sign in to Jellyfin".into(),
            ]);
        } else {
            rows.extend([
                format!("X-Plex-Token · {hidden}"),
                "Save and check Plex".into(),
                "Plex item → Get Info → View XML. Copy X-Plex-Token from the address bar.".into(),
            ]);
        }
        rows
    }
}

enum SetupResult {
    SignedIn(Jellyfin),
    Checked(String),
}

pub(super) struct MusicSetup {
    pub(super) status: &'static str,
    rx: mpsc::Receiver<Result<SetupResult>>,
}

impl MusicSetup {
    fn spawn(
        status: &'static str,
        work: impl FnOnce() -> Result<SetupResult> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(work());
        });
        Self { status, rx }
    }
}

impl App {
    pub(super) fn open_music_server_form(&mut self) {
        let (default, username) = match self.music.source {
            music::Source::Jellyfin => ("http://localhost:8096", Some(String::new())),
            music::Source::Plex => ("http://localhost:32400", None),
            _ => return,
        };
        let mut server = music::Config::load().server;
        if server.is_empty() {
            server = default.into();
        }
        self.deck.shell.music_server_form = Some(MusicServerForm {
            server,
            username,
            secret: Zeroizing::new(String::new()),
            selected: 0,
        });
        self.deck.shell.nav.open(Route::MusicServerForm);
    }

    pub(crate) fn paste_music_server_form(&mut self, text: &str) {
        if let Some(form) = &mut self.deck.shell.music_server_form
            && let Some(field) = form.field()
            && field.len() + text.len() <= MAX_FIELD_BYTES
            && !text.chars().any(char::is_control)
        {
            field.push_str(text);
        }
    }

    pub(super) fn on_music_server_form_key(&mut self, key: Key) {
        if key == Key::Esc {
            self.deck.shell.music_server_form = None;
            self.deck.shell.music_setup = None;
            self.deck.shell.nav.back();
            return;
        }
        let Some(form) = &mut self.deck.shell.music_server_form else {
            return;
        };
        let count = form.actions();
        match key {
            Key::Home => form.selected = 0,
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % count,
            Key::BackTab | Key::Up => form.selected = (form.selected + count - 1) % count,
            Key::Backspace => {
                if let Some(field) = form.field() {
                    field.pop();
                }
            }
            Key::Delete => {
                if let Some(field) = form.field() {
                    field.zeroize();
                }
            }
            Key::Char(c) if !c.is_control() => {
                if let Some(field) = form.field()
                    && field.len() + c.len_utf8() <= MAX_FIELD_BYTES
                {
                    field.push(c);
                }
            }
            Key::Enter if form.selected < form.fields() => form.selected += 1,
            Key::Enter => {
                if form.server.trim().is_empty()
                    || form.username.as_ref().is_some_and(|u| u.trim().is_empty())
                    || form.secret.is_empty()
                {
                    self.deck
                        .shell
                        .notices
                        .push("Fill in each field before confirming.");
                    return;
                }
                if let Some(form) = self.deck.shell.music_server_form.take() {
                    self.start_music_setup(form);
                }
            }
            _ => {}
        }
    }

    fn start_music_setup(&mut self, form: MusicServerForm) {
        let MusicServerForm {
            server,
            username,
            secret,
            ..
        } = form;
        let server = server.trim().to_string();
        if let Some(username) = username {
            let job = MusicSetup::spawn("Signing in to Jellyfin…", move || {
                let login = Jellyfin::login(&server, username.trim(), &secret);
                drop(secret);
                let (token, user_id) = login?;
                Ok(SetupResult::SignedIn(Jellyfin {
                    server,
                    token,
                    user_id,
                }))
            });
            self.deck.shell.music_setup = Some(job);
        } else if let Err(e) = self.save_music_server(
            music::Source::Plex,
            server,
            secret.trim().to_string(),
            String::new(),
        ) {
            self.finish_music_setup(Err(e));
        }
    }

    fn save_music_server(
        &mut self,
        source: music::Source,
        server: String,
        token: String,
        user_id: String,
    ) -> Result<()> {
        let mut cfg = music::Config::load();
        cfg.source = source;
        cfg.server = server;
        cfg.api_key = token;
        cfg.user_id = user_id;
        cfg.save()?;

        let catalog: Box<dyn Catalog> = match source {
            music::Source::Jellyfin => Box::new(Jellyfin {
                server: cfg.server.clone(),
                token: cfg.api_key.clone(),
                user_id: cfg.user_id.clone(),
            }),
            music::Source::Plex => Box::new(Plex {
                server: cfg.server.clone(),
                token: cfg.api_key.clone(),
            }),
            _ => unreachable!(),
        };
        self.music = music::Music::spawn(cfg);
        let job = MusicSetup::spawn("Checking the music library…", move || {
            let tracks = catalog.fetch(20)?;
            let prefix = if source == music::Source::Jellyfin {
                "Signed in. "
            } else {
                ""
            };
            let example = tracks
                .first()
                .map(|t| format!(", e.g. {} — {}", t.title, t.artist))
                .unwrap_or_default();
            Ok(SetupResult::Checked(format!(
                "{prefix}Found {} tracks{example}.",
                tracks.len()
            )))
        });
        self.deck.shell.music_setup = Some(job);
        Ok(())
    }

    pub(in crate::app) fn tick_music_setup(&mut self) {
        let Some(job) = &self.deck.shell.music_setup else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!("music setup stopped")),
        };
        match result {
            Ok(SetupResult::SignedIn(Jellyfin {
                server,
                token,
                user_id,
            })) => {
                if let Err(e) =
                    self.save_music_server(music::Source::Jellyfin, server, token, user_id)
                {
                    self.finish_music_setup(Err(e));
                }
            }
            Ok(SetupResult::Checked(message)) => self.finish_music_setup(Ok(message)),
            Err(e) => self.finish_music_setup(Err(e)),
        }
    }

    fn finish_music_setup(&mut self, result: Result<String>) {
        self.deck.shell.music_setup = None;
        self.deck.shell.nav.back();
        let message = match result {
            Ok(message) => message,
            Err(e) => format!("Music setup failed · {e}"),
        };
        self.deck.shell.notices.push(&message);
    }
}
