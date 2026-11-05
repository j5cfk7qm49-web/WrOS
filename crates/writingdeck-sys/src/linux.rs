//! Conservative Linux adapter. No shell expansion, sudo, password arguments,
//! manuscript access, or implicit network connection. Requires an active local
//! logind session for authorized actions. Read-only status also works in dev mode.
use crate::{Backend, PowerAction, Reply, Request, Snapshot};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Longest the runner waits for each privileged update verb. Staging verifies
/// and copies a whole bundle, so it is far longer than a display preview.
pub const REVIEW_TIMEOUT_SECS: u64 = 180;
pub const STAGE_TIMEOUT_SECS: u64 = 900;
pub const REPAIR_TIMEOUT_SECS: u64 = 900;
pub const CANCEL_TIMEOUT_SECS: u64 = 60;
pub const APPLY_TIMEOUT_SECS: u64 = 900;
pub const MEDIA_TIMEOUT_SECS: u64 = 60;
pub const ONLINE_CHECK_TIMEOUT_SECS: u64 = 30;
/// Downloading a whole update over a slow connection.
pub const ONLINE_FETCH_TIMEOUT_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<String>,
    pub timeout: Duration,
}
impl Invocation {
    fn new(program: &'static str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
            timeout: Duration::from_secs(match program {
                "/usr/libexec/wros-update" => 60,
                "/usr/bin/nmcli" => 20,
                _ => 8,
            }),
        }
    }
    fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout = Duration::from_secs(seconds);
        self
    }
    /// Fixed argv for a narrow privileged WrOS update launcher. pkexec never
    /// starts its own text agent, so a missing polkit rule fails instead of
    /// prompting on the writer's terminal.
    fn update_launcher(helper: &'static str, args: &[&str], timeout_secs: u64) -> Self {
        let mut argv = vec!["--disable-internal-agent", helper];
        argv.extend_from_slice(args);
        Self::new("/usr/bin/pkexec", &argv).with_timeout(timeout_secs)
    }
}

/// Bounded text of a failed helper: stderr first, then the `message` of a JSON
/// reply on stdout, then the bare exit status.
fn failure_detail(stdout: &[u8], stderr: &[u8], status: std::process::ExitStatus) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    if let Ok(serde_json::Value::Object(object)) = serde_json::from_slice(stdout)
        && let Some(message) = object.get("message").and_then(serde_json::Value::as_str)
        && !message.is_empty()
        && message.len() <= 1024
    {
        return message.chars().filter(|c| !c.is_control()).collect();
    }
    format!("exited with {status}")
}
pub trait Runner: Send {
    fn run(&mut self, cmd: &Invocation) -> Result<String>;
    fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
        if !input.is_empty() {
            bail!("this runner does not support protected input");
        }
        self.run(cmd)
    }
}
#[derive(Default)]
pub struct ProcessRunner;
impl Runner for ProcessRunner {
    fn run(&mut self, cmd: &Invocation) -> Result<String> {
        self.run_with_input(cmd, &[])
    }
    fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
        let mut child = Command::new(cmd.program)
            .args(&cmd.args)
            .env("LC_ALL", "C")
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .stdin(if input.is_empty() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting {} (is its package installed?)", cmd.program))?;
        if !input.is_empty()
            && let Some(mut stdin) = child.stdin.take()
            && let Err(err) = stdin.write_all(input).and_then(|()| stdin.flush())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            let _ = child.kill();
            let _ = child.wait();
            bail!("writing protected input to {} failed", cmd.program);
        }
        // Drain pipes while waiting, with a bounded retained buffer.
        fn drain(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
            let mut kept = Vec::new();
            let mut b = [0; 4096];
            loop {
                let n = pipe.read(&mut b)?;
                if n == 0 {
                    return Ok(kept);
                }
                let room = 65_537usize.saturating_sub(kept.len());
                kept.extend_from_slice(&b[..n.min(room)]);
            }
        }
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        let out = std::thread::spawn(move || drain(out));
        let err = std::thread::spawn(move || drain(err));
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if start.elapsed() > cmd.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let Some(status) = status else {
            // Do not join the pipe readers: a helper child that outlives the
            // killed launcher would hold the pipes open and hang the worker.
            // The detached readers end when the pipes close.
            bail!("{} timed out; no success assumed", cmd.program);
        };
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader failed"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader failed"))??;
        if stdout.len() > 65_536 || stderr.len() > 65_536 {
            bail!(
                "{} output exceeds limit; no partial result accepted",
                cmd.program
            );
        }
        if !status.success() {
            bail!(
                "{}: {}",
                cmd.program,
                failure_detail(&stdout, &stderr, status)
            );
        }
        Ok(String::from_utf8(stdout)
            .context("system reply is not UTF-8")?
            .trim_end_matches(['\r', '\n'])
            .to_string())
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
        // timedated changes /etc/localtime; /etc/timezone can be stale.
        match self.runner.run(&timezone_status()) {
            Ok(zone) if crate::timezone::validate(&zone).is_ok() => s.timezone = Some(zone),
            _ => s.warnings.push("Timezone service unavailable".into()),
        }
        // A failed read leaves both unknown rather than claiming either state.
        if let Ok(text) = self.runner.run(&Invocation::new(
            "/usr/bin/timedatectl",
            &[
                "--no-ask-password",
                "--no-pager",
                "show",
                "--property=NTP",
                "--property=NTPSynchronized",
            ],
        )) {
            (s.network_time, s.time_synchronized) = parse_time_sync(&text);
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
/// `NTP=` and `NTPSynchronized=` from `timedatectl show`; anything but yes/no is unknown.
fn parse_time_sync(text: &str) -> (Option<bool>, Option<bool>) {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .and_then(|v| match v.trim() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            })
    };
    (value("NTP"), value("NTPSynchronized"))
}

