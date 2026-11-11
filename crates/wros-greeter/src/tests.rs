use super::*;
use form::{Action, Focus, Form};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

fn press(form: &mut Form, code: KeyCode) -> Action {
    form.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(form: &mut Form, text: &str) {
    for c in text.chars() {
        press(form, KeyCode::Char(c));
    }
}

fn json(request: &Request) -> serde_json::Value {
    serde_json::from_slice(&encode(request).unwrap()).unwrap()
}

/// The form filled in and submitted: the conversation asks greetd for a session.
fn begin_with(session: Session) -> Conversation {
    let mut flow = Conversation::default();
    let request = flow.begin("writer", "fixture-password", session).unwrap();
    assert_eq!(json(&request)["username"], "writer");
    flow
}

fn begin() -> Conversation {
    begin_with(Session::Usual)
}

fn prompt(flow: &mut Conversation, kind: MessageKind) -> Option<Request> {
    flow.receive(Response::AuthMessage {
        auth_message_type: kind,
        auth_message: "PAM question".into(),
    })
    .unwrap()
}

fn screen(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn the_form_password_answers_pam_once_and_the_session_starts_at_once() {
    let mut flow = begin();
    let answer = prompt(&mut flow, MessageKind::Secret).unwrap();
    assert_eq!(json(&answer)["response"], "fixture-password");
    assert!(flow.secret.is_empty());
    let start = flow.receive(Response::Success).unwrap().unwrap();
    let start = json(&start);
    assert_eq!(start["type"], "start_session");
    assert_eq!(start["cmd"], serde_json::json!([SESSION]));
    assert_eq!(
        start["env"],
        serde_json::json!(["TERM=linux", "LANG=C.UTF-8"])
    );
    flow.receive(Response::Success).unwrap();
    assert_eq!(flow.phase, Phase::Done);
}

#[test]
fn the_console_choice_asks_the_session_starter_for_the_console() {
    let mut flow = begin_with(Session::Console);
    prompt(&mut flow, MessageKind::Secret).unwrap();
    let start = flow.receive(Response::Success).unwrap().unwrap();
    assert_eq!(
        json(&start)["cmd"],
        serde_json::json!([SESSION, "--console"])
    );
}

#[test]
fn further_pam_questions_are_asked_on_screen_and_never_get_the_form_password() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    for kind in [
        MessageKind::Info,
        MessageKind::Secret,
        MessageKind::Error,
        MessageKind::Visible,
    ] {
        assert!(prompt(&mut flow, kind).is_none());
        assert_eq!(flow.phase, Phase::Prompt(kind));
        flow.type_char('x');
        let answer = json(&flow.submit().unwrap());
        assert_eq!(
            answer["response"],
            match kind {
                MessageKind::Info | MessageKind::Error => serde_json::Value::Null,
                _ => serde_json::Value::String("x".into()),
            }
        );
        assert!(flow.input.is_empty());
    }
    // A notice before the password question also means the form's password is dropped.
    let mut flow = begin();
    assert!(prompt(&mut flow, MessageKind::Info).is_none());
    assert!(flow.secret.is_empty());
}

#[test]
fn account_picker_excludes_system_and_nonlogin_accounts() {
    let names = local_accounts(
        "root:x:0:0::/root:/bin/bash\nwriter:x:1000:1000::/home/writer:/bin/bash\nnobody:x:65534:65534::/:/bin/sh\nservice:x:1001:1001::/:/usr/sbin/nologin\nclosed:x:1002:1002::/:/bin/false\nbad name:x:1003:1003::/:/bin/sh\nwriter:x:1000:1000::/home/writer:/bin/bash",
    );
    assert_eq!(names, vec!["writer"]);
    let mut form = Form::new(vec!["ann".into(), "writer".into()], false);
    press(&mut form, KeyCode::Down);
    assert_eq!(form.username.text(), "ann");
    press(&mut form, KeyCode::Up);
    assert_eq!(form.username.text(), "writer");
    // Up and Down in the password field never change the account or the password.
    press(&mut form, KeyCode::Enter);
    type_text(&mut form, "x");
    press(&mut form, KeyCode::Down);
    assert_eq!(
        (form.username.text(), form.password.text()),
        ("writer", "x")
    );
}

#[test]
fn enter_moves_through_the_form_and_signs_in_from_the_password() {
    let mut form = Form::new(vec![], true);
    assert_eq!(form.focus, Focus::Username);
    type_text(&mut form, "writer");
    assert_eq!(press(&mut form, KeyCode::Enter), Action::None);
    assert_eq!(form.focus, Focus::Password);
    type_text(&mut form, "pw");
    assert_eq!(press(&mut form, KeyCode::Enter), Action::Submit);
    // The switcher sits above the account field and picks the session.
    press(&mut form, KeyCode::BackTab);
    press(&mut form, KeyCode::BackTab);
    assert_eq!(form.focus, Focus::Switcher);
    assert_eq!(form.session(), Session::Usual);
    press(&mut form, KeyCode::Right);
    assert_eq!(form.session(), Session::Console);
    press(&mut form, KeyCode::Right);
    assert_eq!(form.session(), Session::Console);
    press(&mut form, KeyCode::Left);
    assert_eq!(form.session(), Session::Usual);
    // Without cage and foot there is nothing to switch.
    let mut form = Form::new(vec![], false);
    press(&mut form, KeyCode::BackTab);
    assert_eq!(
        (form.focus, form.session()),
        (Focus::Username, Session::Usual)
    );
}

#[test]
fn fields_edit_in_place_and_wipe_what_they_remove() {
    let mut form = Form::new(vec![], false);
    type_text(&mut form, "a🙂c");
    press(&mut form, KeyCode::Left);
    press(&mut form, KeyCode::Backspace);
    assert_eq!(form.username.text(), "ac");
    press(&mut form, KeyCode::Home);
    press(&mut form, KeyCode::Delete);
    type_text(&mut form, "b");
    assert_eq!(form.username.text(), "bc");
    press(&mut form, KeyCode::End);
    type_text(&mut form, "\n");
    form.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(form.username.is_empty());
    for _ in 0..MAX_INPUT {
        press(&mut form, KeyCode::Char('é'));
    }
    assert!(form.username.text().len() <= MAX_INPUT);
}

#[test]
fn denial_cancels_before_retry_and_never_displays_description() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    assert!(matches!(
        flow.receive(Response::Error {
            error_type: "auth_error".into(),
            description: "PRIVATE fixture".into()
        })
        .unwrap(),
        Some(Request::CancelSession)
    ));
    assert_eq!(flow.phase, Phase::Waiting(Pending::Cancel));
    assert!(flow.submit().is_none());
    flow.receive(Response::Success).unwrap();
    assert_eq!(flow.phase, Phase::Username);
    assert!(!flow.notice.contains("PRIVATE"));
    assert!(flow.notice.contains("not accepted"));
}

