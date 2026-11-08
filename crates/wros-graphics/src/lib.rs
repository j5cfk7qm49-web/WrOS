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

/// The exact font and colour adapter used by scanout and the screenshot templates.
pub fn raster(columns: u16, rows: u16) -> soft_ratatui::SoftBackend<soft_ratatui::CosmicText> {
    let mut pixels = soft_ratatui::SoftBackend::<soft_ratatui::CosmicText>::new(
        columns,
        rows,
        16,
        include_bytes!("../../../assets/DejaVuSansMono.ttf"),
    );
    reset_colours(&mut pixels);
    pixels
}

fn colour(colour: ratatui::style::Color, foreground: bool) -> ratatui::style::Color {
    use ratatui::style::Color::*;
    const ANSI: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    let index = match colour {
        Rgb(..) => return colour,
        Reset => {
            return if foreground {
                Rgb(221, 221, 221)
            } else {
                Rgb(0, 0, 0)
            };
        }
        Black => 0,
        Red => 1,
        Green => 2,
        Yellow => 3,
        Blue => 4,
        Magenta => 5,
        Cyan => 6,
        Gray => 7,
        DarkGray => 8,
        LightRed => 9,
        LightGreen => 10,
        LightYellow => 11,
        LightBlue => 12,
        LightMagenta => 13,
        LightCyan => 14,
        White => 15,
        Indexed(index) => index,
    };
    let (r, g, b) = match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let cube = [0, 95, 135, 175, 215, 255];
            let n = (index - 16) as usize;
            (cube[n / 36], cube[n / 6 % 6], cube[n % 6])
        }
        _ => {
            let gray = 8 + 10 * (index - 232);
            (gray, gray, gray)
        }
    };
    Rgb(r, g, b)
}

fn normalize(cell: &mut Cell) {
    cell.fg = colour(cell.fg, true);
    cell.bg = colour(cell.bg, false);
}

fn reset_colours(pixels: &mut soft_ratatui::SoftBackend<soft_ratatui::CosmicText>) {
    for cell in &mut pixels.buffer.content {
        normalize(cell);
    }
    pixels.redraw();
}

