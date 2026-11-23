use std::collections::BTreeSet;

use chrono::NaiveDate;
use grimoire_brain::loro_store::LoroStore;
use grimoire_brain::model::{Builtin, Change, Edit, FieldId, Model, NodeId, OpId, Place};

#[derive(Clone)]
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for end in (1..values.len()).rev() {
            values.swap(end, self.next() as usize % (end + 1));
        }
    }
}

fn id(clock: u64, device: u64) -> OpId {
    OpId { clock, device }
}

fn note(clock: u64, device: u64) -> NodeId {
    NodeId::Note(id(clock, device))
}

fn edit(clock: u64, device: u64, previous: Option<OpId>, change: Change) -> Edit {
    Edit {
        id: id(clock, device),
        dependencies: previous.into_iter().collect(),
        change,
    }
}

fn create(node: NodeId, parent: Option<NodeId>) -> Change {
    Change::Create {
        node,
        place: Place {
            parent,
            after: None,
        },
    }
}

fn move_under(node: NodeId, parent: NodeId) -> Change {
    Change::Move {
        node,
        place: Place {
            parent: Some(parent),
            after: None,
        },
    }
}

fn text(node: NodeId, text: &str) -> Change {
    Change::InsertText {
        node,
        after: None,
        text: text.into(),
    }
}

fn apply(store: &mut LoroStore, model: &mut Model, op: Edit, trace: &mut Vec<Edit>) -> Vec<u8> {
    trace.push(op.clone());
    model.insert(op.clone()).unwrap();
    store.append(op).unwrap()
}

#[test]
fn three_peers_converge_with_pending_dependencies() {
    for seed in 25_001..25_009 {
        run_schedule(seed, 2);
        run_schedule(seed, 3);
    }
}

