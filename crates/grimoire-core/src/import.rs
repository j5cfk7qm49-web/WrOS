//! Bringing in a draft written somewhere else — a Word file, Markdown or plain
//! text — as a new book, split into parts, chapters and scenes at its
//! headings and scene breaks. Nothing is dropped: every word of the draft
//! lands in a scene, and the scene files are ordinary Markdown.

use crate::project::{Shape, ShapeChapter, ShapePart, ShapeScene};
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

/// The kinds of file a draft can be, by extension.
pub const KINDS: &[&str] = &["docx", "md", "markdown", "txt", "text"];

/// Whether `path` looks like a draft Grimoire can bring in.
pub fn is_draft(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| KINDS.contains(&e.to_lowercase().as_str()))
}

/// A draft, read and split up, ready to become a book.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// From the file itself (a Word title, Markdown frontmatter, a lone top
    /// heading), or the file's name.
    pub title: String,
    pub shape: Shape,
    /// What came before the first chapter in a draft that has chapters: a
    /// title page, a contents list, the author's address. Kept, in Notes,
    /// rather than made into a chapter nobody wrote.
    pub before: String,
}

impl Draft {
    pub fn chapters(&self) -> usize {
        self.shape.parts.iter().map(|p| p.chapters.len()).sum()
    }

    pub fn scenes(&self) -> usize {
        self.shape
            .parts
            .iter()
            .flat_map(|p| &p.chapters)
            .map(|c| c.scenes.len())
            .sum()
    }

    pub fn words(&self) -> usize {
        self.before.split_whitespace().count() + self.manuscript_words()
    }

    /// The words that went into the manuscript itself.
    pub fn manuscript_words(&self) -> usize {
        self.shape
            .parts
            .iter()
            .flat_map(|p| &p.chapters)
            .flat_map(|c| &c.scenes)
            .map(|s| s.body.split_whitespace().count())
            .sum()
    }
}

/// One paragraph-level piece of the draft.
#[derive(Debug, Clone, PartialEq)]
enum Block {
    /// A heading the file marks as one (`#`, a Word heading style), by level.
    Heading(u8, String),
    /// The book's own title (Word's Title style).
    Title(String),
    /// `***`, `* * *`, `#`, `~`, `§`, `⁂`: the end of a scene.
    Break,
    Para(String),
    Footnoted(String, Vec<i64>),
}

type WordFootnotes = BTreeMap<i64, String>;

#[derive(Default)]
struct DraftText {
    paragraphs: Vec<String>,
    notes: BTreeSet<i64>,
}

impl DraftText {
    fn footnoted(&mut self, text: String, notes: Vec<i64>) {
        self.paragraphs.push(text);
        self.notes.extend(notes);
    }

    fn take(&mut self, footnotes: &WordFootnotes) -> String {
        let mut text = self.paragraphs.join("\n\n");
        self.paragraphs.clear();
        for id in std::mem::take(&mut self.notes) {
            let mut lines = footnotes[&id].lines();
            text.push_str(&format!(
                "\n\n[^docx-{id}]: {}",
                lines.next().unwrap_or_default()
            ));
            for line in lines {
                text.push('\n');
                if !line.is_empty() {
                    text.push_str("    ");
                    text.push_str(line);
                }
            }
        }
        text
    }
}

