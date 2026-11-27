//! Loro storage adapter for the Brain qualification tests.
//!
//! The versioned edit map preserves application actions through replication.
//! Brain projects that history with its own rules for Undo, subtree deletion,
//! and stable logical identities. The native tree exercises placement and
//! merge behavior independently of that application projection.

use crate::model::{Change, Edit, Model, ModelError, NodeId, OpId, Place, Snapshot};
use crate::replica::{Checkpoint, Engine, Replica, StoreError};
use loro::{
    ExportMode, LoroDoc, LoroMap, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer,
};

const EDITS: &str = "brain-edits-v1";
const EDIT_PAGES: &str = "brain-edit-pages-v1";
const EDITS_PER_PAGE: u64 = 4096;
const OUTLINE: &str = "brain-outline-v1";
const LOCATIONS: &str = "brain-outline-ids-v1";
const LOGICAL_ID: &str = "logical-id";

pub type LoroStore = LoroReplica<false>;
pub type PagedLoroStore = LoroReplica<true>;

pub struct LoroReplica<const PAGED: bool> {
    doc: LoroDoc,
}

impl<const PAGED: bool> Replica for LoroReplica<PAGED> {
    const ENGINE: Engine = if PAGED {
        Engine::LoroPaged
    } else {
        Engine::Loro
    };

    fn new(device: u64) -> Result<Self, StoreError> {
        let doc = LoroDoc::new();
        doc.set_peer_id(device)?;
        doc.get_tree(OUTLINE).enable_fractional_index(0);
        Ok(Self { doc })
    }

    /// A restored backup takes the identity of the importing device. An
    /// ordinary restart may reuse the same identity with its own latest files.
    fn restore(device: u64, checkpoint: &Checkpoint) -> Result<Self, StoreError> {
        let mut store = Self::new(device)?;
        store.import(&checkpoint.snapshot)?;
        if let Some(bytes) = &checkpoint.pending {
            store.import(bytes)?;
        }
        Ok(store)
    }

    fn import(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        self.doc.import(bytes)?;
        Ok(())
    }

    fn checkpoint(&self) -> Result<Checkpoint, StoreError> {
        Ok(Checkpoint {
            snapshot: self.doc.export(ExportMode::Snapshot)?,
            pending: self.doc.export_pending_updates()?,
        })
    }

    fn has_pending(&self) -> Result<bool, StoreError> {
        Ok(self.doc.export_pending_updates()?.is_some())
    }

    /// Append one causal action and return only its binary update. Repeating
    /// the exact same local action returns an empty update.
    fn append(&mut self, edit: Edit) -> Result<Vec<u8>, StoreError> {
        let before = self.doc.oplog_vv();
        if !self.record(edit)? {
            return Ok(Vec::new());
        }
        self.doc.commit();
        Ok(self.doc.export(ExportMode::updates(&before))?)
    }

    // Building an aged fixture needs the same operations without exporting
    // and fsyncing a half-million individual benchmark setup messages.
    #[cfg(feature = "qualification")]
    fn record_fixture_edit(&mut self, edit: Edit) -> Result<(), StoreError> {
        self.record(edit)?;
        self.doc.commit();
        Ok(())
    }

    #[cfg(feature = "qualification")]
    fn record_fixture_review(&mut self, review: String, _event: u64) -> Result<(), StoreError> {
        self.doc
            .get_list("benchmark-review-events-v1")
            .push(review)?;
        Ok(())
    }

    #[cfg(feature = "qualification")]
    fn edit_count(&self) -> Result<usize, StoreError> {
        Ok(self.edit_maps()?.iter().map(LoroMap::len).sum())
    }

    fn project(&self) -> Result<Snapshot, StoreError> {
        let mut model = Model::default();
        for edits in self.edit_maps()? {
            let LoroValue::Map(records) = edits.get_value() else {
                return Err(StoreError::InvalidDocument);
            };
            for (key, value) in records.iter() {
                let LoroValue::String(record) = value else {
                    return Err(StoreError::InvalidDocument);
                };
                let edit: Edit = serde_json::from_str(record)?;
                if *key != edit_key(edit.id) {
                    return Err(StoreError::InvalidDocument);
                }
                model.insert(edit)?;
            }
        }
        Ok(model.snapshot())
    }
}

impl<const PAGED: bool> LoroReplica<PAGED> {
    #[cfg(feature = "runtime")]
    pub(crate) fn projection_stamp(&self) -> Vec<u8> {
        let version = self.doc.oplog_vv();
        let mut entries: Vec<_> = version.iter().collect();
        entries.sort_unstable_by_key(|entry| *entry.0);
        entries
            .into_iter()
            .flat_map(|(peer, counter)| peer.to_be_bytes().into_iter().chain(counter.to_be_bytes()))
            .collect()
    }

    fn edit_map(&self, id: OpId) -> LoroMap {
        if PAGED {
            self.doc.get_map(page_name(id.clock / EDITS_PER_PAGE))
        } else {
            self.doc.get_map(EDITS)
        }
    }

    fn edit_maps(&self) -> Result<Vec<LoroMap>, StoreError> {
        if !PAGED {
            return Ok(vec![self.doc.get_map(EDITS)]);
        }
        let LoroValue::Map(pages) = self.doc.get_map(EDIT_PAGES).get_value() else {
            return Err(StoreError::InvalidDocument);
        };
        pages
            .keys()
            .map(|address| {
                let address: u64 = address.parse().map_err(|_| StoreError::InvalidDocument)?;
                Ok(self.doc.get_map(page_name(address)))
            })
            .collect()
    }

