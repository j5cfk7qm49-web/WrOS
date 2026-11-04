//! Read-only publishing review of supplied scene bodies and optional story models.
//! Checks footnotes, editorial markers, internal wiki links, Markdown links,
//! image/cover assets, unclosed `%%` comment delimiters, empty scenes,
//! unresolved revision notes, and explicit continuity contradictions.
use crate::annotations::AnnotationStore;
use crate::continuity::ContinuityStore;
use crate::footnotes::analyze_footnotes;
use crate::review_notes::ReviewNotes;
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightSeverity {
    Error,
    Warning,
    Notice,
}

impl PreflightSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Notice => "Notice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightIssue {
    pub severity: PreflightSeverity,
    pub category: String,
    pub scene_id: Option<usize>,
    pub scene_title: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreflightReport {
    pub total_scenes: usize,
    pub total_words: usize,
    pub issues: Vec<PreflightIssue>,
}

impl PreflightReport {
    pub fn errors_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == PreflightSeverity::Error)
            .count()
    }

    pub fn warnings_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == PreflightSeverity::Warning)
            .count()
    }

    pub fn notices_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == PreflightSeverity::Notice)
            .count()
    }

    pub fn is_clean(&self) -> bool {
        self.errors_count() == 0 && self.warnings_count() == 0
    }
}

#[derive(Default)]
pub struct PreflightContext<'a> {
    pub continuity: Option<&'a ContinuityStore>,
    pub annotations: Option<&'a AnnotationStore>,
    pub review_notes: Option<&'a ReviewNotes>,
    pub known_targets: Option<&'a BTreeSet<String>>,
    pub project_root: Option<&'a Path>,
}

/// Inspect scene bodies supplied by a compile-scope adapter. The caller
/// selects included scenes and binds IDs/revisions; this does not read files.
pub fn run_preflight(
    scenes: &[(usize, String, String)], // (id, title, markdown)
    continuity: Option<&ContinuityStore>,
    annotations: Option<&AnnotationStore>,
) -> Result<PreflightReport> {
    run_preflight_with_targets(scenes, continuity, annotations, None)
}

/// Extended preflight that also checks `[[Target]]` wiki links when a set of
/// known project titles/aliases (lowercased) is provided.
pub fn run_preflight_with_targets(
    scenes: &[(usize, String, String)],
    continuity: Option<&ContinuityStore>,
    annotations: Option<&AnnotationStore>,
    known_targets: Option<&BTreeSet<String>>,
) -> Result<PreflightReport> {
    run_unified_preflight(
        scenes,
        &PreflightContext {
            continuity,
            annotations,
            review_notes: None,
            known_targets,
            project_root: None,
        },
    )
}