/// Read the draft at `path`.
pub fn read(path: &Path) -> Result<Draft> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (blocks, front_title, footnotes) = match ext.as_str() {
        "docx" => {
            let (blocks, footnotes) = docx_blocks(path)?;
            (blocks, None, footnotes)
        }
        "md" | "markdown" | "txt" | "text" => {
            let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            let text = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
            let (front, body) = split_frontmatter(&text);
            (
                text_blocks(body, ext.starts_with('m')),
                front,
                WordFootnotes::new(),
            )
        }
        _ => {
            bail!("Grimoire can bring in a Word file (.docx), Markdown (.md) or plain text (.txt)")
        }
    };
    // "wolf_winter.docx" is "Wolf Winter".
    let fallback = path
        .file_stem()
        .map(|s| {
            s.to_string_lossy()
                .replace(['_', '-'], " ")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.chars().any(char::is_uppercase) {
                s
            } else {
                s.split(' ')
                    .map(|w| {
                        let mut c = w.chars();
                        c.next()
                            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        })
        .unwrap_or_else(|| "Untitled".into());
    let draft = build(blocks, front_title, fallback, &footnotes);
    if draft.words() == 0 {
        bail!("there are no words in {} to bring in", path.display());
    }
    Ok(draft)
}

/// Make the book at `root` from `draft`: its manuscript, every section any
/// book has, and (if there was any) what came before the first chapter as a
/// note in Notes. `root` must be new or empty.
pub fn make_book(root: &Path, draft: &Draft) -> Result<()> {
    std::fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    crate::project::scaffold_with(root, Some(&draft.title), &draft.shape)?;
    if !draft.before.trim().is_empty() {
        let notes = crate::project::Area::Notes.path(root);
        let note = crate::project::create(&notes, "Before the first chapter", false)?;
        let mut text = std::fs::read_to_string(&note)?;
        text.push_str("From the start of the draft, before its first chapter.\n\n");
        text.push_str(draft.before.trim());
        text.push('\n');
        crate::atomic::write_text(&note, &text)?;
    }
    Ok(())
}

/// Markdown frontmatter's `title:`, and the text after the frontmatter.
fn split_frontmatter(text: &str) -> (Option<String>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, text);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let title = rest[..offset]
                .lines()
                .find_map(|l| l.strip_prefix("title:"))
                .map(|t| t.trim().trim_matches(['"', '\'']).to_string())
                .filter(|t| !t.is_empty());
            return (title, &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    (None, text)
}

fn is_break(line: &str) -> bool {
    // Markdown exports escape a lone `#` or `*` as `\#`, `\*`.
    let unescaped = line.replace('\\', "");
    let t = unescaped.trim();
    !t.is_empty()
        && t.chars().count() <= 12
        && t.chars()
            .all(|c| matches!(c, '*' | '#' | '~' | '§' | '⁂' | '•' | '·' | '-' | '—' | '=' | ' '))
        // A lone dash could be the start of a list; three of anything is a break.
        && (t.chars().filter(|c| *c != ' ').count() >= 3 || matches!(t, "#" | "§" | "⁂" | "~" | "*"))
}

/// Paragraphs of Markdown or plain text: blank lines between them, headings
/// marked with `#` (Markdown only; in plain text a `#` line is a break).
fn text_blocks(text: &str, markdown: bool) -> Vec<Block> {
    let mut out = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(para.join("\n")));
            para.clear();
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
            continue;
        }
        if is_break(t) {
            flush(&mut para, &mut out);
            out.push(Block::Break);
            continue;
        }
        if markdown {
            let hashes = t.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
                flush(&mut para, &mut out);
                let text = t[hashes..].trim().trim_end_matches('#').trim();
                out.push(Block::Heading(hashes as u8, text.to_string()));
                continue;
            }
        }
        para.push(line.trim_end());
    }
    flush(&mut para, &mut out);
    out
}

/// "Part Two", "BOOK ONE", "Act 3", "Part IV: The Sea": a heading for a
/// part. "Book of Days" is a chapter.
fn is_part(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let mut words = lower.split(|c: char| c.is_whitespace() || c == ':' || c == '.');
    let (Some(first), Some(second)) = (words.next(), words.find(|w| !w.is_empty())) else {
        return false;
    };
    const NUMBERS: &[&str] = &[
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve", "first", "second", "third", "fourth", "fifth", "sixth", "last", "final",
    ];
    matches!(first, "part" | "book" | "act" | "volume")
        && (second.chars().all(|c| c.is_ascii_digit())
            || second.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l'))
            || NUMBERS.contains(&second))
}

/// A paragraph standing alone that reads as a chapter heading in a file with
/// no headings marked: "Chapter 3", "CHAPTER THREE: The Door", "Prologue".
fn is_plain_chapter(text: &str) -> bool {
    let t = text.trim();
    if t.contains('\n') || t.chars().count() > 70 {
        return false;
    }
    let lower = t.to_lowercase();
    let first = lower
        .split(|c: char| c.is_whitespace() || c == ':' || c == '.')
        .next()
        .unwrap_or("");
    matches!(first, "chapter" | "prologue" | "epilogue" | "interlude")
}

