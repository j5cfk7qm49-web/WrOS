//! Scene-linked scratch references and next-open intentions. Prose is always
//! ordinary original Markdown, never duplicated in this metadata store.
//! Promotion is an explicit independent scene snapshot, not a move/live alias.
use crate::{
    pins, project,
    sync::{self, Fingerprint, SaveOutcome},
    workspaces::Position,
};
use anyhow::{Context, Result, bail};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 1_048_576;
const MAX_SOURCE: usize = 16_777_216;
pub const HANDOFF: &str = "notes/handoff.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub len: u64,
    pub hash: u64,
}
impl From<Fingerprint> for Stamp {
    fn from(f: Fingerprint) -> Self {
        Self {
            len: f.len,
            hash: f.hash,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Prepared,
    Ready,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Promotion {
    pub target: PathBuf,
    pub source_stamp: Stamp,
    pub target_stamp: Stamp,
    pub phase: Phase,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scratch {
    pub id: u64,
    pub source: PathBuf,
    pub scene: Option<PathBuf>,
    pub life: pins::Life,
    pub promotion: Option<Promotion>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intention {
    pub left_at: String,
    pub context_id: String,
    pub source_stamp: Stamp,
    pub position: Option<Position>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub show_on_open: bool,
    pub intention: Option<Intention>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub next_id: u64,
    pub selected: Option<u64>,
    pub notes: Vec<Scratch>,
    pub handoff: Handoff,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            selected: None,
            notes: vec![],
            handoff: Handoff::default(),
        }
    }
}
pub fn scratch_path(id: u64) -> PathBuf {
    PathBuf::from(format!("notes/scratchpad/{id:06}.md"))
}
pub fn promotion_path(id: u64) -> PathBuf {
    PathBuf::from(format!("manuscript/90-Scratchpad-{id:06}.md"))
}
fn scene_path(p: &Path) -> Result<()> {
    pins::source_relative(p)?;
    if !p.starts_with("manuscript") {
        bail!("choose a manuscript scene");
    }
    Ok(())
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "unsupported workflow version {}; original preserved",
                self.version
            );
        }
        if self.notes.len() > 256 || self.next_id == 0 || self.next_id == u64::MAX {
            bail!("invalid scratchpad count/next ID");
        }
        let mut ids = std::collections::BTreeSet::new();
        for n in &self.notes {
            if n.id == 0
                || n.id >= self.next_id
                || !ids.insert(n.id)
                || n.source != scratch_path(n.id)
            {
                bail!("invalid scratchpad ID/path");
            }
            if let Some(s) = &n.scene {
                scene_path(s)?;
            }
            if let Some(p) = &n.promotion
                && (p.target != promotion_path(n.id)
                    || p.source_stamp.len > MAX_SOURCE as u64
                    || p.target_stamp.len > MAX_SOURCE as u64)
            {
                bail!("invalid promotion receipt");
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            bail!("invalid selected scratchpad");
        }
        if let Some(h) = &self.handoff.intention {
            if h.left_at.len() > 64
                || DateTime::parse_from_rfc3339(&h.left_at).is_err()
                || h.context_id.is_empty()
                || h.context_id.len() > 128
                || h.context_id.chars().any(char::is_control)
                || h.source_stamp.len > MAX_SOURCE as u64
            {
                bail!("invalid handoff intention");
            }
            if let Some(pos) = &h.position {
                scene_path(
                    pos.document
                        .as_deref()
                        .context("handoff position needs a scene")?,
                )?;
                if pos.line > 1_000_000
                    || pos.column > 1_000_000
                    || pos.scroll > 1_000_000
                    || pos
                        .anchor
                        .is_some_and(|(a, b)| a > 1_000_000 || b > 1_000_000)
                {
                    bail!("handoff position exceeds limits");
                }
            }
        }
        Ok(())
    }
    pub fn note(&self, id: u64) -> Result<&Scratch> {
        self.notes
            .iter()
            .find(|n| n.id == id)
            .context("scratchpad reference missing")
    }
    pub fn note_mut(&mut self, id: u64) -> Result<&mut Scratch> {
        self.notes
            .iter_mut()
            .find(|n| n.id == id)
            .context("scratchpad reference missing")
    }
}
/// Existing ancestors are checked before creating anything. Not a hostile
/// filesystem-race defense, file lock or multi-file atomic transaction.
fn checked(root: &Path, relative: &Path) -> Result<PathBuf> {
    crate::workspaces::relative(relative)?;
    let root = fs::canonicalize(root)?;
    let path = root.join(relative);
    for a in path.ancestors().take_while(|a| *a != root) {
        match fs::symlink_metadata(a) {
            Ok(_) => {
                if !fs::canonicalize(a)?.starts_with(&root) {
                    bail!("workflow path leaves project; preserved");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
/// Notes/research/manuscript roles cannot escape through within-project links.
/// Existing source identities are additionally checked by the editor registry.
pub fn source_path(root: &Path, relative: &Path, role: &str) -> Result<PathBuf> {
    pins::source_relative(relative)?;
    if !relative.starts_with(role) {
        bail!("source is outside expected {role} role");
    }
    let root = fs::canonicalize(root)?;
    let path = checked(&root, relative)?;
    let boundary = root.join(role);
    for a in path.ancestors().take_while(|a| *a != root) {
        match fs::canonicalize(a) {
            Ok(actual) if !actual.starts_with(&boundary) => {
                bail!("source changes {role} role through an alias; preserved")
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
fn read(path: &Path, limit: usize) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)?
        .take((limit + 1) as u64)
        .read_to_string(&mut text)?;
    if text.len() > limit {
        bail!("workflow file exceeds size limit; original preserved");
    }
    Ok(text)
}
fn create_new(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .context("source already exists or cannot be created; not overwritten")?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(())
}
pub fn ensure_handoff(root: &Path) -> Result<PathBuf> {
    let path = source_path(root, Path::new(HANDOFF), "notes")?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            pins::identify(root, Path::new(HANDOFF))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            create_new(&path, "---\ntitle: Next-session intention\n---\n\n")?
        }
        Err(e) => return Err(e.into()),
    }
    Ok(path)
}
/// Only an explicit starter-preset operation creates an empty original.
pub fn ensure_workspace_note(
    root: &Path,
    preset: crate::workspaces::Preset,
) -> Result<Option<PathBuf>> {
    let Some((relative, role)) = preset.source() else {
        return Ok(None);
    };
    let path = source_path(root, Path::new(relative), role)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            pins::identify(root, Path::new(relative))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_new(
            &path,
            &format!("---\ntitle: {} workspace notes\n---\n\n", preset.name()),
        )?,
        Err(e) => return Err(e.into()),
    }
    Ok(Some(path))
}
pub struct Store {
    pub state: State,
    root: PathBuf,
    seen: Option<Fingerprint>,
}
impl Store {
    pub fn path(root: &Path) -> PathBuf {
        root.join(".grimoire/writingdeck/workflow.json")
    }
    pub fn load(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root)?;
        let path = checked(&root, Path::new(".grimoire/writingdeck/workflow.json"))?;
        let (state, seen) = match read(&path, MAX_BYTES) {
            Ok(text) => {
                let s: State = serde_json::from_str(&text)
                    .context("invalid/unknown workflow JSON; preserved")?;
                s.validate()?;
                (s, Some(Fingerprint::of(&text)))
            }
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                (State::default(), None)
            }
            Err(e) => return Err(e),
        };
        Ok(Self { state, root, seen })
    }
    pub fn update<T>(&mut self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut next = self.state.clone();
        let value = f(&mut next)?;
        next.validate()?;
        let text = serde_json::to_string_pretty(&next)? + "\n";
        if text.len() > MAX_BYTES {
            bail!("workflow metadata exceeds 1 MiB");
        }
        let path = checked(&self.root, Path::new(".grimoire/writingdeck/workflow.json"))?;
        fs::create_dir_all(path.parent().unwrap())?;
        match sync::save_guarded(&path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                self.state = next;
                Ok(value)
            }
            SaveOutcome::Conflict { .. } => {
                bail!("workflow metadata changed externally; explicitly reload; original preserved")
            }
        }
    }
    pub fn create_note(&mut self) -> Result<u64> {
        if self.state.notes.len() >= 256 {
            bail!("scratchpad limit (256) reached");
        }
        let id = self.state.next_id;
        let relative = scratch_path(id);
        let path = source_path(&self.root, &relative, "notes")?;
        if fs::symlink_metadata(&path).is_ok() {
            bail!("scratchpad path already exists; not adopted or overwritten");
        }
        // Reserve before file creation: failure leaves an explicit unavailable ref,
        // never an untracked deletion/overwrite. Missing originals are not recreated.
        self.update(|s| {
            s.next_id = s
                .next_id
                .checked_add(1)
                .context("scratchpad IDs exhausted")?;
            s.notes.push(Scratch {
                id,
                source: relative,
                scene: None,
                life: pins::Life::Active,
                promotion: None,
            });
            s.selected = Some(id);
            Ok(())
        })?;
        create_new(&path,&format!("---\ntitle: Scratchpad {id}\n---\n\n")).context("reference reserved but source creation failed; restore original or create another note")?;
        Ok(id)
    }
    /// One intentional scene per note. The durable prepared receipt precedes
    /// creation. Retry recognizes its exact target; never truncates/re-copies it.
    pub fn promote(&mut self, id: u64, expected: Fingerprint) -> Result<PathBuf> {
        let note = self.state.note(id)?.clone();
        if let Some(p) = &note.promotion {
            let target = source_path(&self.root, &p.target, "manuscript")?;
            if p.phase == Phase::Ready {
                if !target.is_file() {
                    bail!("promoted scene moved/missing; not recreated");
                }
                return Ok(target);
            }
            // A previously created exact snapshot is recoverable even if the
            // author has since changed the separate original note.
            if let Ok(text) = read(&target, MAX_SOURCE) {
                if Stamp::from(Fingerprint::of(&text)) != p.target_stamp {
                    bail!("Prepared target changed; all versions preserved");
                }
                self.update(|s| {
                    s.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Ready;
                    Ok(())
                })?;
                return Ok(target);
            }
        }
        let source = source_path(&self.root, &note.source, "notes")?;
        let source_text = read(&source, MAX_SOURCE)?;
        if Fingerprint::of(&source_text) != expected {
            bail!("scratchpad changed externally; reload/settle original before promotion");
        }
        let linked = note
            .scene
            .as_ref()
            .context("link a manuscript scene before promotion")?;
        let linked_path = source_path(&self.root, linked, "manuscript")?;
        pins::identify(&self.root, linked)?;
        if !linked_path.is_file() {
            bail!("linked manuscript scene unavailable");
        }
        let (front, body) = project::split_frontmatter(&source_text);
        if body.trim().is_empty() {
            bail!("write/save original scratchpad prose before promotion");
        }
        let front = project::set_front(front.as_deref(), "compile", "true");
        let front = project::set_front(
            Some(&front),
            "scratchpad_source",
            &serde_json::to_string(&note.source.to_string_lossy())?,
        );
        let front = project::set_front(
            Some(&front),
            "scratchpad_scene",
            &serde_json::to_string(&linked.to_string_lossy())?,
        );
        let snapshot = format!("---\n{front}\n---\n{body}");
        if snapshot.len() > MAX_SOURCE {
            bail!("promoted snapshot exceeds size limit");
        }
        let relative = promotion_path(id);
        let target = source_path(&self.root, &relative, "manuscript")?;
        let source_stamp = Stamp::from(expected);
        let target_stamp = Stamp::from(Fingerprint::of(&snapshot));
        if let Some(p) = &note.promotion {
            if p.source_stamp != source_stamp || p.target_stamp != target_stamp {
                bail!(
                    "prepared snapshot changed; restore original saved version before retry; no competing target guessed"
                );
            }
        } else {
            if fs::symlink_metadata(&target).is_ok() {
                bail!("promotion destination exists; not overwritten or adopted");
            }
            self.update(|s| {
                s.note_mut(id)?.promotion = Some(Promotion {
                    target: relative.clone(),
                    source_stamp,
                    target_stamp,
                    phase: Phase::Prepared,
                });
                Ok(())
            })?;
        }
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                let existing = read(&target, MAX_SOURCE)?;
                if Stamp::from(Fingerprint::of(&existing)) != target_stamp {
                    bail!("prepared destination changed; all versions preserved; not overwritten");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_new(&target, &snapshot)?,
            Err(e) => return Err(e.into()),
        }
        self.update(|s|{s.note_mut(id)?.promotion.as_mut().unwrap().phase=Phase::Ready;Ok(())}).context("scene snapshot exists; receipt still prepared; explicitly reload/retry; no duplicate created")?;
        Ok(target)
    }
    /// Only an unwritten preparation can be cancelled. Existing targets never
    /// lose their receipt or get deleted to make a retry seem successful.
    pub fn cancel_prepared(&mut self, id: u64) -> Result<()> {
        let p = self
            .state
            .note(id)?
            .promotion
            .as_ref()
            .context("No prepared promotion")?;
        if p.phase != Phase::Prepared {
            bail!("Ready promotions remain independent scenes");
        }
        let path = source_path(&self.root, &p.target, "manuscript")?;
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => bail!("Target exists; receipt retained; no file deleted"),
            Err(e) => return Err(e.into()),
        }
        self.update(|s| {
            s.note_mut(id)?.promotion = None;
            Ok(())
        })
    }
    pub fn leave_handoff(
        &mut self,
        expected: Fingerprint,
        at: String,
        context_id: String,
        position: Option<Position>,
    ) -> Result<()> {
        let path = source_path(&self.root, Path::new(HANDOFF), "notes")?;
        let text = read(&path, MAX_SOURCE)?;
        if Fingerprint::of(&text) != expected {
            bail!("handoff changed externally; reload/settle original first");
        }
        if project::split_frontmatter(&text).1.trim().is_empty() {
            bail!("write/save an original next-session intention first");
        }
        if let Some(p) = &position {
            let relative = p.document.as_deref().context("no context document")?;
            pins::identify(&self.root, relative)?;
            source_path(&self.root, relative, "manuscript")?;
        }
        self.update(|s| {
            s.handoff.show_on_open = true;
            s.handoff.intention = Some(Intention {
                left_at: at,
                context_id,
                source_stamp: expected.into(),
                position,
            });
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wd-workflow-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("manuscript")).unwrap();
        fs::write(p.join("manuscript/scene.md"), "Original scene.\n").unwrap();
        p
    }
    fn written(s: &mut Store) -> (u64, PathBuf, Fingerprint) {
        let id = s.create_note().unwrap();
        let path = s.root.join(scratch_path(id));
        fs::write(&path,"---\ntitle: Original\ncustom: [keep, me]\ncompile: false\n---\n\n# Idea\nAn original idea.\n").unwrap();
        s.update(|v| {
            v.note_mut(id)?.scene = Some("manuscript/scene.md".into());
            Ok(())
        })
        .unwrap();
        (id, path.clone(), Fingerprint::read(&path).unwrap())
    }
    #[test]
    fn loading_and_source_paths_are_side_effect_free() {
        let r = root("read");
        assert_eq!(Store::load(&r).unwrap().state, State::default());
        assert!(!r.join(".grimoire").exists());
        assert!(!r.join("notes").exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn notes_are_independent_plaintext_and_roundtrip_refs() {
        let r = root("notes");
        let mut s = Store::load(&r).unwrap();
        let a = s.create_note().unwrap();
        let b = s.create_note().unwrap();
        assert_ne!(a, b);
        fs::write(r.join(scratch_path(a)), "Manual first idea").unwrap();
        assert!(
            !fs::read_to_string(Store::path(&r))
                .unwrap()
                .contains("Manual first")
        );
        assert_eq!(Store::load(&r).unwrap().state, s.state);
        assert_ne!(
            fs::read(r.join(scratch_path(a))).unwrap(),
            fs::read(r.join(scratch_path(b))).unwrap()
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn promotion_preserves_original_and_is_repeatable_without_duplicate() {
        let r = root("promote");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        let original = fs::read(&p).unwrap();
        let target = s.promote(id, f).unwrap();
        let snapshot = fs::read_to_string(&target).unwrap();
        assert_eq!(fs::read(p).unwrap(), original);
        assert!(snapshot.contains("custom: [keep, me]"));
        assert!(snapshot.contains("compile: true"));
        assert!(snapshot.ends_with("An original idea.\n"));
        assert_eq!(s.promote(id, f).unwrap(), target);
        fs::write(&target, "Revised scene, independent.\n").unwrap();
        assert_eq!(s.promote(id, f).unwrap(), target);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "Revised scene, independent.\n"
        );
        fs::remove_file(&target).unwrap();
        assert!(s.promote(id, f).is_err());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn external_source_or_destination_is_never_overwritten() {
        let r = root("clash");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        fs::write(&p, "External new idea").unwrap();
        assert!(s.promote(id, f).is_err());
        assert!(!r.join(promotion_path(id)).exists());
        fs::write(&p, "Saved changed idea").unwrap();
        let f = Fingerprint::read(&p).unwrap();
        fs::write(r.join(promotion_path(id)), "Preexisting scene").unwrap();
        assert!(s.promote(id, f).is_err());
        assert_eq!(
            fs::read_to_string(r.join(promotion_path(id))).unwrap(),
            "Preexisting scene"
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn prepared_retry_recognizes_exact_target_and_rejects_changed_snapshot() {
        let r = root("retry");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        let target = s.promote(id, f).unwrap();
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        let before = fs::read(&target).unwrap();
        assert_eq!(s.promote(id, f).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), before);
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        fs::remove_file(&target).unwrap();
        fs::write(&p, "Changed source after prepare").unwrap();
        assert!(s.promote(id, Fingerprint::read(&p).unwrap()).is_err());
        assert!(!target.exists());
        s.cancel_prepared(id).unwrap();
        assert!(s.state.note(id).unwrap().promotion.is_none());
        s.promote(id, Fingerprint::read(&p).unwrap()).unwrap();
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn existing_destination_directory_preserves_original_without_preparing() {
        let r = root("failure");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        let original = fs::read(&p).unwrap();
        let target = r.join(promotion_path(id));
        fs::create_dir(&target).unwrap();
        assert!(s.promote(id, f).is_err());
        assert_eq!(fs::read(p).unwrap(), original);
        assert!(s.state.note(id).unwrap().promotion.is_none());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn invalid_future_nested_metadata_and_io_are_preserved() {
        let r = root("invalid");
        let mut s = Store::load(&r).unwrap();
        s.create_note().unwrap();
        let path = Store::path(&r);
        let valid = fs::read_to_string(&path).unwrap();
        for raw in [
            "{bad".into(),
            valid.replace("\"version\": 1", "\"version\": 999"),
            valid.replace("\"handoff\": {", "\"handoff\": {\"future\":true,"),
        ] {
            fs::write(&path, &raw).unwrap();
            assert!(Store::load(&r).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        }
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn concurrent_index_conflict_rolls_back_before_creating_note() {
        let r = root("concurrent");
        let mut a = Store::load(&r).unwrap();
        let mut b = Store::load(&r).unwrap();
        a.create_note().unwrap();
        let before = b.state.clone();
        assert!(b.create_note().is_err());
        assert_eq!(b.state, before);
        assert_eq!(fs::read_dir(r.join("notes/scratchpad")).unwrap().count(), 1);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn lifecycle_is_metadata_only_and_id_selection_paths_are_bounded() {
        let r = root("life");
        let mut s = Store::load(&r).unwrap();
        let (id, p, _) = written(&mut s);
        let original = fs::read(&p).unwrap();
        for life in [
            pins::Life::Archived,
            pins::Life::Trashed,
            pins::Life::Active,
        ] {
            s.update(|v| {
                v.note_mut(id)?.life = life;
                Ok(())
            })
            .unwrap();
            assert_eq!(fs::read(&p).unwrap(), original);
        }
        let mut bad = s.state.clone();
        bad.notes[0].source = "../lost.md".into();
        assert!(bad.validate().is_err());
        bad = s.state.clone();
        bad.next_id = id;
        assert!(bad.validate().is_err());
        bad = s.state.clone();
        bad.notes[0].scene = Some("notes/idea.md".into());
        assert!(bad.validate().is_err());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn handoff_prose_is_original_only_and_ready_requires_saved_nonempty() {
        let r = root("handoff");
        let mut s = Store::load(&r).unwrap();
        let p = ensure_handoff(&r).unwrap();
        let at = "2026-10-25T01:00:00-07:00".to_string();
        assert!(
            s.leave_handoff(
                Fingerprint::read(&p).unwrap(),
                at.clone(),
                "first".into(),
                None
            )
            .is_err()
        );
        fs::write(
            &p,
            "---\ntitle: Tomorrow\ncustom: original\n---\n\nFollow the river.\n",
        )
        .unwrap();
        let original = fs::read(&p).unwrap();
        let pos = Position {
            document: Some("manuscript/scene.md".into()),
            line: 8,
            column: 4,
            scroll: 6,
            anchor: Some((7, 1)),
        };
        s.leave_handoff(
            Fingerprint::read(&p).unwrap(),
            at,
            "first".into(),
            Some(pos),
        )
        .unwrap();
        assert_eq!(fs::read(p).unwrap(), original);
        assert!(
            !fs::read_to_string(Store::path(&r))
                .unwrap()
                .contains("Follow the river")
        );
        assert_eq!(Store::load(&r).unwrap().state, s.state);
        assert!(s.state.handoff.show_on_open);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn handoff_stale_bytes_and_bad_positions_do_not_commit() {
        let r = root("stale");
        let mut s = Store::load(&r).unwrap();
        let p = ensure_handoff(&r).unwrap();
        let seen = Fingerprint::read(&p).unwrap();
        fs::write(&p, "Later intention").unwrap();
        assert!(
            s.leave_handoff(seen, "2026-10-25T00:00:00Z".into(), "first".into(), None)
                .is_err()
        );
        assert!(s.state.handoff.intention.is_none());
        assert_eq!(fs::read_to_string(p).unwrap(), "Later intention");
        fs::remove_dir_all(r).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn early_late_metadata_and_source_role_escapes_create_nothing() {
        use std::os::unix::fs::symlink;
        let r = root("escape");
        let outside = root("outside");
        symlink(&outside, r.join("notes")).unwrap();
        assert!(Store::load(&r).unwrap().create_note().is_err());
        assert!(ensure_handoff(&r).is_err());
        assert!(!outside.join("scratchpad").exists());
        fs::remove_file(r.join("notes")).unwrap();
        symlink(r.join("manuscript"), r.join("notes")).unwrap();
        assert!(ensure_handoff(&r).is_err());
        assert!(!r.join("manuscript/handoff.md").exists());
        fs::remove_file(r.join("notes")).unwrap();
        let mut s = Store::load(&r).unwrap();
        s.create_note().unwrap();
        fs::remove_dir_all(r.join(".grimoire/writingdeck")).unwrap();
        symlink(&outside, r.join(".grimoire/writingdeck")).unwrap();
        assert!(
            s.update(|v| {
                v.selected = None;
                Ok(())
            })
            .is_err()
        );
        assert!(!outside.join("workflow.json").exists());
        fs::remove_dir_all(r).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn exact_prepared_scene_can_finalize_after_original_changes_but_changed_target_is_kept() {
        let r = root("prepared-independent");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        let target = s.promote(id, f).unwrap();
        let copied = fs::read(&target).unwrap();
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        fs::write(&p, "Author continued changing the separate original.").unwrap();
        s.promote(id, Fingerprint::read(&p).unwrap()).unwrap();
        assert_eq!(fs::read(&target).unwrap(), copied);
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        fs::write(&target, "Other edited target.").unwrap();
        assert!(s.promote(id, Fingerprint::read(&p).unwrap()).is_err());
        assert!(s.cancel_prepared(id).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "Other edited target.");
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn prepared_io_failure_retains_receipt_original_and_recovers_same_snapshot_after_reopen() {
        let r = root("prepared-io");
        let mut s = Store::load(&r).unwrap();
        let (id, p, f) = written(&mut s);
        let original = fs::read(&p).unwrap();
        let target = s.promote(id, f).unwrap();
        let copied = fs::read(&target).unwrap();
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(s.promote(id, f).is_err());
        assert_eq!(
            s.state.note(id).unwrap().promotion.as_ref().unwrap().phase,
            Phase::Prepared
        );
        assert_eq!(fs::read(&p).unwrap(), original);
        fs::remove_dir(&target).unwrap();
        let mut reopened = Store::load(&r).unwrap();
        assert_eq!(reopened.promote(id, f).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), copied);
        assert_eq!(fs::read(&p).unwrap(), original);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn count_size_source_limits_and_explicit_starter_existing_bytes_are_preserved() {
        let r = root("limits-starters");
        let mut s = Store::load(&r).unwrap();
        s.state.notes = (1..=256)
            .map(|id| Scratch {
                id,
                source: scratch_path(id),
                scene: None,
                life: pins::Life::Active,
                promotion: None,
            })
            .collect();
        s.state.next_id = 257;
        assert!(s.state.validate().is_ok());
        assert!(s.create_note().is_err());
        assert!(!r.join("notes/scratchpad").exists());
        let p = ensure_workspace_note(&r, crate::workspaces::Preset::Planning)
            .unwrap()
            .unwrap();
        fs::write(
            &p,
            "---\ntitle: Private original\ncustom: untouched\n---\n\nDo not template me.\n",
        )
        .unwrap();
        let original = fs::read(&p).unwrap();
        ensure_workspace_note(&r, crate::workspaces::Preset::Planning).unwrap();
        assert_eq!(fs::read(&p).unwrap(), original);
        assert!(
            ensure_workspace_note(&r, crate::workspaces::Preset::Focus)
                .unwrap()
                .is_none()
        );
        let index = Store::path(&r);
        fs::create_dir_all(index.parent().unwrap()).unwrap();
        fs::write(&index, " ".repeat(MAX_BYTES + 1)).unwrap();
        assert!(Store::load(&r).is_err());
        assert_eq!(fs::metadata(index).unwrap().len(), (MAX_BYTES + 1) as u64);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn prepared_exact_target_completion_write_failure_recovers_without_recopying() {
        let r = root("completion-io");
        let mut s = Store::load(&r).unwrap();
        let (id, source, f) = written(&mut s);
        let target = s.promote(id, f).unwrap();
        let meta = Store::path(&r);
        // Model the durable state after target creation, before completion.
        // This is controlled I/O fault simulation, not a real power-cut test.
        s.update(|v| {
            v.note_mut(id)?.promotion.as_mut().unwrap().phase = Phase::Prepared;
            Ok(())
        })
        .unwrap();
        let prepared = fs::read(&meta).unwrap();
        let backup = meta.with_extension("retained");
        let original = fs::read(&source).unwrap();
        let scene = fs::read(&target).unwrap();
        fs::rename(&meta, &backup).unwrap();
        fs::create_dir(&meta).unwrap();
        assert!(s.promote(id, f).is_err());
        assert_eq!(
            s.state.note(id).unwrap().promotion.as_ref().unwrap().phase,
            Phase::Prepared
        );
        assert_eq!(fs::read(&backup).unwrap(), prepared);
        assert!(meta.is_dir());
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(&target).unwrap(), scene);
        fs::remove_dir(&meta).unwrap();
        fs::rename(&backup, &meta).unwrap();
        drop(s);
        let mut s = Store::load(&r).unwrap();
        assert_eq!(s.promote(id, f).unwrap(), target);
        assert_eq!(
            s.state.note(id).unwrap().promotion.as_ref().unwrap().phase,
            Phase::Ready
        );
        assert_eq!(fs::read(source).unwrap(), original);
        assert_eq!(fs::read(target).unwrap(), scene);
        assert_eq!(fs::read_dir(r.join("manuscript")).unwrap().count(), 2);
        fs::remove_dir_all(r).unwrap();
    }
}
