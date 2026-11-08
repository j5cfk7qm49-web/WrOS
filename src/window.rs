//! The optional Wayland window and its input queue. Rendering uses OpenGL first.

use ratatui::{
    backend::Backend,
    crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
};
use std::{
    collections::VecDeque,
    io,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    platform::pump_events::EventLoopExtPumpEvents,
    window::{Fullscreen, Window, WindowAttributes, WindowId},
};

pub type Pixels = ratatui_wgpu::WgpuBackend<'static, 'static>;

pub struct Engine {
    pub pixels: Pixels,
    event_loop: EventLoop<()>,
    state: State,
}

#[derive(Default)]
struct State {
    window: Option<Arc<Window>>,
    events: VecDeque<Event>,
    modifiers: ModifiersState,
    cursor: (f64, f64),
    buttons: Option<MouseButton>,
    cells: (u16, u16),
    cell_pixels: (u16, u16),
    resize: Option<(u32, u32)>,
    closed: bool,
    repaint: bool,
    composing: bool,
    paused: bool,
    error: Option<io::Error>,
}

impl Engine {
    pub fn new() -> io::Result<Self> {
        let mut event_loop = EventLoop::new().map_err(io::Error::other)?;
        let mut state = State::default();
        event_loop.pump_app_events(Some(Duration::ZERO), &mut state);
        let configured = Instant::now() + Duration::from_secs(5);
        while state.resize.is_none() && state.error.is_none() && Instant::now() < configured {
            event_loop.pump_app_events(Some(Duration::from_millis(20)), &mut state);
        }
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        let window = state
            .window
            .as_ref()
            .ok_or_else(|| io::Error::other("Wayland window did not open"))?
            .clone();
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            ))
        });
        let pixels = futures_lite::future::block_on(
            ratatui_wgpu::Builder::from_font(
                ratatui_wgpu::Font::new(include_bytes!("../assets/DejaVuSansMono.ttf"))
                    .ok_or_else(|| io::Error::other("WrOS font could not be loaded"))?,
            )
            .with_instance(instance)
            .with_font_size_px(16)
            .with_fg_color(ratatui::style::Color::White)
            .with_bg_color(ratatui::style::Color::Black)
            .with_width_and_height(ratatui_wgpu::Dimensions {
                width: NonZeroU32::new(size.width.max(1)).unwrap(),
                height: NonZeroU32::new(size.height.max(1)).unwrap(),
            })
            .build_with_target(window),
        )
        .map_err(io::Error::other)?;
        Ok(Self {
            pixels,
            event_loop,
            state,
        })
    }

    fn pump(&mut self, wait: Duration) -> io::Result<()> {
        let size = self.pixels.window_size()?;
        self.state.cells = (size.columns_rows.width, size.columns_rows.height);
        self.state.cell_pixels = (
            size.pixels.width / size.columns_rows.width.max(1),
            size.pixels.height / size.columns_rows.height.max(1),
        );
        self.event_loop.pump_app_events(Some(wait), &mut self.state);
        if let Some(error) = self.state.error.take() {
            return Err(error);
        }
        if let Some((width, height)) = self.state.resize.take()
            && width > 0
            && height > 0
        {
            self.pixels.resize(width, height);
        }
        Ok(())
    }

    pub fn receive(
        &mut self,
        timeout: Duration,
    ) -> Result<io::Result<Event>, std::sync::mpsc::RecvTimeoutError> {
        use std::sync::mpsc::RecvTimeoutError::{Disconnected, Timeout};
        let deadline = Instant::now() + timeout;
        loop {
            if self.state.closed {
                return Err(Disconnected);
            }
            if let Some(event) = self.state.events.pop_front() {
                return Ok(Ok(event));
            }
            if std::mem::take(&mut self.state.repaint) {
                return Err(Timeout);
            }
            let Some(wait) = deadline.checked_duration_since(Instant::now()) else {
                return Err(Timeout);
            };
            if let Err(error) = self.pump(wait) {
                return Ok(Err(error));
            }
        }
    }

    /// Keep the Wayland connection responsive, but never dispatch writing input
    /// while the independent console locker owns authentication.
    pub fn paused(&mut self, paused: bool) -> io::Result<()> {
        self.state.retire();
        if let Some(window) = &self.state.window {
            window.set_ime_allowed(!paused);
        }
        self.pump(Duration::ZERO)?;
        self.state.events.clear();
        self.state.paused = paused;
        Ok(())
    }

    pub fn service_locked(&mut self) -> io::Result<()> {
        self.pump(Duration::from_millis(50))
    }
}

