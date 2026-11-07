//! The real `wros-boot unlock` on a pseudo-terminal standing in for the console:
//! what the boot image's wrapper gets on standard output, and its exit code.
#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_wros-boot");

struct Console {
    master: Arc<OwnedFd>,
    screen: Arc<Mutex<vt100::Parser>>,
    child: Child,
}

impl Console {
    fn start(width: u16, height: u16, env: &[(&str, &str)]) -> Self {
        let (mut master, mut slave) = (0, 0);
        let size = libc::winsize {
            ws_row: height,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(opened, 0);
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        let mut name = [0 as libc::c_char; 128];
        assert_eq!(
            unsafe { libc::ttyname_r(slave.as_raw_fd(), name.as_mut_ptr(), name.len()) },
            0
        );
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_string();
        let child = Command::new(BINARY)
            .args([
                "unlock",
                "Please unlock disk sda3_crypt: ",
                "--tty",
                &path,
                "--theme",
                "/nonexistent",
            ])
            .env_clear()
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(slave);
        let master = Arc::new(master);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(
            std::num::NonZero::new(height).unwrap(),
            std::num::NonZero::new(width).unwrap(),
            0,
        )));
        {
            let (master, screen) = (Arc::clone(&master), Arc::clone(&screen));
            std::thread::spawn(move || {
                let mut buf = [0u8; 65536];
                loop {
                    let n = unsafe {
                        libc::read(master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len())
                    };
                    if n <= 0 {
                        break;
                    }
                    screen.lock().unwrap().process(&buf[..n as usize]);
                }
            });
        }
        let mut console = Self {
            master,
            screen,
            child,
        };
        let end = Instant::now() + Duration::from_secs(10);
        while !console.screen().contains("Type your disk password") {
            if let Some(status) = console.child.try_wait().unwrap() {
                let mut err = String::new();
                console
                    .child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                panic!("wros-boot exited early ({status}): {err}");
            }
            assert!(
                Instant::now() < end,
                "waited for the prompt
{}",
                console.screen()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        console
    }

    fn screen(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn until(&self, what: &str, ready: impl Fn(&str) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !ready(&self.screen()) {
            assert!(Instant::now() < end, "waited for {what}\n{}", self.screen());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn type_bytes(&self, bytes: &[u8]) {
        let n = unsafe { libc::write(self.master.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(n, bytes.len() as isize);
    }

    /// Exit code and everything written to standard output and standard error.
    fn finish(mut self) -> (i32, Vec<u8>, Vec<u8>) {
        let end = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < end,
                "wros-boot never exited\n{}",
                self.screen()
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        self.child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut err)
            .unwrap();
        (status.code().unwrap_or(-1), out, err)
    }
}

#[test]
fn the_password_reaches_standard_output_once_exactly_as_typed() {
    let console = Console::start(100, 37, &[]);
    console.type_bytes("pässwortXY".as_bytes());
    console.until("ten dots", |s| s.contains("* * * * * * * * * *"));
    // Backspace twice, an arrow key that does nothing, then Enter.
    console.type_bytes(b"\x7f\x7f\x1b[D");
    console.until("eight dots", |s| {
        s.contains("* * * * * * * *") && !s.contains("* * * * * * * * *")
    });
    assert!(!console.screen().contains("pässwort"));
    console.type_bytes(b"\r");
    let (code, out, err) = console.finish();
    assert_eq!(code, 0);
    assert_eq!(out, "pässwort".as_bytes(), "no newline, nothing else");
    assert!(err.is_empty());
}

#[test]
fn esc_asks_for_the_text_prompt_and_writes_nothing() {
    let console = Console::start(80, 25, &[]);
    console.type_bytes(b"half-typed");
    console.type_bytes(b"\x1b");
    let (code, out, err) = console.finish();
    assert_eq!(code, 10);
    assert!(out.is_empty() && err.is_empty());
}

#[test]
fn a_console_that_goes_away_ends_with_nothing_written() {
    let console = Console::start(80, 25, &[]);
    console.type_bytes(b"secret");
    console.until("dots", |s| s.contains("* * *"));
    // Hang up the terminal, as when the console is lost.
    let master = Arc::clone(&console.master);
    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSIG, libc::SIGHUP) };
    let pid = console.child.id() as i32;
    unsafe { libc::kill(pid, libc::SIGHUP) };
    let (code, out, _) = console.finish();
    assert_ne!(code, 0);
    assert!(out.is_empty());
}

#[test]
fn a_wrong_try_is_counted_on_screen() {
    let console = Console::start(
        100,
        37,
        &[("CRYPTTAB_TRIED", "2"), ("CRYPTTAB_OPTION_tries", "3")],
    );
    console.until("the try count", |s| s.contains("Try 3 of 3."));
    console.type_bytes(b"\x1b");
    assert_eq!(console.finish().0, 10);
}

#[test]
fn without_a_console_it_fails_fast_and_quietly() {
    let output = Command::new(BINARY)
        .args(["unlock", "--tty", "/nonexistent/console"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(20));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let output = Command::new(BINARY).output().unwrap();
    assert_eq!(output.status.code(), Some(20));
}