/// "PART ONE" as "Part One": a heading in capitals is a style, not a name.
fn tidy(heading: &str) -> String {
    let t = heading.trim();
    if t.chars().any(|c| c.is_lowercase()) || !t.chars().any(|c| c.is_uppercase()) {
        return t.to_string();
    }
    t.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_string() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turn the draft's blocks into parts, chapters and scenes.
fn build(
    mut blocks: Vec<Block>,
    front_title: Option<String>,
    fallback: String,
    footnotes: &WordFootnotes,
) -> Draft {
    let marked = blocks.iter().any(|b| matches!(b, Block::Heading(..)));
    // With no headings marked, lines that read as headings count as them.
    if !marked {
        for b in &mut blocks {
            if let Block::Para(t) = b
                && (is_plain_chapter(t)
                    || (is_part(t) && t.chars().count() <= 40 && !t.contains('\n')))
            {
                *b = Block::Heading(if is_part(t) { 1 } else { 2 }, t.trim().to_string());
            }
        }
    }
    let mut title = front_title;
    if title.is_none() {
        title = blocks.iter().find_map(|b| match b {
            Block::Title(t) => Some(t.clone()),
            _ => None,
        });
    }
    // One top-level heading above deeper ones is the book's title, unless
    // it reads as a chapter ("Prologue").
    let levels: Vec<u8> = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(l, t) if !is_part(t) => Some(*l),
            _ => None,
        })
        .collect();
    if let Some(&top) = levels.iter().min() {
        let at_top = levels.iter().filter(|&&l| l == top).count();
        let first_heading = blocks.iter().position(|b| matches!(b, Block::Heading(..)));
        if at_top == 1
            && levels.iter().any(|&l| l > top)
            && let Some(i) = first_heading
            && matches!(&blocks[i], Block::Heading(l, t) if *l == top && !is_part(t) && !is_plain_chapter(t))
            && let Block::Heading(_, t) = blocks.remove(i)
        {
            title.get_or_insert(t);
        }
    }
    // The chapters are the level most "Chapter …" headings sit at; with
    // none named that way, the highest level that isn't a part.
    let named: Vec<u8> = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(l, t)
                if is_plain_chapter(t) && t.to_lowercase().starts_with("chapter") =>
            {
                Some(*l)
            }
            _ => None,
        })
        .collect();
    let chapter_level = if named.is_empty() {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading(l, t) if !is_part(t) => Some(*l),
                _ => None,
            })
            .min()
    } else {
        (1..=9u8).max_by_key(|l| named.iter().filter(|&&n| n == *l).count())
    };
    // "Prologue" above the chapters' level is a chapter too.
    let heads_chapter = |l: u8, t: &str| {
        Some(l) == chapter_level || (is_plain_chapter(t) && chapter_level.is_some_and(|c| l <= c))
    };
    for b in &mut blocks {
        if let Block::Heading(_, t) = b {
            *t = tidy(t);
        }
    }
    // A draft with chapters: anything before the first one isn't a chapter.
    let structured = blocks.iter().any(|b| matches!(b, Block::Heading(..)));
    let mut started = !structured;
    let mut before = DraftText::default();

    let mut parts: Vec<ShapePart> = Vec::new();
    let mut part: Option<ShapePart> = None;
    let mut chapters: Vec<ShapeChapter> = Vec::new();
    let mut chapter: Option<ShapeChapter> = None;
    let mut scene_title: Option<String> = None;
    let mut body = DraftText::default();

    fn close_scene(
        chapter: &mut Option<ShapeChapter>,
        title: &mut Option<String>,
        body: &mut DraftText,
        footnotes: &WordFootnotes,
    ) {
        let text = body.take(footnotes);
        let titled = title.take();
        if text.trim().is_empty() && titled.is_none() {
            return;
        }
        let ch = chapter.get_or_insert_with(|| ShapeChapter {
            title: "Opening".into(),
            scenes: Vec::new(),
        });
        let n = ch.scenes.len() + 1;
        ch.scenes.push(ShapeScene {
            title: titled.unwrap_or_else(|| crate::manuscript::numbered("Scene", n)),
            status: "draft",
            target: None,
            body: text,
        });
    }
    fn close_chapter(chapters: &mut Vec<ShapeChapter>, chapter: &mut Option<ShapeChapter>) {
        if let Some(c) = chapter.take() {
            chapters.push(c);
        }
    }

    for b in blocks {
        let heads_a_chapter = match &b {
            Block::Heading(l, t) => is_part(t) || heads_chapter(*l, t),
            _ => false,
        };
        if !started && !heads_a_chapter {
            match b {
                Block::Title(t) if Some(&t) == title.as_ref() => {}
                Block::Title(t) | Block::Heading(_, t) | Block::Para(t) => {
                    before.paragraphs.push(t)
                }
                Block::Footnoted(t, notes) => before.footnoted(t, notes),
                Block::Break => {}
            }
            continue;
        }
        started = true;
        match b {
            // The title is the book's name; a second one is just words.
            Block::Title(t) if Some(&t) == title.as_ref() => {}
            Block::Title(t) => body.paragraphs.push(t),
            Block::Heading(_, t) if is_part(&t) => {
                close_scene(&mut chapter, &mut scene_title, &mut body, footnotes);
                close_chapter(&mut chapters, &mut chapter);
                let earlier = std::mem::take(&mut chapters);
                match part.take() {
                    Some(mut p) => {
                        p.chapters = earlier;
                        parts.push(p);
                    }
                    // Chapters before the first part (a prologue) stand on
                    // their own, ahead of it.
                    None if !earlier.is_empty() => parts.push(ShapePart {
                        title: None,
                        chapters: earlier,
                    }),
                    None => {}
                }
                part = Some(ShapePart {
                    title: Some(t),
                    chapters: Vec::new(),
                });
            }
            Block::Heading(l, t) if heads_chapter(l, &t) => {
                close_scene(&mut chapter, &mut scene_title, &mut body, footnotes);
                close_chapter(&mut chapters, &mut chapter);
                // "1" alone isn't a name anyone could find in the outline.
                let title = if t.chars().all(|c| c.is_ascii_digit() || c == '.') && !t.is_empty() {
                    format!("Chapter {}", t.trim_end_matches('.'))
                } else {
                    t
                };
                chapter = Some(ShapeChapter {
                    title,
                    scenes: Vec::new(),
                });
            }
            Block::Heading(_, t) => {
                close_scene(&mut chapter, &mut scene_title, &mut body, footnotes);
                scene_title = Some(t);
            }
            Block::Break => close_scene(&mut chapter, &mut scene_title, &mut body, footnotes),
            Block::Para(t) => body.paragraphs.push(t),
            Block::Footnoted(t, notes) => body.footnoted(t, notes),
        }
    }
    // "END" or "THE END" closing a manuscript is a marker, and the export
    // adds its own; kept, it would print twice.
    if body.paragraphs.last().is_some_and(|t| {
        matches!(
            t.trim().to_lowercase().trim_matches(['.', '*']),
            "end" | "the end"
        )
    }) {
        body.paragraphs.pop();
    }
    close_scene(&mut chapter, &mut scene_title, &mut body, footnotes);
    close_chapter(&mut chapters, &mut chapter);
    match part.take() {
        Some(mut p) => {
            p.chapters = chapters;
            parts.push(p);
        }
        None => parts.push(ShapePart {
            title: None,
            chapters,
        }),
    }
    // A draft with no chapter headings at all is one chapter.
    for p in &mut parts {
        for c in &mut p.chapters {
            if c.title == "Opening" && chapter_level.is_none() {
                c.title = "Chapter One".into();
            }
        }
    }
    parts.retain(|p| !p.chapters.is_empty());
    Draft {
        title: title.unwrap_or(fallback),
        shape: Shape {
            // A draft already past 80,000 aims for the next ten thousand.
            target_words: {
                let words: usize = parts
                    .iter()
                    .flat_map(|p| &p.chapters)
                    .flat_map(|c| &c.scenes)
                    .map(|s| s.body.split_whitespace().count())
                    .sum();
                80_000.max((words / 10_000 + 1) * 10_000)
            },
            daily_target: 1_000,
            parts,
        },
        before: before.take(footnotes),
    }
}

