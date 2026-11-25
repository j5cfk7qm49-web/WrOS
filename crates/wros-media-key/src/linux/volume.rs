use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;

use alsa::mixer::{Mixer, Selem, SelemChannelId, SelemId};

use super::{STEP_PERCENT, TIMEOUT_SECS, TOOL_LANG, TOOL_PATH, round_ratio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VolumeAction {
    Up,
    Down,
    Toggle,
}

pub(super) trait PlaybackMixer {
    fn volume_range(&self) -> io::Result<(i64, i64)>;
    fn volume(&self) -> io::Result<i64>;
    fn set_volume(&mut self, raw: i64) -> io::Result<()>;
    fn has_switch(&self) -> bool;
    fn switch_on(&self) -> io::Result<bool>;
    fn set_switch(&mut self, on: bool) -> io::Result<()>;
}

impl PlaybackMixer for Selem<'_> {
    fn volume_range(&self) -> io::Result<(i64, i64)> {
        if !self.has_playback_volume() {
            return Err(io::Error::other("Master has no playback volume"));
        }
        Ok(self.get_playback_volume_range())
    }

    fn volume(&self) -> io::Result<i64> {
        self.get_playback_volume(SelemChannelId::FrontLeft)
            .map_err(io::Error::other)
    }

    fn set_volume(&mut self, raw: i64) -> io::Result<()> {
        self.set_playback_volume_all(raw).map_err(io::Error::other)
    }

    fn has_switch(&self) -> bool {
        self.has_playback_switch()
    }

    fn switch_on(&self) -> io::Result<bool> {
        self.get_playback_switch(SelemChannelId::FrontLeft)
            .map(|value| value != 0)
            .map_err(io::Error::other)
    }

    fn set_switch(&mut self, on: bool) -> io::Result<()> {
        self.set_playback_switch_all(i32::from(on))
            .map_err(io::Error::other)
    }
}

pub(super) fn change_volume(
    action: VolumeAction,
    mixer: &mut impl PlaybackMixer,
) -> io::Result<()> {
    match action {
        VolumeAction::Up | VolumeAction::Down => {
            let (low, high) = mixer.volume_range()?;
            let now = i128::from(mixer.volume()?);
            let (low, high) = (i128::from(low), i128::from(high));
            let range = high - low;
            let percent = if range > 0 {
                round_ratio((now - low) * 100, range)
            } else {
                0
            };
            let up = action == VolumeAction::Up;
            let step = if up { STEP_PERCENT } else { -STEP_PERCENT };
            let target_percent = (percent + step).clamp(0, 100);
            let target = low + round_ratio(target_percent * range, 100);
            let target = if target == now {
                // A coarse card still moves one of its own steps.
                now + if up { 1 } else { -1 }
            } else {
                target
            };
            mixer.set_volume(target.clamp(low.min(high), high) as i64)?;
            if up && mixer.has_switch() {
                mixer.set_switch(true)?;
            }
        }
        VolumeAction::Toggle if mixer.has_switch() => mixer.set_switch(!mixer.switch_on()?)?,
        VolumeAction::Toggle => mixer.set_volume(mixer.volume_range()?.0)?,
    }
    Ok(())
}

pub(super) fn change_system_volume(action: VolumeAction) -> io::Result<()> {
    // The former sound subprocess ran with a minimal environment and muted diagnostics.
    let null = OpenOptions::new().write(true).open("/dev/null")?;
    // SAFETY: null owns an open descriptor and dup2 replaces this process's stderr.
    if unsafe { libc::dup2(null.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: Only the one-shot binary calls this function, while still single-threaded.
    unsafe {
        if libc::clearenv() != 0 {
            return Err(io::Error::last_os_error());
        }
        std::env::set_var("PATH", TOOL_PATH);
        std::env::set_var("LANG", TOOL_LANG);
    }

    // SAFETY: This one-shot binary owns the alarm. The handler only calls _exit.
    unsafe {
        let handler = volume_timeout as *const () as libc::sighandler_t;
        if libc::signal(libc::SIGALRM, handler) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
        libc::alarm(TIMEOUT_SECS);
    }
    let result = change_alsa_volume(action);
    // SAFETY: The mixer has closed and this cancels the process's pending alarm.
    unsafe { libc::alarm(0) };
    result
}

extern "C" fn volume_timeout(_: libc::c_int) {
    // SAFETY: _exit is async-signal-safe. The kernel closes the lock descriptor.
    unsafe { libc::_exit(1) }
}

fn change_alsa_volume(action: VolumeAction) -> io::Result<()> {
    let mixer = Mixer::new("sysdefault", false).map_err(io::Error::other)?;
    let control = SelemId::new("Master", 0);
    let mut master = mixer
        .find_selem(&control)
        .ok_or_else(|| io::Error::other("This sound card has no Master volume."))?;
    change_volume(action, &mut master)
}
