//! Experimental author-entered facts, ordered events and plot threads.
//! Definite conflicts require an explicit single-value context; no prose
//! inference, character-knowledge service or editor UI is implemented.
use crate::statefile::{Store, Validate};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactCategory {
    Character,
    Location,
    Object,
    Event,
    Lore,
}

impl FactCategory {
    pub const ALL: [Self; 5] = [
        Self::Character,
        Self::Location,
        Self::Object,
        Self::Event,
        Self::Lore,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Location => "Location",
            Self::Object => "Object",
            Self::Event => "Event",
            Self::Lore => "Lore & Rules",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Certainty {
    Definite,
    Stated,
    Rumor,
    Retconned,
}

impl Certainty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Definite => "Definite (Canon)",
            Self::Stated => "Stated in dialogue",
            Self::Rumor => "Rumor / Unconfirmed",
            Self::Retconned => "Retconned / Obsolete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub id: u64,
    pub category: FactCategory,
    pub subject: String,
    pub property: String,
    pub value: String,
    pub scene_id: Option<usize>,
    pub scene_name: Option<String>,
    pub certainty: Certainty,
    /// Explicit author assertion that this property has one value in this
    /// context. Unscoped/changing facts can only suggest an inconsistency.
    pub exclusive_scope: Option<String>,
    pub notes: String,
}

impl Fact {
    pub fn new(
        category: FactCategory,
        subject: impl Into<String>,
        property: impl Into<String>,
        value: impl Into<String>,
        certainty: Certainty,
    ) -> Self {
        Self {
            id: 0,
            category,
            subject: subject.into(),
            property: property.into(),
            value: value.into(),
            scene_id: None,
            scene_name: None,
            certainty,
            exclusive_scope: None,
            notes: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    pub id: u64,
    pub order: i32,
    pub title: String,
    pub story_date: String,
    pub scene_id: Option<usize>,
    pub participants: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadStatus {
    Open,
    Resolved,
    Abandoned,
}

impl ThreadStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Resolved => "Resolved",
            Self::Abandoned => "Abandoned",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotThread {
    pub id: u64,
    pub title: String,
    pub status: ThreadStatus,
    pub introduced_in: Option<String>,
    pub resolved_in: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionSeverity {
    DefiniteConflict,
    SuggestedInconsistency,
}

impl ContradictionSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::DefiniteConflict => "Definite Conflict",
            Self::SuggestedInconsistency => "Suggested Inconsistency",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contradiction {
    pub severity: ContradictionSeverity,
    pub subject: String,
    pub property: String,
    pub fact_a_id: u64,
    pub fact_b_id: u64,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuityStore {
    pub schema_version: u32,
    pub next_id: u64,
    pub facts: Vec<Fact>,
    pub events: Vec<TimelineEvent>,
    pub threads: Vec<PlotThread>,
}

impl Default for ContinuityStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_id: 0,
            facts: Vec::new(),
            events: Vec::new(),
            threads: Vec::new(),
        }
    }
}
impl ContinuityStore {
    pub fn path(book_root: &Path) -> PathBuf {
        book_root.join(".grimoire/writingdeck/continuity.json")
    }

    pub fn load(book_root: &Path) -> Result<Store<Self>> {
        crate::story_state::load(book_root, "continuity", Self::default())
    }

    pub fn add_fact(&mut self, mut fact: Fact) -> Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        fact.id = self.next_id;
        self.facts.push(fact);
        Ok(self.next_id)
    }

    pub fn remove_fact(&mut self, id: u64) -> bool {
        let len_before = self.facts.len();
        self.facts.retain(|f| f.id != id);
        self.facts.len() < len_before
    }

    pub fn add_event(
        &mut self,
        order: i32,
        title: impl Into<String>,
        story_date: impl Into<String>,
        scene_id: Option<usize>,
        participants: Vec<String>,
        description: impl Into<String>,
    ) -> Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        self.events.push(TimelineEvent {
            id,
            order,
            title: title.into().trim().to_string(),
            story_date: story_date.into().trim().to_string(),
            scene_id,
            participants,
            description: description.into().trim().to_string(),
        });
        self.events.sort_by_key(|e| e.order);
        Ok(id)
    }

    pub fn remove_event(&mut self, id: u64) -> bool {
        let len_before = self.events.len();
        self.events.retain(|e| e.id != id);
        self.events.len() < len_before
    }

