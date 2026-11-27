use super::Page;
use crate::app::{App, Overlay};
use crate::palette::Action;
use grimoire_core::tutorial;
use std::fs;
use wros_sys::shortcuts::{self, Action as ShortcutAction, Binding};

pub(super) const SHORTCUT_ACTIONS: [(ShortcutAction, &str); 22] = [
    (ShortcutAction::SystemSettings, "System settings"),
    (ShortcutAction::Tutorial, "Interactive tutorial"),
    (ShortcutAction::Outline, "Toggle outline"),
    (ShortcutAction::Focus, "Focus mode"),
    (ShortcutAction::Save, "Save"),
    (ShortcutAction::Find, "Find in scene"),
    (ShortcutAction::Palette, "Find anything"),
    (ShortcutAction::Secondary, "Editable secondary pane"),
    (ShortcutAction::Pinboard, "Pinboard"),
    (ShortcutAction::Pins, "Toggle pins sidebar"),
    (ShortcutAction::PinCurrent, "Pin current document"),
    (ShortcutAction::Journal, "Writing journal"),
    (ShortcutAction::Scratchpad, "Scratchpad"),
    (ShortcutAction::Handoff, "Session handoff"),
    (ShortcutAction::Productivity, "Productivity ledger"),
    (ShortcutAction::MeasuredGoals, "Advanced goals"),
    (ShortcutAction::Calendar, "Calendar and rest days"),
    (ShortcutAction::AdvancedSprints, "Advanced sprints"),
    (ShortcutAction::Achievements, "Milestone collection"),
    (ShortcutAction::DailyChallenges, "Daily challenges"),
    (ShortcutAction::StorySpark, "Story Spark"),
    (ShortcutAction::MenuBar, "Open the menu bar"),
];

impl App {
    pub(crate) fn deck_shortcut(&mut self, action: ShortcutAction) {
        match action {
            ShortcutAction::SystemSettings => self.open_system_settings(),
            ShortcutAction::Tutorial => self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 }),
            ShortcutAction::Outline => self.toggle_outline(),
            ShortcutAction::Secondary => self.toggle_secondary(),
            ShortcutAction::Focus => self.toggle_focus_mode(),
            ShortcutAction::Save => self.save(),
            ShortcutAction::Find => self.open_find(),
            ShortcutAction::Palette => self.open_palette(),
            ShortcutAction::Pinboard => self.open_pinboard(),
            ShortcutAction::Pins => self.toggle_pins(),
            ShortcutAction::PinCurrent => self.pin_current(),
            ShortcutAction::Journal => self.open_journal(),
            ShortcutAction::Scratchpad => self.open_scratchpad(),
            ShortcutAction::Handoff => self.open_handoff(),
            ShortcutAction::Productivity => self.run_action(Action::Productivity),
            ShortcutAction::MeasuredGoals => self.run_action(Action::MeasuredGoals),
            ShortcutAction::Calendar => self.run_action(Action::Calendar),
            ShortcutAction::AdvancedSprints => self.run_action(Action::AdvancedSprints),
            ShortcutAction::Achievements => self.run_action(Action::Achievements),
            ShortcutAction::DailyChallenges => self.run_action(Action::DailyChallenges),
            ShortcutAction::StorySpark => self.run_action(Action::StorySpark),
            ShortcutAction::MenuBar => self.open_menu(),
        }
    }
    pub(crate) fn add_deck_shortcut(&mut self, chord: &str) {
        let Overlay::Deck(Page::Shortcuts { sel, capture: true }) = self.overlay else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            if !self.deck.shortcuts_writable {
                anyhow::bail!("original shortcut file is unreadable; preserved");
            }
            let chord = shortcuts::canonical(chord)?;
            let action = SHORTCUT_ACTIONS[sel].0;
            let mut shortcuts = self.deck.shortcuts.clone();
            shortcuts.bindings.retain(|b| b.chord != chord);
            if action == ShortcutAction::MenuBar {
                // One key for the bar: the new one replaces F10 (Esc still opens it).
                shortcuts
                    .bindings
                    .retain(|b| b.action != ShortcutAction::MenuBar);
            }
            shortcuts.bindings.push(Binding { chord, action });
            shortcuts.validate()?;
            let text = serde_json::to_string_pretty(&shortcuts.to_saved())? + "\n";
            fs::create_dir_all(
                self.deck
                    .shortcuts_path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("missing shortcuts directory"))?,
            )?;
            match grimoire_core::sync::save_guarded(
                &self.deck.shortcuts_path,
                &text,
                self.deck.shortcuts_seen,
            )? {
                grimoire_core::sync::SaveOutcome::Written => {}
                grimoire_core::sync::SaveOutcome::Conflict { .. } => {
                    anyhow::bail!("shortcuts changed externally; not overwritten")
                }
            }
            self.deck.shortcuts = shortcuts;
            self.deck.shortcuts_seen = Some(grimoire_core::sync::Fingerprint::of(&text));
            Ok(())
        })();
        self.overlay = Overlay::Deck(Page::Shortcuts {
            sel,
            capture: false,
        });
        match result {
            Ok(()) => {
                self.deck.note = "Shortcut alias saved; established bindings are unchanged.".into();
                self.observe_deck(tutorial::Event::ShortcutAdded);
            }
            Err(e) => self.deck.note = format!("Shortcut not saved: {e}"),
        }
    }
}
