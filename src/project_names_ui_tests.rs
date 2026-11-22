//! rc22: a project's files take their new names when the desk opens it (#164).

use super::*;
use grimoire_core::project_files::{self, FileKind};

/// A project made before rc22: novel.toml, .grimoire/book-id, Novel-Format.md.
fn old_project(tag: &str) -> PathBuf {
    let root = book(tag, true);
    fs::write(root.join(".grimoire/project-id"), "old-id\n").unwrap();
    project_files::restore_legacy(&root).unwrap();
    assert!(root.join("novel.toml").is_file() && root.join("Novel-Format.md").is_file());
    root
}

fn recovery(probation: bool, staged_update: bool) -> wros_sys::RecoveryInfo {
    wros_sys::RecoveryInfo {
        root_filesystem: "ext4".into(),
        root_device: "/dev/mapper/root".into(),
        separate_home: false,
        rollback_capable: true,
        rollback_strategy: "nixos-generation".into(),
        interrupted_dpkg: false,
        unconfigured_packages: Vec::new(),
        staged_update,
        staged_packages: Vec::new(),
        update_state: wros_sys::UpdateState::default(),
        previous_version: Some("wros 0.1.0~rc21".into()),
        probation,
        went_back: false,
        went_back_automatically: false,
        status: String::new(),
    }
}

fn renamed(root: &Path) -> bool {
    FileKind::ALL
        .iter()
        .all(|k| !root.join(k.legacy()).exists())
        && root.join("project.toml").is_file()
}

#[test]
fn needs_old_names_for_rc21_and_unknown_but_not_rc22() {
    use crate::app::project_names::needs_old_names;
    for older in [
        Some("wros 0.1.0~rc21"),
        Some("0.1.0~rc4"),
        Some("wros 0.0.9"),
        Some("wros something"),
        Some("0.1.0~beta2"),
        None,
    ] {
        assert!(needs_old_names(older), "{older:?}");
    }
    for newer in [
        Some("wros 0.1.0~rc22"),
        Some("0.1.0~rc30"),
        Some("wros 0.1.0"),
        Some("0.1.1~rc1"),
        Some("1.0.0"),
    ] {
        assert!(!needs_old_names(newer), "{newer:?}");
    }
}

#[test]
fn opening_an_old_project_off_wros_renames_its_files() {
    let root = old_project("names-off-wros");
    let mut d = Desk::open(root.clone(), 120, 40);
    assert!(!d.app.deck.online_updates);
    d.app.tick_project_names();
    assert!(renamed(&root));
    assert_eq!(
        fs::read_to_string(root.join(".grimoire/project-id")).unwrap(),
        "old-id\n",
        "its history keeps its id"
    );
    assert_eq!(
        d.app.msg,
        "This project's files now use the new names (project.toml)."
    );
    // The tree is read again, so the layout guide opens from its new name.
    d.app.sync_tick();
    assert!(
        d.app
            .project
            .nodes
            .iter()
            .any(|n| n.path == root.join("Project-Format.md"))
    );
    d.app.msg.clear();
    d.app.tick_project_names();
    assert!(d.app.msg.is_empty(), "said once");
}

#[test]
fn on_wros_nothing_migrates_until_probation_is_over() {
    let root = old_project("names-probation");
    let mut d = Desk::open(root.clone(), 120, 40);
    d.app.deck.online_updates = true;
    d.app.tick_project_names();
    assert!(!renamed(&root), "the update status isn't known yet");
    d.app.deck.shell.recovery = Some(recovery(true, false));
    d.app.tick_project_names();
    assert!(
        !renamed(&root),
        "a new version on probation can still go back by itself"
    );
    d.app.deck.shell.update_flow.ready_reported = true;
    d.app.tick_project_names();
    assert!(renamed(&root), "once it has said it starts, the names move");
}

#[test]
fn nothing_migrates_while_a_go_back_is_staged() {
    let root = old_project("names-staged");
    let mut d = Desk::open(root.clone(), 120, 40);
    d.app.deck.online_updates = true;
    d.app.deck.shell.recovery = Some(recovery(false, true));
    d.app.tick_project_names();
    assert!(!renamed(&root));
    d.app.deck.shell.recovery = Some(recovery(false, false));
    d.app.names_held = true;
    d.app.tick_project_names();
    assert!(
        !renamed(&root),
        "Go back put the old names back in this session"
    );
    d.app.names_held = false;
    d.app.tick_project_names();
    assert!(renamed(&root));
}

#[cfg(unix)]
#[test]
fn a_read_only_project_opens_and_says_once_it_kept_its_old_names() {
    use std::os::unix::fs::PermissionsExt;
    let root = old_project("names-read-only");
    let mut d = Desk::open(root.clone(), 120, 40);
    let set = |mode| {
        for p in [root.clone(), root.join(".grimoire")] {
            fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    set(0o555);
    if fs::write(root.join("probe"), "").is_ok() {
        // Running as root: nothing is read-only.
        set(0o755);
        return;
    }
    d.app.tick_project_names();
    assert!(
        d.app
            .msg
            .starts_with("Couldn't rename this project's files (novel.toml: "),
        "{}",
        d.app.msg
    );
    assert!(d.app.msg.ends_with("; it opened as before."));
    assert!(root.join("novel.toml").is_file());
    d.app.msg.clear();
    d.app.tick_project_names();
    assert!(d.app.msg.is_empty(), "not again on every tick");
    set(0o755);
}

#[test]
fn both_names_present_trashes_the_old_and_says_so() {
    let root = book("names-both", true);
    fs::write(root.join("novel.toml"), "title = \"Stray copy\"\n").unwrap();
    let mut d = Desk::open(root.clone(), 120, 40);
    d.app.tick_project_names();
    assert_eq!(
        d.app.msg,
        "novel.toml was replaced by project.toml; the old one is in this project's Trash."
    );
    assert!(!root.join("novel.toml").exists());
    let trashed: Vec<String> = fs::read_dir(root.join(".grimoire/trash"))
        .unwrap()
        .flatten()
        .map(|e| fs::read_to_string(e.path()).unwrap_or_default())
        .collect();
    assert!(trashed.iter().any(|t| t.contains("Stray copy")));
}