fn timezone_status() -> Invocation {
    Invocation::new(
        "/usr/bin/timedatectl",
        &[
            "--no-ask-password",
            "--no-pager",
            "--value",
            "--property=Timezone",
            "show",
        ],
    )
}
fn active_wifi_connections() -> Invocation {
    Invocation::new(
        "/usr/bin/nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "--escape",
            "yes",
            "-f",
            "UUID,NAME,TYPE",
            "connection",
            "show",
            "--active",
        ],
    )
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
        Request::ListTimezones => Invocation::new(
            "/usr/bin/timedatectl",
            &["--no-ask-password", "--no-pager", "list-timezones"],
        ),
        Request::SetTimezone(zone) => {
            crate::timezone::validate(zone)?;
            Invocation::new(
                "/usr/bin/timedatectl",
                &["--no-ask-password", "--no-pager", "set-timezone", zone],
            )
        }
        Request::SetLocalClock(ts) => {
            let normalized = crate::timezone::validate_local_datetime(ts)?;
            Invocation::new(
                "/usr/bin/pkexec",
                &[
                    "--disable-internal-agent",
                    "/usr/libexec/wros-clock",
                    "set-time",
                    &normalized,
                ],
            )
        }
        Request::UseNetworkTime => Invocation::new(
            "/usr/bin/pkexec",
            &[
                "--disable-internal-agent",
                "/usr/libexec/wros-clock",
                "network-time",
            ],
        ),
        Request::AudioStatus => Invocation::new("/usr/bin/amixer", &["sget", "Master"]),
        Request::SetVolume(vol) => {
            if *vol > 100 {
                bail!("volume must be 0..=100");
            }
            let arg = format!("{vol}%");
            Invocation::new("/usr/bin/amixer", &["sset", "Master", &arg, "unmute"])
        }
        Request::SetMute(muted) => Invocation::new(
            "/usr/bin/amixer",
            &["sset", "Master", if *muted { "mute" } else { "unmute" }],
        ),
        Request::Wifi(on) => Invocation::new(
            "/usr/bin/nmcli",
            &["radio", "wifi", if *on { "on" } else { "off" }],
        ),
        Request::ScanWifi => Invocation::new(
            "/usr/bin/nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "--escape",
                "yes",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "yes",
            ],
        ),
        Request::ConnectWifi { ssid, password } => {
            crate::system_views::validate_wifi_ssid(ssid)?;
            if let Some(secret) = password {
                crate::system_views::validate_wifi_password(secret.as_str())?;
                Invocation::new(
                    "/usr/bin/nmcli",
                    &[
                        "--colors", "no", "--ask", "--wait", "15", "device", "wifi", "connect",
                        ssid,
                    ],
                )
            } else {
                Invocation::new(
                    "/usr/bin/nmcli",
                    &[
                        "--colors", "no", "--wait", "8", "device", "wifi", "connect", ssid,
                    ],
                )
            }
        }
        Request::ActivateSavedWifi(uuid) => {
            crate::system_views::validate_saved_wifi_uuid(uuid)?;
            Invocation::new(
                "/usr/bin/nmcli",
                &[
                    "--colors",
                    "no",
                    "--wait",
                    "8",
                    "connection",
                    "up",
                    "uuid",
                    uuid,
                ],
            )
        }
        Request::DisconnectWifi => active_wifi_connections(),
        Request::ListSavedWifi => Invocation::new(
            "/usr/bin/nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "--escape",
                "yes",
                "-f",
                "UUID,NAME,TYPE",
                "connection",
                "show",
            ],
        ),
        Request::ForgetWifi(uuid) => {
            crate::system_views::validate_saved_wifi_uuid(uuid)?;
            Invocation::new(
                "/usr/bin/nmcli",
                &["--colors", "no", "connection", "delete", "uuid", uuid],
            )
        }
        Request::CheckUpdates => Invocation::new("/usr/bin/apt-get", &["-s", "dist-upgrade"]),
        Request::RecoveryStatus => {
            Invocation::new("/usr/libexec/wros-update", &["recovery-status"])
        }
        Request::VerifyUpdateBundle { bundle_dir } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            Invocation::new("/usr/libexec/wros-update", &["verify-bundle", bundle_dir])
        }
        // Unprivileged: authenticates the bundle and lists it; changes nothing.
        Request::ReviewUpdateBundle { bundle_dir } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            Invocation::new("/usr/libexec/wros-update", &["review", bundle_dir])
                .with_timeout(REVIEW_TIMEOUT_SECS)
        }
        Request::StageUpdates {
            bundle_dir,
            review_id,
        } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            crate::system_views::validate_review_id(review_id)?;
            Invocation::update_launcher(
                "/usr/libexec/wros-update-stage",
                &[bundle_dir, review_id],
                STAGE_TIMEOUT_SECS,
            )
        }
        Request::FindUpdateMedia => Invocation::update_launcher(
            "/usr/libexec/wros-update-media",
            &["mount"],
            MEDIA_TIMEOUT_SECS,
        ),
        // Unprivileged: signed metadata only, from the fixed URL in the engine.
        Request::CheckOnlineUpdates => {
            Invocation::new("/usr/libexec/wros-update", &["check-online"])
                .with_timeout(ONLINE_CHECK_TIMEOUT_SECS)
        }
        Request::FetchOnlineUpdate => Invocation::update_launcher(
            "/usr/libexec/wros-update-fetch",
            &[],
            ONLINE_FETCH_TIMEOUT_SECS,
        ),
        Request::CancelStagedUpdates => {
            Invocation::update_launcher("/usr/libexec/wros-update-cancel", &[], CANCEL_TIMEOUT_SECS)
        }
        Request::ApplyStagedUpdate => {
            Invocation::update_launcher("/usr/libexec/wros-update-apply", &[], APPLY_TIMEOUT_SECS)
        }
        Request::GoBackToPreviousVersion => Invocation::update_launcher(
            "/usr/libexec/wros-update-rollback",
            &[],
            STAGE_TIMEOUT_SECS,
        ),
        Request::ReportUpdateReady => Invocation::update_launcher(
            "/usr/libexec/wros-update-healthy",
            &["ready"],
            CANCEL_TIMEOUT_SECS,
        ),
        Request::RecoverUpdates => {
            Invocation::update_launcher("/usr/libexec/wros-update-repair", &[], REPAIR_TIMEOUT_SECS)
        }
        Request::ListStorageDevices => Invocation::new(
            "/usr/bin/findmnt",
            &[
                "--json",
                "--list",
                "--bytes",
                "--df",
                "--output",
                "SOURCE,TARGET,SIZE,USED,AVAIL,USE%",
            ],
        ),
        Request::ConnectGoogleAccount
        | Request::DisconnectGoogleAccount
        | Request::ListGoogleDriveFiles
        | Request::MaterializeDriveFile { .. }
        | Request::ListBackups
        | Request::CreateBackup { .. }
        | Request::RestoreBackup { .. }
        | Request::VerifyBackups
        | Request::RebootRecovery
        | Request::RebootFirmware => {
            bail!("this native workflow is not implemented; no action was performed")
        }
        Request::ListLocalAccounts | Request::GetGoogleStatus => {
            bail!("read-only local request handled directly by Linux backend")
        }
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
                    PowerAction::Suspend => bail!(crate::SUSPEND_UNAVAILABLE),
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
            &[
                "--disable-internal-agent",
                "/usr/libexec/writingdeck-keyboard",
                "preview",
                l.id(),
            ],
        ),
        Request::KeyboardConfirm(t) | Request::KeyboardCancel(t) => {
            if t.len() != 32 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("invalid keyboard preview token");
            }
            Invocation::new(
                "/usr/bin/pkexec",
                &[
                    "--disable-internal-agent",
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
        Request::DisplayStatus => Invocation::new(
            "/usr/bin/pkexec",
            &[
                "--disable-internal-agent",
                "/usr/libexec/wros-display",
                "status",
            ],
        ),
        Request::DisplayPreview(choice) => {
            choice.validate()?;
            Invocation::new(
                "/usr/bin/pkexec",
                &[
                    "--disable-internal-agent",
                    "/usr/libexec/wros-display",
                    "preview",
                    if choice.enabled { "on" } else { "off" },
                    &choice.kelvin.to_string(),
                    &choice.strength.to_string(),
                ],
            )
        }
        Request::DisplayConfirm(token) | Request::DisplayCancel(token) => {
            crate::display::validate_token(token)?;
            Invocation::new(
                "/usr/bin/pkexec",
                &[
                    "--disable-internal-agent",
                    "/usr/libexec/wros-display",
                    if matches!(request, Request::DisplayConfirm(_)) {
                        "confirm"
                    } else {
                        "cancel"
                    },
                    token,
                ],
            )
        }
    })
}

