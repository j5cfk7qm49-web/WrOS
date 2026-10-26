//! Controller/real terminal-buffer contracts, with explicitly injected contract
//! reports for routing/failure tests and a separately named actual-worker case.
use super::*;
use grimoire_core::diagnostics::{
    self as checks_model, Category, Diagnostic, RuleKey, Snapshot, SourceRole, SourceUnit,
    harper::{Report, ReportStatus},
    preferences::{CONSENT_VERSION, GLOBAL_FILE, NOVEL_FILE},
};
use ratatui::style::Modifier;
use std::time::{Duration, Instant};

const CHECK_TEXT: &str =
    "Café 🦊 e\u{301}. “I have a apple,” she said.\n\nA footnote.[^x]\n\n[^x]: An dog waits.\n";
fn check_desk(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    metric_clock(&mut d);
    d.app.editor.set_text(CHECK_TEXT);
    d.app.flush();
    d.app.save();
    let personal = d.root.join(".test-personal");
    fs::create_dir_all(&personal).unwrap();
    d.app.checks.use_personal_root(&personal);
    d
}
fn contract_report(d: &Desk) -> Report {
    let i = d.app.open.unwrap();
    let doc = d.app.project.nodes[i].path.strip_prefix(&d.root).unwrap();
    let snapshot = Snapshot::new(
        doc,
        SourceUnit::EditorBody,
        SourceRole::Manuscript,
        7,
        d.app.editor.text(),
    )
    .unwrap();
    let options = d.app.checks.options();
    let raw = snapshot.text();
    let start = raw.find("a apple").unwrap();
    let finding = Diagnostic::new(
        &snapshot.project(&options),
        &options,
        RuleKey::harper("AnA"),
        Category::Grammar,
        (start..start + 1).into(),
        "Incorrect indefinite article.".into(),
        vec!["an".into()],
    )
    .unwrap();
    let mut report = Report::empty(snapshot, options, ReportStatus::Complete);
    report.findings.push(finding);
    report
}
fn install_contract(d: &mut Desk) {
    let report = contract_report(d);
    d.app.install_check_report_for_test(report);
    d.app.overlay = Overlay::Checks {
        page: app::checks::Page::Findings,
    };
    d.draw();
}
fn wait_real_check(d: &mut Desk) {
    let limit = Instant::now() + Duration::from_secs(30);
    loop {
        d.app.tick_checks();
        d.draw();
        if d.shows("current suggestions") {
            return;
        }
        assert!(
            Instant::now() < limit,
            "actual worker did not complete:\n{}",
            d.rows().join("\n")
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn checks_open_and_pasted_commands_cannot_consent_or_change_an_original() {
    let mut d = check_desk("check-open", 80, 24);
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let bytes = fs::read(&path).unwrap();
    d.app.run_action(palette::Action::WritingChecks);
    d.draw();
    assert!(d.shows("Writing checks"));
    assert!(d.app.checks.worker.is_none());
    assert!(!d.app.checks.live());
    assert!(!d.root.join(NOVEL_FILE).exists());
    d.key(KeyCode::Char('r'));
    assert_eq!(
        d.app.overlay,
        Overlay::Checks {
            page: app::checks::Page::ConsentOnce
        }
    );
    d.app.paste("ylyarana Commands must not run");
    d.ctrl('v');
    assert!(d.app.checks.worker.is_none());
    assert!(!d.root.join(NOVEL_FILE).exists());
    let before = d.app.editor.selection();
    d.app.on_drag(20, 10);
    assert_eq!(d.app.editor.selection(), before);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.editor.text(), CHECK_TEXT);
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn actual_worker_permission_current_unicode_highlight_apply_and_undo_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = check_desk(&format!("check-real-{w}"), w, h);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let before = fs::read(&path).unwrap();
        d.app.open_checks();
        d.key(KeyCode::Char('r'));
        assert!(d.app.checks.worker.is_none());
        d.key(KeyCode::Char('y'));
        wait_real_check(&mut d);
        assert!(
            !d.root.join(NOVEL_FILE).exists(),
            "once must not persist model permission or source"
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
        assert!(!d.app.checks.live());
        let finding = d.app.selected_check().unwrap();
        assert_eq!(finding.expected, "a");
        assert_eq!(finding.rule.rule, "AnA");
        let expected_col = CHECK_TEXT[..CHECK_TEXT.find("a apple").unwrap()]
            .chars()
            .count();
        assert_eq!(
            d.app.check_line_spans().get(&0),
            Some(&vec![(expected_col, expected_col + 1)])
        );
        d.key(KeyCode::Esc);
        d.draw();
        assert!(
            d.term
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|c| c.modifier.contains(Modifier::UNDERLINED)
                    && c.underline_color == d.app.theme.moon)
        );
        d.app.open_checks();
        d.key(KeyCode::Char('a'));
        assert_eq!(d.app.overlay, Overlay::None);
        assert!(d.app.editor.text().contains("an apple"));
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "review changes the original buffer, not a separate save path"
        );
        assert!(d.app.check_line_spans().is_empty());
        d.ctrl('z');
        assert_eq!(d.app.editor.text(), CHECK_TEXT);
        d.app.save();
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
#[test]
fn changed_source_hides_marks_before_a_tick_and_cannot_apply_a_stale_diagnostic() {
    let mut d = check_desk("check-stale", 100, 30);
    install_contract(&mut d);
    let finding = d.app.selected_check().unwrap();
    let before_disk = fs::read(&d.app.project.nodes[d.app.open.unwrap()].path).unwrap();
    d.key(KeyCode::Esc);
    d.app.editor.scene_start();
    d.app.editor.insert('🦉');
    let before = d.app.editor.text();
    let selection = d.app.editor.selection();
    assert!(d.app.check_line_spans().is_empty());
    assert!(d.app.apply_check_finding(&finding, 0).is_err());
    assert_eq!(d.app.editor.text(), before);
    assert_eq!(d.app.editor.selection(), selection);
    assert_eq!(
        fs::read(&d.app.project.nodes[d.app.open.unwrap()].path).unwrap(),
        before_disk
    );
}
#[test]
fn backing_file_changes_refuse_an_edit_even_when_the_buffer_still_matches() {
    let mut d = check_desk("check-external", 100, 30);
    install_contract(&mut d);
    let finding = d.app.selected_check().unwrap();
    let before = d.app.editor.text();
    let selection = d.app.editor.selection();
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let external = "---\ntitle: External\n---\n\nIndependently edited source.\n";
    fs::write(&path, external).unwrap();
    assert!(
        d.app
            .apply_check_finding(&finding, 0)
            .unwrap_err()
            .to_string()
            .contains("backing source")
    );
    assert_eq!(d.app.editor.text(), before);
    assert_eq!(d.app.editor.selection(), selection);
    assert_eq!(fs::read_to_string(path).unwrap(), external);
}
#[test]
fn source_navigation_and_replacement_have_different_real_postconditions() {
    let mut d = check_desk("check-locate", 100, 30);
    install_contract(&mut d);
    let before = d.app.editor.text();
    d.key(KeyCode::Enter);
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("a"));
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.msg.contains("no suggestion applied"));
    d.app.open_checks();
    d.key(KeyCode::Char('a'));
    assert!(d.app.editor.text().contains("an apple"));
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.checkpoint_productivity(true));
    let replay = d
        .app
        .productivity
        .ledger
        .known
        .iter()
        .filter(|g| g.origin == grimoire_core::measure::Origin::Undo)
        .count();
    assert!(
        replay > 0,
        "native undo remains real origin-tagged operation accounting"
    );
}
#[test]
fn once_novel_and_personal_mutes_are_distinct_and_do_not_store_a_phrase() {
    let mut a = check_desk("check-mutes-a", 100, 30);
    install_contract(&mut a);
    a.key(KeyCode::Char('i'));
    assert!(a.app.check_line_spans().is_empty());
    assert!(!a.root.join(NOVEL_FILE).exists());
    a.key(KeyCode::Char('u'));
    install_contract(&mut a);
    a.key(KeyCode::Char('n'));
    assert!(
        a.app
            .checks
            .options()
            .disabled_rules
            .contains(&RuleKey::harper("AnA"))
    );
    let personal = a.root.join(".test-personal");
    let mut b = check_desk("check-mutes-b", 100, 30);
    b.app.checks.use_personal_root(&personal);
    install_contract(&mut b);
    assert!(
        !b.app
            .checks
            .options()
            .disabled_rules
            .contains(&RuleKey::harper("AnA"))
    );
    b.key(KeyCode::Char('G'));
    b.key(KeyCode::Esc);
    assert!(!personal.join(GLOBAL_FILE).exists());
    b.key(KeyCode::Char('G'));
    b.key(KeyCode::Char('y'));
    let mut c = check_desk("check-mutes-c", 100, 30);
    c.app.checks.use_personal_root(&personal);
    assert!(
        c.app
            .checks
            .options()
            .disabled_rules
            .contains(&RuleKey::harper("AnA"))
    );
    assert!(!c.app.checks.live());
    for path in [a.root.join(NOVEL_FILE), personal.join(GLOBAL_FILE)] {
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("apple") && !text.contains("Café") && !text.contains("expected"));
        assert!(text.contains("AnA") && text.contains(checks_model::HARPER_VERSION));
    }
    c.app.open_checks();
    c.key(KeyCode::Char('s'));
    c.key(KeyCode::Char('X'));
    c.key(KeyCode::Char('y'));
    assert!(
        c.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .muted_rules
            .is_empty()
    );
    assert!(
        a.app
            .checks
            .novel
            .as_ref()
            .unwrap()
            .value
            .options
            .disabled_rules
            .contains(&RuleKey::harper("AnA"))
    );
}
#[test]
fn corrupt_future_and_external_settings_are_visible_and_never_overwritten_by_mute() {
    let mut d = check_desk("check-future", 80, 24);
    install_contract(&mut d);
    let path = d.root.join(NOVEL_FILE);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = br#"{"version":999,"unknown":{"prose":"must survive"}}"#;
    fs::write(&path, future).unwrap();
    d.key(KeyCode::Char('n'));
    assert_eq!(fs::read(&path).unwrap(), future);
    assert!(
        !d.app
            .checks
            .novel
            .as_ref()
            .unwrap()
            .value
            .options
            .disabled_rules
            .contains(&RuleKey::harper("AnA"))
    );
    d.key(KeyCode::Char('u'));
    assert!(d.app.checks.novel.is_none());
    assert!(d.shows("unavailable"));
    d.key(KeyCode::Char('l'));
    d.key(KeyCode::Char('y'));
    assert!(!d.app.checks.live());
    assert!(d.app.checks.worker.is_none());
    assert_eq!(fs::read(path).unwrap(), future);
}
#[test]
fn imported_novel_consent_alone_never_authorizes_a_personal_profile() {
    let mut d = check_desk("check-import-consent", 100, 30);
    d.app
        .checks
        .novel
        .as_mut()
        .unwrap()
        .update(|p| {
            p.live_consent = Some(CONSENT_VERSION.into());
            Ok(())
        })
        .unwrap();
    d.app.open_checks();
    d.app.tick_checks_at(10_000);
    assert!(!d.app.checks.live());
    assert!(d.app.checks.worker.is_none());
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
fn stopping_live_checks_is_immediate_even_when_off_preference_cannot_be_saved() {
    let mut d = check_desk("check-disable-fail", 100, 30);
    d.app.open_checks();
    d.key(KeyCode::Char('l'));
    assert!(d.app.checks.worker.is_none());
    d.key(KeyCode::Char('y'));
    assert!(d.app.checks.live());
    let future = br#"{"version":999,"retained":true}"#;
    fs::write(d.root.join(NOVEL_FILE), future).unwrap();
    d.key(KeyCode::Char('l'));
    assert!(!d.app.checks.live());
    assert!(d.app.checks.worker.is_none());
    assert!(d.app.checks.notice.contains("stopped for session"));
    assert_eq!(fs::read(d.root.join(NOVEL_FILE)).unwrap(), future);
}
#[test]
fn live_jobs_use_injected_idle_debounce_and_low_power_budget_without_typing_credit() {
    for (low_power, delay) in [(false, 900), (true, 2000)] {
        let mut d = check_desk(&format!("check-debounce-{low_power}"), 100, 30);
        d.app
            .editor
            .set_text("```text\nNot prose; no model needs initialization.\n```\n");
        d.app.flush();
        d.app.save();
        d.app.deck.state.low_power = low_power;
        d.app.open_checks();
        d.key(KeyCode::Char('l'));
        d.key(KeyCode::Char('y'));
        d.app.tick_checks_at(0);
        d.app.tick_checks_at(delay - 1);
        assert!(d.app.checks.worker.is_none());
        d.app.tick_checks_at(delay);
        assert_eq!(d.app.checks.worker.as_ref().unwrap().stats().submitted, 1);
        assert!(d.app.checkpoint_productivity(true));
        let operations_before = d.app.productivity.ledger.known.len();
        d.app.tick_checks_at(delay + 10);
        assert!(d.app.checkpoint_productivity(true));
        assert_eq!(d.app.productivity.ledger.known.len(), operations_before);
        d.key(KeyCode::Char('c'));
        assert!(!d.app.checks.live());
        assert!(d.app.checks.worker.is_none());
    }
}
#[test]
fn reviewed_secondary_source_keeps_primary_owner_and_hidden_source_cannot_receive_an_edit() {
    let mut d = check_desk("check-secondary", 160, 50);
    let primary = d.app.open.unwrap();
    let primary_body = d.app.editor.text();
    let second = d
        .app
        .project
        .nodes
        .iter()
        .enumerate()
        .find(|(i, n)| *i != primary && n.kind == project::Kind::Scene && n.in_manuscript)
        .unwrap()
        .1
        .path
        .clone();
    d.app.run_action(palette::Action::EditSecondary(second));
    d.draw();
    assert_eq!(
        d.app.views.active,
        grimoire_core::documents::Pane::Secondary
    );
    d.app.editor.set_text(CHECK_TEXT);
    d.app.flush();
    d.app.save();
    install_contract(&mut d);
    let finding = d.app.selected_check().unwrap();
    d.key(KeyCode::Char('a'));
    assert!(d.app.editor.text().contains("an apple"));
    assert_eq!(
        d.app
            .view_editor(grimoire_core::documents::Pane::Primary)
            .unwrap()
            .text(),
        primary_body
    );
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), CHECK_TEXT);
    d.app.views.secondary_visible = false;
    assert!(d.app.apply_check_finding(&finding, 0).is_err());
    assert_eq!(d.app.editor.text(), CHECK_TEXT);
}
#[test]
fn settings_and_permission_pages_scroll_and_all_actions_have_offline_help() {
    let mut d = check_desk("check-settings", 80, 24);
    d.app.open_checks();
    assert_eq!(d.app.help_context(), "writing-checks");
    d.key(KeyCode::Char('s'));
    let before = d.app.checks.options();
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char('v'));
    d.key(KeyCode::Char('p'));
    let after = d.app.checks.options();
    assert_ne!(before, after);
    assert!(!d.app.checks.live());
    assert!(d.app.checks.worker.is_none());
    d.key(KeyCode::PageDown);
    assert!(d.app.checks.scroll.is_some());
    d.key(KeyCode::Home);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('r'));
    assert_eq!(
        d.app.overlay,
        Overlay::Checks {
            page: app::checks::Page::ConsentOnce
        }
    );
    d.key(KeyCode::F(1));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    assert!(
        help::TOPICS
            .iter()
            .any(|t| t.id == "writing-checks" && t.body.contains("neural chunker"))
    );
}

