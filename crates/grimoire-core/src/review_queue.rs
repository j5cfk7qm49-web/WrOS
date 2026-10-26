//! Stable, source-bound review receipts and a ranked advisory queue. Priority
//! is a workflow order, not literary quality. Equivalent findings retain all
//! engine/rule provenance; unrelated overlapping findings are never discarded.
use crate::{
    diagnostics::{self, ByteSpan, Category, Diagnostic, RuleKey, SourceRole, SourceUnit},
    statefile::{Store, Validate},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};
pub const RELATIVE: &str = ".grimoire/writingdeck/review.json";
pub const MAX_ITEMS: usize = 512;
pub const MAX_RECEIPTS: usize = 512;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    Grammar,
    Spelling,
    Usage,
    Readability,
    Style,
    Information,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Reviewed,
    Dismissed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub document: String,
    pub unit: SourceUnit,
    pub role: SourceRole,
    pub source_sha256: String,
    pub span: ByteSpan,
    pub provenance: Vec<RuleKey>,
    pub state: State,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub schema_version: u32,
    pub receipts: Vec<Receipt>,
}
impl Default for Review {
    fn default() -> Self {
        Self {
            schema_version: 1,
            receipts: vec![],
        }
    }
}
impl Validate for Review {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unknown/future review schema; original preserved"
        );
        ensure!(
            self.receipts.len() <= MAX_RECEIPTS,
            "review receipt limit reached; originals retained"
        );
        let mut ids = BTreeSet::new();
        for receipt in &self.receipts {
            ensure!(
                is_hash(&receipt.id) && is_hash(&receipt.source_sha256) && ids.insert(&receipt.id),
                "invalid/duplicate review identity"
            );
            ensure!(
                !receipt.document.is_empty()
                    && receipt.document.len() <= 4096
                    && !receipt.document.chars().any(char::is_control)
                    && Path::new(&receipt.document)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "review document must be project-relative"
            );
            ensure!(
                receipt.span.start < receipt.span.end
                    && receipt.span.end <= diagnostics::MAX_SOURCE_BYTES,
                "invalid review source bounds"
            );
            ensure!(
                !receipt.provenance.is_empty() && receipt.provenance.len() <= 16,
                "invalid review provenance"
            );
            for rule in &receipt.provenance {
                rule.validate()?;
            }
            ensure!(
                receipt.recorded_at.len() <= 64
                    && chrono::DateTime::parse_from_rfc3339(&receipt.recorded_at).is_ok(),
                "invalid review date"
            );
        }
        Ok(())
    }
}
pub fn load(root: &Path) -> Result<Store<Review>> {
    Store::load(root, Path::new(RELATIVE), Review::default())
}
#[derive(Debug, Clone)]
pub struct Item {
    pub finding: Diagnostic,
    pub provenance: Vec<RuleKey>,
    pub priority: Priority,
    pub id: String,
    pub state: Option<State>,
}
#[derive(Debug, Default)]
pub struct Queue {
    pub items: Vec<Item>,
    pub omitted: usize,
}
impl Queue {
    pub fn build(findings: impl IntoIterator<Item = Diagnostic>, review: &Review) -> Result<Self> {
        review.validate()?;
        let mut queue = Self::default();
        for (count, finding) in findings.into_iter().enumerate() {
            ensure!(
                count < 4096,
                "too many candidate findings; no partial queue called complete"
            );
            finding.rule.validate()?;
            let priority = priority(&finding);
            if let Some(item) = queue
                .items
                .iter_mut()
                .find(|item| equivalent(&item.finding, &finding))
            {
                if !item.provenance.contains(&finding.rule) {
                    ensure!(
                        item.provenance.len() < 16,
                        "too many equivalent-engine sources"
                    );
                    item.provenance.push(finding.rule.clone());
                }
                if precedence(&finding) < precedence(&item.finding) {
                    item.finding = finding;
                }
                item.priority = item.priority.min(priority);
            } else {
                queue.items.push(Item {
                    provenance: vec![finding.rule.clone()],
                    finding,
                    priority,
                    id: String::new(),
                    state: None,
                });
            }
        }
        for item in &mut queue.items {
            item.provenance.sort();
            item.id = stable_id(item);
            item.state = review
                .receipts
                .iter()
                .find(|r| r.id == item.id)
                .map(|r| r.state);
        }
        queue.items.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.finding.source.document.cmp(&b.finding.source.document))
                .then_with(|| a.finding.span.start.cmp(&b.finding.span.start))
                .then_with(|| a.id.cmp(&b.id))
        });
        queue.omitted = queue.items.len().saturating_sub(MAX_ITEMS);
        queue.items.truncate(MAX_ITEMS);
        Ok(queue)
    }
}
pub fn record(
    store: &mut Store<Review>,
    item: &Item,
    state: State,
    recorded_at: &str,
) -> Result<()> {
    let receipt = Receipt {
        id: item.id.clone(),
        document: item.finding.source.document.clone(),
        unit: item.finding.source.unit,
        role: item.finding.source.role,
        source_sha256: item.finding.source.sha256.clone(),
        span: item.finding.span,
        provenance: item.provenance.clone(),
        state,
        recorded_at: recorded_at.into(),
    };
    ensure!(
        stable_id(item) == receipt.id,
        "review item identity changed"
    );
    store.update(|review| {
        if let Some(old) = review.receipts.iter_mut().find(|r| r.id == receipt.id) {
            *old = receipt;
        } else {
            ensure!(
                review.receipts.len() < MAX_RECEIPTS,
                "512 review receipts retained; explicitly archive before adding more"
            );
            review.receipts.push(receipt);
        }
        Ok(())
    })
}
fn priority(finding: &Diagnostic) -> Priority {
    match finding.category {
        Category::Grammar | Category::Punctuation | Category::Capitalization => Priority::Grammar,
        Category::Usage if finding.rule.engine == "spellbook" => Priority::Spelling,
        Category::Usage => Priority::Usage,
        Category::Readability => Priority::Readability,
        Category::Style => Priority::Style,
        Category::Other => Priority::Information,
    }
}
fn precedence(finding: &Diagnostic) -> u8 {
    match finding.rule.engine.as_str() {
        "harper-core"
            if matches!(
                finding.category,
                Category::Grammar
                    | Category::Usage
                    | Category::Punctuation
                    | Category::Capitalization
            ) =>
        {
            0
        }
        "nabokov" if finding.rule.rule == "NB302" => 0,
        "proselint" => 1,
        "spellbook" => 1,
        "nabokov" => 2,
        _ => 3,
    }
}
fn family(finding: &Diagnostic) -> &str {
    match (finding.rule.engine.as_str(), finding.rule.rule.as_str()) {
        ("writingdeck-native", "wordy-phrase") | ("nabokov", "NB401") => "wordiness",
        ("writingdeck-native", "long-segment") | ("nabokov", "NB201" | "NB202") => {
            "sentence-pattern"
        }
        ("writingdeck-native", "adjacent-echo") | ("harper-core", "RepeatedWords") => "echo",
        ("nabokov", "NB302") => "passive",
        _ => "unclassified",
    }
}
fn equivalent(a: &Diagnostic, b: &Diagnostic) -> bool {
    if a.source != b.source || a.configuration != b.configuration {
        return false;
    }
    if a.span == b.span && a.rule == b.rule {
        return true;
    }
    let af = family(a);
    let bf = family(b);
    if af != "unclassified" && af == bf {
        if a.span == b.span {
            return true;
        }
        if af == "sentence-pattern" {
            let overlap = a
                .span
                .end
                .min(b.span.end)
                .saturating_sub(a.span.start.max(b.span.start));
            let longest = (a.span.end - a.span.start).max(b.span.end - b.span.start);
            return overlap.saturating_mul(100) >= longest.saturating_mul(95);
        }
    }
    // Sharing a location alone is NOT equivalence. A common concrete literal
    // alternative of the same category is additional evidence; directions and
    // unsafe alternatives are not used to throw another issue away.
    a.span == b.span
        && a.category == b.category
        && a.replacements.iter().any(|x| {
            x.source_safe
                && b.replacements
                    .iter()
                    .any(|y| y.source_safe && x.text == y.text)
        })
}
fn stable_id(item: &Item) -> String {
    // Generation excluded deliberately: a new run over identical bytes/rules
    // may retain an explicit review. Changed source/config/version cannot.
    diagnostics::hex_digest(serde_json::to_string(&serde_json::json!({"document":item.finding.source.document,"unit":item.finding.source.unit,"role":item.finding.source.role,"source":item.finding.source.sha256,"configuration":item.finding.configuration,"span":item.finding.span,"rules":item.provenance})).expect("serializable review identity").as_bytes())
}
fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{AnalysisOptions, Profile, Snapshot};
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    fn root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "wd-review-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn finding(
        engine: &str,
        rule: &str,
        category: Category,
        alternative: Option<&str>,
        generation: u64,
    ) -> Diagnostic {
        let source = Snapshot::new(
            Path::new("scene.md"),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            generation,
            "Café 🦊. in order to see.",
        )
        .unwrap();
        let options = AnalysisOptions {
            profile: Profile::Revision,
            ..Default::default()
        };
        let from = source.text().find("in order to").unwrap();
        Diagnostic::new(
            &source.project(&options),
            &options,
            RuleKey {
                engine: engine.into(),
                version: "contract-1".into(),
                rule: rule.into(),
            },
            category,
            (from..from + 11).into(),
            "Injected queue contract, not a real engine result".into(),
            alternative.map(|s| vec![s.into()]).unwrap_or_default(),
        )
        .unwrap()
    }
    #[test]
    fn equivalent_wordiness_keeps_provenance_but_not_unrelated_overlapping_concerns() {
        let a = finding(
            "writingdeck-native",
            "wordy-phrase",
            Category::Style,
            Some("to"),
            1,
        );
        let b = finding("nabokov", "NB401", Category::Style, Some("to"), 1);
        let unrelated = finding("proselint", "misc.something_else", Category::Usage, None, 1);
        let queue = Queue::build([a, b, unrelated], &Review::default()).unwrap();
        assert_eq!(queue.items.len(), 2);
        assert_eq!(queue.items[0].priority, Priority::Usage);
        let wordy = &queue.items[1];
        assert_eq!(wordy.provenance.len(), 2);
        assert_eq!(wordy.finding.rule.engine, "nabokov");
    }
    #[test]
    fn grammar_precedence_is_workflow_priority_not_a_quality_score() {
        let a = finding("proselint", "misc.rule", Category::Usage, Some("to"), 1);
        let b = finding("harper-core", "GrammarRule", Category::Usage, Some("to"), 1);
        let q = Queue::build([a, b], &Review::default()).unwrap();
        assert_eq!(q.items.len(), 1);
        assert_eq!(q.items[0].finding.rule.engine, "harper-core");
        assert_eq!(q.items[0].provenance.len(), 2);
    }
    #[test]
    fn receipts_are_stable_across_rerun_but_not_changed_bytes_config_or_rule_versions() {
        let root = root();
        let mut store = load(&root).unwrap();
        let item = Queue::build(
            [finding("proselint", "misc.rule", Category::Usage, None, 1)],
            &Review::default(),
        )
        .unwrap()
        .items
        .remove(0);
        record(&mut store, &item, State::Reviewed, "2026-10-26T12:00:00Z").unwrap();
        record(&mut store, &item, State::Reviewed, "2026-10-26T12:00:00Z").unwrap();
        assert_eq!(store.value.receipts.len(), 1);
        let rerun = Queue::build(
            [finding("proselint", "misc.rule", Category::Usage, None, 2)],
            &store.value,
        )
        .unwrap();
        assert_eq!(rerun.items[0].state, Some(State::Reviewed));
        let mut changed = finding("proselint", "misc.rule", Category::Usage, None, 3);
        changed.source.sha256 = diagnostics::hex_digest(b"changed");
        assert_eq!(
            Queue::build([changed], &store.value).unwrap().items[0].state,
            None
        );
        let mut changed = finding("proselint", "misc.rule", Category::Usage, None, 3);
        changed.configuration = diagnostics::hex_digest(b"changed config");
        assert_eq!(
            Queue::build([changed], &store.value).unwrap().items[0].state,
            None
        );
        let text = fs::read_to_string(root.join(RELATIVE)).unwrap();
        assert!(!text.contains("Café") && !text.contains("in order"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn future_corrupt_and_external_metadata_is_never_overwritten_by_review() {
        let root = root();
        let mut store = load(&root).unwrap();
        let item = Queue::build(
            [finding("proselint", "misc.rule", Category::Usage, None, 1)],
            &Review::default(),
        )
        .unwrap()
        .items
        .remove(0);
        record(&mut store, &item, State::Reviewed, "2026-10-26T12:00:00Z").unwrap();
        let path = root.join(RELATIVE);
        for bytes in [b"{\"schema_version\":99}".as_slice(), b"broken"] {
            fs::write(&path, bytes).unwrap();
            assert!(load(&root).is_err());
            assert!(record(&mut store, &item, State::Dismissed, "2026-10-26T12:00:00Z").is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn different_source_units_documents_and_generations_cannot_be_deduplicated() {
        let a = finding(
            "writingdeck-native",
            "wordy-phrase",
            Category::Style,
            Some("to"),
            1,
        );
        let mut b = a.clone();
        b.source.unit = SourceUnit::MarkdownFile;
        let mut c = a.clone();
        c.source.document = "other.md".into();
        let mut d = a.clone();
        d.source.generation = 2;
        assert_eq!(
            Queue::build([a, b, c, d], &Review::default())
                .unwrap()
                .items
                .len(),
            4
        );
    }
}
