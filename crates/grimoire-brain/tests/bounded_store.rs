#![cfg(all(feature = "qualification", target_os = "linux"))]

use std::fs;

use grimoire_brain::bounded_store::{PagedBrain, SqliteLedger};
use grimoire_brain::loro_store::PagedLoroStore;
use grimoire_brain::model::{Change, Edit, NodeId, OpId, Place};
use grimoire_brain::replica::Replica;
use grimoire_brain::storage::Journal;

fn id(clock: u64) -> OpId {
    OpId { clock, device: 1 }
}

fn edit(clock: u64, before: Option<u64>, change: Change) -> Edit {
    Edit {
        id: id(clock),
        dependencies: before.map(id).into_iter().collect(),
        change,
    }
}

fn history() -> (NodeId, [Edit; 3]) {
    // Start at the end of a Loro map page so the dependent edit arrives in a
    // separate page.
    let node = NodeId::Note(id(4094));
    let edits = [
        edit(
            4094,
            None,
            Change::Create {
                node,
                place: Place::default(),
            },
        ),
        edit(
            4095,
            Some(4094),
            Change::InsertText {
                node,
                after: None,
                text: "Old".into(),
            },
        ),
        edit(
            4096,
            Some(4095),
            Change::EditText {
                token: id(4095),
                text: "Edited".into(),
            },
        ),
    ];
    (node, edits)
}

#[test]
fn paged_loro_rebuilds_a_stale_view_after_a_durable_update() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (node, [create, insert, amend]) = history();
    let mut disk = PagedBrain::open(root, 1).unwrap();
    disk.append(create).unwrap();
    disk.append(insert).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Old"));
    disk.checkpoint().unwrap();
    drop(disk);

    // Simulate an interruption after the journal acknowledged the action,
    // before the independently stored projection was updated.
    let mut disk = Journal::<PagedLoroStore>::open(root, 1).unwrap();
    disk.append(amend).unwrap();
    drop(disk);
    let mut disk = PagedBrain::open(root, 1).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Edited"));
    assert_eq!(
        disk.project().unwrap().body(node).as_deref(),
        Some("Edited")
    );
}

#[test]
fn paged_loro_keeps_out_of_order_updates_between_pages() {
    let directory = tempfile::tempdir().unwrap();
    let (node, [create, insert, amend]) = history();
    let mut origin = PagedLoroStore::new(1).unwrap();
    let create = origin.append(create).unwrap();
    let insert = origin.append(insert).unwrap();
    let amend = origin.append(amend).unwrap();
    let mut disk = PagedBrain::open(directory.path(), 2).unwrap();
    disk.import(&create).unwrap();
    disk.import(&amend).unwrap();
    disk.checkpoint().unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some(""));
    drop(disk);

    let mut disk = PagedBrain::open(directory.path(), 2).unwrap();
    disk.import(&insert).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Edited"));
}

#[test]
fn sqlite_ledger_restores_a_verified_prior_generation_and_rejoins() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let (node, [create, insert, amend]) = history();
    let mut disk = SqliteLedger::open(&root, 1).unwrap();
    assert!(SqliteLedger::open(&root, 2).is_err());
    disk.append(create).unwrap();
    disk.append(insert).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Old"));
    disk.checkpoint().unwrap();
    let update = disk.append(amend).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Edited"));
    disk.checkpoint().unwrap();
    drop(disk);

    fs::write(
        root.join("ledger-00000000000000000002.snapshot"),
        b"torn backup",
    )
    .unwrap();
    let restored = directory.path().join("restored");
    let mut disk = SqliteLedger::restore_backup(&root, &restored, 2).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Old"));
    disk.import(&update).unwrap();
    disk.import(&update).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Edited"));
    assert!(
        disk.append(edit(
            4097,
            Some(4096),
            Change::EditText {
                token: id(4095),
                text: "Reused identity".into(),
            }
        ))
        .is_err()
    );
}

#[test]
fn sqlite_ledger_retains_pending_ops_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (node, [create, insert, amend]) = history();
    let mut source = SqliteLedger::open(&directory.path().join("source"), 1).unwrap();
    let create = source.append(create).unwrap();
    let insert = source.append(insert).unwrap();
    let amend = source.append(amend).unwrap();
    let target = directory.path().join("target");
    let mut disk = SqliteLedger::open(&target, 2).unwrap();
    disk.import(&amend).unwrap();
    assert_eq!(disk.note(node).unwrap(), None);
    let different = Edit {
        id: OpId {
            clock: 4097,
            device: 2,
        },
        dependencies: [id(4096)].into(),
        change: Change::EditText {
            token: id(4095),
            text: "Too early".into(),
        },
    };
    assert!(disk.append(different).is_err());
    disk.checkpoint().unwrap();
    drop(disk);
    let mut disk = SqliteLedger::open(&target, 2).unwrap();
    disk.import(&create).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some(""));
    disk.import(&insert).unwrap();
    assert_eq!(disk.note(node).unwrap().as_deref(), Some("Edited"));
}

#[test]
fn sqlite_ledger_keeps_the_last_complete_save_and_view_after_a_torn_wal() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source");
    let crash_path = directory.path().join("crash");
    fs::create_dir(&crash_path).unwrap();
    let (node, [create, insert, amend]) = history();
    let mut source = SqliteLedger::open(&source_path, 1).unwrap();
    source.append(create).unwrap();
    source.append(insert).unwrap();
    assert_eq!(source.note(node).unwrap().as_deref(), Some("Old"));

    source.append(amend).unwrap();
    let complete = fs::metadata(source_path.join("ledger.sqlite-wal"))
        .unwrap()
        .len();
    source
        .append(edit(
            4097,
            Some(4096),
            Change::EditText {
                token: id(4095),
                text: "In the torn transaction".into(),
            },
        ))
        .unwrap();
    assert!(
        fs::metadata(source_path.join("ledger.sqlite-wal"))
            .unwrap()
            .len()
            > complete + 100
    );
    for name in ["ledger.sqlite", "ledger.sqlite-wal"] {
        fs::copy(source_path.join(name), crash_path.join(name)).unwrap();
    }
    let wal = fs::OpenOptions::new()
        .write(true)
        .open(crash_path.join("ledger.sqlite-wal"))
        .unwrap();
    wal.set_len(wal.metadata().unwrap().len() - 100).unwrap();

    let mut recovered = SqliteLedger::open(&crash_path, 1).unwrap();
    assert_eq!(recovered.note(node).unwrap().as_deref(), Some("Edited"));
    assert_eq!(
        recovered.project().unwrap().body(node).as_deref(),
        Some("Edited")
    );
    assert_eq!(recovered.edit_count().unwrap(), 3);
}
