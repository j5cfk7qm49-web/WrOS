use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

// Each child gets its own PATH, so these tests never touch a desktop clipboard
// or change the environment of another test running on this thread pool.
fn isolated(name: &str, tools: bool, session: &str, success: bool) -> bool {
    if std::env::var_os("GRIMOIRE_CLIPBOARD_TEST").is_some() {
        return true;
    }
    let dir = tempfile::tempdir().unwrap();
    let clipboard = dir.path().join("clipboard");
    fs::write(&clipboard, "old desktop text").unwrap();
    if tools {
        // The child's PATH holds only the fake tools, so they name cat by its full path
        // (not /bin/cat, which a Nix build sandbox lacks).
        let cat = std::env::var_os("PATH")
            .and_then(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join("cat"))
                    .find(|c| c.is_file())
            })
            .expect("cat on PATH");
        for (name, operation) in [("wl-copy", ">"), ("wl-paste", "")] {
            let path = dir.path().join(name);
            let script = if success {
                format!(
                    "#!/bin/sh\nexec {} {operation} \"$GRIMOIRE_CLIPBOARD_FILE\"\n",
                    cat.display()
                )
            } else {
                "#!/bin/sh\nexit 1\n".into()
            };
            fs::write(&path, script).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("app::clipboard_fallback_tests::{name}"),
            "--nocapture",
        ])
        .env_clear()
        .env("PATH", dir.path())
        .env("WROS_SESSION", session)
        .env("GRIMOIRE_CLIPBOARD_TEST", "1")
        .env("GRIMOIRE_CLIPBOARD_FILE", &clipboard)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    false
}

fn app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("manuscript/01-Chapter/01-Scene.md");
    fs::create_dir_all(scene.parent().unwrap()).unwrap();
    fs::write(scene, "Café\nΩ").unwrap();
    let mut app = App::with(
        Project::load(dir.path()).unwrap(),
        Setup {
            music: music::Config::default(),
            theme: theme::default_theme(),
            settings: Settings::default(),
            background: false,
        },
    )
    .unwrap();
    app.overlay = Overlay::None;
    app.focus = app.editor_focus();
    assert!(app.open.is_some());
    // Exercise the real clipboard path without starting threads or reading HOME.
    app.background = true;
    (dir, app)
}

fn internal_round_trip() {
    let (_dir, mut app) = app();
    let text = app.editor.text();
    assert_eq!(text, "Café\nΩ");
    app.editor.select_all();
    app.cut();
    assert_eq!(app.editor.text(), "");
    assert_eq!(app.clip, text);
    assert_eq!(app.msg, "Cut 6 characters");
    assert_eq!(app.project.nodes[app.open.unwrap()].body, "");
    app.undo();
    assert_eq!(app.editor.text(), text);
    app.redo();
    assert_eq!(app.editor.text(), "");
    app.paste_clipboard();
    assert_eq!(app.editor.text(), text);

    app.editor.select_all();
    app.copy_selection();
    assert_eq!(app.msg, "Copied 6 characters");
    assert_eq!(app.editor.text(), text);
    app.editor.select((0, 0), (0, 1));
    app.copy_selection();
    assert_eq!(app.msg, "Copied 1 character");
    app.cut();
    assert_eq!(app.msg, "Cut 1 character");
    assert_eq!(app.editor.text(), "afé\nΩ");
    app.paste_clipboard();
    assert_eq!(app.editor.text(), text);
}

#[test]
fn cut_copy_and_paste_work_without_clipboard_tools() {
    if isolated(
        "cut_copy_and_paste_work_without_clipboard_tools",
        false,
        "",
        true,
    ) {
        internal_round_trip();
    }
}

#[test]
fn native_session_uses_internal_clipboard_even_with_tools_on_path() {
    if isolated(
        "native_session_uses_internal_clipboard_even_with_tools_on_path",
        true,
        "drm",
        true,
    ) {
        internal_round_trip();
        let path = std::env::var_os("GRIMOIRE_CLIPBOARD_FILE").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "old desktop text");
    }
}

#[test]
fn desktop_tools_still_copy_cut_and_take_priority_for_paste() {
    if !isolated(
        "desktop_tools_still_copy_cut_and_take_priority_for_paste",
        true,
        "",
        true,
    ) {
        return;
    }
    let (_dir, mut app) = app();
    let path = std::env::var_os("GRIMOIRE_CLIPBOARD_FILE").unwrap();
    app.editor.select_all();
    app.cut();
    assert_eq!(app.editor.text(), "");
    assert_eq!(fs::read_to_string(&path).unwrap(), "Café\nΩ");
    fs::write(&path, "desktop").unwrap();
    app.paste_clipboard();
    assert_eq!(app.editor.text(), "desktop");
    app.editor.select_all();
    app.copy_selection();
    assert_eq!(app.msg, "Copied 7 characters");
    assert_eq!(fs::read_to_string(path).unwrap(), "desktop");
}

#[test]
fn a_failing_desktop_tool_still_prevents_cutting() {
    if !isolated(
        "a_failing_desktop_tool_still_prevents_cutting",
        true,
        "",
        false,
    ) {
        return;
    }
    let (_dir, mut app) = app();
    app.editor.select_all();
    app.cut();
    assert_eq!(app.editor.text(), "Café\nΩ");
    assert_eq!(app.clip, "Café\nΩ");
    assert_eq!(
        app.msg,
        "could not copy to the clipboard, so nothing was cut"
    );
    app.copy_selection();
    assert!(
        app.msg
            .contains("could not share it with the system clipboard")
    );
    assert_eq!(app.clipboard_text(), "Café\nΩ");
}
