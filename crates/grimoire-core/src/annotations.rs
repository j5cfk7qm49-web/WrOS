//! Experimental quote-anchored annotation model. No editor or unified review
//! queue integration yet; uncertain/changed anchors remain unresolved.
use crate::statefile::{Store, Validate};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationStatus {
    Open,
    InProgress,
    Resolved,
}

impl AnnotationStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In Progress",
            Self::Resolved => "Resolved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: u64,
    pub scene_id: usize,
    pub scene_title: String,
    pub quote_anchor: String,
    pub prefix_context: String,
    pub suffix_context: String,
    pub offset_hint: usize,
    pub comment: String,
    pub status: AnnotationStatus,
    pub created_at: String,
}

impl Annotation {
    /// Locate the quote anchor in the current text of the scene.
    /// Returns an original UTF-8 byte range only when the exact quote and
    /// available context identify one occurrence. The offset is only a hint;
    /// never guess a replacement range from a case-folded string.
    pub fn locate_in(&self, text: &str) -> Option<(usize, usize)> {
        let anchor = self.quote_anchor.as_str();
        if anchor.is_empty() {
            return None;
        }

        let mut found = None;
        // char_indices also finds overlapping occurrences, unlike match_indices.
        for (start, _) in text.char_indices() {
            if !text[start..].starts_with(anchor) {
                continue;
            }
            let end = start + anchor.len();
            if !text[..start].ends_with(&self.prefix_context)
                || !text[end..].starts_with(&self.suffix_context)
            {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some((start, end));
        }
        found
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationStore {
    pub schema_version: u32,
    pub next_id: u64,
    pub annotations: Vec<Annotation>,
}

impl Default for AnnotationStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_id: 0,
            annotations: Vec::new(),
        }
    }
}
impl AnnotationStore {
    pub fn path(book_root: &Path) -> PathBuf {
        book_root.join(".grimoire/writingdeck/annotations.json")
    }

    pub fn load(book_root: &Path) -> Result<Store<Self>> {
        crate::story_state::load(book_root, "annotations", Self::default())
    }

    pub fn add(
        &mut self,
        scene_id: usize,
        scene_title: impl Into<String>,
        quote_anchor: impl Into<String>,
        offset_hint: usize,
        comment: impl Into<String>,
    ) -> Result<u64> {
        let quote_anchor = quote_anchor.into();
        ensure!(!quote_anchor.is_empty(), "annotation quote cannot be empty");
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        self.annotations.push(Annotation {
            id,
            scene_id,
            scene_title: scene_title.into().trim().to_string(),
            quote_anchor,
            prefix_context: String::new(),
            suffix_context: String::new(),
            offset_hint,
            comment: comment.into().trim().to_string(),
            status: AnnotationStatus::Open,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        });
        Ok(id)
    }

    pub fn resolve(&mut self, id: u64) -> bool {
        if let Some(ann) = self.annotations.iter_mut().find(|a| a.id == id) {
            ann.status = AnnotationStatus::Resolved;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let len_before = self.annotations.len();
        self.annotations.retain(|a| a.id != id);
        self.annotations.len() < len_before
    }

    pub fn open_count(&self) -> usize {
        self.annotations
            .iter()
            .filter(|a| a.status != AnnotationStatus::Resolved)
            .count()
    }

    pub fn for_scene(&self, scene_id: usize) -> Vec<&Annotation> {
        self.annotations
            .iter()
            .filter(|a| a.scene_id == scene_id)
            .collect()
    }
}

impl Validate for AnnotationStore {
    fn validate(&self) -> Result<()> {
        crate::story_state::identities(
            self.schema_version,
            self.next_id,
            self.annotations.iter().map(|a| a.id),
        )?;
        for a in &self.annotations {
            ensure!(
                !a.quote_anchor.is_empty(),
                "annotation quote cannot be empty"
            );
            for value in [
                &a.scene_title,
                &a.quote_anchor,
                &a.prefix_context,
                &a.suffix_context,
                &a.comment,
            ] {
                crate::story_state::text(value, 16_384)?;
            }
            ensure!(
                chrono::NaiveDateTime::parse_from_str(&a.created_at, "%Y-%m-%d %H:%M").is_ok(),
                "invalid annotation date"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_never_invent_offsets_after_unicode_folding_or_ambiguous_repetition() {
        let mut store = AnnotationStore::default();
        store.add(1, "Scene", "key", 0, "note").unwrap();
        let ann = &mut store.annotations[0];
        let text = "İ λ key";
        let (a, b) = ann.locate_in(text).unwrap();
        assert_eq!(&text[a..b], "key");
        assert_eq!(ann.locate_in("İ λ KEY"), None);
        assert_eq!(ann.locate_in("one key; another key"), None);
        ann.prefix_context = "another ".into();
        let (a, b) = ann.locate_in("one key; another key").unwrap();
        assert_eq!((a, b), (17, 20));
        assert_eq!(ann.locate_in("changed key"), None);
        ann.prefix_context.clear();
        ann.quote_anchor = "aa".into();
        assert_eq!(
            ann.locate_in("aaa"),
            None,
            "overlapping anchors are ambiguous too"
        );
    }

    #[test]
    fn guarded_annotation_update_round_trips_and_refuses_duplicate_ids_or_overflow() {
        let root =
            std::env::temp_dir().join(format!("wros-annotations-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut saved = AnnotationStore::load(&root).unwrap();
        let id = saved
            .update(|s| s.add(1, "Scene", "λ words", 0, "review"))
            .unwrap();
        assert_eq!(
            AnnotationStore::load(&root).unwrap().value.annotations[0].id,
            id
        );
        assert!(
            saved
                .update(|s| {
                    s.annotations.push(s.annotations[0].clone());
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(saved.value.annotations.len(), 1);
        let mut exhausted = AnnotationStore {
            next_id: u64::MAX,
            ..AnnotationStore::default()
        };
        assert!(exhausted.add(1, "Scene", "words", 0, "note").is_err());
        assert!(exhausted.annotations.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn anchor_survives_text_movements() {
        let mut store = AnnotationStore::default();
        let id = store
            .add(
                1,
                "Scene 1",
                "the silver key",
                10,
                "Clarify who hid this key",
            )
            .unwrap();
        let ann = store.annotations.iter().find(|a| a.id == id).unwrap();

        let original = "He held the silver key in his trembling hand.";
        assert_eq!(ann.locate_in(original), Some((8, 22)));

        let modified = "Three hours later, he held the silver key in his pocket.";
        assert_eq!(ann.locate_in(modified), Some((27, 41)));
    }

    #[test]
    fn resolve_marks_status_resolved() {
        let mut store = AnnotationStore::default();
        let id = store.add(1, "Scene 1", "key", 0, "note").unwrap();
        assert_eq!(store.open_count(), 1);
        assert!(store.resolve(id));
        assert_eq!(store.open_count(), 0);
    }
}
