//! Native input translation lives only in the disposable reader process.

mod reader;
mod seat;
pub(crate) use reader::Reader;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};
use xkbcommon::xkb::{self, compose};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConfig {
    pub pixels: (u16, u16),
    pub cells: (u16, u16),
}

impl NativeConfig {
    pub fn validate(self) -> io::Result<()> {
        if self.pixels.0 == 0
            || self.pixels.1 == 0
            || self.pixels.0 > 7680
            || self.pixels.1 > 4320
            || self.cells.0 == 0
            || self.cells.1 == 0
            || self.cells.0 > self.pixels.0
            || self.cells.1 > self.pixels.1
        {
            return Err(io::Error::other("Invalid native input geometry"));
        }
        Ok(())
    }
}

struct Pointer {
    geometry: NativeConfig,
    position: (f64, f64),
    buttons: Option<MouseButton>,
    scroll: f64,
}

impl Pointer {
    fn new(geometry: NativeConfig) -> Self {
        Self {
            geometry,
            position: (
                geometry.pixels.0 as f64 / 2.0,
                geometry.pixels.1 as f64 / 2.0,
            ),
            buttons: None,
            scroll: 0.0,
        }
    }

    fn move_to(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.position = (
                x.clamp(0.0, self.geometry.pixels.0.saturating_sub(1) as f64),
                y.clamp(0.0, self.geometry.pixels.1.saturating_sub(1) as f64),
            );
        }
    }

    fn retire(&mut self) {
        self.buttons = None;
        self.scroll = 0.0;
    }

    fn motion_kind(&self) -> MouseEventKind {
        self.buttons
            .map_or(MouseEventKind::Moved, MouseEventKind::Drag)
    }

    fn scroll(&mut self, steps: f64) -> Vec<MouseEventKind> {
        if !steps.is_finite() {
            return vec![];
        }
        self.scroll = (self.scroll + steps).clamp(-24.0, 24.0);
        let count = self.scroll.trunc() as i32;
        self.scroll -= count as f64;
        (0..count.unsigned_abs())
            .map(|_| {
                if count > 0 {
                    MouseEventKind::ScrollDown
                } else {
                    MouseEventKind::ScrollUp
                }
            })
            .collect()
    }

    fn event(&self, kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
        let column = (self.position.0
            / (self.geometry.pixels.0 / self.geometry.cells.0).max(1) as f64)
            as u16;
        let row = (self.position.1 / (self.geometry.pixels.1 / self.geometry.cells.1).max(1) as f64)
            as u16;
        MouseEvent {
            kind,
            modifiers,
            column: column.min(self.geometry.cells.0 - 1),
            row: row.min(self.geometry.cells.1 - 1),
        }
    }
}

#[derive(Default)]
struct Layout {
    model: String,
    layout: String,
    variant: String,
    options: String,
}

impl Layout {
    fn from_debian(text: &str) -> io::Result<Self> {
        if text.len() > 65536 {
            return Err(io::Error::other("Keyboard settings are too large"));
        }
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let Some((name, value)) = line.trim().split_once('=') else {
                continue;
            };
            if !matches!(name, "XKBMODEL" | "XKBLAYOUT" | "XKBVARIANT" | "XKBOPTIONS") {
                continue;
            }
            let value = value.trim();
            let value = if value.starts_with(['\'', '"']) {
                if value.len() < 2 || value.as_bytes().last() != value.as_bytes().first() {
                    return Err(io::Error::other("Invalid keyboard settings"));
                }
                &value[1..value.len() - 1]
            } else {
                value
            };
            if value.len() > 200
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-,:()+".contains(&b))
                || values.insert(name, value.to_string()).is_some()
            {
                return Err(io::Error::other("Invalid keyboard settings"));
            }
        }
        let layout = values
            .remove("XKBLAYOUT")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| io::Error::other("Keyboard layout is missing"))?;
        Ok(Self {
            layout,
            model: values.remove("XKBMODEL").unwrap_or_default(),
            variant: values.remove("XKBVARIANT").unwrap_or_default(),
            options: values.remove("XKBOPTIONS").unwrap_or_default(),
        })
    }
}

struct Keyboard {
    map: xkb::Keymap,
    state: xkb::State,
    table: compose::Table,
    compose: compose::State,
}

