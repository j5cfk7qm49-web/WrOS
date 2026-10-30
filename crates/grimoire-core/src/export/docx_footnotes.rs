//! Manuscript footnotes over the maintained Markdown parser and existing DOCX
//! package writer. No source rewrite, external resource fetch or converter.
use super::*;
use anyhow::ensure;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Footnotes {
    notes: Vec<String>,
    bytes: usize,
}

impl Footnotes {
    pub fn scene(
        &mut self,
        source: &str,
        front: bool,
        look: &Look,
        first: &mut bool,
    ) -> Result<Option<Vec<Para>>> {
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
            "resolve missing or duplicate scene footnotes before Word/PDF export; choose Markdown only to preserve source"
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
        for event in Parser::new_ext(&public, Options::ENABLE_FOOTNOTES) {
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
        Ok(Some(self.render(
            &body,
            &definitions,
            false,
            front,
            look,
            first,
        )?))
    }

    fn render<'a>(
        &mut self,
        events: &[Event<'a>],
        definitions: &HashMap<String, Vec<Event<'a>>>,
        note: bool,
        front: bool,
        look: &Look,
        first: &mut bool,
    ) -> Result<Vec<Para>> {
        let mut out = Vec::new();
        let mut current: Option<Para> = None;
        let mut emphasis = 0usize;
        let mut strong = 0usize;
        let mut quote = 0usize;
        let mut heading = false;
        let mut heading_level = 1usize;
        let mut code = false;
        let mut links = Vec::new();
        let flush = |current: &mut Option<Para>, out: &mut Vec<Para>| {
            if let Some(p) = current.take() {
                out.push(p);
            }
        };
        for event in events {
            match event {
                Event::Start(Tag::Paragraph) => {
                    flush(&mut current, &mut out);
                }
                Event::End(TagEnd::Paragraph) => {
                    flush(&mut current, &mut out);
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(&mut current, &mut out);
                    heading = true;
                    heading_level = *level as usize;
                    *first = true;
                }
                Event::End(TagEnd::Heading(_)) => {
                    if current.is_none() {
                        out.push(Para::new(
                            if note {
                                "FootnoteText"
                            } else if heading_level == 1 {
                                "SceneBreak"
                            } else {
                                "Normal"
                            },
                            docx_text(&"#".repeat(heading_level)),
                        ));
                    }
                    flush(&mut current, &mut out);
                    heading = false;
                    *first = true;
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    quote += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(&mut current, &mut out);
                    quote = quote.saturating_sub(1);
                    *first = true;
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    flush(&mut current, &mut out);
                    code = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    flush(&mut current, &mut out);
                    code = false;
                }
                Event::Start(Tag::Emphasis) => emphasis += 1,
                Event::End(TagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                Event::Start(Tag::Strong) => strong += 1,
                Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
                Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.to_string()),
                Event::End(TagEnd::Link) => {
                    let url = links
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("link target unavailable"))?;
                    if !url.is_empty() {
                        let p = paragraph(&mut current, note, front, heading, quote, look, first);
                        p.runs.push_str(&run(
                            &format!(" ({url})"),
                            look,
                            strong > 0,
                            emphasis > 0,
                            false,
                        ));
                    }
                }
                Event::Text(text) | Event::Code(text) => {
                    let p = paragraph(&mut current, note, front, heading, quote, look, first);
                    p.runs.push_str(&run(
                        text,
                        look,
                        strong > 0 || (note && heading),
                        emphasis > 0,
                        code || matches!(event, Event::Code(_)),
                    ));
                }
                Event::SoftBreak if !note => flush(&mut current, &mut out),
                Event::SoftBreak => {
                    paragraph(&mut current, note, front, heading, quote, look, first)
                        .runs
                        .push_str(&run(" ", look, strong > 0, emphasis > 0, false));
                }
                Event::HardBreak => {
                    paragraph(&mut current, note, front, heading, quote, look, first)
                        .runs
                        .push_str("<w:r><w:br/></w:r>");
                }
                Event::Rule => {
                    flush(&mut current, &mut out);
                    out.push(Para::new(
                        if note { "FootnoteText" } else { "SceneBreak" },
                        docx_text("#"),
                    ));
                    *first = true;
                }
                Event::FootnoteReference(label) => {
                    ensure!(
                        !note,
                        "nested footnote references cannot be rendered in Word/PDF; choose Markdown only"
                    );
                    ensure!(
                        self.notes.len() < 4096,
                        "document exceeds 4096 page footnotes"
                    );
                    let definition = definitions
                        .get(&crate::footnotes::normalize(label))
                        .ok_or_else(|| anyhow::anyhow!("footnote definition unavailable"))?;
                    let id = self.notes.len() + 1;
                    let mut first_note = false;
                    let mut paragraphs =
                        self.render(definition, definitions, true, true, look, &mut first_note)?;
                    ensure!(
                        !paragraphs.is_empty(),
                        "empty page-footnote definition; choose Markdown only"
                    );
                    paragraphs[0].runs.insert_str(0,"<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> </w:t></w:r>");
                    let content = paragraphs
                        .iter()
                        .map(|p| p.xml(None, 240))
                        .collect::<String>();
                    ensure!(
                        self.bytes.saturating_add(content.len()) <= 16 * 1024 * 1024,
                        "page footnotes exceed 16 MiB rendered content"
                    );
                    self.bytes += content.len();
                    self.notes.push(format!(
                        "<w:footnote w:id=\"{id}\">{content}</w:footnote>\n"
                    ));
                    let p = paragraph(&mut current, false, front, heading, quote, look, first);
                    let _ = write!(
                        p.runs,
                        "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"{id}\"/></w:r>"
                    );
                }
                Event::Html(_)
                | Event::InlineHtml(_)
                | Event::Start(_)
                | Event::End(_)
                | Event::TaskListMarker(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_) => {
                    bail!(
                        "this footnoted scene contains an unsupported Markdown structure (such as a list, image, table or raw HTML); choose Markdown only"
                    );
                }
            }
        }
        flush(&mut current, &mut out);
        Ok(out)
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
    pub fn len(&self) -> usize {
        self.notes.len()
    }
    pub fn xml(&self) -> String {
        format!(
            "{XML_HEAD}<w:footnotes xmlns:w=\"{W_NS}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{}</w:footnotes>\n",
            self.notes.concat()
        )
    }
}

fn paragraph<'a>(
    current: &'a mut Option<Para>,
    note: bool,
    front: bool,
    heading: bool,
    quote: usize,
    look: &Look,
    first: &mut bool,
) -> &'a mut Para {
    current.get_or_insert_with(|| {
        let style = if note {
            "FootnoteText"
        } else if heading {
            "Centered"
        } else if quote > 0 {
            "Quote"
        } else if front || (*first && look.flush_first) {
            "Unindented"
        } else {
            "Normal"
        };
        if !heading {
            *first = false;
        }
        Para::new(style, String::new())
    })
}
fn run(text: &str, look: &Look, bold: bool, italic: bool, code: bool) -> String {
    let mut out = String::from("<w:r><w:rPr>");
    if bold {
        out.push_str("<w:b/>");
    }
    if italic {
        out.push_str(if look.classic {
            "<w:u w:val=\"single\"/>"
        } else {
            "<w:i/>"
        });
    }
    if code {
        out.push_str("<w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\"/>");
    }
    out.push_str("</w:rPr>");
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push_str("<w:br/>");
        }
        let _ = write!(
            out,
            "<w:t xml:space=\"preserve\">{}</w:t>",
            xml_escape(&look.text(line))
        );
    }
    out.push_str("</w:r>");
    out
}
