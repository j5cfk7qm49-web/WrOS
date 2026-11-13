//! Every character the interface itself puts on screen is drawn exactly as
//! many cells wide as the layout gives it (rc13 design, call 19). A new symbol
//! Unifont draws two cells wide fails here until `assets/fonts/wros-narrow.txt`
//! gains a one-cell drawing for it.
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthChar;
use wros_graphics::unifont::glyph;

/// Characters outside Unifont's first plane, which the shipped font does not
/// carry: they show the missing-character box (call 21). Each needs a reason.
const ASTRAL_ALLOWED: &[(char, &str)] = &[
    (
        '😀',
        "example of an emoji a writer might type, in a doc comment",
    ),
    (
        '🦊',
        "example of an emoji a writer might type, in a doc comment",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("tests.rs")
        {
            out.push(path);
        }
    }
}

/// The file without its `#[cfg(test)] mod …` block.
fn without_tests(text: &str) -> &str {
    let mut from = 0;
    while let Some(at) = text[from..].find("#[cfg(test)]") {
        let at = from + at;
        let rest = text[at + "#[cfg(test)]".len()..].trim_start();
        if rest.starts_with("mod ") {
            return &text[..at];
        }
        from = at + 1;
    }
    text
}

fn interface_text() -> Vec<(PathBuf, String)> {
    let root = root();
    let mut files = vec![];
    rust_files(&root.join("src"), &mut files);
    for entry in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        if entry.file_name() != "crossterm-upstream" && entry.path().join("src").is_dir() {
            rust_files(&entry.path().join("src"), &mut files);
        }
    }
    let mut out: Vec<_> = files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p, without_tests(&text).to_string())
        })
        .collect();
    for entry in std::fs::read_dir(root.join("assets/help"))
        .unwrap()
        .flatten()
    {
        out.push((entry.path(), std::fs::read_to_string(entry.path()).unwrap()));
    }
    let tutorial = root.join("crates/grimoire-core/assets/wros/tutorial.json");
    out.push((tutorial.clone(), std::fs::read_to_string(tutorial).unwrap()));
    out
}

#[test]
fn every_interface_symbol_is_as_wide_as_the_layout_gives_it() {
    let mut wrong = std::collections::BTreeMap::new();
    for (path, text) in interface_text() {
        for c in text.chars().filter(|c| !c.is_ascii()) {
            let Some(cells) = c.width().filter(|w| *w > 0) else {
                continue; // combining marks and controls take no cell of their own
            };
            let problem = match glyph(c) {
                None if (c as u32) > 0xFFFF => {
                    if ASTRAL_ALLOWED.iter().any(|(a, _)| *a == c) {
                        continue;
                    }
                    "outside Unifont's first plane and not listed in ASTRAL_ALLOWED".to_string()
                }
                None => "missing from the font".to_string(),
                Some(g) if g.width() != cells * 8 => {
                    format!("{} cells in the font, {cells} in the layout", g.width() / 8)
                }
                Some(_) => continue,
            };
            wrong.entry(c).or_insert_with(|| {
                (
                    problem,
                    path.strip_prefix(root()).unwrap().display().to_string(),
                )
            });
        }
    }
    assert!(
        wrong.is_empty(),
        "{}",
        wrong
            .iter()
            .map(|(c, (why, at))| format!("U+{:04X} {c}: {why} ({at})", *c as u32))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
