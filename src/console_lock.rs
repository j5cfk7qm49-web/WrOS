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
use std::{path::Path, process::Command};

pub fn available() -> bool {
    std::env::var_os("WRITINGDECK_OS").is_some()
        && console()
        && unsafe { libc::geteuid() } != 0
        && !Path::new("/etc/writingdeck/VM-DEMO-ONLY").exists()
        && !Path::new("/run/wros-live/session.json").exists()
        && [
            "/usr/libexec/wros-lock",
            "/usr/bin/openvt",
            "/usr/sbin/vlock-main",
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
) -> Result<()> {
    if !available() || !console() || unsafe { libc::geteuid() } == 0 {
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
        let status = Command::new("/usr/bin/pkexec")
            .args(["--disable-internal-agent", "/usr/libexec/wros-lock"])
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
