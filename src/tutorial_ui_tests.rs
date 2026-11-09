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
    assert!(
        grimoire_core::tutorial::lessons()
            .iter()
            .any(|lesson| lesson.id == id),
        "lesson {id} is missing"
    );
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
    // Sync is turned on in Backups since the Synchronization page folded into it (#90).
    assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
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

#[test]
fn lid_lesson_needs_a_saved_choice() {
    let mut desk = Desk::open(book("tutorial-lid", true), 100, 30);
    desk.app.deck.lock_available = true;
    let blocked = desk.root.join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    desk.app.deck.lid_settings = Some(blocked.join("lid.json"));
    launch(&mut desk, 46);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(crate::app::shell::Panel::Power)
    );
    desk.choose("When the lid closes:");
    assert!(!done(&desk, 46));
    let settings = desk.root.join("settings/lid.json");
    desk.app.deck.lid_settings = Some(settings.clone());
    desk.choose("When the lid closes:");
    assert_eq!(crate::lid::load(&settings), crate::lid::LidClose::Lock);
    assert!(done(&desk, 46));
}

#[test]
fn battery_lesson_needs_a_warning_during_discharge() {
    let mut desk = Desk::open(book("tutorial-battery", true), 100, 30);
    launch(&mut desk, 47);
    let now = std::time::Instant::now();
    for (percent, discharging) in [(20, false), (21, true)] {
        desk.app.power_event(
            false,
            crate::battery::Reading {
                percent: Some(percent),
                discharging,
                critical: false,
            },
            now,
        );
        assert!(!done(&desk, 47));
    }
    desk.app.power_event(
        false,
        crate::battery::Reading {
            percent: Some(20),
            discharging: true,
            critical: false,
        },
        now,
    );
    desk.draw();
    assert!(desk.shows("Plug in soon"));
    assert!(done(&desk, 47));
}

#[test]
fn drive_files_lesson_starts_at_the_real_account_page() {
    let mut desk = Desk::open(book("tutorial-drive-files", true), 100, 30);
    launch(&mut desk, 48);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::GoogleAccount);
    finished_backup(
        &mut desk,
        Finished::LinkPhone,
        Ok(Status {
            google_connected: true,
            ..Status::default()
        }),
    );
    assert!(!done(&desk, 48), "linking Drive did not list a folder");
    desk.app.deck.shell.nav.open(Route::GoogleDrive);
    finished_backup(
        &mut desk,
        Finished::DriveList,
        Err("Drive unreachable".into()),
    );
    assert!(!done(&desk, 48));
    finished_backup(
        &mut desk,
        Finished::DriveList,
        Ok(Status {
            google_connected: true,
            drive_folder: "drive:".into(),
            ..Status::default()
        }),
    );
    assert!(desk.shows("This folder is empty."));
    assert!(done(&desk, 48));
}

#[test]
fn the_merge_lesson_uses_real_choices_on_a_disposable_sample() {
    let root = book("tutorial-merge", true);
    let scene = first_scene(&root);
    let before = fs::read(&scene).unwrap();
    fs::write(root.join(".wros-demo"), "WrOS safe tutorial project v1\n").unwrap();
    let mut desk = Desk::open(root, 100, 30);
    launch(&mut desk, 49);
    assert!(desk.shows("MERGE PRACTICE"));
    assert!(!done(&desk, 49));
    desk.key(KeyCode::Esc);
    assert!(!done(&desk, 49));
    assert_eq!(desk.app.overlay, Overlay::Deck(Page::Lesson { id: 49 }));
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Char('3'));
    assert!(done(&desk, 49));
    assert!(
        desk.app
            .msg
            .contains("Mara opened the gate. Mara locked the gate.")
    );
    assert_eq!(fs::read(&scene).unwrap(), before);
}

#[test]
fn start_screen_lesson_needs_a_saved_choice() {
    let mut desk = Desk::open(book("tutorial-start-screen", true), 100, 30);
    desk.app.deck.boot_settings_available = true;
    desk.app.deck.simulate();
    launch(&mut desk, 50);
    wait_system(&mut desk);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(crate::app::shell::Panel::Display)
    );
    assert!(
        !done(&desk, 50),
        "reading the current choice completed the lesson"
    );
    let before = desk.app.deck.boot_status.as_ref().unwrap().enabled;
    desk.choose("Start screen:");
    wait_system(&mut desk);
    assert_ne!(desk.app.deck.boot_status.as_ref().unwrap().enabled, before);
    assert!(done(&desk, 50));
}
