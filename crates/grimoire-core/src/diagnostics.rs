//! Revision-bound diagnostics over original UTF-8 source, never rendered prose.
//!
//! An editor body and a whole Markdown file are different, explicitly named
//! coordinate spaces. A finding cannot migrate between them. Unicode scalars,
//! input bytes and UTF-16 units are converted with checked boundary tables;
//! invalid offsets (including half a surrogate) are rejected, not rounded.
//! Snapshots, projections and findings stay in memory, not prose-bearing JSON.

pub mod harper;
pub mod preferences;
pub mod worker;

use anyhow::{Result, bail, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, ops::Range, path::Path, sync::Arc};

pub const MAX_SOURCE_BYTES: usize = 262_144;
pub const MAX_FINDINGS: usize = 256;
pub const MAX_REPLACEMENT_BYTES: usize = 1_024;
pub const HARPER_VERSION: &str = "2.13.0";

/// Byte offsets are zero-based, half-open, in the *specified original source*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}
impl From<Range<usize>> for ByteSpan {
    fn from(r: Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}
impl ByteSpan {
    pub fn range(self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceUnit {
    EditorBody,
    MarkdownFile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceRole {
    Manuscript,
    Note,
    Research,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Context {
    Narration,
    Dialogue,
    Footnote,
    Note,
    Research,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Drafting,
    Revision,
    DeepReview,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    American,
    British,
    Australian,
    Canadian,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Grammar,
    Usage,
    Punctuation,
    Capitalization,
    Style,
    Readability,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Suggestion,
    Information,
}

/// A rule ignore is version-bound; it stores identifiers, never source prose.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleKey {
    pub engine: String,
    pub version: String,
    pub rule: String,
}
impl RuleKey {
    pub fn harper(rule: impl Into<String>) -> Self {
        Self {
            engine: "harper-core".into(),
            version: HARPER_VERSION.into(),
            rule: rule.into(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        for value in [&self.engine, &self.version, &self.rule] {
            ensure!(
                !value.is_empty() && value.len() <= 128,
                "invalid diagnostic rule identifier length"
            );
            ensure!(
                value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c)),
                "invalid diagnostic rule identifier"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisOptions {
    pub profile: Profile,
    pub dialect: Dialect,
    pub include_footnotes: bool,
    pub include_notes: bool,
    pub include_research: bool,
    /// This relaxes style/capitalization, not grammar or all quoted prose.
    pub relaxed_dialogue: bool,
    pub disabled_rules: BTreeSet<RuleKey>,
}
impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            profile: Profile::Drafting,
            dialect: Dialect::American,
            include_footnotes: false,
            include_notes: false,
            include_research: false,
            relaxed_dialogue: true,
            disabled_rules: BTreeSet::new(),
        }
    }
}
impl AnalysisOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.disabled_rules.len() <= 512,
            "too many muted diagnostic rules"
        );
        for rule in &self.disabled_rules {
            rule.validate()?;
        }
        Ok(())
    }
    pub fn digest(&self) -> String {
        hex_digest(&serde_json::to_vec(self).expect("serializable analysis options"))
    }
    pub fn allows_role(&self, role: SourceRole) -> bool {
        match role {
            SourceRole::Manuscript => true,
            SourceRole::Note => self.include_notes,
            SourceRole::Research => self.include_research,
        }
    }
}

/// Content and generation are both checked. A new request invalidates the old
/// generation even when an undo happens to restore identical bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStamp {
    pub document: String,
    pub unit: SourceUnit,
    pub role: SourceRole,
    pub generation: u64,
    pub sha256: String,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub stamp: SourceStamp,
    text: Arc<str>,
    coordinates: Arc<Coordinates>,
}
impl Snapshot {
    pub fn new(
        document: &Path,
        unit: SourceUnit,
        role: SourceRole,
        generation: u64,
        text: impl Into<Arc<str>>,
    ) -> Result<Self> {
        ensure!(
            !document.as_os_str().is_empty(),
            "diagnostic document reference is empty"
        );
        ensure!(
            document
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "diagnostic document reference must be project-relative"
        );
        let document = document
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("diagnostic document reference is not UTF-8"))?
            .to_owned();
        ensure!(
            document.len() <= 4096 && !document.chars().any(char::is_control),
            "invalid diagnostic document reference"
        );
        let text = text.into();
        ensure!(
            text.len() <= MAX_SOURCE_BYTES,
            "diagnostic snapshot exceeds {MAX_SOURCE_BYTES} byte source limit; nothing analyzed"
        );
        let stamp = SourceStamp {
            document,
            unit,
            role,
            generation,
            sha256: hex_digest(text.as_bytes()),
        };
        let coordinates = Arc::new(Coordinates::new(&text));
        Ok(Self {
            stamp,
            text,
            coordinates,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn scalar_len(&self) -> usize {
        self.coordinates.bytes.len() - 1
    }
    pub fn slice(&self, span: ByteSpan) -> Option<&str> {
        self.text.get(span.range())
    }
    pub fn scalar_span(&self, range: Range<usize>) -> Option<ByteSpan> {
        self.coordinates.scalar_span(range)
    }
    /// Zero-based logical line and Unicode scalar column, not display cells.
    pub fn point(&self, byte: usize) -> Option<(usize, usize)> {
        self.coordinates.point(byte)
    }
    pub fn byte_at(&self, line: usize, scalar_column: usize) -> Option<usize> {
        self.coordinates.byte_at(line, scalar_column)
    }
    pub fn project(&self, options: &AnalysisOptions) -> Projection {
        Projection::new(self, options)
    }
}

/// Tables include EOF as a boundary. A CR is an ordinary scalar; LF advances
/// the logical line. No case folding, smart quotes, entity decoding or NFC.
#[derive(Debug)]
struct Coordinates {
    bytes: Vec<usize>,
    utf16: Vec<usize>,
    lines: Vec<usize>, // scalar offsets
}
impl Coordinates {
    fn new(text: &str) -> Self {
        let mut bytes = vec![];
        let mut utf16 = vec![0];
        let mut lines = vec![0];
        for (i, (byte, c)) in text.char_indices().enumerate() {
            bytes.push(byte);
            utf16.push(utf16.last().copied().unwrap_or(0) + c.len_utf16());
            if c == '\n' {
                lines.push(i + 1);
            }
        }
        bytes.push(text.len());
        Self {
            bytes,
            utf16,
            lines,
        }
    }
    fn scalar_span(&self, range: Range<usize>) -> Option<ByteSpan> {
        if range.start > range.end {
            return None;
        }
        Some(ByteSpan {
            start: *self.bytes.get(range.start)?,
            end: *self.bytes.get(range.end)?,
        })
    }
    fn byte_scalar(&self, byte: usize) -> Option<usize> {
        self.bytes.binary_search(&byte).ok()
    }
    fn utf16_scalar(&self, unit: usize) -> Option<usize> {
        self.utf16.binary_search(&unit).ok()
    }
    fn point(&self, byte: usize) -> Option<(usize, usize)> {
        let scalar = self.byte_scalar(byte)?;
        let line = self
            .lines
            .partition_point(|&start| start <= scalar)
            .checked_sub(1)?;
        Some((line, scalar - self.lines[line]))
    }
    fn byte_at(&self, line: usize, col: usize) -> Option<usize> {
        let at = self.lines.get(line)?.checked_add(col)?;
        // The LF itself is a valid end-of-line boundary, but a column may
        // never silently wrap to the next logical line.
        if self.lines.get(line + 1).is_some_and(|&next| at >= next) {
            return None;
        }
        self.bytes.get(at).copied()
    }
}

/// Both projections preserve the number and order of Unicode scalars, and all
/// original CR/LF characters. Native Markdown retains markup for Harper's own
/// parser; plain prose is a separate source-mapped surface for other engines.
/// Hidden multi-byte characters may become single-byte spaces: input byte and
/// UTF-16 indices must therefore use *this projection's* tables, not raw bytes.
#[derive(Debug)]
pub struct Projection {
    pub snapshot: Snapshot,
    pub markdown: Arc<str>,
    pub prose: Arc<str>,
    markdown_coordinates: Coordinates,
    prose_coordinates: Coordinates,
    readable: Vec<bool>,
    literal: Vec<bool>,
    footnotes: Vec<Range<usize>>, // original scalar offsets
    dialogue: Vec<Range<usize>>,  // heuristic, original scalar offsets
}
#[derive(Debug, Clone, Copy)]
pub enum Surface {
    NativeMarkdown,
    PlainProse,
}
impl Projection {
    fn new(snapshot: &Snapshot, options: &AnalysisOptions) -> Self {
        let original: Vec<char> = snapshot.text.chars().collect();
        let mut native = original.clone();
        let mut plain: Vec<char> = original.iter().map(|&c| mask(c)).collect();
        let mut readable = vec![false; original.len()];
        let mut literal = vec![false; original.len()];
        let mut footnotes = vec![];
        let mut exclusions = vec![];
        let mut suppressed = 0usize;
        let mut stack = vec![];
        let mut md_options = Options::all().difference(Options::ENABLE_SMART_PUNCTUATION);
        if snapshot.stamp.unit == SourceUnit::EditorBody {
            md_options.remove(
                Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
                    | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS,
            );
        }
        for (event, range) in Parser::new_ext(snapshot.text(), md_options).into_offset_iter() {
            let Some(start) = snapshot.coordinates.byte_scalar(range.start) else {
                continue;
            };
            let Some(end) = snapshot.coordinates.byte_scalar(range.end) else {
                continue;
            };
            match event {
                Event::Start(tag) => {
                    let hide = matches!(tag, Tag::CodeBlock(_) | Tag::MetadataBlock(_));
                    let foot = matches!(tag, Tag::FootnoteDefinition(_));
                    if foot {
                        footnotes.push(start..end);
                    }
                    let exclude = matches!(tag, Tag::MetadataBlock(_))
                        || (foot && !options.include_footnotes);
                    if exclude {
                        exclusions.push(start..end);
                    }
                    let hide = hide || (foot && !options.include_footnotes);
                    suppressed += usize::from(hide);
                    stack.push(hide);
                }
                Event::End(_) => {
                    if stack.pop() == Some(true) {
                        suppressed = suppressed.saturating_sub(1);
                    }
                }
                Event::Text(text) if suppressed == 0 => {
                    let exact = snapshot.text().get(range.clone()) == Some(text.as_ref());
                    plain[start..end].copy_from_slice(&original[start..end]);
                    readable[start..end].fill(true);
                    // Escaped punctuation and decoded entities are locatable
                    // but cannot be replaced as though they were literal text.
                    literal[start..end].fill(exact);
                }
                Event::SoftBreak | Event::HardBreak if suppressed == 0 => {
                    for i in start..end {
                        if original[i].is_whitespace() {
                            plain[i] = original[i];
                            readable[i] = true;
                            literal[i] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        for (start, end, _) in crate::notes::ranges(snapshot.text()) {
            if let (Some(start), Some(end)) = (
                snapshot.coordinates.byte_scalar(start),
                snapshot.coordinates.byte_scalar(end),
            ) {
                exclusions.push(start..end);
            }
        }
        if !options.allows_role(snapshot.stamp.role) {
            exclusions.push(0..original.len());
        }
        for range in exclusions {
            for i in range {
                native[i] = mask(original[i]);
                plain[i] = mask(original[i]);
                readable[i] = false;
                literal[i] = false;
            }
        }
        let markdown: Arc<str> = native.into_iter().collect::<String>().into();
        let prose: Arc<str> = plain.into_iter().collect::<String>().into();
        let markdown_coordinates = Coordinates::new(&markdown);
        let prose_coordinates = Coordinates::new(&prose);
        let dialogue = dialogue_ranges(&original, &readable);
        Self {
            snapshot: snapshot.clone(),
            markdown,
            prose,
            markdown_coordinates,
            prose_coordinates,
            readable,
            literal,
            footnotes,
            dialogue,
        }
    }
    fn coordinates(&self, surface: Surface) -> &Coordinates {
        match surface {
            Surface::NativeMarkdown => &self.markdown_coordinates,
            Surface::PlainProse => &self.prose_coordinates,
        }
    }
    pub fn scalar_span(&self, range: Range<usize>) -> Option<ByteSpan> {
        self.snapshot.scalar_span(range)
    }
    pub fn input_byte_span(&self, surface: Surface, range: Range<usize>) -> Option<ByteSpan> {
        if range.start > range.end {
            return None;
        }
        let c = self.coordinates(surface);
        self.scalar_span(c.byte_scalar(range.start)?..c.byte_scalar(range.end)?)
    }
    pub fn input_utf16_span(&self, surface: Surface, range: Range<usize>) -> Option<ByteSpan> {
        if range.start > range.end {
            return None;
        }
        let c = self.coordinates(surface);
        self.scalar_span(c.utf16_scalar(range.start)?..c.utf16_scalar(range.end)?)
    }
    /// Zero-based input line and columns with an explicitly declared encoding.
    pub fn input_line_span(
        &self,
        surface: Surface,
        start: (usize, usize),
        end: (usize, usize),
        encoding: ColumnEncoding,
    ) -> Option<ByteSpan> {
        let c = self.coordinates(surface);
        let scalar = |(line, col): (usize, usize)| -> Option<usize> {
            let origin = *c.lines.get(line)?;
            let at = match encoding {
                ColumnEncoding::Scalars => origin.checked_add(col)?,
                ColumnEncoding::Utf8Bytes => c.byte_scalar(c.bytes[origin].checked_add(col)?)?,
                ColumnEncoding::Utf16 => c.utf16_scalar(c.utf16[origin].checked_add(col)?)?,
            };
            if c.lines.get(line + 1).is_some_and(|&next| at >= next) || at >= c.bytes.len() {
                return None;
            }
            Some(at)
        };
        self.scalar_span(scalar(start)?..scalar(end)?)
    }
    pub fn has_prose(&self) -> bool {
        self.readable.iter().any(|&b| b)
    }
    pub fn readable_span(&self, span: ByteSpan) -> bool {
        let Some(range) = self.source_scalars(span) else {
            return false;
        };
        if range.is_empty() {
            return self.insertion_is_literal(range.start);
        }
        self.readable[range].iter().any(|&b| b)
    }
    pub fn replaceable_span(&self, span: ByteSpan) -> bool {
        let Some(range) = self.source_scalars(span) else {
            return false;
        };
        if range.is_empty() {
            return self.insertion_is_literal(range.start);
        }
        self.literal[range].iter().all(|&b| b)
    }
    fn insertion_is_literal(&self, at: usize) -> bool {
        self.literal.get(at).copied().unwrap_or(false)
            || at
                .checked_sub(1)
                .and_then(|i| self.literal.get(i))
                .copied()
                .unwrap_or(false)
    }
    fn source_scalars(&self, span: ByteSpan) -> Option<Range<usize>> {
        if span.start > span.end {
            return None;
        }
        Some(
            self.snapshot.coordinates.byte_scalar(span.start)?
                ..self.snapshot.coordinates.byte_scalar(span.end)?,
        )
    }
    pub fn context(&self, span: ByteSpan) -> Context {
        match self.snapshot.stamp.role {
            SourceRole::Note => return Context::Note,
            SourceRole::Research => return Context::Research,
            SourceRole::Manuscript => {}
        }
        let Some(at) = self.snapshot.coordinates.byte_scalar(span.start) else {
            return Context::Narration;
        };
        if self.footnotes.iter().any(|r| r.contains(&at)) {
            Context::Footnote
        } else if self.dialogue.iter().any(|r| r.contains(&at)) {
            Context::Dialogue
        } else {
            Context::Narration
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ColumnEncoding {
    Utf8Bytes,
    Scalars,
    Utf16,
}
fn mask(c: char) -> char {
    if c == '\n' || c == '\r' { c } else { ' ' }
}

/// Double-quoted passages and em-dash-led paragraphs are hints, not a claim to
/// solve dialogue attribution. Apostrophes/possessives never open quotations.
fn dialogue_ranges(chars: &[char], readable: &[bool]) -> Vec<Range<usize>> {
    let mut ranges = vec![];
    let mut open = None;
    let mut line_start = true;
    for (i, &c) in chars.iter().enumerate() {
        if line_start && c == '—' && readable[i] {
            let end = chars[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(chars.len(), |n| i + n);
            ranges.push(i..end);
        }
        if readable[i] && (c == '“' || c == '”' || c == '"') {
            if let Some(start) = open.take() {
                ranges.push(start..i + 1);
            } else if c != '”' {
                open = Some(i);
            }
        }
        if c == '\n' {
            line_start = true;
        } else if !c.is_whitespace() {
            line_start = false;
        }
    }
    if let Some(start) = open {
        ranges.push(start..chars.len());
    }
    ranges
}

#[derive(Debug, Clone)]
pub struct Replacement {
    pub text: String,
    /// False when applying would remove markup, an escape or another source
    /// construct. The finding remains readable; manual editing is available.
    pub source_safe: bool,
}
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub id: String,
    pub source: SourceStamp,
    pub configuration: String,
    pub rule: RuleKey,
    pub category: Category,
    pub severity: Severity,
    pub context: Context,
    pub span: ByteSpan,
    pub expected: String,
    pub message: String,
    pub replacements: Vec<Replacement>,
}
impl Diagnostic {
    pub fn new(
        projection: &Projection,
        options: &AnalysisOptions,
        rule: RuleKey,
        category: Category,
        span: ByteSpan,
        message: String,
        alternatives: Vec<String>,
    ) -> Result<Self> {
        rule.validate()?;
        ensure!(
            projection.readable_span(span),
            "finding has no locatable prose"
        );
        let expected = projection
            .snapshot
            .slice(span)
            .ok_or_else(|| anyhow::anyhow!("invalid original-source span"))?
            .to_owned();
        ensure!(
            message.len() <= 4096
                && !message
                    .chars()
                    .any(|c| c.is_control() && !c.is_whitespace()),
            "invalid diagnostic message"
        );
        let source_safe = projection.replaceable_span(span);
        let mut replacements = vec![];
        for text in alternatives.into_iter().take(8) {
            if text.len() > MAX_REPLACEMENT_BYTES
                || text.chars().any(|c| c.is_control())
                || text == expected
            {
                continue;
            }
            if !replacements.iter().any(|r: &Replacement| r.text == text) {
                replacements.push(Replacement { text, source_safe });
            }
        }
        let source = projection.snapshot.stamp.clone();
        let configuration = options.digest();
        let id = hex_digest(
            format!(
                "{}\0{:?}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
                source.document,
                source.unit,
                source.sha256,
                source.generation,
                configuration,
                rule.engine,
                rule.version,
                rule.rule,
                format_args!("{}:{}", span.start, span.end)
            )
            .as_bytes(),
        );
        Ok(Self {
            id,
            source,
            configuration,
            rule,
            category,
            severity: if matches!(category, Category::Style | Category::Readability) {
                Severity::Information
            } else {
                Severity::Suggestion
            },
            context: projection.context(span),
            span,
            expected,
            message,
            replacements,
        })
    }
    /// The application must additionally check its exclusive document lease,
    /// read-only/disk conflict state, and current generation before editing.
    pub fn checked_replacement<'a>(
        &'a self,
        current: &Snapshot,
        options: &AnalysisOptions,
        alternative: usize,
    ) -> Result<(ByteSpan, &'a str)> {
        ensure!(
            self.source == current.stamp && self.configuration == options.digest(),
            "stale diagnostic; check this revision again"
        );
        ensure!(
            current.slice(self.span) == Some(self.expected.as_str()),
            "source no longer matches the suggestion"
        );
        let replacement = self
            .replacements
            .get(alternative)
            .ok_or_else(|| anyhow::anyhow!("no such diagnostic replacement"))?;
        if !replacement.source_safe {
            bail!("replacement crosses Markdown or an escape; edit it manually");
        }
        // Re-evaluate the policy and source structure rather than trusting a
        // cached boolean if a caller constructs a new snapshot/options pair.
        ensure!(
            current.project(options).replaceable_span(self.span),
            "replacement is not safe in the original source"
        );
        Ok((self.span, replacement.text.as_str()))
    }
}

pub fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
