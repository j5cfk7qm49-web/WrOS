//! Prepare the same public Markdown and MLA citation runs for Word and PDF.

use anyhow::{Context, Result, ensure};
use hayagriva::Formatted;
use hayagriva::citationberg::{FontStyle, FontWeight};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

use super::*;
use crate::citations::{self, Cite};
use crate::preflight::{PreflightIssue, PreflightSeverity};
use crate::sources;

const MARK: &str = "\u{e000}";

struct Paragraph {
    style: &'static str,
    items: Vec<Inline>,
}

enum Inline {
    Text(String, bool, bool),
    Citation(usize),
    Footnote(Vec<Paragraph>),
    Break,
}

struct Paper<'a> {
    details: &'a crate::project::MlaDetails,
    title: &'a str,
    paragraphs: Vec<Paragraph>,
    citations: citations::mla::PaperCitations,
}

pub(super) fn issues(p: &Project, b: &Book, options: &ExportOptions) -> Vec<PreflightIssue> {
    let mla = p.meta.uses_mla();
    let rich = options.docx || options.pdf || options.epub || options.paperback;
    let requirement = |message: String| PreflightIssue {
        severity: PreflightSeverity::Error,
        category: "MLA 9".into(),
        scene_id: None,
        scene_title: None,
        line: None,
        message,
    };
    let mut issues = Vec::new();
    if mla && rich {
        if options.epub || options.paperback {
            issues.push(requirement(
                "MLA 9 papers export to Word, PDF or Markdown.".into(),
            ));
        }
        for (label, value) in p.meta.mla.fields() {
            if value.trim().is_empty() {
                issues.push(requirement(format!(
                    "Enter {label} in MLA 9 paper details."
                )));
            }
        }
        if b.title.trim().is_empty() {
            issues.push(requirement("Enter a project title.".into()));
        }
    }
    let mut source_file = None;
    let mut checked_pending = false;
    for &i in &b.source_indices {
        let n = &p.nodes[i];
        if mla
            && rich
            && p.nodes.iter().any(|copy| {
                copy.path.parent() == n.path.parent()
                    && copy.copy_of.as_ref().is_some_and(|origin| {
                        n.path
                            .file_stem()
                            .is_some_and(|stem| stem == origin.original.as_str())
                    })
            })
        {
            let mut issue =
                requirement("Settle the manuscript's pending changes before export.".into());
            issue.scene_id = Some(i);
            issue.scene_title = Some(
                n.path
                    .strip_prefix(&p.root)
                    .unwrap_or(&n.path)
                    .display()
                    .to_string(),
            );
            issues.push(issue);
        }
        let kept = notes::note_ranges(&n.body);
        for marker in citations::markers(&n.body) {
            if kept
                .iter()
                .any(|&(a, z)| marker.range.start < z && a < marker.range.end)
            {
                continue;
            }
            let mut issue = PreflightIssue {
                severity: if rich {
                    PreflightSeverity::Error
                } else {
                    PreflightSeverity::Warning
                },
                category: "Citations".into(),
                scene_id: Some(i),
                scene_title: Some(
                    n.path
                        .strip_prefix(&p.root)
                        .unwrap_or(&n.path)
                        .display()
                        .to_string(),
                ),
                line: Some(
                    1 + n.body[..marker.range.start]
                        .bytes()
                        .filter(|&c| c == b'\n')
                        .count(),
                ),
                message: String::new(),
            };
            if rich && !checked_pending {
                checked_pending = true;
                match sources::parked_sources(&p.root) {
                    Ok(pending) if !pending.is_empty() => issues.push(requirement(
                        "Settle the Drive conflict in project sources before formatting citations."
                            .into(),
                    )),
                    Err(e) => issues.push(requirement(format!(
                        "Could not check project source conflicts: {e}"
                    ))),
                    _ => (),
                }
            }
            if rich && !mla {
                issue.message = "Select the MLA 9 paper layout to format citations.".into();
            } else if let Err(reason) = marker.items {
                issue.message = format!("{reason}.");
            } else if let Ok(items) = marker.items {
                let sources = source_file.get_or_insert_with(|| sources::Snapshot::open(&p.root));
                match sources {
                    Ok(s) => {
                        let missing = items
                            .iter()
                            .filter(|c| s.sources.get(c.key).is_none())
                            .map(|c| c.key)
                            .collect::<Vec<_>>();
                        if !missing.is_empty() {
                            issue.message =
                                format!("Add project sources for {}.", missing.join(", "));
                        }
                    }
                    Err(e) => issue.message = format!("{e:#}"),
                }
            }
            if !issue.message.is_empty() {
                issues.push(issue);
            }
            if issues.len() >= 512 {
                return issues;
            }
        }
    }
    issues
}

