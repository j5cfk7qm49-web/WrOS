//! `wros-boot unlock [PROMPT] [--tty PATH] [--theme PATH]`
//!
//! Run by the boot image's `askpass` wrapper in place of Debian's prompt. It draws
//! on the console and reads keys from it; the password goes to standard output
//! once, after Enter, with no newline, as Debian's `askpass` gives it to cryptsetup.
//! Any other ending writes nothing and exits non-zero, and the wrapper runs
//! Debian's prompt (see `wros_boot::exit`).
//!
//! `wros-boot splash [--tty PATH] [--theme PATH] [--progress-file PATH]`
//!
//! The progress screen after the unlock (`wros-boot-splash.service`): the logo and
//! systemd's boot progress, until systemd stops it as the login screen starts. It
//! ends by itself 3 seconds after the boot is complete, or after 3 minutes.

#[cfg(target_os = "linux")]
fn main() {
    // Nothing is ever printed: not a panic message, and never a password.
    std::panic::set_hook(Box::new(|_| {}));
    let code = std::panic::catch_unwind(linux::run).unwrap_or(wros_boot::exit::FAILED);
    std::process::exit(code);
}

#[cfg(not(target_os = "linux"))]
fn main() {
    std::process::exit(wros_boot::exit::FAILED);
}