fn run_schedule(seed: u64, device_count: u64) {
    let mut random = Random(seed);
    let parent = note(1, 1);
    let child = note(2, 1);
    let other = note(3, 1);
    let index = note(4, 1);
    let original_text = id(5, 1);
    let today = NodeId::Today(NaiveDate::from_ymd_opt(2026, 11, 22).unwrap());
    let book = NodeId::Builtin(Builtin::Book);
    let field = FieldId::Builtin {
        tag: Builtin::Book,
        slot: 1,
    };
    let mut oracle = Model::default();
    let mut trace = Vec::new();
    let mut peers: Vec<_> = (1..=device_count)
        .map(|p| LoroStore::new(p).unwrap())
        .collect();
    let base = [
        edit(1, 1, None, create(parent, None)),
        edit(2, 1, Some(id(1, 1)), create(child, Some(parent))),
        edit(3, 1, Some(id(2, 1)), create(other, None)),
        edit(4, 1, Some(id(3, 1)), create(index, None)),
        edit(5, 1, Some(id(4, 1)), text(child, "Original")),
        edit(6, 1, Some(id(5, 1)), text(other, "Other")),
    ];
    for op in base {
        let bytes = apply(&mut peers[0], &mut oracle, op, &mut trace);
        for peer in &mut peers[1..] {
            peer.import(&bytes).unwrap();
        }
    }
    let old_backup = peers[0].checkpoint().unwrap();
    let deletion = oracle.snapshot().delete_subtree(parent).unwrap();
    let mut updates = Vec::new();

    for (index_of_peer, peer) in peers.iter_mut().enumerate() {
        let device = index_of_peer as u64 + 1;
        let mut previous = id(6, 1);
        let mut clock = 7;
        let mut actions = vec![
            create(today, None),
            text(today, &format!("From device {device}. ")),
            create(book, None),
            Change::SetField {
                node: index,
                field,
                value: Some(format!("Writer {device}")),
            },
        ];
        if device == 1 {
            actions.push(deletion.clone());
        } else {
            actions.extend([
                move_under(child, other),
                Change::EditText {
                    token: original_text,
                    text: format!("Survives from {device}"),
                },
                Change::Split {
                    at: id(6, 1),
                    right: note(clock + 6, device),
                },
                Change::Join {
                    left: index,
                    right: note(clock + 6, device),
                },
            ]);
        }
        for action in actions {
            let op = edit(clock, device, Some(previous), action);
            previous = op.id;
            clock += 1;
            updates.push((device, apply(peer, &mut oracle, op, &mut trace)));
        }
        if device == 2 {
            let target = id(10, device);
            let mut undo = edit(clock, device, Some(previous), Change::Undo { target });
            undo.dependencies.insert(target);
            previous = undo.id;
            clock += 1;
            updates.push((device, apply(peer, &mut oracle, undo, &mut trace)));
        }
        for _ in 0..20 {
            let action = match random.next() % 4 {
                0 => Change::SetTag {
                    node: today,
                    tag: book,
                    present: random.next().is_multiple_of(2),
                },
                1 => Change::SetField {
                    node: index,
                    field,
                    value: Some(format!("Field {}", random.next())),
                },
                2 => create(note(clock, device), Some(index)),
                _ => text(today, &format!("Seed {seed}, entry {clock}. ")),
            };
            let op = edit(clock, device, Some(previous), action);
            previous = op.id;
            clock += 1;
            updates.push((device, apply(peer, &mut oracle, op, &mut trace)));
        }
    }

    // Give the second device the last delta from device one first. Its only copy
    // after restart comes from the checkpoint's pending sidecar.
    let parked = updates
        .iter()
        .rposition(|(device, _)| *device == 1)
        .unwrap();
    peers[1].import(&updates[parked].1).unwrap();
    assert!(peers[1].has_pending().unwrap(), "seed {seed}");
    peers[1] = LoroStore::restore(100, &peers[1].checkpoint().unwrap()).unwrap();
    assert!(peers[1].has_pending().unwrap(), "seed {seed}");

    // A restored, old backup joins with a fresh device identity.
    let mut restored = LoroStore::restore(200, &old_backup).unwrap();
    let old_edit = edit(7, 200, Some(id(6, 1)), text(index, "From the old backup"));
    updates.push((200, apply(&mut restored, &mut oracle, old_edit, &mut trace)));
    peers.push(restored);

    let expected = oracle.snapshot();
    assert!(!expected.is_visible(child), "seed {seed}");
    assert!(expected.body(child).unwrap().starts_with("Survives"));
    assert!(
        expected
            .body(index)
            .unwrap()
            .contains("From the old backup")
    );
    assert_eq!(
        expected
            .children(None)
            .iter()
            .filter(|&&n| n == today)
            .count(),
        1
    );
    assert_eq!(
        expected
            .children(None)
            .iter()
            .filter(|&&n| n == book)
            .count(),
        1
    );
    for (target, peer) in peers.iter_mut().enumerate() {
        let mut delivery: Vec<_> = (0..updates.len())
            .filter(|&i| target != 1 || i != parked)
            .collect();
        random.shuffle(&mut delivery);
        for (attempt, i) in delivery.into_iter().enumerate() {
            let bytes = &updates[i].1;
            peer.import(bytes).unwrap_or_else(|error| {
                panic!("seed {seed}, peers {device_count}, receiver {target}, delivery {i}: {error:?}\n{trace:#?}")
            });
            if attempt.is_multiple_of(5) {
                peer.import(bytes).unwrap();
            }
        }
        assert!(
            !peer.has_pending().unwrap(),
            "seed {seed}, receiver {target}\n{trace:#?}"
        );
        assert_eq!(
            peer.project().unwrap(),
            expected,
            "seed {seed}, receiver {target}\n{trace:#?}"
        );
        assert_eq!(
            peer.edit_count(),
            trace.len(),
            "seed {seed}, receiver {target}"
        );
    }
}

#[test]
fn native_tree_retains_both_nodes_when_concurrent_moves_form_a_cycle() {
    let a = note(1, 1);
    let b = note(2, 1);
    let mut first = LoroStore::new(1).unwrap();
    let mut second = LoroStore::new(2).unwrap();
    for op in [
        edit(1, 1, None, create(a, None)),
        edit(2, 1, Some(id(1, 1)), create(b, None)),
    ] {
        second.import(&first.append(op).unwrap()).unwrap();
    }
    let updates_a = first
        .append(edit(3, 1, Some(id(2, 1)), move_under(a, b)))
        .unwrap();
    let updates_b = second
        .append(edit(3, 2, Some(id(2, 1)), move_under(b, a)))
        .unwrap();
    assert_eq!(first.native_outline().unwrap(), [(b, None), (a, Some(b))]);
    assert_eq!(second.native_outline().unwrap(), [(a, None), (b, Some(a))]);
    first.import(&updates_b).unwrap();
    second.import(&updates_a).unwrap();

    let outline = first.native_outline().unwrap();
    assert_eq!(outline, second.native_outline().unwrap());
    assert_eq!(
        outline
            .iter()
            .map(|(node, _)| *node)
            .collect::<BTreeSet<_>>(),
        [a, b].into()
    );
    assert_eq!(outline.len(), 2);
    assert!(outline[0].1.is_none());
    assert_eq!(outline[1].1, Some(outline[0].0));
}
