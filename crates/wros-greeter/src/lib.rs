//! Independent greetd conversation and bounded wire protocol. No editor state,
//! password verification, command selection, or persistent credentials.
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub mod form;
pub mod keyboard;

pub const MAX_FRAME: usize = 16 * 1024;
pub const MAX_INPUT: usize = 1024;
pub const SESSION: &str = "/usr/libexec/wros-session";

/// The session the login starts, from the form's switcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    /// The writer's own choice from Settings: graphical or console.
    Usual,
    /// The console, for this login only.
    Console,
}
impl Session {
    pub fn title(self) -> &'static str {
        match self {
            Self::Usual => "Usual session",
            Self::Console => "Console session",
        }
    }
    fn command(self) -> Vec<String> {
        match self {
            Self::Usual => vec![SESSION.into()],
            Self::Console => vec![SESSION.into(), "--console".into()],
        }
    }
}

/// A local account name: letters, digits, `_`, `-` and `.`, at most 64 bytes.
fn valid_account(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
}

// Implement the documented JSON protocol independently; do not link the GPL
// greetd_ipc crate into the MIT editor/shared presentation library.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    CreateSession { username: String },
    PostAuthMessageResponse { response: Option<String> },
    StartSession { cmd: Vec<String>, env: Vec<String> },
    CancelSession,
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Self::PostAuthMessageResponse {
            response: Some(text),
        } = self
        {
            text.zeroize();
        }
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Secret,
    Visible,
    Info,
    Error,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Success,
    Error {
        error_type: String,
        description: String,
    },
    AuthMessage {
        auth_message_type: MessageKind,
        auth_message: String,
    },
}

pub fn encode(request: &Request) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    let mut body = Zeroizing::new(Vec::with_capacity(MAX_FRAME));
    serde_json::to_writer(&mut *body, request).map_err(|_| "Invalid login request")?;
    if body.len() > MAX_FRAME {
        return Err("Login request too large");
    }
    Ok(body)
}

pub fn decode(bytes: &[u8]) -> Result<Response, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("Invalid login frame size");
    }
    // Never expose serde errors: they can contain strings from the wire.
    serde_json::from_slice(bytes).map_err(|_| "Invalid login service response")
}

