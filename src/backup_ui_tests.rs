use super::*;
use crate::{
    app::{backup::FormKind, shell::Route, wros::Page},
    backup_service::{DriveEntry, QueuedBackup},
};
use grimoire_core::documents::Pane;

#[test]
fn google_authorization_is_masked_owned_by_the_form_and_discarded_on_escape_at_all_sizes() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("backup-secret-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Original backup writing context. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let text = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);
        desk.app.open_settings();
        desk.app.deck.shell.nav.open(Route::Backups);
        desk.app.open_backup_form(FormKind::Google, String::new());
        desk.app
            .paste(r#"{"access_token":"PRIVATE_ACCESS_FIXTURE"}"#);
        desk.draw();
        assert!(desk.shows("[hidden]"));
        assert!(!desk.rows().join("\n").contains("PRIVATE_ACCESS_FIXTURE"));
        desk.app.clip = "CLIPBOARD_SECRET_FIXTURE".into();
        desk.ctrl('v');
        assert!(!desk.rows().join("\n").contains("CLIPBOARD_SECRET_FIXTURE"));
        assert_eq!(desk.app.editor.text(), text);
        desk.key(KeyCode::Esc);
        assert!(desk.app.deck.shell.backup.form.is_none());
        assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), text);
    }
}

#[test]
fn cancelled_repository_setup_and_restore_forms_do_not_write_files_or_change_the_book() {
    let mut desk = Desk::open(book("backup-cancel", true), 80, 24);
    let original = desk.app.editor.text();
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    for kind in [
        FormKind::NewLocal,
        FormKind::NewDrive,
        FormKind::Connect,
        FormKind::Restore,
        FormKind::ExportKey,
    ] {
        desk.app.open_backup_form(kind, "a".repeat(64));
        desk.app.paste("/tmp/must-not-be-created");
        desk.key(KeyCode::Tab);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
        assert!(desk.app.deck.shell.backup.form.is_none());
        assert!(desk.app.deck.shell.backup.job.is_none());
        assert_eq!(desk.app.editor.text(), original);
    }
}

#[test]
fn linking_from_the_phone_shows_the_code_to_scan_and_the_code_to_type() {
    use crate::backup_service::{Job, LinkOffer, Progress};
    let mut desk = Desk::open(book("link-phone", true), 170, 48);
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::GoogleAccount);
    desk.draw();
    assert!(!desk.shows("with your phone")); // a build without WrOS's Google client
    desk.app.deck.shell.backup.status.phone_linking = true;
    desk.draw();
    assert!(desk.shows("Link Google Drive with your phone"));
    desk.app.deck.shell.backup.status.google_connected = true;
    desk.draw();
    assert!(desk.shows("Relink Google Drive with your phone"));
    assert!(!desk.shows("shared Google access"));
    // A connection made with rclone's own sign-in, which Google throttles: suggest relinking.
    desk.app.deck.shell.backup.status.google_shared_client = true;
    desk.draw();
    assert!(
        desk.shows("Drive uses rclone's shared Google access, which Google often slows down"),
        "{}",
        desk.rows().join("\n")
    );
    let qr: Vec<String> = (0..41)
        .map(|i| {
            if i % 2 == 0 {
                "01".repeat(20) + "0"
            } else {
                "1".repeat(41)
            }
        })
        .collect();
    desk.app.deck.shell.backup.job = Some(Job::running_for_test(Some(Progress {
        phase: "Waiting for your phone".into(),
        percent: None,
        offer: Some(LinkOffer {
            link: "https://j5cfk7qm49-web.github.io/WrOS/l/#abc".into(),
            code: "KXP-QKN-VLWD".into(),
            qr,
        }),
    })));
    desk.app.deck.shell.nav.open(Route::LinkPhone);
    desk.draw();
    assert!(desk.shows("KXP-QKN-VLWD"));
    assert!(desk.shows("Finish linking"));
    // Light modules are white blocks, so the code reads dark-on-light on the black console.
    assert!(desk.rows().iter().filter(|r| r.contains("██")).count() >= 41);
    desk.key(KeyCode::Esc);
    assert!(desk.app.deck.shell.backup.job.is_none()); // leaving the screen stops linking
}

