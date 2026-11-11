use super::{
    Screen, Transaction,
    process::{Process, read_command},
    protocol::Protocol,
};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{MetadataExt, OpenOptionsExt},
            net::UnixStream,
        },
    },
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    time::{Duration, Instant},
};
use wros_sys::{
    Backend, Reply, Request,
    display::{Status, Warmth},
    linux::{Invocation, Linux, ProcessRunner, Runner},
};

const CONFIG: &str = "/etc/wros/display.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    version: u8,
    enabled: bool,
    kelvin: u16,
    strength: u8,
    generation: String,
}

struct Saved {
    choice: Warmth,
    generation: String,
    stamp: String,
}
impl Saved {
    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 4096 {
            bail!("saved screen settings are too large; they were kept");
        }
        let value: Preference = serde_json::from_slice(bytes)?;
        if value.version != 1 {
            bail!("saved screen settings need a newer WrOS; they were kept");
        }
        let choice = Warmth {
            enabled: value.enabled,
            kelvin: value.kelvin,
            strength: value.strength,
        };
        choice.validate()?;
        if value.generation != "initial" {
            wros_sys::display::validate_token(&value.generation)?;
        }
        Ok(Self {
            choice,
            generation: value.generation,
            stamp: format!("{:x}", Sha256::digest(bytes)),
        })
    }

    fn read() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(CONFIG);
        let file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    choice: Warmth {
                        enabled: false,
                        ..Warmth::default()
                    },
                    generation: "initial".into(),
                    stamp: "missing".into(),
                });
            }
            Err(error) => return Err(error).context("reading /etc/wros/display.json"),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            bail!("saved screen settings are not protected; they were kept");
        }
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        Self::decode(&bytes)
    }

    fn persist(&mut self, choice: Warmth, token: &str) -> Result<()> {
        let command = Invocation {
            program: "/usr/bin/pkexec",
            args: vec![
                "--disable-internal-agent".into(),
                "/usr/libexec/wros-display".into(),
                "compositor-save".into(),
                if choice.enabled { "on" } else { "off" }.into(),
                choice.kelvin.to_string(),
                choice.strength.to_string(),
                token.into(),
                self.stamp.clone(),
            ],
            timeout: Duration::from_secs(8),
        };
        let reply = ProcessRunner.run(&command);
        // A lost reply after fsync must not turn a committed generation into a
        // failed Keep. The root-owned receipt is the durable source of truth.
        let current = Self::read()?;
        if current.generation == token && current.choice == choice {
            *self = current;
            return Ok(());
        }
        reply?;
        bail!("the screen setting was not saved; the previous choice was restored")
    }
}

struct Message {
    request: Request,
    reply: SyncSender<Result<Reply, String>>,
    expires: Instant,
}
struct Control {
    tx: SyncSender<Message>,
    wake: UnixStream,
}
impl Control {
    fn start() -> Result<Self> {
        let (wake, listener) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        listener.set_nonblocking(true)?;
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("screen-colours".into())
            .spawn(move || serve(rx, listener))?;
        Ok(Self { tx, wake })
    }
    fn request(&mut self, request: Request) -> Result<Reply> {
        let (reply, response) = mpsc::sync_channel(1);
        self.tx.try_send(Message {
            request,
            reply,
            expires: Instant::now() + Duration::from_secs(4),
        })?;
        self.wake.write_all(&[1])?;
        response
            .recv_timeout(Duration::from_secs(25))?
            .map_err(anyhow::Error::msg)
    }
}

pub(crate) fn worker_main() -> Result<()> {
    // This branch is entered before loading any project, theme or input reader.
    if unsafe { libc::geteuid() } == 0 {
        bail!("screen colours must run as the writer");
    }
    unsafe {
        libc::prctl(libc::PR_SET_NAME, c"wros-colours".as_ptr(), 0, 0, 0);
    }
    let mut actor = Control::start()?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    while let Some(command) = read_command(&mut input)? {
        let reply = actor
            .request(command.request())
            .map_err(|error| format!("{error:#}").chars().take(512).collect::<String>());
        serde_json::to_writer(&mut output, &reply)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

struct Compositor {
    linux: Linux,
    colours: Result<Process>,
}
pub(crate) fn backend() -> Box<dyn Backend> {
    Box::new(Compositor {
        linux: Linux::default(),
        colours: Process::start(),
    })
}
impl Backend for Compositor {
    fn request(&mut self, request: Request) -> Result<Reply> {
        if !matches!(
            request,
            Request::DisplayStatus
                | Request::DisplayPreview(_)
                | Request::DisplayConfirm(_)
                | Request::DisplayCancel(_)
        ) {
            return self.linux.request(request);
        }
        match &mut self.colours {
            Ok(control) => {
                let status = matches!(request, Request::DisplayStatus);
                match control.request(request) {
                    Err(error) if status => Ok(unavailable(Saved::read(), error.to_string())),
                    result => result,
                }
            }
            Err(error) if matches!(request, Request::DisplayStatus) => {
                Ok(unavailable(Saved::read(), error.to_string()))
            }
            Err(error) => bail!("screen warmth unavailable: {error}"),
        }
    }
}

fn unavailable(saved: Result<Saved>, message: String) -> Reply {
    Reply::DisplayStatus(Status {
        warmth: saved.map(|saved| saved.choice).unwrap_or(Warmth {
            enabled: false,
            ..Warmth::default()
        }),
        available: false,
        applied: false,
        message: message.chars().take(512).collect(),
    })
}

fn token() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.into_iter().map(|b| format!("{b:02x}")).collect())
}