fn draw_pixels<'a, I>(pixels: &mut soft_ratatui::SoftBackend<soft_ratatui::CosmicText>, cells: I)
where
    I: Iterator<Item = (u16, u16, &'a Cell)>,
{
    let cells: Vec<_> = cells
        .map(|(x, y, cell)| {
            let mut cell = cell.clone();
            normalize(&mut cell);
            (x, y, cell)
        })
        .collect();
    pixels
        .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
        .unwrap();
}

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

    /// logind's native input controller changes VT modes on acquisition. The
    /// renderer reclaims its own VT signals and releases control on handoff.
    pub fn reclaim(&mut self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut self.graphics {
            display.reclaim()?;
        }
        Ok(())
    }

    /// Position the pointer without adding it to the application's cells.
    pub fn pointer(&mut self, position: Option<Position>) {
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut self.graphics {
            display.pointer(position);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = position;
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
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut self.graphics {
            draw_pixels(&mut display.pixels, content);
            return Ok(());
        }
        self.console.draw(content)
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
        self.clear_region(ClearType::All)
    }
    fn clear_region(&mut self, kind: ClearType) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(display) = &mut self.graphics {
            display.pixels.clear_region(kind).unwrap();
            reset_colours(&mut display.pixels);
            return Ok(());
        }
        self.console.clear_region(kind)
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
    copy_rgb_with_progress(rgb, width, height, target, pitch, rows, |_, _| {});
}

fn copy_rgb_with_progress(
    rgb: &[u8],
    width: usize,
    height: usize,
    target: &mut [u8],
    pitch: usize,
    rows: usize,
    mut copied: impl FnMut(&[u8], usize),
) {
    // This is live scanout. Keep each previous visible row until its new pixels
    // arrive; clearing the whole mapping first exposes a blackout (#106).
    let mut written = 0;
    for (row, source) in rgb
        .chunks_exact(width * 3)
        .take(height.min(rows))
        .enumerate()
    {
        let Some(dest) = target.get_mut(row * pitch..(row + 1) * pitch) else {
            break;
        };
        let visible_bytes = (source.len() / 3).min(dest.len() / 4) * 4;
        for (pixel, out) in source
            .as_chunks::<3>()
            .0
            .iter()
            .zip(dest.as_chunks_mut::<4>().0.iter_mut())
        {
            out.copy_from_slice(&[pixel[2], pixel[1], pixel[0], 0]);
        }
        dest[visible_bytes..].fill(0);
        written = row + 1;
        copied(target, row + 1);
    }
    target[written * pitch..].fill(0);
}

/// The native pointer is a scanout overlay, never part of the writing buffer.
fn pointer_sprite(target: &mut [u8], pitch: usize, rows: usize, width: usize, at: (u16, u16)) {
    const ARROW: [&[u8]; 16] = [
        b"X.........",
        b"XX........",
        b"XOX.......",
        b"XOOX......",
        b"XOOOX.....",
        b"XOOOOX....",
        b"XOOOOOX...",
        b"XOOOOOOX..",
        b"XOOOOOOOX.",
        b"XOOOOXXXXX",
        b"XOOXOX....",
        b"XOX.OX....",
        b"XX..XOX...",
        b"X...XOX...",
        b".....XOX..",
        b".....XXX..",
    ];
    for (dy, row) in ARROW.iter().enumerate() {
        let y = usize::from(at.1) + dy;
        if y >= rows {
            break;
        }
        for (dx, &pixel) in row.iter().enumerate() {
            let x = usize::from(at.0) + dx;
            if pixel == b'.' || x >= width || x * 4 + 4 > pitch {
                continue;
            }
            if let Some(out) = target.get_mut(y * pitch + x * 4..y * pitch + x * 4 + 4) {
                let colour = if pixel == b'O' { 255 } else { 0 };
                out.copy_from_slice(&[colour, colour, colour, 0]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_pixels_start_and_clear_black_and_use_terminal_palette_colours() {
        use super::*;
        use ratatui::style::Color;
        let mut pixels = raster(2, 1);
        assert!(pixels.get_pixmap_data().iter().all(|&b| b == 0));
        let mut cyan = Cell::new("█");
        cyan.set_fg(Color::Cyan);
        let mut red = Cell::new("█");
        red.set_fg(Color::Indexed(196));
        draw_pixels(&mut pixels, [(0, 0, &cyan), (1, 0, &red)].into_iter());
        assert!(
            pixels
                .get_pixmap_data()
                .as_chunks::<3>()
                .0
                .contains(&[0, 128, 128])
        );
        assert!(
            pixels
                .get_pixmap_data()
                .as_chunks::<3>()
                .0
                .contains(&[255, 0, 0])
        );
        pixels.clear().unwrap();
        reset_colours(&mut pixels);
        assert!(pixels.get_pixmap_data().iter().all(|&b| b == 0));
    }

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

    #[test]
    fn rgb_rows_remain_visible_until_their_replacements_are_copied() {
        let mut target = [90, 80, 70, 0, 60, 50, 40, 0];
        super::copy_rgb_with_progress(
            &[1, 2, 3, 4, 5, 6],
            1,
            2,
            &mut target,
            4,
            2,
            |visible, rows| {
                if rows == 1 {
                    assert_eq!(&visible[..4], &[3, 2, 1, 0]);
                    assert_eq!(
                        &visible[4..],
                        &[60, 50, 40, 0],
                        "scanout must retain the previous lower row until its replacement is copied"
                    );
                }
            },
        );
        assert_eq!(target, [3, 2, 1, 0, 6, 5, 4, 0]);
    }

    #[test]
    fn pointer_overlay_clips_at_the_screen_edge_and_preserves_row_padding() {
        let mut pixels = vec![0x66; 20 * 18];
        super::pointer_sprite(&mut pixels, 20, 18, 4, (3, 17));
        assert_eq!(&pixels[17 * 20 + 12..17 * 20 + 16], &[0, 0, 0, 0]);
        for row in pixels.as_chunks::<20>().0 {
            assert_eq!(&row[16..], &[0x66; 4]);
        }
        assert!(pixels[..17 * 20].iter().all(|&byte| byte == 0x66));
        let mut pixels = vec![0x66; 64 * 18];
        super::pointer_sprite(&mut pixels, 64, 18, 16, (0, 0));
        assert_eq!(&pixels[2 * 64 + 4..2 * 64 + 8], &[255, 255, 255, 0]);
    }
}
