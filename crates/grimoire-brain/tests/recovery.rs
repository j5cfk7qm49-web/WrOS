use std::fs;
use std::path::Path;
use std::process::Command;

use grimoire_brain::automerge_store::AutomergeStore;
use grimoire_brain::loro_store::LoroStore;
use grimoire_brain::model::{Change, Edit, NodeId, OpId, Place};
use grimoire_brain::replica::Replica;
use grimoire_brain::storage::{DurableStore, Journal, StorageError};

fn edit(device: u64, clock: u64, change: Change) -> Edit {
    Edit {
        id: OpId { device, clock },
        dependencies: (clock > 1)
            .then_some(OpId {
                device,
                clock: clock.saturating_sub(1),
            })
            .into_iter()
            .collect(),
        change,
    }
}

fn page(device: u64) -> NodeId {
    NodeId::Note(OpId { device, clock: 1 })
}

fn create(device: u64) -> Edit {
    edit(
        device,
        1,
        Change::Create {
            node: page(device),
            place: Place::default(),
        },
    )
}

fn insert(device: u64) -> Edit {
    edit(
        device,
        2,
        Change::InsertText {
            node: page(device),
            after: None,
            text: "Words".into(),
        },
    )
}

fn amend(device: u64) -> Edit {
    edit(
        device,
        3,
        Change::EditText {
            token: OpId { device, clock: 2 },
            text: "Recovered words".into(),
        },
    )
}

#[test]
fn interrupted_checkpoint_keeps_pending_update() {
    checkpoint_keeps_pending_update::<LoroStore>();
}

#[test]
fn interrupted_automerge_checkpoint_keeps_pending_update() {
    checkpoint_keeps_pending_update::<AutomergeStore>();
}

fn checkpoint_keeps_pending_update<S: Replica>() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    let mut source = S::new(1).unwrap();
    let base = source.append(create(1)).unwrap();
    let missing = source.append(insert(1)).unwrap();
    let parked = source.append(amend(1)).unwrap();

    let mut disk = Journal::<S>::open(root, 2).unwrap();
    disk.import(&base).unwrap();
    disk.checkpoint().unwrap();
    disk.import(&parked).unwrap();
    assert!(disk.has_pending().unwrap());
    disk.checkpoint().unwrap();
    drop(disk);

    let original = root.join("journal/00000000000000000002.update");
    assert_eq!(&fs::read(&original).unwrap()[32..], parked);
    fs::write(
        root.join("checkpoint-00000000000000000003.snapshot"),
        b"incomplete",
    )
    .unwrap();
    fs::write(root.join("CURRENT.new"), b"not published").unwrap();
    fs::write(
        root.join("journal/00000000000000000003.update"),
        b"torn tail",
    )
    .unwrap();

    let mut recovered = Journal::<S>::open(root, 2).unwrap();
    assert!(recovered.has_pending().unwrap());
    assert_eq!(
        recovered.project().unwrap().body(page(1)),
        Some(String::new())
    );
    assert_eq!(recovered.recovery().quarantined.len(), 3);
    assert!(
        recovered
            .recovery()
            .quarantined
            .iter()
            .all(|path| path.is_file())
    );
    assert!(original.is_file());

    recovered.import(&missing).unwrap();
    assert!(!recovered.has_pending().unwrap());
    assert_eq!(
        recovered.project().unwrap().body(page(1)),
        Some("Recovered words".into())
    );
    recovered.checkpoint().unwrap();
    assert!(
        original.is_file(),
        "the previous checkpoint may still need its journal"
    );
    recovered.checkpoint().unwrap();
    assert!(
        !original.exists(),
        "a verified fallback now covers the parked operation"
    );
    drop(recovered);
    let reopened = Journal::<S>::open(root, 2).unwrap();
    assert_eq!(
        reopened.project().unwrap().body(page(1)),
        Some("Recovered words".into())
    );
    assert!(!reopened.has_pending().unwrap());
}

#[test]
fn a_damaged_latest_checkpoint_uses_the_previous_one_and_replays_saves() {
    damaged_checkpoint_uses_previous::<LoroStore>();
}

#[test]
fn damaged_automerge_checkpoint_uses_previous() {
    damaged_checkpoint_uses_previous::<AutomergeStore>();
}

fn damaged_checkpoint_uses_previous<S: Replica>() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    let mut disk = Journal::<S>::open(root, 3).unwrap();
    disk.append(create(3)).unwrap();
    disk.checkpoint().unwrap();
    disk.append(insert(3)).unwrap();
    disk.checkpoint().unwrap();
    disk.append(amend(3)).unwrap();
    drop(disk);
    fs::write(
        root.join("checkpoint-00000000000000000002.snapshot"),
        b"damaged",
    )
    .unwrap();

    let mut recovered = Journal::<S>::open(root, 3).unwrap();
    assert!(recovered.recovery().used_previous_checkpoint);
    assert_eq!(
        recovered.project().unwrap().body(page(3)),
        Some("Recovered words".into())
    );
    recovered.checkpoint().unwrap();
    drop(recovered);
    let reopened = Journal::<S>::open(root, 3).unwrap();
    assert!(!reopened.recovery().used_previous_checkpoint);
    assert_eq!(
        reopened.project().unwrap().body(page(3)),
        Some("Recovered words".into())
    );
}

#[test]
fn only_one_process_can_open_the_writer() {
    const CHILD: &str = "WROS_BRAIN_LOCK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        assert!(matches!(
            DurableStore::open(Path::new(&root), 8),
            Err(StorageError::WriterBusy)
        ));
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let writer = DurableStore::open(folder.path(), 7).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "only_one_process_can_open_the_writer"])
        .env(CHILD, folder.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
    drop(writer);
    assert!(DurableStore::open(folder.path(), 8).is_ok());
}

#[test]
fn a_failed_save_cannot_serve_or_acknowledge_its_transient_edit() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    let mut disk = DurableStore::open(root, 4).unwrap();
    disk.append(create(4)).unwrap();
    disk.checkpoint().unwrap();

    fs::rename(root.join("journal"), root.join("original-journal")).unwrap();
    fs::write(root.join("journal"), b"blocks a write").unwrap();
    assert!(matches!(disk.append(insert(4)), Err(StorageError::Io(_))));
    assert!(matches!(disk.project(), Err(StorageError::ReopenRequired)));
    assert!(matches!(
        disk.append(amend(4)),
        Err(StorageError::ReopenRequired)
    ));
    drop(disk);

    fs::remove_file(root.join("journal")).unwrap();
    fs::rename(root.join("original-journal"), root.join("journal")).unwrap();
    let reopened = DurableStore::open(root, 4).unwrap();
    assert_eq!(
        reopened.project().unwrap().body(page(4)),
        Some(String::new())
    );
}
