#[cfg(target_os = "linux")]
fn main() {
    // Fixed errors only. Never format backend frames, answers or panic payloads.
    std::panic::set_hook(Box::new(|_| {}));
    if let Err(reason) = linux::run() {
        eprintln!(
            "WrOS installation frontend stopped: {reason}. Installation status must be checked by its supervisor."
        );
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The WrOS installer requires Linux.");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod linux {
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use std::{
        fs::File,
        io::{BufReader, Read, Write},
        os::fd::FromRawFd,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::Duration,
    };
    use wros_installer::{Exchange, Result, Session};
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            unsafe {
                libc::tcflush(0, libc::TCIFLUSH);
            }
            ratatui::restore();
        }
    }
    fn drain() -> Result<()> {
        for _ in 0..256 {
            if !event::poll(Duration::ZERO).map_err(|_| "Input poll failed")? {
                return if unsafe { libc::tcflush(0, libc::TCIFLUSH) } == 0 {
                    Ok(())
                } else {
                    Err("Input flush failed")
                };
            }
            let _ = event::read().map_err(|_| "Input read failed")?;
        }
        Err("Excessive typeahead")
    }
    pub fn run() -> Result<()> {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        if args.len() != 2 || args[0] != "--frontend-fd" {
            return Err("Installer supervisor required");
        }
        let fd = args[1]
            .parse::<i32>()
            .map_err(|_| "Invalid installer descriptor")?;
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if !(3..1024).contains(&fd)
            || unsafe { libc::geteuid() } == 0
            || unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0
            || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
            || (0..3).any(|n| unsafe { libc::isatty(n) } != 1)
        {
            return Err("Installer process isolation failed");
        }
        let mut kind = 0i32;
        let mut len = std::mem::size_of::<i32>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_TYPE,
                (&mut kind as *mut i32).cast(),
                &mut len,
            )
        } != 0
            || kind != libc::SOCK_STREAM
        {
            return Err("Installer socket required");
        }
        let mut domain = 0i32;
        let mut len = std::mem::size_of::<i32>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_DOMAIN,
                (&mut domain as *mut i32).cast(),
                &mut len,
            )
        } != 0
            || domain != libc::AF_UNIX
        {
            return Err("Local installer socket required");
        }
        let mut channel = unsafe { File::from_raw_fd(fd) };
        let incoming = channel
            .try_clone()
            .map_err(|_| "Installer channel unavailable")?;
        let (send, receive) = mpsc::sync_channel(0);
        std::thread::spawn(move || {
            let mut reader = BufReader::with_capacity(1, incoming);
            loop {
                let result = wros_installer::read_line(&mut reader);
                let ended = !matches!(result, Ok(Some(_)));
                if send.send(result).is_err() || ended {
                    break;
                }
            }
        });
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [libc::SIGINT, libc::SIGHUP, libc::SIGTERM] {
            signal_hook::flag::register(signal, stop.clone()).map_err(|_| "Signal setup failed")?;
        }
        let mut terminal = ratatui::try_init().map_err(|_| "Terminal setup failed")?;
        let _guard = TerminalGuard;
        drain()?;
        let mut session = Session::default();
        let theme = (|| {
            let path = std::path::PathBuf::from(std::env::var_os("HOME")?)
                .join(".config/grimoire/theme.toml");
            let mut saved = String::new();
            File::open(path)
                .ok()?
                .take(65_537)
                .read_to_string(&mut saved)
                .ok()?;
            (saved.len() <= 65_536).then(|| wros_shell::grimoire_theme::from_config(&saved))
        })()
        .unwrap_or_else(wros_shell::grimoire_theme::default_theme);
        let (mut scroll, mut help) = (0u16, false);
        let mut dirty = true;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("Installer interrupted");
            }
            match receive.try_recv() {
                Ok(result) => {
                    dirty = true;
                    let line = result?.ok_or("Installer backend disconnected")?;
                    match session.command(&line)? {
                        Exchange::Reply(response) => {
                            channel
                                .write_all(response.as_bytes())
                                .and_then(|_| channel.write_all(b"\n"))
                                .map_err(|_| "Installer reply failed")?;
                        }
                        Exchange::Show => {
                            drain()?;
                            scroll = 0;
                            help = false;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => return Err("Installer reader stopped"),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if dirty {
                terminal
                    .draw(|frame| wros_installer::ui::render(frame, &session, &theme, scroll, help))
                    .map_err(|_| "Installer rendering failed")?;
                dirty = false;
            }
            if !event::poll(Duration::from_millis(15)).map_err(|_| "Installer input poll failed")? {
                continue;
            }
            let key = match event::read().map_err(|_| "Installer input failed")? {
                Event::Key(key) => key,
                Event::Resize(..) => {
                    dirty = true;
                    continue;
                }
                _ => continue,
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if key.code == KeyCode::Char('c') {
                    return Err("Installer cancelled");
                }
                continue;
            }
            if help {
                dirty = true;
                match key.code {
                    KeyCode::F(1) | KeyCode::Esc => {
                        help = false;
                        scroll = 0;
                    }
                    KeyCode::PageDown => scroll = scroll.saturating_add(8),
                    KeyCode::PageUp => scroll = scroll.saturating_sub(8),
                    _ => {}
                }
                continue;
            }
            // A password has a constant placeholder. Typing into it does not
            // repaint the terminal or expose credential length through frames.
            dirty = !(session
                .active()
                .is_some_and(|q| q.kind == wros_installer::Kind::Password)
                && matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace));
            let response = match key.code {
                KeyCode::F(1) => {
                    help = !help;
                    None
                }
                KeyCode::PageDown => {
                    scroll = scroll.saturating_add(8);
                    None
                }
                KeyCode::PageUp => {
                    scroll = scroll.saturating_sub(8);
                    None
                }
                KeyCode::Enter => {
                    scroll = 0;
                    session.advance()
                }
                KeyCode::Esc => {
                    scroll = 0;
                    session.back()
                }
                code => {
                    if let Some(q) = session.active_mut() {
                        match code {
                            KeyCode::Up => q.move_by(-1),
                            KeyCode::Down => q.move_by(1),
                            KeyCode::Backspace => q.erase(),
                            KeyCode::Char(' ') if q.kind == wros_installer::Kind::Multiselect => {
                                q.toggle()
                            }
                            KeyCode::Char(c) => q.type_char(c),
                            _ => {}
                        }
                    }
                    None
                }
            };
            if let Some(response) = response {
                drain()?;
                channel
                    .write_all(response.as_bytes())
                    .and_then(|_| channel.write_all(b"\n"))
                    .map_err(|_| "Installer acknowledgment failed")?;
            }
            // Prevent old credential keystrokes entering a following field.
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                drain()?;
            }
        }
    }
}
