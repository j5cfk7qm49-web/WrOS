//! Explicit character knowledge at an author-selected point in a scene.
//! Uses shared durable document IDs and the existing guarded statefile store.
use crate::{
    continuity::{ContinuityStore, Fact},
    statefile::{Store, Validate},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub const PATH: &str = ".grimoire/writingdeck/knowledge.json";

pub fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Knows,
    Suspects,
    Unaware,
    BelievesOtherwise,
}
impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::Knows => "Knows the stated fact",
            Self::Suspects => "Suspects it",
            Self::Unaware => "Does not know it",
            Self::BelievesOtherwise => "Believes otherwise",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Knows => Self::Suspects,
            Self::Suspects => Self::Unaware,
            Self::Unaware => Self::BelievesOtherwise,
            Self::BelievesOtherwise => Self::Knows,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Moment {
    Before,
    After,
}
impl Moment {
    pub fn label(self) -> &'static str {
        match self {
            Self::Before => "Before this scene",
            Self::After => "After this scene",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Before => Self::After,
            Self::After => Self::Before,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub character: String,
    pub document_id: u64,
    pub scene_name: String,
    pub source_sha256: String,
    /// Reviewed snapshot of the explicit fact; never a second mutable canon.
    pub fact: Fact,
    pub state: State,
    pub moment: Moment,
    pub notes: String,
    pub archived: bool,
    pub recorded_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: u64,
    pub current: Version,
    pub previous: Vec<Version>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub schema_version: u32,
    pub next_id: u64,
    pub entries: Vec<Entry>,
}
impl Default for Knowledge {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_id: 0,
            entries: vec![],
        }
    }
}
impl Knowledge {
    pub fn load(root: &Path) -> Result<Store<Self>> {
        Store::load(root, Path::new(PATH), Self::default())
    }
    pub fn add(&mut self, current: Version) -> Result<u64> {
        validate_version(&current)?;
        ensure!(
            self.entries.len() < 4096,
            "character knowledge limit reached; original entries retained"
        );
        let id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("knowledge identity exhausted"))?;
        self.entries.push(Entry {
            id,
            current,
            previous: vec![],
        });
        self.next_id = id;
        Ok(id)
    }
    pub fn revise(&mut self, id: u64, next: Version) -> Result<()> {
        validate_version(&next)?;
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("knowledge record unavailable"))?;
        ensure!(
            entry.current.document_id == next.document_id && entry.current.fact.id == next.fact.id,
            "a knowledge revision cannot change its document or fact identity; create a new record"
        );
        let mut same = next.clone();
        same.recorded_at.clone_from(&entry.current.recorded_at);
        if same != entry.current {
            ensure!(
                entry.previous.len() < 64,
                "knowledge history limit reached; originals retained"
            );
            entry
                .previous
                .push(std::mem::replace(&mut entry.current, next));
        }
        Ok(())
    }
    /// Only explicit Knows/Unaware assertions about the same exact fact at the
    /// same scene point/revision conflict. No chronology or knowledge is guessed.
    pub fn conflicts(&self) -> Result<Vec<(u64, u64)>> {
        let mut groups = BTreeMap::<_, (Vec<u64>, Vec<u64>)>::new();
        for e in self.entries.iter().filter(|e| !e.current.archived) {
            let v = &e.current;
            if !matches!(v.state, State::Knows | State::Unaware) {
                continue;
            }
            let fact_hash = source_hash(&serde_json::to_string(&v.fact)?);
            let pair = groups
                .entry((
                    v.document_id,
                    v.moment,
                    v.character.as_str(),
                    v.source_sha256.as_str(),
                    fact_hash,
                ))
                .or_default();
            if v.state == State::Knows {
                pair.0.push(e.id);
            } else {
                pair.1.push(e.id);
            }
        }
        let mut result = Vec::new();
        for (knows, unaware) in groups.values() {
            for a in knows {
                for b in unaware {
                    ensure!(
                        result.len() < 512,
                        "knowledge conflicts exceed 512; no partial review"
                    );
                    result.push((*a, *b));
                }
            }
        }
        result.sort_unstable();
        Ok(result)
    }
}
fn validate_version(v: &Version) -> Result<()> {
    ensure!(
        v.document_id > 0 && v.fact.id > 0,
        "knowledge needs shared document and fact identities"
    );
    ensure!(
        !v.character.trim().is_empty() && v.character == v.character.trim(),
        "name the character without surrounding spaces"
    );
    crate::story_state::text(&v.character, 256)?;
    crate::story_state::text(&v.scene_name, 4096)?;
    crate::story_state::text(&v.notes, 4096)?;
    ensure!(
        v.source_sha256.len() == 64 && v.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid knowledge source revision"
    );
    ensure!(
        v.state != State::BelievesOtherwise || !v.notes.trim().is_empty(),
        "describe the character's different belief in Notes"
    );
    chrono::DateTime::parse_from_rfc3339(&v.recorded_at)?;
    let facts = ContinuityStore {
        next_id: v.fact.id,
        facts: vec![v.fact.clone()],
        ..Default::default()
    };
    facts.validate()?;
    Ok(())
}
impl Validate for Knowledge {
    fn validate(&self) -> Result<()> {
        crate::story_state::identities(
            self.schema_version,
            self.next_id,
            self.entries.iter().map(|e| e.id),
        )?;
        ensure!(
            self.entries.len() <= 4096,
            "character knowledge limit exceeded"
        );
        for e in &self.entries {
            validate_version(&e.current)?;
            ensure!(e.previous.len() <= 64, "knowledge history limit exceeded");
            for previous in &e.previous {
                validate_version(previous)?;
                ensure!(
                    previous.document_id == e.current.document_id
                        && previous.fact.id == e.current.fact.id,
                    "knowledge history changed source identities"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::{Certainty, FactCategory};
    fn version(state: State, moment: Moment) -> Version {
        let mut fact = Fact::new(
            FactCategory::Object,
            "Key",
            "location",
            "Archive",
            Certainty::Definite,
        );
        fact.id = 7;
        Version {
            character: "Wren λ".into(),
            document_id: 4,
            scene_name: "The Brass Key".into(),
            source_sha256: source_hash("Original scene."),
            fact,
            state,
            moment,
            notes: "Author's original note.".into(),
            archived: false,
            recorded_at: "2026-10-30T23:00:00Z".into(),
        }
    }
    #[test]
    fn explicit_knowledge_conflicts_require_same_character_fact_scene_point_and_revision() {
        let mut s = Knowledge::default();
        s.add(version(State::Knows, Moment::Before)).unwrap();
        s.add(version(State::Unaware, Moment::After)).unwrap();
        let mut other = version(State::Unaware, Moment::Before);
        other.character = "Other character".into();
        s.add(other).unwrap();
        let mut revised = version(State::Unaware, Moment::Before);
        revised.source_sha256 = source_hash("Revised scene.");
        s.add(revised).unwrap();
        let mut changed_fact = version(State::Unaware, Moment::Before);
        changed_fact.fact.value = "Tower".into();
        s.add(changed_fact).unwrap();
        assert!(s.conflicts().unwrap().is_empty());
        let b = s.add(version(State::Unaware, Moment::Before)).unwrap();
        assert_eq!(s.conflicts().unwrap(), vec![(1, b)]);
        s.add(version(State::Suspects, Moment::Before)).unwrap();
        assert_eq!(s.conflicts().unwrap(), vec![(1, b)]);
    }
    #[test]
    fn knowledge_revision_archive_and_identity_refusal_keep_every_original() {
        let mut s = Knowledge::default();
        let v = version(State::Knows, Moment::Before);
        let id = s.add(v.clone()).unwrap();
        let mut next = v.clone();
        next.state = State::Unaware;
        s.revise(id, next.clone()).unwrap();
        assert_eq!(s.entries[0].previous, vec![v]);
        let mut bad = next.clone();
        bad.document_id = 99;
        let before = s.clone();
        assert!(s.revise(id, bad).is_err());
        assert_eq!(s, before);
        next.archived = true;
        s.revise(id, next).unwrap();
        assert_eq!(s.entries[0].previous.len(), 2);
        assert!(s.conflicts().unwrap().is_empty());
        s.validate().unwrap();
    }
    #[test]
    fn knowledge_conflict_bound_refuses_a_partial_result() {
        let mut s = Knowledge::default();
        for _ in 0..23 {
            s.add(version(State::Knows, Moment::Before)).unwrap();
            s.add(version(State::Unaware, Moment::Before)).unwrap();
        }
        assert!(
            s.conflicts()
                .unwrap_err()
                .to_string()
                .contains("no partial")
        );
    }

    #[test]
    fn knowledge_current_and_previous_document_and_fact_links_prevent_missing_identity_reuse() {
        use crate::review_notes::{PATH as REGISTRY, ReviewNotes};
        use std::fs;
        for reference in ["current-document", "current-fact", "previous-fact"] {
            let root = std::env::temp_dir().join(format!(
                "wros-knowledge-identity-{reference}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(root.join("manuscript/01-Chapter")).unwrap();
            fs::write(
                root.join("novel.toml"),
                "title = \"Knowledge identity fixture\"\n",
            )
            .unwrap();
            fs::write(root.join("manuscript/01-Chapter/01-A.md"), "Original A.\n").unwrap();
            fs::write(root.join("manuscript/01-Chapter/02-B.md"), "Original B.\n").unwrap();
            let mut registry = ReviewNotes::load(&root).unwrap();
            let a = registry
                .update(|s| s.register("manuscript/01-Chapter/01-A.md"))
                .unwrap();
            let b = registry
                .update(|s| s.register("manuscript/01-Chapter/02-B.md"))
                .unwrap();
            let original = fs::read(root.join(REGISTRY)).unwrap();
            let mut v = version(State::Knows, Moment::After);
            v.document_id = a;
            if reference != "current-document" {
                v.fact.scene_id = Some(b);
            }
            let mut knowledge = Knowledge::load(&root).unwrap();
            let id = knowledge.update(|s| s.add(v.clone())).unwrap();
            if reference == "previous-fact" {
                v.fact.scene_id = None;
                v.notes = "Revised, previous fact link retained".into();
                knowledge.update(|s| s.revise(id, v)).unwrap();
            }
            let bytes = fs::read(root.join(PATH)).unwrap();
            fs::remove_file(root.join(REGISTRY)).unwrap();
            assert!(ReviewNotes::load(&root).is_err(), "{reference}");
            assert!(!root.join(REGISTRY).exists());
            let mut incomplete: ReviewNotes = serde_json::from_slice(&original).unwrap();
            incomplete.documents.retain(|d| {
                d.id != if reference == "current-document" {
                    a
                } else {
                    b
                }
            });
            fs::write(
                root.join(REGISTRY),
                serde_json::to_vec(&incomplete).unwrap(),
            )
            .unwrap();
            assert!(ReviewNotes::load(&root).is_err(), "{reference}");
            assert!(
                crate::project::rename(&root.join("manuscript/01-Chapter/02-B.md"), "Refused")
                    .is_err()
            );
            assert!(root.join("manuscript/01-Chapter/02-B.md").is_file());
            assert_eq!(fs::read(root.join(PATH)).unwrap(), bytes);
            fs::write(root.join(REGISTRY), original).unwrap();
            assert!(ReviewNotes::load(&root).is_ok());
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn unknown_knowledge_prevents_new_registry_allocation_and_owned_source_moves() {
        use crate::review_notes::{PATH as REGISTRY, ReviewNotes};
        use std::fs;
        let root = std::env::temp_dir().join(format!(
            "wros-knowledge-future-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("manuscript/01-Chapter")).unwrap();
        fs::write(
            root.join("novel.toml"),
            "title = \"Knowledge future fixture\"\n",
        )
        .unwrap();
        let scene = root.join("manuscript/01-Chapter/01-A.md");
        fs::write(&scene, "Original.\n").unwrap();
        let mut cached = ReviewNotes::load(&root).unwrap();
        let metadata = root.join(PATH);
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        let bytes = br#"{"schema_version":99,"next_id":0,"entries":[]}"#;
        fs::write(&metadata, bytes).unwrap();
        assert!(
            cached
                .update(|s| s.register("manuscript/01-Chapter/01-A.md"))
                .is_err()
        );
        assert!(crate::project::rename(&scene, "Refused").is_err());
        assert!(scene.is_file());
        assert!(!root.join(REGISTRY).exists());
        assert_eq!(fs::read(metadata).unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
}
