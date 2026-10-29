//! Experimental bounded named-snapshot and scene-variant models. No editor,
//! diff or restore integration yet; existing manuscript history is unchanged.
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
    pub scene_snapshots: HashMap<usize, String>,
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
            schema_version: 1,
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
        scenes: HashMap<usize, String>,
    ) -> Result<u64> {
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
            name: name.into().trim().to_string(),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            description: description.into().trim().to_string(),
            scene_snapshots: scenes,
            total_words,
        });
        Ok(id)
    }

    pub fn get_scene_snapshot(&self, milestone_id: u64, scene_id: usize) -> Option<&str> {
        let m = self.milestones.iter().find(|m| m.id == milestone_id)?;
        m.scene_snapshots.get(&scene_id).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneVariant {
    pub id: u64,
    pub scene_id: usize,
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
            schema_version: 1,
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
        scene_id: usize,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        self.variants.push(SceneVariant {
            id,
            scene_id,
            name: name.into().trim().to_string(),
            content: content.into(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        });
        Ok(id)
    }

    pub fn variants_for_scene(&self, scene_id: usize) -> Vec<&SceneVariant> {
        self.variants
            .iter()
            .filter(|v| v.scene_id == scene_id)
            .collect()
    }

    pub fn remove_variant(&mut self, id: u64) -> bool {
        let len_before = self.variants.len();
        self.variants.retain(|v| v.id != id);
        self.variants.len() < len_before
    }
}

impl Validate for MilestoneStore {
    fn validate(&self) -> Result<()> {
        crate::story_state::identities(
            self.schema_version,
            self.next_id,
            self.milestones.iter().map(|m| m.id),
        )?;
        for m in &self.milestones {
            ensure!(!m.name.trim().is_empty(), "milestone name cannot be empty");
            crate::story_state::text(&m.name, 256)?;
            crate::story_state::text(&m.description, 16_384)?;
            ensure!(m.scene_snapshots.len() <= 4096, "too many snapshot scenes");
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
        crate::story_state::identities(
            self.schema_version,
            self.next_id,
            self.variants.iter().map(|v| v.id),
        )?;
        for v in &self.variants {
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
    }

    #[test]
    fn variant_store_tracks_scene_alternatives() {
        let mut store = VariantStore::default();
        store
            .save_variant(42, "First-person POV", "I walked into the dark hallway.")
            .unwrap();
        store
            .save_variant(42, "Third-person POV", "She walked into the dark hallway.")
            .unwrap();

        let vars = store.variants_for_scene(42);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "First-person POV");
        assert_eq!(vars[1].name, "Third-person POV");
    }
}
