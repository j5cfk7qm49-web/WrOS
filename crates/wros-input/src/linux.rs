use crossterm::event::{self, Event, KeyCode};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

pub const WORKER_ARGUMENT: &str = "--wros-input-reader";
const MAX_FRAME: usize = 2 * 1024 * 1024;
const QUEUE: usize = 32;
const DEADLINE: Duration = Duration::from_secs(2);
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

fn failure(message: &'static str) -> io::Error {
    io::Error::other(message)
}

#[derive(Serialize, Deserialize)]
enum Wire {
    Ready { version: u16, pid: u32, uid: u32 },
    Event(Event),
    EventRejected,
}

// Pending events are discarded at ownership boundaries. Never Debug/log them.
struct Message(Option<Wire>);
impl Drop for Message {
    fn drop(&mut self) {
        if let Some(Wire::Event(event)) = &mut self.0 {
            match event {
                Event::Paste(text) => text.zeroize(),
                Event::Key(key) => {
                    if let KeyCode::Char(c) = &mut key.code {
                        c.zeroize();
                    }
                }
                _ => {}
            }
        }
    }
}

fn read_frame(source: &mut impl Read) -> io::Result<Message> {
    let mut header = [0; 4];
    source
        .read_exact(&mut header)
        .map_err(|_| failure("Input reader disconnected"))?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(failure("Invalid input frame size"));
    }
    let mut bytes = Zeroizing::new(vec![0; size]);
    source
        .read_exact(&mut bytes)
        .map_err(|_| failure("Input reader frame truncated"))?;
    let wire =
        serde_json::from_slice(&bytes).map_err(|_| failure("Invalid input reader message"))?;
    Ok(Message(Some(wire)))
}

fn encode(message: &Message) -> io::Result<Zeroizing<Vec<u8>>> {
    struct Limited<'a>(&'a mut Vec<u8>);
    impl Write for Limited<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > MAX_FRAME {
                return Err(failure("Input event exceeds transport limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(4096));
    serde_json::to_writer(
        Limited(&mut bytes),
        message
            .0
            .as_ref()
            .ok_or_else(|| failure("Missing input message"))?,
    )
    .map_err(|_| failure("Input serialization failed or event too large"))?;
    Ok(bytes)
}
fn write_bytes(output: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|_| failure("Input transport closed"))?;
    output
        .write_all(bytes)
        .map_err(|_| failure("Input transport closed"))?;
    output
        .flush()
        .map_err(|_| failure("Input transport flush failed"))
}
fn write_frame(output: &mut impl Write, message: &Message) -> io::Result<()> {
    write_bytes(output, &encode(message)?)
}

/// Private re-exec entry, called before any editor or project initialization.
/// The caller sets terminal modes. This process only reads and transports input.
pub fn worker_main(expected_parent: u32) -> io::Result<()> {
    std::panic::set_hook(Box::new(|_| {}));
    let core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // Never leave a reader consuming a subsequent login's input after editor
    // death. Check the parent again after arming the death signal (fork race).
    if unsafe { libc::geteuid() } == 0
        || expected_parent <= 1
        || unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0
        || unsafe { libc::getppid() } as u32 != expected_parent
        || unsafe { libc::setrlimit(libc::RLIMIT_CORE, &core) } != 0
        || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_NAME, c"wros-input".as_ptr()) } != 0
        || unsafe { libc::isatty(libc::STDIN_FILENO) } != 1
        || unsafe { libc::isatty(libc::STDOUT_FILENO) } == 1
    {
        return Err(failure("Invalid input reader context"));
    }
    let mut output = io::stdout().lock();
    write_frame(
        &mut output,
        &Message(Some(Wire::Ready {
            version: 1,
            pid: std::process::id(),
            uid: unsafe { libc::geteuid() },
        })),
    )?;
    loop {
        // crossterm's complete AND partial parser state belongs exclusively to
        // this disposable address space. No terminal/event calls on other threads.
        let event = event::read().map_err(|_| failure("Terminal input failed"))?;
        let message = Message(Some(Wire::Event(event)));
        let bytes = match encode(&message) {
            Ok(bytes) => bytes,
            Err(_) => {
                write_frame(&mut output, &Message(Some(Wire::EventRejected)))?;
                return Err(failure("Input event rejected"));
            }
        };
        write_bytes(&mut output, &bytes)?;
    }
}

struct Running {
    child: Child,
    messages: Option<mpsc::Receiver<io::Result<Message>>>,
    reader: Option<JoinHandle<()>>,
}
impl Running {
    fn retire(&mut self) -> io::Result<()> {
        // Cancel a blocked bounded send and wipe the queued event payloads.
        self.messages.take();
        if self
            .child
            .try_wait()
            .map_err(|_| failure("Input reader wait failed"))?
            .is_none()
        {
            let _ = self.child.kill();
        }
        let started = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < DEADLINE => thread::sleep(Duration::from_millis(5)),
                _ => return Err(failure("Input reader exit unacknowledged")),
            }
        }
        // Its only writer is now reaped, so EOF releases even a partial frame.
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| failure("Input transport cleanup failed"))?;
        }
        Ok(())
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.retire();
    }
}

enum State {
    Active(Running),
    Paused(u64),
    Failed,
}

/// Opaque, single-use proof that this Input's old reader was reaped and its
/// transport/queued data discarded. Dropping it NEVER resumes input.
pub struct Paused {
    owner: u64,
    epoch: u64,
}
impl Drop for Paused {
    fn drop(&mut self) {
        self.owner.zeroize();
        self.epoch.zeroize();
    }
}