    fn record(&mut self, edit: Edit) -> Result<bool, StoreError> {
        edit.validate()?;
        if edit.id.device != self.doc.peer_id() {
            return Err(StoreError::WrongDevice(edit.id));
        }
        for &dependency in &edit.dependencies {
            if self
                .edit_map(dependency)
                .get(&edit_key(dependency))
                .is_none()
            {
                return Err(StoreError::MissingDependency(dependency));
            }
        }
        let edits = self.edit_map(edit.id);
        let key = edit_key(edit.id);
        let record = serde_json::to_string(&edit)?;
        if let Some(existing) = edits.get(&key) {
            return if string_value(existing)? == record {
                Ok(false)
            } else {
                Err(ModelError::ReusedId.into())
            };
        }
        match edit.content() {
            Change::Create { node, place } => self.create_native(*node, *place)?,
            Change::CreateWithText { place, .. } | Change::CreateWork { place, .. } => {
                self.create_native(NodeId::Note(edit.id), *place)?;
            }
            Change::Move { node, place } => self.move_native(*node, *place)?,
            _ => {}
        }
        if PAGED && edits.is_empty() {
            self.doc
                .get_map(EDIT_PAGES)
                .insert(&(edit.id.clock / EDITS_PER_PAGE).to_string(), true)?;
        }
        edits.insert(&key, record)?;
        Ok(true)
    }

    /// Read the native forest in tree order for the storage qualification.
    pub fn native_outline(&self) -> Result<Vec<(NodeId, Option<NodeId>)>, StoreError> {
        let tree = self.doc.get_tree(OUTLINE);
        tree.get_nodes(false)
            .into_iter()
            .map(|row| {
                let node = logical_id(&tree, row.id)?;
                let parent = match row.parent {
                    TreeParentId::Root => None,
                    TreeParentId::Node(parent) => Some(logical_id(&tree, parent)?),
                    _ => return Err(StoreError::InvalidDocument),
                };
                Ok((node, parent))
            })
            .collect()
    }

    fn native_id(&self, node: NodeId) -> Result<Option<TreeID>, StoreError> {
        self.doc
            .get_map(LOCATIONS)
            .get(&serde_json::to_string(&node)?)
            .map(|value| TreeID::try_from(string_value(value)?.as_str()).map_err(StoreError::from))
            .transpose()
    }

    fn create_native(&self, node: NodeId, place: Place) -> Result<(), StoreError> {
        if self.native_id(node)?.is_some() {
            return Ok(());
        }
        let tree = self.doc.get_tree(OUTLINE);
        let parent = self.native_parent(place)?;
        let anchor = self.native_anchor(&tree, place, parent)?;
        let id = if let Some(anchor) = anchor {
            let siblings = tree.children(parent).ok_or(StoreError::InvalidDocument)?;
            let index = siblings
                .iter()
                .position(|&id| id == anchor)
                .ok_or(StoreError::InvalidDocument)?;
            tree.create_at(parent, index + 1)?
        } else {
            tree.create(parent)?
        };
        let name = serde_json::to_string(&node)?;
        tree.get_meta(id)?.insert(LOGICAL_ID, name.as_str())?;
        self.doc.get_map(LOCATIONS).insert(&name, id.to_string())?;
        Ok(())
    }

    fn move_native(&self, node: NodeId, place: Place) -> Result<(), StoreError> {
        let Some(id) = self.native_id(node)? else {
            return Ok(());
        };
        let parent = self.native_parent(place)?;
        if place.after == Some(node) || (place.parent.is_some() && parent.is_none()) {
            return Ok(());
        }
        let tree = self.doc.get_tree(OUTLINE);
        let mut ancestor = parent;
        while let Some(current) = ancestor {
            if current == id {
                return Ok(());
            }
            ancestor = match tree.parent(current) {
                Some(TreeParentId::Node(above)) => Some(above),
                _ => None,
            };
        }
        if let Some(anchor) = self.native_anchor(&tree, place, parent)? {
            tree.mov_after(id, anchor)?;
        } else {
            tree.mov(id, parent)?;
        }
        Ok(())
    }

    fn native_parent(&self, place: Place) -> Result<Option<TreeID>, StoreError> {
        place
            .parent
            .map(|node| self.native_id(node))
            .transpose()
            .map(Option::flatten)
    }

    fn native_anchor(
        &self,
        tree: &LoroTree,
        place: Place,
        parent: Option<TreeID>,
    ) -> Result<Option<TreeID>, StoreError> {
        let Some(anchor) = place.after else {
            return Ok(None);
        };
        Ok(self
            .native_id(anchor)?
            .filter(|&id| tree.parent(id) == Some(parent.into())))
    }
}

fn page_name(address: u64) -> String {
    format!("brain-edits-page-{address}")
}

fn edit_key(id: OpId) -> String {
    format!("{:016x}-{:016x}", id.clock, id.device)
}

fn string_value(value: ValueOrContainer) -> Result<String, StoreError> {
    match value {
        ValueOrContainer::Value(LoroValue::String(value)) => Ok(value.to_string()),
        _ => Err(StoreError::InvalidDocument),
    }
}

fn logical_id(tree: &LoroTree, id: TreeID) -> Result<NodeId, StoreError> {
    let value = tree
        .get_meta(id)?
        .get(LOGICAL_ID)
        .ok_or(StoreError::InvalidDocument)?;
    Ok(serde_json::from_str(&string_value(value)?)?)
}
