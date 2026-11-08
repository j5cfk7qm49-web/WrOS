//! Pixel output for the early screens. Keyboard input stays on the console.

/// An exact boot-menu override covers password, progress, login and writing.
pub fn text_start() -> bool {
    std::fs::read_to_string("/proc/cmdline")
        .unwrap_or_default()
        .split_whitespace()
        .any(|word| matches!(word, "wros.unlock=text" | "wros.session=console"))
}

pub fn init(graphical: bool) -> io::Result<ratatui::Terminal<Screen<io::Stdout>>> {
    use ratatui::crossterm::{
        execute,
        terminal::{EnterAlternateScreen, enable_raw_mode},
    };
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    match ratatui::Terminal::new(Screen::new(io::stdout(), graphical && !text_start(), &tty)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            ratatui::restore();
            Err(error)
        }
    }
}

use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use std::io;

#[cfg(target_os = "linux")]
mod kms;

/// The text backend is kept available even in builds with graphical output.
pub struct Screen<W: io::Write> {
    console: CrosstermBackend<W>,
    #[cfg(target_os = "linux")]
    graphics: Option<kms::Display>,
}

impl<W: io::Write> Screen<W> {
    pub fn new(writer: W, graphical: bool, tty: &std::fs::File) -> Self {
        #[cfg(not(target_os = "linux"))]
        let _ = (graphical, tty);
        Self {
            console: CrosstermBackend::new(writer),
            #[cfg(target_os = "linux")]
            graphics: graphical
                .then(|| kms::Display::open(tty))
                .and_then(Result::ok),
        }
    }

    pub fn graphical(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.graphics.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Drop DRM master and restore the console before the independent locker starts.
    pub fn suspend(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.graphics = None;
        }
    }
}

macro_rules! delegate {
    ($self:ident, $method:ident $(, $arg:expr)*) => {{
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut $self.graphics {
            return display.pixels.$method($($arg),*).map_err(io::Error::other);
        }
        $self.console.$method($($arg),*)
    }};
}

impl<W: io::Write> Backend for Screen<W> {
    type Error = io::Error;
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        delegate!(self, draw, content)
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
        #[cfg(target_os = "linux")]
        if let Some(display) = &self.graphics {
            return display.pixels.size().map_err(io::Error::other);
        }
        self.console.size()
    }
    fn window_size(&mut self) -> io::Result<WindowSize> {
        delegate!(self, window_size)
    }
    fn flush(&mut self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut self.graphics {
            display.pixels.flush().map_err(io::Error::other)?;
            return display.present();
        }
        self.console.flush()
    }
}

/// XRGB8888 scanout, including the device's row padding and unused lower rows.
fn copy_rgb(rgb: &[u8], width: usize, height: usize, target: &mut [u8], pitch: usize, rows: usize) {
    target.fill(0);
    for (source, dest) in rgb
        .chunks_exact(width * 3)
        .take(height.min(rows))
        .zip(target.chunks_exact_mut(pitch))
    {
        for (pixel, out) in source
            .as_chunks::<3>()
            .0
            .iter()
            .zip(dest.as_chunks_mut::<4>().0.iter_mut())
        {
            out[..3].copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rgb_rows_respect_drm_pitch_and_black_margins() {
        let mut target = vec![0xff; 24];
        super::copy_rgb(&[1, 2, 3, 4, 5, 6], 2, 1, &mut target, 12, 2);
        assert_eq!(
            target,
            [
                3, 2, 1, 0, 6, 5, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }
}
