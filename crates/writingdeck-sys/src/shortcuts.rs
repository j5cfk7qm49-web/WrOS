//! Additive shortcut aliases. Established Grimoire bindings remain available.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Dashboard,
    SystemSettings,
    Tutorial,
    Outline,
    Focus,
    Save,
    Find,
    Palette,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub chord: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    pub version: u32,
    pub bindings: Vec<Binding>,
}
impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            version: 1,
            bindings: vec![
                Binding {
                    chord: "F10".into(),
                    action: Action::Outline,
                },
                Binding {
                    chord: "F12".into(),
                    action: Action::Dashboard,
                },
            ],
        }
    }
}
impl Shortcuts {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported shortcuts version {}", self.version);
        }
        let mut seen = BTreeSet::new();
        for b in &self.bindings {
            let c = canonical(&b.chord)?;
            if !seen.insert(c.clone()) {
                bail!("duplicate shortcut {c}");
            }
        }
        Ok(())
    }
    pub fn find(&self, chord: &str) -> Option<Action> {
        let c = canonical(chord).ok()?;
        self.bindings
            .iter()
            .find(|b| canonical(&b.chord).ok().as_ref() == Some(&c))
            .map(|b| b.action)
    }
}
/// Restrict aliases to unassigned F10-F12 or Ctrl+Alt+ASCII letters. Plain
/// characters must remain text; old bindings and terminal control keys stay put.
pub fn canonical(chord: &str) -> Result<String> {
    let c = chord.trim().to_ascii_uppercase().replace(' ', "");
    if matches!(c.as_str(), "F10" | "F11" | "F12") {
        return Ok(c);
    }
    if let Some(letter) = c.strip_prefix("CTRL+ALT+")
        && letter.len() == 1
        && letter.as_bytes()[0].is_ascii_alphabetic()
    {
        return Ok(format!("Ctrl+Alt+{letter}"));
    }
    bail!("{chord:?} is reserved or unsafe; choose F10-F12 or Ctrl+Alt+A-Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_chords_do_not_retranslate_colemak_text() {
        assert_eq!(canonical("ctrl+alt+s").unwrap(), "Ctrl+Alt+S");
        for c in ["s", "Ctrl+S", "Ctrl+Q", "F1", "Tab", "Ctrl+Alt+€"] {
            assert!(canonical(c).is_err());
        }
        let mut s = Shortcuts::default();
        s.bindings.push(Binding {
            chord: "f10".into(),
            action: Action::Save,
        });
        assert!(s.validate().is_err());
        assert_eq!(Shortcuts::default().find("f12"), Some(Action::Dashboard));
    }
}
