use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    let [input, output, page, dpi, format] = arguments.as_slice() else {
        return Err("usage: wros-hayro-bench PDF OUTPUT PAGE DPI gray|rgb".into());
    };
    let page = page.to_str().ok_or("invalid page")?.parse::<usize>()?;
    let dpi = dpi.to_str().ok_or("invalid DPI")?.parse::<u16>()?;
    let gray = match format.to_str() {
        Some("gray") => true,
        Some("rgb") => false,
        _ => return Err("expected gray or rgb".into()),
    };
    let pdf = Pdf::new(fs::read(input)?).map_err(|error| format!("PDF: {error:?}"))?;
    let position = page.checked_sub(1).ok_or("page numbers start at one")?;
    let page = pdf
        .pages()
        .iter()
        .nth(position)
        .ok_or("page does not exist")?;
    let scale = f32::from(dpi) / 72.0;
    let (width, height) = page.render_dimensions();
    let (width, height) = (width * scale, height * scale);
    let valid_dimension = |pixels: f32| (1.0..=f32::from(u16::MAX)).contains(&pixels);
    if !valid_dimension(width) || !valid_dimension(height) || width * height > 36_000_000.0 {
        return Err("page raster exceeds the benchmark dimensions".into());
    }
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    let mut writer = BufWriter::new(File::create(Path::new(output))?);
    let magic = if gray { "P5" } else { "P6" };
    writeln!(
        writer,
        "{magic}\n{} {}\n255",
        pixmap.width(),
        pixmap.height()
    )?;
    for pixel in pixmap.data() {
        if gray {
            let luma =
                299 * u32::from(pixel.r) + 587 * u32::from(pixel.g) + 114 * u32::from(pixel.b);
            writer.write_all(&[((luma + 500) / 1000) as u8])?;
        } else {
            writer.write_all(&[pixel.r, pixel.g, pixel.b])?;
        }
    }
    writer.flush()?;
    Ok(())
}
