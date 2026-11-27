#![cfg(test)]
use chrono::NaiveDate;
use grimoire_brain::model::{
    Builtin, Change, Edit, FieldId, Model, ModelError, NodeId, OpId, Place, Snapshot,
};
use grimoire_brain::schema::{FieldKind, FieldSpec, SavedQuery};

const A: u64 = 1;
const B: u64 = 2;
const C: u64 = 3;

fn stamp(clock: u64, device: u64) -> OpId {
    OpId { clock, device }
}

fn note(clock: u64, device: u64) -> NodeId {
    NodeId::Note(stamp(clock, device))
}

fn edit(id: OpId, dependencies: &[OpId], change: Change) -> Edit {
    Edit {
        id,
        dependencies: dependencies.iter().copied().collect(),
        change,
    }
}

fn create(id: OpId, dependencies: &[OpId], node: NodeId, parent: Option<NodeId>) -> Edit {
    edit(
        id,
        dependencies,
        Change::Create {
            node,
            place: Place {
                parent,
                after: None,
            },
        },
    )
}

fn text(id: OpId, dependencies: &[OpId], node: NodeId, after: Option<OpId>, value: &str) -> Edit {
    edit(
        id,
        dependencies,
        Change::InsertText {
            node,
            after,
            text: value.into(),
        },
    )
}

fn replay(events: impl IntoIterator<Item = Edit>) -> Model {
    let mut model = Model::default();
    for event in events {
        model.insert(event).unwrap();
    }
    model
}

fn assert_all_orders(events: &[Edit]) -> Snapshot {
    let forward = replay(events.iter().cloned()).snapshot();
    let backward = replay(events.iter().rev().cloned()).snapshot();
    let by_device = replay((A..=C).flat_map(|device| {
        events
            .iter()
            .filter(move |event| event.id.device == device)
            .cloned()
    }))
    .snapshot();
    assert_eq!(forward, backward);
    assert_eq!(forward, by_device);
    assert!(forward.pending().is_empty());
    forward
}

#[test]
fn renamed_field_and_link_keep_their_targets_and_failed_conversion_keeps_text() {
    let tag = NodeId::Builtin(Builtin::Book);
    let isbn = FieldId::Builtin {
        tag: Builtin::Book,
        slot: 2,
    };
    let page = note(1, A);
    let target = note(2, A);
    let field = |name: &str, kind| FieldSpec {
        name: name.into(),
        kind,
        required: false,
    };
    let events = [
        create(stamp(1, A), &[], page, None),
        create(stamp(2, A), &[stamp(1, A)], target, None),
        edit(
            stamp(3, A),
            &[stamp(2, A)],
            Change::SetTag {
                node: page,
                tag,
                present: true,
            },
        ),
        edit(
            stamp(4, A),
            &[stamp(3, A)],
            Change::SetField {
                node: page,
                field: isbn,
                value: Some("978-0".into()),
            },
        ),
        edit(
            stamp(5, A),
            &[stamp(4, A)],
            Change::DefineField {
                tag,
                field: isbn,
                spec: field("Book identifier", FieldKind::Number),
            },
        ),
        edit(
            stamp(6, B),
            &[stamp(5, A)],
            Change::SetLink {
                node: page,
                target,
                present: true,
            },
        ),
        edit(
            stamp(7, A),
            &[stamp(6, B)],
            Change::SetSearch {
                node: page,
                query: Some(SavedQuery::Tagged(tag)),
            },
        ),
    ];
    let view = assert_all_orders(&events);
    let schema = &view.fields_for_tag(tag)[&isbn];
    assert_eq!(schema.name, "Book identifier");
    assert!(!schema.kind.accepts("978-0"));
    assert_eq!(view.field(page, isbn), Some("978-0"));
    assert_eq!(view.links(page), [target]);
    assert_eq!(view.backlinks(target), [page]);
    assert_eq!(view.saved_search(page), Some(&SavedQuery::Tagged(tag)));
    assert_eq!(target.external_id().parse::<NodeId>().unwrap(), target);
}

