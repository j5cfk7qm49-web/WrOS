//! Author-owned revision notes. Durable IDs and path locators are metadata,
//! never project-tree indices. Path relocation is explicit; no title/hash scan
//! guesses that two documents are the same. All writes use the existing Store.
use crate::{
    annotations::AnnotationStatus,
    statefile::{Store, Validate},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const PATH: &str = ".grimoire/writingdeck/review-notes.json";
pub const MAX_COMMENT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: u64,
    /// A locator, not the identity. Relative to the portable project root.
    pub path: String,
    pub previous_paths: Vec<String>,
    /// Written before any owned filesystem move. An interrupted or failed
    /// move cannot silently bind this ID to a different file at the old path.
    pub needs_reconnect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub offset: usize,
    pub source_sha256: String,
}
impl Anchor {
    pub fn capture(text: &str, start: usize, end: usize) -> Result<Self> {
        ensure!(
            text.len() <= crate::diagnostics::MAX_SOURCE_BYTES,
            "source exceeds annotation limit"
        );
        let quote = text
            .get(start..end)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("select a nonempty original passage first"))?;
        ensure!(quote.len() <= 16_384, "selected passage exceeds 16 KiB");
        let prefix_start = text[..start]
            .char_indices()
            .rev()
            .nth(31)
            .map_or(0, |(i, _)| i);
        let suffix_end = text[end..]
            .char_indices()
            .nth(32)
            .map_or(text.len(), |(i, _)| end + i);
        let value = Self {
            quote: quote.into(),
            prefix: text[prefix_start..start].into(),
            suffix: text[end..suffix_end].into(),
            offset: start,
            source_sha256: crate::diagnostics::hex_digest(text.as_bytes()),
        };
        ensure!(
            value.locate(text) == Some((start, end)),
            "selection is ambiguous even with context; select a longer passage"
        );
        Ok(value)
    }
    pub fn locate(&self, text: &str) -> Option<(usize, usize)> {
        crate::annotations::locate_quote(text, &self.quote, &self.prefix, &self.suffix)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.quote.trim().is_empty() && self.quote.len() <= 16_384,
            "invalid annotation quote"
        );
        ensure!(
            self.prefix.chars().count() <= 32 && self.suffix.chars().count() <= 32,
            "invalid annotation context"
        );
        ensure!(
            self.offset <= crate::diagnostics::MAX_SOURCE_BYTES,
            "invalid source offset"
        );
        digest(&self.source_sha256)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub document_id: u64,
    pub anchor: Anchor,
    pub comment: String,
    pub status: AnnotationStatus,
    pub reviewed_sha256: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: u64,
    pub current: Version,
    /// Prior comments/anchors/statuses are retained, never silently pruned.
    pub previous: Vec<Version>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewNotes {
    pub schema_version: u32,
    pub next_document: u64,
    pub next_note: u64,
    pub documents: Vec<Document>,
    pub notes: Vec<Note>,
}
impl Default for ReviewNotes {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_document: 0,
            next_note: 0,
            documents: vec![],
            notes: vec![],
        }
    }
}
impl ReviewNotes {
    pub fn load(root: &Path) -> Result<Store<Self>> {
        for legacy in [
            ".grimoire/annotations.json",
            ".grimoire/writingdeck/annotations.json",
        ] {
            match std::fs::symlink_metadata(root.join(legacy)) {
                Ok(_) => anyhow::bail!(
                    "prototype annotation data preserved; explicit identity migration required"
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Store::load(root, Path::new(PATH), Self::default())
    }
    pub fn document(&self, id: u64) -> Result<&Document> {
        self.documents
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("document identity unavailable"))
    }
    pub fn register(&mut self, path: &str) -> Result<u64> {
        locator(path)?;
        if let Some(doc) = self.documents.iter().find(|d| d.path == path) {
            ensure!(
                !doc.needs_reconnect,
                "document move needs explicit reconnection"
            );
            return Ok(doc.id);
        }
        ensure!(
            !self.documents.iter().any(|d| d.needs_reconnect),
            "resolve the pending document move/reconnection before assigning new document identities; originals preserved"
        );
        ensure!(
            self.documents.len() < 4096,
            "document identity limit reached"
        );
        self.next_document = self
            .next_document
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("document identity exhausted"))?;
        self.documents.push(Document {
            id: self.next_document,
            path: path.into(),
            previous_paths: vec![],
            needs_reconnect: false,
        });
        Ok(self.next_document)
    }
    /// A reviewed relocation preserves the ID and every note referencing it.
    /// The controller must verify that the old path is missing and review the
    /// new selection. Existing destinations are not merged.
    pub fn relocate(&mut self, id: u64, path: &str) -> Result<()> {
        locator(path)?;
        ensure!(
            !self.documents.iter().any(|d| d.id != id && d.path == path),
            "destination already has an identity; documents cannot be merged"
        );
        let doc = self
            .documents
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("document identity unavailable"))?;
        if doc.path != path {
            ensure!(
                doc.previous_paths.len() < 64,
                "document path history limit reached"
            );
            doc.previous_paths
                .push(std::mem::replace(&mut doc.path, path.into()));
        }
        doc.needs_reconnect = false;
        Ok(())
    }
    pub fn add(&mut self, path: &str, anchor: Anchor, comment: String) -> Result<u64> {
        let document_id = self.register(path)?;
        ensure!(self.notes.len() < 4096, "annotation limit reached");
        self.next_note = self
            .next_note
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("annotation identity exhausted"))?;
        self.notes.push(Note {
            id: self.next_note,
            current: Version {
                document_id,
                anchor,
                comment,
                status: AnnotationStatus::Open,
                reviewed_sha256: None,
            },
            previous: vec![],
        });
        Ok(self.next_note)
    }
    pub fn revise(&mut self, id: u64, edit: impl FnOnce(&mut Version)) -> Result<()> {
        let note = self
            .notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("annotation no longer exists"))?;
        let mut next = note.current.clone();
        edit(&mut next);
        if next != note.current {
            ensure!(
                note.previous.len() < 64,
                "annotation history limit reached; originals retained"
            );
            note.previous
                .push(std::mem::replace(&mut note.current, next));
        }
        Ok(())
    }
}
fn locator(path: &str) -> Result<()> {
    ensure!(
        path.len() <= 4096 && !path.contains('\\') && !path.chars().any(char::is_control),
        "invalid document locator"
    );
    crate::workspaces::relative(Path::new(path))?;
    ensure!(
        path.split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".."),
        "noncanonical document locator"
    );
    ensure!(
        path.ends_with(".md")
            && (!path.starts_with(".grimoire/") || path.starts_with(".grimoire/trash/")),
        "select a project Markdown document"
    );
    Ok(())
}
fn digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid source digest"
    );
    Ok(())
}
impl Validate for ReviewNotes {
    fn validate_context(&self, root: &Path) -> Result<()> {
        let known: BTreeSet<_> = self.documents.iter().map(|d| d.id).collect();
        let mut referenced = BTreeSet::new();
        // Read the same bounded, alias-refusing metadata paths as the stores.
        // Never infer an old ID's scene from today's tree order or body text.
        if let Some(value) = dependency::<crate::continuity::ContinuityStore>(
            root,
            ".grimoire/writingdeck/continuity.json",
        )? {
            referenced.extend(value.facts.iter().filter_map(|f| f.scene_id));
            referenced.extend(value.events.iter().filter_map(|e| e.scene_id));
        }
        if let Some(value) = dependency::<crate::milestones::MilestoneStore>(
            root,
            ".grimoire/writingdeck/milestones.json",
        )? {
            referenced.extend(
                value
                    .milestones
                    .iter()
                    .flat_map(|m| m.scene_snapshots.keys())
                    .copied(),
            );
        }
        if let Some(value) = dependency::<crate::milestones::VariantStore>(
            root,
            ".grimoire/writingdeck/variants.json",
        )? {
            referenced.extend(value.variants.iter().map(|v| v.scene_id));
        }
        for id in referenced {
            ensure!(
                known.contains(&id),
                "document identity registry is missing/incomplete: saved story data refers to document #{id}; restore the original registry before using saved story data; originals preserved"
            );
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        crate::story_state::identities(
            self.schema_version,
            self.next_document,
            self.documents.iter().map(|d| d.id),
        )?;
        crate::story_state::identities(
            self.schema_version,
            self.next_note,
            self.notes.iter().map(|n| n.id),
        )?;
        let mut paths = BTreeSet::new();
        for doc in &self.documents {
            locator(&doc.path)?;
            ensure!(
                doc.previous_paths.len() <= 64,
                "document path history limit exceeded"
            );
            for path in &doc.previous_paths {
                locator(path)?;
            }
            ensure!(paths.insert(&doc.path), "duplicate document locator");
        }
        for note in &self.notes {
            ensure!(
                note.previous.len() <= 64,
                "annotation history exceeds limit"
            );
            for v in std::iter::once(&note.current).chain(&note.previous) {
                self.document(v.document_id)?;
                v.anchor.validate()?;
                ensure!(
                    !v.comment.trim().is_empty() && v.comment.len() <= MAX_COMMENT,
                    "comment must contain 1–4096 bytes"
                );
                ensure!(
                    !v.comment
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t'),
                    "invalid comment control character"
                );
                if let Some(hash) = &v.reviewed_sha256 {
                    digest(hash)?;
                }
                ensure!(
                    (v.status == AnnotationStatus::Resolved) == v.reviewed_sha256.is_some(),
                    "resolved note requires a reviewed source revision"
                );
            }
        }
        Ok(())
    }
}

fn dependency<T: serde::de::DeserializeOwned + Validate>(
    root: &Path,
    relative: &str,
) -> Result<Option<T>> {
    let Some(bytes) = crate::statefile::read(root, Path::new(relative)).with_context(|| {
        format!("reading story identity references in {relative}; originals preserved")
    })?
    else {
        return Ok(None);
    };
    let value: T = serde_json::from_slice(&bytes).with_context(|| {
        format!("unsupported story identity metadata in {relative}; originals preserved")
    })?;
    value.validate().with_context(|| {
        format!("invalid story identity metadata in {relative}; originals preserved")
    })?;
    Ok(Some(value))
}

/// An owned tree move prepares durable invalidation BEFORE touching files.
/// Success publishes all path mappings at once, so swaps do not merge IDs.
/// A crash/failure leaves affected IDs unavailable until explicit reconnection.
/// This is a recoverable intent, not an atomic transaction across files.
pub(crate) struct MoveGuard {
    store: Store<ReviewNotes>,
    changes: Vec<(u64, String)>,
}
impl MoveGuard {
    pub(crate) fn prepare(
        renames: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> Result<Option<Self>> {
        let Some((first, _)) = renames.first() else {
            return Ok(None);
        };
        let Some(root) = first
            .ancestors()
            .skip(1)
            .find(|p| p.join("novel.toml").is_file())
        else {
            return Ok(None);
        };
        let mut store = ReviewNotes::load(root)?;
        if !store.exists() {
            return Ok(None);
        }
        let mut changes = vec![];
        for doc in &store.value.documents {
            let path = root.join(&doc.path);
            if let Some(new) = renames
                .iter()
                .find_map(|(from, to)| path.strip_prefix(from).ok().map(|rest| to.join(rest)))
            {
                ensure!(
                    !doc.needs_reconnect,
                    "document move is unresolved; reconnect its revision notes first"
                );
                ensure!(
                    doc.previous_paths.len() < 64,
                    "document path history limit reached"
                );
                let relative = new
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("non-UTF-8 document path"))?
                    .replace('\\', "/");
                locator(&relative)?;
                changes.push((doc.id, relative));
            }
        }
        if changes.is_empty() {
            return Ok(None);
        }
        // Validate the simultaneous destination map before preparing intent.
        let mut next = store.value.clone();
        for (id, path) in &changes {
            next.documents
                .iter_mut()
                .find(|d| d.id == *id)
                .unwrap()
                .path = path.clone();
        }
        next.validate()?;
        store.update(|s| {
            for (id, _) in &changes {
                s.documents
                    .iter_mut()
                    .find(|d| d.id == *id)
                    .unwrap()
                    .needs_reconnect = true;
            }
            Ok(())
        })?;
        Ok(Some(Self { store, changes }))
    }
    pub(crate) fn finish(guard: Option<Self>) -> Result<()> {
        if let Some(mut guard) = guard {
            guard
                .store
                .update(|s| {
                    for (id, path) in guard.changes {
                        let doc = s.documents.iter_mut().find(|d| d.id == id).unwrap();
                        doc.previous_paths
                            .push(std::mem::replace(&mut doc.path, path));
                        doc.needs_reconnect = false;
                    }
                    Ok(())
                })
                .map_err(|e| {
                    anyhow::anyhow!(
                        "source moved; note identity requires explicit reconnection: {e:#}"
                    )
                })?;
        }
        Ok(())
    }
}
pub(crate) fn tracked_rename(from: &Path, to: &Path) -> Result<()> {
    let guard = MoveGuard::prepare(&[(from.into(), to.into())])?;
    std::fs::rename(from, to)?;
    MoveGuard::finish(guard)
}

#[cfg(test)]
#[path = "review_notes_tests.rs"]
mod tests;
