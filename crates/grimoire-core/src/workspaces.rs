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
    io::Read,
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
    Journal,
    Scratchpad,
    Handoff,
    Battery,
    Backup,
    System,
    Accounting,
    AdvancedGoals,
    Calendar,
    AdvancedSprints,
    Achievements,
    DailyChallenges,
    StorySpark,
}
impl Widget {
    pub const ALL: [Self; 21] = [
        Self::Resume,
        Self::Manuscript,
        Self::DailyGoal,
        Self::NovelGoal,
        Self::WeeklyProgress,
        Self::Sprint,
        Self::RecentScenes,
        Self::Pins,
        Self::Journal,
        Self::Scratchpad,
        Self::Handoff,
        Self::Battery,
        Self::Backup,
        Self::System,
        Self::Accounting,
        Self::AdvancedGoals,
        Self::Calendar,
        Self::AdvancedSprints,
        Self::Achievements,
        Self::DailyChallenges,
        Self::StorySpark,
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
            Self::Journal => "Writing journal",
            Self::Scratchpad => "Scratchpad",
            Self::Handoff => "Session handoff",
            Self::Battery => "Battery",
            Self::Backup => "Backup status",
            Self::System => "System status",
            Self::Accounting => "Recorded edit ledger",
            Self::AdvancedGoals => "Advanced goals",
            Self::Calendar => "Calendar / rest days",
            Self::AdvancedSprints => "Advanced sprints",
            Self::Achievements => "Milestone collection",
            Self::DailyChallenges => "Daily challenges",
            Self::StorySpark => "Story Spark",
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
    /// None follows all cards (including future cards); Some is an explicit set.
    #[serde(default)]
    pub pin_ids: Option<Vec<u64>>,
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
            version: 3,
            selected: 0,
            workspaces: ["Drafting", "Planning", "Research", "Revision", "Focus"]
                .into_iter()
                .map(|name| Workspace {
                    name: name.into(),
                    pin_ids: None,
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
        if self.version != 3 {
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
            if let Some(ids) = &w.pin_ids {
                let unique: std::collections::BTreeSet<_> = ids.iter().collect();
                if ids.len() > 256 || ids.contains(&0) || unique.len() != ids.len() {
                    bail!("invalid workspace pin set");
                }
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
        let (state, seen) = match fs::File::open(&path) {
            Ok(file) => {
                let mut text = String::new();
                file.take(1_048_577).read_to_string(&mut text)?;
                if text.len() > 1_048_576 {
                    bail!("layout exceeds 1 MiB; preserved without loading");
                }
                let mut state: State = serde_json::from_str(&text)
                    .context("invalid layout JSON; original preserved")?;
                // Additive v1/v2 -> v3 migration is in memory only. Read-only
                // inspection never rewrites the file; explicit save is guarded.
                if matches!(state.version, 1 | 2) {
                    state.version = 3;
                }
                state.validate()?;
                (state, Some(Fingerprint::of(&text)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (State::default(), None),
            Err(e) => return Err(e).context("reading workspace state; original preserved"),
        };
        Ok(Self { state, path, seen })
    }
    pub fn verify_current(&self) -> Result<()> {
        let current = match fs::File::open(&self.path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(1_048_577).read_to_end(&mut bytes)?;
                if bytes.len() > 1_048_576 {
                    bail!("layout exceeds 1 MiB; preserved");
                }
                Some(Fingerprint::of_bytes(&bytes))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if current != self.seen {
            bail!("layout changed externally; not adopted or overwritten");
        }
        Ok(())
    }
    pub fn save(&mut self) -> Result<()> {
        self.state.validate()?;
        let text = serde_json::to_string_pretty(&self.state)? + "\n";
        if text.len() > 1_048_576 {
            bail!("layout exceeds 1 MiB; original preserved");
        }
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

/// Explicit starter content, never auto-applied to existing layouts or prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Drafting,
    Planning,
    Research,
    Revision,
    Focus,
}
impl Preset {
    pub const ALL: [Self; 5] = [
        Self::Drafting,
        Self::Planning,
        Self::Research,
        Self::Revision,
        Self::Focus,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Drafting => "Drafting",
            Self::Planning => "Planning",
            Self::Research => "Research",
            Self::Revision => "Revision",
            Self::Focus => "Focus",
        }
    }
    pub fn source(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Drafting => Some(("notes/workspaces/drafting.md", "notes")),
            Self::Planning => Some(("notes/workspaces/planning.md", "notes")),
            Self::Research => Some(("research/workspace.md", "research")),
            Self::Revision => Some(("notes/workspaces/revision.md", "notes")),
            Self::Focus => None,
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
        assert_eq!(loaded.state.version, 3);
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

    #[test]
    fn v2_migrates_to_v3_without_rewriting_and_pin_sets_preserve_opaque_ids() {
        let root = std::env::temp_dir().join(format!("wd-v3-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(Store::path(&root).parent().unwrap()).unwrap();
        let mut value = serde_json::to_value(State::default()).unwrap();
        value["version"] = 2.into();
        for w in value["workspaces"].as_array_mut().unwrap() {
            w.as_object_mut().unwrap().remove("pin_ids");
        }
        let original = value.to_string();
        fs::write(Store::path(&root), &original).unwrap();
        let mut store = Store::load(&root).unwrap();
        assert_eq!(store.state.version, 3);
        assert!(store.state.current().pin_ids.is_none());
        assert_eq!(fs::read_to_string(Store::path(&root)).unwrap(), original);
        store.state.current_mut().pin_ids = Some(vec![17, 900]);
        store.save().unwrap();
        assert_eq!(
            Store::load(&root).unwrap().state.current().pin_ids,
            Some(vec![17, 900])
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn workspace_pin_ids_are_positive_unique_bounded_and_presets_are_explicit() {
        let mut state = State::default();
        for ids in [vec![0], vec![1, 1], (1..=257).collect()] {
            state.current_mut().pin_ids = Some(ids);
            assert!(state.validate().is_err());
        }
        state.current_mut().pin_ids = Some(vec![]);
        assert!(state.validate().is_ok());
        assert_eq!(Preset::ALL.len(), 5);
        assert!(Preset::Focus.source().is_none());
        assert_eq!(
            Preset::Research.source(),
            Some(("research/workspace.md", "research"))
        );
    }
    #[test]
    fn oversized_external_read_preflight_and_local_save_are_bounded_and_preserved() {
        let r = root("size-bounds");
        let mut store = Store::load(&r).unwrap();
        store.save().unwrap();
        let original = fs::read(&store.path).unwrap();
        let large = vec![b'x'; 1_048_578];
        fs::write(&store.path, &large).unwrap();
        assert!(Store::load(&r).is_err());
        assert!(store.verify_current().is_err());
        assert_eq!(fs::read(&store.path).unwrap(), large);
        fs::write(&store.path, &original).unwrap();
        store.state.workspaces[0].primary.document =
            Some(PathBuf::from(format!("{}.md", "x".repeat(1_048_577))));
        assert!(store.save().is_err());
        assert_eq!(fs::read(&store.path).unwrap(), original);
        fs::remove_dir_all(r).unwrap();
    }
}
