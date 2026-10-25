//! Versioned, additive WritingDeck layout state. Manuscript files are untouched.
//! Unknown or corrupt configuration is an error, never silently overwritten.
use crate::{
    documents::Pane,
    sync::{Fingerprint, SaveOutcome},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Widget {
    Resume,
    Manuscript,
    DailyGoal,
    NovelGoal,
    WeeklyProgress,
    Sprint,
    RecentScenes,
    Pins,
    Battery,
    Backup,
    System,
}
impl Widget {
    pub const ALL: [Self; 11] = [
        Self::Resume,
        Self::Manuscript,
        Self::DailyGoal,
        Self::NovelGoal,
        Self::WeeklyProgress,
        Self::Sprint,
        Self::RecentScenes,
        Self::Pins,
        Self::Battery,
        Self::Backup,
        Self::System,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Resume => "Resume writing",
            Self::Manuscript => "Current manuscript",
            Self::DailyGoal => "Daily writing goal",
            Self::NovelGoal => "Novel word goal",
            Self::WeeklyProgress => "Weekly progress",
            Self::Sprint => "Writing sprint",
            Self::RecentScenes => "Recent scenes",
            Self::Pins => "Dashboard pins",
            Self::Battery => "Battery",
            Self::Backup => "Backup status",
            Self::System => "System status",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetConfig {
    pub widget: Widget,
    pub visible: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub document: Option<PathBuf>,
    pub line: usize,
    pub column: usize,
    pub scroll: usize,
    #[serde(default)]
    pub anchor: Option<(usize, usize)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub name: String,
    pub outline: bool,
    pub outline_width: u16,
    pub focus: bool,
    pub primary: Position,
    #[serde(default)]
    pub secondary: Option<Position>,
    #[serde(default)]
    pub secondary_visible: bool,
    #[serde(default = "default_secondary_percent")]
    pub secondary_percent: u16,
    #[serde(default)]
    pub active_editor: Pane,
}
fn default_secondary_percent() -> u16 {
    40
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub selected: usize,
    pub workspaces: Vec<Workspace>,
    pub dashboard: Vec<WidgetConfig>,
    pub launch_dashboard: bool,
    pub low_power: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 2,
            selected: 0,
            workspaces: ["Drafting", "Planning", "Research", "Revision", "Focus"]
                .into_iter()
                .map(|name| Workspace {
                    name: name.into(),
                    outline: name != "Focus",
                    outline_width: 30,
                    focus: name == "Focus",
                    primary: Position::default(),
                    secondary: None,
                    secondary_visible: false,
                    secondary_percent: default_secondary_percent(),
                    active_editor: Pane::Primary,
                })
                .collect(),
            dashboard: Widget::ALL
                .into_iter()
                .map(|widget| WidgetConfig {
                    widget,
                    visible: true,
                })
                .collect(),
            launch_dashboard: false,
            low_power: false,
        }
    }
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.version != 2 {
            bail!(
                "unsupported WritingDeck layout version {}; file preserved",
                self.version
            );
        }
        if self.workspaces.is_empty()
            || self.workspaces.len() > 64
            || self.selected >= self.workspaces.len()
        {
            bail!("invalid workspace selection/count");
        }
        let mut names = std::collections::BTreeSet::new();
        for w in &self.workspaces {
            if w.name.trim().is_empty()
                || w.name.chars().count() > 64
                || !names.insert(w.name.clone())
            {
                bail!("workspace names must be unique, nonempty and at most 64 characters");
            }
            if !(18..=60).contains(&w.outline_width) {
                bail!("outline width must be 18..60 columns");
            }
            if !(25..=75).contains(&w.secondary_percent) {
                bail!("secondary pane proportion must be 25..75 percent");
            }
            for position in std::iter::once(&w.primary).chain(w.secondary.iter()) {
                if let Some(p) = &position.document {
                    relative(p)?;
                }
            }
        }
        let mut widgets = vec![];
        for w in &self.dashboard {
            if widgets.contains(&w.widget) {
                bail!("duplicate dashboard widget");
            }
            widgets.push(w.widget);
        }
        Ok(())
    }
    pub fn current(&self) -> &Workspace {
        &self.workspaces[self.selected]
    }
    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.selected]
    }
}
pub fn relative(p: &Path) -> Result<()> {
    if p.as_os_str().is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("document path must stay inside this project");
    }
    Ok(())
}

