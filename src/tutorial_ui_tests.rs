use super::*;
use crate::{
    app::{backup::Finished, shell::Route, wros::Page},
    backup_service::{self, Status},
};

fn wait_system(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while desk.app.deck.busy && std::time::Instant::now() < end {
        desk.app.tick_deck();
        std::thread::yield_now();
    }
    assert!(!desk.app.deck.busy);
    desk.draw();
}

/// Start lesson `id` the way the writer does: its page, then Enter.
fn launch(desk: &mut Desk, id: u8) {
    desk.app.overlay = Overlay::Deck(Page::Lesson { id });
    desk.draw();
    desk.key(KeyCode::Enter);
    assert_eq!(desk.app.deck.progress.active, Some(id));
}

fn done(desk: &Desk, id: u8) -> bool {
    desk.app.deck.progress.completed.contains(&id)
        && grimoire_core::tutorial::Store::load(&desk.root)
            .unwrap()
            .progress
            .completed
            .contains(&id)
}

/// A backup operation that has already finished with `result`.
fn finished_backup(desk: &mut Desk, kind: Finished, result: Result<Status, String>) {
    let backup = &mut desk.app.deck.shell.backup;
    backup.job = Some(backup_service::spawn_work("test", move |_, _| result).unwrap());
    backup.running = Some(kind);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while desk.app.deck.shell.backup.job.is_some() && std::time::Instant::now() < end {
        desk.app.tick_deck();
        std::thread::yield_now();
    }
    desk.draw();
}

#[test]
fn backup_and_phone_lessons_complete_only_when_the_operation_finishes() {
    let mut desk = Desk::open(book("tutorial-backup-lessons", true), 100, 30);
    launch(&mut desk, 36);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
    finished_backup(
        &mut desk,
        Finished::Restore,
        Err("Drive unreachable".into()),
    );
    assert!(!done(&desk, 36), "a failed restore completed the lesson");
    // Another finished operation, such as linking, is not a restore.
    let linked = || Status {
        google_connected: true,
        ..Status::default()
    };
    finished_backup(&mut desk, Finished::LinkPhone, Ok(linked()));
    assert!(!done(&desk, 36));
    finished_backup(&mut desk, Finished::Restore, Ok(Status::default()));
    assert!(done(&desk, 36));

    launch(&mut desk, 41);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::GoogleAccount);
    // The helper answered, but Drive is still not connected.
    finished_backup(&mut desk, Finished::LinkPhone, Ok(Status::default()));
    assert!(!done(&desk, 41));
    finished_backup(&mut desk, Finished::LinkPhone, Ok(linked()));
    assert!(done(&desk, 41));
}

#[test]
fn sync_lesson_completes_on_a_finished_sync_not_on_a_stopped_one() {
    use crate::sync_service::{Reply, Request as Sync};
    let mut desk = Desk::open(book("tutorial-sync-lesson", true), 100, 30);
    launch(&mut desk, 37);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::SyncPanel);
    let project = desk.root.to_string_lossy().into_owned();
    for (status, completes) in [("confirm", false), ("offline", false), ("synced", true)] {
        let sync = &mut desk.app.deck.shell.backup.sync;
        let reply = Reply {
            status: status.into(),
            ..Reply::default()
        };
        sync.job = Some(backup_service::spawn_work("test", move |_, _| Ok(reply)).unwrap());
        sync.running = Some(Sync::Sync {
            project: project.clone(),
            allow_deletes: false,
        });
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while desk.app.deck.shell.backup.sync.job.is_some() && std::time::Instant::now() < end {
            desk.app.tick_deck();
            std::thread::yield_now();
        }
        assert_eq!(done(&desk, 37), completes, "sync status {status}");
    }
}

#[test]
fn warm_screen_lesson_needs_a_kept_setting_not_a_cancelled_preview() {
    let mut desk = Desk::open(book("tutorial-warm-lesson", true), 100, 30);
    desk.app.deck.simulate();
    launch(&mut desk, 42);
    wait_system(&mut desk);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmScreen);
    desk.choose("Temperature: cooler +100 K");
    desk.choose("Preview choice");
    wait_system(&mut desk);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmPreview);
    desk.key(KeyCode::Esc);
    wait_system(&mut desk);
    assert!(!done(&desk, 42), "a cancelled preview completed the lesson");
    desk.choose("Preview choice");
    wait_system(&mut desk);
    desk.choose("Keep these display settings");
    wait_system(&mut desk);
    assert!(done(&desk, 42));
}

#[test]
fn wifi_lesson_completes_on_a_finished_scan() {
    let mut desk = Desk::open(book("tutorial-wifi-lesson", true), 100, 30);
    desk.app.deck.simulate();
    launch(&mut desk, 44);
    wait_system(&mut desk);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(crate::app::shell::Panel::Network)
    );
    assert!(!done(&desk, 44), "opening the page completed the lesson");
    desk.choose("Scan Wi-Fi networks");
    wait_system(&mut desk);
    assert!(done(&desk, 44));
}

#[test]
fn go_back_lesson_needs_a_kept_version_on_the_go_back_screen() {
    let mut desk = Desk::open(book("tutorial-go-back-lesson", true), 100, 30);
    let mut sim = wros_sys::Simulated::laptop();
    sim.recovery.previous_version = Some("0.1.0~rc7".into());
    desk.app.deck.use_test_backend(Box::new(sim));
    launch(&mut desk, 43);
    wait_system(&mut desk);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(crate::app::shell::Panel::Updates)
    );
    desk.app.refresh_recovery();
    wait_system(&mut desk);
    assert!(!done(&desk, 43));
    desk.choose("Go back to 0.1.0~rc7");
    assert_eq!(desk.app.deck.shell.nav.current(), Route::GoBackConfirm);
    assert!(done(&desk, 43));
    // Seeing the screen changes nothing: the default row stays on this version.
    desk.key(KeyCode::Enter);
    desk.app.refresh_recovery();
    wait_system(&mut desk);
    let kept = desk.app.deck.shell.recovery.as_ref().unwrap();
    assert_eq!(kept.previous_version.as_deref(), Some("0.1.0~rc7"));
}

#[test]
fn lock_lesson_completes_when_the_writer_unlocks() {
    let mut desk = Desk::open(book("tutorial-lock-lesson", true), 100, 30);
    launch(&mut desk, 45);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(crate::app::shell::Panel::Power)
    );
    assert!(!done(&desk, 45));
    desk.app.on_unlocked();
    assert_eq!(
        desk.app.deck.shell.notices.latest(),
        Some("Writing session unlocked.")
    );
    assert!(done(&desk, 45));
}
