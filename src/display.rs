//! The normal terminal build and the installed graphical writing screen.

mod backend;
pub type Terminal = ratatui::Terminal<backend::Backend>;

pub fn init() -> std::io::Result<Terminal> {
    #[cfg(all(target_os = "linux", feature = "gui-wgpu"))]
    if std::env::var_os("WROS_SESSION").is_some_and(|v| v == "wgpu") {
        return ratatui::Terminal::new(backend::Backend::Window(Box::new(
            crate::window::Engine::new()?,
        )));
    }
    use ratatui::crossterm::{
        execute,
        terminal::{EnterAlternateScreen, enable_raw_mode},
    };
    let result = (|| {
        enable_raw_mode()?;
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        ratatui::Terminal::new(backend::Backend::tty()?)
    })();
    if result.is_err() {
        ratatui::restore();
    }
    result
}

pub fn graphical(terminal: &Terminal) -> bool {
    terminal.backend().graphical()
}

pub fn windowed(terminal: &Terminal) -> bool {
    terminal.backend().windowed()
}

#[cfg(target_os = "linux")]
pub fn start_input(terminal: &mut Terminal) -> std::io::Result<Option<wros_input::Input>> {
    if windowed(terminal) {
        return Ok(None);
    }
    let executable = std::env::current_exe()?;
    #[cfg(feature = "gui-drm")]
    if graphical(terminal) {
        use ratatui::backend::Backend;
        let size = terminal.backend_mut().window_size()?;
        let input = wros_input::Input::start_native(
            &executable,
            wros_input::NativeConfig {
                pixels: (size.pixels.width, size.pixels.height),
                cells: (size.columns_rows.width, size.columns_rows.height),
            },
        )?;
        reclaim(terminal)?;
        return Ok(Some(input));
    }
    Ok(Some(wros_input::Input::start(&executable)?))
}

pub fn reclaim(terminal: &mut Terminal) -> std::io::Result<()> {
    terminal.backend_mut().reclaim()
}

pub fn suspend(terminal: &mut Terminal) -> std::io::Result<()> {
    if windowed(terminal) {
        terminal.hide_cursor()?;
        terminal.draw(|_| {})?;
    }
    terminal.backend_mut().suspend()
}

pub fn resume(terminal: &mut Terminal) -> std::io::Result<()> {
    if windowed(terminal) {
        terminal.backend_mut().resume_window()?;
    } else {
        *terminal = init()?;
    }
    terminal.clear()
}

#[cfg(target_os = "linux")]
pub fn receive(
    terminal: &mut Terminal,
    input: &Option<wros_input::Input>,
    wait: std::time::Duration,
) -> Result<std::io::Result<ratatui::crossterm::event::Event>, std::sync::mpsc::RecvTimeoutError> {
    let received = match input {
        Some(input) => input.recv_timeout(wait),
        None => terminal.backend_mut().receive(wait),
    };
    if let Ok(Ok(event)) = &received {
        terminal.backend_mut().observe(event);
    }
    received
}

#[cfg(target_os = "linux")]
pub fn wait_for(
    terminal: &mut Terminal,
    child: &mut std::process::Child,
) -> std::io::Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        terminal.backend_mut().service_locked()?;
    }
}