/// Unified pre-export preflight covering footnotes, editorial markers (TK/TODO/FIXME),
/// internal wiki and Markdown links, local/remote image assets, unclosed `%%` comment
/// delimiters, open revision annotations, and continuity contradictions.
pub fn run_unified_preflight(
    scenes: &[(usize, String, String)],
    ctx: &PreflightContext<'_>,
) -> Result<PreflightReport> {
    ensure!(
        scenes.len() <= 512,
        "preflight scene limit exceeded; no partial report"
    );
    ensure!(
        scenes.iter().map(|(_, _, s)| s.len()).sum::<usize>() <= 16_777_216,
        "preflight source limit exceeded; no partial report"
    );
    let mut ids = BTreeSet::new();
    for (id, title, _) in scenes {
        ensure!(
            ids.insert(*id) && title.len() <= 4096,
            "invalid or duplicate preflight scene identity"
        );
    }
    if let Some(store) = ctx.annotations {
        crate::statefile::Validate::validate(store)?;
    }
    if let Some(store) = ctx.review_notes {
        crate::statefile::Validate::validate(store)?;
    }
    let mut report = PreflightReport {
        total_scenes: scenes.len(),
        ..PreflightReport::default()
    };
    for (scene_id, title, content) in scenes {
        let words = crate::notes::count_words(content);
        report.total_words += words;
        let mut add = |severity, category: &str, line, message| -> Result<()> {
            ensure!(
                report.issues.len() < 512,
                "preflight issue limit exceeded; no partial report"
            );
            report.issues.push(PreflightIssue {
                severity,
                category: category.into(),
                scene_id: Some(*scene_id),
                scene_title: Some(title.clone()),
                line,
                message,
            });
            Ok(())
        };
        if words == 0 {
            add(
                PreflightSeverity::Notice,
                "Structure",
                None,
                "Scene has no counted prose words.".into(),
            )?;
        }
        if let Some(line) = unclosed_comment_delimiter_line(content) {
            add(
                PreflightSeverity::Warning,
                "Privacy",
                Some(line),
                format!(
                    "The '%%' note on line {line} is never closed: only the rest of that line is hidden, and the lines after it will be exported."
                ),
            )?;
        }
        let notes = analyze_footnotes(content);
        // Report locations in the *original UTF-8 scene*, not a rendered or
        // masked copy. A user can navigate directly to these lines when a
        // preflight UI is connected to the manuscript's revision identity.
        for missing in &notes.missing_definitions {
            let line = notes
                .references
                .iter()
                .find(|r| crate::footnotes::normalize(&r.label) == *missing)
                .map(|r| content[..r.offset].bytes().filter(|&b| b == b'\n').count() + 1);
            add(
                PreflightSeverity::Error,
                "Footnotes",
                line,
                format!("Footnote '[^{missing}]' has no definition in this scene."),
            )?;
        }
        for duplicate in &notes.duplicate_definitions {
            let line = notes.definitions.get(duplicate).map(|d| d.line_number);
            add(
                PreflightSeverity::Error,
                "Footnotes",
                line,
                format!("Footnote '[^{duplicate}]' has duplicate definitions in this scene."),
            )?;
        }
        for unused in &notes.unused_definitions {
            let line = notes.definitions.get(unused).map(|d| d.line_number);
            add(
                PreflightSeverity::Warning,
                "Footnotes",
                line,
                format!("Footnote '[^{unused}]' is defined but never referenced in this scene."),
            )?;
        }
        for line in editorial_lines(content) {
            add(
                PreflightSeverity::Warning,
                "Editorial",
                Some(line),
                format!("Unresolved TK/TODO/FIXME marker on line {line}."),
            )?;
        }
        if let Some(targets) = ctx.known_targets {
            for (line, target) in unresolved_wiki_links(content, targets)? {
                add(
                    PreflightSeverity::Warning,
                    "Links",
                    Some(line),
                    format!("Unresolved internal link '[[{target}]]' on line {line}."),
                )?;
            }
        }
        for (severity, category, line, message) in
            inspect_markdown_links_and_assets(content, ctx.known_targets, ctx.project_root)?
        {
            add(severity, category, Some(line), message)?;
        }
        if let Some(store) = ctx.annotations {
            let count = store
                .annotations
                .iter()
                .filter(|a| {
                    a.scene_id == *scene_id
                        && a.status != crate::annotations::AnnotationStatus::Resolved
                })
                .count();
            if count > 0 {
                add(
                    PreflightSeverity::Notice,
                    "Revisions",
                    None,
                    format!("{count} unresolved revision annotation(s) attached to this scene."),
                )?;
            }
        }
        if let Some(store) = ctx.review_notes {
            let normalized = title.replace('\\', "/");
            if let Some(doc) = store
                .documents
                .iter()
                .find(|d| d.path == normalized && !d.needs_reconnect)
            {
                let count = store
                    .notes
                    .iter()
                    .filter(|n| {
                        n.current.document_id == doc.id
                            && n.current.status != crate::annotations::AnnotationStatus::Resolved
                    })
                    .count();
                if count > 0 {
                    add(
                        PreflightSeverity::Notice,
                        "Revisions",
                        None,
                        format!("{count} unresolved revision note(s) attached to this scene."),
                    )?;
                }
            }
        }
    }
    if let Some(store) = ctx.continuity {
        for conflict in store.check_contradictions()? {
            ensure!(
                report.issues.len() < 512,
                "preflight issue limit exceeded; no partial report"
            );
            let severity = match conflict.severity {
                crate::continuity::ContradictionSeverity::DefiniteConflict => {
                    PreflightSeverity::Error
                }
                crate::continuity::ContradictionSeverity::SuggestedInconsistency => {
                    PreflightSeverity::Warning
                }
            };
            report.issues.push(PreflightIssue {
                severity,
                category: "Continuity".into(),
                scene_id: None,
                scene_title: None,
                line: None,
                message: conflict.explanation,
            });
        }
    }
    Ok(report)
}