impl ApplicationHandler for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match event_loop.create_window(
            WindowAttributes::default()
                .with_title("WrOS")
                .with_fullscreen(Some(Fullscreen::Borderless(None))),
        ) {
            Ok(window) => {
                window.set_ime_allowed(true);
                self.window = Some(Arc::new(window));
            }
            Err(error) => self.error = Some(io::Error::other(error)),
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        self.handle(event);
    }
}

impl State {
    fn retire(&mut self) {
        self.paused = true;
        self.events.clear();
        self.composing = false;
        self.buttons = None;
        self.modifiers = ModifiersState::empty();
    }

    fn handle(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => self.closed = true,
            WindowEvent::Resized(size) => {
                self.resize = Some((size.width, size.height));
                self.repaint = true;
            }
            WindowEvent::RedrawRequested => self.repaint = true,
            _ if self.paused => {}
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::empty();
                self.buttons = None;
                self.composing = false;
                self.events.push_back(Event::FocusLost);
            }
            WindowEvent::Focused(true) => self.events.push_back(Event::FocusGained),
            WindowEvent::KeyboardInput { event, .. }
                if !self.composing || !matches!(event.logical_key, Key::Character(_)) =>
            {
                self.events.extend(translate_key(
                    &event.logical_key,
                    modifiers(self.modifiers),
                    event.state == ElementState::Released,
                ));
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.composing = !text.is_empty(),
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                self.events.extend(translate_key(
                    &Key::Character(text.into()),
                    KeyModifiers::NONE,
                    false,
                ));
            }
            WindowEvent::Ime(Ime::Disabled) => self.composing = false,
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if let Some(button) = self.buttons {
                    self.mouse(MouseEventKind::Drag(button));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    winit::event::MouseButton::Left => Some(MouseButton::Left),
                    winit::event::MouseButton::Middle => Some(MouseButton::Middle),
                    winit::event::MouseButton::Right => Some(MouseButton::Right),
                    _ => None,
                };
                if let Some(button) = button {
                    self.buttons = (state == ElementState::Pressed).then_some(button);
                    self.mouse(if state == ElementState::Pressed {
                        MouseEventKind::Down(button)
                    } else {
                        MouseEventKind::Up(button)
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 24.0,
                };
                for _ in 0..y.abs().ceil().min(24.0) as usize {
                    self.mouse(if y > 0.0 {
                        MouseEventKind::ScrollUp
                    } else {
                        MouseEventKind::ScrollDown
                    });
                }
            }
            _ => {}
        }
    }
}

impl State {
    fn mouse(&mut self, kind: MouseEventKind) {
        let (column, row) = cell_position(self.cursor, self.cell_pixels, self.cells);
        self.events.push_back(Event::Mouse(MouseEvent {
            kind,
            column: column.min(self.cells.0.saturating_sub(1)),
            row: row.min(self.cells.1.saturating_sub(1)),
            modifiers: modifiers(self.modifiers),
        }));
    }
}

fn cell_position(position: (f64, f64), pixels: (u16, u16), cells: (u16, u16)) -> (u16, u16) {
    let x = (position.0.max(0.0) / pixels.0.max(1) as f64) as u16;
    let y = (position.1.max(0.0) / pixels.1.max(1) as f64) as u16;
    (
        x.min(cells.0.saturating_sub(1)),
        y.min(cells.1.saturating_sub(1)),
    )
}

fn modifiers(state: ModifiersState) -> KeyModifiers {
    let mut result = KeyModifiers::NONE;
    for (yes, flag) in [
        (state.control_key(), KeyModifiers::CONTROL),
        (state.shift_key(), KeyModifiers::SHIFT),
        (state.alt_key(), KeyModifiers::ALT),
        (state.super_key(), KeyModifiers::SUPER),
    ] {
        if yes {
            result |= flag;
        }
    }
    result
}

