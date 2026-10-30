//! Bounded read-only context over the maintained Markdown footnote parser.
use crate::footnotes::{FootnoteAnalysis, analyze_footnotes, normalize};
use anyhow::{Result, ensure};

pub const MAX_SCENE: usize = 2 * 1024 * 1024;
pub const MAX_DEFINITION: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub key: String,
    pub definition: String,
    pub status: &'static str,
    pub references: usize,
}

fn inspect(text: &str) -> Result<FootnoteAnalysis> {
    ensure!(
        text.len() <= MAX_SCENE && text.matches("[^").count() <= 8192,
        "footnote context limit: 2 MiB/scene and 8192 markers; open the original source"
    );
    Ok(analyze_footnotes(text))
}
fn from_analysis(analysis: &FootnoteAnalysis, key: String) -> Result<Context> {
    let references = analysis
        .references
        .iter()
        .filter(|r| normalize(&r.label) == key)
        .count();
    let (status, definition) = if analysis.duplicate_definitions.contains(&key) {
        ("duplicate", "Multiple definitions share this label. Review the original source; no definition is chosen.".into())
    } else if let Some(definition) = analysis.definitions.get(&key) {
        ensure!(
            definition.content.len() <= MAX_DEFINITION,
            "definition exceeds 64 KiB context limit; open the original source"
        );
        (
            if references == 0 { "unused" } else { "defined" },
            definition.content.clone(),
        )
    } else {
        (
            "missing",
            "No public definition in this scene. Edit the original Markdown to define this label."
                .into(),
        )
    };
    Ok(Context {
        key,
        definition,
        status,
        references,
    })
}
pub fn at_caret(text: &str, byte: usize) -> Result<Context> {
    ensure!(
        byte <= text.len() && text.is_char_boundary(byte),
        "invalid source caret"
    );
    let analysis = inspect(text)?;
    ensure!(
        !crate::notes::note_ranges(text)
            .iter()
            .any(|&(a, b)| a <= byte && byte < b),
        "caret is inside a private note"
    );
    let key = analysis
        .references
        .iter()
        .find(|r| r.offset <= byte && byte < r.end_offset)
        .or_else(|| analysis.references.iter().find(|r| r.end_offset == byte))
        .map(|r| normalize(&r.label))
        .or_else(|| {
            analysis
                .definition_ranges
                .iter()
                .find(|(_, span)| span.contains(&byte) || (byte == text.len() && span.end == byte))
                .map(|(key, _)| key.clone())
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "place the caret on a public footnote reference or inside its definition"
            )
        })?;
    from_analysis(&analysis, key)
}
pub fn for_label(text: &str, label: &str) -> Result<Context> {
    let analysis = inspect(text)?;
    let key = normalize(label);
    ensure!(
        analysis.definitions.contains_key(&key)
            || analysis
                .references
                .iter()
                .any(|r| normalize(&r.label) == key),
        "footnote no longer exists"
    );
    from_analysis(&analysis, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_reference_boundary_and_definition_eof_select_the_current_note() {
        let text = "[^a][^b]\n\n[^a]: A.\n[^b]: B.";
        assert_eq!(at_caret(text, 4).unwrap().key, "b");
        assert_eq!(at_caret(text, text.len()).unwrap().key, "b");
    }
    #[test]
    fn caret_lookup_uses_original_unicode_ranges_and_all_definition_occurrences() {
        let text =
            "λ[^One] and again[^one].\n\n[^one]: First 🦊 paragraph.\n\n    Second paragraph.\n";
        for byte in [2, text.find("]").unwrap() + 1, text.find("Second").unwrap()] {
            let c = at_caret(text, byte).unwrap();
            assert_eq!(c.status, "defined");
            assert_eq!(c.references, 2);
            assert!(c.definition.contains("Second paragraph."));
        }
        let duplicate = format!("{text}\n[^ONE]: Another definition.\n");
        assert_eq!(
            at_caret(&duplicate, duplicate.find("Another").unwrap())
                .unwrap()
                .status,
            "duplicate"
        );
        assert!(at_caret(text, 1).is_err());
    }
    #[test]
    fn public_context_excludes_private_cross_boundary_text_code_and_link_targets() {
        let text = "Reference[^a]. `[^code]` [link](https://example.invalid/[^url])\n\n[^a]: Visible. %% PRIVATE\n\noutside secret %%\n";
        let c = at_caret(text, text.find("[^a]").unwrap()).unwrap();
        assert!(!c.definition.contains("PRIVATE") && !c.definition.contains("secret"));
        assert!(at_caret(text, text.find("PRIVATE").unwrap()).is_err());
        assert!(at_caret(text, text.find("[^code]").unwrap() + 2).is_err());
        assert!(at_caret(text, text.find("[^url]").unwrap() + 2).is_err());
    }
    #[test]
    fn missing_unused_and_oversized_contexts_never_fabricate_a_definition() {
        assert_eq!(at_caret("Text[^missing].", 5).unwrap().status, "missing");
        assert_eq!(
            at_caret("[^unused]: λ original", 14).unwrap().status,
            "unused"
        );
        assert!(for_label("No notes", "missing").is_err());
        let text = format!(
            "Text[^long].\n\n[^long]: {}",
            "x".repeat(MAX_DEFINITION + 1)
        );
        assert!(at_caret(&text, 5).is_err());
    }
}
