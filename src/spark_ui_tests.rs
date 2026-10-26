use super::*;
use grimoire_core::spark::{self, AssignmentStatus, ExportKind, Mode as SparkMode};
fn spark_desk(tag: &str, w: u16, h: u16) -> (Desk, MetricClock) {
    let mut d = metric_writer(tag, w, h);
    let clock = metric_clock(&mut d);
    d.app.spark.engine = Some(spark::Engine::load(&d.root, 7, spark::catalog().unwrap()).unwrap());
    d.app.run_action(palette::Action::StorySpark);
    d.draw();
    (d, clock)
}
fn spark_current(d: &Desk) -> spark::ResultRef {
    d.app
        .spark
        .engine
        .as_ref()
        .unwrap()
        .state()
        .current
        .clone()
        .unwrap()
}
fn spark_state(d: &Desk) -> &spark::State {
    d.app.spark.engine.as_ref().unwrap().state()
}
fn spark_action(d: &mut Desk, index: usize) {
    d.app.open_spark();
    d.key(KeyCode::Char('a'));
    for _ in 0..index {
        d.key(KeyCode::Down);
    }
    d.key(KeyCode::Enter);
}
fn original_sources(d: &Desk) -> Vec<(PathBuf, Vec<u8>)> {
    d.app
        .project
        .nodes
        .iter()
        .filter(|n| n.kind == project::Kind::Scene && n.in_manuscript)
        .map(|n| (n.path.clone(), fs::read(&n.path).unwrap()))
        .collect()
}
fn assert_originals(sources: &[(PathBuf, Vec<u8>)]) {
    for (path, bytes) in sources {
        assert_eq!(
            &fs::read(path).unwrap(),
            bytes,
            "{} changed",
            path.display()
        );
    }
}
fn demo_spark(tag: &str) -> Desk {
    let root = std::env::temp_dir().join(format!("spark-demo-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 100, 30);
    metric_clock(&mut d);
    d.app.open_spark();
    d
}
#[test]
fn opening_spark_is_read_only_and_does_not_generate_or_change_manuscript() {
    let (mut d, _) = spark_desk("spark-open", 80, 24);
    let before = original_sources(&d);
    assert!(spark_state(&d).current.is_none());
    assert!(
        !d.root
            .join(".grimoire/writingdeck/spark/state.json")
            .exists()
    );
    assert!(d.shows("Story Spark"));
    assert!(d.shows("1047 original plots / 84 exercises"));
    d.key(KeyCode::PageDown);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    assert_originals(&before);
}
#[test]
fn real_keys_generate_all_four_modes_and_independent_joint_rerolls() {
    let (mut d, _) = spark_desk("spark-modes", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('1'));
    let p = spark_current(&d);
    assert_eq!(p.mode, SparkMode::Plot);
    assert!(p.exercise.is_none());
    d.key(KeyCode::Char('2'));
    let e = spark_current(&d);
    assert_eq!(e.mode, SparkMode::Exercise);
    assert!(e.plot.is_none());
    d.key(KeyCode::Char('3'));
    let c = spark_current(&d);
    d.key(KeyCode::Char('p'));
    let p = spark_current(&d);
    assert_ne!(p.plot, c.plot);
    assert_eq!(p.exercise, c.exercise);
    d.key(KeyCode::Char('e'));
    let e = spark_current(&d);
    assert_eq!(e.plot, p.plot);
    assert_ne!(e.exercise, p.exercise);
    d.key(KeyCode::Char('r'));
    let r = spark_current(&d);
    assert_ne!(r.plot, e.plot);
    assert_ne!(r.exercise, e.exercise);
    d.key(KeyCode::Char('4'));
    let daily = spark_current(&d);
    assert_eq!(daily.mode, SparkMode::Daily);
    assert_eq!(daily.zone.as_deref(), Some("UTC"));
    assert_originals(&before);
    let (p, e) = spark::catalog().unwrap().resolve(&r).unwrap();
    assert!(spark::compatible(p.unwrap(), e.unwrap()));
}
#[test]
fn static_combined_view_scrolls_and_preserves_sources_at_four_terminal_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, _) = spark_desk(&format!("spark-size-{w}"), w, h);
        let before = original_sources(&d);
        d.key(KeyCode::Char('3'));
        assert!(d.shows("PLOT [wd-p-"));
        assert!(d.shows("EXERCISE [wd-x-"));
        let r = spark_current(&d);
        let bytes = fs::read(d.root.join(".grimoire/writingdeck/spark/state.json")).unwrap();
        for _ in 0..8 {
            d.key(KeyCode::PageDown);
        }
        for _ in 0..8 {
            d.key(KeyCode::PageUp);
        }
        d.app.frame += 100;
        d.draw();
        assert_eq!(spark_current(&d), r);
        assert_eq!(
            fs::read(d.root.join(".grimoire/writingdeck/spark/state.json")).unwrap(),
            bytes
        );
        assert_originals(&before);
        assert_eq!(d.rows().len(), h as usize);
        assert!(d.rows()[0].contains('┐'));
        assert!(d.rows()[h as usize - 1].contains('┘'));
    }
}
#[test]
fn draft_filter_cancel_is_inert_save_is_real_and_empty_pool_keeps_the_old_result() {
    let (mut d, _) = spark_desk("spark-filters", 100, 30);
    d.key(KeyCode::Char('3'));
    let old = spark_current(&d);
    let default = spark_state(&d).filter.clone();
    d.key(KeyCode::Char('o'));
    d.key(KeyCode::Right);
    d.key(KeyCode::Esc);
    assert_eq!(spark_state(&d).filter, default);
    d.key(KeyCode::Char('o'));
    for _ in 0..3 {
        d.key(KeyCode::Down);
    }
    for _ in 0..3 {
        d.key(KeyCode::Left);
    }
    d.key(KeyCode::Enter);
    assert_eq!(spark_state(&d).filter.max_minutes, 10);
    d.key(KeyCode::Char('3'));
    assert_eq!(spark_current(&d), old);
    assert!(d.app.msg.contains("No compatible"));
}
#[test]
fn favorite_history_recall_and_dismissal_are_persisted_references_only() {
    let (mut d, _) = spark_desk("spark-history", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('3'));
    let first = spark_current(&d);
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char('x'));
    d.key(KeyCode::Char('3'));
    assert_ne!(spark_current(&d).key(), first.key());
    d.key(KeyCode::Char('F'));
    d.key(KeyCode::Enter);
    assert_eq!(spark_current(&d), first);
    assert!(d.shows("Favorited"));
    let restarted = Desk::open(d.root.clone(), 100, 30);
    assert_eq!(spark_state(&restarted).current, Some(first.clone()));
    assert_eq!(spark_state(&restarted).favorites[&first.id], first);
    let raw = fs::read_to_string(d.root.join(".grimoire/writingdeck/spark/state.json")).unwrap();
    assert!(!raw.contains("\"text\""));
    assert!(!raw.contains("\"instructions\""));
    assert_originals(&before);
}
#[test]
fn daily_is_frozen_across_real_controller_reopen_date_rollover_and_reroll_branch() {
    let (mut d, clock) = spark_desk("spark-daily", 100, 30);
    d.key(KeyCode::Char('4'));
    let first = spark_current(&d);
    let path = d.root.join(spark::daily_path("2026-10-25").unwrap());
    let bytes = fs::read(&path).unwrap();
    d.key(KeyCode::Char('p'));
    assert_eq!(spark_current(&d).mode, SparkMode::Combined);
    d.key(KeyCode::Char('4'));
    assert_eq!(spark_current(&d), first);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    metric_advance(&clock, 86_400_000);
    d.key(KeyCode::Char('4'));
    assert_ne!(spark_current(&d).id, first.id);
    assert_eq!(spark_current(&d).date, "2026-10-26");
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn save_creates_one_markdown_original_and_retry_preserves_later_writer_edits() {
    let (mut d, _) = spark_desk("spark-save", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('1'));
    d.key(KeyCode::Char('s'));
    let r = spark_current(&d);
    let target = spark::export_path(&r.id, ExportKind::Save, None).unwrap();
    let full = d.root.join(&target);
    assert!(full.is_file());
    let mut text = fs::read_to_string(&full).unwrap();
    assert!(text.contains("license MIT"));
    text.push_str("\nWriter's own interpretation.\n");
    fs::write(&full, &text).unwrap();
    d.key(KeyCode::Char('s'));
    assert_eq!(fs::read_to_string(&full).unwrap(), text);
    assert_eq!(spark_state(&d).exports.len(), 1);
    assert_originals(&before);
}
#[test]
fn pin_action_reuses_the_saved_source_without_copying_or_duplicate_cards() {
    let (mut d, _) = spark_desk("spark-pin", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('3'));
    spark_action(&mut d, 3);
    assert_eq!(d.app.pins.state().pins.len(), 1);
    let source = d.app.pins.state().pins[0].source.clone();
    assert_eq!(
        source,
        spark::export_path(&spark_current(&d).id, ExportKind::Save, None).unwrap()
    );
    spark_action(&mut d, 3);
    assert_eq!(d.app.pins.state().pins.len(), 1);
    assert_eq!(spark_state(&d).exports.len(), 1);
    assert_originals(&before);
}
#[test]
fn scratch_action_registers_a_real_original_and_opens_its_primary_editor() {
    let (mut d, _) = spark_desk("spark-scratch", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 4);
    let id = d.app.workflow.selected.unwrap();
    let note = d
        .app
        .workflow
        .store
        .as_ref()
        .unwrap()
        .state
        .note(id)
        .unwrap();
    let path = d.root.join(&note.source);
    assert!(path.is_file());
    assert_eq!(d.app.project.nodes[d.app.open.unwrap()].path, path);
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(!d.app.project.nodes[d.app.open.unwrap()].in_manuscript);
    assert!(fs::read_to_string(&path).unwrap().contains(&r.id));
    d.typed("A separately authored scratch observation. ");
    d.ctrl('s');
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("separately authored")
    );
    assert_originals(&before);
}
#[test]
fn new_experiment_is_empty_compile_excluded_and_independent_of_the_original() {
    let (mut d, _) = spark_desk("spark-scene", 100, 30);
    let before = original_sources(&d);
    let words = d.app.project.total_words();
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 7);
    let node = &d.app.project.nodes[d.app.open.unwrap()];
    assert!(node.in_manuscript);
    assert!(!node.compile);
    assert!(node.body.trim().is_empty());
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(d.app.project.total_words(), words);
    assert!(
        node.path
            .ends_with(format!("98-Spark-{}-experiment.md", r.id))
    );
    assert_originals(&before);
    let note = d
        .root
        .join(spark::export_path(&r.id, ExportKind::Save, None).unwrap());
    assert!(note.is_file());
    d.typed("Independent experimental prose. ");
    d.ctrl('s');
    assert_originals(&before);
    assert!(
        !fs::read_to_string(note)
            .unwrap()
            .contains("Independent experimental prose")
    );
}
#[test]
fn outline_and_task_actions_create_real_markdown_not_fake_story_database_entries() {
    let (mut d, _) = spark_desk("spark-notes", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 5);
    spark_action(&mut d, 6);
    let outline = fs::read_to_string(
        d.root
            .join(spark::export_path(&r.id, ExportKind::Outline, None).unwrap()),
    )
    .unwrap();
    let task = fs::read_to_string(
        d.root
            .join(spark::export_path(&r.id, ExportKind::Task, None).unwrap()),
    )
    .unwrap();
    assert!(outline.contains("# Outline experiment"));
    assert!(task.contains("- [ ] Explore"));
    assert!(outline.contains(&r.id) && task.contains(&r.id));
    assert_originals(&before);
}
#[test]
fn optional_spark_assignment_is_separately_accepted_and_self_reported_not_measured() {
    let (mut d, _) = spark_desk("spark-optional", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 9);
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Assigned
    );
    d.key(KeyCode::Char('x'));
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Assigned
    );
    d.key(KeyCode::Char('a'));
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Accepted
    );
    d.key(KeyCode::Char('x'));
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Completed
    );
    assert!(d.app.msg.contains("SELF-REPORTED"));
    assert_eq!(d.app.spark.assignment.as_ref().unwrap().result, r);
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('S'));
    assert!(matches!(
        d.app.overlay,
        Overlay::Spark {
            page: app::spark::Page::Assignment,
            ..
        }
    ));
    assert_originals(&before);
}
#[test]
fn sprint_action_starts_real_monotonic_timer_with_reference_but_never_completes_creative_task() {
    let (mut d, clock) = spark_desk("spark-sprint", 100, 30);
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 9);
    d.key(KeyCode::Esc);
    spark_action(&mut d, 8);
    let sprint = d.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(sprint.kind, grimoire_core::productivity::SprintKind::Timed);
    assert_eq!(sprint.spark, Some(r.clone()));
    let minutes = sprint.target;
    assert!(minutes <= 30);
    assert!(d.shows("Spark source"));
    metric_advance(&clock, minutes * 60_000 + 1);
    d.app.tick_productivity();
    assert_eq!(
        d.app.productivity.sprint.as_ref().unwrap().status,
        grimoire_core::productivity::SprintStatus::Completed
    );
    assert_eq!(
        spark::assignment(&d.root, "2026-10-25")
            .unwrap()
            .unwrap()
            .value
            .status,
        AssignmentStatus::Assigned
    );
    let serialized =
        fs::read_to_string(d.root.join(".grimoire/writingdeck/productivity.json")).unwrap();
    assert!(serialized.contains(&r.id));
    assert!(
        !serialized.contains(
            &spark::catalog()
                .unwrap()
                .resolve(&r)
                .unwrap()
                .0
                .unwrap()
                .text
        )
    );
}
#[test]
fn active_sprint_metadata_failure_preserves_external_bytes_and_blocks_normal_quit() {
    let (mut d, clock) = spark_desk("spark-sprint-failure", 100, 30);
    d.key(KeyCode::Char('3'));
    spark_action(&mut d, 8);
    let id = d.app.productivity.sprint.as_ref().unwrap().id;
    let file = d.root.join(".grimoire/writingdeck/productivity.json");
    let external = b"{\"version\":9,\"future\":true}";
    fs::write(&file, external).unwrap();
    metric_advance(&clock, 60_000);
    d.app.tick_productivity();
    d.ctrl('q');
    assert!(!d.left);
    assert!(
        d.shows("Q again to quit anyway"),
        "A refused normal quit must be visible above the modal, not only in an internal string"
    );
    assert_eq!(fs::read(&file).unwrap(), external);
    assert_eq!(d.app.productivity.sprint.as_ref().unwrap().id, id);
    assert!(
        d.app.msg.contains("not")
            || d.app.msg.contains("preserv")
            || d.app.msg.contains("checkpoint")
    );
}
#[test]
fn active_sprint_normal_quit_records_elapsed_and_restart_needs_explicit_resume() {
    let (mut d, clock) = spark_desk("spark-sprint-quit", 100, 30);
    d.key(KeyCode::Char('3'));
    spark_action(&mut d, 8);
    metric_advance(&clock, 40_000);
    d.ctrl('q');
    assert!(d.left, "{}", d.app.msg);
    let settings: grimoire_core::productivity::Settings = serde_json::from_slice(
        &fs::read(d.root.join(".grimoire/writingdeck/productivity.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings.sprint.as_ref().unwrap().elapsed_ms, 40_000);
    assert!(settings.sprint.as_ref().unwrap().spark.is_some());
    let mut restarted = Desk::open(d.root.clone(), 100, 30);
    let later = metric_clock(&mut restarted);
    metric_advance(&later, 86_400_000);
    restarted.app.tick_productivity();
    let sprint = restarted.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(sprint.elapsed_ms, 40_000);
    assert!(sprint.anchor_ms.is_none());
}
#[test]
fn future_spark_metadata_is_reported_and_generation_cannot_reset_favorites() {
    let root = book("spark-future", true);
    let file = root.join(".grimoire/writingdeck/spark/state.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = b"{\"version\":2,\"favorites\":[\"keep-me\"]}";
    fs::write(&file, bytes).unwrap();
    let mut d = Desk::open(root, 100, 30);
    d.app.open_spark();
    d.key(KeyCode::Char('3'));
    d.key(KeyCode::Char('u'));
    assert!(d.app.spark.engine.is_none());
    assert_eq!(fs::read(file).unwrap(), bytes);
    assert!(d.app.spark.rows.iter().any(|r| r.contains("preserved")));
}
#[test]
fn failed_source_export_has_one_prepared_receipt_and_real_retry_keeps_original_scene() {
    let (mut d, _) = spark_desk("spark-export-fail", 100, 30);
    let before = original_sources(&d);
    d.key(KeyCode::Char('1'));
    fs::create_dir_all(d.root.join("notes")).unwrap();
    fs::write(d.root.join("notes/sparks"), "blocker").unwrap();
    d.key(KeyCode::Char('s'));
    assert_eq!(spark_state(&d).exports.len(), 1);
    assert_eq!(spark_state(&d).exports[0].phase, spark::Phase::Prepared);
    assert!(d.app.msg.contains("not completed"));
    fs::remove_file(d.root.join("notes/sparks")).unwrap();
    d.key(KeyCode::Char('s'));
    assert_eq!(spark_state(&d).exports.len(), 1);
    assert_eq!(spark_state(&d).exports[0].phase, spark::Phase::Ready);
    assert_originals(&before);
}
#[test]
fn failed_pin_publication_never_claims_pin_success_or_discards_the_saved_note() {
    let (mut d, _) = spark_desk("spark-pin-fail", 100, 30);
    d.key(KeyCode::Char('3'));
    let file = d.root.join(".grimoire/writingdeck/pins.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = b"{\"version\":99}";
    fs::write(&file, bytes).unwrap();
    spark_action(&mut d, 3);
    assert!(d.app.pins.state().pins.is_empty());
    assert!(d.app.msg.contains("not completed"));
    assert!(!d.app.msg.contains("reference pinned"));
    assert_eq!(fs::read(file).unwrap(), bytes);
    assert!(
        d.root
            .join(spark::export_path(&spark_current(&d).id, ExportKind::Save, None).unwrap())
            .exists()
    );
}
#[test]
fn modal_controls_drag_click_cut_and_undo_cannot_mutate_hidden_original() {
    let (mut d, _) = spark_desk("spark-routing", 100, 30);
    d.key(KeyCode::Esc);
    d.ctrl('a');
    let original = d.app.editor.text();
    let anchor = d.app.editor.selection_anchor();
    d.app.open_spark();
    d.key(KeyCode::Char('3'));
    d.ctrl('x');
    d.ctrl('z');
    d.ctrl('y');
    d.ctrl('a');
    let r = d.app.rect_editor;
    d.app.on_click(r.x + 2, r.y + 2);
    d.app.on_drag(r.x + 4, r.y + 4);
    assert_eq!(d.app.editor.text(), original);
    assert_eq!(d.app.editor.selection_anchor(), anchor);
    assert!(matches!(d.app.overlay, Overlay::Spark { .. }));
}
#[test]
fn roulette_is_presentation_only_and_focus_low_power_remain_static() {
    let (mut d, _) = spark_desk("spark-roulette", 100, 30);
    d.key(KeyCode::Char('l'));
    assert!(spark_state(&d).roulette);
    d.key(KeyCode::Char('3'));
    assert!(d.app.spark.animating());
    let result = spark_current(&d);
    let bytes = fs::read(d.root.join(".grimoire/writingdeck/spark/state.json")).unwrap();
    d.app.deck.state.low_power = true;
    d.app.frame += 1;
    d.draw();
    let static_rows = d.rows();
    d.app.frame += 1;
    d.draw();
    assert_eq!(d.rows(), static_rows);
    assert_eq!(spark_current(&d), result);
    assert_eq!(
        fs::read(d.root.join(".grimoire/writingdeck/spark/state.json")).unwrap(),
        bytes
    );
    d.app.focus_mode = true;
    d.app.deck.state.low_power = false;
    d.app.frame += 1;
    d.draw();
    let rows = d.rows();
    d.app.frame += 1;
    d.draw();
    assert_eq!(d.rows(), rows);
}
#[test]
fn provenance_and_context_help_are_real_scrollable_offline_content() {
    let (mut d, _) = spark_desk("spark-provenance", 80, 24);
    d.key(KeyCode::Char('3'));
    d.key(KeyCode::Char('v'));
    assert!(d.shows("Spark provenance"));
    for _ in 0..5 {
        d.key(KeyCode::PageDown);
    }
    assert!(d.app.spark.render_fit.get().0 > 0);
    d.key(KeyCode::F(1));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    assert_eq!(help::TOPICS[help::index("story-spark")].id, "story-spark");
}
#[test]
fn tutorial_22_requires_real_plot_save_and_23_requires_exercise_favorite() {
    let mut d = demo_spark("lessons22-23");
    d.app.deck.progress.start(22).unwrap();
    d.key(KeyCode::Char('1'));
    assert!(!d.app.deck.progress.completed.contains(&22));
    d.key(KeyCode::Char('s'));
    assert!(d.app.deck.progress.completed.contains(&22), "{}", d.app.msg);
    d.app.deck.progress.start(23).unwrap();
    d.key(KeyCode::Char('2'));
    assert!(!d.app.deck.progress.completed.contains(&23));
    d.key(KeyCode::Char('f'));
    assert!(d.app.deck.progress.completed.contains(&23));
}
#[test]
fn tutorial_24_requires_persisted_independent_joint_rerolls_favorite_and_new_scene() {
    let mut d = demo_spark("lesson24");
    let before = original_sources(&d);
    d.app.deck.progress.start(24).unwrap();
    d.key(KeyCode::Char('3'));
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('e'));
    d.key(KeyCode::Char('r'));
    assert!(!d.app.deck.progress.completed.contains(&24));
    spark_action(&mut d, 7);
    assert!(d.app.deck.progress.completed.contains(&24), "{}", d.app.msg);
    assert_originals(&before);
}
#[test]
fn tutorial_actions_on_real_novel_cannot_complete_demo_exercises() {
    let (mut d, _) = spark_desk("spark-not-demo", 100, 30);
    d.app.deck.progress.start(22).unwrap();
    d.key(KeyCode::Char('1'));
    d.key(KeyCode::Char('s'));
    assert!(!d.app.deck.progress.completed.contains(&22));
}
#[test]
fn render_is_cached_even_when_metadata_is_removed_and_alias_widget_are_additive() {
    let (mut d, _) = spark_desk("spark-cached", 100, 30);
    d.key(KeyCode::Char('3'));
    let file = d.root.join(".grimoire/writingdeck/spark/state.json");
    fs::remove_file(&file).unwrap();
    d.draw();
    d.draw();
    assert!(!file.exists());
    assert!(
        d.app
            .deck
            .state
            .dashboard
            .iter()
            .any(|w| w.widget == grimoire_core::workspaces::Widget::StorySpark)
    );
    d.app
        .deck_shortcut(writingdeck_sys::shortcuts::Action::StorySpark);
    assert!(matches!(d.app.overlay, Overlay::Spark { .. }));
    assert_eq!(
        writingdeck_sys::shortcuts::Shortcuts::default()
            .bindings
            .len(),
        3
    );
}

#[test]
fn explicit_reload_of_missing_or_corrupt_loaded_metadata_keeps_recoverable_cached_references() {
    let (mut d, _) = spark_desk("spark-reload-missing", 100, 30);
    d.key(KeyCode::Char('3'));
    d.key(KeyCode::Char('f'));
    let before = spark_state(&d).clone();
    let file = d.root.join(".grimoire/writingdeck/spark/state.json");
    fs::remove_file(&file).unwrap();
    d.key(KeyCode::Char('u'));
    assert_eq!(spark_state(&d), &before);
    assert!(!file.exists());
    let bytes = b"{\"version\":8,\"opaque\":\"keep\"}";
    fs::write(&file, bytes).unwrap();
    d.key(KeyCode::Char('u'));
    assert_eq!(spark_state(&d), &before);
    assert_eq!(fs::read(&file).unwrap(), bytes);
    d.key(KeyCode::Char('3'));
    assert_eq!(spark_state(&d), &before);
    assert_eq!(fs::read(file).unwrap(), bytes);
}

#[test]
fn recalling_an_already_saved_result_is_not_a_new_tutorial_save() {
    let mut d = demo_spark("not-noop-save");
    d.key(KeyCode::Char('1'));
    d.key(KeyCode::Char('s'));
    d.app.deck.progress.start(22).unwrap();
    d.key(KeyCode::Char('s'));
    assert!(!d.app.deck.progress.completed.contains(&22));
    d.key(KeyCode::Char('1'));
    d.key(KeyCode::Char('s'));
    assert!(d.app.deck.progress.completed.contains(&22));
}

#[test]
fn midnight_cannot_accept_a_different_assignment_than_the_one_displayed() {
    let (mut d, clock) = spark_desk("spark-midnight-accept", 100, 30);
    d.key(KeyCode::Char('3'));
    let r = spark_current(&d);
    spark_action(&mut d, 9);
    metric_advance(&clock, 86_400_000);
    let next = clock.0.lock().unwrap().clone();
    spark::assign(&d.root, &r, &next).unwrap();
    d.key(KeyCode::Char('a'));
    assert_eq!(d.app.spark.assignment_date, "2026-10-26");
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Assigned
    );
    assert!(d.app.msg.contains("Local civil date changed"));
    d.key(KeyCode::Char('a'));
    assert_eq!(
        d.app.spark.assignment.as_ref().unwrap().status,
        AssignmentStatus::Accepted
    );
    assert_eq!(
        spark::assignment(&d.root, "2026-10-25")
            .unwrap()
            .unwrap()
            .value
            .status,
        AssignmentStatus::Assigned
    );
}