// ---- Word ----------------------------------------------------------------

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx_part(path: &Path, name: &str) -> Result<Option<String>> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("{} isn't a Word file Grimoire can open", path.display()))?;
    let Ok(mut entry) = zip.by_name(name) else {
        return Ok(None);
    };
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;
    Ok(Some(xml))
}

/// What the draft's styles mean, from styles.xml.
#[derive(Default)]
struct Styles {
    /// Paragraph styles that are headings: style id → level (0 for Title).
    /// Word stores the id; the name ("heading 1") is the same in every
    /// language, the id isn't.
    headings: std::collections::HashMap<String, u8>,
    /// Character styles that are italic or bold ("Emphasis", "Strong").
    chars: std::collections::HashMap<String, (bool, bool)>,
}

fn read_styles(path: &Path) -> Result<Styles> {
    let mut out = Styles::default();
    let Some(xml) = docx_part(path, "word/styles.xml")? else {
        return Ok(out);
    };
    let doc = roxmltree::Document::parse(&xml).context("reading the Word file's styles")?;
    for style in doc.descendants().filter(|n| n.has_tag_name((W, "style"))) {
        let Some(id) = style.attribute((W, "styleId")) else {
            continue;
        };
        let name = style
            .children()
            .find(|n| n.has_tag_name((W, "name")))
            .and_then(|n| n.attribute((W, "val")))
            .unwrap_or("")
            .to_lowercase();
        if style.attribute((W, "type")) == Some("character") {
            let props = style.children().find(|n| n.has_tag_name((W, "rPr")));
            let italic = on(props, "i") || name == "emphasis";
            let bold = on(props, "b") || name == "strong";
            if italic || bold {
                out.chars.insert(id.to_string(), (italic, bold));
            }
        } else if name == "title" {
            out.headings.insert(id.to_string(), 0);
        } else if let Some(n) = name.strip_prefix("heading ")
            && let Ok(level) = n.trim().parse::<u8>()
        {
            out.headings.insert(id.to_string(), level);
        }
    }
    Ok(out)
}

