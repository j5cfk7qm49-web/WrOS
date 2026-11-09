//! The editor never reads unlock credentials. Debian's independent locker owns
//! another VT while this main-thread handoff holds an input-retirement proof.
use anyhow::{Context, Result, bail};
use ratatui::crossterm::{
    cursor::{Hide, MoveTo},
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{Clear, ClearType},
};
use std::{
    fs,
    io::Write,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub fn available() -> bool {
    #[cfg(all(debug_assertions, feature = "pty-qa"))]
    if fixture_locker().is_some() {
        return true;
    }
    crate::installed_session()
        && display_available()
        && unsafe { libc::geteuid() } != 0
        && !Path::new("/etc/wros/VM-DEMO-ONLY").exists()
        && !Path::new("/run/wros-live/session.json").exists()
        && !Path::new("/run/live/medium/live/filesystem.squashfs").is_file()
        && [
            "/usr/libexec/wros-lock",
            "/usr/bin/openvt",
            "/usr/sbin/vlock-main",
        ]
        .iter()
        .all(|path| Path::new(path).is_file())
}

pub fn suspend_available() -> bool {
    display_available()
        && available()
        && Path::new("/etc/systemd/system/systemd-suspend.service.d/60-wros-lock.conf").is_file()
        && fs::read_to_string("/sys/power/state")
            .is_ok_and(|states| states.split_whitespace().any(|state| state == "mem"))
}

#[cfg(all(debug_assertions, feature = "pty-qa"))]
fn fixture_locker() -> Option<std::path::PathBuf> {
    let helper = std::path::PathBuf::from(std::env::var_os("WROS_QA_LOCK_HELPER")?);
    (unsafe { libc::geteuid() } != 0
        && fs::read_link("/proc/self/fd/0").is_ok_and(|tty| tty.starts_with("/dev/pts"))
        && helper.is_absolute()
        && helper.is_file())
    .then_some(helper)
}

/// The first battery's charge, whether it is running down, and whether it is at its reserve.
fn read_battery(root: &Path) -> crate::battery::Reading {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .find_map(|battery| {
            let path = battery.path();
            let read = |key| fs::read_to_string(path.join(key)).unwrap_or_default();
            (read("type").trim() == "Battery").then(|| {
                let (status, level, capacity) =
                    (read("status"), read("capacity_level"), read("capacity"));
                crate::battery::Reading {
                    percent: capacity.trim().parse().ok(),
                    discharging: status.trim() == "Discharging",
                    critical: battery_needs_sleep(&status, &level, &capacity),
                }
            })
        })
        .unwrap_or_default()
}

fn battery_needs_sleep(status: &str, level: &str, capacity: &str) -> bool {
    status.trim() == "Discharging"
        && (level.trim() == "Critical"
            || capacity
                .trim()
                .parse::<u8>()
                .is_ok_and(|percent| percent <= crate::battery::RESERVE))
}

#[cfg(test)]
mod tests {
    use super::battery_needs_sleep;

    #[test]
    fn only_a_discharging_battery_at_its_reserve_requests_sleep() {
        assert!(battery_needs_sleep("Discharging\n", "Low\n", "8\n"));
        assert!(battery_needs_sleep("Discharging", "Critical", ""));
        assert!(!battery_needs_sleep("Discharging", "Low", "9"));
        assert!(!battery_needs_sleep("Discharging", "Unknown", ""));
        assert!(!battery_needs_sleep("Charging", "Critical", "2"));
    }

    #[test]
    fn a_compositor_needs_the_active_owned_console_for_lock_handoff() {
        assert!(super::lock_display(false, true, "tty1", true));
        assert!(!super::lock_display(false, true, "tty2", true));
        assert!(!super::lock_display(false, true, "tty1", false));
        assert!(!super::lock_display(false, false, "tty1", true));
        assert!(super::lock_display(true, false, "tty1", true));
    }
}

/// The lid and the battery, read every two seconds by a thread of their own.
#[derive(Default)]
pub struct Power {
    pub lid_closed: AtomicBool,
    pub battery: std::sync::Mutex<crate::battery::Reading>,
}

pub fn watch_power() -> Arc<Power> {
    let power = Arc::new(Power::default());
    let output = Arc::downgrade(&power);
    std::thread::spawn(move || {
        while let Some(output) = output.upgrade() {
            let lid = Command::new("/usr/bin/busctl")
                .args([
                    "--timeout=1s",
                    "get-property",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "LidClosed",
                ])
                .output()
                .is_ok_and(|out| out.status.success() && out.stdout == b"b true\n");
            let battery = read_battery(Path::new("/sys/class/power_supply"));
            output.lid_closed.store(lid, Ordering::Relaxed);
            if let Ok(mut reading) = output.battery.lock() {
                *reading = battery;
            }
            drop(output);
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    power
}

fn console() -> bool {
    (0..=2).all(|fd| {
        let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
        unsafe {
            libc::isatty(fd) == 1
                && libc::fstat(fd, info.as_mut_ptr()) == 0
                && info.assume_init().st_rdev == libc::makedev(4, 1)
        }
    })
}

fn console_file() -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open("/dev/tty1")
}

fn lock_display(console: bool, composited: bool, active: &str, accessible: bool) -> bool {
    console || (composited && active.trim() == "tty1" && accessible)
}

fn display_available() -> bool {
    let composed = crate::session::composited();
    lock_display(
        console(),
        composed,
        &fs::read_to_string("/sys/class/tty/tty0/active").unwrap_or_default(),
        composed && console_file().is_ok(),
    )
}

pub fn handoff(
    terminal: &mut crate::display::Terminal,
    input: &mut Option<wros_input::Input>,
    suspend: bool,
    transition_theme: Option<&crate::theme::Theme>,
) -> Result<()> {
    if !available() || (suspend && !suspend_available()) {
        bail!("Console locking is unavailable in this session");
    }
    if input.is_none() && !crate::display::windowed(terminal) {
        bail!("Writing input could not be retired");
    }
    let mut console_file = crate::session::composited()
        .then(console_file)
        .transpose()
        .context("opening the authenticated session console")?;
    let proof = input
        .as_mut()
        .map(wros_input::Input::pause)
        .transpose()
        .context("retiring writing input before lock")?;
    if let Some(theme) = transition_theme {
        crate::motion::before_lock(terminal, theme);
    }
    crate::display::suspend(terminal)?;
    // A worker panic must not run Ratatui's global terminal-restoration hook
    // during authentication. No render, cursor query or event read occurs here.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = (|| -> Result<()> {
        // Write-only controls. Keep tty1 raw/non-echoing during helper startup;
        // ratatui::restore would turn echo on. Credentials go only to root tty8.
        execute!(
            std::io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            DisableFocusChange,
            PopKeyboardEnhancementFlags,
            Clear(ClearType::All),
            Clear(ClearType::Purge),
            MoveTo(0, 0),
            Hide,
        )?;
        let mut locker = Command::new("/usr/bin/pkexec");
        locker.args(["--disable-internal-agent", "/usr/libexec/wros-lock"]);
        if let Some(console) = console_file.as_mut() {
            console.write_all(b"\x1b[0m\x1b[2J\x1b[3J\x1b[H\x1b[?25l")?;
            locker
                .stdin(console.try_clone()?)
                .stdout(console.try_clone()?)
                .stderr(console.try_clone()?);
        }
        if suspend {
            locker.arg("--suspend");
        }
        #[cfg(all(debug_assertions, feature = "pty-qa"))]
        if let Some(helper) = fixture_locker() {
            locker = Command::new(helper);
        }
        let mut child = locker
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LANG", "C.UTF-8")
            .env("TERM", "linux")
            .spawn()
            .context("starting the independent console locker")?;
        let status = crate::display::wait_for(terminal, &mut child)?;
        if !status.success() {
            bail!("Console lock failed; writing input remains retired");
        }
        Ok(())
    })();
    std::panic::set_hook(previous_hook);
    result?; // Failure drops the proof without resuming; caller saves/exits.
    // The maintained locker has exited after authentication. Discard residual
    // typeahead before any terminal initialization/query or fresh reader.
    if input.is_some() && unsafe { libc::tcflush(0, libc::TCIFLUSH) } != 0 {
        bail!("Console input could not be cleared after unlock");
    }
    if let Some(console) = console_file
        && unsafe { libc::tcflush(console.as_raw_fd(), libc::TCIFLUSH) } != 0
    {
        bail!("Session console input could not be cleared after unlock");
    }
    crate::display::resume(terminal).context("restoring writing display after unlock")?;
    let _ = execute!(
        std::io::stdout(),
        EnableMouseCapture,
        EnableBracketedPaste,
        EnableFocusChange
    );
    let _ = execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    if let (Some(input), Some(proof)) = (input, proof) {
        input
            .resume(proof)
            .context("starting fresh writing input after unlock")?;
    }
    crate::display::reclaim(terminal).context("restoring graphical console ownership")?;
    Ok(())
}