    pub fn add_thread(
        &mut self,
        title: impl Into<String>,
        status: ThreadStatus,
        introduced_in: Option<String>,
        resolved_in: Option<String>,
        notes: impl Into<String>,
    ) -> Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("story identity exhausted"))?;
        let id = self.next_id;
        self.threads.push(PlotThread {
            id,
            title: title.into().trim().to_string(),
            status,
            introduced_in,
            resolved_in,
            notes: notes.into().trim().to_string(),
        });
        Ok(id)
    }

    pub fn remove_thread(&mut self, id: u64) -> bool {
        let len_before = self.threads.len();
        self.threads.retain(|t| t.id != id);
        self.threads.len() < len_before
    }

    /// Check for explicit contradictions across facts without hallucinating issues.
    pub fn check_contradictions(&self) -> Result<Vec<Contradiction>> {
        self.validate()?;
        let mut contradictions = Vec::new();

        // Group facts by subject (lowercased) and property (lowercased)
        let mut groups: BTreeMap<(u8, String, String), Vec<&Fact>> = BTreeMap::new();
        for fact in &self.facts {
            if fact.certainty == Certainty::Retconned {
                continue;
            }
            let key = (
                fact.category as u8,
                fact.subject.trim().to_lowercase(),
                fact.property.trim().to_lowercase(),
            );
            groups.entry(key).or_default().push(fact);
        }

        for ((_category, _subj_key, _prop_key), facts) in groups {
            if facts.len() < 2 {
                continue;
            }

            for i in 0..facts.len() {
                for j in (i + 1)..facts.len() {
                    let a = facts[i];
                    let b = facts[j];
                    let val_a = a.value.trim().to_lowercase();
                    let val_b = b.value.trim().to_lowercase();

                    if a.exclusive_scope.is_some()
                        && b.exclusive_scope.is_some()
                        && a.exclusive_scope != b.exclusive_scope
                    {
                        continue;
                    }
                    if val_a != val_b {
                        ensure!(
                            contradictions.len() < 512,
                            "continuity review exceeds limit; no partial report accepted"
                        );
                        let severity = if a.certainty == Certainty::Definite
                            && b.certainty == Certainty::Definite
                            && a.exclusive_scope.is_some()
                            && a.exclusive_scope == b.exclusive_scope
                        {
                            ContradictionSeverity::DefiniteConflict
                        } else {
                            ContradictionSeverity::SuggestedInconsistency
                        };

                        let explanation = format!(
                            "Subject '{}' has conflicting values for '{}': '{}' ({}) vs '{}' ({})",
                            a.subject,
                            a.property,
                            a.value,
                            a.certainty.label(),
                            b.value,
                            b.certainty.label()
                        );

                        contradictions.push(Contradiction {
                            severity,
                            subject: a.subject.clone(),
                            property: a.property.clone(),
                            fact_a_id: a.id,
                            fact_b_id: b.id,
                            explanation,
                        });
                    }
                }
            }
        }

        contradictions
            .sort_by_key(|c| (c.fact_a_id.min(c.fact_b_id), c.fact_a_id.max(c.fact_b_id)));
        Ok(contradictions)
    }
}

impl Validate for ContinuityStore {
    fn validate(&self) -> Result<()> {
        crate::story_state::identities(
            self.schema_version,
            self.next_id,
            self.facts
                .iter()
                .map(|f| f.id)
                .chain(self.events.iter().map(|e| e.id))
                .chain(self.threads.iter().map(|t| t.id)),
        )?;
        ensure!(self.facts.len() <= 512, "continuity fact limit reached");
        for f in &self.facts {
            ensure!(
                !f.subject.trim().is_empty()
                    && !f.property.trim().is_empty()
                    && !f.value.trim().is_empty(),
                "continuity fact must name its subject, property and value"
            );
            for text in [&f.subject, &f.property, &f.value, &f.notes] {
                crate::story_state::text(text, 16_384)?;
            }
            if let Some(scope) = &f.exclusive_scope {
                ensure!(!scope.trim().is_empty(), "continuity scope cannot be empty");
                crate::story_state::text(scope, 256)?;
            }
            if let Some(name) = &f.scene_name {
                crate::story_state::text(name, 4096)?;
            }
        }
        for e in &self.events {
            ensure!(!e.title.trim().is_empty(), "timeline event needs a title");
            for text in [&e.title, &e.story_date, &e.description] {
                crate::story_state::text(text, 16_384)?;
            }
            ensure!(
                e.participants.len() <= 512,
                "too many timeline participants"
            );
            for text in &e.participants {
                crate::story_state::text(text, 4096)?;
            }
        }
        for t in &self.threads {
            ensure!(!t.title.trim().is_empty(), "plot thread needs a title");
            for text in [&t.title, &t.notes] {
                crate::story_state::text(text, 16_384)?;
            }
            for text in [&t.introduced_in, &t.resolved_in].into_iter().flatten() {
                crate::story_state::text(text, 4096)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fact(value: &str, certainty: Certainty, scope: Option<&str>) -> Fact {
        let mut f = Fact::new(
            FactCategory::Character,
            "Arthur",
            "eye color",
            value,
            certainty,
        );
        f.exclusive_scope = scope.map(String::from);
        f
    }
    #[test]
    fn definite_conflicts_require_an_explicit_single_value_scope() {
        let mut store = ContinuityStore::default();
        store
            .add_fact(fact(
                "blue",
                Certainty::Definite,
                Some("canon, no disguise"),
            ))
            .unwrap();
        store
            .add_fact(fact(
                "brown",
                Certainty::Definite,
                Some("canon, no disguise"),
            ))
            .unwrap();
        let conflicts = store.check_contradictions().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].severity,
            ContradictionSeverity::DefiniteConflict
        );
    }
    #[test]
    fn changing_or_unconfirmed_facts_only_suggest_review() {
        for certainty in [Certainty::Definite, Certainty::Rumor] {
            let mut store = ContinuityStore::default();
            store
                .add_fact(fact("blue", Certainty::Definite, None))
                .unwrap();
            store.add_fact(fact("brown", certainty, None)).unwrap();
            assert_eq!(
                store.check_contradictions().unwrap()[0].severity,
                ContradictionSeverity::SuggestedInconsistency
            );
        }
    }
    #[test]
    fn distinct_categories_scopes_and_retconned_facts_are_not_conflicts() {
        let mut store = ContinuityStore::default();
        store
            .add_fact(fact("blue", Certainty::Definite, Some("youth")))
            .unwrap();
        store
            .add_fact(fact("brown", Certainty::Definite, Some("adult disguise")))
            .unwrap();
        store
            .add_fact(fact("red", Certainty::Retconned, Some("youth")))
            .unwrap();
        let mut place = fact("green", Certainty::Definite, Some("youth"));
        place.category = FactCategory::Location;
        store.add_fact(place).unwrap();
        assert!(store.check_contradictions().unwrap().is_empty());
    }
}
