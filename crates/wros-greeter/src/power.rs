//! Shut down or restart from the login screen, as Lemurs offers. No account,
//! PAM or session state: the same fixed `systemctl` requests as Settings, which
//! logind authorizes for the active local login screen by its own policy.
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Shutdown,
    Restart,
}
impl Action {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Shutdown => "poweroff",
            Self::Restart => "reboot",
        }
    }
    /// Fixed refusal text: systemctl's own error output is never shown or logged.
    pub fn refusal(self) -> &'static str {
        match self {
            Self::Shutdown => "Shutdown was refused. Use the maintenance console",
            Self::Restart => "Restart was refused. Use the maintenance console",
        }
    }
    /// The journal line as the request is made.
    pub fn logged(self) -> &'static std::ffi::CStr {
        match self {
            Self::Shutdown => c"shutdown requested from the login screen",
            Self::Restart => c"restart requested from the login screen",
        }
    }
    fn progress(self) -> &'static str {
        match self {
            Self::Shutdown => "Shutting down",
            Self::Restart => "Restarting",
        }
    }
}

/// Back to login comes first and is selected whenever the menu opens.
pub const ROWS: [&str; 3] = ["Back to login", "Shut down", "Restart"];
const CHOOSE: &str = "Shut down or restart this laptop, or go back to login";
/// How long `systemctl` may take to hand the request to logind.
pub const DEADLINE: Duration = Duration::from_secs(25);

#[derive(Default)]
pub struct Power {
    pub open: bool,
    selected: usize,
    pub notice: &'static str,
    pending: Option<Action>,
    accepted: Option<Action>,
}
impl Power {
    pub fn open(&mut self) {
        if self.accepted.is_none() {
            self.open = true;
            self.selected = 0;
            self.notice = CHOOSE;
        }
    }
    /// A request is with systemd, or systemd accepted one.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || self.accepted.is_some()
    }
    /// The accepted request: the login screen is expected to be stopped.
    pub fn accepted(&self) -> Option<Action> {
        self.accepted
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn choose(&mut self, direction: isize) {
        if !self.is_busy() {
            self.selected = self
                .selected
                .saturating_add_signed(direction)
                .min(ROWS.len() - 1);
        }
    }
    pub fn enter(&mut self) -> Option<Action> {
        if self.is_busy() {
            return None;
        }
        let action = match self.selected {
            1 => Action::Shutdown,
            2 => Action::Restart,
            _ => {
                self.open = false;
                return None;
            }
        };
        self.pending = Some(action);
        self.notice = action.progress();
        Some(action)
    }
    pub fn escape(&mut self) {
        if !self.is_busy() {
            self.open = false;
        }
    }
    pub fn receive(&mut self, action: Action, result: Result<(), &'static str>) {
        self.pending = None;
        match result {
            Ok(()) => {
                self.accepted = Some(action);
                self.notice = action.progress();
            }
            Err(reason) => self.notice = reason,
        }
    }
    pub fn render(&self, frame: &mut ratatui::Frame, theme: wros_shell::Theme) {
        let rows: Vec<String> = ROWS.iter().map(|row| (*row).into()).collect();
        wros_shell::render_menu(
            frame,
            frame.area(),
            wros_shell::Menu {
                title: "WrOS · Power",
                rows: &rows,
                kinds: &[],
                selected: self.selected,
                notice: self.notice,
                hint: if self.is_busy() {
                    "Please wait  Ctrl+Alt+F2: maintenance"
                } else {
                    "Up/Down: choose  Enter: confirm  Esc: back to login  Ctrl+Alt+F2: maintenance"
                },
            },
            theme,
        );
    }
}

#[cfg(target_os = "linux")]
pub fn execute(action: Action) -> Result<(), &'static str> {
    use std::process::{Command, Stdio};
    use std::time::Instant;
    // No shell, terminal, password agent or inherited environment. systemctl
    // returns once logind has accepted the request; services stop afterwards.
    let mut child = Command::new("/usr/bin/systemctl")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LANG", "C.UTF-8")
        .args(["--no-ask-password", action.verb()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| action.refusal())?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(action.refusal()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("No answer from the system. Use the maintenance console");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(power: &Power, width: u16, height: u16) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| power.render(frame, wros_shell::Theme::default()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn the_menu_opens_on_back_to_login_and_enter_there_requests_nothing() {
        let mut power = Power::default();
        power.open();
        power.choose(1);
        power.escape();
        power.open();
        assert_eq!(power.selected(), 0);
        assert_eq!(power.enter(), None);
        assert!(!power.open && !power.is_busy());
    }

    #[test]
    fn each_row_requests_its_own_fixed_action() {
        for (steps, action, verb) in [
            (1, Action::Shutdown, "poweroff"),
            (2, Action::Restart, "reboot"),
        ] {
            let mut power = Power::default();
            power.open();
            power.choose(steps);
            assert_eq!(power.enter(), Some(action));
            assert_eq!(action.verb(), verb);
        }
    }

    #[test]
    fn choosing_stops_at_either_end() {
        let mut power = Power::default();
        power.open();
        power.choose(-1);
        assert_eq!(power.selected(), 0);
        power.choose(9);
        assert_eq!(power.selected(), ROWS.len() - 1);
    }

    #[test]
    fn a_pending_or_accepted_request_holds_the_menu_open() {
        let mut power = Power::default();
        power.open();
        power.choose(1);
        let action = power.enter().unwrap();
        power.choose(1);
        power.escape();
        assert!(power.open && power.enter().is_none());
        power.receive(action, Ok(()));
        power.escape();
        power.open();
        assert!(power.open && power.is_busy());
        assert_eq!(power.accepted(), Some(Action::Shutdown));
        assert_eq!(power.selected(), 1);
    }

    #[test]
    fn a_refusal_shows_fixed_text_and_returns_to_choosing() {
        let mut power = Power::default();
        power.open();
        power.choose(2);
        let action = power.enter().unwrap();
        power.receive(action, Err(action.refusal()));
        assert!(!power.is_busy() && power.accepted().is_none());
        assert_eq!(
            power.notice,
            "Restart was refused. Use the maintenance console"
        );
        power.escape();
        assert!(!power.open);
    }

    #[test]
    fn every_supported_size_shows_the_whole_menu() {
        let mut power = Power::default();
        power.open();
        for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (12, 4), (1, 1)] {
            let text = screen(&power, width, height);
            if width >= 80 {
                for row in ROWS {
                    assert!(text.contains(row), "{row} at {width}x{height}");
                }
                assert!(text.contains("Esc: back to login  Ctrl+Alt+F2: maintenance"));
                assert!(text.contains("or go back to login"));
            }
        }
    }
}
