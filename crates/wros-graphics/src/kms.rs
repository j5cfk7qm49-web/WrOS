use super::TextSize;
use crate::Pixels;
use drm::{
    Device as _,
    buffer::{Buffer, DrmFourcc},
    control::{ClipRect, Device as _, connector, crtc, dumbbuffer::DumbBuffer, framebuffer},
};
use std::{
    fs::{File, OpenOptions},
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const KDSETMODE: libc::c_ulong = 0x4b3a;
const KDGETMODE: libc::c_ulong = 0x4b3b;
const VT_SETMODE: libc::c_ulong = 0x5602;
const VT_RELDISP: libc::c_ulong = 0x5605;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct VtMode {
    mode: u8,
    wait: u8,
    release: i16,
    acquire: i16,
    force: i16,
}

struct Card(File);
impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl drm::control::Device for Card {}

/// Owns the previous scanout and its own VT control until restoration.
pub(super) struct Display {
    card: Card,
    tty: File,
    owns_vt: bool,
    crtc: crtc::Info,
    connectors: Vec<connector::Handle>,
    connector: connector::Handle,
    mode: drm::control::Mode,
    buffer: DumbBuffer,
    framebuffer: framebuffer::Handle,
    pub pixels: Pixels,
    /// Scanned out yet. The buffer goes on screen only once it holds a frame, so a screen's
    /// start never shows a blank buffer between the previous picture and its own (#165).
    shown: bool,
    released: bool,
    /// Times the console took the display and this screen put its frame back.
    pub retaken: u32,
    /// Times something set the VT to text mode under this screen's frame and it set graphics
    /// mode back (greetd does as it starts the login screen; systemd at the switch to the
    /// installed system).
    pub text_resets: u32,
    /// The lock screen: every request to switch away from this VT is refused (#158).
    refuse_switches: bool,
    /// The lock screen: DRM master is kept from the open until the end. A program that drops
    /// root after opening the card can take master again only as the same process, and no
    /// other program may draw over a locked screen meanwhile.
    keep_master: bool,
    release: Arc<AtomicBool>,
    acquire: Arc<AtomicBool>,
    signals: Vec<signal_hook::SigId>,
    pointer: Option<(u16, u16)>,
}

fn ioctl(tty: &File, request: libc::c_ulong, arg: libc::c_int) -> io::Result<()> {
    if unsafe { libc::ioctl(tty.as_raw_fd(), request, arg) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn acquire_master(
    mut acquire: impl FnMut() -> io::Result<()>,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match acquire() {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::EACCES | libc::EPERM | libc::EBUSY | libc::EINTR)
                ) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn raster_for_mode(size: TextSize, width: u16, height: u16) -> Pixels {
    let probe = size.raster(1, 1);
    let cols = width as usize / probe.get_pixmap_width();
    let rows = height as usize / probe.get_pixmap_height();
    size.raster(cols as u16, rows as u16)
}

/// The VT is in text mode: the console, not another program, owns what is on screen.
fn text_mode(tty: &File) -> bool {
    let mut mode: libc::c_int = -1;
    let read = unsafe { libc::ioctl(tty.as_raw_fd(), KDGETMODE, &mut mode) };
    read == 0 && mode == 0
}

fn active_console(tty: &File) -> io::Result<()> {
    let mut active = [0u16; 3];
    let mut device = 0u32;
    if unsafe { libc::ioctl(tty.as_raw_fd(), 0x5603, active.as_mut_ptr()) } < 0
        || unsafe { libc::ioctl(tty.as_raw_fd(), 0x80045432u64, &mut device) } < 0
        || device != ((4 << 8) | active[0] as u32)
    {
        return Err(io::Error::other("not the active Linux virtual console"));
    }
    Ok(())
}

impl Display {
    pub fn open(tty: &File, size: TextSize) -> io::Result<Self> {
        // Never take over a PTY or another user's background VT.
        active_console(tty)?;
        let mut last = io::Error::other("no connected display");
        for index in 0..16 {
            match Self::on_card(tty, index, size) {
                Ok(display) => return Ok(display),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    /// Opens the card as DRM master. A program without CAP_SYS_ADMIN (the login screen, the
    /// writing app) becomes master only by opening the card while nobody else holds master, and
    /// can't take it later on that file: so while the previous start screen holds it for a
    /// moment, the card is opened again until it can (#165).
    fn open_as_master(tty: &File, index: u32) -> io::Result<Card> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let card = Card(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(format!("/dev/dri/card{index}"))?,
            );
            match active_console(tty).and_then(|()| card.acquire_master_lock()) {
                Ok(()) => return Ok(card),
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(libc::EACCES | libc::EPERM | libc::EBUSY | libc::EINTR)
                    ) && Instant::now() < deadline =>
                {
                    drop(card);
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn on_card(tty: &File, index: u32, size: TextSize) -> io::Result<Self> {
        let card = Self::open_as_master(tty, index)?;
        let resources = card.resource_handles()?;
        let connector = resources
            .connectors()
            .iter()
            .find_map(|&handle| {
                card.get_connector(handle, true)
                    .ok()
                    .filter(|c| c.state() == connector::State::Connected && !c.modes().is_empty())
            })
            .ok_or_else(|| io::Error::other("no connected display"))?;
        let encoder = connector
            .current_encoder()
            .and_then(|e| card.get_encoder(e).ok())
            .or_else(|| {
                connector
                    .encoders()
                    .iter()
                    .find_map(|&e| card.get_encoder(e).ok())
            })
            .ok_or_else(|| io::Error::other("no display encoder"))?;
        let crtc_handle = encoder
            .crtc()
            .or_else(|| {
                resources
                    .filter_crtcs(encoder.possible_crtcs())
                    .first()
                    .copied()
            })
            .ok_or_else(|| io::Error::other("no display controller"))?;
        let crtc = card.get_crtc(crtc_handle)?;
        let connectors = resources
            .connectors()
            .iter()
            .copied()
            .filter(|&handle| {
                card.get_connector(handle, true)
                    .ok()
                    .and_then(|c| c.current_encoder())
                    .and_then(|e| card.get_encoder(e).ok())
                    .and_then(|e| e.crtc())
                    == Some(crtc_handle)
            })
            .collect();
        let mode = *connector
            .modes()
            .iter()
            .find(|m| {
                m.mode_type()
                    .contains(drm::control::ModeTypeFlags::PREFERRED)
            })
            .unwrap_or(&connector.modes()[0]);
        let (width, height) = mode.size();
        if width < 320 || height < 200 || width > 7680 || height > 4320 {
            return Err(io::Error::other("unsupported screen size"));
        }
        // Construct the rasterizer before changing the physical display. A bad font
        // must never leave the disk prompt in graphics mode.
        let pixels = raster_for_mode(size, width, height);
        let buffer =
            card.create_dumb_buffer((width.into(), height.into()), DrmFourcc::Xrgb8888, 32)?;
        let framebuffer = match card.add_framebuffer(&buffer, 24, 32) {
            Ok(fb) => fb,
            Err(error) => {
                let _ = card.destroy_dumb_buffer(buffer);
                return Err(error);
            }
        };
        let mut display = Self {
            card,
            tty: tty.try_clone()?,
            owns_vt: false,
            crtc,
            connectors,
            connector: connector.handle(),
            mode,
            buffer,
            framebuffer,
            pixels,
            shown: false,
            released: false,
            retaken: 0,
            text_resets: 0,
            refuse_switches: false,
            keep_master: false,
            release: Arc::new(AtomicBool::new(false)),
            acquire: Arc::new(AtomicBool::new(false)),
            signals: vec![],
            pointer: None,
        };
        display.signals.push(signal_hook::flag::register(
            libc::SIGUSR1,
            display.release.clone(),
        )?);
        display.signals.push(signal_hook::flag::register(
            libc::SIGUSR2,
            display.acquire.clone(),
        )?);
        display.take_vt()?;
        Ok(display)
    }

    /// Takes the VT's switch signals again (logind's input controller changes the VT mode when
    /// it starts) and puts this screen's buffer back, if it is already showing.
    pub fn reclaim(&mut self) -> io::Result<()> {
        self.take_vt()?;
        if self.shown {
            self.scanout()?;
        }
        Ok(())
    }

    fn take_vt(&mut self) -> io::Result<()> {
        let vt = VtMode {
            mode: 1,
            release: libc::SIGUSR1 as i16,
            acquire: libc::SIGUSR2 as i16,
            ..VtMode::default()
        };
        if unsafe { libc::ioctl(self.tty.as_raw_fd(), VT_SETMODE, &vt) } < 0 {
            return Err(io::Error::last_os_error());
        }
        self.owns_vt = true;
        Ok(())
    }

    fn with_master<T>(&self, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        if self.keep_master {
            return operation();
        }
        // The colour helper briefly owns master between modesets. A VT handoff
        // must wait for it instead of silently leaving fbcon on the old buffer.
        acquire_master(
            || {
                active_console(&self.tty)?;
                self.card.acquire_master_lock()
            },
            Duration::from_secs(2),
        )?;
        let result = operation();
        let released = self.card.release_master_lock();
        result.and_then(|value| released.map(|()| value))
    }

    fn scanout(&mut self) -> io::Result<()> {
        // Mapped-buffer writes need no master; release it after the modeset so
        // the narrow colour helper can work between handoffs.
        self.with_master(|| {
            self.card.set_crtc(
                self.crtc.handle(),
                Some(self.framebuffer),
                (0, 0),
                &[self.connector],
                Some(self.mode),
            )?;
            ioctl(&self.tty, KDSETMODE, 1)
        })
    }

    pub fn restore(&mut self) -> io::Result<()> {
        self.put_back(false).map(|_| ())
    }

    /// Puts back what was on screen when this one opened and returns the VT to text mode.
    /// With `unless_superseded`, a screen whose frame another has replaced touches nothing
    /// and gives up the VT's switch signals, which the newer screen now holds; the check and
    /// the modeset share one master hold, so the newer screen's modeset can't land between
    /// them. Returns whether it put anything back.
    fn put_back(&mut self, unless_superseded: bool) -> io::Result<bool> {
        let mut superseded = false;
        let put_back = self.with_master(|| {
            if unless_superseded
                && self.card.get_crtc(self.crtc.handle())?.framebuffer() != Some(self.framebuffer)
            {
                superseded = true;
                return Ok(());
            }
            self.card
                .set_crtc(
                    self.crtc.handle(),
                    self.crtc.framebuffer(),
                    self.crtc.position(),
                    &self.connectors,
                    self.crtc.mode(),
                )
                // What was on screen at open was another start screen's frame, since freed:
                // turn the display off, and the console takes it back in text mode.
                .or_else(|_| {
                    self.card
                        .set_crtc(self.crtc.handle(), None, (0, 0), &[], None)
                })
        });
        self.released = true;
        if superseded {
            self.owns_vt = false;
            return Ok(false);
        }
        // Text mode even when the modeset failed: a console left in graphics mode shows
        // nothing at all.
        let text = ioctl(&self.tty, KDSETMODE, 0);
        put_back.and(text).map(|()| true)
    }

    pub fn present(&mut self) -> io::Result<()> {
        if self.release.swap(false, Ordering::Relaxed) {
            if self.refuse_switches {
                // Locked: the switch away is refused and nothing changes on screen.
                ioctl(&self.tty, VT_RELDISP, 0)?;
            } else {
                self.pointer = None;
                let restored = self.restore();
                // The VT switch waits for this answer whatever happened to the screen.
                ioctl(&self.tty, VT_RELDISP, 1)?;
                restored?;
            }
        }
        if self.acquire.swap(false, Ordering::Relaxed) {
            ioctl(&self.tty, VT_RELDISP, 2)?;
            if self.shown {
                self.scanout()?;
            }
            self.released = false;
        }
        if self.released {
            return Ok(());
        }
        let pitch = self.buffer.pitch() as usize;
        let height = self.mode.size().1 as usize;
        let mut mapping = self.card.map_dumb_buffer(&mut self.buffer)?;
        super::copy_rgb(
            self.pixels.get_pixmap_data(),
            self.pixels.get_pixmap_width(),
            self.pixels.get_pixmap_height(),
            mapping.as_mut(),
            pitch,
            height,
        );
        if let Some(position) = self.pointer {
            super::pointer_sprite(
                mapping.as_mut(),
                pitch,
                height,
                self.mode.size().0 as usize,
                position,
            );
        }
        drop(mapping);
        if self.shown {
            self.flush_frame();
            Ok(())
        } else {
            // The first frame is in the buffer: only now does it replace what is on screen.
            self.scanout()?;
            self.shown = true;
            Ok(())
        }
    }

    /// Tells the driver the buffer changed. Drivers that copy a buffer to the screen (QEMU's
    /// bochs and virtio, simpledrm before the real driver loads) otherwise show only what a
    /// modeset copied; i915 uses it for its frame compression. The ioctl needs DRM master,
    /// which is released between modesets for the colour helper, so it is taken around the call.
    /// A driver without the call, or a moment when another program holds master, costs only
    /// this frame's upload: the next frame marks the whole screen again.
    fn flush_frame(&self) {
        let (width, height) = self.mode.size();
        let clip = [ClipRect::new(0, 0, width, height)];
        if let Err(error) = self.card.dirty_framebuffer(self.framebuffer, &clip)
            && matches!(error.raw_os_error(), Some(libc::EACCES | libc::EPERM))
            && self.card.acquire_master_lock().is_ok()
        {
            let _ = self.card.dirty_framebuffer(self.framebuffer, &clip);
            let _ = self.card.release_master_lock();
        }
    }

    /// Another screen has scanned out its own frame over this one: the next screen in the
    /// start-up took over in place (#165).
    ///
    /// A screen that takes over sets the VT to graphics mode in the same master hold as its
    /// modeset. When the display shows something else with the VT in text mode, the console has
    /// taken it back instead (greetd sets the VT to text mode and clears it before it starts the
    /// login screen, and with no DRM master the console's buffer returns): this screen's frame
    /// goes back up and it carries on.
    pub fn superseded(&mut self) -> bool {
        let ours = |card: &Card| {
            card.get_crtc(self.crtc.handle())
                .is_ok_and(|crtc| crtc.framebuffer() == Some(self.framebuffer))
        };
        if !self.shown || self.released {
            return false;
        }
        if ours(&self.card) {
            // Text mode under this frame: the console would take the display at its next write
            // or unblank whenever nobody holds master. Graphics mode goes back at once.
            if text_mode(&self.tty) && ioctl(&self.tty, KDSETMODE, 1).is_ok() {
                self.text_resets += 1;
            }
            return false;
        }
        let mut retook = false;
        let replaced = self.with_master(|| {
            if ours(&self.card) {
                return Ok(false);
            }
            if !text_mode(&self.tty) {
                return Ok(true);
            }
            self.card.set_crtc(
                self.crtc.handle(),
                Some(self.framebuffer),
                (0, 0),
                &[self.connector],
                Some(self.mode),
            )?;
            ioctl(&self.tty, KDSETMODE, 1)?;
            retook = true;
            Ok(false)
        });
        if retook {
            self.retaken += 1;
        }
        replaced.unwrap_or(false)
    }

    /// Locks this VT for the lock screen: switches away are refused and DRM master, held since
    /// the open, is never let go. Call it straight after opening, before the first frame.
    pub fn hold_for_lock(&mut self) {
        self.refuse_switches = true;
        self.keep_master = true;
    }

    /// A VT switch is waiting for this screen's answer, given by the next [`Self::present`].
    pub fn vt_switch_pending(&self) -> bool {
        self.release.load(Ordering::Relaxed) || self.acquire.load(Ordering::Relaxed)
    }

    pub fn pointer(&mut self, position: Option<ratatui::layout::Position>) {
        use ratatui::backend::Backend;
        let cells = self.pixels.size().unwrap();
        self.pointer = position.map(|p| {
            (
                p.x.saturating_mul(
                    (self.pixels.get_pixmap_width() / usize::from(cells.width)) as u16,
                ),
                p.y.saturating_mul(
                    (self.pixels.get_pixmap_height() / usize::from(cells.height)) as u16,
                ),
            )
        });
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // A screen replaced in place by the next one in the start-up leaves the display, the
        // VT's mode and its switch signals to that one, however this one ends (#165).
        if self.shown && !self.released {
            let _ = self.put_back(true);
        }
        if self.owns_vt {
            // VT_SETMODE assigns VT_PROCESS to the calling process. Replaying
            // a preceding renderer's mode cannot restore its ownership: it
            // makes this writer the owner while it waits on the locker (#104).
            // Release only control we acquired, before removing our handlers.
            unsafe {
                libc::ioctl(self.tty.as_raw_fd(), VT_SETMODE, &VtMode::default());
            }
        }
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
        let _ = self.card.destroy_framebuffer(self.framebuffer);
        let _ = self.card.destroy_dumb_buffer(self.buffer);
        let _ = self.card.release_master_lock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::Backend, layout::Size};

    #[test]
    fn a_brief_colour_owner_is_retried_but_permanent_failure_is_reported() {
        let mut calls = 0;
        acquire_master(
            || {
                calls += 1;
                if calls < 3 {
                    Err(io::Error::from_raw_os_error(libc::EACCES))
                } else {
                    Ok(())
                }
            },
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(calls, 3);
        let mut inactive_attempts = 0;
        assert!(
            acquire_master(
                || {
                    inactive_attempts += 1;
                    Err(io::Error::other("not the active console"))
                },
                Duration::from_secs(2)
            )
            .is_err()
        );
        assert_eq!(
            inactive_attempts, 1,
            "an inactive console must not keep retrying"
        );
        for code in [libc::EACCES, libc::EBUSY, libc::EIO] {
            let error = acquire_master(|| Err(io::Error::from_raw_os_error(code)), Duration::ZERO)
                .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(code));
        }
    }

    #[test]
    fn the_writing_grid_is_170_by_48_at_1366_by_768() {
        // Unifont's 8x16 cell, as on the console: startup and writing alike.
        for size in [TextSize::Writing, TextSize::Startup] {
            let mut pixels = raster_for_mode(size, 1366, 768);
            let window = pixels.window_size().unwrap();
            assert_eq!(window.columns_rows, Size::new(170, 48));
            assert_eq!(window.pixels, Size::new(1360, 768));
        }
    }
}