fn on(run_props: Option<roxmltree::Node>, tag: &str) -> bool {
    run_props
        .and_then(|p| p.children().find(|n| n.has_tag_name((W, tag))))
        .is_some_and(|n| !matches!(n.attribute((W, "val")), Some("0" | "false" | "none")))
}

fn word_note_id(node: roxmltree::Node) -> Result<i64> {
    node.attribute((W, "id"))
        .and_then(|id| id.parse().ok())
        .context("a Word footnote has an invalid or missing ID")
}

/// A Word paragraph as Markdown, with its emphasis and footnote references.
fn docx_paragraph(
    p: roxmltree::Node,
    styles: &Styles,
    inline_notes: bool,
) -> Result<(String, Vec<i64>)> {
    let mut out = String::new();
    let mut notes = Vec::new();
    let mut open: (bool, bool) = (false, false);
    let mut pending_space = String::new();
    let mark = |out: &mut String, from: (bool, bool), to: (bool, bool)| {
        if from.0 && !to.0 {
            out.push('*');
        }
        if from.1 && !to.1 {
            out.push_str("**");
        }
        if !from.1 && to.1 {
            out.push_str("**");
        }
        if !from.0 && to.0 {
            out.push('*');
        }
    };
    for r in p.descendants().filter(|n| n.has_tag_name((W, "r"))) {
        // Deleted text in tracked changes isn't part of the draft.
        if r.ancestors().any(|a| a.has_tag_name((W, "del"))) {
            continue;
        }
        let props = r.children().find(|n| n.has_tag_name((W, "rPr")));
        let (styled_i, styled_b) = props
            .and_then(|p| p.children().find(|n| n.has_tag_name((W, "rStyle"))))
            .and_then(|s| s.attribute((W, "val")))
            .and_then(|id| styles.chars.get(id).copied())
            .unwrap_or((false, false));
        let style = (on(props, "i") || styled_i, on(props, "b") || styled_b);
        for c in r.children() {
            if c.has_tag_name((W, "footnoteReference")) {
                let id = word_note_id(c)?;
                notes.push(id);
                if inline_notes {
                    mark(&mut out, open, (false, false));
                    open = (false, false);
                    out.push_str(&pending_space);
                    pending_space.clear();
                    out.push_str(&format!("[^docx-{id}]"));
                }
                continue;
            }
            let text = if c.has_tag_name((W, "t")) {
                c.text().unwrap_or("").to_string()
            } else if c.has_tag_name((W, "tab")) {
                " ".into()
            } else if c.has_tag_name((W, "br")) && c.attribute((W, "type")).is_none() {
                "\n".into()
            } else {
                continue;
            };
            // Spaces sit outside the * marks, or Markdown won't see them.
            let lead: String = text.chars().take_while(|c| c.is_whitespace()).collect();
            let core = text.trim();
            if core.is_empty() {
                pending_space.push_str(&text);
                continue;
            }
            let trail: String = text
                .chars()
                .rev()
                .take_while(|c| c.is_whitespace())
                .collect();
            if style != open {
                mark(&mut out, open, (false, false));
                out.push_str(&pending_space);
                out.push_str(&lead);
                mark(&mut out, (false, false), style);
                open = style;
            } else {
                out.push_str(&pending_space);
                out.push_str(&lead);
            }
            pending_space.clear();
            out.push_str(core);
            pending_space.push_str(&trail);
        }
    }
    mark(&mut out, open, (false, false));
    Ok((out.trim().to_string(), notes))
}

