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
    Secondary,
    Focus,
    Save,
    Find,
    Palette,
    Pinboard,
    Pins,
    PinCurrent,
    Journal,
    Scratchpad,
    Handoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub chord: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
                    chord: "F11".into(),
                    action: Action::Secondary,
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
    #[test]
    fn original_v1_aliases_without_secondary_still_load() {
        let s:Shortcuts=serde_json::from_str(r#"{"version":1,"bindings":[{"chord":"F10","action":"outline"},{"chord":"F12","action":"dashboard"}]}"#).unwrap();
        s.validate().unwrap();
        assert_eq!(s.find("F10"), Some(Action::Outline));
        assert_eq!(s.find("F11"), None);
    }
    #[test]
    fn unknown_shortcut_configuration_is_rejected_instead_of_losing_fields() {
        for text in [
            r#"{"version":1,"bindings":[],"future_setting":true}"#,
            r#"{"version":1,"bindings":[{"chord":"F11","action":"secondary","future_setting":true}]}"#,
        ] {
            assert!(serde_json::from_str::<Shortcuts>(text).is_err());
        }
    }

    #[test]
    fn pin_actions_are_optional_v1_aliases_without_changing_existing_defaults() {
        let mut shortcuts = Shortcuts::default();
        let defaults = shortcuts.bindings.clone();
        for (chord, action) in [
            ("Ctrl+Alt+P", Action::Pinboard),
            ("Ctrl+Alt+I", Action::Pins),
            ("Ctrl+Alt+N", Action::PinCurrent),
        ] {
            shortcuts.bindings.push(Binding {
                chord: chord.into(),
                action,
            });
        }
        shortcuts.validate().unwrap();
        let loaded: Shortcuts =
            serde_json::from_str(&serde_json::to_string(&shortcuts).unwrap()).unwrap();
        assert_eq!(loaded.bindings[..3], defaults);
        assert_eq!(loaded.find("Ctrl+Alt+P"), Some(Action::Pinboard));
    }
}

#[cfg(test)]
#[test]
fn journal_alias_is_additive_and_v1_defaults_remain_unchanged() {
    let mut s = Shortcuts::default();
    let defaults = s.bindings.clone();
    s.bindings.push(Binding {
        chord: "Ctrl+Alt+J".into(),
        action: Action::Journal,
    });
    s.validate().unwrap();
    let loaded: Shortcuts = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(loaded.bindings[..3], defaults);
    assert_eq!(loaded.find("Ctrl+Alt+J"), Some(Action::Journal));
}

#[cfg(test)]
#[test]
fn scratchpad_and_handoff_aliases_roundtrip_without_default_changes() {
    let mut s = Shortcuts::default();
    let defaults = s.bindings.clone();
    for (chord, action) in [
        ("Ctrl+Alt+S", Action::Scratchpad),
        ("Ctrl+Alt+H", Action::Handoff),
    ] {
        s.bindings.push(Binding {
            chord: chord.into(),
            action,
        });
    }
    s.validate().unwrap();
    let loaded: Shortcuts = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(loaded.bindings[..defaults.len()], defaults);
    assert_eq!(loaded.find("Ctrl+Alt+S"), Some(Action::Scratchpad));
    assert_eq!(loaded.find("Ctrl+Alt+H"), Some(Action::Handoff));
}