#[test]
fn the_home_screen_says_when_the_book_was_last_backed_up() {
    let mut desk = Desk::open(book("backup-home", true), 120, 40);
    desk.app.overlay = Overlay::Deck(Page::Home { sel: 0 });
    desk.draw();
    assert!(desk.shows("set up encrypted backup and restore"));
    let root = desk.app.project.root.canonicalize().unwrap();
    let when = chrono::Local::now() - chrono::Duration::minutes(5);
    let backup = &mut desk.app.deck.shell.backup;
    backup.status.configured = true;
    backup
        .status
        .snapshots
        .push(crate::backup_service::Snapshot {
            id: "c".repeat(64),
            time: when.to_rfc3339(),
            _path: root.to_string_lossy().into_owned(),
            project: "Book".into(),
            restore_checked_at: when.to_rfc3339(),
            has_profile: true,
        });
    desk.draw();
    assert!(desk.shows(&format!("Backed up {} · verified", when.format("%H:%M"))));
}

#[test]
fn backups_that_carry_settings_offer_restoring_them_too() {
    let mut desk = Desk::open(book("backup-settings", true), 100, 40);
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    for (id, has_profile) in [("a", true), ("b", false)] {
        desk.app
            .deck
            .shell
            .backup
            .status
            .snapshots
            .push(crate::backup_service::Snapshot {
                id: id.repeat(64),
                time: "2026-11-05T21:10:00-08:00".into(),
                _path: "/book".into(),
                project: "Book".into(),
                restore_checked_at: String::new(),
                has_profile,
            });
    }
    desk.draw();
    // Only the backup made with settings offers them back.
    assert_eq!(
        desk.rows()
            .join("\n")
            .matches("Restore with your settings")
            .count(),
        1
    );
    let root = desk.app.project.root.clone();
    for (kind, settings) in [
        (FormKind::RestoreWithSettings, true),
        (FormKind::Restore, false),
    ] {
        desk.app.open_backup_form(kind, "a".repeat(64));
        let form = desk.app.deck.shell.backup.form.take().unwrap();
        assert!(matches!(
            form.request(&root),
            crate::backup_service::Request::Restore { settings: s, ref snapshot, .. }
                if s == settings && *snapshot == "a".repeat(64)
        ));
    }
    desk.app
        .open_backup_form(FormKind::RestoreWithSettings, "a".repeat(64));
    desk.draw();
    assert!(desk.shows("Your current settings are kept in a dated folder first"));
}

#[test]
fn backup_refuses_source_conflicts_and_preserves_external_and_unsaved_words() {
    let mut desk = Desk::open(book("backup-conflict", true), 100, 30);
    desk.app.focus = Focus::Editor;
    desk.typed("Unsaved local work retained. ");
    let path = desk.app.project.nodes[desk.app.open.unwrap()].path.clone();
    fs::write(&path, "External writing must remain.\n").unwrap();
    desk.app.overlay = Overlay::Deck(Page::Settings);
    desk.app.deck.shell.nav.reset(Route::Backups);
    desk.app.request_project_backup();
    assert!(desk.app.deck.shell.backup.job.is_none());
    assert!(desk.app.deck.shell.backup.notice.contains("conflicts"));
    desk.app.request_project_backup_queue();
    assert!(desk.app.deck.shell.backup.job.is_none());
    assert!(desk.app.deck.shell.backup.notice.contains("conflicts"));
    desk.app.request_run_backup_queue();
    assert!(desk.app.deck.shell.backup.job.is_none());
    assert!(desk.app.deck.shell.backup.notice.contains("conflicts"));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "External writing must remain.\n"
    );
    assert!(
        fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| fs::read_to_string(entry.path())
                .is_ok_and(|text| text.contains("Unsaved local work retained.")))
    );
}

