use drm::{
    Device as _,
    buffer::{Buffer, DrmFourcc},
    control::{Device as _, connector, crtc, dumbbuffer::DumbBuffer, framebuffer},
};
use soft_ratatui::{CosmicText, SoftBackend};
use std::{
    fs::{File, OpenOptions},
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const KDSETMODE: libc::c_ulong = 0x4b3a;
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
    pub pixels: SoftBackend<CosmicText>,
    released: bool,
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

impl Display {
    pub fn open(tty: &File) -> io::Result<Self> {
        // Never take over a PTY or another user's background VT.
        let mut active = [0u16; 3];
        let mut device = 0u32;
        if unsafe { libc::ioctl(tty.as_raw_fd(), 0x5603, active.as_mut_ptr()) } < 0
            || unsafe { libc::ioctl(tty.as_raw_fd(), 0x80045432u64, &mut device) } < 0
            || device != ((4 << 8) | active[0] as u32)
        {
            return Err(io::Error::other("not a Linux virtual console"));
        }
        let mut last = io::Error::other("no connected display");
        for index in 0..16 {
            match Self::on_card(tty, index) {
                Ok(display) => return Ok(display),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn on_card(tty: &File, index: u32) -> io::Result<Self> {
        let card = Card(
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/dri/card{index}"))?,
        );
        card.acquire_master_lock()?;
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
        let probe = super::raster(1, 1);
        let cols = width as usize / probe.get_pixmap_width();
        let rows = height as usize / probe.get_pixmap_height();
        let pixels = super::raster(cols as u16, rows as u16);
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
            released: false,
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
        display.reclaim()?;
        Ok(display)
    }

    pub fn reclaim(&mut self) -> io::Result<()> {
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
        self.scanout()
    }

    fn scanout(&mut self) -> io::Result<()> {
        self.card.acquire_master_lock()?;
        self.card.set_crtc(
            self.crtc.handle(),
            Some(self.framebuffer),
            (0, 0),
            &[self.connector],
            Some(self.mode),
        )?;
        ioctl(&self.tty, KDSETMODE, 1)?;
        // Scanout and mapped-buffer writes do not require master. Let the narrow
        // warm-screen helper take it between modesets, just as on the console.
        self.card.release_master_lock()
    }

    fn restore(&self) {
        let _ = self.card.acquire_master_lock();
        let _ = self.card.set_crtc(
            self.crtc.handle(),
            self.crtc.framebuffer(),
            self.crtc.position(),
            &self.connectors,
            self.crtc.mode(),
        );
        let _ = ioctl(&self.tty, KDSETMODE, 0);
        let _ = self.card.release_master_lock();
    }

    pub fn present(&mut self) -> io::Result<()> {
        if self.release.swap(false, Ordering::Relaxed) {
            self.pointer = None;
            self.restore();
            self.released = true;
            ioctl(&self.tty, VT_RELDISP, 1)?;
        }
        if self.acquire.swap(false, Ordering::Relaxed) {
            ioctl(&self.tty, VT_RELDISP, 2)?;
            self.scanout()?;
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
        Ok(())
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
        if !self.released {
            self.restore();
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
