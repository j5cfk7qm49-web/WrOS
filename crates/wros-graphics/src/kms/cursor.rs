use super::Card;
use drm::{
    Device as _, DriverCapability,
    buffer::{Buffer, DrmFourcc},
    control::{Device as _, crtc, dumbbuffer::DumbBuffer},
};
use std::io;

type Point = (u16, u16);

pub(super) trait CursorDevice<B> {
    fn create(&self) -> io::Result<B>;
    fn show(&self, buffer: &B) -> io::Result<()>;
    fn hide(&self) -> io::Result<()>;
    fn move_to(&self, position: Point) -> io::Result<()>;
}

pub(super) struct KmsCursor<'a>(pub &'a Card, pub crtc::Handle);

#[allow(deprecated)] // Use the legacy cursor plane ioctls alongside legacy modesetting.
impl CursorDevice<DumbBuffer> for KmsCursor<'_> {
    fn create(&self) -> io::Result<DumbBuffer> {
        let dimension = |cap, minimum| {
            let size = self.0.get_driver_capability(cap)?;
            u32::try_from(size)
                .ok()
                .filter(|size| (minimum..=1024).contains(size))
                .ok_or_else(|| io::Error::other("Unsupported DRM cursor size"))
        };
        let width = dimension(DriverCapability::CursorWidth, 10)?;
        let height = dimension(DriverCapability::CursorHeight, 16)?;
        let mut buffer = self
            .0
            .create_dumb_buffer((width, height), DrmFourcc::Argb8888, 32)?;
        let pitch = buffer.pitch() as usize;
        let mapped = self.0.map_dumb_buffer(&mut buffer).map(|mut mapping| {
            mapping.as_mut().fill(0);
            crate::pointer_sprite(
                mapping.as_mut(),
                pitch,
                height as usize,
                width as usize,
                (0, 0),
                255,
            );
        });
        if let Err(error) = mapped {
            let _ = self.0.destroy_dumb_buffer(buffer);
            return Err(error);
        }
        Ok(buffer)
    }

    fn show(&self, buffer: &DumbBuffer) -> io::Result<()> {
        // The tip of the arrow is its top-left pixel.
        self.0.set_cursor2(self.1, Some(buffer), (0, 0))
    }

    fn hide(&self) -> io::Result<()> {
        self.0.set_cursor::<DumbBuffer>(self.1, None)
    }

    fn move_to(&self, (x, y): Point) -> io::Result<()> {
        self.0.move_cursor(self.1, (i32::from(x), i32::from(y)))
    }
}

pub(super) struct Pointer<B> {
    pub position: Option<Point>,
    pub focused: bool,
    pub buffer: Option<B>,
    shown_at: Option<Point>,
    fallback: bool,
}

impl<B> Default for Pointer<B> {
    fn default() -> Self {
        Self {
            position: None,
            focused: true,
            buffer: None,
            shown_at: None,
            fallback: false,
        }
    }
}

impl<B> Pointer<B> {
    fn target(&self) -> Option<Point> {
        self.position.filter(|_| self.focused)
    }

    pub fn needs_sync(&self) -> bool {
        self.shown_at != self.target().filter(|_| !self.fallback)
    }

    pub fn sync(&mut self, device: &impl CursorDevice<B>, after_modeset: bool) {
        let Some(position) = self.target().filter(|_| !self.fallback) else {
            self.hide(device);
            return;
        };
        if !after_modeset && self.shown_at == Some(position) {
            return;
        }
        let first = self.buffer.is_none();
        let result = (|| {
            let buffer = match &self.buffer {
                Some(buffer) => buffer,
                None => self.buffer.insert(device.create()?),
            };
            // Place the arrow before enabling it, so its tip first appears at the click point.
            device.move_to(position)?;
            if self.shown_at.is_none() || after_modeset {
                device.show(buffer)?;
            }
            Ok::<_, io::Error>(())
        })();
        if result.is_ok() {
            self.shown_at = Some(position);
            if first {
                unsafe { libc::syslog(libc::LOG_INFO, c"WrOS pointer: DRM cursor plane".as_ptr()) };
            }
        } else {
            self.hide(device);
            self.fallback = true;
            unsafe { libc::syslog(libc::LOG_INFO, c"WrOS pointer: frame sprite".as_ptr()) };
        }
    }

