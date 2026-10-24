//! Conservative Linux adapter. No shell expansion, sudo, password arguments,
//! manuscript access, or implicit network connection. Requires an active local
//! logind session for authorized actions. Read-only status also works in dev mode.
use crate::{Backend, PowerAction, Reply, Request, Snapshot};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<String>,
}
impl Invocation {
    fn new(program: &'static str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }
}
pub trait Runner: Send {
    fn run(&mut self, cmd: &Invocation) -> Result<String>;
}
#[derive(Default)]
pub struct ProcessRunner;
impl Runner for ProcessRunner {
    fn run(&mut self, cmd: &Invocation) -> Result<String> {
        let mut child = Command::new(cmd.program)
            .args(&cmd.args)
            .env("LC_ALL", "C")
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting {} (is its package installed?)", cmd.program))?;
        // Drain pipes while waiting, with a bounded retained buffer.
        fn drain(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
            let mut kept = Vec::new();
            let mut b = [0; 4096];
            loop {
                let n = pipe.read(&mut b)?;
                if n == 0 {
                    return Ok(kept);
                }
                let room = 65_536usize.saturating_sub(kept.len());
                kept.extend_from_slice(&b[..n.min(room)]);
            }
        }
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        let out = std::thread::spawn(move || drain(out));
        let err = std::thread::spawn(move || drain(err));
        let start = Instant::now();
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait()? {
                break (status, false);
            }
            if start.elapsed() > Duration::from_secs(8) {
                let _ = child.kill();
                break (child.wait()?, true);
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader failed"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader failed"))??;
        if timed_out {
            bail!("{} timed out; no success assumed", cmd.program);
        }
        if !status.success() {
            bail!(
                "{}: {}",
                cmd.program,
                String::from_utf8_lossy(&stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&stdout).trim().to_string())
    }
}

pub struct Linux<R = ProcessRunner> {
    pub runner: R,
    pub sys: PathBuf,
    pub etc: PathBuf,
    pub storage: PathBuf,
}
impl Default for Linux {
    fn default() -> Self {
        Self {
            runner: ProcessRunner,
            sys: "/sys".into(),
            etc: "/etc".into(),
            storage: "/home".into(),
        }
    }
}
impl<R: Runner> Linux<R> {
    pub fn snapshot(&mut self) -> Snapshot {
        let os = fs::read_to_string(self.etc.join("os-release"))
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("PRETTY_NAME=")
                        .map(|s| s.trim_matches('"').to_string())
                })
            })
            .unwrap_or_else(|| "Linux (OS information unavailable)".into());
        let mut s = Snapshot {
            os,
            ..Snapshot::default()
        };
        match self.runner.run(&Invocation::new(
            "/usr/bin/nmcli",
            &["-t", "-f", "STATE,CONNECTIVITY", "general"],
        )) {
            Ok(v) => s.network = Some(v),
            Err(e) => s.warnings.push(format!("NetworkManager: {e}")),
        }
        if let Ok(v) = fs::read_to_string(self.etc.join("timezone")) {
            s.timezone = Some(v.trim().into());
        }
        if let Ok(v) = fs::read_to_string(self.etc.join("writingdeck/keyboard.json")) {
            s.keyboard = serde_json::from_str::<serde_json::Value>(&v)
                .ok()
                .and_then(|v| v.get("layout")?.as_str().map(str::to_string));
        }
        if let Ok(entries) = fs::read_dir(self.sys.join("class/power_supply")) {
            let mut charge = 0u64;
            let mut full = 0u64;
            let mut fallback = vec![];
            let mut charging = false;
            for e in entries.flatten() {
                let p = e.path();
                if read(&p.join("type")).as_deref() != Some("Battery") {
                    continue;
                }
                charging |= read(&p.join("status")).as_deref() == Some("Charging");
                if let (Some(n), Some(f)) = (
                    number(&p.join("energy_now")),
                    number(&p.join("energy_full")),
                ) {
                    charge = charge.saturating_add(n);
                    full = full.saturating_add(f);
                } else if let Some(p) = number(&p.join("capacity")) {
                    fallback.push(p.min(100));
                }
            }
            s.battery_percent = percentage(charge, full).or_else(|| {
                fallback
                    .iter()
                    .sum::<u64>()
                    .checked_div(fallback.len() as u64)
                    .map(|p| p as u8)
            });
            s.charging = s.battery_percent.map(|_| charging);
        }
        if let Ok(entries) = fs::read_dir(self.sys.join("class/backlight")) {
            for e in entries.flatten() {
                let p = e.path();
                if let (Some(n), Some(max)) = (
                    number(&p.join("brightness")),
                    number(&p.join("max_brightness")),
                ) && let Some(percent) = percentage(n, max)
                {
                    s.brightness_percent = Some(percent);
                    break;
                }
            }
        }
        s.available_bytes = available(&self.storage);
        s
    }
}
fn percentage(now: u64, full: u64) -> Option<u8> {
    (u128::from(now) * 100)
        .checked_div(u128::from(full))
        .map(|p| p.min(100) as u8)
}
fn read(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}
fn number(p: &Path) -> Option<u64> {
    read(p)?.parse().ok()
}
#[cfg(unix)]
fn available(p: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    let mut v = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c is NUL-terminated and v is writable. Assume init only on success.
    if unsafe { libc::statvfs(c.as_ptr(), v.as_mut_ptr()) } != 0 {
        return None;
    }
    let v = unsafe { v.assume_init() };
    Some(u64::try_from((v.f_bavail as u128) * (v.f_frsize as u128)).unwrap_or(u64::MAX))
}
#[cfg(not(unix))]
fn available(_: &Path) -> Option<u64> {
    None
}

