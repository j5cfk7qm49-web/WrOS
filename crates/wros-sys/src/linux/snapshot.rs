//! Read-only Linux status from system services and sysfs.
use super::{Invocation, Linux, Runner, invocation::timezone_status};
use crate::Snapshot;
use std::{fs, path::Path};

impl<R: Runner> Linux<R> {
    #[must_use]
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
            crate::paths::NMCLI,
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
            crate::paths::TIMEDATECTL,
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
        if let Ok(v) = fs::read_to_string(self.etc.join("wros/keyboard.json")) {
            s.keyboard = serde_json::from_str::<serde_json::Value>(&v)
                .ok()
                .and_then(|v| v.get("layout")?.as_str().map(str::to_string));
        }
        (s.battery_percent, s.charging) = battery(&self.sys);
        s.brightness_percent = brightness(&self.sys);
        s.available_bytes = available(&self.storage);
        s
    }
}
/// Battery charge and whether it is charging, from `<sys>/class/power_supply`. Several
/// batteries add up; a battery without energy readings counts by its capacity.
pub fn battery(sys: &Path) -> (Option<u8>, Option<bool>) {
    let Ok(entries) = fs::read_dir(sys.join("class/power_supply")) else {
        return (None, None);
    };
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
    let percent = percentage(charge, full).or_else(|| {
        fallback
            .iter()
            .sum::<u64>()
            .checked_div(fallback.len() as u64)
            .map(|p| p as u8)
    });
    (percent, percent.map(|_| charging))
}

/// Screen brightness of the first backlight, from `<sys>/class/backlight`.
#[must_use]
pub fn brightness(sys: &Path) -> Option<u8> {
    fs::read_dir(sys.join("class/backlight"))
        .ok()?
        .flatten()
        .find_map(|e| {
            let p = e.path();
            percentage(
                number(&p.join("brightness"))?,
                number(&p.join("max_brightness"))?,
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn percentages_are_bounded_without_zero_division_or_multiply_overflow() {
        assert_eq!(percentage(1, 0), None);
        assert_eq!(percentage(0, 100), Some(0));
        assert_eq!(percentage(51, 100), Some(51));
        assert_eq!(percentage(999, 100), Some(100));
        assert_eq!(percentage(u64::MAX, u64::MAX), Some(100));
        assert_eq!(percentage(u64::MAX / 2, u64::MAX), Some(49));
    }
}