fn docx_blocks(path: &Path) -> Result<(Vec<Block>, WordFootnotes)> {
    let styles = read_styles(path)?;
    let Some(xml) = docx_part(path, "word/document.xml")? else {
        bail!("{} has no document inside it", path.display());
    };
    let doc = roxmltree::Document::parse(&xml).context("reading the Word file")?;
    let mut out = Vec::new();
    for p in doc.descendants().filter(|n| n.has_tag_name((W, "p"))) {
        let style = p
            .children()
            .find(|n| n.has_tag_name((W, "pPr")))
            .and_then(|pr| pr.children().find(|n| n.has_tag_name((W, "pStyle"))))
            .and_then(|s| s.attribute((W, "val")))
            .and_then(|id| {
                styles
                    .headings
                    .get(id)
                    .copied()
                    .or_else(|| builtin_heading(id))
            });
        let (text, notes) = docx_paragraph(p, &styles, style.is_none())?;
        if style.is_none() && !notes.is_empty() {
            out.push(Block::Footnoted(text, notes));
            continue;
        }
        if !text.is_empty() {
            let plain = || text.replace(['*'], "").trim().to_string();
            out.push(match style {
                Some(0) => Block::Title(plain()),
                Some(level) => Block::Heading(level, plain()),
                None if is_break(&text) => Block::Break,
                None => Block::Para(text),
            });
        }
        if !notes.is_empty() {
            // Heading titles become outline names. Keep their references in the draft text.
            let references = notes
                .iter()
                .map(|id| format!("[^docx-{id}]"))
                .collect::<Vec<_>>();
            out.push(Block::Footnoted(references.join(" "), notes));
        }
    }
    let mut footnotes = WordFootnotes::new();
    if out
        .iter()
        .any(|block| matches!(block, Block::Footnoted(..)))
    {
        let xml = docx_part(path, "word/footnotes.xml")?
            .context("the Word file has footnote references but no footnote part")?;
        let doc = roxmltree::Document::parse(&xml).context("reading the Word file's footnotes")?;
        for note in doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name((W, "footnote")))
        {
            if !matches!(note.attribute((W, "type")), None | Some("normal")) {
                continue;
            }
            let id = word_note_id(note)?;
            let mut paragraphs = Vec::new();
            for p in note.descendants().filter(|n| n.has_tag_name((W, "p"))) {
                let (text, nested) = docx_paragraph(p, &styles, false)?;
                ensure!(
                    nested.is_empty(),
                    "a Word footnote contains another footnote reference"
                );
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
            ensure!(
                footnotes.insert(id, paragraphs.join("\n\n")).is_none(),
                "duplicate Word footnote {id}"
            );
        }
        for block in &out {
            if let Block::Footnoted(_, ids) = block {
                for id in ids {
                    ensure!(footnotes.contains_key(id), "Word footnote {id} is missing");
                }
            }
        }
    }
    Ok((out, footnotes))
}

