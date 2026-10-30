//! Bounded, public-source footnote parsing shared by native publishing adapters.
use crate::notes;
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

pub(super) fn with_scene<T>(
    source: &str,
    render: impl for<'a> FnOnce(&[Event<'a>], &HashMap<String, Vec<Event<'a>>>) -> Result<T>,
) -> Result<Option<T>> {
    if !source.contains("[^") {
        return Ok(None);
    }
    ensure!(
        source.len() <= 2 * 1024 * 1024 && source.matches("[^").count() <= 8192,
        "footnoted scene exceeds 2 MiB or 8192 markers; choose Markdown only"
    );
    let public = notes::strip_notes(source);
    let analysis = crate::footnotes::analyze_footnotes(&public);
    if analysis.references.is_empty() && analysis.definitions.is_empty() {
        return Ok(None);
    }
    ensure!(
        analysis.missing_definitions.is_empty() && analysis.duplicate_definitions.is_empty(),
        "resolve missing or duplicate scene footnotes before rich export; choose Markdown only to preserve source"
    );
    ensure!(
        analysis.unused_definitions.is_empty(),
        "unreferenced footnote definitions cannot become page footnotes; cite or remove them, or choose Markdown only"
    );
    ensure!(
        analysis
            .definitions
            .values()
            .all(|d| d.content.len() <= 64 * 1024),
        "footnote definition exceeds 64 KiB; choose Markdown only"
    );
    let mut definitions: HashMap<String, Vec<Event<'_>>> = HashMap::new();
    let mut body = Vec::new();
    let mut capture: Option<(String, Vec<Event<'_>>)> = None;
    // Recognize tables so the unsupported-structure branch refuses them
    // rather than emitting pipe-delimited text as a successful layout.
    for event in Parser::new_ext(&public, Options::ENABLE_FOOTNOTES | Options::ENABLE_TABLES) {
        match event {
            Event::Start(Tag::FootnoteDefinition(label)) => {
                ensure!(
                    capture.is_none(),
                    "nested footnote definition is not supported"
                );
                capture = Some((crate::footnotes::normalize(&label), Vec::new()));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                let (key, events) = capture
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("footnote definition unavailable"))?;
                definitions.insert(key, events);
            }
            event => {
                if let Some((_, events)) = &mut capture {
                    events.push(event);
                } else {
                    body.push(event);
                }
            }
        }
    }
    Ok(Some(render(&body, &definitions)?))
}
