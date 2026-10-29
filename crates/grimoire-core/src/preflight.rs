//! Experimental read-only publishing review of supplied scene bodies.
//! Checks footnotes, editorial markers, empty scenes and explicit story models.
//! No export hook, missing-link check or OOXML/PDF footnote writer is implemented.
use crate::annotations::AnnotationStore;
use crate::continuity::ContinuityStore;
use crate::footnotes::analyze_footnotes;
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::BTreeSet;

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

/// Inspect scene bodies supplied by a future compile-scope adapter. The caller
/// must select included scenes and bind IDs/revisions; this does not read files.
pub fn run_preflight(
    scenes: &[(usize, String, String)], // (id, title, markdown)
    continuity: Option<&ContinuityStore>,
    annotations: Option<&AnnotationStore>,
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
    if let Some(store) = annotations {
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
        if let Some(store) = annotations {
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
    }
    if let Some(store) = continuity {
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
        let scenes=vec![(1,"Scene".into(),"Atka Toolkit TODOS FIXMEable. %% TK TODO FIXME %%\n`TODO TK`\n\n```\nFIXME TK\n```\n\nTKTK and TODO here.".into())];
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
        assert_eq!(footnotes.iter().find(|i| i.message.contains("lost")).unwrap().line, Some(2));
        assert_eq!(footnotes.iter().find(|i| i.message.contains("unused")).unwrap().line, Some(4));
        assert_eq!(footnotes.iter().find(|i| i.message.contains("repeated")).unwrap().line, Some(6));
        assert!(footnotes.iter().all(|i| i.scene_id == Some(7)));
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