#[test]
fn google_account_lifecycle_and_offline_queue_views_preserve_both_editors_at_all_sizes() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("google-lifecycle-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Original Google lifecycle and queue context. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let text = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        desk.app.open_settings();
        desk.app.deck.shell.nav.open(Route::GoogleAccount);
        desk.app.deck.shell.backup.job = None;
        desk.app.deck.shell.backup.status.google_connected = true;
        desk.app.deck.shell.backup.status.google_state = "reauthorization-required".into();
        desk.app.deck.shell.backup.status.google_verified_at = "2026-10-31T04:00:00Z".into();
        desk.app.deck.shell.backup.status.google_expiry = "2026-10-31T05:00:00Z".into();
        desk.draw();
        assert!(desk.shows("reauthorization-required"));
        assert!(desk.shows("Verify Google Drive connection"));
        assert!(desk.shows("Reauthorize Google Drive"));
        assert!(desk.shows("Disconnect Google Drive"));

        // Open Disconnect confirmation; default is Cancel.
        desk.choose("Disconnect Google Drive");
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::GoogleDisconnectConfirm
        );
        assert!(desk.shows("Cancel and keep saved Google Drive credentials"));
        desk.key(KeyCode::Enter); // Default Cancel returns without starting a disconnect job.
        assert_eq!(desk.app.deck.shell.nav.current(), Route::GoogleAccount);
        assert!(desk.app.deck.shell.backup.job.is_none());

        // Inspect the offline backup queue, one step down from Backups.
        desk.app.deck.shell.backup.status.queued_backups = vec![QueuedBackup {
            id: "0123456789abcdef".into(),
            project: desk.root.to_string_lossy().into_owned(),
            label: "Novel".into(),
            queued_at: "2026-10-31T04:30:00Z".into(),
            attempts: 2,
            last_error: "Destination unavailable".into(),
            inventory_sha256: "a".repeat(64),
        }];
        desk.app.deck.shell.nav.open(Route::Backups);
        desk.app.deck.shell.backup.job = None;
        desk.draw();
        assert!(desk.shows("Offline queue"));
        desk.app.deck.shell.nav.open(Route::BackupQueue);
        desk.draw();
        assert!(desk.shows("Queue saved project for offline backup"));
        assert!(desk.shows("Run queued backups and retry interrupted jobs"));
        assert!(desk.shows("Clear pending backup queue"));

        for _ in 0..4 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), text);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), text);
    }
}

#[test]
fn drive_folder_browser_materialization_and_sync_forms_preserve_both_editors_at_all_sizes() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("drive-browser-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Original Drive browser and sync context. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let text = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        desk.app.open_settings();
        desk.app.deck.shell.nav.open(Route::GoogleDrive);
        desk.app.deck.shell.backup.status.drive_folder = "drive:WrOS-Library".into();
        desk.app.deck.shell.backup.status.drive_entries = vec![DriveEntry {
            path: "drive:WrOS-Library/Outline.md".into(),
            name: "Outline.md".into(),
            size: 1024,
            modified: "2026-10-31T10:00:00Z".into(),
            is_dir: false,
            cached: true,
        }];
        desk.draw();
        assert!(desk.shows("Browse app-scoped Google Drive folder"));
        assert!(desk.shows("General Drive file browsing requires restricted scopes"));
        assert!(desk.shows("Outline.md"));

        desk.app.open_backup_form(
            FormKind::DriveMaterialize,
            "drive:WrOS-Library/Outline.md".into(),
        );
        desk.draw();
        assert!(desk.shows("Materialize Google Drive item into a NEW local path"));
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::GoogleDrive);

        // Sync itself is the per-book readable Drive sync in Backups.
        desk.app.deck.shell.nav.open(Route::SyncPanel);
        desk.draw();
        assert!(desk.shows("Readable Google Drive sync is set per book"));
        assert!(desk.shows("This book is not synced"));

        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), text);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), text);
    }
}