pub fn invocation(request: &Request) -> Result<Invocation> {
    Ok(match request {
        Request::Refresh => bail!("refresh is a composite read-only operation"),
        Request::Wifi(on) => Invocation::new(
            "/usr/bin/nmcli",
            &["radio", "wifi", if *on { "on" } else { "off" }],
        ),
        Request::ScanWifi => Invocation::new(
            "/usr/bin/nmcli",
            &[
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "yes",
            ],
        ),
        Request::Brightness(p) => {
            if !(1..=100).contains(p) {
                bail!("brightness must be 1..100");
            }
            Invocation::new(
                "/usr/bin/brightnessctl",
                &["--class=backlight", "set", &format!("{p}%")],
            )
        }
        Request::Power(action) => Invocation::new(
            "/usr/bin/systemctl",
            &[
                "--no-ask-password",
                match action {
                    PowerAction::Suspend => "suspend",
                    PowerAction::Shutdown => "poweroff",
                    PowerAction::Restart => "reboot",
                },
            ],
        ),
        Request::PowerSave(on) => Invocation::new(
            "/usr/bin/powerprofilesctl",
            &["set", if *on { "power-saver" } else { "balanced" }],
        ),
        Request::KeyboardPreview(l) => Invocation::new(
            "/usr/bin/pkexec",
            &["/usr/libexec/writingdeck-keyboard", "preview", l.id()],
        ),
        Request::KeyboardConfirm(t) | Request::KeyboardCancel(t) => {
            if t.len() != 32 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("invalid keyboard preview token");
            }
            Invocation::new(
                "/usr/bin/pkexec",
                &[
                    "/usr/libexec/writingdeck-keyboard",
                    if matches!(request, Request::KeyboardConfirm(_)) {
                        "confirm"
                    } else {
                        "cancel"
                    },
                    t,
                ],
            )
        }
    })
}
impl<R: Runner> Backend for Linux<R> {
    fn request(&mut self, request: Request) -> Result<Reply> {
        if request == Request::Refresh {
            return Ok(Reply::Snapshot(self.snapshot()));
        }
        let output = self.runner.run(&invocation(&request)?)?;
        if matches!(request, Request::KeyboardPreview(_)) {
            let v: serde_json::Value =
                serde_json::from_str(&output).context("invalid keyboard helper reply")?;
            let token = v["token"]
                .as_str()
                .context("missing keyboard token")?
                .to_string();
            invocation(&Request::KeyboardConfirm(token.clone()))?;
            let seconds = v["seconds"].as_u64().unwrap_or(15).min(15);
            return Ok(Reply::KeyboardPreview { token, seconds });
        }
        Ok(Reply::Message(if output.is_empty() {
            "request completed".into()
        } else {
            output
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake(Vec<Invocation>);
    impl Runner for Fake {
        fn run(&mut self, c: &Invocation) -> Result<String> {
            self.0.push(c.clone());
            Ok("disconnected:none".into())
        }
    }
    #[test]
    fn percentages_are_bounded_without_zero_division_or_multiply_overflow() {
        assert_eq!(percentage(1, 0), None);
        assert_eq!(percentage(0, 100), Some(0));
        assert_eq!(percentage(51, 100), Some(51));
        assert_eq!(percentage(999, 100), Some(100));
        assert_eq!(percentage(u64::MAX, u64::MAX), Some(100));
        assert_eq!(percentage(u64::MAX / 2, u64::MAX), Some(49));
    }
    #[test]
    fn arguments_cannot_be_shell_commands() {
        assert!(invocation(&Request::KeyboardConfirm(";reboot".into())).is_err());
        assert!(invocation(&Request::Brightness(0)).is_err());
        let c = invocation(&Request::Power(PowerAction::Shutdown)).unwrap();
        assert_eq!(c.program, "/usr/bin/systemctl");
        assert_eq!(c.args, ["--no-ask-password", "poweroff"]);
        assert!(
            invocation(&Request::Wifi(false))
                .unwrap()
                .args
                .contains(&"off".to_string())
        );
    }
    #[test]
    fn missing_hardware_is_unavailable_not_a_fake_battery() {
        let mut b = Linux {
            runner: Fake::default(),
            sys: "/does-not-exist".into(),
            etc: "/does-not-exist".into(),
            storage: "/does-not-exist".into(),
        };
        let s = b.snapshot();
        assert_eq!(s.battery_percent, None);
        assert_eq!(s.brightness_percent, None);
        assert!(!s.simulated);
        assert_eq!(s.available_bytes, None);
    }
    #[test]
    fn real_command_errors_are_not_reported_as_success() {
        let r = ProcessRunner.run(&Invocation::new("/usr/bin/false", &[]));
        assert!(r.is_err());
    }
}
