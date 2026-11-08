//! The normal terminal build and the installed graphical writing screen.

#[cfg(all(target_os = "linux", feature = "gui-drm"))]
pub type Terminal = ratatui::Terminal<wros_graphics::Screen<std::io::Stdout>>;
#[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
pub type Terminal = ratatui::DefaultTerminal;

pub fn init() -> std::io::Result<Terminal> {
    #[cfg(all(target_os = "linux", feature = "gui-drm"))]
    {
        wros_graphics::init(std::env::var_os("WROS_SESSION").is_some_and(|v| v == "drm"))
    }
    #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
    {
        ratatui::try_init()
    }
}

pub fn graphical(terminal: &Terminal) -> bool {
    #[cfg(all(target_os = "linux", feature = "gui-drm"))]
    {
        terminal.backend().graphical()
    }
    #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
    {
        let _ = terminal;
        false
    }
}

pub fn suspend(terminal: &mut Terminal) {
    #[cfg(all(target_os = "linux", feature = "gui-drm"))]
    {
        terminal.backend_mut().suspend();
    }
    #[cfg(not(all(target_os = "linux", feature = "gui-drm")))]
    {
        let _ = terminal;
    }
}
