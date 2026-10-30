//! Bounded named-snapshot milestones and reversible scene-variant models.
//! Integrates with existing word-level diffing (`crate::history::diff`) and
//! preserves existing manuscript history and undo.
use crate::history::{self, Piece};
use crate::statefile::{Store, Validate};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Milestone {
    pub id: u64,
    pub name: String,
    pub timestamp: String,
    pub description: String,
    pub scene_snapshots: HashMap<u64, String>,
    pub total_words: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilestoneStore {
    pub schema_version: u32,
    pub next_id: u64,
    pub milestones: Vec<Milestone>,
}

impl Default for MilestoneStore {
    fn default() -> Self {
        Self {
            schema_version: 2,
            next_id: 0,
            milestones: Vec::new(),
        }
    }
}
impl MilestoneStore {
    pub fn path(book_root: &Path) -> PathBuf {
        book_root.join(".grimoire/writingdeck/milestones.json")
    }

    pub fn load(book_root: &Path) -> Result<Store<Self>> {
        crate::story_state::load(book_root, "milestones", Self::default())
    }

    pub fn create_milestone(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        scenes: HashMap<u64, String>,
    ) -> Result<u64> {
        let name = name.into().trim().to_string();
        let description = description.into().trim().to_string();
        ensure!(!name.is_empty(), "milestone name cannot be empty");
        crate::story_state::text(&name, 256)?;
        crate::story_state::text(&description, 16_384)?;
        ensure!(scenes.len() <= 4096, "too many snapshot scenes");
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        let total_words = scenes
            .values()
            .map(|text| crate::notes::count_words(text))
            .sum();

        self.milestones.push(Milestone {
            id,
            name,
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            description,
            scene_snapshots: scenes,
            total_words,
        });
        Ok(id)
    }

    pub fn get_scene_snapshot(&self, milestone_id: u64, scene_id: u64) -> Option<&str> {
        let m = self.milestones.iter().find(|m| m.id == milestone_id)?;
        m.scene_snapshots.get(&scene_id).map(|s| s.as_str())
    }

    pub fn diff_scene(
        &self,
        milestone_id: u64,
        scene_id: u64,
        current_body: &str,
    ) -> Option<Vec<Piece>> {
        let snapshot = self.get_scene_snapshot(milestone_id, scene_id)?;
        Some(history::diff(snapshot, current_body))
    }

    pub fn remove_milestone(&mut self, id: u64) -> bool {
        let before = self.milestones.len();
        self.milestones.retain(|m| m.id != id);
        self.milestones.len() < before
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneVariant {
    pub id: u64,
    pub scene_id: u64,
    pub name: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantStore {
    pub schema_version: u32,
    pub next_id: u64,
    pub variants: Vec<SceneVariant>,
}

impl Default for VariantStore {
    fn default() -> Self {
        Self {
            schema_version: 2,
            next_id: 0,
            variants: Vec::new(),
        }
    }
}
impl VariantStore {
    pub fn path(book_root: &Path) -> PathBuf {
        book_root.join(".grimoire/writingdeck/variants.json")
    }

    pub fn load(book_root: &Path) -> Result<Store<Self>> {
        crate::story_state::load(book_root, "variants", Self::default())
    }

    pub fn save_variant(
        &mut self,
        scene_id: u64,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<u64> {
        let name = name.into().trim().to_string();
        ensure!(!name.is_empty(), "variant name cannot be empty");
        crate::story_state::text(&name, 256)?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        self.variants.push(SceneVariant {
            id,
            scene_id,
            name,
            content: content.into(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        });
        Ok(id)
    }

    pub fn variants_for_scene(&self, scene_id: u64) -> Vec<&SceneVariant> {
        self.variants
            .iter()
            .filter(|v| v.scene_id == scene_id)
            .collect()
    }

    pub fn diff_variant(&self, variant_id: u64, current_body: &str) -> Option<Vec<Piece>> {
        let v = self.variants.iter().find(|v| v.id == variant_id)?;
        Some(history::diff(&v.content, current_body))
    }

    pub fn remove_variant(&mut self, id: u64) -> bool {
        let len_before = self.variants.len();
        self.variants.retain(|v| v.id != id);
        self.variants.len() < len_before
    }
}

impl Validate for MilestoneStore {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 2,
            "prototype scene identities require explicit migration; original preserved"
        );
        crate::story_state::identities(1, self.next_id, self.milestones.iter().map(|m| m.id))?;
        for m in &self.milestones {
            ensure!(!m.name.trim().is_empty(), "milestone name cannot be empty");
            crate::story_state::text(&m.name, 256)?;
            crate::story_state::text(&m.description, 16_384)?;
            ensure!(m.scene_snapshots.len() <= 4096, "too many snapshot scenes");
            ensure!(
                m.scene_snapshots.keys().all(|id| *id > 0),
                "invalid document identity"
            );
            ensure!(
                m.total_words
                    == m.scene_snapshots
                        .values()
                        .map(|s| crate::notes::count_words(s))
                        .sum::<usize>(),
                "milestone word count disagrees with saved snapshots"
            );
            ensure!(
                chrono::NaiveDateTime::parse_from_str(&m.timestamp, "%Y-%m-%d %H:%M").is_ok(),
                "invalid milestone date"
            );
        }
        Ok(())
    }
}
impl Validate for VariantStore {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 2,
            "prototype scene identities require explicit migration; original preserved"
        );
        crate::story_state::identities(1, self.next_id, self.variants.iter().map(|v| v.id))?;
        for v in &self.variants {
            ensure!(v.scene_id > 0, "invalid document identity");
            ensure!(!v.name.trim().is_empty(), "variant name cannot be empty");
            crate::story_state::text(&v.name, 256)?;
            ensure!(
                chrono::NaiveDateTime::parse_from_str(&v.created_at, "%Y-%m-%d %H:%M").is_ok(),
                "invalid variant date"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_totals_follow_existing_private_note_and_tk_word_policy() {
        let mut store = MilestoneStore::default();
        let scenes = HashMap::from([(1, "Public words. %% private three words %% TK".into())]);
        store.create_milestone("Draft", "", scenes).unwrap();
        assert_eq!(store.milestones[0].total_words, 2);
        store.validate().unwrap();
    }

    #[test]
    fn milestone_saves_scene_snapshots_and_calculates_words() {
        let mut store = MilestoneStore::default();
        let mut scenes = HashMap::new();
        scenes.insert(1, "The first line of the book.".into());
        scenes.insert(2, "Another short scene with six words.".into());

        let id = store
            .create_milestone("Draft 1 Complete", "Sent to first beta", scenes)
            .unwrap();
        let m = store.milestones.iter().find(|m| m.id == id).unwrap();
        assert_eq!(m.total_words, 12);
        assert_eq!(
            store.get_scene_snapshot(id, 1),
            Some("The first line of the book.")
        );
        let diff = store
            .diff_scene(id, 1, "The revised line of the book.")
            .unwrap();
        assert!(diff.iter().any(|p| matches!(p, Piece::Removed(_))));
        assert!(diff.iter().any(|p| matches!(p, Piece::Added(_))));
    }

    #[test]
    fn variant_store_tracks_scene_alternatives() {
        let mut store = VariantStore::default();
        let id = store
            .save_variant(42, "First-person POV", "I walked into the dark hallway.")
            .unwrap();
        store
            .save_variant(42, "Third-person POV", "She walked into the dark hallway.")
            .unwrap();

        let vars = store.variants_for_scene(42);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "First-person POV");
        assert_eq!(vars[1].name, "Third-person POV");
        let diff = store
            .diff_variant(id, "She walked into the dark hallway.")
            .unwrap();
        assert!(
            diff.iter()
                .any(|p| matches!(p, Piece::Removed(s) if s.contains("I")))
        );
    }
    #[test]
    fn schema_v1_index_shift_is_refused_and_schema_v2_scene_snapshots_survive_reordering() {
        let root = std::env::temp_dir().join(format!(
            "grimoire-milestone-schema-compat-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let chap = root.join("manuscript/01-Chapter-One");
        std::fs::create_dir_all(&chap).unwrap();
        std::fs::write(
            root.join("novel.toml"),
            "title = \"Index Shift Reproduction\"\n",
        )
        .unwrap();
        let scene_a = chap.join("01-Scene-A.md");
        std::fs::write(&scene_a, "Scene A original text.\n").unwrap();

        // 1. A schema-1 file keyed by project-tree index 1 must be refused when
        // an earlier scene B is inserted, never attaching A's text to B.
        let path = MilestoneStore::path(&root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let v1_bytes = br#"{"schema_version":1,"next_id":1,"milestones":[{"id":1,"name":"Old V1","timestamp":"2026-10-30 10:00","description":"","scene_snapshots":{"1":"Scene A historical text."},"total_words":4}]}"#;
        std::fs::write(&path, v1_bytes).unwrap();
        let scene_b = chap.join("00-Scene-B.md");
        std::fs::write(&scene_b, "Scene B newly inserted.\n").unwrap();
        let err = MilestoneStore::load(&root)
            .err()
            .expect("legacy data must be refused")
            .to_string();
        assert!(err.contains("explicit migration"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), v1_bytes);
        assert!(!path.with_extension("json.v1.bak").exists());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&scene_b).unwrap();

        // 2. Schema-2 `scene_snapshots` keyed by shared durable ReviewNotes IDs
        // stays bound to Scene A after Scene B is inserted before it.
        let project = crate::project::Project::load(&root).unwrap();
        let mut notes = crate::review_notes::ReviewNotes::load(&root).unwrap();
        let old_index = project
            .nodes
            .iter()
            .position(|n| n.path == scene_a)
            .unwrap();
        let path_a = "manuscript/01-Chapter-One/01-Scene-A.md";
        let id_a = notes.update(|value| value.register(path_a)).unwrap();

        let mut store = MilestoneStore::load(&root).unwrap();
        let mut snaps = HashMap::new();
        snaps.insert(id_a, "Scene A historical text.".into());
        let ms_id = store
            .update(|value| value.create_milestone("Draft 2", "Durable IDs", snaps))
            .unwrap();

        std::fs::write(&scene_b, "Scene B newly inserted.\n").unwrap();
        let reloaded_project = crate::project::Project::load(&root).unwrap();
        let mut reloaded_notes = crate::review_notes::ReviewNotes::load(&root).unwrap();
        let new_index = reloaded_project
            .nodes
            .iter()
            .position(|n| n.path == scene_a)
            .unwrap();
        assert_ne!(old_index, new_index);
        let id_a_after = reloaded_notes
            .update(|value| value.register(path_a))
            .unwrap();
        assert_eq!(id_a, id_a_after);
        let id_b = reloaded_notes
            .update(|value| value.register("manuscript/01-Chapter-One/00-Scene-B.md"))
            .unwrap();
        assert_ne!(id_a, id_b);

        let reloaded_store = MilestoneStore::load(&root).unwrap();
        assert_eq!(
            reloaded_store.value.get_scene_snapshot(ms_id, id_a),
            Some("Scene A historical text.")
        );
        assert_eq!(reloaded_store.value.get_scene_snapshot(ms_id, id_b), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
