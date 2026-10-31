//! Supervised live-session handoff to the native WrOS encrypted installer.
//!
//! Grimoire never reads target disks or encryption passphrases. It retires its
//! tty1 input reader (`wros_input::Input::pause()`), invokes the root-owned
//! `/usr/libexec/wros-install-session` supervisor via `pkexec`, and resumes the
//! live writing session on `tty1` after the installer completes, is cancelled,
//! or stops with target mounts/swaps/mappers retired.
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
use std::{path::Path, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Cancelled,
    Stopped,
}

fn outcome(code: Option<i32>) -> Result<Outcome> {
    match code {
        Some(0) => Ok(Outcome::Completed),
        Some(2) => Ok(Outcome::Cancelled),
        Some(3) => Ok(Outcome::Stopped),
        _ => bail!("Installer did not confirm safe target cleanup; writing input remains retired"),
    }
}

pub fn available() -> bool {
    std::env::var_os("WRITINGDECK_OS").is_some()
        && console()
        && unsafe { libc::geteuid() } != 0
        && !Path::new("/etc/writingdeck/VM-DEMO-ONLY").exists()
        && Path::new("/run/wros-live/session.json").is_file()
        && Path::new("/run/live/medium/live/filesystem.squashfs").is_file()
        && [
            "/usr/libexec/wros-install-session",
            "/usr/bin/wros-installer",
            "/usr/bin/systemd-run",
            "/usr/bin/chvt",
            "/usr/bin/unshare",
        ]
        .iter()
        .all(|path| Path::new(path).is_file())
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
) -> Result<Outcome> {
    if !available() || !console() || unsafe { libc::geteuid() } == 0 {
        bail!("Live installer handoff is unavailable in this session");
    }
    let proof = input
        .pause()
        .context("retiring writing input before starting live installer")?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let status_result = (|| -> Result<std::process::ExitStatus> {
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
        let status = Command::new("/usr/bin/pkexec")
            .args([
                "--disable-internal-agent",
                "/usr/libexec/wros-install-session",
            ])
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LANG", "C.UTF-8")
            .env("TERM", "linux")
            .status()
            .context("starting the supervised live installer")?;
        Ok(status)
    })();
    std::panic::set_hook(previous_hook);
    let status = status_result?;
    let outcome = outcome(status.code())?;
    if unsafe { libc::tcflush(0, libc::TCIFLUSH) } != 0 {
        bail!("Console input could not be cleared after installer handoff");
    }
    *terminal = ratatui::try_init().context("restoring writing display after installer handoff")?;
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
        .context("starting fresh writing input after installer handoff")?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_cleanup_verified_completion_or_cancel_allows_input_to_resume() {
        assert_eq!(outcome(Some(0)).unwrap(), Outcome::Completed);
        assert_eq!(outcome(Some(2)).unwrap(), Outcome::Cancelled);
        assert_eq!(outcome(Some(3)).unwrap(), Outcome::Stopped);
        for code in [None, Some(1), Some(126), Some(127), Some(137)] {
            assert!(outcome(code).is_err());
        }
    }
}
