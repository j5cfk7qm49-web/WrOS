//! Replayable, event-verified lessons. No mock editor and no manual "done" bit.
//! All 39 required lessons are cataloged; unsupported ones remain unavailable.
use crate::{
    project,
    sync::{Fingerprint, SaveOutcome},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    DashboardOpened,
    LayoutConfirmed,
    TextEdited,
    SecondaryEdited,
    SceneOpened,
    CorkboardOpened,
    WorkspaceSelected,
    FocusEntered,
    OutlineToggled,
    ShortcutAdded,
    GoalsChanged,
    SprintStarted,
    SpellingOpened,
    HistoryOpened,
    SystemOpened,
    RecoveryHelpOpened,
}
impl Event {
    pub fn safe_outside_demo(self) -> bool {
        matches!(
            self,
            Self::DashboardOpened
                | Self::LayoutConfirmed
                | Self::OutlineToggled
                | Self::FocusEntered
                | Self::WorkspaceSelected
                | Self::ShortcutAdded
                | Self::SystemOpened
                | Self::RecoveryHelpOpened
        )
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Lesson {
    pub id: u8,
    pub title: String,
    pub help_topic: String,
    pub event: Option<Event>,
    pub instructions: String,
}
pub fn lessons() -> &'static [Lesson] {
    static LESSONS: OnceLock<Vec<Lesson>> = OnceLock::new();
    LESSONS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/writingdeck/tutorial.json"))
            .expect("compiled tutorial catalog is valid")
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub version: u32,
    pub active: Option<u8>,
    pub completed: BTreeSet<u8>,
    pub skipped: BTreeSet<u8>,
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            version: 1,
            active: None,
            completed: BTreeSet::new(),
            skipped: BTreeSet::new(),
        }
    }
}
impl Progress {
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported tutorial progress version; preserved");
        }
        if self
            .completed
            .iter()
            .chain(&self.skipped)
            .chain(self.active.iter())
            .any(|id| !(1..=39).contains(id))
        {
            bail!("invalid tutorial lesson id");
        }
        Ok(())
    }
    pub fn start(&mut self, id: u8) -> Result<()> {
        let lesson = lessons()
            .iter()
            .find(|l| l.id == id)
            .context("unknown lesson")?;
        if lesson.event.is_none() {
            bail!("lesson is planned; its feature is not yet implemented");
        }
        self.active = Some(id);
        self.skipped.remove(&id);
        Ok(())
    }
    pub fn skip(&mut self, id: u8) {
        if (1..=39).contains(&id) {
            self.skipped.insert(id);
            if self.active == Some(id) {
                self.active = None;
            }
        }
    }
    pub fn observe(&mut self, event: Event, demo: bool) -> bool {
        let Some(id) = self.active else {
            return false;
        };
        // Writing exercises cannot be marked complete against a user's novel.
        if !demo && !event.safe_outside_demo() {
            return false;
        }
        if lessons()
            .iter()
            .any(|l| l.id == id && l.event == Some(event))
        {
            self.completed.insert(id);
            self.active = None;
            return true;
        }
        false
    }
}
pub struct Store {
    pub progress: Progress,
    path: PathBuf,
    seen: Option<Fingerprint>,
}
impl Store {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".grimoire/writingdeck/tutorial.json");
        let (progress, seen) = match fs::read_to_string(&path) {
            Ok(text) => {
                let p: Progress = serde_json::from_str(&text)
                    .context("invalid tutorial state; original preserved")?;
                p.validate()?;
                (p, Some(Fingerprint::of(&text)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Progress::default(), None),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            progress,
            path,
            seen,
        })
    }
    pub fn save(&mut self) -> Result<()> {
        self.progress.validate()?;
        let text = serde_json::to_string_pretty(&self.progress)? + "\n";
        fs::create_dir_all(self.path.parent().unwrap())?;
        match crate::sync::save_guarded(&self.path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                Ok(())
            }
            SaveOutcome::Conflict { .. } => {
                bail!("tutorial progress changed externally; not overwritten")
            }
        }
    }
}
pub fn is_demo(root: &Path) -> bool {
    fs::read_to_string(root.join(".writingdeck-demo"))
        .is_ok_and(|s| s == "WritingDeck safe tutorial project v1\n")
}
/// Claim a NEW directory exclusively. Existing directories, even empty ones,
/// are refused. No tutorial routine deletes or replaces a project.
pub fn create_demo(root: &Path) -> Result<()> {
    fs::create_dir(root)
        .context("tutorial needs a new directory; existing paths are never reused")?;
    fs::write(
        root.join(".writingdeck-demo"),
        "WritingDeck safe tutorial project v1\n",
    )?;
    project::scaffold_with(
        root,
        Some("WritingDeck Tutorial"),
        &project::Template::Blank.shape(),
    )?;
    let p = project::Project::load(root)?;
    let scene = p
        .nodes
        .iter()
        .find(|n| n.in_manuscript && n.kind == project::Kind::Scene)
        .context("demo template has a scene")?;
    let seed = "---\ntitle: The Brass Key\n---\n\nA brass key waited under the teacup. Mara had never seen its lock.\n\nThe door was definately open.\n";
    crate::atomic::write_text(&scene.path, seed)?;
    crate::atomic::write_text(
        &scene.path.with_file_name("02-The-Other-Door.md"),
        "---\ntitle: The Other Door\n---\n\nAcross the courtyard, someone extinguished a lamp.\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_required_lessons_are_present_and_unavailable_is_honest() {
        assert_eq!(lessons().len(), 39);
        for (i, l) in lessons().iter().enumerate() {
            assert_eq!(l.id as usize, i + 1);
            assert!(!l.instructions.is_empty());
        }
        let mut p = Progress::default();
        assert!(p.start(22).is_err());
        assert!(p.completed.is_empty());
    }
    #[test]
    fn actual_events_not_a_next_button_complete_lessons() {
        let mut p = Progress::default();
        p.start(4).unwrap();
        assert!(!p.observe(Event::SystemOpened, true));
        assert!(!p.observe(Event::TextEdited, false));
        assert!(p.observe(Event::TextEdited, true));
        assert!(p.completed.contains(&4));
        p.start(4).unwrap();
        assert!(p.observe(Event::TextEdited, true));
        p.skip(9);
        assert!(p.skipped.contains(&9));
    }
    #[test]
    fn demo_cannot_overwrite_an_existing_project_and_progress_round_trips() {
        let root = std::env::temp_dir().join(format!("wd-tutorial-demo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        create_demo(&root).unwrap();
        assert!(is_demo(&root));
        assert!(create_demo(&root).is_err());
        let mut s = Store::load(&root).unwrap();
        s.progress.start(1).unwrap();
        s.progress.observe(Event::DashboardOpened, true);
        s.save().unwrap();
        assert_eq!(Store::load(&root).unwrap().progress, s.progress);
        fs::remove_dir_all(root).unwrap();
    }
}
