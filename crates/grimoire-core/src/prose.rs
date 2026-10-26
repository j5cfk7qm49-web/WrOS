//! Lightweight, local, non-model prose patterns. These are descriptive
//! approximations, NOT native/ledger word units, grammar truth, authorship
//! detection, or a literary-quality score. Original source is never rewritten.
use crate::diagnostics::{
    AnalysisOptions, Category, Context, Diagnostic, Profile, RuleKey, Snapshot,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use unicode_normalization::UnicodeNormalization;

pub const ENGINE: &str = "writingdeck-native";
pub const VERSION: &str = "1";
pub const MAX_ANALYSIS_BYTES: usize = 65_536;
pub const MIN_GRADE_WORDS: u64 = 100;
/// Closed function-word vocabulary only: profiles must not retain arbitrary
/// manuscript words as another copy of the author's prose.
pub const FUNCTION_WORDS: &[&str] = &[
    "i", "we", "you", "he", "she", "they", "it", "and", "but", "or", "the", "a", "an", "of", "to",
    "in", "with", "as", "that", "was", "were", "is", "are", "had", "have", "not", "if", "then",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FictionPreset {
    #[default]
    General,
    Accessible,
    Experimental,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub analysis: AnalysisOptions,
    pub preset: FictionPreset,
}
impl Options {
    pub fn digest(&self) -> String {
        crate::diagnostics::hex_digest(
            &serde_json::to_vec(self).expect("serializable prose options"),
        )
    }
    fn sentence_limit(&self) -> Option<usize> {
        match self.preset {
            FictionPreset::General => Some(35),
            FictionPreset::Accessible => Some(24),
            FictionPreset::Experimental => None,
        }
    }
    fn paragraph_limit(&self) -> usize {
        match self.preset {
            FictionPreset::General => 180,
            FictionPreset::Accessible => 120,
            FictionPreset::Experimental => 320,
        }
    }
}
/// Fiction preset only; shared grammar/profile/scope/rule choices retain their
/// single authoritative diagnostic-preferences store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub preset: FictionPreset,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            preset: FictionPreset::General,
        }
    }
}
impl crate::statefile::Validate for Settings {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unknown/future prose settings; original preserved"
        );
        Ok(())
    }
}
impl Settings {
    pub fn load(root: &std::path::Path) -> Result<crate::statefile::Store<Self>> {
        crate::statefile::Store::load(
            root,
            std::path::Path::new(".grimoire/writingdeck/prose-settings.json"),
            Self::default(),
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Complete,
    NoProse,
    TooLarge,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub analysis_words: u64,
    pub unique_normalized_words: u64,
    pub word_letters: u64,
    pub syllables_heuristic: u64,
    pub sentence_segments: u64,
    pub sentence_word_sum: u64,
    pub sentence_word_sum_squares: u64,
    pub paragraph_segments: u64,
    pub paragraph_word_sum: u64,
    pub dialogue_words_heuristic: u64,
    pub long_words: u64,
    pub commas: u64,
    pub semicolons: u64,
    pub colons: u64,
    pub dashes: u64,
    pub function_words: BTreeMap<String, u64>,
    /// Approximate English formula only, absent below 100 words/3 segments.
    pub approximate_english_grade: Option<f64>,
    pub sentence_words: Vec<u64>,
}
impl Metrics {
    /// Validate numeric, finite, bounded non-prose metadata before aggregation.
    pub fn validate(&self) -> Result<()> {
        let w = self.analysis_words;
        ensure!(
            w <= 2_000_000 && self.word_letters <= 128_000_000,
            "analysis metrics exceed bounded material limits"
        );
        ensure!(
            self.unique_normalized_words <= w
                && self.long_words <= w
                && self.dialogue_words_heuristic <= w,
            "invalid analysis word proportions"
        );
        ensure!(
            self.syllables_heuristic <= self.word_letters
                && self.paragraph_segments <= w
                && self.sentence_segments <= w,
            "invalid analysis segment counts"
        );
        ensure!(
            self.sentence_word_sum == w && self.paragraph_word_sum == w,
            "inconsistent analysis word totals"
        );
        ensure!(
            self.sentence_word_sum_squares >= w && self.sentence_word_sum_squares <= w * w,
            "invalid analysis second moment"
        );
        ensure!(
            w == 0 || (self.sentence_segments > 0 && self.paragraph_segments > 0),
            "missing analysis segments"
        );
        ensure!(
            [self.commas, self.semicolons, self.colons, self.dashes]
                .iter()
                .all(|n| *n <= 128_000_000),
            "invalid analysis punctuation counts"
        );
        ensure!(
            self.sentence_words.len() <= 2_000_000
                && self.sentence_words.iter().all(|n| *n > 0 && *n <= w),
            "invalid analysis segment lengths"
        );
        if !self.sentence_words.is_empty() {
            ensure!(
                self.sentence_words.len() as u64 == self.sentence_segments
                    && self.sentence_words.iter().sum::<u64>() == w
                    && self.sentence_words.iter().map(|n| n * n).sum::<u64>()
                        == self.sentence_word_sum_squares,
                "inconsistent analysis segment distribution"
            );
        }
        ensure!(
            self.approximate_english_grade
                .is_none_or(|n| n.is_finite() && (0.0..=1_000_000.0).contains(&n)),
            "invalid approximate English grade"
        );
        for (key, count) in &self.function_words {
            ensure!(
                FUNCTION_WORDS.contains(&key.as_str()) && *count <= w,
                "profile cannot retain arbitrary manuscript vocabulary or invalid counts"
            );
        }
        Ok(())
    }
    pub fn mean_sentence_words(&self) -> Option<f64> {
        (self.sentence_segments > 0)
            .then(|| self.sentence_word_sum as f64 / self.sentence_segments as f64)
    }
    pub fn sentence_standard_deviation(&self) -> Option<f64> {
        self.mean_sentence_words().map(|mean| {
            (self.sentence_word_sum_squares as f64 / self.sentence_segments as f64 - mean * mean)
                .max(0.0)
                .sqrt()
        })
    }
    /// Numeric descriptive vector, not a scalar quality/authorship score.
    /// Adequacy/selected-source provenance is enforced by voice-profile code.
    pub fn voice_features(&self) -> BTreeMap<String, f64> {
        if self.analysis_words == 0 {
            return BTreeMap::new();
        }
        let per_word = self.analysis_words as f64;
        let mut values = BTreeMap::from([
            (
                "sentence_mean".into(),
                self.mean_sentence_words().unwrap_or(0.0),
            ),
            (
                "sentence_sd".into(),
                self.sentence_standard_deviation().unwrap_or(0.0),
            ),
            (
                "paragraph_mean".into(),
                self.paragraph_word_sum as f64 / self.paragraph_segments.max(1) as f64,
            ),
            (
                "word_letters_mean".into(),
                self.word_letters as f64 / per_word,
            ),
            (
                "dialogue_fraction_heuristic".into(),
                self.dialogue_words_heuristic as f64 / per_word,
            ),
            (
                "long_word_fraction".into(),
                self.long_words as f64 / per_word,
            ),
            (
                "commas_per_1000".into(),
                1000.0 * self.commas as f64 / per_word,
            ),
            (
                "semicolons_per_1000".into(),
                1000.0 * self.semicolons as f64 / per_word,
            ),
            (
                "colons_per_1000".into(),
                1000.0 * self.colons as f64 / per_word,
            ),
            (
                "dashes_per_1000".into(),
                1000.0 * self.dashes as f64 / per_word,
            ),
        ]);
        for word in FUNCTION_WORDS {
            values.insert(
                format!("function_{word}_per_1000"),
                1000.0 * self.function_words.get(*word).copied().unwrap_or(0) as f64 / per_word,
            );
        }
        values
    }
}
#[derive(Debug, Clone)]
pub struct Report {
    pub snapshot: Snapshot,
    pub options: Options,
    pub configuration: String,
    pub status: Status,
    pub metrics: Metrics,
    pub findings: Vec<Diagnostic>,
    pub omitted: usize,
    pub elapsed_ms: u64,
}
impl Report {
    pub fn matches(&self, snapshot: &Snapshot, options: &Options) -> bool {
        self.snapshot.stamp == snapshot.stamp && self.configuration == options.digest()
    }
}
struct Word {
    start: usize,
    end: usize,
    normalized: String,
    letters: usize,
}
pub fn analyze(snapshot: Snapshot, options: Options) -> Result<Report> {
    options.analysis.validate()?;
    let start = Instant::now();
    let mut report = Report {
        configuration: options.digest(),
        snapshot,
        options,
        status: Status::Complete,
        metrics: Metrics::default(),
        findings: vec![],
        omitted: 0,
        elapsed_ms: 0,
    };
    if report.snapshot.text().len() > MAX_ANALYSIS_BYTES {
        report.status = Status::TooLarge;
        return Ok(report);
    }
    let projection = report.snapshot.project(&report.options.analysis);
    let chars: Vec<char> = projection.prose.chars().collect();
    let words = words(&chars);
    if words.is_empty() {
        report.status = Status::NoProse;
        return Ok(report);
    }
    let mut metrics = Metrics::default();
    let mut vocabulary = BTreeSet::new();
    for word in &words {
        metrics.analysis_words += 1;
        metrics.word_letters += word.letters as u64;
        metrics.syllables_heuristic += syllables(&word.normalized);
        metrics.long_words += u64::from(word.letters >= 8);
        vocabulary.insert(&word.normalized);
        if FUNCTION_WORDS.contains(&word.normalized.as_str()) {
            *metrics
                .function_words
                .entry(word.normalized.clone())
                .or_default() += 1;
        }
        if let Some(span) = projection.scalar_span(word.start..word.end)
            && projection.context(span) == Context::Dialogue
        {
            metrics.dialogue_words_heuristic += 1;
        }
    }
    metrics.unique_normalized_words = vocabulary.len() as u64;
    for c in &chars {
        match c {
            ',' => metrics.commas += 1,
            ';' => metrics.semicolons += 1,
            ':' => metrics.colons += 1,
            '—' | '–' => metrics.dashes += 1,
            _ => {}
        }
    }
    let mut sentence_start = 0;
    let mut paragraph_start = 0;
    let mut sentences = vec![];
    let mut paragraphs = vec![];
    for i in 1..words.len() {
        let gap: String = chars[words[i - 1].end..words[i].start].iter().collect();
        let paragraph = paragraph_break(&gap);
        let abbreviation = matches!(
            words[i - 1].normalized.as_str(),
            "mr" | "mrs" | "ms" | "dr" | "prof" | "st" | "vs"
        ) || (words[i - 1].letters == 1 && words[i - 1].normalized != "i");
        let terminal = gap
            .chars()
            .any(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？'))
            && !abbreviation;
        if paragraph || terminal {
            sentences.push(sentence_start..i);
            sentence_start = i;
        }
        if paragraph {
            paragraphs.push(paragraph_start..i);
            paragraph_start = i;
        }
    }
    sentences.push(sentence_start..words.len());
    paragraphs.push(paragraph_start..words.len());
    metrics.sentence_segments = sentences.len() as u64;
    metrics.paragraph_segments = paragraphs.len() as u64;
    for range in &sentences {
        let n = range.len() as u64;
        metrics.sentence_words.push(n);
        metrics.sentence_word_sum += n;
        metrics.sentence_word_sum_squares += n * n;
    }
    metrics.paragraph_word_sum = words.len() as u64;
    if metrics.analysis_words >= MIN_GRADE_WORDS && metrics.sentence_segments >= 3 {
        // Flesch-Kincaid-like English approximation, not an objective grade for
        // literary readers, dialogue, poetry, names or multilingual prose.
        let grade = 0.39 * metrics.analysis_words as f64 / metrics.sentence_segments as f64
            + 11.8 * metrics.syllables_heuristic as f64 / metrics.analysis_words as f64
            - 15.59;
        metrics.approximate_english_grade = Some(grade.max(0.0));
    }
    if report.options.analysis.profile != Profile::Drafting {
        let mut emit = |rule: &str,
                        category: Category,
                        from: usize,
                        to: usize,
                        message: String,
                        alternatives: Vec<String>| {
            let key = RuleKey {
                engine: ENGINE.into(),
                version: VERSION.into(),
                rule: rule.into(),
            };
            if report.options.analysis.disabled_rules.contains(&key) {
                return;
            }
            let Some(span) = projection.scalar_span(from..to) else {
                return;
            };
            if report.options.analysis.relaxed_dialogue
                && projection.context(span) == Context::Dialogue
            {
                return;
            }
            if let Ok(finding) = Diagnostic::new(
                &projection,
                &report.options.analysis,
                key,
                category,
                span,
                message,
                alternatives,
            ) {
                if report.findings.len() < crate::diagnostics::MAX_FINDINGS {
                    report.findings.push(finding)
                } else {
                    report.omitted += 1;
                }
            }
        };
        if let Some(limit) = report.options.sentence_limit() {
            for range in &sentences {
                if range.len() > limit {
                    emit(
                        "long-segment",
                        Category::Readability,
                        words[range.start].start,
                        words[range.end - 1].end,
                        format!(
                            "Long heuristic sentence segment: {} analysis words (preset guide {limit}). Review rhythm/intent; length is not an error or quality score.",
                            range.len()
                        ),
                        vec![],
                    );
                }
            }
        }
        for range in &paragraphs {
            if range.len() > report.options.paragraph_limit() {
                emit(
                    "dense-paragraph",
                    Category::Readability,
                    words[range.start].start,
                    words[range.end - 1].end,
                    format!(
                        "Dense paragraph: {} analysis words. Consider pacing and intended form; no split is prescribed.",
                        range.len()
                    ),
                    vec![],
                );
            }
        }
        for (phrase, replacement) in [
            ("in order to", "to"),
            ("due to the fact that", "because"),
            ("at this point in time", "now"),
            ("in the event that", "if"),
            ("a large number of", "many"),
            ("prior to", "before"),
            ("at the present time", "now"),
        ] {
            let parts: Vec<_> = phrase.split_whitespace().collect();
            for slice in words.windows(parts.len()) {
                if !slice
                    .iter()
                    .zip(&parts)
                    .all(|(word, part)| word.normalized == *part)
                {
                    continue;
                }
                let from = slice[0].start;
                let to = slice[slice.len() - 1].end;
                let Some(span) = projection.scalar_span(from..to) else {
                    continue;
                };
                let Some(raw) = report.snapshot.slice(span) else {
                    continue;
                };
                // Do not join words across hidden code/private material or
                // silently remove markup, even when the projection has spaces.
                if raw
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
                    != phrase
                {
                    continue;
                }
                let mut replacement = replacement.to_owned();
                if raw.chars().next().is_some_and(char::is_uppercase) {
                    replacement.replace_range(..1, &replacement[..1].to_uppercase());
                }
                let alternatives = if raw.contains(['\n', '\r']) {
                    vec![]
                } else {
                    vec![replacement]
                };
                emit(
                    "wordy-phrase",
                    Category::Style,
                    from,
                    to,
                    format!(
                        "Possible wordy phrase: {phrase:?}. The shorter alternative may change rhythm/register; retain it deliberately if intended."
                    ),
                    alternatives,
                );
            }
        }
        if report.options.analysis.profile == Profile::DeepReview {
            for pair in words.windows(2) {
                if pair[0].normalized == pair[1].normalized && pair[0].letters >= 2 {
                    let gap: String = chars[pair[0].end..pair[1].start].iter().collect();
                    if gap.chars().all(char::is_whitespace) {
                        emit("adjacent-echo",Category::Style,pair[0].start,pair[1].end,"Adjacent repeated word: may be deliberate, dialogue or grammatical (for example had had). Review, not an automatic deletion.".into(),vec![]);
                    }
                }
            }
        }
    }
    report
        .findings
        .sort_by_key(|d| (d.span.start, d.span.end, d.rule.rule.clone()));
    report.metrics = metrics;
    report.elapsed_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok(report)
}
fn words(chars: &[char]) -> Vec<Word> {
    let inside = |c: char| {
        c.is_alphanumeric()
            || unicode_normalization::char::is_combining_mark(c)
            || matches!(c, '\'' | '’' | '-')
    };
    let mut out = vec![];
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_alphabetic() {
            at += 1;
            continue;
        }
        let from = at;
        at += 1;
        while at < chars.len() && inside(chars[at]) {
            at += 1;
        }
        while at > from && matches!(chars[at - 1], '\'' | '’' | '-') {
            at -= 1;
        }
        if at == from {
            at += 1;
            continue;
        }
        let raw: String = chars[from..at].iter().collect();
        let normalized = raw.replace('’', "'").to_lowercase().nfc().collect();
        out.push(Word {
            start: from,
            end: at,
            letters: raw.chars().filter(|c| c.is_alphabetic()).count(),
            normalized,
        });
    }
    out
}
fn paragraph_break(gap: &str) -> bool {
    let parts: Vec<_> = gap.split('\n').collect();
    parts.len() >= 3
        && parts[1..parts.len() - 1]
            .iter()
            .any(|line| line.trim().is_empty())
}
fn syllables(word: &str) -> u64 {
    let lower = word.to_ascii_lowercase();
    let mut count = 0u64;
    let mut previous = false;
    for c in lower.chars() {
        let vowel = matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if vowel && !previous {
            count += 1;
        }
        previous = vowel;
    }
    if lower.ends_with('e') && !lower.ends_with("le") && count > 1 {
        count -= 1;
    }
    count.max(1)
}
/// Use only numeric descriptive data from complete reference material. The
/// function intentionally does not aggregate vocabulary/type ratios or retain
/// arbitrary source words as profile metadata.
pub fn merge_metrics(values: &[Metrics]) -> Result<Metrics> {
    ensure!(
        !values.is_empty() && values.len() <= 256,
        "select at most 256 reference units first"
    );
    let mut out = Metrics::default();
    for value in values {
        value.validate()?;
        out.analysis_words = out
            .analysis_words
            .checked_add(value.analysis_words)
            .ok_or_else(|| anyhow::anyhow!("reference count overflow"))?;
        out.word_letters += value.word_letters;
        out.syllables_heuristic += value.syllables_heuristic;
        out.sentence_segments += value.sentence_segments;
        out.sentence_word_sum += value.sentence_word_sum;
        out.sentence_word_sum_squares += value.sentence_word_sum_squares;
        out.paragraph_segments += value.paragraph_segments;
        out.paragraph_word_sum += value.paragraph_word_sum;
        out.dialogue_words_heuristic += value.dialogue_words_heuristic;
        out.long_words += value.long_words;
        out.commas += value.commas;
        out.semicolons += value.semicolons;
        out.colons += value.colons;
        out.dashes += value.dashes;
        for (key, count) in &value.function_words {
            ensure!(
                FUNCTION_WORDS.contains(&key.as_str()),
                "profile cannot retain arbitrary manuscript vocabulary"
            );
            *out.function_words.entry(key.clone()).or_default() += count;
        }
    }
    out.validate()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{SourceRole, SourceUnit};
    fn snapshot(raw: &str) -> Snapshot {
        Snapshot::new(
            std::path::Path::new("scene.md"),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            1,
            raw,
        )
        .unwrap()
    }
    fn revision() -> Options {
        Options {
            analysis: AnalysisOptions {
                profile: Profile::Revision,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn short_material_has_descriptive_counts_not_a_fabricated_grade_or_quality_score() {
        let report = analyze(snapshot("Mara waited. She looked away.\n"), revision()).unwrap();
        assert_eq!(report.metrics.analysis_words, 5);
        assert_eq!(report.metrics.sentence_segments, 2);
        assert_eq!(report.metrics.approximate_english_grade, None);
        assert_eq!(report.metrics.mean_sentence_words(), Some(2.5));
        assert_eq!(report.metrics.sentence_standard_deviation(), Some(0.5));
    }
    #[test]
    fn unicode_literal_wordiness_is_byte_exact_and_current_configuration_bound() {
        let source = snapshot("Café 🦊 e\u{301}. We waited in order to see.\n");
        let options = revision();
        let report = analyze(source.clone(), options.clone()).unwrap();
        let finding = report
            .findings
            .iter()
            .find(|d| d.rule.rule == "wordy-phrase")
            .unwrap();
        assert_eq!(source.slice(finding.span), Some("in order to"));
        assert_eq!(finding.expected, "in order to");
        assert_eq!(
            finding
                .checked_replacement(&source, &options.analysis, 0)
                .unwrap()
                .1,
            "to"
        );
        let other = Options {
            preset: FictionPreset::Experimental,
            ..options.clone()
        };
        assert!(!report.matches(&source, &other));
        let changed = snapshot("Changed source in order to test.");
        assert!(
            finding
                .checked_replacement(&changed, &options.analysis, 0)
                .is_err()
        );
    }
    #[test]
    fn private_code_metadata_targets_footnotes_and_excluded_roles_are_not_analysis_words() {
        let raw = "---\ntitle: secret title\n---\nVisible words. `code words` [link](hidden-target) %% private words %% TK\n\n[^n]: footnote words\n";
        let source = Snapshot::new(
            std::path::Path::new("source.md"),
            SourceUnit::MarkdownFile,
            SourceRole::Manuscript,
            1,
            raw,
        )
        .unwrap();
        let report = analyze(source, revision()).unwrap();
        assert_eq!(report.metrics.analysis_words, 3, "only Visible words link");
        let note = Snapshot::new(
            std::path::Path::new("notes/n.md"),
            SourceUnit::EditorBody,
            SourceRole::Note,
            1,
            "Note words.",
        )
        .unwrap();
        assert_eq!(analyze(note, revision()).unwrap().status, Status::NoProse);
    }
    #[test]
    fn dialogue_remains_counted_but_relaxed_style_does_not_treat_it_as_an_error() {
        let source = snapshot("“We waited in order to see,” she said.\n");
        let mut options = revision();
        let relaxed = analyze(source.clone(), options.clone()).unwrap();
        assert!(relaxed.metrics.dialogue_words_heuristic > 0);
        assert!(relaxed.findings.is_empty());
        options.analysis.relaxed_dialogue = false;
        assert!(
            analyze(source, options)
                .unwrap()
                .findings
                .iter()
                .any(|d| d.rule.rule == "wordy-phrase")
        );
    }
    #[test]
    fn drafting_is_low_interruption_and_experimental_does_not_prescribe_sentence_lengths() {
        let raw = format!("{} in order to finish.\n", "word ".repeat(40));
        let draft = analyze(snapshot(&raw), Options::default()).unwrap();
        assert!(draft.findings.is_empty());
        assert_eq!(draft.metrics.analysis_words, 44);
        let general = analyze(snapshot(&raw), revision()).unwrap();
        assert!(
            general
                .findings
                .iter()
                .any(|d| d.rule.rule == "long-segment")
        );
        let experimental = analyze(
            snapshot(&raw),
            Options {
                preset: FictionPreset::Experimental,
                ..revision()
            },
        )
        .unwrap();
        assert!(
            !experimental
                .findings
                .iter()
                .any(|d| d.rule.rule == "long-segment")
        );
    }
    #[test]
    fn crlf_paragraphs_and_grammar_valid_echoes_are_descriptive_not_auto_deleted() {
        let mut options = revision();
        options.analysis.profile = Profile::DeepReview;
        let report = analyze(
            snapshot("He had had enough.\r\n\r\nShe waited.\r\n"),
            options,
        )
        .unwrap();
        assert_eq!(report.metrics.paragraph_segments, 2);
        let echo = report
            .findings
            .iter()
            .find(|d| d.rule.rule == "adjacent-echo")
            .unwrap();
        assert!(echo.replacements.is_empty());
    }
    #[test]
    fn large_prose_is_not_silently_truncated_or_declared_clear() {
        let source = snapshot(&"word ".repeat(MAX_ANALYSIS_BYTES / 5 + 1));
        let report = analyze(source, revision()).unwrap();
        assert_eq!(report.status, Status::TooLarge);
        assert_eq!(report.metrics.analysis_words, 0);
    }
    #[test]
    fn profile_statistics_merge_by_counts_without_retaining_arbitrary_source_vocabulary() {
        let a = analyze(snapshot("I waited. She looked away."), revision())
            .unwrap()
            .metrics;
        let b = analyze(snapshot("They saw the sea. We stayed."), revision())
            .unwrap()
            .metrics;
        let merged = merge_metrics(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(merged.analysis_words, a.analysis_words + b.analysis_words);
        assert_eq!(
            merged.sentence_segments,
            a.sentence_segments + b.sentence_segments
        );
        assert!(merged.voice_features().contains_key("function_i_per_1000"));
        let json = serde_json::to_string(&merged).unwrap();
        assert!(!json.contains("waited") && !json.contains("stayed"));
        let mut poisoned = a;
        poisoned
            .function_words
            .insert("private manuscript phrase".into(), 1);
        assert!(merge_metrics(&[poisoned]).is_err());
    }
}
