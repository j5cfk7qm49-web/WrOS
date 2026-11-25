use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions, Permissions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use self::volume::VolumeAction;

#[cfg(test)]
mod tests;
mod volume;

const STEP_PERCENT: i128 = 5;
const LOWEST_PERCENT: i128 = 5;
const BRIGHTNESSCTL: &str = match option_env!("WROS_PATH_BRIGHTNESSCTL") {
    Some(path) => path,
    None => "/usr/bin/brightnessctl",
};
const TOOL_PATH: &str = "/usr/bin:/bin";
const TOOL_LANG: &str = "C.UTF-8";
const TIMEOUT_SECS: u32 = 5;
const TIMEOUT: Duration = Duration::from_secs(TIMEOUT_SECS as u64);
const MUSIC_ACTIONS: [&str; 3] = ["play-pause", "next", "previous"];
const MUSIC_LOG_SIZE: usize = 8;
const USAGE: &str = "usage: wros-media-key brightness-up | brightness-down | volume-up | \
                     volume-down | mute | play-pause | next | previous";

enum Action {
    Brightness { up: bool },
    Volume(VolumeAction),
    Music(&'static str),
}

impl Action {
    fn parse(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Option<Self> {
        let mut args = args.into_iter();
        let action = args.next()?;
        if args.next().is_some() {
            return None;
        }
        match action.as_ref().to_str()? {
            "brightness-up" => Some(Self::Brightness { up: true }),
            "brightness-down" => Some(Self::Brightness { up: false }),
            "volume-up" => Some(Self::Volume(VolumeAction::Up)),
            "volume-down" => Some(Self::Volume(VolumeAction::Down)),
            "mute" => Some(Self::Volume(VolumeAction::Toggle)),
            action => MUSIC_ACTIONS
                .into_iter()
                .find(|&key| key == action)
                .map(Self::Music),
        }
    }
}

struct MediaKeys<'a, R, V> {
    state: &'a Path,
    sys_root: &'a Path,
    run_command: R,
    change_volume: V,
}

impl<R, V> MediaKeys<'_, R, V>
where
    R: FnMut(&mut Command, Duration) -> io::Result<()>,
    V: FnMut(VolumeAction) -> io::Result<()>,
{
    fn run(
        &mut self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        mut stderr: impl Write,
    ) -> u8 {
        let Some(action) = Action::parse(args) else {
            let _ = writeln!(stderr, "{USAGE}");
            return 2;
        };
        let result = match action {
            Action::Music(key) => record_music_key(self.state, key),
            Action::Brightness { up } => change_and_notify(self.state, || {
                change_brightness(up, self.sys_root, &mut self.run_command)
            }),
            Action::Volume(action) => {
                change_and_notify(self.state, || (self.change_volume)(action).map(|()| true))
            }
        };
        if result.is_ok() { 0 } else { 1 }
    }
}

pub(super) fn run() -> u8 {
    MediaKeys {
        state: Path::new("/run/wros-keys"),
        sys_root: Path::new("/sys"),
        run_command,
        change_volume: volume::change_system_volume,
    }
    .run(std::env::args_os().skip(1), io::stderr().lock())
}

fn open_lock(state: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(state.join("lock"))
}

fn change_and_notify(state: &Path, change: impl FnOnce() -> io::Result<bool>) -> io::Result<()> {
    // On Linux File::try_lock uses flock(LOCK_EX | LOCK_NB).
    let lock = match open_lock(state) {
        Ok(lock) => match lock.try_lock() {
            Err(TryLockError::WouldBlock) => return Ok(()),
            _ => Some(lock),
        },
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => return Ok(()),
        // For example, the state directory may not exist yet during boot.
        Err(_) => None,
    };
    let changed = change();
    drop(lock);
    if changed? {
        let _ = fs::write(state.join("changed"), b"");
    }
    Ok(())
}

fn backlight(sys_root: &Path) -> Option<(OsString, i64, i64)> {
    let entries = fs::read_dir(sys_root.join("class/backlight")).ok()?;
    let mut entries = entries.collect::<io::Result<Vec<_>>>().ok()?;
    entries.sort_unstable_by_key(fs::DirEntry::file_name);
    entries.into_iter().find_map(|entry| {
        let path = entry.path();
        let now = fs::read_to_string(path.join("brightness")).ok()?;
        let top = fs::read_to_string(path.join("max_brightness")).ok()?;
        let now = now.trim().parse().ok()?;
        let top = top.trim().parse().ok()?;
        (top > 0).then(|| (entry.file_name(), now, top))
    })
}

// Python's round() rounds halfway values to the nearest even integer.
fn round_ratio(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator.div_euclid(denominator);
    let twice_remainder = 2 * numerator.rem_euclid(denominator);
    if twice_remainder > denominator || (twice_remainder == denominator && quotient % 2 != 0) {
        quotient + 1
    } else {
        quotient
    }
}

fn brightness_target(now: i64, top: i64, up: bool) -> i64 {
    let top = i128::from(top);
    let now = i128::from(now);
    let lowest = ((top * LOWEST_PERCENT + 99) / 100).max(1);
    let step = round_ratio(top * STEP_PERCENT, 100).max(1);
    let target = if up { now + step } else { now - step };
    target.clamp(lowest.min(top), top) as i64
}

fn change_brightness(
    up: bool,
    sys_root: &Path,
    mut run: impl FnMut(&mut Command, Duration) -> io::Result<()>,
) -> io::Result<bool> {
    let Some((name, now, top)) = backlight(sys_root) else {
        return Ok(false);
    };
    let target = brightness_target(now, top, up);
    if target == now {
        return Ok(false);
    }
    let mut device = OsString::from("--device=");
    device.push(name);
    let mut command = Command::new(BRIGHTNESSCTL);
    command
        .args(["--quiet", "--class=backlight"])
        .arg(device)
        .args(["set", &target.to_string()])
        .env_clear()
        .env("PATH", TOOL_PATH)
        .env("LANG", TOOL_LANG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run(&mut command, TIMEOUT)?;
    Ok(true)
}

fn run_command(command: &mut Command, timeout: Duration) -> io::Result<()> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    let error = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(io::Error::other(format!("brightnessctl {status}"))),
            Ok(None) => {}
            Err(error) => break error,
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break io::Error::new(io::ErrorKind::TimedOut, "brightnessctl timed out");
        }
        thread::sleep(remaining.min(Duration::from_millis(1)));
    };
    let _ = child.kill();
    let _ = child.wait();
    Err(error)
}

