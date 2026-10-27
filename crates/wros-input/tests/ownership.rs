//! Actual Linux PTY and reader processes. No host terminal, PAM or locking.
#[cfg(target_os = "linux")]
mod linux {
    use crossterm::event::{Event, KeyCode};
    use std::{
        fs,
        io::{self, Read, Write},
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use wros_input::{Input, WORKER_ARGUMENT, worker_main};

    struct Pty {
        master: fs::File,
        original: OwnedFd,
        _slave: OwnedFd,
    }
    impl Pty {
        fn new() -> Self {
            let (mut master, mut slave) = (0, 0);
            assert_eq!(
                unsafe {
                    libc::openpty(
                        &mut master,
                        &mut slave,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                        std::ptr::null(),
                    )
                },
                0
            );
            let mut mode = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(unsafe { libc::tcgetattr(slave, mode.as_mut_ptr()) }, 0);
            let mut mode = unsafe { mode.assume_init() };
            unsafe { libc::cfmakeraw(&mut mode) };
            assert_eq!(unsafe { libc::tcsetattr(slave, libc::TCSANOW, &mode) }, 0);
            let saved = unsafe { libc::dup(0) };
            assert!(saved >= 0);
            assert_eq!(unsafe { libc::dup2(slave, 0) }, 0);
            Self {
                master: unsafe { fs::File::from_raw_fd(master) },
                original: unsafe { OwnedFd::from_raw_fd(saved) },
                _slave: unsafe { OwnedFd::from_raw_fd(slave) },
            }
        }
        fn send(&mut self, bytes: &[u8]) {
            self.master.write_all(bytes).unwrap();
        }
        fn empty(&self) -> bool {
            let mut count = 0;
            assert_eq!(unsafe { libc::ioctl(0, libc::FIONREAD, &mut count) }, 0);
            count == 0
        }
    }
    impl Drop for Pty {
        fn drop(&mut self) {
            assert_eq!(unsafe { libc::dup2(self.original.as_raw_fd(), 0) }, 0);
        }
    }
    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(Instant::now() < deadline, "ownership barrier timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn child_pid() -> i32 {
        let pid = std::process::id();
        let values = fs::read_to_string(format!("/proc/self/task/{pid}/children")).unwrap();
        let ids: Vec<_> = values.split_whitespace().collect();
        assert_eq!(ids.len(), 1);
        ids[0].parse().unwrap()
    }
    fn key(input: &Input, c: char) {
        let event = input.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
        assert!(
            matches!(event,Event::Key(k) if k.code==KeyCode::Char(c)),
            "unexpected input after ownership transition"
        );
    }
    pub fn main() {
        let mut args = std::env::args().skip(1);
        if args.next().as_deref() == Some(WORKER_ARGUMENT) {
            let parent = args.next().unwrap().parse().unwrap();
            std::process::exit(if worker_main(parent).is_ok() { 0 } else { 1 });
        }
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "ownership tests must run unprivileged"
        );
        let executable = std::env::current_exe().unwrap();
        if std::env::args().nth(1).as_deref() == Some("--crash-parent") {
            let _input = Input::start(&executable).unwrap();
            println!("{}", child_pid());
            io::stdout().flush().unwrap();
            loop {
                thread::park();
            }
        }
        let mut pty = Pty::new();
        let mut input = Input::start(&executable).unwrap();
        pty.send(b"a");
        key(&input, 'a');
        let original_reader = child_pid();
        // Deliberately leave an incomplete bracketed paste INSIDE crossterm.
        // FIONREAD proves it consumed the bytes, not merely a kernel flush.
        pty.send(b"\x1b[200~pre-handoff-partial");
        thread::sleep(Duration::from_millis(50));
        until(|| pty.empty());
        let paused = input.pause().unwrap();
        assert!(!fs::exists(format!("/proc/{original_reader}")).unwrap());
        assert!(input.recv_timeout(Duration::ZERO).is_err());
        // A distinct reader owns the private PTY only AFTER the proof exists.
        let synthetic = b"ownership-fixture-response\r";
        pty.send(synthetic);
        let mut response = vec![0; synthetic.len()];
        fs::File::open("/proc/self/fd/0")
            .unwrap()
            .read_exact(&mut response)
            .unwrap();
        assert_eq!(response, synthetic);
        pty.send(b"leftover-response-typeahead\x1b[201~");
        input.resume(paused).unwrap();
        assert_ne!(child_pid(), original_reader);
        pty.send(b"z");
        key(&input, 'z');
        assert!(input.recv_timeout(Duration::from_millis(80)).is_err());
        println!(
            "PASS partial-parser retirement, exclusive PTY handoff and fresh-parser/typeahead reset"
        );

        // Backpressure must not trap pause behind a full event channel or pipe.
        pty.send(&vec![b'q'; 2048]);
        thread::sleep(Duration::from_millis(80));
        let proof = input.pause().unwrap();
        input.resume(proof).unwrap();
        pty.send(b"y");
        key(&input, 'y');
        assert!(input.recv_timeout(Duration::from_millis(80)).is_err());
        let proof = input.pause().unwrap();
        drop(proof);
        assert!(input.recv_timeout(Duration::ZERO).is_err());
        assert!(
            fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id()))
                .unwrap()
                .trim()
                .is_empty()
        );
        println!("PASS full-queue retirement and dropped-proof fail-closed behavior");
        drop(input);

        // Adopt/reap the fixture's orphan; never depend on container PID1 behavior.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
        let mut parent = Command::new(&executable)
            .arg("--crash-parent")
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        use std::io::BufRead;
        io::BufReader::new(parent.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let orphan = line.trim().parse::<i32>().unwrap();
        parent.kill().unwrap();
        parent.wait().unwrap();
        let mut status = 0;
        until(|| unsafe { libc::waitpid(orphan, &mut status, libc::WNOHANG) } == orphan);
        assert!(libc::WIFSIGNALED(status));
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 0) }, 0);
        println!("PASS editor-parent crash kills the reader without a keypress");

        let mut input = Input::start(&executable).unwrap();
        let proof = input.pause().unwrap();
        assert_eq!(unsafe { libc::close(0) }, 0);
        assert!(input.resume(proof).is_err());
        assert!(input.recv_timeout(Duration::ZERO).is_err());
        assert_eq!(unsafe { libc::dup2(pty._slave.as_raw_fd(), 0) }, 0);
        assert!(Input::start(std::path::Path::new("/bin/false")).is_err());
        println!("PASS terminal flush and startup failures cannot resume input");
    }
}
#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}
#[cfg(not(target_os = "linux"))]
fn main() {}
