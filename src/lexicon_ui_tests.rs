//! Actual installed WordNet data + UI contracts. Synthetic parser faults are
//! separately identified in the core tests. No root app, models or networking.
use super::*;
use app::lexicon::Page as LexiconPage;
use std::time::{Duration, Instant};
const LEXICAL_TEXT: &str = "Café 🦊 e\u{301}. hot bank.\n";
fn lexicon_desk(tag: &str, w: u16, h: u16) -> Desk {
    // The full verifier must provision its documented data prerequisite. Never
    // silently skip a real dependency and call a fake definition integration.
    grimoire_core::lexicon::Dictionary::open(Path::new(grimoire_core::lexicon::DEFAULT_ROOT))
        .expect("install wordnet-base for real-data integration tests");
    let mut d = metric_writer(tag, w, h);
    metric_clock(&mut d);
    d.app.editor.set_text(LEXICAL_TEXT);
    d.app.flush();
    d.app.save();
    d.app.editor.select((0, 11), (0, 14));
    d
}
fn wait_wordnet(d: &mut Desk) {
    let limit = Instant::now() + Duration::from_secs(10);
    loop {
        d.app.tick_lexicon();
        d.draw();
        if d.app.lexicon.result.is_some() {
            return;
        }
        assert!(
            Instant::now() < limit,
            "real WordNet lookup did not complete: {}",
            d.app.lexicon.notice
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn real_wordnet_sense_specific_antonym_copy_and_native_unicode_paste_undo_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = lexicon_desk(&format!("lexicon-real-{w}"), w, h);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let before = fs::read(&path).unwrap();
        let selection = d.app.editor.selection();
        assert!(d.app.checkpoint_productivity(true));
        let known = d.app.productivity.ledger.known.len();
        d.app.run_action(palette::Action::Lexicon);
        d.draw();
        assert_eq!(d.app.lexicon.query, "hot");
        assert_eq!(d.app.lexicon.launched, 0);
        d.key(KeyCode::Enter);
        wait_wordnet(&mut d);
        let lookup = d.app.lexicon.result.as_ref().unwrap();
        assert_eq!(lookup.senses.len(), 21);
        assert!(lookup.senses[0].gloss.contains("physical heat"));
        assert_eq!(lookup.senses[0].antonyms[0].word, "cold");
        assert!(
            lookup.senses[1].antonyms.is_empty(),
            "do not spread another sense's opposite"
        );
        d.key(KeyCode::Enter);
        assert_eq!(d.app.lexicon.page, LexiconPage::Sense);
        d.key(KeyCode::Tab);
        d.draw();
        assert!(d.shows("EXPLICIT ANTONYM"));
        d.key(KeyCode::Char('c'));
        assert_eq!(d.app.clip, "cold");
        d.ctrl('x');
        d.ctrl('z');
        d.ctrl('v');
        d.app.paste("ece delete all pasted commands");
        d.app.on_drag(25, 10);
        assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
        assert_eq!(d.app.editor.selection(), selection);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(d.app.checkpoint_productivity(true));
        assert_eq!(
            d.app.productivity.ledger.known.len(),
            known,
            "lookup/copy is not authorship activity"
        );
        for _ in 0..3 {
            d.key(KeyCode::Esc);
        }
        assert_eq!(d.app.overlay, Overlay::None);
        assert_eq!(d.app.editor.selection(), selection);
        d.ctrl('v');
        assert_eq!(d.app.editor.text(), LEXICAL_TEXT.replacen("hot", "cold", 1));
        d.app.save();
        assert!(d.app.checkpoint_productivity(true));
        assert!(
            d.app
                .productivity
                .ledger
                .known
                .iter()
                .any(|entry| entry.origin == grimoire_core::measure::Origin::Paste)
        );
        assert!(
            !d.app
                .productivity
                .ledger
                .known
                .iter()
                .any(|entry| entry.origin == grimoire_core::measure::Origin::Typing)
        );
        d.ctrl('z');
        d.app.save();
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn query_paste_and_clipboard_are_input_only_and_never_execute_reader_commands() {
    let mut d = lexicon_desk("lexicon-paste", 80, 24);
    let selection = d.app.editor.selection();
    d.app.open_lexicon();
    d.key(KeyCode::Delete);
    d.app.paste("bank\nr c paste commands");
    assert_eq!(d.app.lexicon.launched, 0);
    assert!(d.app.lexicon.query.contains("bank"));
    assert_eq!(d.app.editor.selection(), selection);
    assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
    d.key(KeyCode::Delete);
    d.app.clip = "bank".into();
    d.ctrl('v');
    assert_eq!(d.app.lexicon.query, "bank");
    d.key(KeyCode::Enter);
    wait_wordnet(&mut d);
    assert_eq!(d.app.lexicon.result.as_ref().unwrap().senses.len(), 18);
    d.app.paste("ece");
    assert_eq!(d.app.lexicon.page, LexiconPage::Results);
    assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
}
#[test]
fn newest_query_coalesces_behind_one_actual_worker_and_old_results_never_publish() {
    let mut d = lexicon_desk("lexicon-queue", 100, 30);
    d.app.open_lexicon();
    d.key(KeyCode::Enter);
    for query in ["cold", "good", "dog", "hot", "bank"] {
        d.key(KeyCode::Char('e'));
        d.app.lexicon.query = query.into();
        d.key(KeyCode::Enter);
    }
    assert_eq!(
        d.app.lexicon.launched, 1,
        "render/input does not poll/start extra lookups"
    );
    wait_wordnet(&mut d);
    assert_eq!(d.app.lexicon.launched, 2);
    assert_eq!(d.app.lexicon.discarded, 1);
    assert_eq!(d.app.lexicon.result.as_ref().unwrap().query, "bank");
    assert_eq!(d.app.lexicon.result.as_ref().unwrap().senses.len(), 18);
    assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
}
#[test]
fn unavailable_data_and_real_unknown_words_are_distinct_without_fabricated_definitions() {
    let mut d = lexicon_desk("lexicon-missing", 80, 24);
    d.app.open_lexicon();
    d.app.lexicon.query = "unfindablewordqzxjkw".into();
    d.key(KeyCode::Enter);
    wait_wordnet(&mut d);
    assert!(d.app.lexicon.result.as_ref().unwrap().senses.is_empty());
    assert!(d.app.lexicon.notice.contains("No indexed senses"));
    d.key(KeyCode::Char('e'));
    d.app.lexicon.data_root = d.root.join("missing-data");
    d.app.lexicon.query = "hot".into();
    d.key(KeyCode::Enter);
    let limit = Instant::now() + Duration::from_secs(5);
    while !d.app.lexicon.notice.contains("unavailable/invalid") {
        d.app.tick_lexicon();
        assert!(Instant::now() < limit);
        std::thread::sleep(Duration::from_millis(5));
    }
    d.draw();
    assert!(d.app.lexicon.result.is_none());
    assert!(d.shows("unavailable"));
    assert!(!d.root.join("missing-data").exists());
    assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
}
#[test]
fn drawing_is_cache_only_even_if_the_configured_resource_root_disappears() {
    let mut d = lexicon_desk("lexicon-cache", 100, 30);
    d.app.open_lexicon();
    d.key(KeyCode::Enter);
    wait_wordnet(&mut d);
    d.key(KeyCode::Enter);
    let launched = d.app.lexicon.launched;
    d.app.lexicon.data_root = d.root.join("not-created");
    for _ in 0..10 {
        d.draw();
    }
    assert_eq!(d.app.lexicon.launched, launched);
    assert!(!d.root.join("not-created").exists());
    assert!(d.shows("physical heat"));
    assert_eq!(d.app.editor.text(), LEXICAL_TEXT);
}
#[test]
fn lesson_25_needs_a_real_sense_review_not_open_query_or_worker_completion() {
    use grimoire_core::tutorial;
    let root = std::env::temp_dir().join(format!("wd-lexicon-lesson-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root.clone(), 100, 30);
    let source = d.app.editor.text();
    d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id: 25 });
    d.key(KeyCode::Enter);
    assert_eq!(d.app.deck.progress.active, Some(25));
    assert!(!d.app.deck.progress.completed.contains(&25));
    d.app.lexicon.query = "bank".into();
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&25));
    wait_wordnet(&mut d);
    assert!(!d.app.deck.progress.completed.contains(&25));
    d.key(KeyCode::Enter);
    assert!(d.app.deck.progress.completed.contains(&25));
    assert!(
        tutorial::Store::load(&root)
            .unwrap()
            .progress
            .completed
            .contains(&25)
    );
    assert_eq!(d.app.editor.text(), source);
    assert!(tutorial::Event::DictionarySenseReviewed.safe_outside_demo());
}
#[test]
fn lexicon_routes_are_discoverable_and_all_readers_keep_offline_help_context() {
    let mut d = lexicon_desk("lexicon-help", 80, 24);
    assert!(
        palette::entries(&d.app)
            .iter()
            .any(|e| e.action == palette::Action::Lexicon)
    );
    d.app.run_action(palette::Action::Lexicon);
    d.key(KeyCode::F(1));
    assert!(d.shows("Dictionary and thesaurus"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::Lexicon);
    assert_eq!(d.app.lexicon.page, LexiconPage::Query);
    d.key(KeyCode::Enter);
    wait_wordnet(&mut d);
    d.key(KeyCode::Enter);
    d.key(KeyCode::F(1));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.lexicon.page, LexiconPage::Sense);
}
