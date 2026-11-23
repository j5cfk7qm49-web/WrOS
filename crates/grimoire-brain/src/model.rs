//! Independent reference model for the Brain storage tests.
//!
//! Rebuild the view from the operation graph. This is a correctness oracle for
//! the storage adapter, so it favors explicit replay over incremental updates.

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use chrono::NaiveDate;

/// A Lamport clock followed by a stable device ID for deterministic ties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpId {
    pub clock: u64,
    pub device: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Builtin {
    Book,
    Article,
    Chapter,
    Lecture,
    Audio,
    Web,
    Concept,
    Person,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeId {
    Note(OpId),
    Today(NaiveDate),
    Builtin(Builtin),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldId {
    Custom(OpId),
    Builtin { tag: Builtin, slot: u16 },
}

/// A missing or stale sibling anchor appends to the destination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub parent: Option<NodeId>,
    pub after: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub id: OpId,
    pub dependencies: BTreeSet<OpId>,
    pub change: Change,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Create {
        node: NodeId,
        place: Place,
    },
    Move {
        node: NodeId,
        place: Place,
    },
    /// The edit ID identifies the inserted text token.
    InsertText {
        node: NodeId,
        after: Option<OpId>,
        text: String,
    },
    EditText {
        token: OpId,
        text: String,
    },
    /// Move the boundary token and everything after it into a new sibling.
    Split {
        at: OpId,
        right: NodeId,
    },
    Join {
        left: NodeId,
        right: NodeId,
    },
    SetField {
        node: NodeId,
        field: FieldId,
        value: Option<String>,
    },
    SetTag {
        node: NodeId,
        tag: NodeId,
        present: bool,
    },
    /// Freeze the visible descendants in the deleting device's view.
    Delete {
        members: BTreeSet<NodeId>,
    },
    Restore {
        deletion: OpId,
    },
    Undo {
        target: OpId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    ReusedId,
    InvalidDependency,
    UnobservedTarget,
    ForeignUndo,
}

#[derive(Debug, Default)]
pub struct Model {
    edits: BTreeMap<OpId, Edit>,
}

impl Model {
    pub fn insert(&mut self, edit: Edit) -> Result<(), ModelError> {
        if edit
            .dependencies
            .iter()
            .any(|dependency| dependency.clock >= edit.id.clock)
        {
            return Err(ModelError::InvalidDependency);
        }
        let target = match &edit.change {
            Change::Undo { target } => {
                if edit.id.device != target.device {
                    return Err(ModelError::ForeignUndo);
                }
                Some(target)
            }
            Change::Restore { deletion } => Some(deletion),
            _ => None,
        };
        if target.is_some_and(|target| !edit.dependencies.contains(target)) {
            return Err(ModelError::UnobservedTarget);
        }
        match self.edits.entry(edit.id) {
            Entry::Vacant(slot) => {
                slot.insert(edit);
            }
            Entry::Occupied(slot) if *slot.get() != edit => return Err(ModelError::ReusedId),
            Entry::Occupied(_) => {}
        }
        Ok(())
    }

    fn depends_on(&self, edit: OpId, dependency: OpId) -> bool {
        let mut visit = Vec::from([edit]);
        let mut seen = BTreeSet::new();
        while let Some(id) = visit.pop() {
            if id <= dependency || !seen.insert(id) {
                continue;
            }
            if let Some(edit) = self.edits.get(&id) {
                if edit.dependencies.contains(&dependency) {
                    return true;
                }
                visit.extend(edit.dependencies.iter().copied());
            }
        }
        false
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut ready = BTreeSet::new();
        let mut view = Snapshot::default();
        for (&id, edit) in &self.edits {
            if edit.dependencies.is_subset(&ready) {
                ready.insert(id);
            } else {
                view.pending.insert(id);
            }
        }
        // Work backwards so undoing an Undo restores its original action.
        let mut undone = BTreeSet::new();
        let mut restored = BTreeSet::new();
        for &id in ready.iter().rev() {
            if undone.contains(&id) {
                continue;
            }
            match self.edits[&id].change {
                Change::Undo { target } => {
                    undone.insert(target);
                }
                Change::Restore { deletion } => {
                    restored.insert(deletion);
                }
                _ => {}
            }
        }
        let deletions: Vec<_> = ready
            .iter()
            .filter(|id| !undone.contains(id) && !restored.contains(id))
            .filter_map(|id| match &self.edits[id].change {
                Change::Delete { members } => Some((*id, members)),
                _ => None,
            })
            .collect();
        view.hidden.extend(
            deletions
                .iter()
                .flat_map(|(_, members)| members.iter().copied()),
        );

        for id in ready {
            let edit = &self.edits[&id];
            let active = !undone.contains(&id);
            // A split or join can move text out of a deleted note. Keep that
            // text in the hidden note unless the delete already saw the move.
            let blocked = |node| {
                deletions.iter().any(|(deletion, members)| {
                    members.contains(&node) && !self.depends_on(*deletion, id)
                })
            };
            match &edit.change {
                Change::Create { node, place } => view.create(*node, *place, active),
                Change::InsertText { node, after, text } => {
                    view.insert_text(id, *node, *after, text.clone(), active);
                }
                Change::Split { right, .. } if !active => {
                    view.nodes.entry(*right).or_default();
                }
                _ if !active => {}
                Change::Move { node, place } => {
                    view.move_node(*node, *place);
                }
                Change::Split { at, right } => {
                    if let Some(owner) = view.tokens.get(at).map(|token| token.owner)
                        && !blocked(owner)
                    {
                        view.split(owner, *at, *right);
                    } else {
                        view.nodes.entry(*right).or_default();
                    }
                }
                Change::Join { left, right } => {
                    let owner = view.resolve(*left);
                    if !blocked(owner) && !blocked(*right) {
                        view.join(owner, *right);
                    }
                }
                Change::EditText { token, text } => {
                    if let Some(token) = view.tokens.get_mut(token) {
                        token.text.assign(self, id, text.clone());
                    }
                }
                Change::SetField { node, field, value } => {
                    view.nodes
                        .entry(*node)
                        .or_default()
                        .fields
                        .entry(*field)
                        .or_default()
                        .assign(self, id, value.clone());
                }
                Change::SetTag { node, tag, present } => {
                    view.nodes
                        .entry(*node)
                        .or_default()
                        .tags
                        .entry(*tag)
                        .or_default()
                        .assign(self, id, *present);
                }
                _ => {}
            }
        }
        view
    }
}

/// All causally maximal values, ordered by operation ID. The last value wins
/// display ties. A later edit that observed both alternatives supersedes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Values<T>(Vec<(OpId, T)>);

impl<T> Values<T> {
    fn assign(&mut self, model: &Model, id: OpId, value: T) {
        self.0
            .retain(|(previous, _)| !model.depends_on(id, *previous));
        self.0.push((id, value));
    }

    fn winner(&self) -> Option<&T> {
        self.0.last().map(|(_, value)| value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Note {
    created: bool,
    parent: Option<NodeId>,
    text: Vec<OpId>,
    fields: BTreeMap<FieldId, Values<Option<String>>>,
    tags: BTreeMap<NodeId, Values<bool>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    owner: NodeId,
    present: bool,
    text: Values<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    nodes: BTreeMap<NodeId, Note>,
    order: BTreeMap<Option<NodeId>, Vec<NodeId>>,
    tokens: BTreeMap<OpId, Token>,
    hidden: BTreeSet<NodeId>,
    aliases: BTreeMap<NodeId, NodeId>,
    pending: BTreeSet<OpId>,
}

impl Snapshot {
    pub fn pending(&self) -> &BTreeSet<OpId> {
        &self.pending
    }

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.get(&node)?.parent
    }

    pub fn children(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        self.order
            .get(&parent)
            .into_iter()
            .flatten()
            .copied()
            .filter(|node| self.is_visible(*node))
            .collect()
    }

    pub fn is_visible(&self, mut node: NodeId) -> bool {
        loop {
            let Some(note) = self.nodes.get(&node) else {
                return false;
            };
            if !note.created || self.hidden.contains(&node) || self.aliases.contains_key(&node) {
                return false;
            }
            let Some(parent) = note.parent else {
                return true;
            };
            node = parent;
        }
    }

    /// Includes the retained body of a deleted note. Use is_visible for display.
    pub fn body(&self, node: NodeId) -> Option<String> {
        let note = self.nodes.get(&node)?;
        let text = note
            .text
            .iter()
            .map(|id| &self.tokens[id])
            .filter(|token| token.present)
            .filter_map(|token| token.text.winner().map(String::as_str))
            .collect();
        Some(text)
    }

    pub fn recoverable_text(&self, token: OpId) -> Option<&str> {
        self.tokens.get(&token)?.text.winner().map(String::as_str)
    }

    pub fn field(&self, node: NodeId, field: FieldId) -> Option<&str> {
        self.nodes
            .get(&node)?
            .fields
            .get(&field)?
            .winner()?
            .as_deref()
    }

    pub fn field_alternatives(&self, node: NodeId, field: FieldId) -> Vec<Option<&str>> {
        let Some(values) = self
            .nodes
            .get(&node)
            .and_then(|note| note.fields.get(&field))
        else {
            return Vec::new();
        };
        let Some((_, alternatives)) = values.0.split_last() else {
            return Vec::new();
        };
        alternatives
            .iter()
            .map(|(_, value)| value.as_deref())
            .collect()
    }

    /// Keep a concurrent tag addition. A removal that saw it supersedes it.
    pub fn has_tag(&self, node: NodeId, tag: NodeId) -> bool {
        self.nodes
            .get(&node)
            .and_then(|note| note.tags.get(&tag))
            .is_some_and(|values| values.0.iter().any(|(_, present)| *present))
    }

    pub fn link_target(&self, node: NodeId) -> Option<NodeId> {
        let node = self.resolve(node);
        self.is_visible(node).then_some(node)
    }

    pub fn delete_subtree(&self, root: NodeId) -> Option<Change> {
        if !self.is_visible(root) {
            return None;
        }
        let mut members = BTreeSet::new();
        let mut visit = vec![root];
        while let Some(node) = visit.pop() {
            members.insert(node);
            visit.extend(self.children(Some(node)));
        }
        Some(Change::Delete { members })
    }

    fn resolve(&self, mut node: NodeId) -> NodeId {
        while let Some(next) = self.aliases.get(&node) {
            node = *next;
        }
        node
    }

    fn create(&mut self, node: NodeId, place: Place, active: bool) {
        let note = self.nodes.entry(node).or_default();
        if !active || note.created {
            return;
        }
        note.created = true;
        if !self.move_node(node, place) {
            self.move_node(node, Place::default());
        }
    }

    fn move_node(&mut self, node: NodeId, place: Place) -> bool {
        if !self.nodes.contains_key(&node) || place.after == Some(node) {
            return false;
        }
        let parent = place.parent.map(|id| self.resolve(id));
        let mut ancestor = parent;
        while let Some(id) = ancestor {
            if id == node || !self.nodes.contains_key(&id) {
                return false;
            }
            ancestor = self.parent(id);
        }
        let note = self.nodes.get_mut(&node).unwrap();
        self.order
            .entry(note.parent)
            .or_default()
            .retain(|id| *id != node);
        note.parent = parent;
        let siblings = self.order.entry(parent).or_default();
        siblings.insert(after(siblings, place.after), node);
        true
    }

    fn insert_text(
        &mut self,
        id: OpId,
        note: NodeId,
        anchor: Option<OpId>,
        text: String,
        active: bool,
    ) {
        let owner = anchor
            .and_then(|anchor| self.tokens.get(&anchor).map(|token| token.owner))
            .unwrap_or_else(|| self.resolve(note));
        let list = &mut self.nodes.entry(owner).or_default().text;
        list.insert(after(list, anchor), id);
        self.tokens.insert(
            id,
            Token {
                owner,
                present: active,
                text: Values(vec![(id, text)]),
            },
        );
    }

    fn split(&mut self, left: NodeId, at: OpId, right: NodeId) {
        if right == left || self.nodes.get(&right).is_some_and(|note| note.created) {
            return;
        }
        let parent = self.parent(left);
        self.create(
            right,
            Place {
                parent,
                after: Some(left),
            },
            true,
        );
        let tokens = &mut self.nodes.get_mut(&left).unwrap().text;
        let boundary = tokens.iter().position(|id| *id == at).unwrap();
        let suffix = tokens.split_off(boundary);
        self.transfer_text(right, suffix);
    }

    fn join(&mut self, left: NodeId, right: NodeId) {
        if left == right
            || self.aliases.contains_key(&right)
            || !self.nodes.get(&left).is_some_and(|note| note.created)
            || !self.nodes.get(&right).is_some_and(|note| note.created)
            || self.parent(left) != self.parent(right)
        {
            return;
        }
        let suffix = std::mem::take(&mut self.nodes.get_mut(&right).unwrap().text);
        self.transfer_text(left, suffix);
        for child in self.order.get(&Some(right)).cloned().unwrap_or_default() {
            self.move_node(
                child,
                Place {
                    parent: Some(left),
                    after: None,
                },
            );
        }
        self.aliases.insert(right, left);
    }

    fn transfer_text(&mut self, owner: NodeId, tokens: Vec<OpId>) {
        for id in &tokens {
            self.tokens.get_mut(id).unwrap().owner = owner;
        }
        self.nodes.get_mut(&owner).unwrap().text.extend(tokens);
    }
}

fn after<T: PartialEq>(items: &[T], anchor: Option<T>) -> usize {
    anchor
        .as_ref()
        .and_then(|a| items.iter().position(|item| item == a))
        .map_or(items.len(), |position| position + 1)
}
