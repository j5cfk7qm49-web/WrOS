//! Only wl_output and gamma-control are bound. No seat or surface is opened.
use super::{Screen, ramps};
use anyhow::{Result, bail};
#[cfg(test)]
use std::os::unix::net::UnixStream;
use std::{
    fs::File,
    io::{Seek, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_callback, wl_output, wl_registry},
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1 as manager, zwlr_gamma_control_v1 as gamma,
};
use wros_sys::display::Warmth;

struct Output {
    global: u32,
    object: wl_output::WlOutput,
    control: Option<gamma::ZwlrGammaControlV1>,
    points: Option<u32>,
}

#[derive(Default)]
struct State {
    manager: Option<(u32, manager::ZwlrGammaControlManagerV1)>,
    outputs: Vec<Output>,
    settled: bool,
    failed: Option<&'static str>,
    done: u64,
}

pub(super) struct Protocol {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    serial: u64,
}

impl Protocol {
    pub(super) fn connect() -> Result<Self> {
        if unsafe { libc::geteuid() } == 0 {
            bail!("screen colours must run as the writer");
        }
        Self::new(Connection::connect_to_env()?)
    }

    #[cfg(test)]
    pub(super) fn from_socket(stream: UnixStream) -> Result<Self> {
        Self::new(Connection::from_socket(stream)?)
    }

    fn new(connection: Connection) -> Result<Self> {
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut this = Self {
            connection,
            queue,
            state: State::default(),
            serial: 0,
        };
        this.sync()?;
        if this.state.manager.is_none() || this.state.outputs.is_empty() {
            bail!("this display does not offer screen warmth");
        }
        this.acquire()?;
        this.clear();
        this.sync()?;
        this.state.settled = true;
        Ok(this)
    }

    fn sync(&mut self) -> Result<()> {
        self.serial += 1;
        let serial = self.serial;
        self.connection.display().sync(&self.queue.handle(), serial);
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.state.done < serial {
            if Instant::now() >= deadline {
                bail!("the display did not answer the colour request");
            }
            self.wait(-1, Some(deadline))?;
        }
        self.check()
    }

    pub(super) fn check(&self) -> Result<()> {
        if let Some(reason) = self.state.failed {
            bail!(reason);
        }
        Ok(())
    }

    fn acquire(&mut self) -> Result<()> {
        self.check()?;
        let manager = &self.state.manager.as_ref().unwrap().1;
        let handle = self.queue.handle();
        for output in &mut self.state.outputs {
            if output.control.is_none() {
                output.points = None;
                output.control =
                    Some(manager.get_gamma_control(&output.object, &handle, output.global));
            }
        }
        self.sync()?;
        if self
            .state
            .outputs
            .iter()
            .any(|output| output.points.is_none())
        {
            bail!("the display did not provide colour curve sizes");
        }
        Ok(())
    }

    fn clear(&mut self) {
        for output in &mut self.state.outputs {
            if let Some(control) = output.control.take() {
                control.destroy();
            }
            output.points = None;
        }
    }

    /// Poll the protocol and the actor's wake pipe. With no preview there is no
    /// periodic timer; the compositor or a typed request wakes this connection.
    pub(super) fn wait(&mut self, wake: i32, deadline: Option<Instant>) -> Result<()> {
        self.queue.dispatch_pending(&mut self.state)?;
        self.check()?;
        self.connection.flush()?;
        let Some(read) = self.connection.prepare_read() else {
            return Ok(());
        };
        let mut fds = [
            libc::pollfd {
                fd: self.connection.as_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout = deadline.map_or(-1, |end| {
            end.saturating_duration_since(Instant::now())
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128) as i32
        });
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout) };
        if rc < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(error.into());
        }
        if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            bail!("the display connection ended; warmth was released");
        }
        if fds[0].revents & libc::POLLIN != 0 {
            read.read()?;
        } else {
            drop(read);
        }
        self.queue.dispatch_pending(&mut self.state)?;
        self.check()
    }
}

impl Screen for Protocol {
    fn apply(&mut self, choice: Warmth) -> Result<()> {
        choice.validate()?;
        self.check()?;
        if !choice.enabled || choice.strength == 0 || choice.kelvin == 6500 {
            self.clear();
            return self.sync();
        }
        let changed = (|| {
            self.acquire()?;
            for output in &self.state.outputs {
                let values = ramps(output.points.unwrap(), choice)?;
                let fd = unsafe {
                    libc::memfd_create(
                        c"wros-gamma".as_ptr(),
                        libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
                    )
                };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                let mut file = unsafe { File::from_raw_fd(fd) };
                let bytes: Vec<_> = values.into_iter().flat_map(u16::to_ne_bytes).collect();
                file.write_all(&bytes)?;
                file.rewind()?;
                let seals = libc::F_SEAL_WRITE
                    | libc::F_SEAL_SHRINK
                    | libc::F_SEAL_GROW
                    | libc::F_SEAL_SEAL;
                if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                output.control.as_ref().unwrap().set_gamma(file.as_fd());
            }
            // Protocol acceptance only: gamma-control provides no ramp readback
            // or presentation acknowledgement. Never describe this as either.
            self.sync()
        })();
        if changed.is_err() {
            self.state
                .failed
                .get_or_insert("screen warmth failed and was released; reopen the writing session");
            self.release();
        }
        changed
    }

    fn release(&mut self) {
        self.clear();
        let _ = self.connection.flush();
    }
}

impl Drop for Protocol {
    fn drop(&mut self) {
        self.release();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name, interface, ..
            } if interface == "wl_output" => {
                if state.settled || state.outputs.len() >= 32 {
                    state.failed =
                        Some("the displays changed; reopen the writing session to check warmth");
                    return;
                }
                state.outputs.push(Output {
                    global: name,
                    object: registry.bind(name, 1, handle, ()),
                    control: None,
                    points: None,
                });
            }
            wl_registry::Event::Global {
                name, interface, ..
            } if interface == "zwlr_gamma_control_manager_v1" => {
                if state.manager.is_some() {
                    state.failed = Some("the display offered conflicting colour controls");
                } else {
                    state.manager = Some((name, registry.bind(name, 1, handle, ())));
                }
            }
            wl_registry::Event::GlobalRemove { name }
                if state.outputs.iter().any(|o| o.global == name)
                    || state.manager.as_ref().is_some_and(|m| m.0 == name) =>
            {
                state.failed = Some("a display was disconnected; warmth was released");
            }
            _ => {}
        }
    }
}

impl Dispatch<gamma::ZwlrGammaControlV1, u32> for State {
    fn event(
        state: &mut Self,
        control: &gamma::ZwlrGammaControlV1,
        event: gamma::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            gamma::Event::GammaSize { size } if (2..=4096).contains(&size) => {
                if let Some(output) = state.outputs.iter_mut().find(|o| o.global == *name) {
                    output.points = Some(size);
                }
            }
            gamma::Event::Failed => {
                control.destroy();
                if let Some(output) = state.outputs.iter_mut().find(|o| o.global == *name) {
                    output.control = None;
                }
                state.failed = Some(
                    "this display cannot accept screen warmth, or another colour tool owns it",
                );
            }
            _ => {
                state.failed = Some("the display offered an unsupported colour curve size");
            }
        }
    }
}
impl Dispatch<wl_callback::WlCallback, u64> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        serial: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.done = *serial;
    }
}
delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore manager::ZwlrGammaControlManagerV1);
