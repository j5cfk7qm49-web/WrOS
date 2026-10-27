use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn begin() -> Conversation {
    let mut flow = Conversation::default();
    for c in "writer".chars() {
        flow.type_char(c);
    }
    assert!(matches!(flow.submit(), Some(Request::CreateSession { .. })));
    flow
}

#[test]
fn account_picker_excludes_system_and_nonlogin_accounts_and_cannot_replace_secrets() {
    let names = local_accounts(
        "root:x:0:0::/root:/bin/bash\nwriter:x:1000:1000::/home/writer:/bin/bash\nnobody:x:65534:65534::/:/bin/sh\nservice:x:1001:1001::/:/usr/sbin/nologin\nclosed:x:1002:1002::/:/bin/false\nbad name:x:1003:1003::/:/bin/sh\nwriter:x:1000:1000::/home/writer:/bin/bash",
    );
    assert_eq!(names, vec!["writer"]);
    let mut flow = Conversation::default();
    flow.choose_account(&names[0]);
    assert_eq!(&*flow.input, "writer");
    flow.submit().unwrap();
    prompt(&mut flow, MessageKind::Secret);
    flow.type_char('x');
    flow.choose_account("writer");
    assert_eq!(&*flow.input, "x");
}
fn prompt(flow: &mut Conversation, kind: MessageKind) {
    flow.receive(Response::AuthMessage {
        auth_message_type: kind,
        auth_message: "PAM question".into(),
    })
    .unwrap();
}

#[test]
fn arbitrary_pam_conversation_then_fixed_session() {
    let mut flow = begin();
    for kind in [
        MessageKind::Info,
        MessageKind::Secret,
        MessageKind::Error,
        MessageKind::Visible,
        MessageKind::Secret,
    ] {
        prompt(&mut flow, kind);
        flow.type_char('x');
        let request = flow.submit().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&encode(&request).unwrap()).unwrap();
        assert_eq!(
            json["response"],
            match kind {
                MessageKind::Info | MessageKind::Error => serde_json::Value::Null,
                _ => serde_json::Value::String("x".into()),
            }
        );
        assert!(flow.input.is_empty());
    }
    flow.receive(Response::Success).unwrap();
    assert_eq!(flow.phase, Phase::Authenticated);
    let json: serde_json::Value =
        serde_json::from_slice(&encode(&flow.submit().unwrap()).unwrap()).unwrap();
    assert_eq!(json["cmd"], serde_json::json!([SESSION]));
    assert_eq!(
        json["env"],
        serde_json::json!(["TERM=linux", "LANG=C.UTF-8"])
    );
    flow.receive(Response::Success).unwrap();
    assert_eq!(flow.phase, Phase::Done);
}

#[test]
fn denial_cancels_before_retry_and_never_displays_description() {
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret);
    flow.type_char('x');
    flow.submit().unwrap();
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
    assert!(flow.input.is_empty());
}

#[test]
fn escape_discards_secret_and_requires_cancel_ack() {
    let mut flow = begin();
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
    prompt(&mut flow, MessageKind::Secret);
    flow.submit().unwrap();
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
fn no_start_before_success_and_reject_unexpected_responses() {
    let mut flow = Conversation::default();
    assert!(flow.receive(Response::Success).is_err());
    let mut flow = begin();
    assert!(flow.submit().is_none());
    flow.receive(Response::Success).unwrap();
    flow.submit().unwrap();
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
    let mut flow = begin();
    prompt(&mut flow, MessageKind::Secret);
    for c in "fixture-secret-αβ".chars() {
        flow.type_char(c);
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (10, 3), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, &flow)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!screen.contains("fixture-secret"));
        assert!(!screen.contains('α'));
        if width >= 80 {
            assert!(screen.contains("[hidden]"));
        }
    }
}

#[test]
fn unicode_backspace_limit_and_control_filter() {
    let mut flow = begin();
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