#[test]
fn copied_live_novel_needs_personal_location_binding_and_revocation_is_explicit() {
    let mut a = check_desk("check-binding-a", 80, 24);
    a.app.open_checks();
    a.key(KeyCode::Char('l'));
    a.key(KeyCode::Char('y'));
    assert!(a.app.checks.live());
    let personal = a.root.join(".test-personal");
    let consented = fs::read(a.root.join(NOVEL_FILE)).unwrap();
    let mut b = check_desk("check-binding-b", 80, 24);
    fs::write(b.root.join(NOVEL_FILE), &consented).unwrap();
    b.app.checks = app::checks::Checks::load(&b.root, Some(personal.clone()));
    assert!(
        b.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .allows_runtime()
    );
    assert!(
        !b.app.checks.live(),
        "another novel's prior profile consent is insufficient"
    );
    b.app.tick_checks_at(4000);
    assert!(b.app.checks.worker.is_none());
    b.app.open_checks();
    b.key(KeyCode::Char('l'));
    b.key(KeyCode::Char('y'));
    assert!(b.app.checks.live());
    assert_eq!(
        b.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .live_projects
            .len(),
        2
    );
    b.key(KeyCode::Char('s'));
    b.key(KeyCode::Char('k'));
    b.key(KeyCode::Esc);
    assert!(
        b.app.checks.live(),
        "canceling revocation has no side effect"
    );
    b.key(KeyCode::Char('s'));
    b.key(KeyCode::Char('k'));
    b.key(KeyCode::Char('y'));
    assert!(!b.app.checks.live());
    assert!(b.app.checks.worker.is_none());
    assert!(
        b.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .live_projects
            .is_empty()
    );
    assert!(
        !b.app
            .checks
            .personal
            .as_ref()
            .unwrap()
            .value
            .allows_runtime()
    );
    assert_eq!(fs::read(a.root.join(NOVEL_FILE)).unwrap(), consented);
    a.app.checks = app::checks::Checks::load(&a.root, Some(personal));
    assert!(!a.app.checks.live());
}
#[test]
fn genuine_demo_grammar_lesson_needs_actual_worker_and_current_original_replacement() {
    use grimoire_core::tutorial;
    let root = std::env::temp_dir().join(format!("wd-check-lesson-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root.clone(), 100, 30);
    metric_clock(&mut d);
    let scene = d
        .app
        .project
        .nodes
        .iter()
        .find(|n| n.kind == project::Kind::Scene && n.title == "The Brass Key")
        .unwrap()
        .path
        .clone();
    d.app.run_action(palette::Action::Open(scene));
    d.app.editor.set_text(CHECK_TEXT);
    d.app.flush();
    d.app.save();
    let personal = root.join(".test-personal");
    fs::create_dir_all(&personal).unwrap();
    d.app.checks.use_personal_root(&personal);
    d.app.deck.progress.completed.insert(26);
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 40 });
    let before = d.app.editor.text();
    d.key(KeyCode::Enter);
    assert_eq!(d.app.editor.text(), before);
    assert_eq!(d.app.deck.progress.active, Some(40));
    assert!(!d.app.deck.progress.completed.contains(&40));
    d.app.open_checks();
    d.key(KeyCode::Char('r'));
    d.key(KeyCode::Char('y'));
    wait_real_check(&mut d);
    assert!(!d.app.deck.progress.completed.contains(&40));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.editor.text(), before);
    assert!(!d.app.deck.progress.completed.contains(&40));
    d.app.open_checks();
    d.key(KeyCode::Char('a'));
    assert!(d.app.editor.text().contains("an apple"));
    assert!(d.app.deck.progress.completed.contains(&40));
    assert!(d.app.deck.progress.completed.contains(&26));
    assert!(
        tutorial::Store::load(&root)
            .unwrap()
            .progress
            .completed
            .contains(&40)
    );
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), before);
    let mut ordinary = check_desk("check-lesson-not-demo", 80, 24);
    ordinary.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 40 });
    ordinary.key(KeyCode::Enter);
    assert_ne!(ordinary.app.deck.progress.active, Some(40));
    assert_eq!(ordinary.app.editor.text(), CHECK_TEXT);
}
#[test]
fn empty_stale_slots_do_not_bypass_once_worker_idle_cleanup() {
    let mut d = check_desk("check-idle-empty", 80, 24);
    d.app.checks.worker = Some(checks_model::worker::Worker::start().unwrap());
    d.app.overlay = Overlay::None;
    d.app.tick_checks_at(30_001);
    assert!(d.app.checks.worker.is_none());
}
