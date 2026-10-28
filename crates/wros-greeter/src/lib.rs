//! Independent greetd conversation and bounded wire protocol. No editor state,
//! password verification, command selection, or persistent credentials.
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub mod keyboard;

pub const MAX_FRAME: usize = 16 * 1024;
pub const MAX_INPUT: usize = 1024;
pub const SESSION: &str = "/usr/libexec/writingdeck-session";

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
    Username,
    Waiting(Pending),
    Prompt(MessageKind),
    Authenticated,
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
}
impl Default for Conversation {
    fn default() -> Self {
        Self {
            phase: Phase::Username,
            prompt: "Local account".into(),
            notice: "Offline login",
            input: Zeroizing::new(String::with_capacity(MAX_INPUT)),
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
    pub fn choose_account(&mut self, name: &str) {
        if self.phase == Phase::Username {
            self.clear();
            for c in name.chars().take(64) {
                self.type_char(c);
            }
        }
    }
    pub fn type_char(&mut self, c: char) {
        if !c.is_control()
            && self.input.len() + c.len_utf8() <= MAX_INPUT
            && matches!(
                self.phase,
                Phase::Username | Phase::Prompt(MessageKind::Secret | MessageKind::Visible)
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
            Phase::Username => {
                let name = self.input.as_str();
                if name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
                {
                    self.notice = "Enter a valid local account name";
                    return None;
                }
                self.phase = Phase::Waiting(Pending::Create);
                Request::CreateSession {
                    username: name.into(),
                }
            }
            Phase::Prompt(kind) => {
                let response = match kind {
                    MessageKind::Secret | MessageKind::Visible => Some(self.input.to_string()),
                    MessageKind::Info | MessageKind::Error => None,
                };
                self.phase = Phase::Waiting(Pending::Answer);
                Request::PostAuthMessageResponse { response }
            }
            Phase::Authenticated => {
                self.phase = Phase::Waiting(Pending::Start);
                Request::StartSession {
                    cmd: vec![SESSION.into()],
                    env: vec!["TERM=linux".into(), "LANG=C.UTF-8".into()],
                }
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
            Phase::Prompt(_) | Phase::Authenticated => {
                self.phase = Phase::Waiting(Pending::Cancel);
                self.prompt = "Cancelling login".into();
                self.notice = "Login cancelled";
                Some(Request::CancelSession)
            }
            _ => None,
        }
    }
    pub fn receive(&mut self, response: Response) -> Result<Option<Request>, &'static str> {
        self.clear();
        let Phase::Waiting(pending) = self.phase else {
            return Err("Unexpected login response");
        };
        match response {
            Response::Success => match pending {
                Pending::Create | Pending::Answer => {
                    self.phase = Phase::Authenticated;
                    self.prompt = "Open writing session".into();
                    self.notice = "Authenticated. Enter to continue; Escape to cancel";
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

pub fn render(frame: &mut ratatui::Frame, conversation: &Conversation) {
    let rows = vec![conversation.prompt.clone(), conversation.shown_input()];
    wros_shell::render_menu(
        frame,
        frame.area(),
        wros_shell::Menu {
            title: "WrOS · Sign in",
            rows: &rows,
            selected: 0,
            notice: conversation.notice,
            hint: if conversation.phase == Phase::Username {
                "Enter: continue  F2: keyboard  Up/Down: accounts  Ctrl+Alt+F2: maintenance"
            } else {
                "Enter: continue  Esc: cancel for account/keyboard recovery  Ctrl+Alt+F2: maintenance"
            },
        },
        wros_shell::Theme::default(),
    );
}

#[cfg(target_os = "linux")]
pub mod transport;

#[cfg(test)]
mod tests;
