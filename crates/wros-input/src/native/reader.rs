//! libinput/XKB and their device descriptors stay in this one reader process.

use super::{Keyboard, Layout, NativeConfig, Pointer, seat};
use crossterm::event::{Event, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use input::{
    DeviceCapability, Libinput,
    event::{
        DeviceEvent, EventTrait,
        keyboard::{KeyState, KeyboardEvent, KeyboardEventTrait},
        pointer::{Axis, ButtonState, PointerEvent, PointerScrollEvent},
    },
};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Repeat(Option<(u32, Vec<Event>, Instant)>);

impl Repeat {
    pub(super) fn retire(&mut self) {
        self.0 = None;
    }

    pub(super) fn key(
        &mut self,
        code: u32,
        pressed: bool,
        repeats: bool,
        events: Vec<Event>,
        now: Instant,
    ) {
        if pressed {
            self.0 = (repeats && !events.is_empty()).then_some((
                code,
                events,
                now + Duration::from_millis(400),
            ));
        } else if self.0.as_ref().is_some_and(|(key, _, _)| *key == code) {
            self.retire();
        }
    }

    pub(super) fn due(&mut self, now: Instant) -> Vec<Event> {
        let Some((_, events, next)) = &mut self.0 else {
            return vec![];
        };
        if now < *next {
            return vec![];
        }
        *next = now + Duration::from_millis(33);
        events
            .iter()
            .cloned()
            .map(|mut event| {
                if let Event::Key(key) = &mut event {
                    key.kind = KeyEventKind::Repeat;
                }
                event
            })
            .collect()
    }

    fn timeout(&self) -> i32 {
        self.0.as_ref().map_or(-1, |(_, _, next)| {
            next.saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, i32::MAX as u128) as i32
        })
    }
}

struct Watch(OwnedFd);

struct Stop {
    flag: Arc<AtomicBool>,
    signal: signal_hook::SigId,
}

impl Stop {
    fn new() -> io::Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let signal = signal_hook::flag::register(libc::SIGTERM, flag.clone())?;
        Ok(Self { flag, signal })
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.signal);
    }
}

fn settings_changed(mut bytes: &[u8]) -> io::Result<bool> {
    let invalid = || io::Error::other("Invalid keyboard settings notification");
    let mut changed = false;
    while !bytes.is_empty() {
        // Linux inotify's four 32-bit header fields precede the padded name.
        // Decode bytes directly; the read buffer need not be struct-aligned.
        let header = bytes.get(..16).ok_or_else(invalid)?;
        let mask = u32::from_ne_bytes(header[4..8].try_into().unwrap());
        let length = u32::from_ne_bytes(header[12..16].try_into().unwrap()) as usize;
        let end = 16usize.checked_add(length).ok_or_else(invalid)?;
        let name = bytes.get(16..end).ok_or_else(invalid)?;
        if mask & libc::IN_Q_OVERFLOW != 0 {
            changed = true;
        } else if !name.is_empty() {
            let nul = name.iter().position(|&b| b == 0).ok_or_else(invalid)?;
            changed |= &name[..nul] == b"keyboard";
        }
        bytes = &bytes[end..];
    }
    Ok(changed)
}

impl Watch {
    fn new() -> io::Result<Self> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::other("Keyboard settings watch is unavailable"));
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe {
            libc::inotify_add_watch(
                fd.as_raw_fd(),
                c"/etc/default".as_ptr(),
                libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_DELETE | libc::IN_ATTRIB,
            )
        } < 0
        {
            return Err(io::Error::other("Keyboard settings watch is unavailable"));
        }
        Ok(Self(fd))
    }

    fn changed(&self) -> io::Result<bool> {
        let mut changed = false;
        loop {
            let mut bytes = [0u8; 4096];
            let count =
                unsafe { libc::read(self.0.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
            if count > 0 {
                changed |= settings_changed(&bytes[..count as usize])?;
                continue;
            }
            if count == 0 {
                return Ok(changed);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(changed);
            }
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::other("Keyboard settings watch failed"));
            }
        }
    }
}

fn keyboard() -> io::Result<Keyboard> {
    let mut text = String::new();
    File::open("/etc/default/keyboard")?
        .take(65537)
        .read_to_string(&mut text)?;
    Keyboard::new(&Layout::from_debian(&text)?)
}