#[test]
fn escape_discards_secret_and_requires_cancel_ack() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    prompt(&mut flow, MessageKind::Secret);
    flow.type_char('x');
    assert!(matches!(flow.cancel(), Some(Request::CancelSession)));
    assert!(flow.input.is_empty());
    assert!(flow.submit().is_none());
    flow.receive(Response::Success).unwrap();
    assert_eq!(flow.phase, Phase::Username);
}

#[test]
fn failed_cancel_requires_fresh_service_instead_of_reusing_uncertain_state() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    flow.receive(Response::Error {
        error_type: "auth_error".into(),
        description: "denied".into(),
    })
    .unwrap();
    flow.receive(Response::Error {
        error_type: "error".into(),
        description: "failed worker".into(),
    })
    .unwrap();
    assert_eq!(flow.phase, Phase::Retry);
    assert!(flow.notice.contains("not accepted"));
    flow.type_char('x');
    assert!(flow.input.is_empty());
    assert!(flow.submit().is_none());
    assert_eq!(flow.phase, Phase::Restart);
}

#[test]
fn bad_account_names_and_unexpected_responses_are_refused() {
    let mut flow = Conversation::default();
    assert!(flow.receive(Response::Success).is_err());
    for name in ["", "bad name", "-x;sh", &"a".repeat(65)] {
        assert!(flow.begin(name, "pw", Session::Usual).is_none(), "{name}");
        assert!(flow.secret.is_empty());
    }
    let mut flow = begin();
    assert!(flow.begin("writer", "again", Session::Usual).is_none());
    prompt(&mut flow, MessageKind::Secret).unwrap();
    flow.receive(Response::Success).unwrap();
    assert!(
        flow.receive(Response::AuthMessage {
            auth_message_type: MessageKind::Secret,
            auth_message: "wrong phase".into()
        })
        .is_err()
    );
}

