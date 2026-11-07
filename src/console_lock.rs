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
        && console()
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
    console()
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

fn low_battery(root: &Path) -> bool {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|battery| {
            let path = battery.path();
            let read = |key| fs::read_to_string(path.join(key)).unwrap_or_default();
            read("type").trim() == "Battery"
                && battery_needs_sleep(&read("status"), &read("capacity_level"), &read("capacity"))
        })
}

fn battery_needs_sleep(status: &str, level: &str, capacity: &str) -> bool {
    status.trim() == "Discharging"
        && (level.trim() == "Critical"
            || capacity
                .trim()
                .parse::<u8>()
                .is_ok_and(|percent| percent <= 8))
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
}

/// The lid and the battery, read every two seconds by a thread of their own.
#[derive(Default)]
pub struct Power {
    pub lid_closed: AtomicBool,
    pub battery_critical: AtomicBool,
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
            let critical = low_battery(Path::new("/sys/class/power_supply"));
            output.lid_closed.store(lid, Ordering::Relaxed);
            output.battery_critical.store(critical, Ordering::Relaxed);
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

pub fn handoff(
    terminal: &mut ratatui::DefaultTerminal,
    input: &mut wros_input::Input,
    suspend: bool,
) -> Result<()> {
    if !available() || (suspend && !suspend_available()) {
        bail!("Console locking is unavailable in this session");
    }
    let proof = input
        .pause()
        .context("retiring writing input before lock")?;
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
        if suspend {
            locker.arg("--suspend");
        }
        #[cfg(all(debug_assertions, feature = "pty-qa"))]
        if let Some(helper) = fixture_locker() {
            locker = Command::new(helper);
        }
        let status = locker
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LANG", "C.UTF-8")
            .env("TERM", "linux")
            .status()
            .context("starting the independent console locker")?;
        if !status.success() {
            bail!("Console lock failed; writing input remains retired");
        }
        Ok(())
    })();
    std::panic::set_hook(previous_hook);
    result?; // Failure drops the proof without resuming; caller saves/exits.
    // The maintained locker has exited after authentication. Discard residual
    // typeahead before any terminal initialization/query or fresh reader.
    if unsafe { libc::tcflush(0, libc::TCIFLUSH) } != 0 {
        bail!("Console input could not be cleared after unlock");
    }
    *terminal = ratatui::try_init().context("restoring writing display after unlock")?;
    terminal.clear()?;
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
    input
        .resume(proof)
        .context("starting fresh writing input after unlock")?;
    Ok(())
}
