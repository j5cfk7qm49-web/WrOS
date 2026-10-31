//! Pre-authentication keyboard recovery. No PAM messages or account state.
use std::time::{Duration, Instant};

pub const LAYOUTS: [(&str, &str); 4] = [
    ("qwerty", "QWERTY"),
    ("colemak", "Colemak"),
    ("colemak-dh-ansi", "Colemak-DH ANSI"),
    ("colemak-dh-iso", "Colemak-DH ISO"),
];

pub enum Command {
    Preview(usize),
    Confirm(String),
    Cancel(String),
}
impl Command {
    pub fn arguments(&self) -> Result<[&str; 2], &'static str> {
        match self {
            Self::Preview(index) => LAYOUTS
                .get(*index)
                .map(|(id, _)| ["preview", *id])
                .ok_or("Invalid keyboard layout"),
            Self::Confirm(token) | Self::Cancel(token)
                if token.len() == 32 && token.bytes().all(|c| c.is_ascii_hexdigit()) =>
            {
                Ok([
                    if matches!(self, Self::Confirm(_)) {
                        "confirm"
                    } else {
                        "cancel"
                    },
                    token,
                ])
            }
            _ => Err("Invalid keyboard transaction"),
        }
    }
}

pub struct Keyboard {
    pub open: bool,
    selected: usize,
    pub notice: &'static str,
    busy: bool,
    // An uncertain action must be resolved before returning to authentication.
    verified: bool,
    preview: Option<(String, Instant)>,
}
impl Default for Keyboard {
    fn default() -> Self {
        Self {
            open: false,
            selected: 0,
            notice: "Choose a layout. Unconfirmed changes revert automatically",
            busy: false,
            verified: true,
            preview: None,
        }
    }
}
impl Keyboard {
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    pub fn choose(&mut self, direction: isize) {
        if !self.busy && self.preview.is_none() {
            self.selected = self
                .selected
                .saturating_add_signed(direction)
                .min(LAYOUTS.len() - 1);
        }
    }
    fn begin(&mut self, command: Command) -> Option<Command> {
        self.busy = true;
        self.verified = false;
        self.notice = "Applying keyboard action. Please wait";
        Some(command)
    }
    pub fn enter(&mut self, now: Instant) -> Option<Command> {
        if self.busy {
            return None;
        }
        if let Some((token, deadline)) = self.preview.take() {
            self.begin(if now < deadline {
                Command::Confirm(token)
            } else {
                Command::Cancel(token)
            })
        } else {
            self.begin(Command::Preview(self.selected))
        }
    }
    pub fn escape(&mut self) -> Option<Command> {
        if self.busy {
            return None;
        }
        if let Some((token, _)) = self.preview.take() {
            self.begin(Command::Cancel(token))
        } else if self.verified {
            self.open = false;
            None
        } else {
            self.notice = "Layout unverified. Preview and revert, or use maintenance";
            None
        }
    }
    pub fn tick(&mut self, now: Instant) -> Option<Command> {
        if !self.busy
            && self
                .preview
                .as_ref()
                .is_some_and(|(_, deadline)| now >= *deadline)
        {
            let (token, _) = self.preview.take().unwrap();
            self.begin(Command::Cancel(token))
        } else {
            None
        }
    }
    pub fn receive(
        &mut self,
        command: &Command,
        response: Result<&[u8], &'static str>,
        now: Instant,
    ) {
        self.busy = false;
        self.preview = None;
        if let Err(reason) = response {
            self.verified = false;
            self.notice = reason;
            return;
        }
        let parsed = response
            .ok()
            .filter(|bytes| bytes.len() <= 4096)
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
        let success = parsed.is_some_and(|value| match command {
            Command::Preview(_) => {
                if value.get("status").and_then(|v| v.as_str()) == Some("preserved-system-layout")
                    && value.get("preview_available").and_then(|v| v.as_bool()) == Some(false)
                {
                    self.verified = true;
                    self.notice = "System keyboard preserved. Escape returns to login";
                    return true;
                }
                let token = value.get("token").and_then(|v| v.as_str());
                let seconds = value.get("seconds").and_then(|v| v.as_u64());
                if let (Some(token), Some(seconds @ 0..=15)) = (token, seconds)
                    && Command::Cancel(token.to_string()).arguments().is_ok()
                {
                    self.preview = Some((token.into(), now + Duration::from_secs(seconds)));
                    self.notice = "Preview active. Enter keeps this layout; Escape reverts";
                    return true;
                }
                false
            }
            Command::Confirm(_)
                if value.get("status").and_then(|v| v.as_str()) == Some("committed")
                    && value.get("layout").and_then(|v| v.as_str())
                        == Some(LAYOUTS[self.selected].0) =>
            {
                self.verified = true;
                self.open = false;
                self.notice = "Keyboard layout saved";
                true
            }
            Command::Cancel(_)
                if matches!(
                    value.get("status").and_then(|v| v.as_str()),
                    Some("cancelled" | "no-preview")
                ) =>
            {
                self.verified = true;
                self.notice = "Preview ended. Saved layout restored; Escape returns to login";
                true
            }
            _ => false,
        });
        if !success {
            self.verified = false;
            self.notice = "Keyboard action unavailable. Retry a preview or use maintenance";
        }
    }
    pub fn render(&self, frame: &mut ratatui::Frame, now: Instant) {
        let mut rows: Vec<String> = LAYOUTS.iter().map(|(_, label)| (*label).into()).collect();
        if let Some((_, deadline)) = &self.preview {
            rows.push(format!(
                "Revert in {} seconds",
                deadline
                    .saturating_duration_since(now)
                    .as_secs()
                    .saturating_add(1)
            ));
        }
        wros_shell::render_menu(
            frame,
            frame.area(),
            wros_shell::Menu {
                title: "WrOS · Login keyboard",
                rows: &rows,
                selected: self.selected,
                notice: self.notice,
                hint: "Up/Down: layout  Enter: preview/keep  Esc: revert/back  Ctrl+Alt+F2: maintenance",
            },
            wros_shell::Theme::default(),
        );
    }
}