/// The line of the first `%%` that is never closed. Grimoire hides an unclosed
/// note only to the end of its line (`notes::note_ranges`), so the lines after
/// it export. Markers are paired exactly as export pairs them; one inside code
/// is not reported.
fn unclosed_comment_delimiter_line(content: &str) -> Option<usize> {
    let mut code = Vec::new();
    let mut block = None;
    for (event, range) in Parser::new_ext(content, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => block = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(from) = block.take() {
                    code.push(from..range.end);
                }
            }
            Event::Code(_) => code.push(range),
            _ => {}
        }
    }
    let mut from = 0;
    while let Some(open) = content[from..].find("%%").map(|i| from + i) {
        match content[open + 2..].find("%%") {
            Some(close) => from = open + 2 + close + 2,
            None if code.iter().any(|r| r.contains(&open)) => from = open + 2,
            None => return Some(content[..open].bytes().filter(|&b| b == b'\n').count() + 1),
        }
    }
    None
}

fn safe_project_asset(root: &Path, relative: &str) -> bool {
    let clean = relative.split(['#', '?']).next().unwrap_or("").trim();
    if clean.is_empty() || clean.starts_with('/') || clean.contains('\\') {
        return false;
    }
    let rel_path = Path::new(clean);
    if rel_path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return false;
    }
    let Ok(canon_root) = std::fs::canonicalize(root) else {
        return false;
    };
    let candidate = root.join(rel_path);
    let Ok(meta) = std::fs::symlink_metadata(&candidate) else {
        return false;
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return false;
    }
    std::fs::canonicalize(&candidate).is_ok_and(|c| c.starts_with(&canon_root))
}