fn translate_key(key: &Key, modifiers: KeyModifiers, released: bool) -> Vec<Event> {
    let codes: Vec<_> = match key {
        Key::Character(text) => text
            .chars()
            .filter(|c| !c.is_control())
            .map(KeyCode::Char)
            .collect(),
        Key::Named(named) => {
            let code = match named {
                NamedKey::Enter => KeyCode::Enter,
                NamedKey::Escape => KeyCode::Esc,
                NamedKey::Tab if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
                NamedKey::Tab => KeyCode::Tab,
                NamedKey::Space => KeyCode::Char(' '),
                NamedKey::Backspace => KeyCode::Backspace,
                NamedKey::Delete => KeyCode::Delete,
                NamedKey::Insert => KeyCode::Insert,
                NamedKey::ArrowLeft => KeyCode::Left,
                NamedKey::ArrowRight => KeyCode::Right,
                NamedKey::ArrowUp => KeyCode::Up,
                NamedKey::ArrowDown => KeyCode::Down,
                NamedKey::Home => KeyCode::Home,
                NamedKey::End => KeyCode::End,
                NamedKey::PageUp => KeyCode::PageUp,
                NamedKey::PageDown => KeyCode::PageDown,
                NamedKey::F1 => KeyCode::F(1),
                NamedKey::F2 => KeyCode::F(2),
                NamedKey::F3 => KeyCode::F(3),
                NamedKey::F4 => KeyCode::F(4),
                NamedKey::F5 => KeyCode::F(5),
                NamedKey::F6 => KeyCode::F(6),
                NamedKey::F7 => KeyCode::F(7),
                NamedKey::F8 => KeyCode::F(8),
                NamedKey::F9 => KeyCode::F(9),
                NamedKey::F10 => KeyCode::F(10),
                NamedKey::F11 => KeyCode::F(11),
                NamedKey::F12 => KeyCode::F(12),
                _ => return vec![],
            };
            vec![code]
        }
        _ => vec![],
    };
    codes
        .into_iter()
        .map(|code| {
            Event::Key(KeyEvent::new_with_kind(
                code,
                modifiers,
                if released {
                    KeyEventKind::Release
                } else {
                    KeyEventKind::Press
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn logical_keys_keep_the_compositor_layout_unicode_and_shortcuts() {
        let keys = super::translate_key(&Key::Character("éΩ".into()), KeyModifiers::NONE, false);
        assert_eq!(keys.len(), 2);
        assert!(
            matches!(keys[0], Event::Key(k) if k.code == KeyCode::Char('é') && k.kind == KeyEventKind::Press)
        );
        assert!(matches!(keys[1], Event::Key(k) if k.code == KeyCode::Char('Ω')));
        let keys = super::translate_key(&Key::Character("s".into()), KeyModifiers::CONTROL, false);
        assert!(
            matches!(keys[0], Event::Key(k) if k.code == KeyCode::Char('s') && k.modifiers == KeyModifiers::CONTROL)
        );
        let keys =
            super::translate_key(&Key::Named(NamedKey::ArrowLeft), KeyModifiers::SHIFT, true);
        assert!(
            matches!(keys[0], Event::Key(k) if k.code == KeyCode::Left && k.kind == KeyEventKind::Release)
        );
    }

    #[test]
    fn dead_keys_and_control_text_do_not_become_editor_characters() {
        assert!(super::translate_key(&Key::Dead(Some('´')), KeyModifiers::NONE, false).is_empty());
        assert!(
            super::translate_key(
                &Key::Character("\u{1b}\0".into()),
                KeyModifiers::NONE,
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn lock_retires_composition_modifiers_and_all_pending_input() {
        let mut state = super::State {
            composing: true,
            modifiers: winit::keyboard::ModifiersState::CONTROL,
            ..Default::default()
        };
        state.events.extend(super::translate_key(
            &Key::Character("old".into()),
            KeyModifiers::NONE,
            false,
        ));
        state.retire();
        state.handle(winit::event::WindowEvent::Ime(winit::event::Ime::Commit(
            "fixture-unlock".into(),
        )));
        assert!(state.events.is_empty());
        assert!(!state.composing && state.modifiers.is_empty());
        state.paused = false;
        state.handle(winit::event::WindowEvent::Ime(winit::event::Ime::Commit(
            "é".into(),
        )));
        assert_eq!(state.events.len(), 1);
    }

    #[test]
    fn mouse_cells_use_font_pixels_and_clamp_at_the_window_edges() {
        assert_eq!(
            super::cell_position((10.0, 16.0), (10, 16), (136, 48)),
            (1, 1)
        );
        assert_eq!(
            super::cell_position((-1.0, -3.0), (10, 16), (136, 48)),
            (0, 0)
        );
        assert_eq!(
            super::cell_position((1365.0, 900.0), (10, 16), (136, 48)),
            (135, 47)
        );
    }
}