fn last_music_sequence(line: &str) -> Option<u64> {
    let mut words = line
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '\x1c'..='\x1f'))
        .filter(|word| !word.is_empty());
    let number = words.next()?;
    let action = words.next()?;
    // The old helper's Python int() also limits the number of decimal digits.
    if number.len() > 4300
        || !number.bytes().all(|digit| digit.is_ascii_digit())
        || !MUSIC_ACTIONS.contains(&action)
        || words.next().is_some()
    {
        return None;
    }
    let sequence = number.parse().ok()?;
    (1..u64::MAX).contains(&sequence).then_some(sequence)
}

fn record_music_key(state: &Path, action: &str) -> io::Result<()> {
    let lock = open_lock(state)?;
    lock.lock()?;
    let path = state.join("music");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text.replace("\r\n", "\n"),
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData
            ) =>
        {
            String::new()
        }
        Err(err) => return Err(err),
    };
    // Python's splitlines() recognizes these separators and drops a final terminator.
    let mut lines: Vec<_> = text
        .split_terminator([
            '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect();
    let sequence = lines.last().copied().and_then(last_music_sequence);
    if sequence.is_none() {
        lines.clear();
    }

    let mut temp = tempfile::NamedTempFile::new_in(state)?;
    let skip = lines.len().saturating_sub(MUSIC_LOG_SIZE - 1);
    for line in &lines[skip..] {
        writeln!(temp, "{line}")?;
    }
    writeln!(temp, "{} {action}", sequence.unwrap_or(0) + 1)?;
    temp.as_file()
        .set_permissions(Permissions::from_mode(0o644))?;
    temp.persist(path).map_err(|err| err.error)?;
    Ok(())
}