#[test]
fn secrets_never_render_at_any_supported_size() {
    let mut form = Form::new(vec![], true);
    type_text(&mut form, "writer");
    press(&mut form, KeyCode::Enter);
    type_text(&mut form, "fixture-secret-αβ");
    let mut flow = Conversation::default();
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (10, 3), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, &flow, &form)).unwrap();
        let shown = screen(&terminal);
        assert!(!shown.contains("fixture-secret"));
        assert!(!shown.contains('α'));
        if width >= 80 {
            assert!(shown.contains(form::HIDDEN) && shown.contains("Local account"));
            assert!(shown.contains("Usual session") && shown.contains("Console session"));
        }
    }
    // A PAM question typed on screen is hidden the same way.
    flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    prompt(&mut flow, MessageKind::Secret);
    for c in "second-secret".chars() {
        flow.type_char(c);
    }
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &flow, &form)).unwrap();
    let shown = screen(&terminal);
    assert!(!shown.contains("second-secret") && shown.contains("[hidden]"));
}

#[test]
fn the_password_mark_does_not_show_its_length() {
    let draw = |typed: &str| {
        let mut form = Form::new(vec![], false);
        type_text(&mut form, "writer");
        press(&mut form, KeyCode::Enter);
        type_text(&mut form, typed);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| render(frame, &Conversation::default(), &form))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    assert_eq!(draw("a"), draw("a much longer password"));
}

#[test]
fn the_login_keeps_the_start_screens_logo_above_the_whole_form() {
    let form = Form::new(vec![], true);
    for (width, height, border) in [
        (80, 25, "+--------+--------+"),
        (100, 37, "+----------------+----------------+"),
        (128, 48, "+----------------+----------------+"),
        (170, 48, "+----------------+----------------+"),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, &Conversation::default(), &form))
            .unwrap();
        let shown = screen(&terminal);
        let logo = shown.find(border).expect("the shared logo above the form");
        assert!(logo < shown.find("Local account").unwrap());
        assert!(shown.contains("Password") && shown.contains("Console session"));
    }
}

#[test]
fn the_large_login_clock_leaves_the_whole_password_form_visible() {
    let mut form = Form::new(vec![], true);
    type_text(&mut form, "writer");
    press(&mut form, KeyCode::Enter);
    type_text(&mut form, "disposable-test-secret");
    for (width, height) in [(80, 25), (100, 37), (128, 48), (170, 48)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                render_themed_with_clock(
                    frame,
                    &Conversation::default(),
                    &form,
                    &wros_shell::grimoire_theme::default_theme(),
                    wros_shell::clock::Time::new(23, 45),
                )
            })
            .unwrap();
        let shown = screen(&terminal);
        assert!(shown.contains("23:45"), "clock at {width} by {height}");
        assert!(shown.contains(['▀', '▄', '█']));
        assert!(
            shown.contains("Local account")
                && shown.contains("Password")
                && shown.contains("Console session")
        );
        assert!(shown.contains("Enter: sign in") && shown.contains("[hidden]"));
        assert!(!shown.contains("disposable-test-secret"));
        assert!(shown.find("+--------").unwrap() < shown.find("Local account").unwrap());
    }
}