fn scrub_bytes(buf: &mut [u8]) {
    for byte in buf {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

impl<R: Runner> Backend for Linux<R> {
    fn request(&mut self, request: Request) -> Result<Reply> {
        if request == Request::Refresh {
            return Ok(Reply::Snapshot(self.snapshot()));
        }
        if request == Request::ListLocalAccounts {
            let file =
                fs::File::open(self.etc.join("passwd")).context("reading local account list")?;
            let mut content = String::new();
            file.take(1_048_577).read_to_string(&mut content)?;
            if content.len() > 1_048_576 {
                bail!("local account list exceeds limit");
            }
            return Ok(Reply::LocalAccounts(crate::system_views::local_accounts(
                &content,
                unsafe { libc::geteuid() },
            )?));
        }
        if request == Request::GetGoogleStatus {
            return Ok(Reply::GoogleStatus(crate::GoogleAccountStatus {
                connected: false,
                email: None,
                status_message: "Google enrollment is not configured in this build.".into(),
            }));
        }
        if let Request::ConnectWifi { ssid, password } = &request {
            let cmd = invocation(&request)?;
            let run_result = if let Some(secret) = password {
                let mut stdin_bytes = format!("{}\n", secret.as_str()).into_bytes();
                let res = self.runner.run_with_input(&cmd, &stdin_bytes);
                scrub_bytes(&mut stdin_bytes);
                res.map_err(|e| {
                    let redacted = e.to_string().replace(secret.as_str(), "[hidden]");
                    anyhow::anyhow!("{redacted}")
                })
            } else {
                self.runner.run(&cmd)
            };
            let output = run_result?;
            if output.contains("Error:") {
                bail!("Wi-Fi connection failed; check network credentials and signal");
            }
            let active_output = self.runner.run(&Invocation::new(
                "/usr/bin/nmcli",
                &[
                    "--colors",
                    "no",
                    "-t",
                    "--escape",
                    "yes",
                    "-f",
                    "IN-USE,SSID,SIGNAL,SECURITY",
                    "device",
                    "wifi",
                    "list",
                    "--rescan",
                    "no",
                ],
            ))?;
            let active = crate::system_views::wifi_scan(&active_output)?;
            if !active
                .iter()
                .any(|network| network.in_use && network.ssid == *ssid)
            {
                bail!(
                    "Requested Wi-Fi network could not be verified as active; check credentials and signal"
                );
            }
            return Ok(Reply::Message(format!(
                "Connected to Wi-Fi network {ssid}."
            )));
        }
        if let Request::ActivateSavedWifi(uuid) = &request {
            let cmd = invocation(&request)?;
            let output = self.runner.run(&cmd)?;
            if output.contains("Error:") {
                bail!("saved Wi-Fi profile could not be activated");
            }
            let active_output = self.runner.run(&active_wifi_connections())?;
            let active = crate::system_views::saved_wifi(&active_output)?;
            if !active.iter().any(|p| &p.uuid == uuid) {
                bail!("saved Wi-Fi profile activation could not be verified");
            }
            return Ok(Reply::Message(format!(
                "Connected saved Wi-Fi profile {uuid}."
            )));
        }
        if request == Request::DisconnectWifi {
            let active_output = self.runner.run(&invocation(&request)?)?;
            let active = crate::system_views::saved_wifi(&active_output)?;
            if active.is_empty() {
                bail!("no active Wi-Fi connection to disconnect");
            }
            for profile in &active {
                self.runner.run(&Invocation::new(
                    "/usr/bin/nmcli",
                    &[
                        "--colors",
                        "no",
                        "connection",
                        "down",
                        "uuid",
                        &profile.uuid,
                    ],
                ))?;
            }
            return Ok(Reply::Message("Wi-Fi disconnected.".into()));
        }
        if let Request::ForgetWifi(uuid) = &request {
            let cmd = invocation(&request)?;
            self.runner.run(&cmd)?;
            return Ok(Reply::Message(format!(
                "Removed saved Wi-Fi profile {uuid}."
            )));
        }
        let output = self.runner.run(&invocation(&request)?)?;
        if request == Request::ListTimezones {
            return Ok(Reply::Timezones(crate::timezone::catalog(&output)?));
        }
        if let Request::SetTimezone(expected) = &request {
            // A successful method call alone does not establish the current
            // value; another session may have changed it concurrently.
            let actual = self.runner.run(&timezone_status())?;
            crate::timezone::validate(&actual)?;
            if &actual != expected {
                bail!("timezone change could not be verified; refresh current status");
            }
            return Ok(Reply::Timezone(actual));
        }
        if let Request::SetLocalClock(expected) = &request {
            let expected_norm = crate::timezone::validate_local_datetime(expected)?;
            let actual = crate::system_views::parse_clock_reply(&output, &expected_norm)?;
            return Ok(Reply::LocalClock(actual));
        }
        if matches!(request, Request::UseNetworkTime) {
            crate::system_views::parse_network_time_reply(&output)?;
            return Ok(Reply::NetworkTime);
        }
        if matches!(
            request,
            Request::AudioStatus | Request::SetVolume(_) | Request::SetMute(_)
        ) {
            return Ok(Reply::Audio(crate::system_views::parse_amixer_status(
                &output,
            )?));
        }
        if matches!(request, Request::DisplayStatus) {
            let status: crate::display::Status =
                serde_json::from_str(&output).context("invalid display status")?;
            status.validate()?;
            return Ok(Reply::DisplayStatus(status));
        }
        if matches!(request, Request::DisplayPreview(_)) {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Preview {
                token: String,
                seconds: u64,
            }
            let preview: Preview =
                serde_json::from_str(&output).context("invalid display preview reply")?;
            crate::display::validate_token(&preview.token)?;
            if preview.seconds == 0 || preview.seconds > 15 {
                bail!("display preview expired or invalid");
            }
            return Ok(Reply::DisplayPreview {
                token: preview.token,
                seconds: preview.seconds,
            });
        }
        if matches!(
            request,
            Request::DisplayConfirm(_) | Request::DisplayCancel(_)
        ) {
            let result: serde_json::Value =
                serde_json::from_str(&output).context("invalid display completion reply")?;
            let message = match (request, result["status"].as_str()) {
                (Request::DisplayConfirm(_), Some("committed")) => {
                    "Screen warmth kept for this display and future boots."
                }
                (Request::DisplayCancel(_), Some("cancelled")) => {
                    "Previous screen colors restored."
                }
                (Request::DisplayCancel(_), Some("no-preview")) => {
                    "Display preview has already finished."
                }
                _ => bail!("display transaction was not confirmed by the service"),
            };
            return Ok(Reply::Message(message.into()));
        }
        match &request {
            Request::ScanWifi => {
                return Ok(Reply::WifiScan(crate::system_views::wifi_scan(&output)?));
            }
            Request::ListSavedWifi => {
                return Ok(Reply::SavedWifi(crate::system_views::saved_wifi(&output)?));
            }
            Request::ListStorageDevices => {
                return Ok(Reply::StorageDevices(crate::system_views::storage(
                    &output,
                )?));
            }
            Request::CheckUpdates => {
                return Ok(Reply::Updates(crate::system_views::updates(&output)?));
            }
            Request::RecoveryStatus => {
                return Ok(Reply::Recovery(crate::system_views::recovery_status(
                    &output,
                )?));
            }
            Request::VerifyUpdateBundle { .. } => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["verified"],
                )?));
            }
            Request::ReviewUpdateBundle { .. } => {
                return Ok(Reply::UpdateReview(crate::system_views::update_review(
                    &output,
                )?));
            }
            Request::StageUpdates { review_id, .. } => {
                return Ok(Reply::UpdateStaged(crate::system_views::update_staged(
                    &output, review_id,
                )?));
            }
            Request::FindUpdateMedia => {
                return Ok(Reply::UpdateMedia(crate::system_views::update_media(
                    &output,
                )?));
            }
            Request::CheckOnlineUpdates => {
                return Ok(Reply::OnlineUpdate(crate::system_views::online_update(
                    &output,
                )?));
            }
            Request::FetchOnlineUpdate => {
                return Ok(Reply::OnlineUpdateFetched(
                    crate::system_views::online_fetch(&output)?,
                ));
            }
            Request::ApplyStagedUpdate => {
                return Ok(Reply::UpdateApplied(crate::system_views::update_applied(
                    &output,
                )?));
            }
            Request::CancelStagedUpdates => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["cancelled", "none"],
                )?));
            }
            Request::GoBackToPreviousVersion => {
                return Ok(Reply::UpdateStaged(crate::system_views::update_gone_back(
                    &output,
                )?));
            }
            Request::ReportUpdateReady => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["ready", "none", "busy"],
                )?));
            }
            Request::RecoverUpdates => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["recovered", "clean"],
                )?));
            }
            _ => {}
        }
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
    fn parse_time_sync_reads_yes_no_and_ignores_anything_else() {
        assert_eq!(
            parse_time_sync("NTP=yes\nNTPSynchronized=no\n"),
            (Some(true), Some(false))
        );
        assert_eq!(
            parse_time_sync("NTPSynchronized=yes\nNTP=no\n"),
            (Some(false), Some(true))
        );
        assert_eq!(parse_time_sync("NTP=maybe\n"), (None, None));
        assert_eq!(parse_time_sync(""), (None, None));
    }
    #[test]
    fn unfinished_mutations_cannot_invoke_tools_or_fabricate_service_success() {
        let mut backend = Linux {
            runner: Fake::default(),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        for request in [
            Request::ForgetWifi("ambiguous profile".into()),
            Request::ActivateSavedWifi("not-a-uuid".into()),
            Request::ConnectWifi {
                ssid: "--invalid-flag".into(),
                password: None,
            },
            Request::ConnectWifi {
                ssid: "ValidSSID".into(),
                password: Some(crate::WifiSecret::new("short")),
            },
            Request::VerifyUpdateBundle {
                bundle_dir: "relative-bundle".into(),
            },
            Request::VerifyUpdateBundle {
                bundle_dir: "/media/../etc".into(),
            },
            Request::ConnectGoogleAccount,
            Request::DisconnectGoogleAccount,
            Request::ListGoogleDriveFiles,
            Request::MaterializeDriveFile {
                file_id: "file-01".into(),
                local_name: "draft.md".into(),
            },
            Request::ListBackups,
            Request::CreateBackup {
                target_dir: "/unused".into(),
            },
            Request::RestoreBackup {
                snapshot_id: "latest".into(),
                restore_dir: "/unused".into(),
            },
            Request::VerifyBackups,
            Request::StageUpdates {
                bundle_dir: "/media/usb/wros-updates".into(),
                review_id: "not-a-review-id".into(),
            },
            Request::StageUpdates {
                bundle_dir: "/etc".into(),
                review_id: "a".repeat(64),
            },
            Request::ReviewUpdateBundle {
                bundle_dir: "/var/lib/wros-update".into(),
            },
            Request::RebootRecovery,
            Request::RebootFirmware,
        ] {
            assert!(backend.request(request).is_err());
        }
        assert!(backend.runner.0.is_empty());
        // Even with valid arguments, unverified Fake output ("disconnected:none")
        // is refused by the readback/JSON verification checks.
        assert!(
            backend
                .request(Request::ConnectWifi {
                    ssid: "ValidSSID".into(),
                    password: None,
                })
                .is_err()
        );
        assert!(backend.request(Request::DisconnectWifi).is_err());
        assert!(backend.request(Request::RecoveryStatus).is_err());
        assert!(
            backend
                .request(Request::VerifyUpdateBundle {
                    bundle_dir: "/media/usb/wros-updates".into()
                })
                .is_err()
        );
        assert!(backend.request(Request::CancelStagedUpdates).is_err());
        assert!(backend.request(Request::RecoverUpdates).is_err());
        let Reply::GoogleStatus(status) = backend.request(Request::GetGoogleStatus).unwrap() else {
            panic!()
        };
        assert!(!status.connected);
        assert!(status.email.is_none());
        assert!(
            backend.request(Request::ListLocalAccounts).is_err(),
            "missing passwd is an error, never a fabricated writer account"
        );
    }
    #[test]
    fn update_verbs_use_fixed_argv_and_only_privileged_verbs_use_pkexec() {
        let dir = "/media/usb/wros-updates";
        let id = "0123456789abcdef".repeat(4);
        let verify = invocation(&Request::VerifyUpdateBundle {
            bundle_dir: dir.into(),
        })
        .unwrap();
        assert_eq!(verify.program, "/usr/libexec/wros-update");
        assert_eq!(verify.args, ["verify-bundle", dir]);

        // Review is unprivileged and never goes through pkexec.
        let review = invocation(&Request::ReviewUpdateBundle {
            bundle_dir: dir.into(),
        })
        .unwrap();
        assert_eq!(review.program, "/usr/libexec/wros-update");
        assert_eq!(review.args, ["review", dir]);
        assert_eq!(review.timeout, Duration::from_secs(180));

        let pk = "--disable-internal-agent";
        let stage = invocation(&Request::StageUpdates {
            bundle_dir: dir.into(),
            review_id: id.clone(),
        })
        .unwrap();
        assert_eq!(stage.program, "/usr/bin/pkexec");
        assert_eq!(
            stage.args,
            [pk, "/usr/libexec/wros-update-stage", dir, id.as_str()]
        );
        assert_eq!(stage.timeout, Duration::from_secs(900));

        for (request, helper, args, seconds) in [
            (
                Request::CancelStagedUpdates,
                "/usr/libexec/wros-update-cancel",
                vec![],
                60,
            ),
            (
                Request::RecoverUpdates,
                "/usr/libexec/wros-update-repair",
                vec![],
                900,
            ),
            (
                Request::FindUpdateMedia,
                "/usr/libexec/wros-update-media",
                vec!["mount"],
                60,
            ),
        ] {
            let command = invocation(&request).unwrap();
            assert_eq!(command.program, "/usr/bin/pkexec");
            let mut expected = vec![pk, helper];
            expected.extend(args);
            assert_eq!(command.args, expected);
            assert_eq!(command.timeout, Duration::from_secs(seconds));
        }
        // Read-only status and display calls keep their existing budgets.
        assert_eq!(
            invocation(&Request::RecoveryStatus).unwrap().timeout,
            Duration::from_secs(60)
        );
        assert_eq!(
            invocation(&Request::DisplayStatus).unwrap().timeout,
            Duration::from_secs(8)
        );
    }
    #[test]
    fn update_requests_with_bad_paths_or_review_ids_never_reach_the_runner() {
        let mut backend = Linux {
            runner: Fake::default(),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        let good_id = "ab".repeat(32);
        for bad_dir in [
            "relative/bundle",
            "/",
            "/media/../etc",
            "/etc",
            "/var/lib/wros-update/public",
            "/run/wros-update",
            "/run/wros-update/staging",
            "/home/writer/novel",
            "/media",
            "/media/",
            "/media/usb/",
            "/media//usb",
            "/media/./usb",
            "/mnt",
            "/proc/self",
            "/media/usb\nreview",
        ] {
            for request in [
                Request::ReviewUpdateBundle {
                    bundle_dir: bad_dir.into(),
                },
                Request::StageUpdates {
                    bundle_dir: bad_dir.into(),
                    review_id: good_id.clone(),
                },
            ] {
                assert!(backend.request(request).is_err(), "{bad_dir:?}");
            }
        }
        let bad_ids: Vec<String> = vec![
            String::new(),
            "short".into(),
            "A".repeat(64),
            "g".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
            format!("{}\n", "a".repeat(63)),
            "--help".into(),
        ];
        for bad_id in bad_ids {
            assert!(
                backend
                    .request(Request::StageUpdates {
                        bundle_dir: "/media/usb/wros-updates".into(),
                        review_id: bad_id.clone(),
                    })
                    .is_err(),
                "{bad_id:?}"
            );
        }
        assert!(backend.runner.0.is_empty());
        for ok in [
            "/media/usb/wros-updates",
            "/run/media/writer/WROS-UPDATE",
            "/mnt/update",
            "/run/wros-update/media/0",
        ] {
            assert!(
                crate::system_views::validate_update_bundle_path(ok).is_ok(),
                "{ok}"
            );
        }
    }
    #[test]
    fn fabricated_update_output_is_never_success() {
        // Fake answers every command with the unrelated text "disconnected:none".
        let mut backend = Linux {
            runner: Fake::default(),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        for request in [
            Request::CancelStagedUpdates,
            Request::RecoverUpdates,
            Request::FindUpdateMedia,
            Request::ReviewUpdateBundle {
                bundle_dir: "/media/usb/wros-updates".into(),
            },
            Request::StageUpdates {
                bundle_dir: "/media/usb/wros-updates".into(),
                review_id: "ab".repeat(32),
            },
        ] {
            assert!(backend.request(request).is_err());
        }
        assert_eq!(backend.runner.0.len(), 5);
    }
    #[test]
    fn wifi_enrollment_passes_secret_on_stdin_never_argv_and_verifies_active_connection() {
        use std::collections::VecDeque;
        struct WifiRunner {
            replies: VecDeque<Result<&'static str, &'static str>>,
            calls: Vec<(Invocation, String)>,
        }
        impl Runner for WifiRunner {
            fn run(&mut self, cmd: &Invocation) -> Result<String> {
                self.run_with_input(cmd, &[])
            }
            fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
                self.calls
                    .push((cmd.clone(), String::from_utf8_lossy(input).into_owned()));
                match self.replies.pop_front().unwrap() {
                    Ok(out) => Ok(out.into()),
                    Err(err) => bail!("{err}"),
                }
            }
        }
        let uuid = "00000000-0000-4000-8000-000000000001";
        let active_line = "00000000-0000-4000-8000-000000000001:WriterDeck-Cafe:802-11-wireless";
        let mut backend = Linux {
            runner: WifiRunner {
                replies: VecDeque::from([
                    Ok("Device 'wlan0' successfully activated."),
                    Ok("*:WriterDeck-Cafe:88:WPA2"),
                    Ok("Connection 'WriterDeck-Cafe' successfully activated."),
                    Ok(active_line),
                    Ok(active_line),
                    Ok("Connection 'WriterDeck-Cafe' successfully deactivated."),
                    Ok(
                        "Connection 'WriterDeck-Cafe' (00000000-0000-4000-8000-000000000001) successfully deleted.",
                    ),
                ]),
                calls: Vec::new(),
            },
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        let secret = crate::WifiSecret::new("SUPER_SECRET_PSK_VALUE");
        let reply = backend
            .request(Request::ConnectWifi {
                ssid: "WriterDeck-Cafe".into(),
                password: Some(secret),
            })
            .unwrap();
        assert_eq!(
            reply,
            Reply::Message("Connected to Wi-Fi network WriterDeck-Cafe.".into())
        );
        assert_eq!(backend.runner.calls[0].0.program, "/usr/bin/nmcli");
        assert!(
            backend.runner.calls[0]
                .0
                .args
                .iter()
                .all(|a| !a.contains("SUPER_SECRET_PSK_VALUE"))
        );
        assert!(
            backend.runner.calls[0]
                .0
                .args
                .contains(&"--ask".to_string())
        );
        assert_eq!(backend.runner.calls[0].1, "SUPER_SECRET_PSK_VALUE\n");

        // Reconnect saved Wi-Fi by exact UUID and verify active state.
        assert_eq!(
            backend
                .request(Request::ActivateSavedWifi(uuid.into()))
                .unwrap(),
            Reply::Message(format!("Connected saved Wi-Fi profile {uuid}."))
        );
        // Disconnect active Wi-Fi without assuming an interface name.
        assert_eq!(
            backend.request(Request::DisconnectWifi).unwrap(),
            Reply::Message("Wi-Fi disconnected.".into())
        );
        // Forget saved Wi-Fi profile by UUID.
        assert_eq!(
            backend.request(Request::ForgetWifi(uuid.into())).unwrap(),
            Reply::Message(format!("Removed saved Wi-Fi profile {uuid}."))
        );

        let mut unrelated = Linux {
            runner: WifiRunner {
                replies: VecDeque::from([Ok("success"), Ok("*:OtherSSID:90:WPA2")]),
                calls: Vec::new(),
            },
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        assert!(
            unrelated
                .request(Request::ConnectWifi {
                    ssid: "WriterDeck-Cafe".into(),
                    password: None
                })
                .is_err()
        );

        // Failed authentication redacts any echoed secret from the error.
        let mut failing = Linux {
            runner: WifiRunner {
                replies: VecDeque::from([Err(
                    "Secrets were required, rejected SUPER_SECRET_PSK_VALUE",
                )]),
                calls: Vec::new(),
            },
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        let err = failing
            .request(Request::ConnectWifi {
                ssid: "WriterDeck-Cafe".into(),
                password: Some(crate::WifiSecret::new("SUPER_SECRET_PSK_VALUE")),
            })
            .unwrap_err()
            .to_string();
        assert!(!err.contains("SUPER_SECRET_PSK_VALUE"));
        assert!(err.contains("[hidden]"));
    }
    #[test]
    fn read_only_views_use_structured_fixed_commands_without_secrets_or_assumed_interfaces() {
        let scan = invocation(&Request::ScanWifi).unwrap();
        assert_eq!(scan.program, "/usr/bin/nmcli");
        assert!(scan.args.windows(2).any(|p| p == ["--escape", "yes"]));
        let profiles = invocation(&Request::ListSavedWifi).unwrap();
        assert!(profiles.args.contains(&"UUID,NAME,TYPE".into()));
        let disks = invocation(&Request::ListStorageDevices).unwrap();
        assert_eq!(disks.program, "/usr/bin/findmnt");
        assert!(disks.args.contains(&"--json".into()));
        assert_eq!(
            invocation(&Request::CheckUpdates).unwrap().args,
            ["-s", "dist-upgrade"]
        );
    }
    #[test]
    fn process_runner_rejects_truncation_and_invalid_utf8_instead_of_partial_success() {
        let mut runner = ProcessRunner;
        assert!(
            runner
                .run(&Invocation::new(
                    "/usr/bin/python3",
                    &["-c", "import sys; sys.stdout.write('x'*65537)"]
                ))
                .is_err()
        );
        assert!(
            runner
                .run(&Invocation::new(
                    "/usr/bin/python3",
                    &["-c", "import sys; sys.stdout.buffer.write(bytes([255]))"]
                ))
                .is_err()
        );
        assert_eq!(
            runner
                .run(&Invocation::new(
                    "/usr/bin/python3",
                    &["-c", "print('  name  ')"]
                ))
                .unwrap(),
            "  name  "
        );
        assert_eq!(
            runner
                .run_with_input(
                    &Invocation::new(
                        "/usr/bin/python3",
                        &["-c", "import sys; print(sys.stdin.readline().strip())"]
                    ),
                    b"stdin-secret-line\n"
                )
                .unwrap(),
            "stdin-secret-line"
        );
    }
    #[test]
    fn actual_debian_findmnt_json_matches_the_storage_view_contract() {
        let mut runner = ProcessRunner;
        let output = runner
            .run(&Invocation::new(
                "/usr/bin/findmnt",
                &[
                    "--json",
                    "--list",
                    "--bytes",
                    "--df",
                    "--target",
                    ".",
                    "--output",
                    "SOURCE,TARGET,SIZE,USED,AVAIL,USE%",
                ],
            ))
            .unwrap();
        let devices = crate::system_views::storage(&output).unwrap();
        assert_eq!(devices.len(), 1);
        assert!(
            std::env::current_dir()
                .unwrap()
                .starts_with(&devices[0].mountpoint)
        );
        assert!(devices[0].total_bytes > 0);
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
    #[test]
    fn unsupported_suspend_never_reaches_a_process_runner() {
        let mut backend = Linux {
            runner: Fake::default(),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        let error = backend
            .request(Request::Power(PowerAction::Suspend))
            .unwrap_err();
        assert!(error.to_string().contains("secure screen locking"));
        assert!(
            backend.runner.0.is_empty(),
            "not even a systemctl probe may run"
        );
        assert_eq!(
            invocation(&Request::Power(PowerAction::Restart))
                .unwrap()
                .args,
            ["--no-ask-password", "reboot"]
        );
        assert_eq!(
            invocation(&Request::Power(PowerAction::Shutdown))
                .unwrap()
                .args,
            ["--no-ask-password", "poweroff"]
        );
        assert!(
            crate::Simulated::laptop()
                .request(Request::Power(PowerAction::Suspend))
                .is_err()
        );
    }
    #[test]
    fn timezone_change_is_fixed_noninteractive_and_requires_matching_readback() {
        use std::collections::VecDeque;
        struct Replies(VecDeque<&'static str>, Vec<Invocation>);
        impl Runner for Replies {
            fn run(&mut self, invocation: &Invocation) -> Result<String> {
                self.1.push(invocation.clone());
                let output = self.0.pop_front().unwrap();
                if output == "FAIL" {
                    bail!("fixture service unavailable");
                }
                Ok(output.into())
            }
        }
        for (outputs, accepted) in [
            (vec!["", "Asia/Tokyo"], true),
            (vec!["", "UTC"], false),
            (vec!["", "bad output"], false),
            (vec!["", "FAIL"], false),
            (vec!["FAIL"], false),
        ] {
            let mut backend = Linux {
                runner: Replies(outputs.into(), vec![]),
                sys: "/missing".into(),
                etc: "/missing".into(),
                storage: "/missing".into(),
            };
            let result = backend.request(Request::SetTimezone("Asia/Tokyo".into()));
            assert_eq!(result.is_ok(), accepted);
            if accepted {
                assert_eq!(result.unwrap(), Reply::Timezone("Asia/Tokyo".into()));
            }
            assert_eq!(
                backend.runner.1[0],
                Invocation::new(
                    "/usr/bin/timedatectl",
                    &[
                        "--no-ask-password",
                        "--no-pager",
                        "set-timezone",
                        "Asia/Tokyo"
                    ]
                )
            );
        }
        let mut backend = Linux {
            runner: Fake::default(),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        assert!(
            backend
                .request(Request::SetTimezone("--host=other".into()))
                .is_err()
        );
        assert!(backend.runner.0.is_empty());
        assert!(
            backend.request(Request::ListTimezones).is_err(),
            "invalid service output must not become a catalog"
        );
    }
    #[test]
    fn display_calls_are_fixed_bounded_and_never_start_a_tty_auth_reader() {
        use crate::display::Warmth;
        let command = invocation(&Request::DisplayPreview(Warmth::default())).unwrap();
        assert_eq!(command.program, "/usr/bin/pkexec");
        assert_eq!(
            command.args,
            [
                "--disable-internal-agent",
                "/usr/libexec/wros-display",
                "preview",
                "on",
                "2200",
                "100"
            ]
        );
        assert!(
            invocation(&Request::DisplayPreview(Warmth {
                kelvin: 999,
                ..Warmth::default()
            }))
            .is_err()
        );
        assert!(invocation(&Request::DisplayConfirm(";rm -rf /".into())).is_err());
        assert_eq!(
            invocation(&Request::KeyboardPreview(
                crate::keyboard::Layout::ALL[0].clone()
            ))
            .unwrap()
            .args[0],
            "--disable-internal-agent"
        );
    }
    #[test]
    fn malformed_or_expired_display_replies_never_become_successful_previews() {
        struct Output(&'static str);
        impl Runner for Output {
            fn run(&mut self, _: &Invocation) -> Result<String> {
                Ok(self.0.into())
            }
        }
        for output in [
            r#"{"token":"bad","seconds":15}"#,
            r#"{"token":"0123456789abcdef0123456789abcdef","seconds":0}"#,
            r#"{"token":"0123456789abcdef0123456789abcdef","seconds":16}"#,
        ] {
            let mut backend = Linux {
                runner: Output(output),
                sys: "/missing".into(),
                etc: "/missing".into(),
                storage: "/missing".into(),
            };
            assert!(
                backend
                    .request(Request::DisplayPreview(crate::display::Warmth::default()))
                    .is_err()
            );
        }
    }
}