    pub fn hide(&mut self, device: &impl CursorDevice<B>) {
        if self.shown_at.is_some() && device.hide().is_ok() {
            self.shown_at = None;
        }
    }

    pub fn draw_sprite(&self, screen: &mut [u8], pitch: usize, height: usize, width: usize) {
        if let Some(position) = self.target().filter(|_| self.fallback) {
            crate::pointer_sprite(screen, pitch, height, width, position, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Create,
        Move(Point),
        Show,
        Hide,
    }

    #[derive(Default)]
    struct Cursor {
        calls: RefCell<Vec<Call>>,
        no_plane: bool,
        rejects_buffer: bool,
    }

    impl CursorDevice<()> for Cursor {
        fn create(&self) -> io::Result<()> {
            self.calls.borrow_mut().push(Call::Create);
            if self.no_plane {
                return Err(io::Error::from_raw_os_error(libc::ENODEV));
            }
            Ok(())
        }

        fn show(&self, (): &()) -> io::Result<()> {
            self.calls.borrow_mut().push(Call::Show);
            if self.rejects_buffer {
                return Err(io::Error::from_raw_os_error(libc::EINVAL));
            }
            Ok(())
        }

        fn hide(&self) -> io::Result<()> {
            self.calls.borrow_mut().push(Call::Hide);
            Ok(())
        }

        fn move_to(&self, position: Point) -> io::Result<()> {
            self.calls.borrow_mut().push(Call::Move(position));
            Ok(())
        }
    }

    #[test]
    fn three_pixels_move_the_plane_with_only_a_cursor_ioctl() {
        let device = Cursor::default();
        let mut pointer = Pointer {
            position: Some((80, 32)),
            ..Pointer::default()
        };
        pointer.sync(&device, false);
        assert_eq!(
            device.calls.take(),
            [Call::Create, Call::Move((80, 32)), Call::Show]
        );
        pointer.position = Some((83, 32));
        // Receive uses sync directly, with no framebuffer or frame presentation.
        pointer.sync(&device, false);
        assert_eq!(device.calls.take(), [Call::Move((83, 32))]);
        let mut screen = [0x66; 384 * 48];
        pointer.draw_sprite(&mut screen, 384, 48, 96);
        assert!(screen.iter().all(|&byte| byte == 0x66));
    }

    #[test]
    fn focus_loss_and_display_release_hide_the_plane_and_retake_restores_it() {
        let device = Cursor::default();
        let mut pointer = Pointer {
            position: Some((83, 35)),
            ..Pointer::default()
        };
        pointer.sync(&device, false);
        device.calls.take();
        pointer.focused = false;
        pointer.sync(&device, false);
        assert_eq!(device.calls.take(), [Call::Hide]);
        pointer.sync(&device, true);
        assert!(device.calls.take().is_empty());
        pointer.focused = true;
        pointer.sync(&device, false);
        assert_eq!(device.calls.take(), [Call::Move((83, 35)), Call::Show]);
        pointer.hide(&device);
        assert_eq!(device.calls.take(), [Call::Hide]);
        pointer.sync(&device, true);
        assert_eq!(device.calls.take(), [Call::Move((83, 35)), Call::Show]);
    }

    #[test]
    fn a_missing_plane_or_rejected_buffer_uses_the_sprite_at_the_pixel_position() {
        for device in [
            Cursor {
                no_plane: true,
                ..Cursor::default()
            },
            Cursor {
                rejects_buffer: true,
                ..Cursor::default()
            },
        ] {
            let mut pointer = Pointer {
                position: Some((80, 32)),
                ..Pointer::default()
            };
            pointer.sync(&device, false);
            device.calls.take();
            pointer.position = Some((83, 35));
            assert!(!pointer.needs_sync());
            let pitch = 384;
            let mut screen = [0x66; 384 * 52];
            pointer.draw_sprite(&mut screen, pitch, 52, 96);
            let tip = 35 * pitch + 83 * 4;
            assert_eq!(
                screen[tip - 4..tip + 8],
                [0x66, 0x66, 0x66, 0x66, 0, 0, 0, 0, 0x66, 0x66, 0x66, 0x66]
            );
            assert_eq!(
                screen[37 * pitch + 84 * 4..37 * pitch + 85 * 4],
                [255, 255, 255, 0]
            );
            assert!(device.calls.take().is_empty());
        }
    }
}