#[test]
fn model_preserves_visible_and_recoverable_content() {
    let parent = note(1, A);
    let child = note(2, A);
    let outside = note(3, A);
    let token = stamp(4, A);
    let base = [
        create(stamp(1, A), &[], parent, None),
        create(stamp(2, A), &[stamp(1, A)], child, Some(parent)),
        create(stamp(3, A), &[stamp(2, A)], outside, None),
        text(token, &[stamp(3, A)], child, None, "Original"),
    ];
    let deletion = stamp(5, A);
    let other_deletion = stamp(6, C);
    let deleted_subtree = replay(base.clone())
        .snapshot()
        .delete_subtree(parent)
        .unwrap();
    let mut events = base.to_vec();
    events.extend([
        edit(deletion, &[token], deleted_subtree.clone()),
        edit(
            stamp(5, B),
            &[token],
            Change::Move {
                node: child,
                place: Place {
                    parent: Some(outside),
                    after: None,
                },
            },
        ),
        edit(
            stamp(6, B),
            &[stamp(5, B)],
            Change::EditText {
                token,
                text: "Edited elsewhere".into(),
            },
        ),
        edit(other_deletion, &[token], deleted_subtree),
    ]);

    let hidden = assert_all_orders(&events);
    assert_eq!(hidden.children(None), [outside]);
    assert!(!hidden.is_visible(child));
    assert_eq!(hidden.parent(child), Some(outside));
    assert_eq!(hidden.body(child), Some("Edited elsewhere".into()));

    events.push(edit(stamp(7, A), &[deletion], Change::Restore { deletion }));
    assert!(!assert_all_orders(&events).is_visible(child));
    events.push(edit(
        stamp(8, A),
        &[other_deletion],
        Change::Restore {
            deletion: other_deletion,
        },
    ));
    let restored = assert_all_orders(&events);
    assert_eq!(restored.children(None), [parent, outside]);
    assert_eq!(restored.children(Some(outside)), [child]);
    assert_eq!(restored.body(child), Some("Edited elsewhere".into()));
}

#[test]
fn a_field_conflict_keeps_its_alternative_and_undo_is_local() {
    let id = note(1, A);
    let field = FieldId::Builtin {
        tag: Builtin::Book,
        slot: 1,
    };
    let assignment = |when, depends_on: &[OpId], value: &str| {
        edit(
            when,
            depends_on,
            Change::SetField {
                node: id,
                field,
                value: Some(value.into()),
            },
        )
    };
    let first = stamp(2, A);
    let second = stamp(2, B);
    let mut events = vec![
        create(stamp(1, A), &[], id, None),
        assignment(first, &[stamp(1, A)], "First"),
        assignment(second, &[stamp(1, A)], "Second"),
    ];
    let disputed = assert_all_orders(&events);
    assert_eq!(disputed.field(id, field), Some("Second"));
    assert_eq!(disputed.field_alternatives(id, field), [Some("First")]);

    let resolution = stamp(3, A);
    events.push(assignment(resolution, &[first, second], "Resolved"));
    let resolved = assert_all_orders(&events);
    assert_eq!(resolved.field(id, field), Some("Resolved"));
    assert!(resolved.field_alternatives(id, field).is_empty());

    let undo = edit(
        stamp(4, A),
        &[resolution],
        Change::Undo { target: resolution },
    );
    events.push(undo);
    assert_eq!(assert_all_orders(&events).field(id, field), Some("Second"));

    let mut model = replay(events);
    let foreign_undo = edit(stamp(5, A), &[second], Change::Undo { target: second });
    assert_eq!(model.insert(foreign_undo), Err(ModelError::ForeignUndo));
    assert_eq!(model.snapshot().field(id, field), Some("Second"));
}

#[test]
fn split_and_join_follow_tokens_through_edits_and_undo() {
    let left = note(1, A);
    let right = note(4, A);
    let child = note(6, A);
    let prefix = stamp(2, A);
    let boundary = stamp(3, A);
    let split = stamp(4, A);
    let join = stamp(7, A);
    let tag = NodeId::Builtin(Builtin::Concept);
    let mut events = vec![
        create(stamp(1, A), &[], left, None),
        create(stamp(1, B), &[], tag, None),
        text(prefix, &[stamp(1, A)], left, None, "left"),
        text(boundary, &[prefix], left, Some(prefix), " right"),
        edit(
            split,
            &[boundary],
            Change::Split {
                at: boundary,
                right,
            },
        ),
        text(stamp(5, B), &[boundary], left, Some(boundary), " plus"),
        edit(
            stamp(5, A),
            &[split, stamp(1, B)],
            Change::SetTag {
                node: right,
                tag,
                present: true,
            },
        ),
        create(stamp(6, A), &[stamp(5, A)], child, Some(right)),
        edit(join, &[stamp(6, A)], Change::Join { left, right }),
        edit(
            stamp(8, C),
            &[boundary],
            Change::EditText {
                token: boundary,
                text: " revised".into(),
            },
        ),
    ];
    let merged = assert_all_orders(&events);
    assert_eq!(merged.body(left), Some("left revised plus".into()));
    assert_eq!(merged.children(Some(left)), [child]);
    assert_eq!(merged.link_target(right), Some(left));

    events.push(edit(stamp(9, A), &[join], Change::Undo { target: join }));
    let unjoined = assert_all_orders(&events);
    assert_eq!(unjoined.body(left), Some("left".into()));
    assert_eq!(unjoined.body(right), Some(" revised plus".into()));
    assert_eq!(unjoined.children(Some(right)), [child]);
    assert!(unjoined.has_tag(right, tag));
}

