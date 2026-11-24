//! The selected Loro journal and its disposable SQLite read model.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
#[cfg(feature = "qualification")]
use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::{Connection, TransactionBehavior};

use crate::loro_store::PagedLoroStore;
use crate::model::{Change, Edit, FieldId, NodeId, OpId, Place, Snapshot};
use crate::projection::{
    Result, TextTarget, apply_paged_edit, create_view, edit_target, flush_wal, has_stamp,
    open_database, read_children, read_frontier, read_note, replace_view, stamp_view,
};
use crate::readable::{self, ReadableWorker};
use crate::schema::{FieldSpec, SavedQuery};
use crate::search;
use crate::storage::{Journal, sync_directory};

pub use crate::projection::OutlineRow;
pub use crate::search::NoteDetails;

#[cfg(feature = "qualification")]
pub use crate::sqlite_ledger::SqliteLedger;

const LIVE_DB: &str = "live.sqlite";
const DEVICE_FILE: &str = "device-id";

pub struct PagedBrain {
    journal: Journal<PagedLoroStore>,
    view: Connection,
    device: u64,
    readable: Option<ReadableWorker>,
    readable_started: bool,
    #[cfg(feature = "qualification")]
    view_file: PathBuf,
}

impl PagedBrain {
    pub fn open(root: &Path, device: u64) -> Result<Self> {
        Self::open_at(root, &root.join(LIVE_DB), device)
    }

    /// The operation journal lives under the library, and the rebuildable
    /// projection lives in the machine's cache. Backups take the store and
    /// media, so the device identity is chosen afresh on an isolated restore.
    pub fn open_library(library: &Path, cache: &Path) -> Result<Self> {
        fs::create_dir_all(library)?;
        fs::create_dir_all(cache)?;
        let device = library_device(library)?;
        let db = cache.join(LIVE_DB);
        let mut brain = Self::open_at(&library.join("store"), &db, device)?;
        brain.readable = Some(ReadableWorker::start(db, library.join("readable"))?);
        Ok(brain)
    }

    fn open_at(root: &Path, view_file: &Path, device: u64) -> Result<Self> {
        let journal = Journal::open(root, device)?;
        let view = open_database(view_file)?;
        // The checked Loro journal already makes the edit durable. A power
        // loss can lose recent view transactions, which the version check
        // detects and rebuilds from the journal.
        view.pragma_update(None, "synchronous", "NORMAL")?;
        create_view(&view)?;
        Ok(Self {
            journal,
            view,
            device,
            readable: None,
            readable_started: false,
            #[cfg(feature = "qualification")]
            view_file: view_file.to_owned(),
        })
    }

    pub fn project(&self) -> Result<Snapshot> {
        Ok(self.journal.project()?)
    }

    pub(crate) fn prime_view(&mut self, snapshot: &Snapshot) -> Result<()> {
        let stamp = self.projection_stamp();
        replace_view(&mut self.view, &stamp, snapshot)?;
        if self.readable.is_some() {
            self.wake_readable();
            Ok(())
        } else {
            flush_wal(&self.view)
        }
    }

    pub fn note(&mut self, node: NodeId) -> Result<Option<String>> {
        self.refresh_view()?;
        read_note(&self.view, node)
    }

    pub fn children(
        &mut self,
        parent: Option<NodeId>,
        offset: u32,
        count: u16,
    ) -> Result<Vec<OutlineRow>> {
        self.refresh_view()?;
        read_children(&self.view, parent, offset, count, false)
    }

    pub fn named_pages(&mut self, offset: u32, count: u16) -> Result<Vec<OutlineRow>> {
        self.refresh_view()?;
        read_children(&self.view, None, offset, count, true)
    }

    pub fn search(
        &mut self,
        query: &SavedQuery,
        offset: u32,
        count: u16,
    ) -> Result<Vec<OutlineRow>> {
        self.refresh_view()?;
        search::search(&self.view, query, offset, count)
    }

    pub fn details(&mut self, node: NodeId) -> Result<NoteDetails> {
        self.refresh_view()?;
        self.require_visible(node)?;
        search::details(&self.view, node)
    }

    pub fn fields_for_tag(
        &mut self,
        tag: NodeId,
    ) -> Result<std::collections::BTreeMap<FieldId, FieldSpec>> {
        self.refresh_view()?;
        search::fields_for_tag(&self.view, tag)
    }