impl Keyboard {
    fn new(layout: &Layout) -> io::Result<Self> {
        // The worker never loads a writer's XKB or Compose configuration.
        let mut context =
            xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES | xkb::CONTEXT_NO_ENVIRONMENT_NAMES);
        if !context.include_path_append(std::path::Path::new("/usr/share/X11/xkb")) {
            return Err(io::Error::other("System keyboard data is unavailable"));
        }
        let map = xkb::Keymap::new_from_names(
            &context,
            "evdev",
            &layout.model,
            &layout.layout,
            &layout.variant,
            Some(layout.options.clone()),
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| io::Error::other("System keyboard layout is unavailable"))?;
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open("/usr/share/X11/locale/en_US.UTF-8/Compose")?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(io::Error::other("System compose keys are too large"));
        }
        let table = compose::Table::new_from_buffer(
            &context,
            bytes,
            "en_US.UTF-8",
            compose::FORMAT_TEXT_V1,
            compose::COMPILE_NO_FLAGS,
        )
        .map_err(|_| io::Error::other("System compose keys are unavailable"))?;
        let state = xkb::State::new(&map);
        let compose = compose::State::new(&table, compose::STATE_NO_FLAGS);
        Ok(Self {
            map,
            state,
            table,
            compose,
        })
    }

    fn reset(&mut self) {
        self.state = xkb::State::new(&self.map);
        self.compose = compose::State::new(&self.table, compose::STATE_NO_FLAGS);
    }

    fn modifiers(&self) -> KeyModifiers {
        let mut result = KeyModifiers::NONE;
        for (name, flag) in [
            (xkb::MOD_NAME_SHIFT, KeyModifiers::SHIFT),
            (xkb::MOD_NAME_CTRL, KeyModifiers::CONTROL),
            (xkb::MOD_NAME_ALT, KeyModifiers::ALT),
            (xkb::MOD_NAME_LOGO, KeyModifiers::SUPER),
        ] {
            if self
                .state
                .mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE)
            {
                result |= flag;
            }
        }
        result
    }

    #[allow(non_upper_case_globals)] // XKB's published keysym names.
    fn key(&mut self, evdev: u32, pressed: bool) -> Vec<Event> {
        let Some(code) = evdev.checked_add(8).filter(|code| *code <= 0xffff) else {
            return vec![];
        };
        let code = xkb::Keycode::new(code);
        self.state.update_key(
            code,
            if pressed {
                xkb::KeyDirection::Down
            } else {
                xkb::KeyDirection::Up
            },
        );
        let symbol = self.state.key_get_one_sym(code);
        let modifiers = self.modifiers();
        let mut composed = None;
        if pressed {
            if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) {
                self.compose.reset();
            } else {
                self.compose.feed(symbol);
                match self.compose.status() {
                    compose::Status::Composing => return vec![],
                    compose::Status::Composed => {
                        composed = self.compose.utf8();
                        self.compose.reset();
                    }
                    compose::Status::Cancelled => {
                        self.compose.reset();
                        return vec![];
                    }
                    compose::Status::Nothing => {}
                }
            }
        }
        let kind = if pressed {
            KeyEventKind::Press
        } else {
            KeyEventKind::Release
        };
        if let Some(text) = composed {
            return text
                .chars()
                .filter(|c| !c.is_control())
                .map(|c| Event::Key(KeyEvent::new_with_kind(KeyCode::Char(c), modifiers, kind)))
                .collect();
        }
        use xkb::keysyms::*;
        let code = match symbol.raw() {
            KEY_Return | KEY_KP_Enter => KeyCode::Enter,
            KEY_Escape => KeyCode::Esc,
            KEY_Tab if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
            KEY_Tab => KeyCode::Tab,
            KEY_ISO_Left_Tab => KeyCode::BackTab,
            KEY_BackSpace => KeyCode::Backspace,
            KEY_Delete | KEY_KP_Delete => KeyCode::Delete,
            KEY_Insert | KEY_KP_Insert => KeyCode::Insert,
            KEY_Left | KEY_KP_Left => KeyCode::Left,
            KEY_Right | KEY_KP_Right => KeyCode::Right,
            KEY_Up | KEY_KP_Up => KeyCode::Up,
            KEY_Down | KEY_KP_Down => KeyCode::Down,
            KEY_Home | KEY_KP_Home => KeyCode::Home,
            KEY_End | KEY_KP_End => KeyCode::End,
            KEY_Page_Up | KEY_KP_Page_Up => KeyCode::PageUp,
            KEY_Page_Down | KEY_KP_Page_Down => KeyCode::PageDown,
            sym @ KEY_F1..=KEY_F35 => KeyCode::F((sym - KEY_F1 + 1) as u8),
            _ => {
                let Some(mut c) = char::from_u32(xkb::keysym_to_utf32(symbol))
                    .filter(|c| !c.is_control() && *c != '\0')
                else {
                    return vec![];
                };
                if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) {
                    c.make_ascii_lowercase();
                }
                KeyCode::Char(c)
            }
        };
        vec![Event::Key(KeyEvent::new_with_kind(code, modifiers, kind))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

    fn keyboard() -> Keyboard {
        let layout = Layout::from_debian(
            "XKBLAYOUT=\"us\"\nXKBVARIANT=\"colemak\"\nXKBOPTIONS=\"compose:ralt\"\n",
        )
        .unwrap();
        Keyboard::new(&layout).unwrap()
    }

    #[test]
    fn physical_colemak_keys_keep_logical_shortcuts_and_key_releases() {
        let mut keys = keyboard();
        let event = keys.key(32, true);
        assert!(
            matches!(event.as_slice(), [Event::Key(k)] if k.code == KeyCode::Char('s') && k.modifiers.is_empty())
        );
        let event = keys.key(32, false);
        assert!(matches!(event.as_slice(), [Event::Key(k)] if k.kind == KeyEventKind::Release));
        assert!(keys.key(29, true).is_empty());
        let event = keys.key(32, true);
        assert!(
            matches!(event.as_slice(), [Event::Key(k)] if k.code == KeyCode::Char('s') && k.modifiers == KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn the_compose_prefix_never_becomes_text_and_retirement_discards_it() {
        let mut keys = keyboard();
        assert!(keys.key(100, true).is_empty());
        keys.key(100, false);
        assert!(keys.key(40, true).is_empty());
        keys.key(40, false);
        let event = keys.key(37, true);
        assert!(matches!(event.as_slice(), [Event::Key(k)] if k.code == KeyCode::Char('é')));
        keys.key(37, false);
        keys.key(100, true);
        keys.key(100, false);
        keys.key(40, true);
        keys.reset();
        let event = keys.key(37, true);
        assert!(
            matches!(event.as_slice(), [Event::Key(k)] if k.code == KeyCode::Char('e') && k.modifiers.is_empty())
        );
    }

    #[test]
    fn layout_data_is_bounded_and_never_evaluated_as_a_shell() {
        for text in [
            "XKBLAYOUT=\"$(touch /tmp/unwanted)\"\n",
            "XKBLAYOUT=us\nXKBLAYOUT=de\n",
            "XKBVARIANT=colemak\n",
        ] {
            assert!(Layout::from_debian(text).is_err());
        }
        assert!(Layout::from_debian(&"x".repeat(65537)).is_err());
    }

    #[test]
    fn pointer_motion_is_clamped_to_font_cells_and_retirement_releases_drag() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let geometry = NativeConfig {
            pixels: (1360, 768),
            cells: (170, 48),
        };
        assert!(geometry.validate().is_ok());
        let mut pointer = Pointer::new(geometry);
        pointer.move_to(80.0, 32.0);
        let event = pointer.event(MouseEventKind::Moved, KeyModifiers::NONE);
        assert_eq!((event.column, event.row), (10, 2));
        pointer.move_to(2000.0, 1000.0);
        let event = pointer.event(MouseEventKind::Moved, KeyModifiers::NONE);
        assert_eq!((event.column, event.row), (169, 47));
        pointer.move_to(-4.0, -1.0);
        pointer.buttons = Some(MouseButton::Left);
        assert_eq!(
            pointer.motion_kind(),
            MouseEventKind::Drag(MouseButton::Left)
        );
        pointer.retire();
        assert_eq!(pointer.motion_kind(), MouseEventKind::Moved);
        let event = pointer.event(MouseEventKind::Moved, KeyModifiers::NONE);
        assert_eq!((event.column, event.row), (0, 0));
        for bad in [
            NativeConfig {
                pixels: (0, 768),
                cells: (170, 48),
            },
            NativeConfig {
                pixels: (1360, 768),
                cells: (0, 48),
            },
            NativeConfig {
                pixels: (1360, 768),
                cells: (1700, 48),
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn device_access_is_never_attempted_from_a_pseudo_terminal_or_file() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let (mut master, mut slave) = (0, 0);
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let (_master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        assert!(!seat::eligible_console(slave.as_raw_fd()));
        let null = std::fs::File::open("/dev/null").unwrap();
        assert!(!seat::eligible_console(null.as_raw_fd()));
    }

    #[test]
    fn fractional_scroll_accumulates_and_retirement_discards_the_remainder() {
        let mut pointer = Pointer::new(NativeConfig {
            pixels: (800, 600),
            cells: (100, 37),
        });
        assert!(pointer.scroll(0.4).is_empty());
        assert!(pointer.scroll(0.4).is_empty());
        assert_eq!(pointer.scroll(0.4), vec![MouseEventKind::ScrollDown]);
        pointer.retire();
        assert!(pointer.scroll(0.8).is_empty());
        assert!(pointer.scroll(f64::NAN).is_empty());
    }

    #[test]
    fn key_repeat_waits_for_the_delay_and_cannot_survive_retirement() {
        use std::time::{Duration, Instant};
        let mut repeat = reader::Repeat::default();
        let start = Instant::now();
        let events = keyboard().key(32, true);
        repeat.key(32, true, true, events, start);
        assert!(repeat.due(start + Duration::from_millis(399)).is_empty());
        let events = repeat.due(start + Duration::from_millis(400));
        assert!(
            matches!(events.as_slice(), [Event::Key(k)] if k.code == KeyCode::Char('s') && k.kind == KeyEventKind::Repeat)
        );
        repeat.retire();
        assert!(repeat.due(start + Duration::from_secs(2)).is_empty());
    }
}
