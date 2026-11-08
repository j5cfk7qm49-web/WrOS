//! A bounded colour-only pipe. The child starts before any project or input setup.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Read, Write},
    os::fd::AsRawFd,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
use wros_sys::{Reply, Request, display::Warmth};

pub(crate) const ARGUMENT: &str = "--wros-colour-worker";
#[derive(Serialize, Deserialize)]
pub(super) enum Colour {
    Status,
    Preview(Warmth),
    Confirm(String),
    Cancel(String),
}
impl Colour {
    fn from_request(request: Request) -> Result<Self> {
        Ok(match request {
            Request::DisplayStatus => Self::Status,
            Request::DisplayPreview(choice) => {
                choice.validate()?;
                Self::Preview(choice)
            }
            Request::DisplayConfirm(token) => {
                wros_sys::display::validate_token(&token)?;
                Self::Confirm(token)
            }
            Request::DisplayCancel(token) => {
                wros_sys::display::validate_token(&token)?;
                Self::Cancel(token)
            }
            _ => bail!("the colour worker accepts only screen warmth requests"),
        })
    }
    pub(super) fn request(self) -> Request {
        match self {
            Self::Status => Request::DisplayStatus,
            Self::Preview(choice) => Request::DisplayPreview(choice),
            Self::Confirm(token) => Request::DisplayConfirm(token),
            Self::Cancel(token) => Request::DisplayCancel(token),
        }
    }
}

pub(super) fn read_command(source: &mut impl BufRead) -> Result<Option<Colour>> {
    let mut bytes = Vec::new();
    source.take(4097).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > 4096 || !bytes.ends_with(b"\n") {
        bail!("invalid colour request length");
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub(super) struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
}
impl Process {
    pub(super) fn start() -> Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(ARGUMENT)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8");
        for name in [
            "XDG_RUNTIME_DIR",
            "WAYLAND_DISPLAY",
            "WROS_COMPOSITOR_WARMTH",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let flags = unsafe { libc::fcntl(output.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(output.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                < 0
        {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        Ok(Self {
            child,
            input: Some(input),
            output,
        })
    }

    pub(super) fn request(&mut self, request: Request) -> Result<Reply> {
        let colour = Colour::from_request(request)?;
        let mut bytes = serde_json::to_vec(&colour)?;
        bytes.push(b'\n');
        self.input.as_mut().unwrap().write_all(&bytes)?;
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 1024];
            match self.output.read(&mut chunk) {
                Ok(0) => bail!("the colour worker stopped; no change was confirmed"),
                Ok(count) => {
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.len() > 4096 {
                        bail!("the colour reply was too large");
                    }
                    if bytes.ends_with(b"\n") {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        bail!("the colour worker did not answer");
                    }
                    let mut fd = libc::pollfd {
                        fd: self.output.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let timeout = deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(i32::MAX as u128) as i32;
                    if unsafe { libc::poll(&mut fd, 1, timeout) } < 0 {
                        let error = std::io::Error::last_os_error();
                        if error.kind() != std::io::ErrorKind::Interrupted {
                            return Err(error.into());
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        let reply: Result<Reply, String> = serde_json::from_slice(&bytes)?;
        let reply = reply.map_err(anyhow::Error::msg)?;
        match (&colour, &reply) {
            (Colour::Status, Reply::DisplayStatus(status)) => status.validate()?,
            (Colour::Preview(_), Reply::DisplayPreview { token, seconds })
                if (1..=15).contains(seconds) =>
            {
                wros_sys::display::validate_token(token)?
            }
            (Colour::Confirm(_) | Colour::Cancel(_), Reply::Message(message))
                if message.len() <= 2048 => {}
            _ => bail!("the colour worker returned an unrelated reply"),
        }
        Ok(reply)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.input = None;
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