#[cfg(target_os = "linux")]
mod linux {
    use ratatui::{Terminal, TerminalOptions, Viewport, backend::Backend, layout::Rect};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Write},
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
        path::PathBuf,
    };
    use wros_boot::{
        exit,
        keys::Decoder,
        splash,
        unlock::{Outcome, Unlock},
    };
    use wros_graphics::Screen;

    /// How long an Esc waits for the rest of an arrow key's sequence.
    const ESC_WAIT_MS: i32 = 60;

    #[derive(PartialEq)]
    enum Mode {
        Unlock,
        Splash,
    }

    struct Args {
        mode: Mode,
        tty: PathBuf,
        theme: PathBuf,
        /// Tests only: read the progress from this file instead of systemd.
        progress_file: Option<PathBuf>,
    }

    fn args() -> Option<Args> {
        let mut args = std::env::args().skip(1);
        let (mode, tty) = match args.next().as_deref() {
            Some("unlock") => (Mode::Unlock, "/dev/console"),
            Some("splash") => (Mode::Splash, "/dev/tty1"),
            _ => return None,
        };
        let mut parsed = Args {
            mode,
            tty: tty.into(),
            theme: wros_boot::THEME.into(),
            progress_file: None,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--tty" => parsed.tty = args.next()?.into(),
                "--theme" => parsed.theme = args.next()?.into(),
                "--progress-file" => parsed.progress_file = Some(args.next()?.into()),
                // Debian's prompt text ("Please unlock disk …"); the screen words its own.
                _ => {}
            }
        }
        Some(parsed)
    }

    /// Set by SIGTERM or SIGINT: systemd stopping the progress screen.
    static STOP: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_stop(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }

    /// The console in raw mode, put back as it was however the screen ends.
    struct Raw {
        tty: File,
        saved: libc::termios,
        /// Clear the screen on the way out (the password screen); the progress screen
        /// leaves its last frame for the login screen to draw over, without a blank flash.
        clear: bool,
    }

    impl Raw {
        fn new(tty: File, clear: bool) -> Option<Self> {
            let fd = tty.as_raw_fd();
            let mut saved = unsafe { std::mem::zeroed::<libc::termios>() };
            if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
                return None;
            }
            let mut raw = saved;
            unsafe { libc::cfmakeraw(&mut raw) };
            (unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } == 0).then_some(Self {
                tty,
                saved,
                clear,
            })
        }

        fn size(&self) -> Rect {
            let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
            let ok = unsafe { libc::ioctl(self.tty.as_raw_fd(), libc::TIOCGWINSZ, &mut size) } == 0;
            if ok && size.ws_col > 0 && size.ws_row > 0 {
                Rect::new(0, 0, size.ws_col, size.ws_row)
            } else {
                Rect::new(0, 0, 80, 25)
            }
        }

        /// Waits for a byte; `None` on time-out, `Err` when the console is gone.
        fn read(&mut self, wait_ms: i32, buf: &mut [u8]) -> Result<Option<usize>, ()> {
            let mut poll = libc::pollfd {
                fd: self.tty.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            match unsafe { libc::poll(&mut poll, 1, wait_ms) } {
                0 => Ok(None),
                n if n < 0 => {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        Ok(None)
                    } else {
                        Err(())
                    }
                }
                _ => match self.tty.read(buf) {
                    Ok(0) | Err(_) => Err(()),
                    Ok(n) => Ok(Some(n)),
                },
            }
        }
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            // Leave the cursor back (and a clear screen) for whatever comes next.
            let tail: &[u8] = if self.clear {
                b"\x1b[0m\x1b[2J\x1b[H\x1b[?25h"
            } else {
                b"\x1b[0m\x1b[?25h"
            };
            let _ = self.tty.write_all(tail);
            unsafe { libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSAFLUSH, &self.saved) };
        }
    }

    pub fn run() -> i32 {
        let Some(args) = args() else {
            return exit::FAILED;
        };
        let Ok(tty) = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&args.tty)
        else {
            return exit::FAILED;
        };
        let Some(mut raw) = Raw::new(tty, args.mode == Mode::Unlock) else {
            return exit::FAILED;
        };
        let theme = wros_boot::theme(&args.theme);
        if args.mode == Mode::Splash {
            return show_progress(&args, &mut raw, &theme);
        }
        let mut unlock = Unlock::from_env(|name| std::env::var(name).ok());
        let outcome = {
            let Ok(out) = raw.tty.try_clone() else {
                return exit::FAILED;
            };
            let backend = Screen::new(out, !wros_graphics::text_start(), &raw.tty);
            let theme = if backend.graphical() {
                theme
            } else {
                theme.for_console()
            };
            let size = backend
                .size()
                .map(|s| Rect::new(0, 0, s.width, s.height))
                .unwrap_or(raw.size());
            let viewport = Viewport::Fixed(size);
            let Ok(mut terminal) = Terminal::with_options(backend, TerminalOptions { viewport })
            else {
                return exit::FAILED;
            };
            // A plain clear: Ratatui's asks the terminal where the cursor is, and in the boot
            // image nothing need answer.
            if raw.tty.write_all(b"\x1b[0m\x1b[2J\x1b[H").is_err()
                || terminal.hide_cursor().is_err()
            {
                return exit::FAILED;
            }
            let mut decoder = Decoder::default();
            let mut buf = [0u8; 64];
            loop {
                if terminal.draw(|f| unlock.draw(f, &theme)).is_err() {
                    return exit::FAILED;
                }
                let wait = if decoder.esc_pending() {
                    ESC_WAIT_MS
                } else {
                    100
                };
                let keys: Vec<_> = match raw.read(wait, &mut buf) {
                    Err(()) => return exit::FAILED,
                    Ok(None) => decoder.timeout().into_iter().collect(),
                    Ok(Some(n)) => buf[..n].iter().filter_map(|&b| decoder.feed(b)).collect(),
                };
                buf.fill(0);
                let mut finished = None;
                for key in keys {
                    match unlock.key(key) {
                        Outcome::Typing => {}
                        done => {
                            finished = Some(done);
                            break;
                        }
                    }
                }
                if let Some(done) = finished {
                    break done;
                }
            }
        };
        drop(raw);
        match outcome {
            Outcome::Done => {
                let mut stdout = std::io::stdout().lock();
                if stdout.write_all(unlock.password().as_bytes()).is_err()
                    || stdout.flush().is_err()
                {
                    return exit::FAILED;
                }
                exit::UNLOCK
            }
            Outcome::Text => exit::TEXT,
            Outcome::Typing => exit::FAILED,
        }
    }

    /// systemd's boot progress, or the test's file.
    fn progress(args: &Args) -> Option<f64> {
        let text = match &args.progress_file {
            Some(path) => std::fs::read_to_string(path).ok()?,
            None => {
                let out = std::process::Command::new("/usr/bin/busctl")
                    .args([
                        "get-property",
                        "org.freedesktop.systemd1",
                        "/org/freedesktop/systemd1",
                        "org.freedesktop.systemd1.Manager",
                        "Progress",
                    ])
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()?;
                String::from_utf8(out.stdout).ok()?
            }
        };
        splash::parse_progress(&text)
    }

    fn show_progress(args: &Args, raw: &mut Raw, theme: &wros_shell::grimoire_theme::Theme) -> i32 {
        unsafe {
            libc::signal(
                libc::SIGTERM,
                on_stop as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t,
            );
            libc::signal(
                libc::SIGINT,
                on_stop as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t,
            );
        }
        let Ok(out) = raw.tty.try_clone() else {
            return exit::FAILED;
        };
        let backend = Screen::new(out, !wros_graphics::text_start(), &raw.tty);
        let theme = if backend.graphical() {
            theme.clone()
        } else {
            theme.for_console()
        };
        let size = backend
            .size()
            .map(|s| Rect::new(0, 0, s.width, s.height))
            .unwrap_or(raw.size());
        let viewport = Viewport::Fixed(size);
        let Ok(mut terminal) = Terminal::with_options(backend, TerminalOptions { viewport }) else {
            return exit::FAILED;
        };
        if raw.tty.write_all(b"\x1b[0m\x1b[2J\x1b[H").is_err() || terminal.hide_cursor().is_err() {
            return exit::FAILED;
        }
        let started = Instant::now();
        let mut finished: Option<Instant> = None;
        let mut shown: f64 = -1.0;
        let mut buf = [0u8; 64];
        while !STOP.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(180) {
            let now = progress(args).unwrap_or(shown.max(0.0));
            {
                if terminal.draw(|f| splash::draw(f, &theme, now)).is_err() {
                    return exit::FAILED;
                }
                shown = now;
            }
            if now >= 1.0
                && finished.get_or_insert_with(Instant::now).elapsed() >= Duration::from_secs(3)
            {
                break;
            }
            // Keys typed while the laptop starts are read and dropped, never echoed.
            if raw.read(150, &mut buf).is_err() {
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        exit::UNLOCK
    }
}
