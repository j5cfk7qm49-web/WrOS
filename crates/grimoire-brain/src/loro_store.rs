//! Loro storage adapter for the Brain qualification tests.
//!
//! The versioned edit map preserves application actions through replication.
//! Brain projects that history with its own rules for Undo, subtree deletion,
//! and stable logical identities. The native tree exercises placement and
//! merge behavior independently of that application projection.

use loro::{
    ExportMode, LoroDoc, LoroEncodeError, LoroError, LoroTree, LoroValue, TreeID, TreeParentId,
    ValueOrContainer,
};
use thiserror::Error;

use crate::model::{Change, Edit, Model, ModelError, NodeId, OpId, Place, Snapshot};

const EDITS: &str = "brain-edits-v1";
const OUTLINE: &str = "brain-outline-v1";
const LOCATIONS: &str = "brain-outline-ids-v1";
const LOGICAL_ID: &str = "logical-id";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Loro could not apply an operation: {0}")]
    Loro(#[from] LoroError),
    #[error("Loro could not export the document: {0}")]
    Encode(#[from] LoroEncodeError),
    #[error("Invalid Brain record: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid Brain edit: {0:?}")]
    Model(ModelError),
    #[error("Local edit {0:?} belongs to another device")]
    WrongDevice(OpId),
    #[error("The local edit needs a missing operation: {0:?}")]
    MissingDependency(OpId),
    #[error("Unexpected value in the Brain document")]
    InvalidDocument,
}

impl From<ModelError> for StoreError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// A consistent Loro snapshot and the received operations it could not apply.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub snapshot: Vec<u8>,
    pub pending: Option<Vec<u8>>,
}

pub struct LoroStore {
    doc: LoroDoc,
}

impl LoroStore {
    pub fn new(device: u64) -> Result<Self, StoreError> {
        let doc = LoroDoc::new();
        doc.set_peer_id(device)?;
        doc.get_tree(OUTLINE).enable_fractional_index(0);
        Ok(Self { doc })
    }

    /// A restored backup takes the identity of the importing device. An
    /// ordinary restart may reuse the same identity with its own latest files.
    pub fn restore(device: u64, checkpoint: &Checkpoint) -> Result<Self, StoreError> {
        let mut store = Self::new(device)?;
        store.import(&checkpoint.snapshot)?;
        if let Some(bytes) = &checkpoint.pending {
            store.import(bytes)?;
        }
        Ok(store)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        self.doc.import(bytes)?;
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<Checkpoint, StoreError> {
        Ok(Checkpoint {
            snapshot: self.doc.export(ExportMode::Snapshot)?,
            pending: self.doc.export_pending_updates()?,
        })
    }

    pub fn has_pending(&self) -> Result<bool, StoreError> {
        Ok(self.doc.export_pending_updates()?.is_some())
    }

    /// Append one causal action and return only its binary update. Repeating
    /// the exact same local action returns an empty update.
    pub fn append(&mut self, edit: Edit) -> Result<Vec<u8>, StoreError> {
        edit.validate()?;
        if edit.id.device != self.doc.peer_id() {
            return Err(StoreError::WrongDevice(edit.id));
        }
        let edits = self.doc.get_map(EDITS);
        for &dependency in &edit.dependencies {
            if edits.get(&edit_key(dependency)).is_none() {
                return Err(StoreError::MissingDependency(dependency));
            }
        }
        let key = edit_key(edit.id);
        let record = serde_json::to_string(&edit)?;
        if let Some(existing) = edits.get(&key) {
            return if string_value(existing)? == record {
                Ok(Vec::new())
            } else {
                Err(ModelError::ReusedId.into())
            };
        }
        let before = self.doc.oplog_vv();
        match edit.change {
            Change::Create { node, place } => self.create_native(node, place)?,
            Change::Move { node, place } => self.move_native(node, place)?,
            _ => {}
        }
        edits.insert(&key, record)?;
        self.doc.commit();
        Ok(self.doc.export(ExportMode::updates(&before))?)
    }

    pub fn edit_count(&self) -> usize {
        self.doc.get_map(EDITS).len()
    }

    pub fn project(&self) -> Result<Snapshot, StoreError> {
        let LoroValue::Map(records) = self.doc.get_map(EDITS).get_value() else {
            return Err(StoreError::InvalidDocument);
        };
        let mut model = Model::default();
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
        Ok(model.snapshot())
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
