use super::*;
use super::{
    harper::{HarperEngine, Report, ReportStatus},
    preferences::{CONSENT_VERSION, NovelPreferences, PersonalPreferences},
    worker::Worker,
};
use crate::statefile::Validate;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

fn snapshot(text: &str) -> Snapshot {
    Snapshot::new(
        Path::new("chapters/café.md"),
        SourceUnit::EditorBody,
        SourceRole::Manuscript,
        1,
        text,
    )
    .unwrap()
}
fn span(text: &str, needle: &str) -> ByteSpan {
    let start = text.find(needle).unwrap();
    (start..start + needle.len()).into()
}
fn fixture(tag: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "wd-checks-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn diagnostic(source: &Snapshot, range: ByteSpan) -> Diagnostic {
    let options = AnalysisOptions::default();
    Diagnostic::new(
        &source.project(&options),
        &options,
        RuleKey::harper("AnA"),
        Category::Grammar,
        range,
        "Review this article.".into(),
        vec!["an".into()],
    )
    .unwrap()
}

#[test]
fn unicode_coordinates_are_exact_in_every_declared_encoding() {
    let raw = "café 🦊 e\u{301} – ‘quote’ 漢\r\nnext\n";
    let source = snapshot(raw);
    let projection = source.project(&AnalysisOptions::default());
    for surface in [Surface::NativeMarkdown, Surface::PlainProse] {
        let input = match surface {
            Surface::NativeMarkdown => &projection.markdown,
            Surface::PlainProse => &projection.prose,
        };
        let c = Coordinates::new(input);
        for (i, &byte) in c.bytes.iter().enumerate() {
            let expected = source.scalar_span(i..i).unwrap();
            assert_eq!(
                projection.input_byte_span(surface, byte..byte),
                Some(expected)
            );
            assert_eq!(
                projection.input_utf16_span(surface, c.utf16[i]..c.utf16[i]),
                Some(expected)
            );
            let (line, col) = c.point(byte).unwrap();
            let origin = c.lines[line];
            for (encoding, column) in [
                (ColumnEncoding::Utf8Bytes, byte - c.bytes[origin]),
                (ColumnEncoding::Utf16, c.utf16[i] - c.utf16[origin]),
            ] {
                assert_eq!(
                    projection.input_line_span(surface, (line, column), (line, column), encoding),
                    Some(expected)
                );
            }
            assert_eq!(
                projection.input_line_span(
                    surface,
                    (line, col),
                    (line, col),
                    ColumnEncoding::Scalars
                ),
                Some(expected)
            );
            assert_eq!(
                source.byte_at(
                    source.point(expected.start).unwrap().0,
                    source.point(expected.start).unwrap().1
                ),
                Some(expected.start)
            );
        }
    }
    let fox = span(raw, "🦊");
    assert_eq!(source.slice(fox), Some("🦊"));
    let utf16_start = raw[..fox.start].encode_utf16().count();
    assert!(
        projection
            .input_utf16_span(Surface::NativeMarkdown, utf16_start + 1..utf16_start + 2)
            .is_none()
    );
    assert!(
        projection
            .input_byte_span(Surface::NativeMarkdown, fox.start + 1..fox.end)
            .is_none()
    );
    assert!(source.point(fox.start + 1).is_none());
    assert!(source.scalar_span(Range { start: 6, end: 5 }).is_none());
    assert!(
        projection
            .input_line_span(
                Surface::NativeMarkdown,
                (0, 999),
                (1, 0),
                ColumnEncoding::Scalars
            )
            .is_none()
    );
    assert!(source.scalar_span(0..source.scalar_len() + 1).is_none());
}
#[test]
fn masked_multibyte_source_uses_input_tables_not_raw_byte_or_utf16_indices() {
    let raw = "%% 🦊 漢 hidden %% café a apple.";
    let source = snapshot(raw);
    let projection = source.project(&AnalysisOptions::default());
    let source_span = span(raw, "a apple");
    let needle = projection.markdown.find("a apple").unwrap();
    assert_ne!(needle, source_span.start);
    assert_eq!(
        projection.input_byte_span(Surface::NativeMarkdown, needle..needle + 7),
        Some(source_span)
    );
    let utf16 = projection.markdown[..needle].encode_utf16().count();
    assert_eq!(
        projection.input_utf16_span(Surface::NativeMarkdown, utf16..utf16 + 7),
        Some(source_span)
    );
    assert_eq!(
        projection.input_line_span(
            Surface::NativeMarkdown,
            (0, needle),
            (0, needle + 7),
            ColumnEncoding::Utf8Bytes
        ),
        Some(source_span)
    );
    assert!(!projection.readable_span(span(raw, "hidden")));
    assert_eq!(source.text(), raw);
}
#[test]
fn original_markdown_and_frontmatter_offsets_survive_without_stripping() {
    let raw = "---\ntitle: ‘An dog 🦊’\n---\n\n# Café\n\nA **quoted** word and [a label](https://example.test/an-dog) with `an dog`.\n\n```text\nAn dog.\n```\n\n%% An dog. %% TK\n\nA footnote.[^x]\n\n[^x]: An dog.\n";
    let source = Snapshot::new(
        Path::new("draft.md"),
        SourceUnit::MarkdownFile,
        SourceRole::Manuscript,
        4,
        raw,
    )
    .unwrap();
    let projection = source.project(&AnalysisOptions::default());
    assert_eq!(projection.markdown.chars().count(), raw.chars().count());
    assert_eq!(projection.prose.chars().count(), raw.chars().count());
    assert!(!projection.prose.contains("title"));
    assert!(!projection.prose.contains("https"));
    assert!(!projection.prose.contains("An dog"));
    assert!(!projection.prose.contains("TK"));
    assert!(projection.prose.contains("a label"));
    assert!(projection.markdown.contains("**quoted**"));
    assert!(!projection.replaceable_span(span(raw, "**quoted**")));
    assert!(projection.replaceable_span(span(raw, "quoted")));
    let include = AnalysisOptions {
        include_footnotes: true,
        ..AnalysisOptions::default()
    };
    let with_footnote = source.project(&include);
    let foot = raw.rfind("An dog").unwrap();
    assert!(with_footnote.prose.contains("An dog"));
    assert_eq!(
        with_footnote.context((foot..foot + 2).into()),
        Context::Footnote
    );
    assert!(!with_footnote.replaceable_span(span(raw, "[^x]")));
}
#[test]
fn dialogue_is_still_prose_and_apostrophes_are_not_quotes() {
    let raw = "The writer’s book. “I have a apple,” she said.\n— An dog barks.\nThe end.";
    let p = snapshot(raw).project(&AnalysisOptions::default());
    assert_eq!(p.context(span(raw, "writer’s")), Context::Narration);
    assert_eq!(p.context(span(raw, "a apple")), Context::Dialogue);
    assert_eq!(p.context(span(raw, "An dog")), Context::Dialogue);
    assert_eq!(p.context(span(raw, "The end")), Context::Narration);
    assert!(p.replaceable_span(span(raw, "a apple")));
}
#[test]
fn note_and_research_scopes_are_explicit_not_silently_treated_as_manuscript() {
    for role in [SourceRole::Note, SourceRole::Research] {
        let source = Snapshot::new(
            Path::new("notes.md"),
            SourceUnit::EditorBody,
            role,
            1,
            "An dog.",
        )
        .unwrap();
        let mut options = AnalysisOptions::default();
        assert!(!source.project(&options).has_prose());
        options.include_notes = true;
        options.include_research = true;
        assert!(source.project(&options).has_prose());
        assert_eq!(
            source.project(&options).context(span(source.text(), "An")),
            if role == SourceRole::Note {
                Context::Note
            } else {
                Context::Research
            }
        );
    }
}
#[test]
fn escaping_entities_and_markup_can_be_located_but_not_destructively_replaced() {
    for raw in ["a **apple**", "a &amp; apple", "a \\*apple\\*"] {
        let source = snapshot(raw);
        let finding = diagnostic(&source, (0..raw.len()).into());
        assert!(!finding.replacements[0].source_safe, "{raw}");
        assert!(
            finding
                .checked_replacement(&source, &AnalysisOptions::default(), 0)
                .is_err()
        );
    }
}
#[test]
fn explicit_replacement_verifies_source_generation_document_unit_role_and_policy() {
    let source = snapshot("café has a apple.");
    let finding = diagnostic(&source, span(source.text(), "a apple"));
    let options = AnalysisOptions::default();
    assert_eq!(
        finding.checked_replacement(&source, &options, 0).unwrap().1,
        "an"
    );
    for changed in [
        Snapshot::new(
            Path::new("other.md"),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            1,
            source.text(),
        )
        .unwrap(),
        Snapshot::new(
            Path::new(&source.stamp.document),
            SourceUnit::MarkdownFile,
            SourceRole::Manuscript,
            1,
            source.text(),
        )
        .unwrap(),
        Snapshot::new(
            Path::new(&source.stamp.document),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            2,
            source.text(),
        )
        .unwrap(),
        Snapshot::new(
            Path::new(&source.stamp.document),
            SourceUnit::EditorBody,
            SourceRole::Research,
            1,
            source.text(),
        )
        .unwrap(),
        Snapshot::new(
            Path::new(&source.stamp.document),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            1,
            "café had a apple.",
        )
        .unwrap(),
    ] {
        assert!(finding.checked_replacement(&changed, &options, 0).is_err());
    }
    let mut changed = options.clone();
    changed.include_footnotes = true;
    assert!(finding.checked_replacement(&source, &changed, 0).is_err());
    assert!(finding.checked_replacement(&source, &options, 9).is_err());
    assert_eq!(source.text(), "café has a apple.");
}
#[test]
fn snapshots_rule_keys_and_replacements_reject_invalid_boundaries_and_controls() {
    for path in [
        "",
        "/scene.md",
        "../scene.md",
        "a/../scene.md",
        "./scene.md",
        "bad\n.md",
    ] {
        assert!(
            Snapshot::new(
                Path::new(path),
                SourceUnit::EditorBody,
                SourceRole::Manuscript,
                1,
                "ok"
            )
            .is_err()
        );
    }
    assert!(
        Snapshot::new(
            Path::new("big.md"),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            1,
            "x".repeat(MAX_SOURCE_BYTES + 1)
        )
        .is_err()
    );
    assert!(RuleKey::harper("bad\u{1b}rule").validate().is_err());
    let source = snapshot("a apple");
    let p = source.project(&AnalysisOptions::default());
    let finding = Diagnostic::new(
        &p,
        &AnalysisOptions::default(),
        RuleKey::harper("AnA"),
        Category::Grammar,
        (0..1).into(),
        "review".into(),
        vec![
            "a".into(),
            "an".into(),
            "an".into(),
            "\u{1b}[0m".into(),
            "x".repeat(MAX_REPLACEMENT_BYTES + 1),
        ],
    )
    .unwrap();
    assert_eq!(finding.replacements.len(), 1);
    assert_eq!(finding.replacements[0].text, "an");
}
#[test]
fn merely_constructing_or_skipping_an_engine_loads_no_models_or_dictionary() {
    let mut engine = HarperEngine::default();
    assert!(!engine.is_loaded());
    for source in [
        snapshot(""),
        snapshot("```text\nAn dog.\n```"),
        Snapshot::new(
            Path::new("n.md"),
            SourceUnit::EditorBody,
            SourceRole::Note,
            1,
            "An dog.",
        )
        .unwrap(),
    ] {
        let report = engine.analyze(source, AnalysisOptions::default());
        assert!(report.findings.is_empty());
        assert!(!engine.is_loaded());
    }
    let report = engine.analyze(
        snapshot(&"x".repeat(harper::MAX_HARPER_BYTES + 1)),
        AnalysisOptions::default(),
    );
    assert_eq!(report.status, ReportStatus::TooLarge);
    assert!(!engine.is_loaded());
}
#[test]
fn actual_harper_is_pinned_local_source_mapped_and_separate_from_spelling() {
    let raw = "---\ntitle: ‘An dog 🦊 漢’\n---\n\n# Café\n\n%% An dog. %%\n\n`An dog.`\n\n```text\nAn dog.\n```\n\n“I have a apple,” she said. [Label](https://example.test/an-dog) 🦊 e\u{301}.\n\nA note.[^n]\n\n[^n]: An dog waits.\n";
    let source = Snapshot::new(
        Path::new("draft.md"),
        SourceUnit::MarkdownFile,
        SourceRole::Manuscript,
        7,
        raw,
    )
    .unwrap();
    let mut engine = HarperEngine::default();
    let options = AnalysisOptions::default();
    let report = engine.analyze(source.clone(), options.clone());
    assert_eq!(report.status, ReportStatus::Complete);
    let articles: Vec<_> = report
        .findings
        .iter()
        .filter(|d| d.rule.rule == "AnA")
        .collect();
    assert_eq!(articles.len(), 1, "{:?}", report.findings);
    let article = articles[0];
    assert_eq!(
        article.span,
        (raw.find("a apple").unwrap()..raw.find("a apple").unwrap() + 1).into()
    );
    assert_eq!(article.context, Context::Dialogue);
    assert_eq!(article.expected, "a");
    assert_eq!(article.rule.version, HARPER_VERSION);
    assert_eq!(
        article.checked_replacement(&source, &options, 0).unwrap().1,
        "an"
    );
    assert!(engine.is_loaded());
    assert!(report.findings.iter().all(|d| d.rule.rule != "SpellCheck"));
    let mut include = options.clone();
    include.include_footnotes = true;
    let report = engine.analyze(source.clone(), include);
    let foot = raw.rfind("An dog").unwrap();
    assert!(
        report.findings.iter().any(|d| d.rule.rule == "AnA"
            && d.span.start == foot
            && d.context == Context::Footnote)
    );
    let mut muted = options.clone();
    muted.disabled_rules.insert(RuleKey::harper("AnA"));
    assert!(
        engine
            .analyze(source.clone(), muted)
            .findings
            .iter()
            .all(|d| d.rule.rule != "AnA")
    );
    let mut wrong_version = options;
    let mut old = RuleKey::harper("AnA");
    old.version = "old-version".into();
    wrong_version.disabled_rules.insert(old);
    assert!(
        engine
            .analyze(source.clone(), wrong_version)
            .findings
            .iter()
            .any(|d| d.rule.rule == "AnA")
    );
    assert_eq!(source.text(), raw);
}
#[test]
fn preferences_default_to_no_consent_no_creation_and_scoped_identifier_only_mutes() {
    let a = fixture("novel-a");
    let b = fixture("novel-b");
    let home = fixture("personal");
    let mut novel = NovelPreferences::load(&a).unwrap();
    let mut personal = PersonalPreferences::load(&home).unwrap();
    assert!(!novel.exists());
    assert!(!novel.value.live_enabled());
    assert!(!a.join(".grimoire").exists());
    assert!(!home.join(".grimoire").exists());
    novel
        .update(|p| {
            p.live_consent = Some(CONSENT_VERSION.into());
            p.options.disabled_rules.insert(RuleKey::harper("AnA"));
            Ok(())
        })
        .unwrap();
    personal
        .update(|p| {
            p.muted_rules.insert(RuleKey::harper("RepeatedWords"));
            Ok(())
        })
        .unwrap();
    let a_options = NovelPreferences::load(&a)
        .unwrap()
        .value
        .effective(&personal.value);
    let b_options = NovelPreferences::load(&b)
        .unwrap()
        .value
        .effective(&PersonalPreferences::load(&home).unwrap().value);
    assert!(a_options.disabled_rules.contains(&RuleKey::harper("AnA")));
    assert!(!b_options.disabled_rules.contains(&RuleKey::harper("AnA")));
    assert!(
        b_options
            .disabled_rules
            .contains(&RuleKey::harper("RepeatedWords"))
    );
    assert!(NovelPreferences::load(&a).unwrap().value.live_enabled());
    let bytes = fs::read_to_string(a.join(preferences::NOVEL_FILE)).unwrap();
    assert!(!bytes.contains("apple"));
    assert!(!bytes.contains("expected"));
    assert!(!bytes.contains("snippet"));
    let mut old = novel.value.clone();
    old.live_consent = Some("older-engine".into());
    assert!(old.validate().is_ok());
    assert!(!old.live_enabled());
    for root in [a, b, home] {
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn corrupt_future_unknown_and_external_preferences_are_preserved() {
    let root = fixture("preferences");
    let path = root.join(preferences::NOVEL_FILE);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    for bytes in [
        b"{broken".to_vec(),
        b"{\"version\":9}".to_vec(),
        b"{\"version\":1,\"unknown\":true}".to_vec(),
        vec![b'x'; crate::statefile::MAX_BYTES + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(NovelPreferences::load(&root).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    let mut store = NovelPreferences::load(&root).unwrap();
    let external = serde_json::to_vec(&NovelPreferences::default()).unwrap();
    fs::write(&path, &external).unwrap();
    assert!(
        store
            .update(|p| {
                p.live_consent = Some(CONSENT_VERSION.into());
                Ok(())
            })
            .is_err()
    );
    assert!(!store.value.live_enabled());
    assert_eq!(fs::read(&path).unwrap(), external);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn worker_discards_inflight_old_revisions_and_keeps_only_two_pending_documents() {
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let worker = Worker::with_analyzer(move |source, options| {
        if source.stamp.document == "chapters/café.md" && source.stamp.generation == 1 {
            entered.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Report::empty(source, options, ReportStatus::Complete)
    })
    .unwrap();
    worker
        .submit(snapshot("first"), AnalysisOptions::default())
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut current = snapshot("new revision");
    current.stamp.generation = 2;
    worker
        .submit(current.clone(), AnalysisOptions::default())
        .unwrap();
    let second = Snapshot::new(
        Path::new("other.md"),
        SourceUnit::EditorBody,
        SourceRole::Manuscript,
        3,
        "other",
    )
    .unwrap();
    worker.submit(second, AnalysisOptions::default()).unwrap();
    assert_eq!(worker.stats().queued, 2);
    release.send(()).unwrap();
    let one = worker.wait_for_test().unwrap();
    let two = worker.wait_for_test().unwrap();
    assert_eq!(one.snapshot.stamp, current.stamp);
    assert_eq!(two.snapshot.stamp.document, "other.md");
    assert_eq!(worker.stats().discarded, 1);
    assert!(worker.try_recv().unwrap().is_none());
}
#[test]
fn worker_cancellation_panic_and_nonjoining_exit_do_not_touch_source() {
    let worker = Worker::with_analyzer(|source, options| {
        if source.stamp.generation == 1 {
            panic!("intentional analyzer fault");
        }
        Report::empty(source, options, ReportStatus::Complete)
    })
    .unwrap();
    worker
        .submit(snapshot("unchanged"), AnalysisOptions::default())
        .unwrap();
    assert!(matches!(
        worker.wait_for_test().unwrap().status,
        ReportStatus::EngineFailed(_)
    ));
    let mut next = snapshot("unchanged");
    next.stamp.generation = 2;
    worker.submit(next, AnalysisOptions::default()).unwrap();
    assert_eq!(
        worker.wait_for_test().unwrap().status,
        ReportStatus::Complete
    );
    assert_eq!(worker.stats().panics, 1);
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let (done, done_rx) = mpsc::channel();
    let worker = Worker::with_analyzer(move |source, options| {
        entered.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        done.send(()).unwrap();
        Report::empty(source, options, ReportStatus::Complete)
    })
    .unwrap();
    worker
        .submit(snapshot("never saved"), AnalysisOptions::default())
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.cancel("chapters/café.md", SourceUnit::EditorBody);
    drop(worker); // Must not wait for the release or a running inference.
    release.send(()).unwrap();
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
}
#[test]
fn locked_runtime_matches_the_explicit_model_consent_contract() {
    let lock: toml::Value = toml::from_str(include_str!("../../../../Cargo.lock")).unwrap();
    let packages = lock["package"].as_array().unwrap();
    for (name, version) in [
        ("harper-core", "2.13.0"),
        ("harper-brill", "2.13.0"),
        ("harper-pos-utils", "2.13.0"),
    ] {
        assert!(packages.iter().any(|p| p["name"].as_str() == Some(name) && p["version"].as_str() == Some(version)), "update consent contract after reviewing {name} runtime");
    }
}

#[test]
fn third_and_fourth_document_evict_stale_owners_with_a_bounded_queue() {
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let worker = Worker::with_analyzer(move |source, options| {
        if source.stamp.document == "chapters/café.md" {
            entered.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Report::empty(source, options, ReportStatus::Complete)
    })
    .unwrap();
    worker
        .submit(snapshot("first"), AnalysisOptions::default())
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    for (index, expected_eviction) in [(2, false), (3, true), (4, true)] {
        let mut next = snapshot("next");
        next.stamp.document = format!("chapters/{index}.md");
        assert_eq!(
            worker
                .submit(next, AnalysisOptions::default())
                .unwrap()
                .evicted,
            expected_eviction
        );
        assert!(worker.stats().queued <= 2);
    }
    release.send(()).unwrap();
    assert_eq!(
        worker.wait_for_test().unwrap().snapshot.stamp.document,
        "chapters/3.md"
    );
    assert_eq!(
        worker.wait_for_test().unwrap().snapshot.stamp.document,
        "chapters/4.md"
    );
    assert_eq!(worker.stats().submitted, 4);
    assert_eq!(worker.stats().discarded, 1);
}
