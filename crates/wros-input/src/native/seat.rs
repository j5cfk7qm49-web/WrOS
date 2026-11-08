//! Device access belongs to logind and to this disposable process, never a group grant.

use input::LibinputInterface;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{CStr, CString, c_void},
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

#[repr(C)]
struct RawSeat {
    _private: [u8; 0],
}

#[repr(C)]
struct Listener {
    enable: unsafe extern "C" fn(*mut RawSeat, *mut c_void),
    disable: unsafe extern "C" fn(*mut RawSeat, *mut c_void),
}

#[link(name = "seat")]
unsafe extern "C" {
    fn libseat_open_seat(listener: *const Listener, data: *mut c_void) -> *mut RawSeat;
    fn libseat_close_seat(seat: *mut RawSeat) -> libc::c_int;
    fn libseat_disable_seat(seat: *mut RawSeat) -> libc::c_int;
    fn libseat_seat_name(seat: *mut RawSeat) -> *const libc::c_char;
    fn libseat_get_fd(seat: *mut RawSeat) -> libc::c_int;
    fn libseat_dispatch(seat: *mut RawSeat, timeout: libc::c_int) -> libc::c_int;
    fn libseat_open_device(
        seat: *mut RawSeat,
        path: *const libc::c_char,
        fd: *mut libc::c_int,
    ) -> libc::c_int;
    fn libseat_close_device(seat: *mut RawSeat, id: libc::c_int) -> libc::c_int;
    fn libseat_switch_session(seat: *mut RawSeat, session: libc::c_int) -> libc::c_int;
    fn libseat_set_log_level(level: libc::c_int);
}

#[derive(Default)]
struct Flags {
    enabled: Cell<bool>,
    retire: Cell<bool>,
    failed: Cell<bool>,
}

unsafe extern "C" fn enable(_: *mut RawSeat, data: *mut c_void) {
    unsafe { &*data.cast::<Flags>() }.enabled.set(true);
}

unsafe extern "C" fn disable(_: *mut RawSeat, data: *mut c_void) {
    let flags = unsafe { &*data.cast::<Flags>() };
    flags.enabled.set(false);
    flags.retire.set(true);
}

static LISTENER: Listener = Listener { enable, disable };

pub(super) fn eligible_console(fd: RawFd) -> bool {
    let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
    unsafe {
        libc::isatty(fd) == 1
            && libc::fstat(fd, info.as_mut_ptr()) == 0
            && info.assume_init().st_rdev == libc::makedev(4, 1)
    }
}

pub(super) struct Seat {
    raw: Cell<Option<NonNull<RawSeat>>>,
    flags: Box<Flags>,
    devices: RefCell<BTreeMap<RawFd, i32>>,
}

impl Seat {
    pub(super) fn open() -> io::Result<Rc<Self>> {
        if !eligible_console(0)
            || unsafe { libc::geteuid() } == 0
            || std::env::var("LIBSEAT_BACKEND").as_deref() != Ok("logind")
            || std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("tty")
        {
            return Err(io::Error::other(
                "Native input needs the authenticated writing console",
            ));
        }
        let flags = Box::<Flags>::default();
        unsafe { libseat_set_log_level(0) };
        let raw = NonNull::new(unsafe {
            libseat_open_seat(&LISTENER, (&*flags as *const Flags).cast_mut().cast())
        })
        .ok_or_else(|| io::Error::other("Native input seat is unavailable"))?;
        let seat = Rc::new(Self {
            raw: Cell::new(Some(raw)),
            flags,
            devices: RefCell::new(BTreeMap::new()),
        });
        let name = unsafe { libseat_seat_name(raw.as_ptr()) };
        if name.is_null() || unsafe { CStr::from_ptr(name) }.to_bytes() != b"seat0" {
            return Err(io::Error::other("Native input is on another seat"));
        }
        seat.dispatch()?;
        if !seat.enabled() {
            return Err(io::Error::other("Native input seat is inactive"));
        }
        Ok(seat)
    }

    fn raw(&self) -> io::Result<*mut RawSeat> {
        self.raw
            .get()
            .map(NonNull::as_ptr)
            .ok_or_else(|| io::Error::other("Native input seat is closed"))
    }

    pub(super) fn dispatch(&self) -> io::Result<()> {
        if unsafe { libseat_dispatch(self.raw()?, 0) } < 0 {
            return Err(io::Error::other("Native input seat disconnected"));
        }
        Ok(())
    }

    pub(super) fn enabled(&self) -> bool {
        self.flags.enabled.get()
    }
    pub(super) fn retire_requested(&self) -> bool {
        self.flags.retire.replace(false)
    }

    pub(super) fn acknowledge_disable(&self) -> io::Result<()> {
        if !self.devices.borrow().is_empty() || unsafe { libseat_disable_seat(self.raw()?) } < 0 {
            return Err(io::Error::other(
                "Native input devices could not be retired",
            ));
        }
        Ok(())
    }

    pub(super) fn fd(&self) -> io::Result<RawFd> {
        let fd = unsafe { libseat_get_fd(self.raw()?) };
        if fd < 0 {
            Err(io::Error::other("Native input seat disconnected"))
        } else {
            Ok(fd)
        }
    }

    pub(super) fn switch(&self, vt: u16) -> io::Result<()> {
        if !(1..=12).contains(&vt) || unsafe { libseat_switch_session(self.raw()?, vt as i32) } < 0
        {
            return Err(io::Error::other("Console switch was unavailable"));
        }
        Ok(())
    }

    pub(super) fn close(&self) -> io::Result<()> {
        let empty = self.devices.borrow().is_empty();
        let closed = self
            .raw
            .take()
            .is_none_or(|raw| unsafe { libseat_close_seat(raw.as_ptr()) } == 0);
        if !empty || !closed || self.flags.failed.get() {
            return Err(io::Error::other(
                "Native input retirement was not confirmed",
            ));
        }
        Ok(())
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub(super) struct Access(pub(super) Rc<Seat>);

impl LibinputInterface for Access {
    fn open_restricted(&mut self, path: &Path, _: i32) -> Result<OwnedFd, i32> {
        let name = path.as_os_str().as_bytes();
        if !self.0.enabled()
            || !name.starts_with(b"/dev/input/event")
            || name.len() == b"/dev/input/event".len()
            || !name[b"/dev/input/event".len()..]
                .iter()
                .all(u8::is_ascii_digit)
        {
            return Err(-libc::EACCES);
        }
        let name = CString::new(name).map_err(|_| -libc::EINVAL)?;
        let mut fd = -1;
        let id = unsafe {
            libseat_open_device(
                self.0.raw().map_err(|_| -libc::EBADF)?,
                name.as_ptr(),
                &mut fd,
            )
        };
        if id < 0 || fd < 0 {
            return Err(-libc::EACCES);
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            if let Ok(raw) = self.0.raw() {
                unsafe { libseat_close_device(raw, id) };
            }
            return Err(-libc::EIO);
        }
        self.0.devices.borrow_mut().insert(fd, id);
        Ok(owned)
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        let id = self.0.devices.borrow_mut().remove(&fd.as_raw_fd());
        let closed = id.is_some_and(|id| {
            self.0
                .raw()
                .is_ok_and(|raw| unsafe { libseat_close_device(raw, id) } == 0)
        });
        if !closed {
            self.0.flags.failed.set(true);
        }
        // libseat releases logind's device grant; OwnedFd closes our copy.
        drop(fd);
    }
}