    pub fn define_field(
        &mut self,
        tag: NodeId,
        field: Option<FieldId>,
        spec: FieldSpec,
    ) -> Result<FieldId> {
        if spec.name.trim().is_empty() || spec.name.len() > 240 {
            return Err("Enter a field name between 1 and 240 bytes".into());
        }
        let (id, dependencies) = self.next_local_id()?;
        if !matches!(tag, NodeId::Builtin(_)) {
            self.require_visible(tag)?;
        }
        if let Some(field) = field
            && !search::fields_for_tag(&self.view, tag)?.contains_key(&field)
        {
            return Err("This field does not belong to the tag".into());
        }
        let field = field.unwrap_or(FieldId::Custom(id));
        self.append(Edit {
            id,
            dependencies,
            change: Change::DefineField { tag, field, spec },
        })?;
        Ok(field)
    }

    pub fn set_tag(&mut self, node: NodeId, tag: NodeId, present: bool) -> Result<()> {
        self.refresh_view()?;
        if present && !matches!(tag, NodeId::Builtin(_)) {
            self.require_visible(tag)?;
        }
        self.update_note(node, Change::SetTag { node, tag, present })
    }

    pub fn set_link(&mut self, node: NodeId, target: NodeId, present: bool) -> Result<()> {
        self.refresh_view()?;
        if present {
            self.require_visible(target)?;
        }
        self.update_note(
            node,
            Change::SetLink {
                node,
                target,
                present,
            },
        )
    }

    pub fn set_search(&mut self, node: NodeId, query: Option<SavedQuery>) -> Result<()> {
        if matches!(&query, Some(SavedQuery::Text(text)) if text.trim().is_empty() || text.len() > 1024)
        {
            return Err("Enter a search phrase between 1 and 1024 bytes".into());
        }
        self.update_note(node, Change::SetSearch { node, query })
    }

    pub fn set_field(&mut self, node: NodeId, field: FieldId, value: Option<String>) -> Result<()> {
        if let Some(value) = &value {
            let details = self.details(node)?;
            let mut spec = None;
            for tag in details.tags {
                if let Some(found) = search::fields_for_tag(&self.view, tag)?.remove(&field) {
                    spec = Some(found);
                    break;
                }
            }
            let spec = spec.ok_or("Add the field's tag to this note first")?;
            if !spec.kind.accepts(value) {
                return Err("This value does not match the field type".into());
            }
        }
        self.update_note(node, Change::SetField { node, field, value })
    }

    fn update_note(&mut self, node: NodeId, change: Change) -> Result<()> {
        let edit = self.local_edit(change)?;
        self.require_visible(node)?;
        self.append(edit)?;
        Ok(())
    }

    fn require_visible(&self, node: NodeId) -> Result<()> {
        if read_note(&self.view, node)?.is_none() {
            return Err("This Brain note is hidden or missing".into());
        }
        Ok(())
    }

    pub fn move_note(&mut self, node: NodeId, place: Place) -> Result<()> {
        self.refresh_view()?;
        if !matches!(node, NodeId::Note(_))
            || !crate::projection::move_allowed(&self.view, node, place)?
        {
            return Err("Choose a destination outside this note and its children".into());
        }
        self.update_note(node, Change::Move { node, place })
    }

    pub fn ensure_today(&mut self, date: NaiveDate) -> Result<NodeId> {
        let node = NodeId::Today(date);
        if self.note(node)?.is_none() {
            let edit = self.local_edit(Change::Create {
                node,
                place: Place::default(),
            })?;
            self.append(edit)?;
            if self.note(node)?.is_none() {
                return Err("Today is hidden. Restore it to resume editing".into());
            }
        }
        Ok(node)
    }

    pub fn create_note(&mut self, place: Place) -> Result<NodeId> {
        let (id, dependencies) = self.next_local_id()?;
        self.validate_place(place)?;
        let node = NodeId::Note(id);
        self.append(Edit {
            id,
            dependencies,
            change: Change::Create { node, place },
        })?;
        Ok(node)
    }

    pub fn create_note_with_text(&mut self, place: Place, text: &str) -> Result<NodeId> {
        self.create_tagged_note(place, text, None)
    }

    pub fn create_tagged_note(
        &mut self,
        place: Place,
        text: &str,
        tag: Option<NodeId>,
    ) -> Result<NodeId> {
        let edit = self.local_edit(Change::CreateWithText {
            place,
            text: text.to_owned(),
            tag,
        })?;
        self.validate_place(place)?;
        let node = NodeId::Note(edit.id);
        self.append(edit)?;
        Ok(node)
    }