pub(crate) struct Reader {
    input: Libinput,
    seat: Rc<seat::Seat>,
    keys: Keyboard,
    pointer: Pointer,
    keyboards: BTreeSet<String>,
    config: NativeConfig,
    watch: Watch,
    stop: Stop,
}

impl Reader {
    pub(crate) fn new(config: NativeConfig) -> io::Result<Self> {
        config.validate()?;
        if !seat::eligible_console(0) {
            return Err(io::Error::other("Native input needs tty1"));
        }
        let keys = keyboard()?;
        let seat = seat::Seat::open()?;
        let mut input = Libinput::new_with_udev(seat::Access(seat.clone()));
        input
            .udev_assign_seat("seat0")
            .map_err(|_| io::Error::other("Native input devices are unavailable"))?;
        input.dispatch()?;
        let mut keyboards = BTreeSet::new();
        for event in input.by_ref() {
            if let input::Event::Device(DeviceEvent::Added(event)) = event
                && event.device().has_capability(DeviceCapability::Keyboard)
            {
                keyboards.insert(event.device().sysname().to_string());
            }
        }
        if keyboards.is_empty() {
            return Err(io::Error::other("Native keyboard is unavailable"));
        }
        let watch = Watch::new()?;
        let stop = Stop::new()?;
        Ok(Self {
            input,
            seat,
            keys,
            pointer: Pointer::new(config),
            keyboards,
            config,
            watch,
            stop,
        })
    }

    pub(crate) fn run(mut self, mut emit: impl FnMut(Event) -> io::Result<()>) -> io::Result<()> {
        let stop = self.stop.flag.clone();
        let result = self.events(&stop, &mut emit);
        // No exit status can acknowledge retirement until libinput closed its
        // descriptors and the synchronous logind ReleaseControl call finished.
        self.input.suspend();
        for _ in self.input.by_ref() {}
        let Self { input, seat, .. } = self;
        drop(input);
        let closed = seat.close();
        if stop.load(Ordering::Relaxed) {
            closed
        } else {
            result.and(closed)
        }
    }