pub struct Store {
    pub state: State,
    path: PathBuf,
    seen: Option<Fingerprint>,
}
impl Store {
    pub fn path(root: &Path) -> PathBuf {
        root.join(".grimoire/writingdeck/layout.json")
    }
    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::path(root);
        let (state, seen) = match fs::read_to_string(&path) {
            Ok(text) => {
                if text.len() > 1_048_576 {
                    bail!("layout exceeds 1 MiB; preserved without loading");
                }
                let mut state: State = serde_json::from_str(&text)
                    .context("invalid layout JSON; original preserved")?;
                // Additive v1 -> v2 migration is in memory only. Read-only
                // inspection never rewrites the file; explicit save is guarded.
                if state.version == 1 {
                    state.version = 2;
                }
                state.validate()?;
                (state, Some(Fingerprint::of(&text)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (State::default(), None),
            Err(e) => return Err(e).context("reading workspace state; original preserved"),
        };
        Ok(Self { state, path, seen })
    }
    pub fn save(&mut self) -> Result<()> {
        self.state.validate()?;
        let text = serde_json::to_string_pretty(&self.state)? + "\n";
        fs::create_dir_all(self.path.parent().unwrap())?;
        match crate::sync::save_guarded(&self.path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                Ok(())
            }
            SaveOutcome::Conflict { .. } => {
                bail!("workspace state changed outside this instance; not overwritten")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wd-layout-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn layouts_round_trip_positions_and_widget_order() {
        let root = root("roundtrip");
        let mut s = Store::load(&root).unwrap();
        s.state.current_mut().primary = Position {
            document: Some("manuscript/chapter/scene.md".into()),
            line: 22,
            column: 6,
            scroll: 17,
            anchor: Some((21, 4)),
        };
        s.state.current_mut().secondary = Some(Position {
            document: Some("notes/character.md".into()),
            line: 7,
            column: 9,
            scroll: 4,
            anchor: Some((8, 2)),
        });
        s.state.current_mut().secondary_visible = true;
        s.state.current_mut().active_editor = Pane::Secondary;
        s.state.current_mut().secondary_percent = 55;
        s.state.dashboard.swap(0, 2);
        s.state.dashboard[1].visible = false;
        s.save().unwrap();
        assert_eq!(Store::load(&root).unwrap().state, s.state);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_future_and_externally_changed_state_is_never_overwritten() {
        let root = root("safe");
        let mut s = Store::load(&root).unwrap();
        s.save().unwrap();
        let mut other = Store::load(&root).unwrap();
        s.state.low_power = true;
        s.save().unwrap();
        other.state.launch_dashboard = true;
        assert!(other.save().is_err());
        fs::write(Store::path(&root), "{\"version\":999}").unwrap();
        assert!(Store::load(&root).is_err());
        assert_eq!(
            fs::read_to_string(Store::path(&root)).unwrap(),
            "{\"version\":999}"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn old_layout_migration_is_read_only_until_a_guarded_explicit_save() {
        let root = root("migration");
        let mut old = serde_json::to_value(State::default()).unwrap();
        old["version"] = 1.into();
        for workspace in old["workspaces"].as_array_mut().unwrap() {
            for key in [
                "secondary",
                "secondary_visible",
                "secondary_percent",
                "active_editor",
            ] {
                workspace.as_object_mut().unwrap().remove(key);
            }
            workspace["primary"]
                .as_object_mut()
                .unwrap()
                .remove("anchor");
        }
        let original = serde_json::to_string_pretty(&old).unwrap();
        fs::create_dir_all(Store::path(&root).parent().unwrap()).unwrap();
        fs::write(Store::path(&root), &original).unwrap();
        let mut loaded = Store::load(&root).unwrap();
        assert_eq!(loaded.state.version, 2);
        assert_eq!(loaded.state.current().secondary, None);
        assert_eq!(loaded.state.current().secondary_percent, 40);
        assert_eq!(fs::read_to_string(Store::path(&root)).unwrap(), original);
        loaded.save().unwrap();
        assert_eq!(Store::load(&root).unwrap().state, loaded.state);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn secondary_traversal_and_unsafe_ratios_are_rejected() {
        let mut state = State::default();
        state.current_mut().secondary = Some(Position {
            document: Some("../escape.md".into()),
            ..Position::default()
        });
        assert!(state.validate().is_err());
        state.current_mut().secondary = None;
        state.current_mut().secondary_percent = 99;
        assert!(state.validate().is_err());
    }
    #[test]
    fn traversal_and_duplicate_names_are_rejected() {
        for p in [
            "../outside.md",
            "/etc/shadow",
            "manuscript/../../outside.md",
        ] {
            assert!(relative(Path::new(p)).is_err());
        }
        let mut s = State::default();
        s.workspaces[1].name = s.workspaces[0].name.clone();
        assert!(s.validate().is_err());
    }
    #[test]
    fn unknown_layout_fields_at_every_level_are_preserved_not_silently_dropped() {
        let root = root("unknown-fields");
        let path = Store::path(&root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for level in 0..4 {
            let mut value = serde_json::to_value(State::default()).unwrap();
            let target = match level {
                0 => &mut value,
                1 => &mut value["workspaces"][0],
                2 => &mut value["workspaces"][0]["primary"],
                _ => &mut value["dashboard"][0],
            };
            target["future_setting"] = serde_json::json!({"preserve":"me"});
            let text = serde_json::to_string_pretty(&value).unwrap();
            fs::write(&path, &text).unwrap();
            assert!(Store::load(&root).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        let _ = fs::remove_dir_all(root);
    }
}
