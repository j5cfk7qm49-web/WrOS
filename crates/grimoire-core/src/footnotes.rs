//! Markdown contextual footnote parser and validator.
//! Detects `[^label]` inline references and `[^label]: ...` definitions.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteRef {
    pub label: String,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteDef {
    pub label: String,
    pub content: String,
    pub line_number: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FootnoteAnalysis {
    pub references: Vec<FootnoteRef>,
    pub definitions: HashMap<String, FootnoteDef>,
    pub missing_definitions: Vec<String>,
    pub unused_definitions: Vec<String>,
    pub duplicate_definitions: Vec<String>,
}

pub fn analyze_footnotes(markdown: &str) -> FootnoteAnalysis {
    let mut analysis = FootnoteAnalysis::default();
    let mut visible: Vec<Range<usize>> = Vec::new();
    let mut code = false;
    // Preserve source offsets when removing private notes, so a definition
    // inside a non-exported comment cannot satisfy a public reference.
    let mut masked = markdown.as_bytes().to_vec();
    for (start, end) in crate::notes::note_ranges(markdown) {
        for byte in &mut masked[start..end] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    let parsed = String::from_utf8(masked).expect("masking complete UTF-8 ranges with ASCII");
    for (event, range) in Parser::new_ext(&parsed, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Start(Tag::FootnoteDefinition(label)) => {
                let key = normalize(&label);
                let raw = &markdown[range.clone()];
                let content = raw
                    .find("]:")
                    .map_or("", |at| &raw[at + 2..])
                    .trim()
                    .to_string();
                let definition = FootnoteDef {
                    label: label.to_string(),
                    content,
                    line_number: markdown[..range.start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1,
                };
                match analysis.definitions.entry(key) {
                    std::collections::hash_map::Entry::Occupied(entry) => {
                        analysis.duplicate_definitions.push(entry.key().clone());
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(definition);
                    }
                }
            }
            Event::FootnoteReference(label) => analysis.references.push(FootnoteRef {
                label: label.to_string(),
                offset: range.start,
            }),
            Event::Text(_) if !code => {
                if let Some(last) = visible.last_mut().filter(|last| last.end == range.start) {
                    last.end = range.end;
                } else {
                    visible.push(range);
                }
            }
            _ => {}
        }
    }
    // The maintained GFM parser renders undefined references as literal Text.
    // Inspect only those source Text ranges, never code, HTML or link targets.
    for range in visible {
        let raw = &parsed[range.clone()];
        let mut at = 0;
        while let Some(relative) = raw[at..].find("[^") {
            let start = at + relative;
            let Some(end) = raw[start + 2..].find(']').map(|n| start + 2 + n) else {
                break;
            };
            at = end + 1;
            let absolute = range.start + start;
            let slashes = parsed.as_bytes()[..absolute]
                .iter()
                .rev()
                .take_while(|b| **b == b'\\')
                .count();
            if slashes % 2 == 1 {
                continue;
            }
            let label = &raw[start + 2..end];
            if label.is_empty() || label.chars().any(|c| c == '\n' || c == '\r' || c == '[') {
                continue;
            }
            analysis.references.push(FootnoteRef {
                label: label.into(),
                offset: absolute,
            });
        }
    }
    analysis.references.sort_by_key(|r| r.offset);
    analysis.references.dedup_by_key(|r| r.offset);
    for reference in &analysis.references {
        let key = normalize(&reference.label);
        if !analysis.definitions.contains_key(&key) && !analysis.missing_definitions.contains(&key)
        {
            analysis.missing_definitions.push(key);
        }
    }
    for label in analysis.definitions.keys() {
        if !analysis
            .references
            .iter()
            .any(|r| normalize(&r.label) == *label)
        {
            analysis.unused_definitions.push(label.clone());
        }
    }
    analysis.unused_definitions.sort();
    analysis.duplicate_definitions.sort();
    analysis.duplicate_definitions.dedup();
    analysis
}

pub(crate) fn normalize(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintained_markdown_context_excludes_code_escapes_links_and_private_definitions() {
        let text = "`[^inline]` \\[^escaped] [link](https://example.invalid/[^url])\n\n```\n[^fenced]\n[^not-a-definition]: code\n```\n\nActual[^actual]. %% [^actual]: private %%\n";
        let analysis = analyze_footnotes(text);
        assert_eq!(
            analysis
                .references
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            vec!["actual"]
        );
        assert_eq!(analysis.missing_definitions, vec!["actual"]);
        assert!(analysis.definitions.is_empty());
    }
    #[test]
    fn multiline_definitions_case_matching_and_duplicate_labels_are_reviewed() {
        let text = "Claim[^Source].\n\n[^source]: First paragraph.\n\n    Continued λ paragraph.\n\n[^source]: Duplicate.\n";
        let analysis = analyze_footnotes(text);
        assert!(analysis.missing_definitions.is_empty());
        assert!(analysis.unused_definitions.is_empty());
        assert_eq!(analysis.duplicate_definitions, vec!["source"]);
        assert!(
            analysis.definitions["source"]
                .content
                .contains("Continued λ paragraph.")
        );
        for r in analysis.references {
            assert!(text.is_char_boundary(r.offset));
        }
    }

    #[test]
    fn parses_and_matches_footnotes() {
        let text = "Here is a claim[^1] and another detail[^source].\n\n[^1]: First footnote.\n[^source]: Second footnote.";
        let analysis = analyze_footnotes(text);
        assert_eq!(analysis.references.len(), 2);
        assert_eq!(analysis.definitions.len(), 2);
        assert!(analysis.missing_definitions.is_empty());
        assert!(analysis.unused_definitions.is_empty());
    }

    #[test]
    fn detects_missing_definitions() {
        let text = "A claim with no backing[^missing].";
        let analysis = analyze_footnotes(text);
        assert_eq!(analysis.missing_definitions, vec!["missing"]);
    }

    #[test]
    fn detects_unused_definitions() {
        let text = "Prose without notes.\n\n[^orphan]: I was never linked.";
        let analysis = analyze_footnotes(text);
        assert_eq!(analysis.unused_definitions, vec!["orphan"]);
    }
}
