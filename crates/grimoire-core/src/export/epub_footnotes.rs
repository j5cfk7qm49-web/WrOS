//! EPUB notes over the shared parser and maintained pulldown-cmark HTML renderer.
//! Only controlled markup is injected; manuscript HTML and resources are refused.
use super::*;
use anyhow::ensure;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd, html};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Footnotes {
    count: usize,
    bytes: usize,
}
impl Footnotes {
    pub fn css(&self) -> String {
        if self.count == 0 {
            return EPUB_CSS.into();
        }
        format!(
            "{EPUB_CSS}\n.footnotes {{ margin-top: 2em; border-top: thin solid; }}\n.footnotes aside {{ margin: 1em 0; }}\n.footnotes p {{ text-indent: 0; margin-bottom: 0.5em; }}\na[role=doc-noteref] {{ text-decoration: none; }}\n"
        )
    }
    pub fn scene(&mut self, source: &str, hyphen: typeset::SpacedHyphen) -> Result<String> {
        Ok(super::footnote_source::with_scene(source, |body, definitions| {
            let mut notes = String::new();
            let mut body = self.render(body, definitions, hyphen, false, &mut notes)?;
            if !notes.is_empty() {
                body.push_str("<section class=\"footnotes\" epub:type=\"footnotes\" aria-label=\"Footnotes\">\n");
                body.push_str(&notes);
                body.push_str("</section>\n");
            }
            Ok(body)
        })?.unwrap_or_else(|| html_blocks(source)))
    }

    fn render<'a>(
        &mut self,
        events: &[Event<'a>],
        definitions: &HashMap<String, Vec<Event<'a>>>,
        hyphen: typeset::SpacedHyphen,
        note: bool,
        notes: &mut String,
    ) -> Result<String> {
        let mut output = Vec::new();
        let mut previous = None;
        let mut code = false;
        let mut first = true;
        let mut inlines = Vec::new();
        let mut links = Vec::new();
        let mut visible = false;
        let mut empty_heading_end = false;
        for (index, event) in events.iter().enumerate() {
            match event {
                Event::Start(Tag::Paragraph) => {
                    previous = None;
                    output.push(paragraph(first));
                    first = false;
                }
                Event::End(TagEnd::Paragraph) => output.push(markup("</p>\n")),
                Event::Start(Tag::Heading { .. }) => {
                    first = true;
                    previous = None;
                    empty_heading_end =
                        matches!(events.get(index + 1), Some(Event::End(TagEnd::Heading(_))));
                    if empty_heading_end {
                        output.push(markup("<p class=\"break\">* * *</p>\n"));
                        visible = true;
                    } else {
                        output.push(Event::Start(Tag::Heading {
                            level: HeadingLevel::H3,
                            id: None,
                            classes: vec![],
                            attrs: vec![],
                        }));
                    }
                }
                Event::End(TagEnd::Heading(_)) => {
                    if !empty_heading_end {
                        output.push(Event::End(TagEnd::Heading(HeadingLevel::H3)));
                    }
                    empty_heading_end = false;
                    first = true;
                }
                Event::Start(Tag::BlockQuote(_)) | Event::End(TagEnd::BlockQuote(_)) => {
                    output.push(event.clone());
                    first = true;
                    previous = None;
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    code = true;
                    previous = None;
                    output.push(event.clone());
                }
                Event::End(TagEnd::CodeBlock) => {
                    code = false;
                    output.push(event.clone());
                }
                Event::Start(Tag::Emphasis | Tag::Strong) => {
                    if let Event::Start(tag) = event {
                        inlines.push(tag.clone());
                    }
                    output.push(event.clone());
                }
                Event::End(TagEnd::Emphasis | TagEnd::Strong) => {
                    inlines.pop();
                    output.push(event.clone());
                }
                // Print the exact target as readable text. Do not fetch or
                // create unqualified relative-resource links in the package.
                Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.clone()),
                Event::End(TagEnd::Link) => {
                    let url = links
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("link target unavailable"))?;
                    if !url.is_empty() {
                        let text = format!(" ({url})");
                        previous = text.chars().last();
                        output.push(Event::Text(text.into()));
                    }
                }
                Event::Text(text) | Event::Code(text) => {
                    visible |= !text.trim().is_empty();
                    if code || matches!(event, Event::Code(_)) {
                        previous = text.chars().last().or(previous);
                        output.push(event.clone());
                    } else {
                        let text = typeset::line_after(text, hyphen, previous).into_owned();
                        previous = text.chars().last().or(previous);
                        output.push(Event::Text(text.into()));
                    }
                }
                // Grimoire's manuscript convention treats body lines as
                // paragraphs. Balance styles across a line, preserving the
                // parser's paragraph structure inside the note itself.
                Event::SoftBreak if !note && !code => {
                    for tag in inlines.iter().rev() {
                        output.push(Event::End(tag.to_end()));
                    }
                    output.push(markup("</p>\n"));
                    output.push(paragraph(false));
                    for tag in &inlines {
                        output.push(Event::Start(tag.clone()));
                    }
                    previous = None;
                }
                Event::SoftBreak | Event::HardBreak | Event::Rule => {
                    output.push(event.clone());
                    previous = None;
                    if matches!(event, Event::Rule) {
                        first = true;
                        visible = true;
                    }
                }
                Event::FootnoteReference(label) => {
                    ensure!(
                        !note,
                        "nested footnote references cannot be rendered in EPUB; choose Markdown only"
                    );
                    ensure!(self.count < 4096, "document exceeds 4096 EPUB footnotes");
                    let definition = definitions
                        .get(&crate::footnotes::normalize(label))
                        .ok_or_else(|| anyhow::anyhow!("footnote definition unavailable"))?;
                    self.count += 1;
                    let id = self.count;
                    let content =
                        self.render(definition, definitions, hyphen, true, &mut String::new())?;
                    let content = format!(
                        "<aside id=\"fn-{id}\" epub:type=\"footnote\" role=\"doc-footnote\" aria-label=\"Footnote {id}\">\n{content}<p class=\"first\"><a href=\"#ref-{id}\" epub:type=\"backlink\" role=\"doc-backlink\" aria-label=\"Return to reference {id}\">↩ {id}</a></p>\n</aside>\n"
                    );
                    ensure!(
                        self.bytes.saturating_add(content.len()) <= 16 * 1024 * 1024,
                        "EPUB footnotes exceed 16 MiB rendered content"
                    );
                    self.bytes += content.len();
                    notes.push_str(&content);
                    output.push(Event::InlineHtml(format!("<a id=\"ref-{id}\" href=\"#fn-{id}\" epub:type=\"noteref\" role=\"doc-noteref\" aria-label=\"Footnote {id}\"><sup>{id}</sup></a>").into()));
                    visible = true;
                }
                _ => bail!(
                    "this footnoted scene contains an unsupported Markdown structure (such as a list, image, table or raw HTML); choose Markdown only"
                ),
            }
        }
        ensure!(
            !note || visible,
            "empty EPUB footnote definition; choose Markdown only"
        );
        let mut rendered = String::new();
        html::push_html(&mut rendered, output.into_iter());
        Ok(rendered)
    }
}
fn markup(text: &'static str) -> Event<'static> {
    Event::InlineHtml(text.into())
}
fn paragraph(first: bool) -> Event<'static> {
    markup(if first { "<p class=\"first\">" } else { "<p>" })
}
