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
    Productivity,
    MeasuredGoals,
    Calendar,
    AdvancedSprints,
    Achievements,
    DailyChallenges,
    StorySpark,
    /// Open the menu bar (rc13). Never written to the file unless the writer
    /// binds it themselves, so rc12 can still read a file rc13 saved.
    MenuBar,
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
                    action: Action::MenuBar,
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
            .or_else(|| (c == MENU_BAR_KEY && self.bar_key_is_free()).then_some(Action::MenuBar))
    }

    /// The key that runs `action`, if any (the menu bar's default included).
    pub fn chord_for(&self, action: Action) -> Option<String> {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord.clone())
            .or_else(|| {
                (action == Action::MenuBar && self.bar_key_is_free()).then(|| MENU_BAR_KEY.into())
            })
    }

    /// F10 opens the menu bar unless the file gives F10 to something else or
    /// gives the menu bar a key of its own. A file rc12 saved lists F10 for the
    /// outline, so it keeps exactly what it says.
    fn bar_key_is_free(&self) -> bool {
        !self.bindings.iter().any(|b| {
            b.action == Action::MenuBar || canonical(&b.chord).ok().as_deref() == Some(MENU_BAR_KEY)
        })
    }

    /// What to write to the file: the menu bar's default key is left out (the
    /// code supplies it), so rc12 can still read the file after a go-back.
    pub fn to_saved(&self) -> Shortcuts {
        let mut saved = self.clone();
        saved.bindings.retain(|b| {
            !(b.action == Action::MenuBar
                && canonical(&b.chord).ok().as_deref() == Some(MENU_BAR_KEY))
        });
        saved
    }
}
/// The menu bar's default key, supplied by the code rather than the file.
pub const MENU_BAR_KEY: &str = "F10";

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

#[cfg(test)]
mod menu_bar_tests {
    use super::*;

    /// What rc12 (d438fe4) wrote with no changes made: every default spelled out.
    const RC12_DEFAULTS: &str = r#"{"version":1,"bindings":[{"chord":"F10","action":"outline"},{"chord":"F11","action":"secondary"},{"chord":"F12","action":"dashboard"}]}"#;

    /// The action names rc12 can read; a file naming anything else stops rc12 loading it.
    const RC12_ACTIONS: [&str; 22] = [
        "dashboard",
        "system-settings",
        "tutorial",
        "outline",
        "secondary",
        "focus",
        "save",
        "find",
        "palette",
        "pinboard",
        "pins",
        "pin-current",
        "journal",
        "scratchpad",
        "handoff",
        "productivity",
        "measured-goals",
        "calendar",
        "advanced-sprints",
        "achievements",
        "daily-challenges",
        "story-spark",
    ];

    fn rc12_can_read(text: &str) -> bool {
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        value["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| RC12_ACTIONS.contains(&b["action"].as_str().unwrap()))
    }

    #[test]
    fn no_file_gives_f10_to_the_menu_bar_and_the_outline_no_key() {
        let s = Shortcuts::default();
        assert_eq!(s.find("F10"), Some(Action::MenuBar));
        assert_eq!(s.find("F11"), Some(Action::Secondary));
        assert_eq!(s.find("F12"), Some(Action::Dashboard));
        assert_eq!(s.chord_for(Action::Outline), None);
    }

    #[test]
    fn an_rc12_file_keeps_f10_on_the_outline() {
        let s: Shortcuts = serde_json::from_str(RC12_DEFAULTS).unwrap();
        s.validate().unwrap();
        assert_eq!(s.find("F10"), Some(Action::Outline));
        assert_eq!(
            s.chord_for(Action::MenuBar),
            None,
            "Esc still opens the bar"
        );
    }

    #[test]
    fn a_saved_file_that_leaves_f10_free_gives_it_to_the_menu_bar() {
        let s: Shortcuts = serde_json::from_str(
            r#"{"version":1,"bindings":[{"chord":"Ctrl+Alt+O","action":"outline"}]}"#,
        )
        .unwrap();
        assert_eq!(s.find("F10"), Some(Action::MenuBar));
        assert_eq!(s.find("Ctrl+Alt+O"), Some(Action::Outline));
    }

    #[test]
    fn saving_the_default_bar_key_writes_nothing_rc12_cannot_read() {
        let mut s = Shortcuts::default();
        s.bindings.push(Binding {
            chord: "Ctrl+Alt+J".into(),
            action: Action::Journal,
        });
        let text = serde_json::to_string(&s.to_saved()).unwrap();
        assert!(!text.contains("menu-bar"), "{text}");
        assert!(rc12_can_read(&text));
        // Read back by rc13, F10 still opens the bar.
        let back: Shortcuts = serde_json::from_str(&text).unwrap();
        assert_eq!(back.find("F10"), Some(Action::MenuBar));
        assert_eq!(back.find("Ctrl+Alt+J"), Some(Action::Journal));
    }

    #[test]
    fn a_bar_key_the_writer_chose_is_saved() {
        let mut s = Shortcuts::default();
        s.bindings.retain(|b| b.action != Action::MenuBar);
        s.bindings.push(Binding {
            chord: "Ctrl+Alt+M".into(),
            action: Action::MenuBar,
        });
        s.validate().unwrap();
        let saved = s.to_saved();
        assert_eq!(
            saved.chord_for(Action::MenuBar).as_deref(),
            Some("Ctrl+Alt+M")
        );
        assert!(serde_json::to_string(&saved).unwrap().contains("menu-bar"));
    }
}