/// Heading style ids Word and Google Docs use when styles.xml is missing.
fn builtin_heading(id: &str) -> Option<u8> {
    if id.eq_ignore_ascii_case("title") {
        return Some(0);
    }
    id.strip_prefix("Heading")
        .and_then(|n| n.parse::<u8>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> usize {
        s.split_whitespace().count()
    }

    /// Each part's title, and its chapters' titles with their scenes'.
    type Outline = Vec<(Option<String>, Vec<(String, Vec<String>)>)>;

    fn shape_of(d: &Draft) -> Outline {
        d.shape
            .parts
            .iter()
            .map(|p| {
                (
                    p.title.clone(),
                    p.chapters
                        .iter()
                        .map(|c| {
                            (
                                c.title.clone(),
                                c.scenes.iter().map(|s| s.title.clone()).collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn frontmatter_requires_a_complete_closing_fence() {
        let raw = "---\nWords that belong to the draft.\n---still prose\nLast words.\n";
        assert_eq!(split_frontmatter(raw), (None, raw));
        let raw = "---\ntitle: Draft\n---not a fence\n---\nBody.\n";
        assert_eq!(split_frontmatter(raw), (Some("Draft".into()), "Body.\n"));
    }

    #[test]
    fn markdown_splits_at_chapters_and_scene_breaks_and_keeps_every_word() {
        let md = "---\ntitle: The Salt Archive\n---\n\nA note before it all.\n\n## Chapter One\n\nThe rain had stopped.\n\n\\#\n\nOren had promised.\n\n## CHAPTER TWO\n\nShe crossed the lot.\n";
        let (front, body) = split_frontmatter(md);
        let d = build(
            text_blocks(body, true),
            front,
            "x".into(),
            &WordFootnotes::new(),
        );
        assert_eq!(d.title, "The Salt Archive");
        assert_eq!(d.before, "A note before it all.");
        assert_eq!(
            shape_of(&d),
            vec![(
                None,
                vec![
                    (
                        "Chapter One".into(),
                        vec!["Scene One".into(), "Scene Two".into()]
                    ),
                    ("Chapter Two".into(), vec!["Scene One".into()]),
                ]
            )]
        );
        let body_words = words(
            "A note before it all. The rain had stopped. Oren had promised. She crossed the lot.",
        );
        assert_eq!(d.words(), body_words);
    }

    #[test]
    fn a_lone_top_heading_is_the_title_and_parts_hold_chapters() {
        let md = "# Wolf Winter\n\n## Prologue\n\nCold.\n\n# Part One\n\n## 1\n\nSnow.\n\n## 2\n\nMore snow.\n\n# Part Two\n\n## 3\n\n### The Thaw\n\nWater.\n";
        let d = build(
            text_blocks(md, true),
            None,
            "x".into(),
            &WordFootnotes::new(),
        );
        assert_eq!(d.title, "Wolf Winter");
        assert_eq!(
            shape_of(&d),
            vec![
                (None, vec![("Prologue".into(), vec!["Scene One".into()])]),
                (
                    Some("Part One".into()),
                    vec![
                        ("Chapter 1".into(), vec!["Scene One".into()]),
                        ("Chapter 2".into(), vec!["Scene One".into()]),
                    ]
                ),
                (
                    Some("Part Two".into()),
                    vec![("Chapter 3".into(), vec!["The Thaw".into()])]
                ),
            ]
        );
    }

    #[test]
    fn a_prologue_above_the_chapters_level_is_a_chapter_not_the_title() {
        let md = "# Prologue\n\nBefore the war.\n\n# Part One\n\n## Chapter 1\n\nBoats.\n\n## Chapter 2\n\n### The Pier\n\nGone.\n";
        let d = build(
            text_blocks(md, true),
            None,
            "Harbour".into(),
            &WordFootnotes::new(),
        );
        assert_eq!(d.title, "Harbour");
        assert_eq!(
            shape_of(&d),
            vec![
                (None, vec![("Prologue".into(), vec!["Scene One".into()])]),
                (
                    Some("Part One".into()),
                    vec![
                        ("Chapter 1".into(), vec!["Scene One".into()]),
                        ("Chapter 2".into(), vec!["The Pier".into()]),
                    ]
                ),
            ]
        );
    }

    #[test]
    fn plain_text_finds_chapter_lines_and_a_file_with_none_is_one_chapter() {
        let txt = "CHAPTER ONE\n\nIt began\nwith a letter.\n\n#\n\nThen another.\n\nChapter 2: The Reply\n\nNo one wrote back.\n\nTHE END\n";
        let d = build(
            text_blocks(txt, false),
            None,
            "Letters".into(),
            &WordFootnotes::new(),
        );
        assert_eq!(d.title, "Letters");
        assert_eq!(d.words(), 11, "THE END is left for the export to add");
        assert_eq!(
            shape_of(&d),
            vec![(
                None,
                vec![
                    (
                        "Chapter One".into(),
                        vec!["Scene One".into(), "Scene Two".into()]
                    ),
                    ("Chapter 2: The Reply".into(), vec!["Scene One".into()]),
                ]
            )]
        );
        let none = build(
            text_blocks("Just a story.\n\n***\n\nThe last line.", false),
            None,
            "Tiny".into(),
            &WordFootnotes::new(),
        );
        assert_eq!(
            shape_of(&none),
            vec![(
                None,
                vec![(
                    "Chapter One".into(),
                    vec!["Scene One".into(), "Scene Two".into()]
                )]
            )]
        );
    }

    #[test]
    fn word_keeps_italics_and_reads_heading_styles_by_name() {
        let xml = format!(
            r#"<w:document xmlns:w="{W}"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Titel"/></w:pPr><w:r><w:t>The Long Night</w:t></w:r></w:p>
            <w:p><w:pPr><w:pStyle w:val="berschrift1"/></w:pPr><w:r><w:t>Chapter One</w:t></w:r></w:p>
            <w:p><w:r><w:t xml:space="preserve">It was </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">very </w:t></w:r><w:r><w:t>late.</w:t></w:r></w:p>
            <w:p><w:r><w:t>* * *</w:t></w:r></w:p>
            <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Morning.</w:t></w:r><w:del><w:r><w:t>gone</w:t></w:r></w:del></w:p>
            </w:body></w:document>"#
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let paras: Vec<String> = doc
            .descendants()
            .filter(|n| n.has_tag_name((W, "p")))
            .map(|p| docx_paragraph(p, &Styles::default(), true).unwrap().0)
            .collect();
        assert_eq!(paras[2], "It was *very* late.");
        assert_eq!(paras[4], "**Morning.**");
        assert_eq!(builtin_heading("Heading2"), Some(2));
    }

    #[test]
    fn importing_word_preserves_shared_and_multiline_footnotes_in_each_scene() {
        use std::io::Write;

        let path =
            std::env::temp_dir().join(format!("grimoire-word-notes-{}.docx", std::process::id()));
        let mut zip = zip::ZipWriter::new(std::fs::File::create_new(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", options).unwrap();
        write!(
            zip,
            r#"<w:document xmlns:w="{W}"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
                <w:r><w:t>Chapter One</w:t><w:footnoteReference w:id="9"/></w:r>
            </w:p>
            <w:p><w:r><w:rPr><w:i/></w:rPr><w:t>Résumé</w:t></w:r>
                <w:r><w:footnoteReference w:id="7"/><w:t>.</w:t></w:r>
                <w:del><w:r><w:footnoteReference w:id="90"/></w:r></w:del>
            </w:p>
            <w:p><w:r><w:t>Again</w:t><w:footnoteReference w:id="7"/></w:r></w:p>
            <w:p><w:r><w:t>* * *</w:t></w:r></w:p>
            <w:p><w:r><w:t>In the next scene</w:t><w:footnoteReference w:id="7"/></w:r></w:p>
            </w:body></w:document>"#
        )
        .unwrap();
        zip.start_file("word/footnotes.xml", options).unwrap();
        write!(
            zip,
            r#"<w:footnotes xmlns:w="{W}">
            <w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
            <w:footnote w:id="7">
                <w:p><w:r><w:footnoteRef/><w:t>First paragraph.</w:t></w:r></w:p>
                <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>λ second paragraph.</w:t></w:r></w:p>
            </w:footnote>
            <w:footnote w:id="9"><w:p><w:r><w:t>Heading context.</w:t></w:r></w:p></w:footnote>
            </w:footnotes>"#
        ).unwrap();
        zip.finish().unwrap();

        let draft = read(&path);
        std::fs::remove_file(&path).unwrap();
        let draft = draft.unwrap();
        let chapter = &draft.shape.parts[0].chapters[0];
        assert_eq!(chapter.title, "Chapter One");
        assert_eq!(chapter.scenes.len(), 2);
        assert!(chapter.scenes[0].body.contains("*Résumé*[^docx-7]."));
        for scene in &chapter.scenes {
            let notes = crate::footnotes::analyze_footnotes(&scene.body);
            assert!(notes.missing_definitions.is_empty());
            assert!(notes.duplicate_definitions.is_empty());
            assert!(notes.unused_definitions.is_empty());
            assert_eq!(
                notes.definitions["docx-7"].content,
                "First paragraph.\n\n    **λ second paragraph.**"
            );
        }
        assert!(chapter.scenes[0].body.contains("Heading context."));
        assert!(!chapter.scenes[1].body.contains("Heading context."));
    }
}
