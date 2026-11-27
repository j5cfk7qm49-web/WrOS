//! Automerge adapter for the same Brain operations and recovery tests.

use automerge::{
    ActorId, Automerge, Change as AutomergeChange, ROOT, ReadDoc, ScalarValueRef, ValueRef,
    transaction::Transactable,
};

use crate::model::{Edit, Model, ModelError, OpId, Snapshot};
use crate::replica::{Checkpoint, Engine, Replica, StoreError};

const SNAPSHOT: u8 = 1;
const UPDATE: u8 = 2;

pub struct AutomergeStore {
    doc: Automerge,
    device: u64,
}

impl Replica for AutomergeStore {
    const ENGINE: Engine = Engine::Automerge;

    fn new(device: u64) -> Result<Self, StoreError> {
        Ok(Self {
            doc: Automerge::new().with_actor(actor(device)),
            device,
        })
    }

    fn restore(device: u64, checkpoint: &Checkpoint) -> Result<Self, StoreError> {
        let Some((&SNAPSHOT, payload)) = checkpoint.snapshot.split_first() else {
            return Err(StoreError::InvalidDocument);
        };
        if checkpoint.pending.is_some() {
            return Err(StoreError::InvalidDocument);
        }
        Ok(Self {
            doc: Automerge::load(payload)?.with_actor(actor(device)),
            device,
        })
    }

    fn import(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        match bytes.split_first() {
            Some((&UPDATE, payload)) => {
                let change = AutomergeChange::from_bytes(payload.to_vec())
                    .map_err(|_| StoreError::InvalidDocument)?;
                self.doc.apply_changes([change])?;
            }
            Some((&SNAPSHOT, payload)) => {
                let imported = Automerge::load(payload)?;
                if self.doc.is_empty() && !self.has_pending()? {
                    self.doc = imported.with_actor(actor(self.device));
                } else {
                    self.doc.load_incremental(payload)?;
                }
            }
            _ => return Err(StoreError::InvalidDocument),
        }
        Ok(())
    }

    fn checkpoint(&self) -> Result<Checkpoint, StoreError> {
        // Automerge includes unresolved changes in the snapshot itself.
        Ok(Checkpoint {
            snapshot: packet(SNAPSHOT, &self.doc.save()),
            pending: None,
        })
    }

    fn has_pending(&self) -> Result<bool, StoreError> {
        Ok(!self.doc.get_missing_deps(&[]).is_empty())
    }

    fn append(&mut self, edit: Edit) -> Result<Vec<u8>, StoreError> {
        let Some(hash) = self.record(edit)? else {
            return Ok(Vec::new());
        };
        let mut change = self
            .doc
            .get_change_by_hash(&hash)
            .ok_or(StoreError::InvalidDocument)?;
        Ok(packet(UPDATE, &change.bytes()))
    }

    fn project(&self) -> Result<Snapshot, StoreError> {
        let mut model = Model::default();
        for record in self
            .doc
            .map_range(ROOT, String::from("e-")..String::from("f"))
        {
            let ValueRef::Scalar(ScalarValueRef::Bytes(bytes)) = record.value else {
                return Err(StoreError::InvalidDocument);
            };
            let edit: Edit = serde_json::from_slice(&bytes)?;
            if record.conflict || record.key != edit_key(edit.id) {
                return Err(StoreError::InvalidDocument);
            }
            model.insert(edit)?;
        }
        Ok(model.snapshot())
    }

    #[cfg(feature = "qualification")]
    fn edit_count(&self) -> Result<usize, StoreError> {
        Ok(self
            .doc
            .map_range(ROOT, String::from("e-")..String::from("f"))
            .count())
    }

    #[cfg(feature = "qualification")]
    fn record_fixture_edit(&mut self, edit: Edit) -> Result<(), StoreError> {
        self.record(edit)?;
        Ok(())
    }

    #[cfg(feature = "qualification")]
    fn record_fixture_review(&mut self, review: String, event: u64) -> Result<(), StoreError> {
        let mut transaction = self.doc.transaction();
        transaction.put(ROOT, format!("r-{event:016x}"), review.into_bytes())?;
        transaction.commit();
        Ok(())
    }
}

impl AutomergeStore {
    fn record(&mut self, edit: Edit) -> Result<Option<automerge::ChangeHash>, StoreError> {
        edit.validate()?;
        if edit.id.device != self.device {
            return Err(StoreError::WrongDevice(edit.id));
        }
        for &dependency in &edit.dependencies {
            if self.doc.get(ROOT, edit_key(dependency))?.is_none() {
                return Err(StoreError::MissingDependency(dependency));
            }
        }
        let key = edit_key(edit.id);
        let record = serde_json::to_vec(&edit)?;
        if let Some((existing, _)) = self.doc.get(ROOT, key.as_str())? {
            return if existing.as_bytes() == Some(record.as_slice()) {
                Ok(None)
            } else {
                Err(ModelError::ReusedId.into())
            };
        }
        let mut transaction = self.doc.transaction();
        transaction.put(ROOT, key, record)?;
        Ok(transaction.commit().0)
    }
}

fn actor(device: u64) -> ActorId {
    ActorId::from(device.to_be_bytes().as_slice())
}

fn edit_key(id: OpId) -> String {
    format!("e-{:016x}-{:016x}", id.clock, id.device)
}

fn packet(kind: u8, bytes: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(1 + bytes.len());
    packet.push(kind);
    packet.extend_from_slice(bytes);
    packet
}
