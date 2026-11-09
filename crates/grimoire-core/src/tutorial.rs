//! Replayable, event-verified lessons. No mock editor and no manual "done" bit.
//! Original 39 required lessons, supplemental opt-in model grammar, and lessons for the
//! system features added since; a lesson whose feature is missing stays unavailable.
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
    NovelCreated,
    TextEdited,
    SecondaryEdited,
    PinCreated,
    PinAnchorRestored,
    JournalReflectionSaved,
    ScratchpadPromoted,
    HandoffReopened,
    SceneOpened,
    CorkboardOpened,
    WorkspaceSelected,
    FocusEntered,
    OutlineToggled,
    ShortcutAdded,
    GoalsChanged,
    CalendarConfigured,
    AchievementReviewed,
    DailyChallengeCompleted,
    SprintStarted,
    SparkPlotSaved,
    SparkExerciseFavorited,
    SparkCombinedCaptured,
    SpellingOpened,
    GrammarApplied,
    DictionarySenseReviewed,
    ProseStatsReviewed,
    NabokovFindingReviewed,
    ProselintFindingReviewed,
    VoiceProfileCompared,
    ContinuityReviewed,
    HistoryOpened,
    AnnotationReanchored,
    MarkdownExported,
    ManuscriptFootnotesExported,
    SystemOpened,
    RecoveryHelpOpened,
    BackupRestored,
    SyncCompleted,
    PhoneLinked,
    WarmthKept,
    GoBackReviewed,
    WifiScanned,
    SessionUnlocked,
    LidChoiceSaved,
    BatteryWarningShown,
    DriveFolderListed,
    MergePractised,
    StartScreenSaved,
}
impl Event {
    pub fn safe_outside_demo(self) -> bool {
        matches!(
            self,
            Self::DictionarySenseReviewed
                | Self::ProseStatsReviewed
                | Self::NabokovFindingReviewed
                | Self::ProselintFindingReviewed
                | Self::VoiceProfileCompared
                | Self::DashboardOpened
                | Self::LayoutConfirmed
                | Self::NovelCreated
                | Self::OutlineToggled
                | Self::FocusEntered
                | Self::WorkspaceSelected
                | Self::ShortcutAdded
                | Self::SystemOpened
                | Self::RecoveryHelpOpened
                | Self::BackupRestored
                | Self::SyncCompleted
                | Self::PhoneLinked
                | Self::WarmthKept
                | Self::GoBackReviewed
                | Self::WifiScanned
                | Self::SessionUnlocked
                | Self::LidChoiceSaved
                | Self::BatteryWarningShown
                | Self::DriveFolderListed
                | Self::StartScreenSaved
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
        serde_json::from_str(include_str!("../assets/wros/tutorial.json"))
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
    /// Lessons a newer WrOS added stay in the file, so going back and forth keeps
    /// them, but this catalog never counts, starts or completes them.
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported tutorial progress version; preserved");
        }
        if self
            .completed
            .iter()
            .chain(&self.skipped)
            .chain(self.active.iter())
            .any(|id| *id == 0)
        {
            bail!("invalid tutorial lesson id");
        }
        Ok(())
    }
    /// Completed lessons this catalog has.
    pub fn completed_lessons(&self) -> impl Iterator<Item = u8> + '_ {
        self.completed
            .iter()
            .copied()
            .filter(|id| lessons().iter().any(|lesson| lesson.id == *id))
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
        if lessons().iter().any(|lesson| lesson.id == id) {
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
    fs::read_to_string(root.join(".wros-demo"))
        .is_ok_and(|s| s == "WrOS safe tutorial project v1\n")
        || fs::read_to_string(root.join(".writingdeck-demo"))
            .is_ok_and(|s| s == "WritingDeck safe tutorial project v1\n")
}
/// Claim a NEW directory exclusively. Existing directories, even empty ones,
/// are refused. No tutorial routine deletes or replaces a project.
pub fn create_demo(root: &Path) -> Result<()> {
    fs::create_dir(root)
        .context("tutorial needs a new directory; existing paths are never reused")?;
    fs::write(root.join(".wros-demo"), "WrOS safe tutorial project v1\n")?;
    // Older editors still recognize this disposable book after going back.
    fs::write(
        root.join(".writingdeck-demo"),
        "WritingDeck safe tutorial project v1\n",
    )?;
    project::scaffold_with(
        root,
        Some("WrOS Tutorial"),
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
    // Ship an immediately usable semantic-anchor exercise, not just a
    // synthetic test heading or a request to modify the demo manuscript.
    let mut practice = String::from(
        "---\ntitle: Pin Practice\n---\n\n## The Courtyard\n\nA lamp lit the courtyard wall. This note is the single source for the pin exercise.\n\n",
    );
    for row in 1..=24 {
        practice.push_str(&format!(
            "Observation {row:02}: Mara followed the lamplight along the courtyard wall.\n"
        ));
    }
    crate::atomic::write_text(&root.join("notes/pin-practice.md"), &practice)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_required_lessons_are_present_and_unavailable_is_honest() {
        assert_eq!(lessons().len(), 50);
        for (i, l) in lessons().iter().enumerate() {
            assert_eq!(l.id as usize, i + 1);
            assert!(!l.instructions.is_empty());
            assert!(l.event.is_some(), "lesson {} has no real completion", l.id);
        }
        let mut p = Progress::default();
        assert!(p.start(51).is_err());
        assert!(p.active.is_none() && p.completed.is_empty());
    }
    #[test]
    fn system_lessons_complete_only_on_their_own_finished_outcome_even_in_a_real_book() {
        let cases = [
            (36, Event::BackupRestored),
            (37, Event::SyncCompleted),
            (41, Event::PhoneLinked),
            (42, Event::WarmthKept),
            (43, Event::GoBackReviewed),
            (44, Event::WifiScanned),
            (45, Event::SessionUnlocked),
            (46, Event::LidChoiceSaved),
            (47, Event::BatteryWarningShown),
            (48, Event::DriveFolderListed),
            (50, Event::StartScreenSaved),
        ];
        for (id, event) in cases {
            let mut p = Progress::default();
            p.start(id).unwrap();
            // Opening Settings, or another lesson's outcome, is not this one.
            assert!(!p.observe(Event::SystemOpened, false));
            for (_, other) in cases.iter().filter(|(other, _)| *other != id) {
                assert!(!p.observe(*other, false), "{other:?} completed {id}");
            }
            assert!(p.observe(event, false), "{event:?} in a real book");
            assert_eq!(p.completed, BTreeSet::from([id]));
        }
    }
    #[test]
    fn progress_from_a_newer_wros_is_kept_but_never_counted_or_awarded() {
        let root = std::env::temp_dir().join(format!("wd-tutorial-newer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join(".grimoire/writingdeck/tutorial.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"version":1,"active":60,"completed":[4,58],"skipped":[59]}"#,
        )
        .unwrap();
        let mut s = Store::load(&root).unwrap();
        assert_eq!(s.progress.completed_lessons().collect::<Vec<_>>(), [4]);
        assert!(!s.progress.observe(Event::TextEdited, true));
        s.progress.start(5).unwrap();
        assert!(s.progress.observe(Event::SceneOpened, true));
        s.save().unwrap();
        let again = Store::load(&root).unwrap().progress;
        assert_eq!(again.completed, BTreeSet::from([4, 5, 58]));
        assert_eq!(again.skipped, BTreeSet::from([59]));
        assert_eq!(again.completed_lessons().count(), 2);
        fs::remove_dir_all(root).unwrap();
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
        p.start(35).unwrap();
        assert!(!p.observe(Event::MarkdownExported, false));
        assert!(!p.completed.contains(&35));
        assert!(p.observe(Event::MarkdownExported, true));
        assert!(p.completed.contains(&35));
        p.start(34).unwrap();
        assert!(!p.observe(Event::MarkdownExported, true));
        assert!(!p.observe(Event::ManuscriptFootnotesExported, false));
        assert!(p.observe(Event::ManuscriptFootnotesExported, true));
        assert!(p.completed.contains(&34));
        p.start(3).unwrap();
        assert!(p.observe(Event::NovelCreated, false));
        assert!(p.completed.contains(&3));
        p.start(49).unwrap();
        assert!(!p.observe(Event::MergePractised, false));
        assert!(p.observe(Event::MergePractised, true));
    }
    #[test]
    fn demo_cannot_overwrite_an_existing_project_and_progress_round_trips() {
        let root = std::env::temp_dir().join(format!("wd-tutorial-demo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        create_demo(&root).unwrap();
        assert!(is_demo(&root));
        let project = project::Project::load(&root).unwrap();
        let practice = project
            .nodes
            .iter()
            .find(|n| n.path.ends_with("notes/pin-practice.md"))
            .unwrap();
        assert_eq!(crate::pins::headings(&practice.body).len(), 1);
        assert!(practice.body.lines().count() > 24);
        assert!(create_demo(&root).is_err());
        let mut s = Store::load(&root).unwrap();
        s.progress.start(1).unwrap();
        s.progress.observe(Event::DashboardOpened, true);
        s.save().unwrap();
        assert_eq!(Store::load(&root).unwrap().progress, s.progress);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod supplemental_grammar_tests {
    use super::*;
    #[test]
    fn spelling_history_does_not_grant_new_grammar_completion_and_progress_is_bounded_by_catalog() {
        let mut p = Progress::default();
        p.start(26).unwrap();
        assert!(p.observe(Event::SpellingOpened, true));
        p.start(40).unwrap();
        assert!(!p.observe(Event::SpellingOpened, true));
        assert!(!p.observe(Event::GrammarApplied, false));
        assert!(p.completed.contains(&26) && !p.completed.contains(&40));
        assert!(p.observe(Event::GrammarApplied, true));
        p.validate().unwrap();
        p.skip(40);
        assert!(p.skipped.contains(&40));
        p.completed.insert(0);
        assert!(p.validate().is_err());
    }
}