/// Kept on its owning controller thread: Linux's parent-death signal follows
/// that spawning thread. Moving it to a short-lived worker is prohibited.
/// ```compile_fail
/// use wros_input::Input;
/// fn transfer(input: Input) { std::thread::spawn(move || drop(input)); }
/// ```
pub struct Input {
    executable: PathBuf,
    owner: u64,
    epoch: u64,
    state: State,
    _controller_thread: PhantomData<Rc<()>>,
}
impl Input {
    /// Re-execute the same unprivileged program in its private reader mode.
    pub fn start(executable: &Path) -> io::Result<Self> {
        let mut input = Self {
            executable: executable.into(),
            owner: NEXT_OWNER.fetch_add(1, Ordering::Relaxed),
            epoch: 0,
            state: State::Failed,
            _controller_thread: PhantomData,
        };
        input.spawn()?;
        Ok(input)
    }
    fn spawn(&mut self) -> io::Result<()> {
        let mut child = Command::new(&self.executable)
            .args([WORKER_ARGUMENT, &std::process::id().to_string()])
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LANG", "C.UTF-8")
            .env("TERM", "xterm-256color")
            .current_dir("/")
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| failure("Input reader unavailable"))?;
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| failure("Input reader transport unavailable"))?;
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        let reader = thread::spawn(move || {
            loop {
                let message = read_frame(&mut output);
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });
        let running = Running {
            child,
            messages: Some(receiver),
            reader: Some(reader),
        };
        let ready = running
            .messages
            .as_ref()
            .unwrap()
            .recv_timeout(DEADLINE)
            .map_err(|_| failure("Input reader startup unacknowledged"))??;
        if !matches!(ready.0, Some(Wire::Ready { version: 1, pid, uid }) if pid == running.child.id() && uid == unsafe { libc::geteuid() })
        {
            return Err(failure("Invalid input reader startup"));
        }
        self.state = State::Active(running);
        Ok(())
    }
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<io::Result<Event>, mpsc::RecvTimeoutError> {
        let State::Active(running) = &self.state else {
            return Err(mpsc::RecvTimeoutError::Disconnected);
        };
        let message = running.messages.as_ref().unwrap().recv_timeout(timeout)?;
        Ok(message.and_then(|mut message| match message.0.take() {
            Some(Wire::Event(event)) => Ok(event),
            Some(Wire::EventRejected) => Err(failure(
                "Input event exceeds 2 MiB transport limit; prior edits are being saved",
            )),
            _ => Err(failure("Unexpected input reader message")),
        }))
    }
    /// No credential reader may start until this returns its proof successfully.
    /// On error the object remains failed, with no automatic recovery/restart.
    pub fn pause(&mut self) -> io::Result<Paused> {
        let State::Active(mut running) = std::mem::replace(&mut self.state, State::Failed) else {
            return Err(failure("Input is not active"));
        };
        running.retire()?;
        if unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) } != 0 {
            return Err(failure("Terminal typeahead flush failed"));
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or_else(|| failure("Input epoch exhausted"))?;
        self.state = State::Paused(self.epoch);
        Ok(Paused {
            owner: self.owner,
            epoch: self.epoch,
        })
    }
    /// Call only after the independent terminal owner has exited. Kernel
    /// typeahead is discarded BEFORE a new parser process can read anything.
    pub fn resume(&mut self, proof: Paused) -> io::Result<()> {
        if !matches!(self.state, State::Paused(epoch) if epoch == proof.epoch && self.owner == proof.owner)
        {
            return Err(failure("Input ownership proof mismatch"));
        }
        self.state = State::Failed;
        if unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) } != 0 {
            return Err(failure("Terminal typeahead flush failed"));
        }
        self.spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_frames_fail_before_body_allocation_with_fixed_errors() {
        for size in [0_u32, MAX_FRAME as u32 + 1, u32::MAX] {
            let error = read_frame(&mut size.to_le_bytes().as_slice())
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "Invalid input frame size");
        }
        for data in [vec![], vec![2, 0, 0, 0, b'{'], vec![2, 0, 0, 0, b'{', b'x']] {
            assert!(read_frame(&mut data.as_slice()).is_err());
        }
    }
    #[test]
    fn paste_unicode_keys_and_control_events_round_trip() {
        for event in [
            Event::Paste("λ prose Ω".into()),
            Event::Resize(80, 24),
            Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char('é'),
                crossterm::event::KeyModifiers::CONTROL,
            )),
            Event::FocusLost,
        ] {
            let mut bytes = Vec::new();
            write_frame(&mut bytes, &Message(Some(Wire::Event(event.clone())))).unwrap();
            let mut decoded = read_frame(&mut bytes.as_slice()).unwrap();
            let Some(Wire::Event(actual)) = decoded.0.take() else {
                panic!("wrong frame");
            };
            assert_eq!(actual, event);
        }
    }
    #[test]
    fn oversized_paste_is_rejected_without_an_unbounded_serialized_copy() {
        let mut bytes = Vec::new();
        assert!(
            write_frame(
                &mut bytes,
                &Message(Some(Wire::Event(Event::Paste("x".repeat(MAX_FRAME)))))
            )
            .is_err()
        );
        assert!(bytes.is_empty());
    }
    #[test]
    fn dropped_proof_and_foreign_proof_cannot_restart_a_reader() {
        let mut input = Input {
            executable: PathBuf::from("/does/not/exist"),
            owner: 1,
            epoch: 3,
            state: State::Paused(3),
            _controller_thread: PhantomData,
        };
        let proof = Paused { owner: 1, epoch: 3 };
        drop(proof);
        assert!(matches!(input.state, State::Paused(3)));
        assert!(input.resume(Paused { owner: 2, epoch: 3 }).is_err());
        assert!(input.resume(Paused { owner: 1, epoch: 2 }).is_err());
        assert!(input.pause().is_err());
        assert!(matches!(input.state, State::Failed));
    }
}
