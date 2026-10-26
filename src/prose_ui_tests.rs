//! Genuine native + installed pinned Python UI contracts, not fabricated NLP
//! results. Temporary novels/HOME only; no root app or user manuscript.
use super::*;
use app::prose_dashboard::{Filter, Page as ProsePage};
use grimoire_core::{diagnostics::Profile, prose_tools, review_queue};
use std::time::{Duration, Instant};
const PROSE_TEXT: &str = "Café 🦊 e\u{301}. We waited in order to see. He could of gone.\n";
fn tools_runtime() -> PathBuf {
    let path = std::env::var_os("WRITINGDECK_TOOLS_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("build/prose-runtime/runtime/bin/python3")
        });
    assert!(
        path.is_file(),
        "provision the documented pinned Python runtime; never silently skip genuine engine integration"
    );
    path
}
fn prose_desk(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    metric_clock(&mut d);
    d.app.editor.set_text(PROSE_TEXT);
    d.app.flush();
    d.app.save();
    d.app.prose_dashboard.runtime.python = tools_runtime();
    d
}
fn wait_prose(d: &mut Desk) {
    let limit = Instant::now() + Duration::from_secs(30);
    while d.app.prose_dashboard.result.is_none() {
        d.app.tick_prose_dashboard();
        d.draw();
        assert!(Instant::now() < limit, "{}", d.app.prose_dashboard.notice);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn select_prose(d: &mut Desk, engine: &str, rule: &str) {
    d.app.prose_dashboard.filter = Filter::All;
    let selected = d
        .app
        .prose_dashboard
        .visible()
        .iter()
        .position(|i| {
            i.provenance
                .iter()
                .any(|r| r.engine == engine && r.rule == rule)
        })
        .expect("actual current engine finding");
    d.app.prose_dashboard.page = ProsePage::Queue;
    d.app.prose_dashboard.selected = selected;
    d.key(KeyCode::Enter);
    assert_eq!(d.app.prose_dashboard.page, ProsePage::Detail);
}
#[test]
fn real_prose_current_unicode_original_replace_and_exact_undo_at_four_sizes_without_typing_credit()
{
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = prose_desk(&format!("prose-real-{w}"), w, h);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let before = fs::read(&path).unwrap();
        d.app.editor.select((0, 0), (0, 4));
        let selection = d.app.editor.selection();
        assert!(d.app.checkpoint_productivity(true));
        let known = d.app.productivity.ledger.known.len();
        d.app.run_action(palette::Action::ProseDashboard);
        assert_eq!(d.app.prose_dashboard.launched, 0);
        assert!(d.app.prose_dashboard.result.is_none());
        d.key(KeyCode::Char('p'));
        assert_eq!(d.app.checks.options().profile, Profile::Revision);
        d.key(KeyCode::Char('r'));
        wait_prose(&mut d);
        assert!(
            d.app
                .current_prose_analysis()
                .unwrap()
                .services
                .iter()
                .any(|s| s.engine == prose_tools::Engine::Proselint
                    && s.status == prose_tools::Status::Complete)
        );
        assert!(
            !d.app
                .current_prose_analysis()
                .unwrap()
                .services
                .iter()
                .any(|s| s.engine == prose_tools::Engine::Nabokov)
        );
        assert!(
            d.app
                .prose_dashboard
                .voices
                .as_ref()
                .unwrap()
                .value
                .profiles
                .is_empty()
        );
        assert_eq!(d.app.editor.selection(), selection);
        select_prose(&mut d, "writingdeck-native", "wordy-phrase");
        let item = d.app.prose_dashboard.visible()[d.app.prose_dashboard.selected];
        assert_eq!(item.finding.expected, "in order to");
        let snapshot = &d.app.current_prose_analysis().unwrap().native.snapshot;
        let (row, col) = snapshot.point(item.finding.span.start).unwrap();
        let (last, end) = snapshot.point(item.finding.span.end).unwrap();
        assert_eq!(row, last);
        assert!(
            d.app
                .check_line_spans()
                .get(&row)
                .unwrap()
                .contains(&(col, end)),
            "native editor receives original scalar spans after multibyte emoji and combining accent"
        );
        assert_eq!(
            item.finding.context,
            grimoire_core::diagnostics::Context::Narration
        );
        d.ctrl('x');
        d.ctrl('z');
        d.ctrl('v');
        d.app.paste("awxmy pasted commands");
        d.app.on_drag(20, 10);
        assert_eq!(d.app.editor.text(), PROSE_TEXT);
        assert_eq!(d.app.editor.selection(), selection);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(d.app.checkpoint_productivity(true));
        assert_eq!(
            d.app.productivity.ledger.known.len(),
            known,
            "analysis/review never becomes authored input"
        );
        d.key(KeyCode::Char('a'));
        assert_eq!(
            d.app.editor.text(),
            PROSE_TEXT.replacen("in order to", "to", 1)
        );
        d.app.save();
        assert!(d.app.checkpoint_productivity(true));
        assert!(
            d.app
                .productivity
                .ledger
                .known
                .iter()
                .any(|e| e.origin == grimoire_core::measure::Origin::Replace)
        );
        assert!(
            !d.app
                .productivity
                .ledger
                .known
                .iter()
                .any(|e| e.origin == grimoire_core::measure::Origin::Typing)
        );
        d.key(KeyCode::Esc);
        d.ctrl('z');
        d.app.save();
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(
            d.app.check_line_spans().is_empty(),
            "Undo must not revive a stale cached style underline"
        );
    }
}
#[test]
fn actual_nabokov_permission_is_separate_one_run_and_rewrites_are_not_literal_alternatives() {
    let mut d = prose_desk("prose-model", 100, 30);
    let personal = d.root.join("personal-profile");
    fs::create_dir_all(&personal).unwrap();
    d.app.checks.use_personal_root(&personal);
    d.app
        .editor
        .set_text("Café 🦊 e\u{301}. The gate was opened by Mara in order to see.\n");
    d.app.flush();
    d.app.save();
    let before = d.app.editor.text();
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('d'));
    assert_eq!(d.app.prose_dashboard.page, ProsePage::ModelConsent);
    assert_eq!(d.app.prose_dashboard.launched, 0);
    assert!(
        d.app
            .checks
            .novel
            .as_ref()
            .unwrap()
            .value
            .live_consent
            .is_none()
    );
    assert!(
        d.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .allowed_runtime
            .is_none()
    );
    d.key(KeyCode::Char('y'));
    wait_prose(&mut d);
    let result = d.app.current_prose_analysis().unwrap();
    let nlp = result
        .services
        .iter()
        .find(|s| s.engine == prose_tools::Engine::Nabokov)
        .unwrap();
    assert_eq!(nlp.status, prose_tools::Status::Complete);
    assert!(nlp.peak_rss_kib > 0);
    let passive = nlp
        .findings
        .iter()
        .find(|f| f.rule.rule == "NB302")
        .unwrap();
    assert_eq!(passive.expected, "was opened by Mara");
    assert!(passive.replacements.is_empty());
    let wordy = result
        .queue
        .items
        .iter()
        .find(|i| i.provenance.iter().any(|r| r.rule == "NB401"))
        .unwrap();
    assert!(
        wordy
            .provenance
            .iter()
            .any(|r| r.engine == "writingdeck-native"),
        "equivalent finding provenance retained"
    );
    select_prose(&mut d, "nabokov", "NB302");
    d.key(KeyCode::Char('a'));
    assert_eq!(d.app.editor.text(), before);
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    assert!(
        !d.app
            .current_prose_analysis()
            .unwrap()
            .services
            .iter()
            .any(|s| s.engine == prose_tools::Engine::Nabokov),
        "r never reuses the consumed model permission"
    );
    assert!(
        d.app
            .checks
            .novel
            .as_ref()
            .unwrap()
            .value
            .live_consent
            .is_none()
    );
    assert!(
        d.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .allowed_runtime
            .is_none()
    );
}
#[test]
fn edits_undo_reopened_owners_and_backing_conflicts_reject_current_or_late_findings() {
    let mut d = prose_desk("prose-stale", 100, 30);
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    select_prose(&mut d, "writingdeck-native", "wordy-phrase");
    d.app.editor.insert('x');
    d.app.editor.undo();
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    assert!(
        d.app.current_prose_analysis().is_none(),
        "restored bytes cannot restore a stale editor epoch"
    );
    d.key(KeyCode::Char('a'));
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    select_prose(&mut d, "writingdeck-native", "wordy-phrase");
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let changed = fs::read_to_string(&path)
        .unwrap()
        .replace("in order to", "in order not to");
    fs::write(&path, &changed).unwrap();
    d.key(KeyCode::Char('a'));
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    assert_eq!(fs::read_to_string(path).unwrap(), changed);
}
#[test]
fn missing_runtime_and_future_metadata_are_visible_not_fake_empty_clean_or_overwritten() {
    let mut d = prose_desk("prose-unavailable", 80, 24);
    d.app.prose_dashboard.runtime.python = PathBuf::from("/not-a-python-runtime");
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    assert_eq!(
        d.app.current_prose_analysis().unwrap().services[0].status,
        prose_tools::Status::Unavailable
    );
    select_prose(&mut d, "writingdeck-native", "wordy-phrase");
    let path = d.root.join(review_queue::RELATIVE);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = b"{\"schema_version\":99,\"keep\":\"opaque\"}";
    fs::write(&path, future).unwrap();
    d.key(KeyCode::Char('w'));
    assert_eq!(fs::read(path).unwrap(), future);
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    let path = d.root.join(".grimoire/writingdeck/prose-settings.json");
    fs::write(&path, future).unwrap();
    d.app.prose_dashboard = app::prose_dashboard::Dashboard::load(&d.root);
    assert!(d.app.prose_dashboard.settings.is_none());
    d.key(KeyCode::Char('f'));
    assert_eq!(fs::read(path).unwrap(), future);
}
#[test]
fn actual_review_receipts_and_version_bound_novel_mute_preserve_original_source() {
    let mut d = prose_desk("prose-review", 100, 30);
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    select_prose(&mut d, "proselint", "spelling.ve_of");
    d.key(KeyCode::Char('w'));
    assert_eq!(
        d.app
            .prose_dashboard
            .reviews
            .as_ref()
            .unwrap()
            .value
            .receipts
            .len(),
        1
    );
    let bytes = fs::read_to_string(d.root.join(review_queue::RELATIVE)).unwrap();
    assert!(!bytes.contains("could of") && !bytes.contains("Café"));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    let reviewed = d
        .app
        .current_prose_analysis()
        .unwrap()
        .queue
        .items
        .iter()
        .find(|i| i.finding.rule.rule == "spelling.ve_of")
        .unwrap();
    assert_eq!(reviewed.state, Some(review_queue::State::Reviewed));
    select_prose(&mut d, "proselint", "spelling.ve_of");
    d.key(KeyCode::Char('m'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    assert!(
        !d.app
            .current_prose_analysis()
            .unwrap()
            .queue
            .items
            .iter()
            .any(|i| i.finding.rule.rule == "spelling.ve_of")
    );
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    assert!(
        d.app
            .checks
            .novel
            .as_ref()
            .unwrap()
            .value
            .options
            .disabled_rules
            .iter()
            .any(|r| r.engine == "proselint"
                && r.version == prose_tools::Engine::Proselint.version())
    );
}
#[test]
fn queued_requests_coalesce_and_genuine_model_work_on_an_older_edit_is_never_published() {
    let mut d = prose_desk("prose-queue", 100, 30);
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('d'));
    d.key(KeyCode::Char('y'));
    d.key(KeyCode::Char('r'));
    d.key(KeyCode::Char('r'));
    d.key(KeyCode::Char('r'));
    assert_eq!(d.app.prose_dashboard.launched, 1);
    wait_prose(&mut d);
    assert_eq!(d.app.prose_dashboard.launched, 2);
    assert_eq!(d.app.prose_dashboard.discarded, 1);
    d.key(KeyCode::Char('d'));
    d.key(KeyCode::Char('y'));
    d.app.editor.insert('x');
    let limit = Instant::now() + Duration::from_secs(30);
    while d.app.prose_dashboard.pending() {
        d.app.tick_prose_dashboard();
        assert!(Instant::now() < limit);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(d.app.prose_dashboard.result.is_none());
    assert!(d.app.prose_dashboard.discarded >= 2);
}
#[test]
fn readonly_paste_help_and_rendering_cannot_execute_commands_or_restart_a_worker() {
    let mut d = prose_desk("prose-help", 80, 24);
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    let launched = d.app.prose_dashboard.launched;
    d.key(KeyCode::F(1));
    let Overlay::Help { sel, .. } = &d.app.overlay else {
        panic!("context help reader not opened")
    };
    assert_eq!(crate::help::TOPICS[*sel].id, "prose-dashboard");
    d.draw();
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::ProseDashboard);
    for _ in 0..10 {
        d.draw();
    }
    assert_eq!(d.app.prose_dashboard.launched, launched);
    d.app.paste("adymqx pasted commands");
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
    assert_eq!(d.app.prose_dashboard.launched, launched);
}
#[test]
fn original_lessons27_to29_require_actual_stats_and_engine_findings_not_navigation_or_permission() {
    let mut d = prose_desk("prose-lessons", 100, 30);
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 27 });
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&27));
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&27));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    assert!(!d.app.deck.progress.completed.contains(&27));
    d.key(KeyCode::Enter);
    assert!(d.app.deck.progress.completed.contains(&27));
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 29 });
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    assert!(!d.app.deck.progress.completed.contains(&29));
    select_prose(&mut d, "proselint", "spelling.ve_of");
    assert!(d.app.deck.progress.completed.contains(&29));
    d.app.editor.set_text("The gate was opened by Mara.\n");
    d.app.flush();
    d.app.save();
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 28 });
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('d'));
    assert!(!d.app.deck.progress.completed.contains(&28));
    d.key(KeyCode::Char('y'));
    wait_prose(&mut d);
    assert!(!d.app.deck.progress.completed.contains(&28));
    select_prose(&mut d, "nabokov", "NB302");
    assert!(d.app.deck.progress.completed.contains(&28));
    assert!(
        !d.app.deck.progress.completed.contains(&26)
            && !d.app.deck.progress.completed.contains(&40)
    );
}
#[test]
fn real_selected_material_voice_is_not_completed_by_prepare_save_or_menu_only_and_stores_no_prose()
{
    let mut d = prose_desk("prose-voice", 100, 30);
    // Explicit synthetic threshold fixture, not a real author's voice/identity.
    let material = (0..40)
        .map(|i| {
            format!(
                "{}.",
                (0..130)
                    .map(|j| format!("lexeme{}", (j + i) % 160))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    d.app.editor.set_text(&material);
    d.app.flush();
    d.app.save();
    let original = d.app.editor.text();
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 30 });
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('n'));
    let source = d.app.open.unwrap();
    d.app.prose_dashboard.selected = d
        .app
        .prose_dashboard
        .sources
        .iter()
        .position(|i| *i == source)
        .unwrap();
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.prose_dashboard.page, ProsePage::VoiceName);
    d.key(KeyCode::Delete);
    d.app.paste("Chosen reference");
    assert_eq!(d.app.editor.text(), original);
    d.key(KeyCode::Enter);
    let limit = Instant::now() + Duration::from_secs(15);
    while d.app.prose_dashboard.prepared.is_none() {
        d.app.tick_prose_dashboard();
        assert!(Instant::now() < limit, "{}", d.app.prose_dashboard.notice);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!d.app.deck.progress.completed.contains(&30));
    d.key(KeyCode::Char('s'));
    assert!(!d.app.deck.progress.completed.contains(&30));
    let bytes = fs::read_to_string(d.root.join(grimoire_core::voice_profile::RELATIVE)).unwrap();
    assert!(!bytes.contains("lexeme") && bytes.contains("sha256"));
    assert_eq!(d.app.editor.text(), original);
    d.key(KeyCode::Esc);
    d.app
        .editor
        .set_text(&"I saw the sea. We waited for her.\n\n".repeat(60));
    d.app.flush();
    d.app.save();
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    d.key(KeyCode::Char('v'));
    d.key(KeyCode::Char('c'));
    assert!(d.app.deck.progress.completed.contains(&30));
    assert!(
        d.app
            .prose_dashboard
            .notice
            .contains("informational feature deviations")
    );
}

#[test]
fn completed_style_cache_is_really_painted_on_the_original_editor_then_explicitly_cleared() {
    let mut d = prose_desk("prose-native-editor", 100, 30);
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('r'));
    wait_prose(&mut d);
    let spans = d.app.check_line_spans();
    assert!(!spans.is_empty());
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    d.draw();
    assert_eq!(d.app.check_line_spans(), spans);
    assert!(
        d.term
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|c| c.modifier.contains(Modifier::UNDERLINED)
                && c.underline_color == d.app.theme.moon),
        "style marks must actually reach the visible original editor, not just a covered reader cache"
    );
    let launched = d.app.prose_dashboard.launched;
    d.app.open_prose_dashboard();
    d.key(KeyCode::Char('c'));
    assert!(d.app.check_line_spans().is_empty());
    assert_eq!(d.app.prose_dashboard.launched, launched);
    assert_eq!(d.app.editor.text(), PROSE_TEXT);
}