#[test]
fn delete_hides_a_concurrent_split_but_keeps_a_split_it_already_saw() {
    let left = note(1, A);
    let right = note(3, A);
    let token = stamp(2, A);
    let split = stamp(3, A);
    let mut events = vec![
        create(stamp(1, A), &[], left, None),
        text(token, &[stamp(1, A)], left, None, "split text"),
        edit(split, &[token], Change::Split { at: token, right }),
    ];
    let delete_change = replay(events[..2].iter().cloned())
        .snapshot()
        .delete_subtree(left)
        .unwrap();
    let deletion = stamp(4, B);
    events.push(edit(deletion, &[token], delete_change.clone()));
    let concurrent = assert_all_orders(&events);
    assert!(concurrent.children(None).is_empty());
    assert_eq!(concurrent.body(left), Some("split text".into()));

    events.push(edit(stamp(5, B), &[deletion], Change::Restore { deletion }));
    let restored = assert_all_orders(&events);
    assert_eq!(restored.children(None), [left, right]);
    assert_eq!(restored.body(right), Some("split text".into()));

    events.truncate(3);
    events.push(edit(deletion, &[split], delete_change));
    let sequential = assert_all_orders(&events);
    assert_eq!(sequential.children(None), [right]);
    assert_eq!(sequential.body(right), Some("split text".into()));
}

#[test]
fn duplicate_today_and_builtin_initialization_preserves_both_peers() {
    let page = NodeId::Today(NaiveDate::from_ymd_opt(2026, 11, 22).unwrap());
    let tag = NodeId::Builtin(Builtin::Book);
    let first = create(stamp(1, A), &[], page, None);
    let second = create(stamp(1, B), &[], page, None);
    let events = [
        first.clone(),
        second.clone(),
        text(stamp(2, A), &[first.id], page, None, "One"),
        text(stamp(2, B), &[second.id], page, None, "Two"),
        create(stamp(3, A), &[stamp(2, A)], tag, None),
        create(stamp(3, B), &[stamp(2, B)], tag, None),
        edit(
            stamp(4, A),
            &[stamp(3, A)],
            Change::SetTag {
                node: page,
                tag,
                present: true,
            },
        ),
        edit(
            stamp(4, B),
            &[stamp(3, B)],
            Change::SetTag {
                node: page,
                tag,
                present: false,
            },
        ),
    ];
    let mut model = replay([events[3].clone()]);
    assert_eq!(
        model
            .snapshot()
            .pending()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [stamp(2, B)]
    );
    for event in &events {
        model.insert(event.clone()).unwrap();
    }
    let view = assert_all_orders(&events);
    assert_eq!(model.snapshot(), view);
    assert_eq!(view.children(None), [page, tag]);
    assert_eq!(view.body(page), Some("OneTwo".into()));
    assert!(view.has_tag(page, tag));

    assert_eq!(
        model.insert(create(stamp(1, C), &[first.id], tag, None)),
        Err(ModelError::InvalidDependency),
    );
    assert_eq!(
        model.insert(create(first.id, &[], tag, None)),
        Err(ModelError::ReusedId),
    );
    assert_eq!(model.snapshot(), view);
}

#[test]
fn undoing_origins_preserves_other_peers_work_and_moves_cannot_form_cycles() {
    let first = note(1, A);
    let second = note(1, B);
    let insertion = stamp(2, A);
    let mut events = vec![
        create(stamp(1, A), &[], first, None),
        create(stamp(1, B), &[], second, None),
        text(insertion, &[stamp(1, A)], first, None, "Initial"),
        edit(
            stamp(2, B),
            &[stamp(1, A), stamp(1, B)],
            Change::Move {
                node: first,
                place: Place {
                    parent: Some(second),
                    after: None,
                },
            },
        ),
        edit(
            stamp(2, C),
            &[stamp(1, A), stamp(1, B)],
            Change::Move {
                node: second,
                place: Place {
                    parent: Some(first),
                    after: None,
                },
            },
        ),
        edit(
            stamp(3, B),
            &[insertion],
            Change::EditText {
                token: insertion,
                text: "A peer's edit".into(),
            },
        ),
    ];
    let view = assert_all_orders(&events);
    assert_eq!(view.children(None), [second]);
    assert_eq!(view.children(Some(second)), [first]);
    assert_eq!(view.body(first), Some("A peer's edit".into()));

    events.push(edit(
        stamp(4, A),
        &[insertion],
        Change::Undo { target: insertion },
    ));
    let undone = assert_all_orders(&events);
    assert_eq!(undone.body(first), Some("".into()));
    assert_eq!(undone.recoverable_text(insertion), Some("A peer's edit"));
}
