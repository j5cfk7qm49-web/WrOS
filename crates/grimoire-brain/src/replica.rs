//! Replication operations shared by the durable Brain store and qualification.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{Edit, ModelError, OpId, Snapshot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Loro,
    Automerge,
}

impl Engine {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Loro => "loro",
            Self::Automerge => "automerge",
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Loro could not apply an operation: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("Loro could not export the document: {0}")]
    Encode(#[from] loro::LoroEncodeError),
    #[error("Automerge could not apply an operation: {0}")]
    Automerge(#[from] automerge::AutomergeError),
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

/// A consistent snapshot and any pending data that needs its own file.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub snapshot: Vec<u8>,
    pub pending: Option<Vec<u8>>,
}

pub trait Replica: Sized {
    const ENGINE: Engine;

    fn new(device: u64) -> Result<Self, StoreError>;
    fn restore(device: u64, checkpoint: &Checkpoint) -> Result<Self, StoreError>;
    fn import(&mut self, bytes: &[u8]) -> Result<(), StoreError>;
    fn checkpoint(&self) -> Result<Checkpoint, StoreError>;
    fn has_pending(&self) -> Result<bool, StoreError>;
    fn append(&mut self, edit: Edit) -> Result<Vec<u8>, StoreError>;
    fn project(&self) -> Result<Snapshot, StoreError>;

    #[cfg(feature = "qualification")]
    fn edit_count(&self) -> usize;
    #[cfg(feature = "qualification")]
    fn record_fixture_edit(&mut self, edit: Edit) -> Result<(), StoreError>;
    #[cfg(feature = "qualification")]
    fn record_fixture_review(&mut self, review: String, event: u64) -> Result<(), StoreError>;
}