    fn events(
        &mut self,
        stop: &AtomicBool,
        emit: &mut impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut active = true;
        let mut repeat = Repeat::default();
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            self.seat.dispatch()?;
            if self.seat.retire_requested() {
                self.input.suspend();
                for _ in self.input.by_ref() {}
                self.keyboards.clear();
                self.keys.reset();
                self.pointer.retire();
                repeat.retire();
                active = false;
                self.seat.acknowledge_disable()?;
                emit(Event::FocusLost)?;
            }
            if !active && self.seat.enabled() {
                self.input
                    .resume()
                    .map_err(|_| io::Error::other("Native input could not resume"))?;
                self.keys.reset();
                active = true;
                emit(Event::FocusGained)?;
            }
            if self.watch.changed()? {
                self.keys = keyboard()?;
                repeat.retire();
            }
            if active {
                self.input.dispatch()?;
                let mut moved = false;
                while let Some(event) = self.input.next() {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match event {
                        input::Event::Device(DeviceEvent::Added(event)) => {
                            if event.device().has_capability(DeviceCapability::Keyboard) {
                                self.keyboards.insert(event.device().sysname().to_string());
                            }
                        }
                        input::Event::Device(DeviceEvent::Removed(event)) => {
                            self.keyboards.remove(event.device().sysname());
                            self.keys.reset();
                            self.pointer.retire();
                            repeat.retire();
                        }
                        input::Event::Keyboard(KeyboardEvent::Key(event)) => {
                            let pressed = event.key_state() == KeyState::Pressed;
                            if pressed && event.seat_key_count() != 1
                                || !pressed && event.seat_key_count() != 0
                            {
                                continue;
                            }
                            let code = event.key();
                            let events = self.keys.key(code, pressed);
                            if pressed
                                && self
                                    .keys
                                    .modifiers()
                                    .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
                                && let Some(vt) = match code {
                                    59..=68 => Some(code - 58),
                                    87 => Some(11),
                                    88 => Some(12),
                                    _ => None,
                                }
                            {
                                repeat.retire();
                                let _ = self.seat.switch(vt as u16);
                                continue;
                            }
                            let repeats = self
                                .keys
                                .map
                                .key_repeats(xkbcommon::xkb::Keycode::new(code + 8));
                            repeat.key(code, pressed, repeats, events.clone(), Instant::now());
                            for event in events {
                                emit(event)?;
                            }
                        }
                        input::Event::Pointer(event) => match event {
                            PointerEvent::Motion(event) => {
                                self.pointer.move_to(
                                    self.pointer.position.0 + event.dx(),
                                    self.pointer.position.1 + event.dy(),
                                );
                                moved = true;
                            }
                            PointerEvent::MotionAbsolute(event) => {
                                self.pointer.move_to(
                                    event.absolute_x_transformed(self.config.pixels.0 as u32),
                                    event.absolute_y_transformed(self.config.pixels.1 as u32),
                                );
                                moved = true;
                            }
                            PointerEvent::Button(event) => {
                                let button = match event.button() {
                                    0x110 => MouseButton::Left,
                                    0x111 => MouseButton::Right,
                                    0x112 => MouseButton::Middle,
                                    _ => continue,
                                };
                                let pressed = event.button_state() == ButtonState::Pressed;
                                if pressed && event.seat_button_count() != 1
                                    || !pressed && event.seat_button_count() != 0
                                {
                                    continue;
                                }
                                self.pointer.buttons = pressed.then_some(button);
                                let kind = if pressed {
                                    MouseEventKind::Down(button)
                                } else {
                                    MouseEventKind::Up(button)
                                };
                                emit(Event::Mouse(
                                    self.pointer.event(kind, self.keys.modifiers()),
                                ))?;
                                moved = false;
                            }
                            PointerEvent::ScrollWheel(event) => {
                                if event.has_axis(Axis::Vertical) {
                                    self.scroll(
                                        event.scroll_value_v120(Axis::Vertical) / 120.0,
                                        emit,
                                    )?;
                                }
                            }
                            PointerEvent::ScrollFinger(event) => {
                                if event.has_axis(Axis::Vertical) {
                                    self.scroll(event.scroll_value(Axis::Vertical) / 15.0, emit)?;
                                }
                            }
                            PointerEvent::ScrollContinuous(event)
                                if event.has_axis(Axis::Vertical) =>
                            {
                                self.scroll(event.scroll_value(Axis::Vertical) / 15.0, emit)?;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                if moved {
                    emit(Event::Mouse(
                        self.pointer
                            .event(self.pointer.motion_kind(), self.keys.modifiers()),
                    ))?;
                }
                for event in repeat.due(Instant::now()) {
                    emit(event)?;
                }
            }
            let mut fds = [
                libc::pollfd {
                    fd: self.seat.fd()?,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.input.as_raw_fd(),
                    events: if active { libc::POLLIN } else { 0 },
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.watch.0.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            if unsafe {
                libc::poll(
                    fds.as_mut_ptr(),
                    fds.len() as libc::nfds_t,
                    repeat.timeout(),
                )
            } < 0
                && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
            {
                return Err(io::Error::other("Native input polling failed"));
            }
            if fds
                .iter()
                .any(|fd| fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0)
            {
                return Err(io::Error::other("Native input device connection closed"));
            }
        }
    }

    fn scroll(
        &mut self,
        steps: f64,
        emit: &mut impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<()> {
        for kind in self.pointer.scroll(steps) {
            emit(Event::Mouse(
                self.pointer.event(kind, self.keys.modifiers()),
            ))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_keyboard_settings_or_queue_overflow_require_a_layout_reload() {
        fn frame(name: &[u8], mask: u32) -> Vec<u8> {
            let mut bytes = Vec::new();
            for field in [1u32, mask, 0, name.len() as u32 + 1] {
                bytes.extend_from_slice(&field.to_ne_bytes());
            }
            bytes.extend_from_slice(name);
            bytes.push(0);
            bytes
        }
        assert!(!super::settings_changed(&frame(b"locale", libc::IN_CLOSE_WRITE)).unwrap());
        assert!(!super::settings_changed(&frame(b"keyboard.tmp", libc::IN_CLOSE_WRITE)).unwrap());
        assert!(super::settings_changed(&frame(b"keyboard", libc::IN_MOVED_TO)).unwrap());
        assert!(super::settings_changed(&frame(b"", libc::IN_Q_OVERFLOW)).unwrap());
        assert!(super::settings_changed(&[0; 15]).is_err());
    }
}
