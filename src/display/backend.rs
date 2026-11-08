use ratatui::{
    backend::{ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use std::io;

#[cfg(all(target_os = "linux", feature = "gui-drm"))]
type Tty = wros_graphics::Screen<io::Stdout>;
#[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
type Tty = ratatui::backend::CrosstermBackend<io::Stdout>;

pub enum Backend {
    Tty(Box<Tty>),
    #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
    Window(Box<crate::window::Engine>),
}

impl Backend {
    #[cfg(all(target_os = "linux", feature = "gui-drm"))]
    fn tty_mut(&mut self) -> Option<&mut Tty> {
        match self {
            Self::Tty(backend) => Some(backend),
            #[cfg(feature = "gui-wgpu")]
            Self::Window(_) => None,
        }
    }
    pub fn observe(&mut self, event: &ratatui::crossterm::event::Event) {
        #[cfg(all(target_os = "linux", feature = "gui-drm"))]
        if let Some(backend) = self.tty_mut() {
            use ratatui::crossterm::event::Event;
            match event {
                Event::Mouse(mouse) => {
                    backend.pointer(Some(Position::new(mouse.column, mouse.row)))
                }
                Event::FocusLost => backend.pointer(None),
                _ => {}
            }
        }
        #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
        let _ = event;
    }
    pub fn reclaim(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", feature = "gui-drm"))]
        if let Some(backend) = self.tty_mut() {
            backend.reclaim()?;
        }
        Ok(())
    }
    pub fn tty() -> io::Result<Self> {
        #[cfg(all(target_os = "linux", feature = "gui-drm"))]
        {
            let tty = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/tty")?;
            Ok(Self::Tty(Box::new(Tty::new(
                io::stdout(),
                crate::session::native(),
                &tty,
            ))))
        }
        #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
        {
            Ok(Self::Tty(Box::new(Tty::new(io::stdout()))))
        }
    }

    pub fn graphical(&self) -> bool {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(_) => true,
            #[cfg(all(target_os = "linux", feature = "gui-drm"))]
            Self::Tty(backend) => backend.graphical(),
            #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
            Self::Tty(_) => false,
        }
    }

    pub fn windowed(&self) -> bool {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(_) => true,
            Self::Tty(_) => false,
        }
    }

    pub fn suspend(&mut self) -> io::Result<()> {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(window) => window.paused(true),
            Self::Tty(backend) => {
                #[cfg(all(target_os = "linux", feature = "gui-drm"))]
                backend.suspend();
                #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
                let _ = backend;
                Ok(())
            }
        }
    }

    pub fn resume_window(&mut self) -> io::Result<()> {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(window) => window.paused(false),
            Self::Tty(_) => Ok(()),
        }
    }

    pub fn receive(
        &mut self,
        wait: std::time::Duration,
    ) -> Result<io::Result<ratatui::crossterm::event::Event>, std::sync::mpsc::RecvTimeoutError>
    {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(window) => window.receive(wait),
            Self::Tty(_) => {
                let _ = wait;
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
            }
        }
    }

    pub fn service_locked(&mut self) -> io::Result<()> {
        match self {
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Self::Window(window) => window.service_locked(),
            Self::Tty(_) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok(())
            }
        }
    }
}

macro_rules! delegate {
    ($self:ident, $method:ident $(, $arg:expr)*) => {
        match $self {
            Backend::Tty(backend) => backend.$method($($arg),*),
            #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
            Backend::Window(window) => window.pixels.$method($($arg),*),
        }
    };
}

impl ratatui::backend::Backend for Backend {
    type Error = io::Error;
    fn draw<'a, I>(&mut self, cells: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        delegate!(self, draw, cells)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        delegate!(self, hide_cursor)
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        delegate!(self, show_cursor)
    }
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        delegate!(self, get_cursor_position)
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, p: P) -> io::Result<()> {
        delegate!(self, set_cursor_position, p)
    }
    fn clear(&mut self) -> io::Result<()> {
        delegate!(self, clear)
    }
    fn clear_region(&mut self, kind: ClearType) -> io::Result<()> {
        delegate!(self, clear_region, kind)
    }
    fn size(&self) -> io::Result<Size> {
        delegate!(self, size)
    }
    fn window_size(&mut self) -> io::Result<WindowSize> {
        delegate!(self, window_size)
    }
    fn flush(&mut self) -> io::Result<()> {
        delegate!(self, flush)
    }
}