fn inspect_markdown_links_and_assets(
    content: &str,
    known_targets: Option<&BTreeSet<String>>,
    project_root: Option<&Path>,
) -> Result<Vec<(PreflightSeverity, &'static str, usize, String)>> {
    let mut masked = content.as_bytes().to_vec();
    for (a, b) in crate::notes::note_ranges(content) {
        for byte in &mut masked[a..b] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    let parsed = String::from_utf8(masked).expect("complete UTF-8 ranges masked with ASCII");
    let mut out = Vec::new();
    for (event, range) in Parser::new_ext(&parsed, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                let dest = dest_url.trim();
                let line = content[..range.start]
                    .bytes()
                    .filter(|&b| b == b'\n')
                    .count()
                    + 1;
                if dest.is_empty() {
                    ensure!(
                        out.len() < 512,
                        "link/asset review limit exceeded; no partial report"
                    );
                    out.push((
                        PreflightSeverity::Warning,
                        "Links",
                        line,
                        format!("Empty Markdown link destination on line {line}."),
                    ));
                } else if !dest.starts_with("http://")
                    && !dest.starts_with("https://")
                    && !dest.starts_with("mailto:")
                    && !dest.starts_with('#')
                {
                    let base = dest.split(['#', '?']).next().unwrap_or("").trim();
                    let matched_target = known_targets.is_some_and(|targets| {
                        targets.contains(&base.to_lowercase())
                            || Path::new(base)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .is_some_and(|s| targets.contains(&s.to_lowercase()))
                    });
                    let matched_file =
                        project_root.is_some_and(|root| safe_project_asset(root, base));
                    if (known_targets.is_some() || project_root.is_some())
                        && !matched_target
                        && !matched_file
                    {
                        ensure!(
                            out.len() < 512 && dest.len() <= 4096,
                            "link/asset review limit exceeded; no partial report"
                        );
                        out.push((
                            PreflightSeverity::Warning,
                            "Links",
                            line,
                            format!("Unresolved link target '{dest}' on line {line}."),
                        ));
                    }
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let dest = dest_url.trim();
                let line = content[..range.start]
                    .bytes()
                    .filter(|&b| b == b'\n')
                    .count()
                    + 1;
                ensure!(
                    out.len() < 512 && dest.len() <= 4096,
                    "link/asset review limit exceeded; no partial report"
                );
                if dest.is_empty() {
                    out.push((
                        PreflightSeverity::Error,
                        "Assets",
                        line,
                        format!("Image has an empty asset path on line {line}."),
                    ));
                } else if dest.starts_with("http://") || dest.starts_with("https://") {
                    out.push((
                        PreflightSeverity::Warning,
                        "Assets",
                        line,
                        format!(
                            "Remote image URL '{dest}' on line {line} cannot be bundled offline."
                        ),
                    ));
                } else if let Some(root) = project_root
                    && !safe_project_asset(root, dest)
                {
                    out.push((
                        PreflightSeverity::Error,
                        "Assets",
                        line,
                        format!("Missing or outside-project image asset '{dest}' on line {line}."),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn unresolved_wiki_links(
    content: &str,
    known_targets: &BTreeSet<String>,
) -> Result<Vec<(usize, String)>> {
    let mut masked = content.as_bytes().to_vec();
    for (a, b) in crate::notes::note_ranges(content) {
        for byte in &mut masked[a..b] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    let parsed = String::from_utf8(masked).expect("complete UTF-8 ranges masked with ASCII");
    let mut excluded = Vec::new();
    let mut code = false;
    for (event, range) in Parser::new_ext(&parsed, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) => excluded.push(range),
            _ if code => excluded.push(range),
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = parsed[from..].find("[[") {
        let start = from + rel;
        let escaped = parsed[..start]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count()
            % 2
            == 1;
        let Some(end_rel) = parsed[start + 2..].find("]]") else {
            break;
        };
        let end = start + 2 + end_rel;
        from = end + 2;
        if escaped || excluded.iter().any(|r| r.start <= start && start < r.end) {
            continue;
        }
        let inner = parsed[start + 2..end].trim();
        let target = inner.split('|').next().unwrap_or("").trim();
        if !target.is_empty()
            && !target.contains('\n')
            && !known_targets.contains(&target.to_lowercase())
        {
            ensure!(
                out.len() < 512 && target.len() <= 4096,
                "wiki-link review limit exceeded; no partial report"
            );
            let line = content[..start].bytes().filter(|&b| b == b'\n').count() + 1;
            out.push((line, target.to_string()));
        }
    }
    Ok(out)
}

fn editorial_lines(content: &str) -> BTreeSet<usize> {
    let mut masked = content.as_bytes().to_vec();
    for (a, b) in crate::notes::note_ranges(content) {
        for byte in &mut masked[a..b] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    let parsed = String::from_utf8(masked).expect("complete UTF-8 ranges masked with ASCII");
    let mut code = false;
    let mut visible = Vec::new();
    for (event, range) in Parser::new_ext(&parsed, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Text(_) if !code => visible.push(range),
            _ => {}
        }
    }
    let mut lines = BTreeSet::new();
    for (a, b) in crate::notes::tk_ranges(&parsed) {
        if visible.iter().any(|r| r.start <= a && r.end >= b) {
            lines.insert(content[..a].bytes().filter(|b| *b == b'\n').count() + 1);
        }
    }
    for range in visible {
        let mut offset = range.start;
        for token in parsed[range].split_inclusive(|c: char| !c.is_alphanumeric()) {
            let word = token.trim_end_matches(|c: char| !c.is_alphanumeric());
            if matches!(word, "TODO" | "FIXME") {
                lines.insert(content[..offset].bytes().filter(|b| *b == b'\n').count() + 1);
            }
            offset += token.len();
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editorial_markers_follow_existing_private_note_and_token_rules() {
        let scenes = vec![(
            1,
            "Scene".into(),
            "Atka Toolkit TODOS FIXMEable. %% TK TODO FIXME %%\n`TODO TK`\n\n```\nFIXME TK\n```\n\nTKTK and TODO here.".into(),
        )];
        let report = run_preflight(&scenes, None, None).unwrap();
        assert_eq!(
            report.warnings_count(),
            1,
            "one public editorial line, no substring/code/private false positives"
        );
    }

    #[test]
    fn empty_scenes_keep_revision_notices_and_invalid_inputs_never_look_clean() {
        let mut annotations = AnnotationStore::default();
        annotations
            .add(1, "Scene", "removed words", 0, "still open")
            .unwrap();
        let scenes = vec![(1, "Scene".into(), String::new())];
        assert_eq!(
            run_preflight(&scenes, None, Some(&annotations))
                .unwrap()
                .notices_count(),
            2
        );
        let duplicate = vec![scenes[0].clone(), scenes[0].clone()];
        assert!(run_preflight(&duplicate, None, None).is_err());
        annotations.schema_version = 99;
        assert!(run_preflight(&scenes, None, Some(&annotations)).is_err());
    }

    #[test]
    fn preflight_flags_missing_footnotes_and_tks() {
        let scenes = vec![(
            1,
            "Chapter 1".into(),
            "He walked down the street[^missing]. We need TK more details here.".into(),
        )];

        let report = run_preflight(&scenes, None, None).unwrap();
        assert_eq!(report.errors_count(), 1); // missing footnote
        assert_eq!(report.warnings_count(), 1); // TK marker
        assert!(!report.is_clean());
    }

    #[test]
    fn preflight_detects_unresolved_wiki_links() {
        let scenes = vec![(
            1,
            "Chapter 1".into(),
            "Mara met [[Wren]] at [[The Lost Harbor]].".into(),
        )];
        let targets = BTreeSet::from(["wren".to_string()]);
        let report = run_preflight_with_targets(&scenes, None, None, Some(&targets)).unwrap();
        assert_eq!(report.warnings_count(), 1);
        assert!(report.issues[0].message.contains("The Lost Harbor"));
    }

    #[test]
    fn wiki_review_ignores_code_private_and_escaped_links_and_refuses_partial_reports() {
        let targets = BTreeSet::new();
        let text = "λ [[Public]]\n\n`[[inline]]`\n\n```\n[[fenced]]\n```\n\n%% [[private]] %% \\[[escaped]]";
        let scenes = vec![(1, "Scene".into(), text.into())];
        let report = run_preflight_with_targets(&scenes, None, None, Some(&targets)).unwrap();
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].line, Some(1));
        assert!(report.issues[0].message.contains("Public"));
        let many = vec![(1, "Scene".into(), "[[missing]] ".repeat(513))];
        assert!(run_preflight_with_targets(&many, None, None, Some(&targets)).is_err());
    }

    #[test]
    fn footnote_issues_point_to_original_unicode_lines_without_private_false_positives() {
        let scenes = vec![(
            7,
            "Annotations".into(),
            "λ introduction\nMissing[^lost] and cited[^repeated].\n\n[^unused]: No citation.\n\n[^Repeated]: First.\n\n[^repeated]: Second.\n\n%% [^private]: secret %%".into(),
        )];
        let report = run_preflight(&scenes, None, None).unwrap();
        let footnotes: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.category == "Footnotes")
            .collect();
        assert_eq!(footnotes.len(), 3);
        assert_eq!(
            footnotes
                .iter()
                .find(|i| i.message.contains("lost"))
                .unwrap()
                .line,
            Some(2)
        );
        assert_eq!(
            footnotes
                .iter()
                .find(|i| i.message.contains("unused"))
                .unwrap()
                .line,
            Some(4)
        );
        assert_eq!(
            footnotes
                .iter()
                .find(|i| i.message.contains("repeated"))
                .unwrap()
                .line,
            Some(6)
        );
        assert!(footnotes.iter().all(|i| i.scene_id == Some(7)));
    }

    #[test]
    fn an_unclosed_note_marker_warns_that_the_lines_after_it_export() {
        // Grimoire hides an unclosed %% only to the end of its line; the lines after export.
        assert_eq!(
            unclosed_comment_delimiter_line(
                "Prose.
%% meant to span
these lines
"
            ),
            Some(2)
        );
        for quiet in [
            "Prose %% closed %% here.
",
            "Prose %% a note
across lines %% ends.
",
            "Rate: `100%%` exactly.
",
            "```
fenced %% code
```
",
            "No markers at all.
",
        ] {
            assert_eq!(unclosed_comment_delimiter_line(quiet), None, "{quiet:?}");
        }
    }

    #[test]
    fn unified_preflight_validates_markdown_links_assets_unclosed_delimiters_and_review_notes() {
        let root =
            std::env::temp_dir().join(format!("wros-unified-preflight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/map.png"), b"png").unwrap();

        let mut review_notes = ReviewNotes::default();
        let anchor = crate::review_notes::Anchor::capture("The map was torn.", 0, 7).unwrap();
        review_notes
            .add("manuscript/01-Scene.md", anchor, "check map detail".into())
            .unwrap();

        let targets = BTreeSet::from(["known-node".to_string()]);
        let text = "The map was torn. See [empty]() and [broken](missing.md) and [ok](assets/map.png).\n\
                    ![ok](assets/map.png) ![missing](assets/absent.png) ![remote](https://example.invalid/img.png)\n\
                    Unclosed %% secret tail";
        let scenes = vec![(1, "manuscript/01-Scene.md".into(), text.into())];
        let report = run_unified_preflight(
            &scenes,
            &PreflightContext {
                continuity: None,
                annotations: None,
                review_notes: Some(&review_notes),
                known_targets: Some(&targets),
                project_root: Some(&root),
            },
        )
        .unwrap();

        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Privacy" && i.line == Some(3))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Links" && i.message.contains("Empty Markdown link"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Links" && i.message.contains("missing.md"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Assets" && i.message.contains("assets/absent.png"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Assets" && i.message.contains("Remote image URL"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.category == "Revisions" && i.message.contains("1 unresolved"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_preflight_passes() {
        let scenes = vec![(
            1,
            "Chapter 1".into(),
            "The quiet forest breathed slowly under the summer sun.".into(),
        )];

        let report = run_preflight(&scenes, None, None).unwrap();
        assert!(report.is_clean());
    }
}