    /// Save ordinary line edits from the version displayed by the local store.
    /// The token-level interface preserves multi-token and conflicting notes.
    pub fn set_note(&mut self, node: NodeId, text: &str) -> Result<bool> {
        self.refresh_view()?;
        let (old, target) =
            edit_target(&self.view, node)?.ok_or("This Brain note is hidden or missing")?;
        if old == text {
            return Ok(false);
        }
        let text = text.to_owned();
        let (change, extra) = match target {
            TextTarget::Empty => (
                Change::InsertText {
                    node,
                    after: None,
                    text,
                },
                None,
            ),
            TextTarget::Single { token, tip } => (Change::EditText { token, text }, Some(tip)),
            TextTarget::Multiple => return Err("This note needs token-level editing".into()),
        };
        let mut edit = self.local_edit(change)?;
        edit.dependencies.extend(extra);
        self.append(edit)?;
        Ok(true)
    }

    fn next_local_id(&mut self) -> Result<(OpId, std::collections::BTreeSet<OpId>)> {
        self.refresh_view()?;
        let state = read_frontier(&self.view)?;
        let clock = state
            .clock
            .checked_add(1)
            .ok_or("Brain operation clock exhausted")?;
        Ok((
            OpId {
                clock,
                device: self.device,
            },
            state.heads,
        ))
    }

    fn local_edit(&mut self, change: Change) -> Result<Edit> {
        let (id, dependencies) = self.next_local_id()?;
        Ok(Edit {
            id,
            dependencies,
            change,
        })
    }

    fn validate_place(&self, place: Place) -> Result<()> {
        if let Some(parent) = place.parent
            && read_note(&self.view, parent)?.is_none()
        {
            return Err("The Brain parent is hidden or missing".into());
        }
        Ok(())
    }

    fn refresh_view(&mut self) -> Result<()> {
        self.journal.require_healthy()?;
        if !has_stamp(&self.view, &self.projection_stamp())? {
            self.prime_view(&self.project()?)?;
        }
        if let Some(worker) = &self.readable
            && !self.readable_started
        {
            worker.notify();
            self.readable_started = true;
        }
        Ok(())
    }

    fn projection_stamp(&self) -> Vec<u8> {
        let mut stamp = b"paged-outline-v4\0".to_vec();
        stamp.extend(self.journal.replica().projection_stamp());
        stamp
    }

    pub fn append(&mut self, edit: Edit) -> Result<Vec<u8>> {
        let before = self.projection_stamp();
        let update = self.journal.append(edit.clone())?;
        if !update.is_empty() {
            // The edit is already durable. If this derived transaction fails,
            // the old stamp makes the next reader rebuild it from the journal.
            let _ = self.advance_projection(&before, &edit);
        }
        Ok(update)
    }

    fn advance_projection(&mut self, before: &[u8], edit: &Edit) -> Result<()> {
        let after = self.projection_stamp();
        let transaction = self
            .view
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if has_stamp(&transaction, before)? {
            let old = if matches!(edit.change, Change::Move { .. }) {
                readable::affected_root(&transaction, &edit.change)?
            } else {
                None
            };
            if apply_paged_edit(&transaction, edit)? {
                readable::mark_edit(
                    &transaction,
                    &edit.change,
                    NodeId::Note(edit.id),
                    old,
                    &after,
                )?;
                stamp_view(&transaction, &after)?;
                transaction.commit()?;
                self.wake_readable();
            }
        }
        Ok(())
    }

    fn wake_readable(&self) {
        if let Some(worker) = &self.readable {
            worker.notify();
        }
    }

    pub fn readable_error(&self) -> Option<String> {
        self.readable.as_ref().and_then(ReadableWorker::error)
    }

    pub fn version_id(&self) -> String {
        use sha2::{Digest, Sha256};
        format!(
            "{:x}",
            Sha256::digest(self.journal.replica().projection_stamp())
        )
    }

    pub fn import(&mut self, update: &[u8]) -> Result<()> {
        self.journal.import(update)?;
        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        self.journal.checkpoint()?;
        if self.readable.is_some() {
            self.wake_readable();
            Ok(())
        } else {
            flush_wal(&self.view)
        }
    }

    #[cfg(feature = "qualification")]
    pub(crate) fn wal_path(&self) -> PathBuf {
        self.view_file.with_file_name("live.sqlite-wal")
    }
}

fn library_device(library: &Path) -> Result<u64> {
    let path = library.join(DEVICE_FILE);
    let read = || fs::read_to_string(&path);
    match read() {
        Ok(saved) => return Ok(saved.trim().parse()?),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let device = loro::LoroDoc::new().peer_id();
    let mut temporary = tempfile::NamedTempFile::new_in(library)?;
    writeln!(temporary, "{device}")?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            return Ok(fs::read_to_string(library.join(DEVICE_FILE))?
                .trim()
                .parse()?);
        }
        Err(error) => return Err(error.into()),
    }
    sync_directory(library)?;
    Ok(device)
}
