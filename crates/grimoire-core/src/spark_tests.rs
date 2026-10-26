use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
struct Fixture(PathBuf);
impl Fixture {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "spark-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn engine(&self, seed: u64) -> Engine<'static> {
        Engine::load(&self.0, seed, catalog().unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn now() -> Moment {
    at("2026-10-25T12:00:00-07:00", "America/Los_Angeles")
}
fn at(wall: &str, zone: &str) -> Moment {
    Moment {
        mono_ms: 10,
        wall: chrono::DateTime::parse_from_rfc3339(wall).unwrap(),
        zone: Some(zone.into()),
    }
}
#[test]
fn compiled_corpus_has_1047_original_plots_84_independent_exercises_and_exact_provenance() {
    let c = catalog().unwrap();
    assert_eq!(c.plot_count, 1047);
    assert_eq!(c.exercise_count, 84);
    let mut hashes = BTreeSet::new();
    for r in &c.records {
        assert_eq!(r.sha256, sha256(r.text.as_bytes()));
        assert!(hashes.insert(&r.sha256));
        assert_eq!(r.source.license, "MIT");
        assert_eq!(r.source.section, r.id);
        assert_eq!(r.source.edition, c.version);
        assert!(r.source.file.contains("original-"));
    }
    assert_eq!(c.categories(Kind::Exercise).len(), 14);
    for id in ["wd-p-0544", "wd-p-0679", "wd-p-0922", "wd-p-0986"] {
        assert!(c.get(id).is_none());
    }
}
#[test]
fn malformed_corpus_duplicate_hash_unknown_fields_and_wrong_source_are_refused() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../assets/writingdeck/spark-corpus.json")).unwrap();
    let mut cases = vec![];
    let mut v = original.clone();
    v["schema_version"] = 2.into();
    cases.push(v);
    let mut v = original.clone();
    v["unexpected"] = true.into();
    cases.push(v);
    let mut v = original.clone();
    v["records"][0]["sha256"] = "0".repeat(64).into();
    cases.push(v);
    let mut v = original.clone();
    v["records"][0]["source"]["license"] = "unknown".into();
    cases.push(v);
    let mut v = original.clone();
    v["records"][1] = v["records"][0].clone();
    cases.push(v);
    let mut v = original.clone();
    v["records"][0]["source"]["file"] = "../escape".into();
    cases.push(v);
    let mut v = original.clone();
    v["records"][0]["requires"] = serde_json::json!(["single-scene", "multi-scene"]);
    cases.push(v);
    let mut v = original.clone();
    v["records"][0]["requires"] = serde_json::json!(["future-feature"]);
    cases.push(v);
    for case in cases {
        assert!(Corpus::parse(&case.to_string()).is_err());
    }
    assert!(Corpus::parse(&" ".repeat(4_194_305)).is_err());
}
#[test]
fn plot_exercise_combined_are_real_separate_shapes_and_deterministic() {
    let a = Fixture::new("det-a");
    let b = Fixture::new("det-b");
    let mut x = a.engine(7);
    let mut y = b.engine(7);
    for mode in [
        Mode::Plot,
        Mode::Exercise,
        Mode::Combined,
        Mode::Plot,
        Mode::Combined,
    ] {
        let l = x.generate(mode, &now()).unwrap();
        let r = y.generate(mode, &now()).unwrap();
        assert_eq!(l, r);
        assert_eq!(l.plot.is_some(), mode != Mode::Exercise);
        assert_eq!(l.exercise.is_some(), mode != Mode::Plot);
        if let (Some(p), Some(e)) = catalog().unwrap().resolve(&l).unwrap() {
            assert!(compatible(p, e));
        }
    }
    assert_eq!(x.state(), y.state());
}
#[test]
fn splitmix_fixture_and_reference_replay_do_not_depend_on_random_device_or_clock() {
    let mut counter = 0;
    assert_eq!(draw(0, &mut counter, 1000).unwrap(), 535);
    assert_eq!(counter, 1);
    let d = Fixture::new("fixture");
    let mut e = d.engine(0);
    let r = e.generate(Mode::Plot, &now()).unwrap();
    assert_eq!(r.counter_start, 0);
    assert_eq!(r.counter_end, 1);
    assert_eq!(r.id, "r-0000000000000001");
    let loaded = d.engine(9999);
    assert_eq!(loaded.state().seed, 0);
    assert_eq!(loaded.state().current, Some(r.clone()));
    assert_eq!(
        loaded.bank.resolve(&r).unwrap().0.unwrap().id,
        r.plot.unwrap().id
    );
}
#[test]
fn recent_components_are_avoided_until_filtered_pool_is_exhausted() {
    let d = Fixture::new("recent");
    let mut e = d.engine(17);
    let mut seen = BTreeSet::new();
    for _ in 0..12 {
        let r = e.generate(Mode::Combined, &now()).unwrap();
        assert!(!r.relaxed_recent);
        assert!(seen.insert(r.plot.unwrap().id));
        assert!(seen.insert(r.exercise.unwrap().id));
    }
    assert_eq!(e.state().recent.len(), 12);
    let mut filter = Filter::default();
    filter.plot_categories.insert("speech".into());
    e.set_filter(filter).unwrap();
    let first = e.generate(Mode::Plot, &now()).unwrap();
    let second = e.generate(Mode::Plot, &now()).unwrap();
    assert_eq!(first.plot, second.plot);
    assert!(second.relaxed_recent);
    let before = e.state().clone();
    assert!(e.reroll(Reroll::Plot, &now()).is_err());
    assert_eq!(e.state(), &before);
}
#[test]
fn independent_rerolls_preserve_exact_component_and_joint_changes_both() {
    let d = Fixture::new("reroll");
    let mut e = d.engine(5);
    let first = e.generate(Mode::Combined, &now()).unwrap();
    let p = e.reroll(Reroll::Plot, &now()).unwrap();
    assert_eq!(p.exercise, first.exercise);
    assert_ne!(p.plot, first.plot);
    assert_eq!(p.derived_from, Some(first.id));
    let x = e.reroll(Reroll::Exercise, &now()).unwrap();
    assert_eq!(x.plot, p.plot);
    assert_ne!(x.exercise, p.exercise);
    let both = e.reroll(Reroll::Both, &now()).unwrap();
    assert_ne!(both.plot, x.plot);
    assert_ne!(both.exercise, x.exercise);
    e.generate(Mode::Exercise, &now()).unwrap();
    assert!(e.reroll(Reroll::Plot, &now()).is_err());
}
#[test]
fn contradictory_forms_never_pair_and_existing_text_exercises_stay_independent() {
    let c = catalog().unwrap();
    let silent = c.get("wd-p-1005").unwrap();
    let dialogue = c.get("wd-x-0001").unwrap();
    let braid = c.get("wd-x-0028").unwrap();
    assert!(!compatible(silent, dialogue));
    assert!(!compatible(silent, braid));
    let description = c.get("wd-x-0007").unwrap();
    assert!(compatible(silent, description));
    let mut other = silent.clone();
    other.requires = BTreeSet::from(["narration-only".into()]);
    other.excludes.clear();
    assert!(!compatible(&other, dialogue));
    for r in c
        .records
        .iter()
        .filter(|r| r.work_mode.is_some_and(|m| m != WorkMode::Drafting))
    {
        assert!(!compatible(silent, r));
    }
    let d = Fixture::new("revision");
    let mut e = d.engine(3);
    let mut f = Filter::default();
    f.exercise_categories.insert("revision".into());
    e.set_filter(f).unwrap();
    assert!(e.generate(Mode::Exercise, &now()).is_ok());
    let old = e.state().clone();
    assert!(e.generate(Mode::Combined, &now()).is_err());
    assert_eq!(e.state(), &old);
}
#[test]
fn genre_category_difficulty_duration_filters_are_hard_not_silently_relaxed() {
    let d = Fixture::new("filters");
    let mut e = d.engine(8);
    let mut f = Filter::default();
    f.genres.insert("historical".into());
    f.max_difficulty = 3;
    e.set_filter(f.clone()).unwrap();
    for _ in 0..6 {
        let r = e.generate(Mode::Plot, &now()).unwrap();
        let p = e.bank.resolve(&r).unwrap().0.unwrap();
        assert!(p.genres.contains("historical"));
        assert_eq!(p.difficulty, 3);
    }
    f.max_difficulty = 1;
    e.set_filter(f.clone()).unwrap();
    let before = e.state().clone();
    assert!(e.generate(Mode::Plot, &now()).is_err());
    assert_eq!(e.state(), &before);
    f.genres.clear();
    f.max_minutes = 5;
    e.set_filter(f).unwrap();
    assert!(e.generate(Mode::Combined, &now()).is_err());
    let mut invalid = Filter::default();
    invalid.plot_categories.insert("made-up-category".into());
    let before = e.state().clone();
    assert!(e.set_filter(invalid).is_err());
    assert_eq!(e.state(), &before);
}
#[test]
fn fixed_component_survives_new_filters_but_new_component_obeys_them() {
    let d = Fixture::new("kept-filter");
    let mut e = d.engine(9);
    let old = e.generate(Mode::Combined, &now()).unwrap();
    let mut f = Filter::default();
    f.exercise_categories.insert("dialogue".into());
    e.set_filter(f).unwrap();
    let r = e.reroll(Reroll::Exercise, &now()).unwrap();
    assert_eq!(r.plot, old.plot);
    assert_eq!(e.bank.resolve(&r).unwrap().1.unwrap().category, "dialogue");
}
#[test]
fn daily_survives_reboot_filter_changes_same_date_travel_and_dst() {
    let d = Fixture::new("daily");
    let mut e = d.engine(45);
    let m = at("2026-11-01T01:30:00-07:00", "America/Los_Angeles");
    let first = e.daily(&m).unwrap();
    let path = d.0.join(daily_path("2026-11-01").unwrap());
    let bytes = fs::read(&path).unwrap();
    let mut f = Filter::default();
    f.genres.insert("historical".into());
    e.set_filter(f).unwrap();
    drop(e);
    let mut e = d.engine(111);
    let later = at("2026-11-01T01:30:00-08:00", "America/Los_Angeles");
    assert_eq!(e.daily(&later).unwrap(), first);
    let travel = at("2026-11-01T20:00:00+09:00", "Asia/Tokyo");
    assert_eq!(e.daily(&travel).unwrap(), first);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let next = e
        .daily(&at("2026-11-02T00:01:00+09:00", "Asia/Tokyo"))
        .unwrap();
    assert_ne!(next.id, first.id);
    assert_eq!(next.zone.as_deref(), Some("Asia/Tokyo"));
}
#[test]
fn daily_rerolls_branch_without_replacing_the_fixed_day() {
    let d = Fixture::new("daily-branch");
    let mut e = d.engine(11);
    let daily = e.daily(&now()).unwrap();
    let path = d.0.join(daily_path(&now().day()).unwrap());
    let bytes = fs::read(&path).unwrap();
    let branch = e.reroll(Reroll::Plot, &now()).unwrap();
    assert_eq!(branch.mode, Mode::Combined);
    assert_eq!(branch.exercise, daily.exercise);
    assert_ne!(branch.plot, daily.plot);
    assert_eq!(branch.derived_from, Some(daily.id.clone()));
    assert_eq!(e.daily(&now()).unwrap(), daily);
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn unknown_timezone_and_mismatched_offset_do_not_invent_daily_dates() {
    let d = Fixture::new("zone");
    let mut e = d.engine(10);
    let mut m = now();
    m.zone = None;
    assert!(e.daily(&m).is_err());
    assert!(e.generate(Mode::Plot, &m).is_ok());
    m.zone = Some("Asia/Tokyo".into());
    assert!(e.daily(&m).is_err());
    assert!(!d.0.join(daily_path(&now().day()).unwrap()).exists());
}
#[test]
fn old_corpus_daily_favorites_and_history_are_preserved_not_reassigned() {
    let d = Fixture::new("upgrade");
    let mut e = d.engine(2);
    let r = e.daily(&now()).unwrap();
    e.favorite().unwrap();
    let path = d.0.join(daily_path(&now().day()).unwrap());
    let bytes = fs::read(&path).unwrap();
    let mut bank = catalog().unwrap().clone();
    bank.version = "writingdeck-original-2".into();
    let mut e = Engine::load(&d.0, 0, &bank).unwrap();
    assert_eq!(e.daily(&now()).unwrap(), r);
    assert!(bank.resolve(&r).is_err());
    assert!(e.state().favorites.contains_key(&r.id));
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert!(e.generate(Mode::Combined, &now()).is_ok());
    assert_eq!(e.state().favorites[&r.id], r);
}
#[test]
fn favorites_survive_history_retention_and_dismissals_block_only_the_exact_pair() {
    let d = Fixture::new("favorite");
    let mut e = d.engine(72);
    let r = e.generate(Mode::Combined, &now()).unwrap();
    assert!(e.favorite().unwrap());
    e.dismiss().unwrap();
    for _ in 0..140 {
        let next = e.generate(Mode::Combined, &now()).unwrap();
        assert_ne!(next.key(), r.key());
    }
    assert_eq!(e.state().history.len(), HISTORY_LIMIT);
    assert!(!e.state().history.iter().any(|p| p.id == r.id));
    assert_eq!(e.state().favorites[&r.id], r);
    drop(e);
    let mut e = d.engine(0);
    e.show(&r.id).unwrap();
    assert_eq!(e.state().current, Some(r));
    assert!(!e.favorite().unwrap());
    e.clear_dismissals().unwrap();
    assert!(e.state().dismissed.is_empty());
}
#[test]
fn exhausted_dismissed_pool_is_an_error_not_an_undismissed_result() {
    let d = Fixture::new("dismissed");
    let mut e = d.engine(0);
    let mut f = Filter::default();
    f.plot_categories.insert("speech".into());
    e.set_filter(f).unwrap();
    e.generate(Mode::Plot, &now()).unwrap();
    e.dismiss().unwrap();
    let before = e.state().clone();
    assert!(e.generate(Mode::Plot, &now()).is_err());
    assert_eq!(e.state(), &before);
}
#[test]
fn corrupt_future_unknown_oversized_and_conflicting_state_is_not_replaced() {
    let d = Fixture::new("bad-state");
    let path = d.0.join(".grimoire/writingdeck/spark/state.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut v = serde_json::to_value(State::new(0)).unwrap();
    v["version"] = 2.into();
    let future = serde_json::to_vec(&v).unwrap();
    v["version"] = 1.into();
    v["manuscript"] = "copied prose is forbidden".into();
    let unknown = serde_json::to_vec(&v).unwrap();
    for bytes in [
        b"not json".to_vec(),
        future,
        unknown,
        vec![b'x'; crate::statefile::MAX_BYTES + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(Engine::load(&d.0, 0, catalog().unwrap()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    let mut e = d.engine(5);
    e.generate(Mode::Plot, &now()).unwrap();
    let before = e.state().clone();
    let external = serde_json::to_vec(&State::new(76)).unwrap();
    fs::write(&path, &external).unwrap();
    assert!(e.generate(Mode::Combined, &now()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(fs::read(path).unwrap(), external);
}
#[test]
fn selection_never_reads_or_modifies_manuscript_and_state_contains_only_references() {
    let d = Fixture::new("prose");
    let source = d.0.join("manuscript/Original.md");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "Private manuscript sentence never copied into Spark state.",
    )
    .unwrap();
    let mut e = d.engine(81);
    let r = e.generate(Mode::Combined, &now()).unwrap();
    e.favorite().unwrap();
    e.dismiss().unwrap();
    e.daily(&now()).unwrap();
    let raw = fs::read_to_string(d.0.join(".grimoire/writingdeck/spark/state.json")).unwrap();
    assert!(!raw.contains("Private manuscript"));
    assert!(!raw.contains(&catalog().unwrap().resolve(&r).unwrap().0.unwrap().text));
    assert!(!raw.contains("instructions"));
    assert!(!raw.contains("\"text\""));
    assert_eq!(
        fs::read_to_string(source).unwrap(),
        "Private manuscript sentence never copied into Spark state."
    );
}
#[test]
fn exports_are_explicit_exclusive_reused_and_keep_user_edits() {
    let d = Fixture::new("export");
    let mut e = d.engine(14);
    let r = e.generate(Mode::Combined, &now()).unwrap();
    assert!(!d.0.join("notes").exists());
    let note = e.export(ExportKind::Save, None).unwrap();
    let full = d.0.join(&note);
    let original = fs::read_to_string(&full).unwrap();
    assert!(original.contains(&r.id));
    assert!(original.contains("license MIT"));
    fs::write(
        &full,
        format!("{original}\nWriter's added interpretation.\n"),
    )
    .unwrap();
    let changed = fs::read(&full).unwrap();
    assert_eq!(e.export(ExportKind::Save, None).unwrap(), note);
    assert_eq!(fs::read(&full).unwrap(), changed);
    for kind in [ExportKind::Task, ExportKind::Outline, ExportKind::Scene] {
        let p = e.export(kind, None).unwrap();
        let text = fs::read_to_string(d.0.join(&p)).unwrap();
        assert!(text.contains("compile: false"));
        if kind == ExportKind::Scene {
            assert!(p.starts_with("manuscript"));
            assert!(text.contains("spark-reference: notes/sparks/"));
        }
        if kind == ExportKind::Task {
            assert!(text.contains("- [ ]"));
        }
    }
    fs::remove_file(&full).unwrap();
    assert!(e.export(ExportKind::Save, None).is_err());
    assert!(!full.exists());
}
#[test]
fn failed_export_prepares_one_receipt_and_retry_does_not_duplicate() {
    let d = Fixture::new("export-retry");
    let mut e = d.engine(6);
    e.generate(Mode::Combined, &now()).unwrap();
    fs::create_dir_all(d.0.join("notes")).unwrap();
    fs::write(d.0.join("notes/sparks"), "blocker").unwrap();
    assert!(e.export(ExportKind::Save, None).is_err());
    assert_eq!(e.state().exports.len(), 1);
    assert_eq!(e.state().exports[0].phase, Phase::Prepared);
    fs::remove_file(d.0.join("notes/sparks")).unwrap();
    let p = e.export(ExportKind::Save, None).unwrap();
    assert!(d.0.join(p).is_file());
    assert_eq!(e.state().exports.len(), 1);
    assert_eq!(e.state().exports[0].phase, Phase::Ready);
}
#[test]
fn prepared_matching_file_resumes_but_conflicting_original_is_never_overwritten() {
    let d = Fixture::new("prepared");
    let mut e = d.engine(19);
    let r = e.generate(Mode::Combined, &now()).unwrap();
    let target = export_path(&r.id, ExportKind::Save, None).unwrap();
    let text = markdown(e.bank, &r, ExportKind::Save).unwrap();
    e.store
        .update(|s| {
            s.exports.push(ExportReceipt {
                result: r,
                kind: ExportKind::Save,
                target: target.clone(),
                scratch_id: None,
                sha256: sha256(text.as_bytes()),
                phase: Phase::Prepared,
            });
            Ok(())
        })
        .unwrap();
    fs::create_dir_all(d.0.join(target.parent().unwrap())).unwrap();
    fs::write(d.0.join(&target), "preexisting writer original").unwrap();
    assert!(e.export(ExportKind::Save, None).is_err());
    assert_eq!(
        fs::read_to_string(d.0.join(&target)).unwrap(),
        "preexisting writer original"
    );
    fs::write(d.0.join(&target), &text).unwrap();
    assert_eq!(e.export(ExportKind::Save, None).unwrap(), target);
    assert_eq!(e.state().exports[0].phase, Phase::Ready);
    assert_eq!(fs::read_to_string(d.0.join(&target)).unwrap(), text);
}
#[test]
fn metadata_publication_failure_happens_before_source_creation() {
    let d = Fixture::new("metadata-export");
    let mut e = d.engine(1);
    let r = e.generate(Mode::Plot, &now()).unwrap();
    let state = d.0.join(".grimoire/writingdeck/spark/state.json");
    fs::write(&state, b"{\"version\":2}").unwrap();
    assert!(e.export(ExportKind::Save, None).is_err());
    assert!(
        !d.0.join(export_path(&r.id, ExportKind::Save, None).unwrap())
            .exists()
    );
    assert_eq!(fs::read(state).unwrap(), b"{\"version\":2}");
}
#[test]
fn optional_assignment_requires_explicit_acceptance_and_truthful_self_report() {
    let d = Fixture::new("assignment");
    let mut e = d.engine(22);
    let r = e.generate(Mode::Combined, &now()).unwrap();
    assign(&d.0, &r, &now()).unwrap();
    assert!(accept_or_complete(&d.0, &now().day(), &now(), true).is_err());
    accept_or_complete(&d.0, &now().day(), &now(), false).unwrap();
    accept_or_complete(&d.0, &now().day(), &now(), true).unwrap();
    let a = assignment(&d.0, &now().day()).unwrap().unwrap();
    assert_eq!(a.value.status, AssignmentStatus::Completed);
    assert!(a.value.accepted_at.is_some());
    assert!(a.value.completed_at.is_some());
    assert!(
        !fs::read_to_string(d.0.join(assignment_path(&now().day()).unwrap()))
            .unwrap()
            .contains("measured")
    );
    let another = e.generate(Mode::Plot, &now()).unwrap();
    assert!(assign(&d.0, &another, &now()).is_err());
    assert_eq!(
        assignment(&d.0, &now().day())
            .unwrap()
            .unwrap()
            .value
            .result,
        r
    );
}
#[test]
fn daily_and_assignment_corruption_date_mismatch_and_read_only_checks_preserve_bytes() {
    let d = Fixture::new("date-corrupt");
    assert_eq!(validate_existing(&d.0).unwrap(), (0, 0));
    assert!(!d.0.join(".grimoire").exists());
    let mut e = d.engine(2);
    e.daily(&now()).unwrap();
    let p = d.0.join(daily_path(&now().day()).unwrap());
    let bytes = b"{\"version\":99}";
    fs::write(&p, bytes).unwrap();
    assert!(e.daily(&now()).is_err());
    assert!(validate_existing(&d.0).is_err());
    assert_eq!(fs::read(p).unwrap(), bytes);
}
#[cfg(unix)]
#[test]
fn metadata_and_generated_source_aliases_cannot_escape_or_replace_originals() {
    use std::os::unix::fs::symlink;
    let d = Fixture::new("alias");
    let outside = Fixture::new("outside");
    let mut e = d.engine(4);
    e.generate(Mode::Plot, &now()).unwrap();
    fs::create_dir_all(d.0.join("notes")).unwrap();
    symlink(&outside.0, d.0.join("notes/sparks")).unwrap();
    assert!(e.export(ExportKind::Save, None).is_err());
    assert!(fs::read_dir(&outside.0).unwrap().next().is_none());
    fs::remove_file(d.0.join("notes/sparks")).unwrap();
    let meta = d.0.join(".grimoire/writingdeck/spark/state.json");
    fs::remove_file(&meta).unwrap();
    symlink(outside.0.join("untouched"), &meta).unwrap();
    assert!(Engine::load(&d.0, 0, catalog().unwrap()).is_err());
    assert!(!outside.0.join("untouched").exists());
}
#[test]
fn counter_and_retention_bounds_fail_without_publishing_a_partial_clone() {
    let d = Fixture::new("bounds");
    let mut e = d.engine(2);
    e.store
        .update(|s| {
            s.counter = u64::MAX;
            Ok(())
        })
        .unwrap();
    let old = e.state().clone();
    assert!(e.generate(Mode::Plot, &now()).is_err());
    assert_eq!(e.state(), &old);
    assert!(
        e.store
            .update(|s| {
                s.version = 2;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(e.state(), &old);
}

#[test]
fn resolving_imported_known_version_reference_refuses_an_incompatible_pair() {
    let d = Fixture::new("import-pair");
    let mut engine = d.engine(4);
    let mut r = engine.generate(Mode::Combined, &now()).unwrap();
    let bank = catalog().unwrap();
    let exercise = bank
        .records
        .iter()
        .find(|r| r.kind == Kind::Exercise && r.work_mode != Some(WorkMode::Drafting))
        .unwrap();
    r.exercise = Some(Component::from(exercise));
    r.validate().unwrap();
    assert!(bank.resolve(&r).is_err());
}
#[test]
fn experimental_scene_has_no_prompt_body_and_only_a_reference_to_saved_notes() {
    let d = Fixture::new("empty-scene");
    let mut engine = d.engine(1);
    let r = engine.generate(Mode::Combined, &now()).unwrap();
    let text = markdown(catalog().unwrap(), &r, ExportKind::Scene).unwrap();
    let body = text.split("---\n").nth(2).unwrap();
    assert!(body.trim().is_empty());
    assert!(text.contains(&format!("spark-reference: notes/sparks/{}.md", r.id)));
    assert!(text.contains("compile: false"));
    assert!(!text.contains(&catalog().unwrap().resolve(&r).unwrap().0.unwrap().text));
}

#[test]
fn prepared_retry_refuses_foreign_store_before_creating_any_new_source() {
    let d = Fixture::new("prepared-foreign");
    let mut engine = d.engine(1);
    engine.generate(Mode::Plot, &now()).unwrap();
    fs::create_dir_all(d.0.join("notes")).unwrap();
    fs::write(d.0.join("notes/sparks"), "blocker").unwrap();
    assert!(engine.export(ExportKind::Save, None).is_err());
    let target = engine.state().exports[0].target.clone();
    fs::remove_file(d.0.join("notes/sparks")).unwrap();
    let meta = d.0.join(".grimoire/writingdeck/spark/state.json");
    let bytes = b"{\"version\":77}";
    fs::write(&meta, bytes).unwrap();
    assert!(engine.export(ExportKind::Save, None).is_err());
    assert!(!d.0.join(target).exists());
    assert_eq!(fs::read(meta).unwrap(), bytes);
}