pub(super) fn validate(p: &Project, b: &Book) -> Result<()> {
    prepare(p, b).map(|_| ())
}

fn prepare<'a>(p: &'a Project, b: &Book) -> Result<Paper<'a>> {
    let mut paragraphs = Vec::new();
    let public: Vec<_> = b
        .source_indices
        .iter()
        .map(|&i| notes::strip_notes(&p.nodes[i].body))
        .collect();
    let mark = (1..)
        .map(|n| MARK.repeat(n))
        .find(|mark| public.iter().all(|scene| !scene.contains(mark)))
        .context("Could not reserve citation positions")?;
    let mut ordered = Vec::new();
    for (&id, text) in b.source_indices.iter().zip(&public) {
        let mut saved = Vec::new();
        let mut masked = String::new();
        let mut start = 0;
        for cite in citations::markers(text) {
            let items = cite.items.map_err(|e| anyhow::anyhow!("{e}"))?;
            masked.push_str(&text[start..cite.range.start]);
            write!(masked, "{mark}{}{mark}", saved.len())?;
            saved.push(items);
            start = cite.range.end;
        }
        masked.push_str(&text[start..]);
        let scene = footnote_source::with_scene(&masked, |events, definitions| {
            parse(events, definitions, &mark, &saved, &mut ordered, false)
        })
        .and_then(|parsed| match parsed {
            Some(paragraphs) => Ok(paragraphs),
            None => {
                let events: Vec<_> = Parser::new_ext(&masked, Options::ENABLE_TABLES).collect();
                parse(&events, &HashMap::new(), &mark, &saved, &mut ordered, false)
            }
        })
        .with_context(|| format!("Preparing {}", p.nodes[id].path.display()))?;
        paragraphs.extend(scene);
    }
    let sources = if ordered.is_empty() {
        sources::Sources::default()
    } else {
        sources::Snapshot::open(&p.root)?.sources
    };
    let citations = citations::mla::render(&sources, &ordered)?;
    Ok(Paper {
        details: &p.meta.mla,
        title: &p.meta.title,
        paragraphs,
        citations,
    })
}

