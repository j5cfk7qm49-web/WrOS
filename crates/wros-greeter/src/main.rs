#[cfg(target_os = "linux")]
fn main() {
    // Never format an authentication value or panic payload on screen/in logs.
    std::panic::set_hook(Box::new(|_| {}));
    if let Err(reason) = linux::run() {
        // Only fixed internal diagnostics reach syslog, never PAM/wire strings.
        let message = std::ffi::CString::new(reason).unwrap();
        unsafe {
            libc::syslog(
                libc::LOG_ERR,
                c"wros-greeter: %s".as_ptr(),
                message.as_ptr(),
            );
        }
        eprintln!("WrOS login unavailable. Use the authenticated maintenance console.");
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("WrOS greeter requires Linux and greetd.");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod linux {
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use std::{
        io::Read,
        net::Shutdown,
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    use wros_greeter::{Conversation, Phase, Request, keyboard, transport::Client};

    struct TerminalGuard;
    fn repaint<B: ratatui::backend::Backend>(terminal: &mut ratatui::Terminal<B>) {
        // After draw, the current buffer is empty. Swapping also resets the
        // previous frame, forcing the next diff to repaint without blanking
        // the physical console or querying the authentication input stream.
        terminal.swap_buffers();
    }
    // Linux console startup/hotplug output can invalidate Ratatui's cached
    // screen without generating an input or resize event. Recover the full
    // prompt while idle; response-only redraws cannot repair a cold-boot wipe.
    fn refresh<B: ratatui::backend::Backend>(
        terminal: &mut ratatui::Terminal<B>,
        last_refresh: &mut Instant,
        now: Instant,
    ) {
        if now.duration_since(*last_refresh) >= Duration::from_secs(1) {
            repaint(terminal);
            *last_refresh = now;
        }
    }
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            // Discard typeahead before restoring canonical/echo mode.
            unsafe {
                libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
            }
            ratatui::restore();
        }
    }
    fn drain_input() -> Result<(), &'static str> {
        // crossterm may already have parsed events beyond the kernel queue.
        for _ in 0..256 {
            if !event::poll(Duration::ZERO).map_err(|_| "input drain poll failed")? {
                return if unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) } == 0 {
                    Ok(())
                } else {
                    Err("terminal input flush failed")
                };
            }
            let _ = event::read().map_err(|_| "input drain read failed")?;
        }
        Err("excessive terminal typeahead")
    }
    pub fn run() -> Result<(), &'static str> {
        // Defense in depth for a non-root greeter; PAM remains greetd's job.
        let core = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::geteuid() } == 0
            || unsafe { libc::setrlimit(libc::RLIMIT_CORE, &core) } != 0
            || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
        {
            return Err("process hardening failed");
        }
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
            signal_hook::flag::register(signal, stop.clone())
                .map_err(|_| "signal registration failed")?;
        }
        let path = std::env::var_os("GREETD_SOCK").ok_or("login socket missing")?;
        let mut client = Client::connect(Path::new(&path))?;
        let interrupt = client.interrupter()?;
        let mut terminal = ratatui::try_init().map_err(|_| "terminal initialization failed")?;
        let _terminal_guard = TerminalGuard;
        drain_input()?;
        let mut passwd = String::new();
        let accounts = if std::fs::File::open("/etc/passwd")
            .and_then(|file| file.take(1024 * 1024).read_to_string(&mut passwd))
            .is_ok()
        {
            wros_greeter::local_accounts(&passwd)
        } else {
            vec![]
        };
        let mut selected_account = None;
        let (requests, incoming) = mpsc::sync_channel::<Request>(1);
        let (replies, completed) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            while let Ok(request) = incoming.recv() {
                let response = client.exchange(&request);
                drop(request);
                let failed = response.is_err();
                if replies.send(response).is_err() || failed {
                    break;
                }
            }
        });
        let (keyboard_requests, keyboard_incoming) = mpsc::sync_channel::<keyboard::Command>(1);
        let (keyboard_replies, keyboard_completed) = mpsc::sync_channel(1);
        // No terminal reader or PAM state in this worker. An outstanding helper
        // blocks authentication, while the main input loop remains responsive.
        std::thread::spawn(move || {
            while let Ok(command) = keyboard_incoming.recv() {
                let reply = keyboard::execute(&command);
                if keyboard_replies.send((command, reply)).is_err() {
                    break;
                }
            }
        });
        let result = (|| {
            let mut conversation = Conversation::default();
            let mut keyboard = keyboard::Keyboard::default();
            let mut last_refresh = Instant::now();
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Err("login interrupted");
                }
                match keyboard_completed.try_recv() {
                    Ok((command, reply)) => {
                        drain_input()?;
                        keyboard.receive(
                            &command,
                            reply.as_deref().map_err(|reason| *reason),
                            Instant::now(),
                        );
                        conversation.clear();
                        repaint(&mut terminal);
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("keyboard worker disconnected");
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                if let Some(command) = keyboard.tick(Instant::now()) {
                    drain_input()?;
                    keyboard_requests
                        .try_send(command)
                        .map_err(|_| "keyboard queue unavailable")?;
                }
                match completed.try_recv() {
                    Ok(response) => {
                        drain_input()?;
                        let followup = conversation.receive(response?)?;
                        repaint(&mut terminal);
                        if let Some(request) = followup {
                            requests
                                .try_send(request)
                                .map_err(|_| "login request queue unavailable")?;
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("login worker disconnected");
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                if conversation.phase == Phase::Done {
                    drain_input()?;
                    return Ok(());
                }
                if conversation.phase == Phase::Restart {
                    return Err("login reset after rejected cancellation");
                }
                refresh(&mut terminal, &mut last_refresh, Instant::now());
                terminal
                    .draw(|frame| {
                        if keyboard.open {
                            keyboard.render(frame, Instant::now());
                        } else {
                            wros_greeter::render(frame, &conversation);
                        }
                    })
                    .map_err(|_| "terminal draw failed")?;
                if !event::poll(Duration::from_millis(50)).map_err(|_| "terminal poll failed")? {
                    continue;
                }
                let Event::Key(key) = event::read().map_err(|_| "terminal read failed")? else {
                    continue;
                };
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if keyboard.open {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Err("keyboard recovery interrupted");
                    }
                    let command = match key.code {
                        KeyCode::Up => {
                            keyboard.choose(-1);
                            None
                        }
                        KeyCode::Down => {
                            keyboard.choose(1);
                            None
                        }
                        KeyCode::Enter => keyboard.enter(Instant::now()),
                        KeyCode::Esc => keyboard.escape(),
                        _ => None,
                    };
                    if let Some(command) = command {
                        drain_input()?;
                        keyboard_requests
                            .try_send(command)
                            .map_err(|_| "keyboard queue unavailable")?;
                    }
                    if !keyboard.open {
                        drain_input()?;
                        conversation.clear();
                        conversation.notice = keyboard.notice;
                        repaint(&mut terminal);
                    }
                    continue;
                }
                if key.code == KeyCode::F(2) && conversation.open_keyboard(&mut keyboard) {
                    drain_input()?;
                    repaint(&mut terminal);
                    continue;
                }
                if key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    if let Some(request) = conversation.cancel() {
                        requests
                            .try_send(request)
                            .map_err(|_| "login request queue unavailable")?;
                    } else {
                        return Err("login cancelled");
                    } // pending request: close socket, let greetd reset
                    continue;
                }
                if matches!(conversation.phase, Phase::Waiting(_)) {
                    continue;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Down
                        if conversation.phase == Phase::Username && !accounts.is_empty() =>
                    {
                        let index = match (selected_account, key.code) {
                            (None, KeyCode::Up) => accounts.len() - 1,
                            (None, _) => 0,
                            (Some(index), KeyCode::Up) => {
                                (index + accounts.len() - 1) % accounts.len()
                            }
                            (Some(index), _) => (index + 1) % accounts.len(),
                        };
                        selected_account = Some(index);
                        conversation.choose_account(&accounts[index]);
                    }
                    KeyCode::Enter => {
                        if let Some(request) = conversation.submit() {
                            requests
                                .try_send(request)
                                .map_err(|_| "login request queue unavailable")?;
                        }
                    }
                    KeyCode::Backspace => conversation.backspace(),
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        conversation.clear()
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        conversation.type_char(c)
                    }
                    _ => {}
                }
            }
        })();
        let _ = interrupt.shutdown(Shutdown::Both);
        drop(requests);
        drop(completed);
        drop(keyboard_requests);
        drop(keyboard_completed);
        worker.join().map_err(|_| "login worker failed")?;
        result
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ratatui::{
            Terminal,
            backend::{Backend, TestBackend},
        };

        #[test]
        fn idle_console_wipe_recovers_without_input_and_keeps_secrets_hidden() {
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            let mut conversation = Conversation::default();
            conversation.choose_account("writer");
            conversation.submit().unwrap();
            conversation
                .receive(wros_greeter::Response::AuthMessage {
                    auth_message_type: wros_greeter::MessageKind::Secret,
                    auth_message: "Password:".into(),
                })
                .unwrap();
            for c in "synthetic-secret".chars() {
                conversation.type_char(c);
            }
            let draw = |frame: &mut ratatui::Frame| wros_greeter::render(frame, &conversation);
            terminal.draw(draw).unwrap();
            let expected = terminal.backend().buffer().clone();
            // Independent console output clears the actual device, not its
            // cached previous frame. Ordinary diff rendering cannot recover it.
            terminal.backend_mut().clear().unwrap();
            terminal.draw(draw).unwrap();
            let text = |terminal: &Terminal<TestBackend>| {
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
            };
            assert!(!text(&terminal).contains("Password:"));
            let start = Instant::now();
            let mut last = start;
            refresh(&mut terminal, &mut last, start + Duration::from_secs(1));
            terminal.draw(draw).unwrap();
            let recovered = text(&terminal);
            assert_eq!(terminal.backend().buffer(), &expected);
            assert!(recovered.contains("Password:") && recovered.contains("[hidden]"));
            assert!(!recovered.contains("synthetic-secret"));
        }
    }
}