#[test]
fn the_lock_message_labels_its_static_clock_and_contains_no_input_protocol() {
    let message = lock_message(wros_shell::clock::Time::new(0, 7));
    assert!(message.starts_with("\x1b[2J\x1b[3J\x1b[H"));
    assert!(message.contains("Locked at 00:07"));
    assert!(message.contains(['▀', '▄', '█']));
    assert!(
        !message.contains(['▀', '▄']),
        "the installed console font has full blocks only"
    );
    assert!(message.ends_with("Press Enter to unlock."));
    assert!(!message.contains("\x1b[6n") && !message.contains("\x1b[?"));
    assert!(message.len() < 8192);
    assert!(message.lines().count() < 25);
    let unavailable = lock_message(None);
    assert!(!unavailable.contains("Locked at"));
    assert!(unavailable.ends_with("Press Enter to unlock."));
}

#[test]
fn unicode_backspace_limit_and_control_filter() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret).unwrap();
    prompt(&mut flow, MessageKind::Secret);
    for c in "a🙂".chars() {
        flow.type_char(c);
    }
    flow.backspace();
    assert_eq!(&*flow.input, "a");
    for _ in 0..MAX_INPUT {
        flow.type_char('é');
    }
    assert!(flow.input.len() <= MAX_INPUT);
    flow.clear();
    flow.type_char('\n');
    assert!(flow.input.is_empty());
    assert!(!display_text("\x1b[2J\u{202e}hello").contains('\x1b'));
    assert!(!display_text("\u{202e}").contains('\u{202e}'));
}

#[test]
fn malformed_protocol_has_only_static_diagnostics() {
    assert_eq!(
        decode(b"secret-value").err(),
        Some("Invalid login service response")
    );
    assert!(
        decode(br#"{"type":"auth_message","auth_message_type":"unknown","auth_message":"x"}"#)
            .is_err()
    );
    assert!(decode(&vec![b' '; MAX_FRAME + 1]).is_err());
    assert!(
        encode(&Request::PostAuthMessageResponse {
            response: Some("x".repeat(MAX_FRAME + 1))
        })
        .is_err()
    );
}

#[test]
fn keyboard_recovery_opens_only_before_sign_in() {
    let mut conversation = Conversation::default();
    let mut keyboard = crate::keyboard::Keyboard::default();
    assert!(conversation.open_keyboard(&mut keyboard));
    keyboard.open = false;
    let mut conversation = begin();
    assert!(!conversation.open_keyboard(&mut keyboard) && !keyboard.open);
    prompt(&mut conversation, MessageKind::Secret).unwrap();
    assert!(!conversation.open_keyboard(&mut keyboard) && !keyboard.open);
    conversation.receive(Response::Success).unwrap();
    assert!(!conversation.open_keyboard(&mut keyboard) && !keyboard.open);
}

#[test]
fn every_login_hint_fits_an_80_column_console_and_offers_power() {
    let flow = Conversation::default();
    let mut form = Form::new(vec![], true);
    for (focus, key) in [
        (Focus::Username, KeyCode::BackTab),
        (Focus::Switcher, KeyCode::Tab),
        (Focus::Username, KeyCode::Tab),
        (Focus::Password, KeyCode::Tab),
    ] {
        assert_eq!(form.focus, focus);
        let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
        terminal.draw(|frame| render(frame, &flow, &form)).unwrap();
        let shown = screen(&terminal);
        assert!(
            shown.contains("F1: power  F2: keyboard  Ctrl+Alt+F2: maintenance"),
            "{focus:?}"
        );
        press(&mut form, key);
    }
}
