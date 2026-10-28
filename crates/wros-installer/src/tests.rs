use super::*;
use ratatui::{Terminal, backend::TestBackend};

/// Optional renderer-cell capture, using the retained Grimoire screenshot
/// format. Only synthetic states from these tests are eligible for export.
fn capture(terminal: &Terminal<TestBackend>, name: &str, theme: &str) {
    let Some(directory) = std::env::var_os("WROS_INSTALLER_FRAMES") else {
        return;
    };
    let buffer = terminal.backend().buffer();
    let color = |value| match value {
        ratatui::style::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        _ => None,
    };
    let rows = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| {
                    let cell = &buffer[(x, y)];
                    serde_json::json!([cell.symbol(), color(cell.fg), color(cell.bg), 0])
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let frame = serde_json::json!([{"name":name,"theme":theme,"w":buffer.area.width,"h":buffer.area.height,"rows":rows}]);
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec(&frame).unwrap(),
    )
    .unwrap();
}
fn send(session: &mut Session, line: &str) {
    assert!(matches!(session.command(line).unwrap(), Exchange::Reply(_)));
}
fn answer(session: &mut Session, tag: &str) -> Zeroizing<String> {
    let Exchange::Reply(value) = session.command(&format!("GET {tag}")).unwrap() else {
        panic!("missing answer");
    };
    value
}
fn string(session: &mut Session, kind: &str) {
    send(session, &format!("DATA fixture/entry type {kind}"));
    send(session, "DATA fixture/entry description Your account");
    send(
        session,
        "DATA fixture/entry extended_description A synthetic fixture, not a user account.",
    );
    send(session, "INPUT critical fixture/entry");
    assert!(matches!(session.command("GO").unwrap(), Exchange::Show));
}
#[test]
fn canonical_translated_choices_and_explicit_partition_consent() {
    let mut session = Session::default();
    for line in [
        "DATA partman/confirm type boolean",
        "DATA partman/confirm description Write changes?",
        "SET partman/confirm true",
        "INPUT critical partman/confirm",
    ] {
        send(&mut session, line);
    }
    assert!(matches!(session.command("GO").unwrap(), Exchange::Show));
    assert_eq!(session.active().unwrap().selected, 0);
    session.advance().unwrap();
    assert_eq!(&*answer(&mut session, "partman/confirm"), "0 false");
    for line in [
        "DATA fixture/layout type select",
        "DATA fixture/layout choices-c us, fr, uk",
        "DATA fixture/layout choices English, Français, British",
        "SET fixture/layout fr",
        "INPUT high fixture/layout",
    ] {
        send(&mut session, line);
    }
    session.command("GO").unwrap();
    assert_eq!(session.active().unwrap().selected, 1);
    session.advance().unwrap();
    assert_eq!(&*answer(&mut session, "fixture/layout"), "0 fr");
}
#[test]
fn password_never_enters_any_size_render_and_is_erased_after_one_get() {
    let mut session = Session::default();
    string(&mut session, "password");
    let secret = "sensitive".repeat(3) + "λ";
    for c in secret.chars() {
        session.active_mut().unwrap().type_char(c);
    }
    session.active_mut().unwrap().erase();
    session.active_mut().unwrap().type_char('λ');
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (1, 1), (20, 5)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                ui::render(
                    f,
                    &session,
                    &wros_shell::grimoire_theme::default_theme(),
                    0,
                    false,
                )
            })
            .unwrap();
        let shown = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(!shown.contains(&secret));
        assert!(!shown.contains("sensitive"));
        if width >= 80 {
            assert!(shown.contains("[hidden]"));
        }
        if width == 80 {
            capture(&terminal, "native-installer-password", "Grimoire");
        }
    }
    session.advance().unwrap();
    let value = answer(&mut session, "fixture/entry");
    assert_eq!(*value, format!("0 {secret}"));
    assert_eq!(&*answer(&mut session, "fixture/entry"), "0 ");
}
#[test]
fn back_is_only_offered_when_supported_and_clears_secret_answers() {
    let mut session = Session::default();
    string(&mut session, "password");
    session.active_mut().unwrap().type_char('x');
    assert!(session.back().is_none());
    assert!(session.showing);
    session.allow_back = true;
    assert_eq!(&*session.back().unwrap(), "30");
    assert!(session.command("GET fixture/entry").is_err());
    send(&mut session, "INPUT high fixture/entry");
    session.command("GO").unwrap();
    assert!(session.active().unwrap().answer.is_empty());
}
#[test]
fn multiple_fields_wait_for_the_final_enter_and_keep_canonical_commas() {
    let mut session = Session::default();
    for line in [
        "DATA fixture/options type multiselect",
        "DATA fixture/options choices-c one\\,two, three",
        "DATA fixture/options choices First, Second",
        "SET fixture/options three",
        "INPUT high fixture/options",
        "DATA fixture/name type string",
        "INPUT high fixture/name",
    ] {
        send(&mut session, line);
    }
    session.command("GO").unwrap();
    session.active_mut().unwrap().move_by(-1);
    session.active_mut().unwrap().toggle();
    assert!(session.advance().is_none());
    session.active_mut().unwrap().type_char('λ');
    assert!(session.back().is_none());
    assert_eq!(session.current, 0);
    assert!(session.advance().is_none());
    session.advance().unwrap();
    assert_eq!(
        &*answer(&mut session, "fixture/options"),
        "0 one\\,two, three"
    );
    assert_eq!(&*answer(&mut session, "fixture/name"), "0 λ");
}
#[test]
fn progress_and_long_explanations_share_the_grimoire_palette() {
    let mut session = Session::default();
    for line in [
        "DATA fixture/progress type text",
        "DATA fixture/progress description Installing WrOS",
        "DATA fixture/progress extended_description A long explanation\\nWith a second line",
        "PROGRESS START 0 10 fixture/progress",
        "PROGRESS STEP 3",
        "PROGRESS SET 7",
    ] {
        send(&mut session, line);
    }
    assert_eq!(session.progress.as_ref().unwrap().value, 7);
    assert!(session.progress.as_ref().unwrap().detail.contains('\n'));
    assert!(session.command("PROGRESS SET 11").is_err());
    for theme in wros_shell::grimoire_theme::presets() {
        for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| ui::render(f, &session, &theme, 0, false))
                .unwrap();
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .any(|c| c.fg == theme.accent)
            );
            if width == 80 && theme.name == "Grimoire" {
                capture(&terminal, "native-installer-progress", &theme.name);
            }
        }
    }
    send(&mut session, "PROGRESS STOP");
    assert!(session.progress.is_none());
}
#[test]
fn malformed_unknown_oversized_or_unapproved_input_cannot_advance() {
    for line in [
        "SET",
        "GO extra",
        "INPUT high unknown",
        "DATA q type executable",
        "GET q",
        "DATA q arbitrary field",
        "PROGRESS START 2 1 q",
        "INPUT impossible q",
        "DATA q type string\nGO",
    ] {
        assert!(Session::default().command(line).is_err());
    }
    assert!(
        Session::default()
            .command(&"x".repeat(MAX_LINE + 1))
            .is_err()
    );
    assert!(read_line(&mut std::io::Cursor::new(b"GET incomplete")).is_err());
    assert!(read_line(&mut std::io::Cursor::new(vec![b'x'; MAX_LINE + 1])).is_err());
    assert!(read_line(&mut std::io::Cursor::new(b"bad\0frame\n")).is_err());
    assert!(read_line(&mut std::io::Cursor::new(b"\xff\n")).is_err());
    assert!(read_line(&mut std::io::Cursor::new(b"")).unwrap().is_none());
}
#[test]
fn terminal_controls_are_never_replayed_as_backend_instructions() {
    let mut session = Session::default();
    send(&mut session, "TITLE fixture\x1b[2J\u{202e}reverse");
    assert!(!session.title.contains(['\x1b', '\u{202e}']));
    string(&mut session, "string");
    for c in "λ\x1b\u{202e}\n".chars() {
        session.active_mut().unwrap().type_char(c);
    }
    session.advance().unwrap();
    assert_eq!(&*answer(&mut session, "fixture/entry"), "0 λ");
}
