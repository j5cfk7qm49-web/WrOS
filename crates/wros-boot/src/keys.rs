//! Keys from the console's raw bytes. In the boot image there is no terminal type
//! to ask about, so this reads what the kernel sends: the keyboard layout is
//! already applied, as it is for Debian's own prompt.

/// What a key means to the password screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    /// Ctrl-U: start the password again.
    Clear,
    Enter,
    /// Esc on its own: switch to Debian's text prompt.
    Esc,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    /// An Esc byte, which is Esc itself unless more follows at once.
    Escape,
    /// Inside an arrow or function key's sequence, skipped to its end.
    Sequence,
    /// Partway through a UTF-8 character.
    Utf8 { need: usize },
}

/// Bytes in, keys out.
#[derive(Debug, Default)]
pub struct Decoder {
    state: State,
    bytes: Vec<u8>,
}

impl Decoder {
    /// Whether an Esc is waiting to learn if a sequence follows it.
    pub fn esc_pending(&self) -> bool {
        self.state == State::Escape
    }

    /// Nothing followed the Esc in time: it was Esc.
    pub fn timeout(&mut self) -> Option<Key> {
        (std::mem::take(&mut self.state) == State::Escape).then_some(Key::Esc)
    }

    pub fn feed(&mut self, byte: u8) -> Option<Key> {
        match std::mem::take(&mut self.state) {
            State::Escape => {
                if byte == b'[' || byte == b'O' {
                    self.state = State::Sequence;
                    return None;
                }
                // Alt with a key: neither Esc nor a character of the password.
                None
            }
            State::Sequence => {
                if !(0x40..=0x7e).contains(&byte) {
                    self.state = State::Sequence;
                }
                None
            }
            State::Utf8 { need } => {
                if byte & 0xc0 != 0x80 {
                    self.bytes.clear();
                    return self.feed(byte);
                }
                self.bytes.push(byte);
                if need > 1 {
                    self.state = State::Utf8 { need: need - 1 };
                    return None;
                }
                let decoded = std::str::from_utf8(&self.bytes)
                    .ok()
                    .and_then(|s| s.chars().next());
                self.bytes.clear();
                decoded.map(Key::Char)
            }
            State::Ground => match byte {
                0x1b => {
                    self.state = State::Escape;
                    None
                }
                b'\r' | b'\n' => Some(Key::Enter),
                0x7f | 0x08 => Some(Key::Backspace),
                0x15 => Some(Key::Clear),
                0x20..=0x7e => Some(Key::Char(byte as char)),
                0xc2..=0xf4 => {
                    self.bytes = vec![byte];
                    let need = if byte >= 0xf0 {
                        3
                    } else if byte >= 0xe0 {
                        2
                    } else {
                        1
                    };
                    self.state = State::Utf8 { need };
                    None
                }
                // Other control keys and stray bytes do nothing.
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Key> {
        let mut decoder = Decoder::default();
        let mut out: Vec<Key> = bytes.iter().filter_map(|&b| decoder.feed(b)).collect();
        out.extend(decoder.timeout());
        out
    }

    #[test]
    fn letters_utf8_and_editing_keys() {
        assert_eq!(
            keys("aé€😀".as_bytes()),
            vec![
                Key::Char('a'),
                Key::Char('é'),
                Key::Char('€'),
                Key::Char('😀')
            ]
        );
        assert_eq!(
            keys(b"\x7f\x08\x15\r\n"),
            vec![
                Key::Backspace,
                Key::Backspace,
                Key::Clear,
                Key::Enter,
                Key::Enter
            ]
        );
    }

    #[test]
    fn esc_alone_is_esc_but_arrow_keys_are_skipped() {
        assert_eq!(keys(b"\x1b"), vec![Key::Esc]);
        assert_eq!(keys(b"\x1b[A\x1b[3~\x1bOP"), vec![]);
        assert_eq!(keys(b"a\x1b[Db"), vec![Key::Char('a'), Key::Char('b')]);
        // Alt+x is neither Esc nor a character.
        assert_eq!(keys(b"\x1bx"), vec![]);
    }

    #[test]
    fn control_keys_and_broken_bytes_type_nothing() {
        assert_eq!(keys(b"\x03\x04\x09\x00"), vec![]);
        // A broken UTF-8 lead byte gives way to the next character.
        assert_eq!(keys(b"\xc3a"), vec![Key::Char('a')]);
        assert_eq!(keys(b"\x80\xff"), vec![]);
    }
}