fn serve(requests: Receiver<Message>, mut wake: UnixStream) {
    let opened = (|| -> Result<_> {
        match std::env::var("WROS_COMPOSITOR_WARMTH").as_deref() {
            Ok("ready") => (),
            Ok("unsupported-cage") => bail!(
                "Debian 13's Cage cannot apply screen warmth. Display warmth is available in the native DRM session on supported hardware."
            ),
            _ => bail!("screen warmth could not take over safely; writing is available"),
        }
        let saved = Saved::read()?;
        let mut screen = Protocol::connect()?;
        screen.apply(saved.choice)?;
        let choice = saved.choice;
        Ok((
            saved,
            Transaction {
                screen,
                saved: choice,
                pending: None,
            },
        ))
    })();
    let (mut active, mut failure) = match opened {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };
    loop {
        loop {
            let message = match requests.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            };
            let preview = matches!(message.request, Request::DisplayPreview(_));
            let result = if Instant::now() >= message.expires {
                Err(anyhow::anyhow!(
                    "the colour request expired before it started"
                ))
            } else if let Some((saved, trial)) = &mut active {
                handle(saved, trial, message.request)
            } else if matches!(message.request, Request::DisplayStatus) {
                Ok(unavailable(
                    Saved::read(),
                    failure.clone().unwrap_or_default(),
                ))
            } else {
                Err(anyhow::anyhow!(failure.clone().unwrap_or_default()))
            };
            if message
                .reply
                .send(result.map_err(|e| format!("{e:#}")))
                .is_err()
                && preview
                && let Some((_, trial)) = &mut active
            {
                trial.pending = None;
                if let Err(error) = trial.screen.apply(trial.saved) {
                    failure = Some(error.to_string());
                }
            }
        }
        let waited = if let Some((_, trial)) = &mut active {
            trial.expire(Instant::now()).and_then(|()| {
                trial
                    .screen
                    .wait(wake.as_raw_fd(), trial.pending.as_ref().map(|p| p.deadline))
            })
        } else {
            wait_only(wake.as_raw_fd())
        };
        if let Err(error) = waited {
            failure = Some(format!("{error:#}"));
            // Dropping every control returns all outputs to the compositor's
            // original curves, including after a partial multi-output failure.
            active = None;
        }
        let mut bytes = [0; 64];
        loop {
            match wake.read(&mut bytes) {
                Ok(0) => return,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => return,
            }
        }
    }
}

fn wait_only(fd: i32) -> Result<()> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut poll, 1, -1) } < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.into());
        }
    }
    Ok(())
}

fn handle(saved: &mut Saved, trial: &mut Transaction<Protocol>, request: Request) -> Result<Reply> {
    trial.screen.check()?;
    trial.expire(Instant::now())?;
    match request {
        Request::DisplayStatus => Ok(Reply::DisplayStatus(Status {
            warmth: trial.saved,
            available: true,
            applied: true,
            message: "Screen warmth uses this session. Warm colours may change calibrated colours."
                .into(),
        })),
        Request::DisplayPreview(choice) => {
            if Saved::read()?.stamp != saved.stamp {
                bail!("saved screen settings changed; reopen the writing session");
            }
            let token = token()?;
            trial.preview(choice, token.clone(), Instant::now())?;
            let seconds = trial
                .pending
                .as_ref()
                .unwrap()
                .deadline
                .saturating_duration_since(Instant::now())
                .as_secs();
            if seconds == 0 {
                trial.pending = None;
                trial.screen.apply(trial.saved)?;
                bail!("the display took too long; the previous choice was restored");
            }
            Ok(Reply::DisplayPreview { token, seconds })
        }
        Request::DisplayConfirm(token) => {
            trial.resolve(&token, true, Instant::now(), |choice, token| {
                saved.persist(choice, token)
            })?;
            Ok(Reply::Message("Screen warmth saved.".into()))
        }
        Request::DisplayCancel(token) => {
            wros_sys::display::validate_token(&token)?;
            if trial.pending.is_some() {
                trial.resolve(&token, false, Instant::now(), |_, _| unreachable!())?;
            }
            Ok(Reply::Message("Screen warmth preview finished.".into()))
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_unavailable_display_still_reports_the_confirmed_warmth_choice() {
        let saved = Saved::decode(
            br#"{"version":1,"enabled":true,"kelvin":1800,"strength":75,"generation":"initial"}"#,
        )
        .unwrap();
        let Reply::DisplayStatus(status) = unavailable(Ok(saved), "No supported display".into())
        else {
            panic!("display status");
        };
        assert_eq!(
            status.warmth,
            Warmth {
                enabled: true,
                kelvin: 1800,
                strength: 75
            }
        );
        assert!(!status.available && !status.applied);
    }
    #[test]
    fn preference_versions_duplicates_bounds_and_unknown_fields_are_preserved() {
        let valid =
            br#"{"version":1,"enabled":true,"kelvin":2200,"strength":75,"generation":"initial"}"#;
        let saved = Saved::decode(valid).unwrap();
        assert_eq!(saved.choice.kelvin, 2200);
        assert_eq!(saved.stamp.len(), 64);
        for invalid in [
            String::from_utf8_lossy(valid).replace("\"version\":1", "\"version\":2"),
            String::from_utf8_lossy(valid).replace("2200", "999"),
            String::from_utf8_lossy(valid).replace("75", "101"),
            String::from_utf8_lossy(valid).replace("initial", "bad-token"),
            String::from_utf8_lossy(valid).replace("\"version\":1", "\"version\":1,\"version\":1"),
            String::from_utf8_lossy(valid).replace("\"version\":1", "\"version\":1,\"future\":0"),
            " ".repeat(4097),
        ] {
            assert!(Saved::decode(invalid.as_bytes()).is_err());
        }
    }
}