fn parse<'a>(
    events: &[Event<'_>],
    notes: &HashMap<String, Vec<Event<'_>>>,
    mark: &str,
    saved: &[Vec<Cite<'a>>],
    ordered: &mut Vec<Vec<Cite<'a>>>,
    in_note: bool,
) -> Result<Vec<Paragraph>> {
    let mut out = Vec::new();
    let mut style = if in_note { "FootnoteText" } else { "Normal" };
    let mut items = Vec::new();
    let mut italic = 0usize;
    let mut bold = 0usize;
    let mut quote = 0usize;
    let mut links = Vec::new();
    let flush = |items: &mut Vec<Inline>, out: &mut Vec<Paragraph>, style| {
        if !items.is_empty() {
            out.push(Paragraph {
                style,
                items: std::mem::take(items),
            });
        }
    };
    for event in events {
        match event {
            Event::Start(Tag::Paragraph) => (),
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock) => {
                flush(&mut items, &mut out, style);
                style = if in_note {
                    "FootnoteText"
                } else if quote > 0 {
                    "Quote"
                } else {
                    "Normal"
                };
            }
            Event::Start(Tag::Heading { .. }) => {
                if !in_note {
                    style = "Centered";
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                if !in_note {
                    style = "Unindented";
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quote += 1;
                if !in_note {
                    style = "Quote";
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote -= 1;
                if quote == 0 && !in_note {
                    style = "Normal";
                }
            }
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url),
            Event::End(TagEnd::Link) => {
                if let Some(url) = links.pop() {
                    items.push(Inline::Text(format!(" ({url})"), false, false));
                }
            }
            Event::Text(text) => {
                let mut parts = text.split(mark);
                while let Some(text) = parts.next() {
                    if !text.is_empty() {
                        items.push(Inline::Text(text.to_owned(), italic != 0, bold != 0));
                    }
                    if let Some(id) = parts.next() {
                        let id: usize = id
                            .parse()
                            .context("Incomplete citation marker during layout")?;
                        let group = saved
                            .get(id)
                            .context("Unknown citation marker during layout")?;
                        items.push(Inline::Citation(ordered.len()));
                        ordered.push(group.clone());
                    }
                }
            }
            Event::Code(text) => items.push(Inline::Text(text.to_string(), false, false)),
            Event::SoftBreak => items.push(Inline::Text(" ".into(), false, false)),
            Event::HardBreak => items.push(Inline::Break),
            Event::Rule => flush(&mut items, &mut out, style),
            Event::FootnoteReference(label) => {
                ensure!(!in_note, "Nested footnote references cannot be exported.");
                let definition = notes
                    .get(&crate::footnotes::normalize(label))
                    .context("Missing footnote definition")?;
                let content = parse(definition, notes, mark, saved, ordered, true)?;
                ensure!(!content.is_empty(), "A footnote needs text.");
                items.push(Inline::Footnote(content));
            }
            _ => bail!(
                "This Markdown structure cannot be laid out in MLA yet. Use Markdown to preserve it."
            ),
        }
    }
    flush(&mut items, &mut out, style);
    Ok(out)
}

pub(super) fn export(p: &Project, b: &Book, opts: &ExportOptions) -> Result<Exported> {
    let findings = issues(p, b, opts);
    if findings
        .iter()
        .any(|i| i.severity == PreflightSeverity::Error)
    {
        bail!(
            "{}",
            findings
                .iter()
                .map(|i| format!(
                    "{}{}",
                    i.scene_title
                        .as_ref()
                        .zip(i.line)
                        .map_or(String::new(), |(file, line)| format!("{file}:{line}: ")),
                    i.message
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let paper = if opts.docx || opts.pdf {
        Some(prepare(p, b)?)
    } else {
        None
    };
    let word = paper.as_ref().map(Paper::word).transpose()?;
    let dir = p.root.join("exports");
    std::fs::create_dir_all(&dir)?;
    let stem = format!(
        "{}_{}_MLA",
        crate::names::stem(&p.meta.mla.surname),
        crate::names::stem(&b.title)
    );
    let mut files = Vec::new();
    let mut pdf_error = None;
    let mut mla_document = None;
    if let Some((bytes, _)) = &word {
        let docx = dir.join(format!("{stem}.docx"));
        crate::atomic::write(&docx, bytes)?;
        files.push(docx.clone());
        if let Some(paper) = paper.as_ref().filter(|_| opts.pdf) {
            let path = docx.with_extension("pdf");
            match crate::pdf::typst(&paper.typst(), &path) {
                Ok(()) => files.push(path),
                Err(e) => {
                    pdf_error = Some(if path.is_file() {
                        format!("{e:#}. The existing PDF is from an earlier export.")
                    } else {
                        format!("{e:#}")
                    });
                }
            }
        }
    }
    if !opts.pdf
        && let (Some(paper), Some(path)) = (&paper, files.first())
    {
        mla_document = Some((paper.typst(), path.with_extension("pdf")));
    }
    if opts.markdown {
        let path = dir.join(format!("{}.md", slug(&b.title)));
        let raw = b
            .source_indices
            .iter()
            .map(|&i| notes::strip_notes(&p.nodes[i].body))
            .collect::<Vec<_>>()
            .join("\n\n");
        crate::atomic::write_text(&path, &raw)?;
        files.push(path);
    }
    Ok(Exported {
        files,
        words: b.words,
        chapters: b.chapters,
        pages: pages(b),
        tks: b.tks.clone(),
        empty: b.empty,
        pdf_error,
        notes: findings.into_iter().map(|i| i.message).collect(),
        manuscript_footnotes: word.map_or(0, |(_, n)| n),
        paperback_document: None,
        mla_document,
    })
}

impl Paper<'_> {
    fn word(&self) -> Result<(Vec<u8>, usize)> {
        let look = Look::of(&Manuscript::default());
        let mut paragraphs = self
            .details
            .fields()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, (_, value))| Para::new("Unindented", docx_text(value.trim())))
            .collect::<Vec<_>>();
        paragraphs.push(Para::new("Centered", docx_text(self.title)));
        let mut footnotes = docx_footnotes::Footnotes::default();
        for p in &self.paragraphs {
            paragraphs.push(self.word_para(p, &mut footnotes)?);
        }
        if !self.citations.works_cited.is_empty() {
            let mut heading = Para::new("Centered", docx_text("Works Cited"));
            heading.page_break = true;
            paragraphs.push(heading);
            for item in &self.citations.works_cited {
                paragraphs.push(Para::new(
                    "MlaWorksCited",
                    item.iter().map(word_citation).collect(),
                ));
            }
        }
        let mut doc =
            format!("{XML_HEAD}<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\"><w:body>\n");
        for para in paragraphs {
            doc.push_str(&para.xml(None, 480));
        }
        write!(
            doc,
            "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdHeader\"/>{}<w:pgNumType w:start=\"1\"/></w:sectPr></w:body></w:document>",
            look.page()
        )?;
        let header = page_header(&format!("{} ", self.details.surname.trim()));
        docx_package(
            self.title,
            self.details.name.trim(),
            doc,
            header,
            &look,
            footnotes,
            true,
        )
    }

    fn word_para(&self, p: &Paragraph, notes: &mut docx_footnotes::Footnotes) -> Result<Para> {
        let mut runs = String::new();
        for item in &p.items {
            match item {
                Inline::Text(text, italic, bold) => runs.push_str(&word_text(text, *italic, *bold)),
                Inline::Citation(i) => {
                    runs.extend(self.citations.citations[*i].iter().map(word_citation))
                }
                Inline::Break => runs.push_str("<w:r><w:br/></w:r>"),
                Inline::Footnote(contents) => {
                    let paragraphs = contents
                        .iter()
                        .map(|p| self.word_para(p, notes))
                        .collect::<Result<Vec<_>>>()?;
                    runs.push_str(&notes.add(paragraphs)?);
                }
            }
        }
        Ok(Para::new(p.style, runs))
    }

    fn typst(&self) -> String {
        let surname = typst_string(self.details.surname.trim());
        let mut s = format!(
            r#"#set text(font:"Liberation Serif", size:12pt, top-edge:0.8em, bottom-edge:-0.2em)
#set page("us-letter", margin:1in, header-ascent:0pt, header:align(top + right, pad(top:0.5in, context [#text({surname}) #counter(page).display("1")])))
#set par(leading:1em, spacing:1em)
#show footnote.entry: set text(size:12pt)
#show footnote.entry: set par(leading:1em, spacing:1em)
"#
        );
        for (i, (_, value)) in self.details.fields().into_iter().enumerate() {
            if i != 1 {
                s.push_str(&format!(
                    "#text({}) #linebreak()\n",
                    typst_string(value.trim())
                ));
            }
        }
        s.push_str(&format!(
            "#align(center)[#text({})]\n\n",
            typst_string(self.title)
        ));
        for p in &self.paragraphs {
            self.typst_paragraph(p, &mut s);
        }
        if !self.citations.works_cited.is_empty() {
            s.push_str("#pagebreak()\n#align(center)[Works Cited]\n\n");
            for item in &self.citations.works_cited {
                s.push_str("#par(hanging-indent:0.5in)[");
                for run in item {
                    typst_run(&mut s, &run.text, italic(run), bold(run));
                }
                s.push_str("]\n\n");
            }
        }
        s
    }

    fn typst_paragraph(&self, p: &Paragraph, out: &mut String) {
        out.push_str(match p.style {
            "Normal" => "#par(first-line-indent:(amount:0.5in, all:true))[",
            "Centered" => "#align(center)[#par[",
            "Quote" => "#pad(left:0.5in)[#par[",
            _ => "#par[",
        });
        self.typst_items(&p.items, out);
        if matches!(p.style, "Centered" | "Quote") {
            out.push(']');
        }
        out.push_str("]\n\n");
    }

    fn typst_items(&self, items: &[Inline], out: &mut String) {
        for item in items {
            match item {
                Inline::Text(text, italic, bold) => typst_run(out, text, *italic, *bold),
                Inline::Citation(i) => {
                    for run in &self.citations.citations[*i] {
                        typst_run(out, &run.text, italic(run), bold(run));
                    }
                }
                Inline::Break => out.push_str("#linebreak()"),
                Inline::Footnote(note) => {
                    out.push_str("#footnote[");
                    for (i, p) in note.iter().enumerate() {
                        if i > 0 {
                            out.push_str("#parbreak()");
                        }
                        self.typst_items(&p.items, out);
                    }
                    out.push(']');
                }
            }
        }
    }
}

fn italic(run: &Formatted) -> bool {
    run.formatting.font_style != FontStyle::Normal
}
fn bold(run: &Formatted) -> bool {
    matches!(run.formatting.font_weight, FontWeight::Bold)
}

fn word_citation(run: &Formatted) -> String {
    word_text(&run.text, italic(run), bold(run))
}

fn word_text(text: &str, italic: bool, bold: bool) -> String {
    if italic || bold {
        format!(
            "<w:r><w:rPr>{}{}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            if bold { "<w:b/>" } else { "" },
            if italic { "<w:i/>" } else { "" },
            xml_escape(text)
        )
    } else {
        docx_text(text)
    }
}

fn typst_run(out: &mut String, text: &str, italic: bool, bold: bool) {
    out.push_str(&format!(
        "#text(style:\"{}\",weight:\"{}\",{})",
        if italic { "italic" } else { "normal" },
        if bold { "bold" } else { "regular" },
        typst_string(text)
    ));
}

fn typst_string(text: &str) -> String {
    // JSON and Typst differ in their Unicode escapes. Emit Typst string
    // literals directly and leave all author-provided characters as data.
    let mut s = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\\\""),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if c.is_control() => s.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{MlaDetails, Template};
    use std::fs;

    #[test]
    fn mla_export_uses_real_page_notes_and_leaves_source_intact() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Template::Essay
            .scaffold(root, Some("Reading and attention"))
            .unwrap();
        let settings = root.join("project.toml");
        crate::submission::save_mla(
            root,
            &MlaDetails {
                name: "Casey Lane".into(),
                surname: "Lane".into(),
                instructor: "Professor Gray".into(),
                course: "Writing 201".into(),
                date: "22 November 2026".into(),
            },
            true,
            crate::sync::Fingerprint::read(&settings),
        )
        .unwrap();
        fs::write(root.join("sources.json"), r#"[{"id":"baron","type":"article-journal","title":"Redefining Reading: The Impact of Digital Communication Media","author":[{"family":"Baron","given":"Naomi S."}],"container-title":"PMLA","volume":"128","issue":"1","issued":{"date-parts":[[2013,1]]},"page":"193-200"}]"#).unwrap();
        let source = root.join("manuscript/01-Introduction.md");
        let original = "Some *written* prose [@baron, p. 194].[^fn]\n\n%% [@private] %%\n\n[^fn]: A public note [-@baron, p. 194].\n";
        fs::write(&source, original).unwrap();
        let project = Project::load(root).unwrap();
        let opts = ExportOptions {
            docx: true,
            epub: false,
            markdown: true,
            ..ExportOptions::default()
        };
        let report = super::super::preflight(&project, &opts).unwrap().report;
        assert!(
            !report.issues.iter().any(|i| i.category == "Citations"),
            "{:?}",
            report.issues
        );
        let result = super::super::export(&project, &opts).unwrap();
        assert_eq!(result.manuscript_footnotes, 1);
        let bytes = fs::read(&result.files[0]).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        use std::io::Read as _;
        let mut read = |name: &str| {
            let mut text = String::new();
            zip.by_name(name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            roxmltree::Document::parse(&text).unwrap();
            text
        };
        let doc = read("word/document.xml");
        assert!(doc.contains("Works Cited") && doc.contains("w:pageBreakBefore"));
        assert!(doc.contains("w:footnoteReference") && doc.contains("PMLA"));
        assert!(!doc.contains("Introduction") && !doc.contains("[@baron"));
        let note = read("word/footnotes.xml");
        assert!(note.contains("A public note") && note.contains("194"));
        assert!(!note.contains("Baron"));
        assert!(read("word/header1.xml").contains(" PAGE "));
        assert!(read("word/styles.xml").contains("Times New Roman"));
        assert!(
            fs::read_to_string(&result.files[1])
                .unwrap()
                .contains("[@baron, p. 194]")
        );
        assert_eq!(fs::read_to_string(source).unwrap(), original);
        if crate::pdf::has_typst() {
            let (layout, path) = result.mla_document.unwrap();
            crate::pdf::typst(&layout, &path).unwrap();
            assert!(fs::read(path).unwrap().starts_with(b"%PDF-"));
        }
    }
}