#[cfg(target_os = "linux")]
pub fn execute(command: &Command) -> Result<Vec<u8>, &'static str> {
    use std::{
        io::Read,
        process::{Command as Process, Stdio},
    };
    let arguments = command.arguments()?;
    // No shell, terminal input, password agent, client paths or arbitrary argv.
    // Only the existing narrow helper is eligible for the greeter policy.
    let mut child = Process::new("/usr/bin/pkexec")
        // greetd's locked service account can have SHELL=/usr/sbin/nologin;
        // pkexec rejects inherited shells outside /etc/shells. Supply only
        // fixed helper environment, keeping that account locked and unchanged.
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LANG", "C.UTF-8")
        .args([
            "--disable-internal-agent",
            "/usr/libexec/writingdeck-keyboard",
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Keyboard process unavailable")?;
    let mut output = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("Keyboard response unavailable")?
        .take(4097)
        .read_to_end(&mut output);
    if read.is_err() || output.len() > 4096 {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Invalid keyboard response");
    }
    let status = child.wait().map_err(|_| "Keyboard process failed")?;
    // Exit 3 is the helper's explicit, unchanged-system-layout refusal. Other
    // failures remain uncertain and cannot return the user to authentication.
    if !status.success() && !(status.code() == Some(3) && matches!(command, Command::Preview(_))) {
        return Err(match status.code() {
            Some(126 | 127) => "Keyboard authorization unavailable. Use maintenance",
            _ => "Keyboard helper failed. Use maintenance",
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    fn preview(keyboard: &mut Keyboard, now: Instant) {
        keyboard.open = true;
        let command = keyboard.enter(now).unwrap();
        let reply = format!(r#"{{"token":"{TOKEN}","seconds":3}}"#);
        keyboard.receive(&command, Ok(reply.as_bytes()), now);
    }
    #[test]
    fn expired_enter_cancels_instead_of_committing() {
        let mut keyboard = Keyboard::default();
        let now = Instant::now();
        preview(&mut keyboard, now);
        assert!(matches!(
            keyboard.enter(now + Duration::from_secs(3)),
            Some(Command::Cancel(_))
        ));
    }
    #[test]
    fn preserved_system_keyboard_allows_return_to_login_without_a_preview() {
        let mut keyboard = Keyboard {
            open: true,
            ..Keyboard::default()
        };
        let now = Instant::now();
        let command = keyboard.enter(now).unwrap();
        keyboard.receive(
            &command,
            Ok(br#"{"status":"preserved-system-layout","preview_available":false}"#),
            now,
        );
        assert!(!keyboard.is_busy());
        assert!(keyboard.preview.is_none());
        assert!(keyboard.notice.contains("preserved"));
        assert!(keyboard.escape().is_none());
        assert!(!keyboard.open);
    }
    #[test]
    fn preserved_layout_notice_cannot_acknowledge_an_uncertain_cancel() {
        let mut keyboard = Keyboard::default();
        let now = Instant::now();
        preview(&mut keyboard, now);
        let cancel = keyboard.escape().unwrap();
        keyboard.receive(
            &cancel,
            Ok(br#"{"status":"preserved-system-layout","preview_available":false}"#),
            now,
        );
        keyboard.escape();
        assert!(keyboard.open);
    }
    #[test]
    fn timeout_reverts_and_login_waits_for_acknowledgment() {
        let mut keyboard = Keyboard::default();
        let now = Instant::now();
        preview(&mut keyboard, now);
        let cancel = keyboard.tick(now + Duration::from_secs(3)).unwrap();
        assert!(keyboard.is_busy());
        assert!(keyboard.escape().is_none() && keyboard.open);
        keyboard.receive(
            &cancel,
            Ok(br#"{"status":"no-preview","layout":"qwerty"}"#),
            now,
        );
        keyboard.escape();
        assert!(!keyboard.open);
    }
    #[test]
    fn failure_cannot_return_to_login_until_resolved() {
        let mut keyboard = Keyboard::default();
        let now = Instant::now();
        preview(&mut keyboard, now);
        let cancel = keyboard.escape().unwrap();
        keyboard.receive(&cancel, Err("Keyboard action unavailable"), now);
        keyboard.escape();
        assert!(keyboard.open);
        preview(&mut keyboard, now);
        let cancel = keyboard.escape().unwrap();
        keyboard.receive(&cancel, Ok(br#"{"status":"cancelled"}"#), now);
        keyboard.escape();
        assert!(!keyboard.open);
    }
    #[test]
    fn only_registered_layouts_tokens_and_reply_shapes_are_accepted() {
        assert!(Command::Preview(4).arguments().is_err());
        for token in [
            "../bad",
            "",
            "a; reboot",
            "fffffffffffffffffffffffffffffffz",
        ] {
            assert!(Command::Confirm(token.into()).arguments().is_err());
        }
        for bytes in [
            br#"{"token":"bad","seconds":3}"#.as_slice(),
            br#"{"token":"0123456789abcdef0123456789abcdef","seconds":99}"#,
            b"private error",
        ] {
            let mut keyboard = Keyboard {
                open: true,
                ..Keyboard::default()
            };
            let command = keyboard.enter(Instant::now()).unwrap();
            keyboard.receive(&command, Ok(bytes), Instant::now());
            keyboard.escape();
            assert!(keyboard.open && keyboard.preview.is_none());
            assert!(!keyboard.notice.contains("private error"));
        }
    }
    #[test]
    fn confirmation_must_match_chosen_layout_and_never_reuses_token() {
        let now = Instant::now();
        let mut keyboard = Keyboard::default();
        keyboard.choose(1);
        preview(&mut keyboard, now);
        let confirm = keyboard.enter(now).unwrap();
        keyboard.receive(
            &confirm,
            Ok(br#"{"status":"committed","layout":"qwerty"}"#),
            now,
        );
        assert!(keyboard.open);
        preview(&mut keyboard, now);
        let confirm = keyboard.enter(now).unwrap();
        keyboard.receive(
            &confirm,
            Ok(br#"{"status":"committed","layout":"colemak"}"#),
            now,
        );
        assert!(!keyboard.open && keyboard.preview.is_none());
    }
    #[test]
    fn every_supported_size_renders_preview_and_recovery() {
        for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (12, 4), (1, 1)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            let mut keyboard = Keyboard::default();
            preview(&mut keyboard, Instant::now());
            terminal
                .draw(|frame| keyboard.render(frame, Instant::now()))
                .unwrap();
            if width >= 80 {
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect();
                assert!(text.contains("Colemak-DH ISO") && text.contains("Revert in"));
                assert!(!text.contains(TOKEN));
            }
        }
    }
}
