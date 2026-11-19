//! `wros-boot unlock [PROMPT] [--tty PATH] [--theme PATH]`
//!
//! Run by the boot image's `askpass` wrapper in place of Debian's prompt. It draws
//! on the console and reads keys from it; the password goes to standard output
//! once, after Enter, with no newline, as Debian's `askpass` gives it to cryptsetup.
//! Any other ending writes nothing and exits non-zero, and the wrapper runs
//! Debian's prompt (see `wros_boot::exit`).
//!
//! `wros-boot splash [--tty PATH] [--theme PATH] [--stage initrd] [--progress-file PATH]`
//!
//! The progress screen after the unlock: the logo and systemd's boot progress. With
//! `--stage initrd` (`wros-initrd-splash.service`) it waits until the unlock screen has
//! handed the password over, replaces that screen in place, fills the first part of the bar
//! until the switch to the installed system, and leaves its figure in `/run` for the
//! installed system's screen (`wros-boot-splash.service`). That one carries on from there
//! until the login screen replaces it: greetd sends SIGHUP before it starts the login screen,
//! and the progress screen gives the console back and ends once the login screen's first
//! frame is up (or after 5 seconds, so a text login screen shows). Without a login screen it
//! ends by itself 3 seconds after the boot is complete; either ends after 3 minutes.

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
        stage: splash::Stage,
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
            stage: splash::Stage::System,
            progress_file: None,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--tty" => parsed.tty = args.next()?.into(),
                "--theme" => parsed.theme = args.next()?.into(),
                "--stage" => {
                    parsed.stage = match args.next()?.as_str() {
                        "initrd" => splash::Stage::Initrd,
                        _ => splash::Stage::System,
                    }
                }
                "--progress-file" => parsed.progress_file = Some(args.next()?.into()),
                // Debian's prompt text ("Please unlock disk …"); the screen words its own.
                _ => {}
            }
        }
        Some(parsed)
    }

    /// Set by SIGTERM or SIGINT: systemd stopping the progress screen.
    static STOP: AtomicBool = AtomicBool::new(false);
    /// Set by SIGHUP: greetd is about to start the login screen (SIGUSR1 and SIGUSR2 are the
    /// VT's switch signals).
    static HANDOVER: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_stop(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }

    extern "C" fn on_handover(_: libc::c_int) {
        HANDOVER.store(true, Ordering::SeqCst);
    }

    fn catch(signal: libc::c_int, handler: extern "C" fn(libc::c_int)) {
        unsafe {
            libc::signal(signal, handler as *const () as libc::sighandler_t);
        }
    }

    /// How long the unlock screen stays after handing the password over, for the progress
    /// screen to replace it in place. It holds up the disk's opening, so it is short.
    const UNLOCK_HOLD: Duration = Duration::from_millis(1000);
    /// How long the progress screen waits, once greetd says the login screen is starting, for
    /// that screen's first frame. If none comes (a text login screen), it gives the console back.
    const HANDOVER_WAIT: Duration = Duration::from_secs(5);
    /// How long the initrd's progress screen stays once systemd stops answering it (after the
    /// switch to the installed system) without the installed system's screen replacing it.
    const SWITCH_WAIT: Duration = Duration::from_secs(8);
    /// The progress screen's pace: how soon it notices that the console took the display, and
    /// puts its frame back.
    const TICK_MS: i32 = 25;
    /// After the switch to the installed system, the new systemd stops the initrd's progress
    /// screen before the installed system's screen is up: it stays this long for that screen
    /// instead of giving the display back to the console in between.
    const STOP_GRACE: Duration = Duration::from_secs(3);

    /// The console in raw mode, put back as it was however the screen ends.
    struct Raw {
        tty: File,
        saved: libc::termios,
        /// Clear the screen on the way out (the password screen); the progress screen
        /// leaves its last frame for the login screen to draw over, without a blank flash.
        clear: bool,
        /// The screen draws through DRM: nothing is written to the console, on the way in or
        /// out. Any write ends fbcon's deferred takeover, which blanks the picture the boot
        /// menu left on screen (#165).
        graphical: bool,
        /// The console's settings are back: never put them back twice, as by then the next
        /// screen may have set its own.
        released: bool,
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
                graphical: false,
                released: false,
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

    impl Raw {
        /// Gives the console back (its settings, and keys typed meanwhile dropped) before the
        /// next screen takes it, rather than when this one ends.
        fn release(&mut self) {
            if std::mem::replace(&mut self.released, true) {
                return;
            }
            // Leave the cursor back (and a clear screen) for whatever comes next. A graphical
            // screen never hid the console's cursor or wrote to it.
            let tail: &[u8] = if self.graphical {
                b""
            } else if self.clear {
                b"\x1b[0m\x1b[2J\x1b[H\x1b[?25h"
            } else {
                b"\x1b[0m\x1b[?25h"
            };
            let _ = self.tty.write_all(tail);
            unsafe { libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSAFLUSH, &self.saved) };
        }
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            self.release();
        }
    }

    /// The initrd's progress screen waits, touching neither the console nor the display, until
    /// the unlock screen has handed a password over. Without one (Esc, the text prompt) it never
    /// shows. `false` when systemd stops it first.
    fn wait_for_unlock() -> bool {
        while !STOP.load(Ordering::SeqCst) {
            if std::path::Path::new(splash::UNLOCKED).exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Tells the initrd's progress screen to take over. Best effort: without it the unlock
    /// screen ends after [`UNLOCK_HOLD`] and the progress screen never starts.
    fn mark_unlocked() {
        let path = std::path::Path::new(splash::UNLOCKED);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, b"");
    }

    pub fn run() -> i32 {
        let Some(args) = args() else {
            return exit::FAILED;
        };
        if args.mode == Mode::Splash {
            catch(libc::SIGTERM, on_stop);
            catch(libc::SIGINT, on_stop);
            catch(libc::SIGHUP, on_handover);
            if args.stage == splash::Stage::Initrd && !wait_for_unlock() {
                return exit::UNLOCK;
            }
        }
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
            raw.graphical = backend.graphical();
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
            if !raw.graphical && raw.tty.write_all(b"\x1b[0m\x1b[2J\x1b[H").is_err()
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
                    if done == Outcome::Done {
                        let mut stdout = std::io::stdout().lock();
                        if stdout.write_all(unlock.password().as_bytes()).is_err()
                            || stdout.flush().is_err()
                        {
                            return exit::FAILED;
                        }
                        // The console goes back before the progress screen takes it, and the
                        // picture stays until that screen's first frame replaces it in place.
                        raw.release();
                        mark_unlocked();
                        let until = Instant::now() + UNLOCK_HOLD;
                        while raw.graphical
                            && Instant::now() < until
                            && !terminal.backend_mut().superseded()
                        {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                    }
                    break done;
                }
            }
        };
        drop(raw);
        match outcome {
            Outcome::Done => exit::UNLOCK,
            Outcome::Text => exit::TEXT,
            Outcome::Typing => exit::FAILED,
        }
    }

    /// A line in the kernel log (the journal keeps it; the screen does not show it at the boot's
    /// log level). Never anything typed.
    fn note(text: &str) {
        if let Ok(mut kmsg) = OpenOptions::new().write(true).open("/dev/kmsg") {
            let _ = kmsg.write_all(format!("<6>wros-boot: {text}\n").as_bytes());
        }
    }

    fn output(program: &str, args: &[&str]) -> Option<String> {
        let out = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).ok())
            .flatten()
    }

    /// systemd's boot progress, or the test's file.
    ///
    /// `systemctl` run as root talks to systemd over its private socket, which exists from the
    /// first moment. `busctl` needs the D-Bus daemon, which starts well into the boot: asked
    /// first, the bar stayed empty until the login screen replaced it. It stays as a fallback.
    fn progress(progress_file: Option<&std::path::Path>) -> Option<f64> {
        if let Some(path) = progress_file {
            return splash::parse_progress(&std::fs::read_to_string(path).ok()?);
        }
        output(
            wros_shell::paths::SYSTEMCTL,
            &["show", "--property=Progress", "--value"],
        )
        .as_deref()
        .and_then(splash::parse_progress)
        .or_else(|| {
            output(
                wros_shell::paths::BUSCTL,
                &[
                    "get-property",
                    "org.freedesktop.systemd1",
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    "Progress",
                ],
            )
            .as_deref()
            .and_then(splash::parse_progress)
        })
    }

    /// Leaves the initrd's figure for the installed system's screen. Best effort: without it
    /// that screen starts its bar from empty, as it did before there was an initrd screen.
    fn hand_over(shown: f64) {
        let path = std::path::Path::new(splash::HANDOVER);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("{shown:.4}\n"));
    }

    fn show_progress(args: &Args, raw: &mut Raw, theme: &wros_shell::grimoire_theme::Theme) -> i32 {
        let Ok(out) = raw.tty.try_clone() else {
            return exit::FAILED;
        };
        let backend = Screen::new(out, !wros_graphics::text_start(), &raw.tty);
        raw.graphical = backend.graphical();
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
        if !raw.graphical && raw.tty.write_all(b"\x1b[0m\x1b[2J\x1b[H").is_err()
            || terminal.hide_cursor().is_err()
        {
            return exit::FAILED;
        }
        let started = Instant::now();
        let mut finished: Option<Instant> = None;
        // The installed system's bar starts where the initrd's stopped.
        let start = match args.stage {
            splash::Stage::Initrd => 0.0,
            splash::Stage::System => std::fs::read_to_string(splash::HANDOVER)
                .ok()
                .and_then(|text| splash::parse_progress(&text))
                .unwrap_or(0.0),
        };
        let mut shown: f64 = -1.0;
        // systemd's first reading in this stage: the stage's part of the bar counts from it.
        let mut first: Option<f64> = None;
        let mut buf = [0u8; 64];
        // When greetd said the login screen is starting.
        let mut handover: Option<Instant> = None;
        // When systemd asked the initrd's screen to stop after the switch.
        let mut stopping: Option<Instant> = None;
        // systemd's progress is read on its own thread: asking can block (at the switch to the
        // installed system), and the screen must keep watching the display meanwhile. It holds
        // the last reading and when it was taken. The initrd's screen outlives the switch, where
        // its systemctl is gone: if the installed system's screen hasn't replaced it by then, it
        // ends rather than cover whatever comes next.
        let latest = std::sync::Arc::new(std::sync::Mutex::new((None::<f64>, Instant::now())));
        {
            let latest = latest.clone();
            let file = args.progress_file.clone();
            std::thread::spawn(move || {
                loop {
                    if let Some(p) = progress(file.as_deref())
                        && let Ok(mut slot) = latest.lock()
                    {
                        *slot = (Some(p), Instant::now());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            });
        }
        let mut ended = "stopped by systemd";
        loop {
            let (reading, answered) = latest.lock().map(|slot| *slot).unwrap_or((None, started));
            if STOP.load(Ordering::SeqCst) {
                // Before the switch (a wrong password's prompt, shutdown) a stop is obeyed at once.
                if args.stage == splash::Stage::System
                    || answered.elapsed() < Duration::from_secs(1)
                {
                    break;
                }
                if stopping.get_or_insert_with(Instant::now).elapsed() >= STOP_GRACE {
                    ended = "stopped after the switch, and nothing replaced it";
                    break;
                }
            }
            if started.elapsed() >= Duration::from_secs(180) {
                ended = "after 3 minutes";
                break;
            }
            // The next screen in the start-up has its first frame up: end, leaving it on screen.
            if terminal.backend_mut().superseded() {
                ended = "replaced by the next screen";
                break;
            }
            if handover.is_none() && HANDOVER.load(Ordering::SeqCst) {
                // The login screen sets the console up as it needs: give it back now, with the
                // keys typed during the start dropped, and stop reading it. greetd waits for
                // this file before it goes on. (Its reset of the console, text mode and a clear,
                // briefly gives the display to the console: superseded() puts this frame back.)
                raw.release();
                let _ = std::fs::write(splash::HANDED_OVER, b"");
                handover = Some(Instant::now());
            }
            // With no login screen's frame in time (a text login screen), end and give the
            // console its picture back.
            if handover.is_some_and(|since| since.elapsed() >= HANDOVER_WAIT) {
                ended = "no login screen's frame after the hand-over";
                break;
            }
            if args.stage == splash::Stage::Initrd && answered.elapsed() >= SWITCH_WAIT {
                ended = "systemd stopped answering and nothing replaced it";
                break;
            }
            // systemd's figure dips whenever new jobs are queued; the bar only ever moves forward.
            let now = reading
                .map(|p| splash::overall(args.stage, start, *first.get_or_insert(p), p))
                .map_or(shown, |p| p.max(shown))
                .max(start);
            // Drawn only when the figure moves (each frame is a full copy to the screen), or to
            // answer a VT switch.
            if now != shown || terminal.backend().vt_switch_pending() {
                if terminal.draw(|f| splash::draw(f, &theme, now)).is_err() {
                    note("progress screen: drawing failed");
                    return exit::FAILED;
                }
                if args.stage == splash::Stage::Initrd {
                    hand_over(now);
                }
                shown = now;
            }
            // The initrd's screen stays until the installed system's replaces it.
            if args.stage == splash::Stage::System
                && handover.is_none()
                && now >= 1.0
                && finished.get_or_insert_with(Instant::now).elapsed() >= Duration::from_secs(3)
            {
                ended = "the start-up finished";
                break;
            }
            // Keys typed while the laptop starts are read and dropped, never echoed. Once the
            // console is the login screen's, nothing more is read from it.
            if handover.is_some() || raw.read(TICK_MS, &mut buf).is_err() {
                std::thread::sleep(Duration::from_millis(TICK_MS as u64));
            }
        }
        let stage = match args.stage {
            splash::Stage::Initrd => "initrd",
            splash::Stage::System => "system",
        };
        let backend = terminal.backend();
        let (retaken, resets) = (backend.retaken(), backend.text_resets());
        note(&format!(
            "progress screen ({stage}) ended: {ended}; console text mode undone {resets} times, \
             screen taken back from the console {retaken} times"
        ));
        exit::UNLOCK
    }
}
