//! Versioned, additive WritingDeck layout state. Manuscript files are untouched.
//! Unknown or corrupt configuration is an error, never silently overwritten.
use crate::sync::{Fingerprint, SaveOutcome};
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
    Battery,
    Backup,
    System,
}
impl Widget {
    pub const ALL: [Self; 10] = [
        Self::Resume,
        Self::Manuscript,
        Self::DailyGoal,
        Self::NovelGoal,
        Self::WeeklyProgress,
        Self::Sprint,
        Self::RecentScenes,
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
            Self::Battery => "Battery",
            Self::Backup => "Backup status",
            Self::System => "System status",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetConfig {
    pub widget: Widget,
    pub visible: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub document: Option<PathBuf>,
    pub line: usize,
    pub column: usize,
    pub scroll: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub outline: bool,
    pub outline_width: u16,
    pub focus: bool,
    pub primary: Position,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            version: 1,
            selected: 0,
            workspaces: ["Drafting", "Planning", "Research", "Revision", "Focus"]
                .into_iter()
                .map(|name| Workspace {
                    name: name.into(),
                    outline: name != "Focus",
                    outline_width: 30,
                    focus: name == "Focus",
                    primary: Position::default(),
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
        if self.version != 1 {
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
            if let Some(p) = &w.primary.document {
                relative(p)?;
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
                let state: State = serde_json::from_str(&text)
                    .context("invalid layout JSON; original preserved")?;
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
        };
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
}
