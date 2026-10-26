//! Pinned Harper adapter. All work is local and opt-in at the controller.
//!
//! Harper 2.13 unconditionally uses harper-brill's bundled tagger and neural
//! chunker when Document is parsed. This is NOT advertised as a non-AI engine.
//! No remote service, model download, spell replacement or automatic edit.
use super::*;
use harper_core::{
    Document,
    linting::{FlatConfig, LintGroup, LintKind, Suggestion},
    parsers::Markdown,
    spell::FstDictionary,
};
use std::time::Instant;

pub const MAX_HARPER_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportStatus {
    Complete,
    ScopeExcluded,
    TooLarge,
    EngineFailed(String),
}
#[derive(Debug)]
pub struct Report {
    pub snapshot: Snapshot,
    pub options: AnalysisOptions,
    pub status: ReportStatus,
    pub findings: Vec<Diagnostic>,
    pub omitted_invalid: usize,
    pub truncated: bool,
    pub elapsed_ms: u128,
}
impl Report {
    pub fn empty(snapshot: Snapshot, options: AnalysisOptions, status: ReportStatus) -> Self {
        Self {
            snapshot,
            options,
            status,
            findings: vec![],
            omitted_invalid: 0,
            truncated: false,
            elapsed_ms: 0,
        }
    }
    pub fn matches(&self, snapshot: &Snapshot, options: &AnalysisOptions) -> bool {
        self.snapshot.stamp == snapshot.stamp && self.options == *options
    }
}
struct Loaded {
    dialect: Dialect,
    dictionary: Arc<FstDictionary>,
    group: LintGroup,
    curated: FlatConfig,
}
/// Constructing this handle does not load dictionary or model data. It should
/// live on a dedicated worker, never in the editor's render/input path.
#[derive(Default)]
pub struct HarperEngine {
    loaded: Option<Loaded>,
}
impl HarperEngine {
    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }
    pub fn analyze(&mut self, snapshot: Snapshot, options: AnalysisOptions) -> Report {
        let started = Instant::now();
        let mut report = Report::empty(snapshot, options, ReportStatus::Complete);
        if let Err(error) = report.options.validate() {
            report.status = ReportStatus::EngineFailed(error.to_string());
            return report;
        }
        if !report.options.allows_role(report.snapshot.stamp.role) {
            report.status = ReportStatus::ScopeExcluded;
            return report;
        }
        if report.snapshot.text().len() > MAX_HARPER_BYTES {
            report.status = ReportStatus::TooLarge;
            return report;
        }
        let projection = report.snapshot.project(&report.options);
        if !projection.has_prose() {
            return report;
        }
        let dialect = report.options.dialect;
        if self.loaded.as_ref().is_none_or(|s| s.dialect != dialect) {
            let dictionary = FstDictionary::curated();
            let group = LintGroup::new_curated(dictionary.clone(), harper_dialect(dialect));
            let curated = group.config.clone();
            self.loaded = Some(Loaded {
                dialect,
                dictionary,
                group,
                curated,
            });
        }
        let loaded = self.loaded.as_mut().expect("initialized grammar engine");
        loaded.group.config = loaded.curated.clone();
        if report.options.profile == Profile::DeepReview {
            loaded.group.set_all_rules_to(Some(true));
        }
        // Native Spellbook retains ownership of spelling/dictionaries.
        loaded.group.config.set_rule_enabled("SpellCheck", false);
        for key in &report.options.disabled_rules {
            if key.engine == "harper-core" && key.version == HARPER_VERSION {
                loaded.group.config.set_rule_enabled(&key.rule, false);
            }
        }
        let document = Document::new(
            &projection.markdown,
            &Markdown::default(),
            loaded.dictionary.as_ref(),
        );
        if document.get_source().len() != report.snapshot.scalar_len() {
            report.status = ReportStatus::EngineFailed(
                "grammar parser changed source coordinate space; no findings accepted".into(),
            );
            return report;
        }
        for (rule, findings) in loaded.group.organized_lints(&document) {
            for lint in findings {
                let category = category(&rule, lint.lint_kind);
                if lint.lint_kind == LintKind::Spelling
                    || !profile_allows(report.options.profile, category)
                {
                    continue;
                }
                let Some(span) = projection.scalar_span(lint.span.start..lint.span.end) else {
                    report.omitted_invalid += 1;
                    continue;
                };
                if !projection.readable_span(span) {
                    continue;
                }
                if report.options.relaxed_dialogue
                    && projection.context(span) == Context::Dialogue
                    && matches!(
                        category,
                        Category::Style | Category::Capitalization | Category::Readability
                    )
                {
                    continue;
                }
                let original = report.snapshot.slice(span).unwrap_or("");
                let alternatives = lint
                    .suggestions
                    .into_iter()
                    .map(|suggestion| match suggestion {
                        Suggestion::ReplaceWith(chars) => chars.into_iter().collect(),
                        Suggestion::Remove => String::new(),
                        Suggestion::InsertAfter(chars) => {
                            format!("{original}{}", chars.into_iter().collect::<String>())
                        }
                    })
                    .collect();
                match Diagnostic::new(
                    &projection,
                    &report.options,
                    RuleKey::harper(&rule),
                    category,
                    span,
                    lint.message,
                    alternatives,
                ) {
                    Ok(finding) => report.findings.push(finding),
                    Err(_) => report.omitted_invalid += 1,
                }
            }
        }
        report.findings.sort_by(|a, b| {
            (a.span.start, a.span.end, &a.rule).cmp(&(b.span.start, b.span.end, &b.rule))
        });
        report.truncated = report.findings.len() > MAX_FINDINGS;
        report.findings.truncate(MAX_FINDINGS);
        report.elapsed_ms = started.elapsed().as_millis();
        report
    }
}
fn harper_dialect(dialect: Dialect) -> harper_core::Dialect {
    match dialect {
        Dialect::American => harper_core::Dialect::American,
        Dialect::British => harper_core::Dialect::British,
        Dialect::Australian => harper_core::Dialect::Australian,
        Dialect::Canadian => harper_core::Dialect::Canadian,
    }
}
fn profile_allows(profile: Profile, category: Category) -> bool {
    match profile {
        Profile::DeepReview => true,
        Profile::Drafting => category == Category::Grammar,
        Profile::Revision => !matches!(category, Category::Style | Category::Readability),
    }
}
fn category(rule: &str, kind: LintKind) -> Category {
    // Harper labels its article rule Miscellaneous. Pin this explicit mapping
    // rather than treating all miscellaneous/style findings as grammar errors.
    if rule == "AnA" {
        return Category::Grammar;
    }
    match kind {
        LintKind::Agreement
        | LintKind::BoundaryError
        | LintKind::Grammar
        | LintKind::Typo
        | LintKind::WordOrder => Category::Grammar,
        LintKind::Punctuation | LintKind::Formatting => Category::Punctuation,
        LintKind::Capitalization => Category::Capitalization,
        LintKind::Readability => Category::Readability,
        LintKind::Enhancement | LintKind::Style | LintKind::Redundancy | LintKind::Repetition => {
            Category::Style
        }
        LintKind::Usage
        | LintKind::WordChoice
        | LintKind::Eggcorn
        | LintKind::Malapropism
        | LintKind::Regionalism
        | LintKind::Nonstandard => Category::Usage,
        _ => Category::Other,
    }
}