pub fn display_text(text: &str) -> String {
    text.chars()
        .take(512)
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Local account names only; no shadow/password database or network NSS lookup.
/// PAM still decides whether a listed account can authenticate.
pub fn local_accounts(passwd: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in passwd.lines() {
        let fields: Vec<_> = line.split(':').collect();
        if fields.len() != 7 {
            continue;
        }
        let Ok(uid) = fields[2].parse::<u32>() else {
            continue;
        };
        let name = fields[0];
        if (1000..65534).contains(&uid)
            && !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
            && fields[6].starts_with('/')
            && !fields[6].ends_with("/nologin")
            && !fields[6].ends_with("/false")
            && !names.iter().any(|known| known == name)
        {
            names.push(name.to_string());
            if names.len() == 32 {
                break;
            }
        }
    }
    names.sort();
    names
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Create,
    Answer,
    Start,
    Cancel,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Phase {
    /// The form is open.
    Username,
    Waiting(Pending),
    /// PAM asked something the form did not answer (a second question, a notice).
    Prompt(MessageKind),
    Retry,
    Restart,
    Done,
}

pub struct Conversation {
    pub phase: Phase,
    pub prompt: String,
    pub notice: &'static str,
    // Fixed capacity prevents reallocation of typed secrets; no Debug/Clone.
    input: Zeroizing<String>,
    /// The form's password, kept only until PAM's first password question.
    secret: Zeroizing<String>,
    session: Session,
}
impl Default for Conversation {
    fn default() -> Self {
        Self {
            phase: Phase::Username,
            prompt: "Local account".into(),
            notice: "Offline login",
            input: Zeroizing::new(String::with_capacity(MAX_INPUT)),
            secret: Zeroizing::new(String::with_capacity(MAX_INPUT)),
            session: Session::Usual,
        }
    }
}
impl Conversation {
    pub fn open_keyboard(&mut self, keyboard: &mut keyboard::Keyboard) -> bool {
        if self.phase != Phase::Username {
            return false;
        }
        self.clear();
        keyboard.open = true;
        true
    }
    /// Starts the login with what the form collected. The password goes to PAM
    /// when it asks for one; the session starts as soon as PAM accepts.
    pub fn begin(&mut self, username: &str, password: &str, session: Session) -> Option<Request> {
        if self.phase != Phase::Username {
            return None;
        }
        if !valid_account(username) {
            self.notice = "Enter a valid local account name";
            return None;
        }
        self.clear();
        if password.len() <= MAX_INPUT {
            self.secret.push_str(password);
        }
        self.session = session;
        self.phase = Phase::Waiting(Pending::Create);
        self.prompt = "Please wait".into();
        self.notice = "Signing in";
        Some(Request::CreateSession {
            username: username.into(),
        })
    }
    pub fn type_char(&mut self, c: char) {
        if !c.is_control()
            && self.input.len() + c.len_utf8() <= MAX_INPUT
            && matches!(
                self.phase,
                Phase::Prompt(MessageKind::Secret | MessageKind::Visible)
            )
        {
            self.input.push(c);
        }
    }
    pub fn backspace(&mut self) {
        if let Some((index, _)) = self.input.char_indices().next_back() {
            // Wipe removed bytes before truncating so they cannot remain in capacity.
            unsafe {
                self.input.as_bytes_mut()[index..].zeroize();
            }
            self.input.truncate(index);
        }
    }
    pub fn clear(&mut self) {
        self.input.zeroize();
        self.secret.zeroize();
    }
    pub fn shown_input(&self) -> String {
        if matches!(self.phase, Phase::Prompt(MessageKind::Secret)) {
            // Constant display: neither text nor password length is rendered.
            "[hidden]".into()
        } else {
            display_text(&self.input)
        }
    }
    pub fn submit(&mut self) -> Option<Request> {
        let request = match self.phase {
            Phase::Prompt(kind) => {
                let response = match kind {
                    MessageKind::Secret | MessageKind::Visible => Some(self.input.to_string()),
                    MessageKind::Info | MessageKind::Error => None,
                };
                self.phase = Phase::Waiting(Pending::Answer);
                Request::PostAuthMessageResponse { response }
            }
            Phase::Retry => {
                self.clear();
                self.phase = Phase::Restart;
                return None;
            }
            _ => return None,
        };
        self.clear();
        self.prompt = "Please wait".into();
        Some(request)
    }
    pub fn cancel(&mut self) -> Option<Request> {
        self.clear();
        match self.phase {
            Phase::Prompt(_) => {
                self.phase = Phase::Waiting(Pending::Cancel);
                self.prompt = "Cancelling login".into();
                self.notice = "Login cancelled";
                Some(Request::CancelSession)
            }
            _ => None,
        }
    }
    pub fn receive(&mut self, response: Response) -> Result<Option<Request>, &'static str> {
        self.input.zeroize();
        let Phase::Waiting(pending) = self.phase else {
            self.secret.zeroize();
            return Err("Unexpected login response");
        };
        // The form's password answers PAM's first password question, and only that.
        if let Response::AuthMessage {
            auth_message_type: MessageKind::Secret,
            ..
        } = response
            && pending == Pending::Create
            && !self.secret.is_empty()
        {
            let answer = Some(self.secret.to_string());
            self.secret.zeroize();
            self.phase = Phase::Waiting(Pending::Answer);
            return Ok(Some(Request::PostAuthMessageResponse { response: answer }));
        }
        self.secret.zeroize();
        match response {
            Response::Success => match pending {
                Pending::Create | Pending::Answer => {
                    self.phase = Phase::Waiting(Pending::Start);
                    self.prompt = "Opening your writing session".into();
                    self.notice = "Signed in";
                    return Ok(Some(Request::StartSession {
                        cmd: self.session.command(),
                        env: vec!["TERM=linux".into(), "LANG=C.UTF-8".into()],
                    }));
                }
                Pending::Start => self.phase = Phase::Done,
                Pending::Cancel => {
                    self.phase = Phase::Username;
                    self.prompt = "Local account".into();
                }
            },
            Response::AuthMessage {
                auth_message_type,
                auth_message,
            } if matches!(pending, Pending::Create | Pending::Answer) => {
                self.phase = Phase::Prompt(auth_message_type);
                self.prompt = display_text(&auth_message);
                self.notice = "Enter to respond; Escape to cancel";
            }
            Response::Error { error_type, .. }
                if error_type == "auth_error"
                    && matches!(pending, Pending::Create | Pending::Answer) =>
            {
                self.notice = "Login was not accepted. Try again";
                self.phase = Phase::Waiting(Pending::Cancel);
                self.prompt = "Resetting login".into();
                return Ok(Some(Request::CancelSession));
            }
            Response::Error { .. } if pending == Pending::Cancel => {
                // Debian greetd may report the failed PAM worker again when
                // cancelling it. Do not reinterpret that as a successful reset
                // or reuse uncertain state. Exit only after acknowledgment;
                // greetd/systemd will start a completely new login service.
                self.phase = Phase::Retry;
                self.prompt = "Enter to restart login".into();
            }
            Response::Error { .. } => {
                return Err(match pending {
                    Pending::Create => "Login service refused session creation",
                    Pending::Answer => "Login service failed the authentication exchange",
                    Pending::Cancel => "Login service could not acknowledge cancellation",
                    Pending::Start => "Login service refused session startup",
                });
            }
            _ => return Err("Unexpected login service message"),
        }
        Ok(None)
    }
}

/// The Lemurs form while the writer signs in; PAM's own questions, if it asks
/// any the form did not answer, in the WrOS menu.
pub fn render(frame: &mut ratatui::Frame, conversation: &Conversation, form: &form::Form) {
    render_themed(
        frame,
        conversation,
        form,
        &wros_shell::grimoire_theme::default_theme(),
    );
}

pub fn render_themed(
    frame: &mut ratatui::Frame,
    conversation: &Conversation,
    form: &form::Form,
    palette: &wros_shell::grimoire_theme::Theme,
) {
    use ratatui::layout::{Constraint, Layout, Rect};
    use wros_shell::logo::Logo;
    let theme = palette.into();
    // Leave the complete Lemurs form visible even on a 25-line console.
    let area = frame.area();
    let height = Logo::height_for(Rect {
        height: area.height.saturating_sub(14),
        ..area
    });
    let [logo, body] =
        Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(area);
    frame.render_widget(Logo { theme: palette }, logo);
    let title = "WrOS · Sign in";
    if matches!(conversation.phase, Phase::Prompt(_) | Phase::Retry) {
        let rows = vec![conversation.prompt.clone(), conversation.shown_input()];
        wros_shell::render_menu(
            frame,
            body,
            wros_shell::Menu {
                title,
                rows: &rows,
                kinds: &[],
                selected: 0,
                notice: conversation.notice,
                hint: "Enter: continue  Esc: cancel for account/keyboard recovery  Ctrl+Alt+F2: maintenance",
            },
            theme,
        );
        return;
    }
    let hint = match form.focus {
        form::Focus::Switcher => {
            "Left/Right: session  Enter: continue  F2: keyboard  Ctrl+Alt+F2: maintenance"
        }
        form::Focus::Username => {
            "Enter: continue  Up/Down: accounts  F2: keyboard  Ctrl+Alt+F2: maintenance"
        }
        form::Focus::Password => "Enter: sign in  Up: back  F2: keyboard  Ctrl+Alt+F2: maintenance",
    };
    let body = wros_shell::render_panel(frame, body, title, conversation.notice, hint, theme);
    form.render(frame, body, theme);
}

#[cfg(target_os = "linux")]
pub mod transport;

#[cfg(test)]
mod tests;
